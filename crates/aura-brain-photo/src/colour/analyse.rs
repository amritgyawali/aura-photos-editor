//! One decoded frame in, one grade out.
//!
//! PHASE-16 section 3's data flow, as one function. The pixels are read **once** - phase 15's
//! `tone::stats::FrameStats` carries the histogram and the grid, and this module adds one
//! content pass and one skin sampling over the same buffer - and the learned head is called
//! at most never. Section 11 budgets 20 ms per image and that is only reachable because of
//! it.
//!
//! ## The order, and why it is this order
//!
//! 1. **Statistics.** The histogram, the percentiles and the subject's own spread.
//! 2. **Tone.** The five parameters, from the histogram and the scene's intent.
//! 3. **Curve.** Fitted to the subject's contrast target, under the noise and highlight
//!    constraints - and the highlight ceiling comes from step 4's dress reading, so the
//!    content pass runs before the fit even though the fit is listed first in section 3.
//! 4. **Content, harmony, HSL.** What is in the frame, what should change about it, and what
//!    the recipe should say.
//! 5. **The subtlety cap and the clipping guard.** Both reductions, both before the skin
//!    guard, so that the skin guard measures the grade that will actually be stored.
//! 6. **The skin guard.** Last, always, because it is a post-condition. ADR-0033 decision 1.
//! 7. **The variants**, each one through steps 5 and 6 of its own. A variant that has not
//!    been guarded is a variant that cannot be promoted safely.
//!
//! ## The placeholder, said once and plainly
//!
//! The shipped `tone_model` weights are **untrained**, for the reason every model since
//! phase 05 has been: there is no corpus of RAW files paired with expert edits in this
//! repository. [`crate::colour::tone::TONE_HEAD_TRAINED`] is false, so the head is never
//! run, and what ships is the deterministic solver. It is condition C1 of
//! `docs/progress/PHASE-16-EXIT.md`.

use std::sync::Arc;

use aura_core::contract::colour::{
    BandReading, ColourCode, ColourDecision, ColourReason, ColourVariant, ContentBand, HslBand,
    HslShift, ImageId, SkinGuardReport, ToneCurve, VariantKind,
};
use aura_core::contract::people::ImageSubjects;
use aura_core::{AttrFlags, AuraResult, Priority, SceneId};
use aura_infer::contract::infer::{InferRequest, InferService, ModelRef, TensorView, Version};
use aura_infer::onnx::fixtures::{TONE_INPUT_DIM, TONE_MODEL, TONE_OUTPUTS, TONE_SCENES};
use aura_raw::contract::pixels::{PixelBuffer, PixelLevel};
use aura_vision::embed::descriptors::{luma_plane, LumaPlane};

use crate::colour::clip_guard::{self, Clipping};
use crate::colour::codec::{self, Explained};
use crate::colour::content::{self, Reading};
use crate::colour::curve;
use crate::colour::harmony;
use crate::colour::hsl::{self, Solved as HslSolved};
use crate::colour::intent::{IntentTable, SceneIntent, UNTARGETED_CONFIDENCE_PENALTY};
use crate::colour::skin_guard::{self, Grade, SkinSamples};
use crate::colour::tone::{self, Measurements, ToneParams, TONE_HEAD_TRAINED};
use crate::tone::stats::{self, FrameStats};

/// Which build's arithmetic produced a decision.
///
/// Bumped on any change to a statistic, the solver, the curve fitter, the content
/// classifier, the harmony objective, a guard or the way the confidence is combined. It is
/// written into `image_colour_decision.analysis_ver`, and two decisions made under different
/// values of it are not comparable: `AURA-ML-5071` exists so that comparison never happens
/// silently.
pub const ANALYSIS_VER: u16 = 1;

/// The pixel rung the colour pass reads.
///
/// Tier 2, the 2048 px proxy, and it is a cache hit: phases 06, 09, 11 and 15 already read
/// it. What forces the proxy rather than a thumbnail is the **content classification** - a
/// hedge behind a couple is forty pixels of green on a thumbnail and the hue estimate over
/// forty pixels of an 8-bit JPEG is noise.
pub const COLOUR_LEVEL: PixelLevel = PixelLevel::Proxy2048;

/// Pinned version of the tone head.
pub const TONE_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// The version stamped on every `image_colour_decision.model_ver`.
pub const MODEL_VER: u16 =
    TONE_MODEL_VERSION.major * 100 + TONE_MODEL_VERSION.minor * 10 + TONE_MODEL_VERSION.patch;

/// The confidence a grade starts from, before its doubts are subtracted.
///
/// 0.82. Not higher, because every grade in this build rests on a content classification that
/// is an inference rather than a segmentation, and a phase that started at 0.95 would be
/// claiming a certainty it does not have on any frame.
pub const BASE_CONFIDENCE: f32 = 0.82;

/// How far below the frame's own brightest tone the curve's shoulder is held when a bright
/// near-neutral region is present.
///
/// Section 6.1's constraint (c), "highlight roll-off preserves the dress texture", as a
/// number. Eight percent of headroom below the dress's own luminance is enough to keep the
/// gradient across a fold; less and the fold flattens into one tone.
pub const DRESS_HEADROOM: f32 = 0.08;

/// Everything outside the pixels that one frame's grade depends on.
///
/// Assembled by the caller from the frozen services - `StoryService` for the scene,
/// `PeopleService` for the subjects, `IntegrityService`'s calibration for the noise headroom -
/// so this crate keeps taking no dependency on the crates that implement them.
#[derive(Debug, Clone)]
pub struct FrameContext {
    /// What phase 07 says the photograph is of.
    pub scene: SceneId,
    /// True when a scene service actually answered.
    pub scene_known: bool,
    /// Phase 07's fourteen attribute bits.
    pub attrs: AttrFlags,
    /// Who is in the frame, from phase 06.
    pub subjects: ImageSubjects,
    /// Phase 09's measured shadow headroom for this body at this ISO, in stops.
    ///
    /// Taken rather than recomputed. `IntegrityService` owns what a lift costs in noise, and
    /// a second answer to "how far can this body be pushed" is two grades that disagree.
    pub shadow_headroom_ev: f32,
}

/// What one frame's analysis produced.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameOutcome {
    /// The decision to store.
    pub decision: ColourDecision,
    /// The numbers each of the decision's reasons names, in the same order.
    pub reason_numbers: Vec<Vec<f32>>,
    /// What the content pass read, for the gate and the eval harness.
    pub reading: Reading,
}

/// The learned tone head.
#[derive(Debug)]
pub struct Head {
    infer: Arc<dyn InferService>,
    model: ModelRef,
}

impl Head {
    /// Wrap the one way to run a model.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            infer,
            model: ModelRef::new(TONE_MODEL, TONE_MODEL_VERSION),
        }
    }

    /// Load the head before a photographer is waiting on it.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.infer.warmup(&[self.model])
    }

    /// Which execution provider the hardware plan selected.
    #[must_use]
    pub fn ep(&self) -> &'static str {
        self.infer.plan().selected_ep().as_str()
    }

    /// The learned five parameters for one frame, in recipe units.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the tensor shape disagrees with the manifest.
    pub fn predict(&self, features: &[f32], prio: Priority) -> AuraResult<ToneParams> {
        if features.len() != TONE_INPUT_DIM {
            return Err(aura_core::errors::ml::shape_mismatch(
                &format!("{TONE_INPUT_DIM} tone features"),
                &format!("{}", features.len()),
            ));
        }
        let view = TensorView::new(vec![1, TONE_INPUT_DIM], features)?;
        let result = self
            .infer
            .run(InferRequest::new(self.model, vec![view], prio))?;
        let Some(tensor) = result.outputs.first() else {
            return Err(aura_core::errors::ml::shape_mismatch(
                "one tone tensor",
                "no outputs",
            ));
        };
        if tensor.data.len() < TONE_OUTPUTS {
            return Err(aura_core::errors::ml::shape_mismatch(
                &format!("{TONE_OUTPUTS} tone values"),
                &format!("{}", tensor.data.len()),
            ));
        }
        // Each sigmoid maps onto plus or minus the module's own bound for that parameter. A
        // bounded head is one that cannot propose a value the recipe would clamp, which a
        // linear output trained on a few thousand expert edits certainly would.
        let at = |index: usize, range: f32| {
            (tensor
                .data
                .get(index)
                .copied()
                .unwrap_or(0.5)
                .clamp(0.0, 1.0)
                * 2.0
                - 1.0)
                * range
        };
        Ok(ToneParams {
            contrast: at(0, tone::MAX_CONTRAST),
            highlights: at(1, tone::MAX_RECOVERY),
            shadows: at(2, tone::MAX_RECOVERY),
            whites: at(3, tone::MAX_ENDPOINT),
            blacks: at(4, tone::MAX_ENDPOINT),
        })
    }
}

/// The eleven measurements the tone head reads, plus the scene one-hot.
///
/// Assembled here rather than in the head so the feature order lives beside the code that
/// computes each one. Renumbering it relabels a trained model, exactly as phase 10's
/// `FaceExpression::channels` and phase 15's `exposure_features` do.
#[must_use]
pub fn tone_features(measured: &Measurements, scene: SceneId) -> Vec<f32> {
    let mut out = Vec::with_capacity(TONE_INPUT_DIM);
    out.push(measured.p01);
    out.push(measured.p05);
    out.push(measured.p50);
    out.push(measured.p95);
    out.push(measured.p99);
    out.push(measured.range());
    out.push(measured.subject_spread);
    out.push(measured.subject_luma);
    out.push(measured.clipped);
    out.push(measured.crushed);
    out.push((measured.shadow_headroom_ev / 4.0).clamp(0.0, 1.0));
    let index = scene.as_index();
    for slot in 0..TONE_SCENES {
        out.push(f32::from(u8::from(slot == index)));
    }
    out
}

/// One frame in, one grade out.
#[derive(Debug)]
pub struct Analyser {
    head: Head,
    intents: IntentTable,
}

impl Analyser {
    /// Assemble an analyser over the embedded intent table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5070` when the embedded table will not load.
    pub fn new(infer: Arc<dyn InferService>) -> Result<Self, aura_core::AuraError> {
        Ok(Self {
            head: Head::new(infer),
            intents: IntentTable::embedded()?,
        })
    }

    /// Use a different intent table.
    #[must_use]
    pub fn with_intents(mut self, intents: IntentTable) -> Self {
        self.intents = intents;
        self
    }

    /// The intent table underneath.
    #[must_use]
    pub const fn intents(&self) -> &IntentTable {
        &self.intents
    }

    /// The intent one scene is graded by. For the gate and the eval harness.
    #[must_use]
    pub fn intent_for(&self, scene: SceneId) -> SceneIntent {
        self.intents.get(scene)
    }

    /// Which execution provider the hardware plan selected.
    #[must_use]
    pub fn ep(&self) -> &'static str {
        self.head.ep()
    }

    /// Load the head.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.head.warmup()
    }

    /// The learned parameters, or `None` when there is no trained head.
    ///
    /// **Returns `None` in this build**, because [`TONE_HEAD_TRAINED`] is false. See the
    /// module header: a random projection of a histogram blended at any weight would be
    /// indistinguishable in the panel from a learned one.
    #[must_use]
    pub fn tone_hint(
        &self,
        measured: &Measurements,
        scene: SceneId,
        prio: Priority,
    ) -> Option<ToneParams> {
        if !TONE_HEAD_TRAINED {
            return None;
        }
        self.head
            .predict(&tone_features(measured, scene), prio)
            .ok()
    }

    /// Grade one frame.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5068` when the buffer is not 8-bit sRGB, which is the one shape this analyser
    /// cannot work around: every statistic in the phase is defined over an encoded sRGB proxy
    /// and reading a 16-bit linear buffer as one would produce a plausible, wrong answer
    /// rather than a failure.
    #[allow(clippy::too_many_lines)]
    pub fn analyse(
        &self,
        image: ImageId,
        buffer: &PixelBuffer,
        context: &FrameContext,
        prio: Priority,
    ) -> AuraResult<FrameOutcome> {
        let Some(rgb) = buffer.as_srgb8() else {
            return Err(crate::errors::colour_failed(
                &image.to_db(),
                "the proxy is not 8-bit sRGB and this phase's statistics are defined over one",
            ));
        };
        let width = buffer.width as usize;
        let height = buffer.height as usize;
        let plane = luma_plane(rgb, width, height);
        let table = stats::linear_table();
        let frame = FrameStats::measure(rgb, width, height, &plane);

        let intent = self.intents.get(context.scene);
        let scene = if context.scene_known {
            context.scene
        } else {
            SceneId::Unknown
        };

        // 1. What the histogram and the subject say.
        let (subject_luma, subject_spread, subject_is_face) =
            subject_stats(&plane, &context.subjects);
        let mut measured = Measurements::from_histogram(&frame.luma_histogram, frame.counted);
        measured.subject_luma = subject_luma;
        measured.subject_spread = subject_spread;
        measured.subject_is_face = subject_is_face;
        measured.clipped = frame.clipped;
        measured.crushed = frame.crushed;
        measured.shadow_headroom_ev = context.shadow_headroom_ev;
        // The head, if it were ever trained. Requested and discarded rather than skipped, so
        // the call site that will one day use it is visible in this build.
        let _hint = self.tone_hint(&measured, scene, prio);

        // 2. What is in the frame.
        let reading = content::read(rgb, width, height, &table, &context.subjects.faces);

        // 3. The five parameters.
        let solved_tone = tone::solve(&measured, &intent);

        // 4. The curve, under all three of section 6.1's constraints.
        let fitted = curve::fit(&curve::Constraints {
            subject_luma,
            measured_spread: subject_spread,
            target_spread: intent.subject_contrast,
            // Constraint (b): phase 09's headroom, in luminance units rather than in slider
            // units. A tenth of the range per stop is the same conversion `LIFT_PER_HEADROOM_EV`
            // makes, expressed for a curve node rather than for a slider.
            shadow_lift_allow: (context.shadow_headroom_ev * 0.06).clamp(0.0, 0.25),
            // Constraint (c): a dress in the frame lowers the shoulder.
            highlight_ceiling: highlight_ceiling(&reading),
        });

        // 5. The harmony objective and the eight bands.
        let plan = harmony::plan(&reading, &intent);
        let mean_saturation = reading
            .bands
            .iter()
            .filter(|band| !band.band.is_protected())
            .map(|band| band.saturation * band.area)
            .sum::<f32>()
            .clamp(0.0, 1.0);
        let colour = hsl::solve(&plan, &intent, mean_saturation);

        let proposed = Grade {
            tone: solved_tone.params,
            curve: fitted.curve.clone(),
            colour,
        };

        // 6. The subtlety cap and the clipping guard, then the skin guard.
        let skin = skin_guard::sample(rgb, width, height, &table, &context.subjects.faces);
        let (final_grade, clipping_before, clipping_after, guard_report, guard_reasons, resolved) =
            guard(&proposed, &frame, &intent, &skin);

        // 7. The alternatives, each guarded on its own.
        let alternatives = variants(&final_grade, &measured, &intent, &frame, &skin);

        // -- reasons ----------------------------------------------------
        let mut reasons: Vec<Explained> = Vec::new();
        reasons.extend(solved_tone.reasons);
        if fitted.curve.is_identity() {
            reasons.push(codec::plain(ColourCode::CurveIdentity, 0.0));
        } else {
            reasons.push(codec::numbered(
                ColourCode::CurveFitted,
                format!(
                    "a tone curve was drawn to give the subject the {:.0}% separation this kind \
                     of photograph wants",
                    intent.subject_contrast * 100.0
                ),
                0.0,
                vec![fitted.achieved_spread, intent.subject_contrast, fitted.gain],
            ));
        }
        reasons.extend(final_grade.colour.reasons.clone());
        reasons.extend(resolved);
        reasons.extend(guard_reasons);
        if !intent.measured {
            reasons.push(codec::plain(
                ColourCode::NoIntentRow,
                -UNTARGETED_CONFIDENCE_PENALTY,
            ));
        }
        // Invariant 2 and migration 16's `reason_count` CHECK. Nothing above can produce an
        // empty list and the fallback is here anyway, because the schema's refusal is a failed
        // write in the middle of a 4,000-image pass and this is a bug caught in a test.
        if reasons.is_empty() {
            reasons.push(codec::plain(ColourCode::ToneAlreadyRight, 0.0));
        }
        let (reasons, reason_numbers) = rank_reasons(reasons);
        let confidence = confidence_of(&reasons, &reading);

        let decision = ColourDecision {
            image_id: image,
            contrast: final_grade.tone.contrast,
            highlights: final_grade.tone.highlights,
            shadows: final_grade.tone.shadows,
            whites: final_grade.tone.whites,
            blacks: final_grade.tone.blacks,
            curve: final_grade.curve.clone(),
            hsl: final_grade.colour.hsl.clamped(),
            vibrance: final_grade.colour.vibrance,
            saturation: final_grade.colour.saturation,
            skin_guard: guard_report,
            clipping_after: (clipping_after.highlight, clipping_after.shadow),
            clipping_before: (clipping_before.highlight, clipping_before.shadow),
            bands: reading.bands.clone(),
            alternatives,
            reasons,
            confidence,
            subtlety: clip_guard::subtlety(&final_grade),
            scene,
            skin_measured: guard_report.measured,
            user_edited: false,
            reviewed: false,
            model_ver: MODEL_VER,
            analysis_ver: ANALYSIS_VER,
            intent_ver: self.intents.version(),
        };

        Ok(FrameOutcome {
            decision,
            reason_numbers,
            reading,
        })
    }
}

/// Run the three guards over one candidate grade.
///
/// Returns the grade that survived, the clipping before and after, the skin report, the skin
/// guard's reasons and the clipping guard's reasons. The order is fixed and documented in the
/// module header: reductions first, the post-condition last.
fn guard(
    proposed: &Grade,
    frame: &FrameStats,
    intent: &SceneIntent,
    skin: &SkinSamples,
) -> (
    Grade,
    Clipping,
    Clipping,
    SkinGuardReport,
    Vec<Explained>,
    Vec<Explained>,
) {
    let bounded = clip_guard::enforce(
        proposed,
        &frame.luma_histogram,
        frame.counted,
        crate::colour::intent::clip_tolerance(intent),
        intent.subtlety_cap,
    );
    let guarded = skin_guard::enforce(&bounded.grade, skin);
    (
        guarded.grade,
        bounded.before,
        bounded.after,
        guarded.report,
        guarded.reasons,
        bounded.reasons,
    )
}

/// Build the three alternatives, each one guarded on its own.
///
/// A variant that survives its guards as something indistinguishable from the primary is
/// dropped: an alternatives switcher with three buttons that all do the same thing is worse
/// than one with none.
fn variants(
    primary: &Grade,
    measured: &Measurements,
    intent: &SceneIntent,
    frame: &FrameStats,
    skin: &SkinSamples,
) -> Vec<ColourVariant> {
    let mut out = Vec::with_capacity(VariantKind::COUNT);
    for kind in VariantKind::ALL {
        let candidate = propose_variant(kind, primary, measured, intent);
        let (grade, _, after, report, _, _) = guard(&candidate, frame, intent, skin);
        if indistinguishable(&grade, primary) {
            continue;
        }
        out.push(ColourVariant {
            kind,
            contrast: grade.tone.contrast,
            highlights: grade.tone.highlights,
            shadows: grade.tone.shadows,
            whites: grade.tone.whites,
            blacks: grade.tone.blacks,
            vibrance: grade.colour.vibrance,
            saturation: grade.colour.saturation,
            curve: grade.curve.clone(),
            hsl: grade.colour.hsl.clamped(),
            clipping_after: (after.highlight, after.shadow),
            skin_guard: report,
        });
    }
    out
}

/// One direction, before it is guarded.
fn propose_variant(
    kind: VariantKind,
    primary: &Grade,
    measured: &Measurements,
    intent: &SceneIntent,
) -> Grade {
    let mut out = primary.clone();
    match kind {
        VariantKind::Flatter => {
            out.tone.contrast = primary.tone.contrast * 0.45;
            out.tone.blacks = primary.tone.blacks * 0.4 + 6.0;
            out.tone.shadows = (primary.tone.shadows + 8.0).min(tone::MAX_RECOVERY);
            out.curve = curve::fit(&curve::Constraints {
                subject_luma: measured.subject_luma,
                measured_spread: measured.subject_spread,
                target_spread: intent.subject_contrast * 0.8,
                shadow_lift_allow: (measured.shadow_headroom_ev * 0.06).clamp(0.0, 0.25),
                highlight_ceiling: 1.0,
            })
            .curve;
        }
        VariantKind::Punchier => {
            out.tone.contrast = (primary.tone.contrast * 1.4 + 6.0).min(tone::MAX_CONTRAST);
            out.tone.blacks = (primary.tone.blacks - 8.0).max(-tone::MAX_ENDPOINT);
            out.curve = curve::fit(&curve::Constraints {
                subject_luma: measured.subject_luma,
                measured_spread: measured.subject_spread,
                target_spread: intent.subject_contrast * 1.25,
                shadow_lift_allow: (measured.shadow_headroom_ev * 0.06).clamp(0.0, 0.25),
                highlight_ceiling: 1.0,
            })
            .curve;
        }
        VariantKind::Warmer => {
            // **Not a white-balance change.** Phase 15 owns the temperature and nothing here
            // can reach it; what "warmer" means in this phase is a small lift of the warm
            // bands, which is what a photographer does with an HSL panel rather than with the
            // temperature slider. The skin guard measures it like any other colour move.
            out.colour.vibrance = (primary.colour.vibrance + 5.0).clamp(-100.0, 100.0);
            for band in [HslBand::Red, HslBand::Orange, HslBand::Yellow] {
                let existing = out.colour.hsl.get(band);
                out.colour.hsl.set(
                    band,
                    HslShift {
                        h: existing.h,
                        s: (existing.s + 4.0).clamp(-intent.max_band_shift, intent.max_band_shift),
                        l: existing.l,
                    },
                );
            }
        }
    }
    out.tone = out.tone.clamped();
    out
}

/// True when two grades would look the same.
fn indistinguishable(a: &Grade, b: &Grade) -> bool {
    (a.tone.contrast - b.tone.contrast).abs() < 2.0
        && (a.tone.shadows - b.tone.shadows).abs() < 2.0
        && (a.tone.blacks - b.tone.blacks).abs() < 2.0
        && (a.colour.vibrance - b.colour.vibrance).abs() < 2.0
        && a.curve == b.curve
}

/// The curve's highlight ceiling for this frame.
///
/// Section 6.1's constraint (c). A bright near-neutral region - a dress, a cake or a
/// tablecloth - lowers the shoulder so the gradient across a fold survives; a frame with none
/// has no ceiling at all.
#[must_use]
pub fn highlight_ceiling(reading: &Reading) -> f32 {
    reading
        .band(ContentBand::Dress)
        .map_or(1.0, |dress| (dress.luma - DRESS_HEADROOM).clamp(0.55, 1.0))
}

/// The subject's median luminance, its interquartile spread, and whether a face defined it.
///
/// A face when there is one, the centre half of the frame when there is not. The fallback is
/// stated rather than silent: `Measurements::subject_is_face` reaches the reason's sentence,
/// so a photographer reading "this frame separated by 18 %" knows it is about the frame and
/// not about anybody in it.
#[must_use]
pub fn subject_stats(plane: &LumaPlane, subjects: &ImageSubjects) -> (f32, f32, bool) {
    let mut samples: Vec<f32> = Vec::new();
    for face in &subjects.faces {
        let area = face.bbox.clamped();
        let x0 = (area.x * plane.width as f32) as usize;
        let y0 = (area.y * plane.height as f32) as usize;
        let x1 = (((area.x + area.w) * plane.width as f32) as usize).min(plane.width);
        let y1 = (((area.y + area.h) * plane.height as f32) as usize).min(plane.height);
        for y in (y0..y1).step_by(2) {
            for x in (x0..x1).step_by(2) {
                if let Some(value) = plane.values.get(y * plane.width + x) {
                    samples.push(*value);
                }
            }
        }
    }
    let is_face = !samples.is_empty();
    if !is_face {
        // The centre half, which is where a subject is when nobody found one.
        let x0 = plane.width / 4;
        let x1 = plane.width * 3 / 4;
        let y0 = plane.height / 4;
        let y1 = plane.height * 3 / 4;
        for y in (y0..y1).step_by(2) {
            for x in (x0..x1).step_by(2) {
                if let Some(value) = plane.values.get(y * plane.width + x) {
                    samples.push(*value);
                }
            }
        }
    }
    if samples.is_empty() {
        return (0.5, 0.25, false);
    }
    samples.sort_by(f32::total_cmp);
    let at = |fraction: f32| -> f32 {
        let index = ((samples.len() as f32 - 1.0) * fraction).round() as usize;
        samples.get(index).copied().unwrap_or(0.5)
    };
    let median = at(0.5);
    // Interquartile rather than a standard deviation: a face with a specular highlight on the
    // nose has a long tail, and a standard deviation reads it as contrast that is not there.
    let spread = (at(0.75) - at(0.25)).clamp(0.0, 1.0);
    (median, spread, is_face)
}

/// Keep the six most informative reasons, strongest doubt first.
///
/// Doubts before everything else, phase 15's ordering: a photographer opening a panel wants
/// to know what went wrong before being told what went right.
///
/// **Then by informativeness, not alphabetically.** Six is the cap and a wedding frame can
/// easily produce nine reasons, so something has to be dropped - and a tie broken on the
/// reason's *slug* drops `greenery_tamed` because `g` sorts after `d`, which is not a product
/// decision anybody made. [`informativeness`] is the ordering instead, and it says what a
/// photographer would ask about first: what happened to the people, then what happened to the
/// colours, then which slider moved.
fn rank_reasons(mut reasons: Vec<Explained>) -> (Vec<ColourReason>, Vec<Vec<f32>>) {
    reasons.sort_by(|a, b| {
        a.0.weight
            .total_cmp(&b.0.weight)
            .then_with(|| informativeness(a.0.code).cmp(&informativeness(b.0.code)))
            .then_with(|| a.0.code.as_str().cmp(b.0.code.as_str()))
    });
    reasons.dedup_by(|a, b| a.0.code == b.0.code);
    reasons.truncate(ColourDecision::MAX_REASONS);
    reasons.into_iter().unzip()
}

/// How interesting one code is when two reasons cost the same confidence. Lower is kept.
///
/// Four tiers, and the argument for the order is the question a photographer asks in each
/// case:
///
/// 1. **What happened to the people.** The skin guarantee is the phase's headline claim and
///    the one thing a photographer checks first.
/// 2. **What happened to a named thing in the frame.** "You changed the greenery" is a
///    sentence about the photograph; a slider value is not.
/// 3. **What the curve did.** The phase's main work, and the thing the Curve panel draws.
/// 4. **Which slider moved.** True, useful, and the first thing to drop when six is the cap -
///    the panel shows every slider's value anyway.
const fn informativeness(code: ColourCode) -> u8 {
    match code {
        ColourCode::SkinGuardWithdrew
        | ColourCode::SkinGuardResolved
        | ColourCode::SkinProtected
        | ColourCode::NoSkinInFrame => 0,
        ColourCode::GreeneryTamed
        | ColourCode::DistractorTamed
        | ColourCode::HueConflictFound
        | ColourCode::DressProtected
        | ColourCode::SkyDeepened
        | ColourCode::WoodWarmed
        | ColourCode::BandUnconfident
        | ColourCode::ContentInferred
        | ColourCode::HslNeutral => 1,
        ColourCode::CurveFitted
        | ColourCode::CurveFlattenedForClipping
        | ColourCode::CurveIdentity
        | ColourCode::ClippingGuardResolved
        | ColourCode::SubtletyCapped
        | ColourCode::ShadowLiftHeldForNoise
        | ColourCode::NoIntentRow
        | ColourCode::UserOverride => 2,
        _ => 3,
    }
}

/// How sure this grade is.
///
/// The base minus every doubt the reasons carry, then multiplied by how much of the frame the
/// content pass could classify at all. The second term is the honest one: a frame whose
/// colours said nothing has been graded on its histogram alone, and the confidence should say
/// so even when no individual reason complained.
fn confidence_of(reasons: &[ColourReason], reading: &Reading) -> f32 {
    let doubts: f32 = reasons
        .iter()
        .filter(|reason| reason.is_doubt())
        .map(|reason| reason.weight)
        .sum();
    let classified = (0.75 + reading.classified * 0.25).clamp(0.0, 1.0);
    ((BASE_CONFIDENCE + doubts) * classified).clamp(0.05, 0.98)
}

/// One band reading, for the panel and the gate.
#[must_use]
pub fn band_of(decision: &ColourDecision, band: ContentBand) -> Option<BandReading> {
    decision.band(band).copied()
}

/// The curve a decision would hand the renderer, for the gate.
#[must_use]
pub fn curve_of(decision: &ColourDecision) -> ToneCurve {
    decision.curve.clone()
}

/// What the grade would do to one linear sRGB triple, through the real renderer.
///
/// For the eval harness and the gate. It calls `skin_guard`'s replay, so there is one answer
/// to "what does this grade do to a pixel" rather than two.
#[must_use]
pub fn apply_to(decision: &ColourDecision, rgb: [f32; 3]) -> [f32; 3] {
    let grade = Grade {
        tone: ToneParams {
            contrast: decision.contrast,
            highlights: decision.highlights,
            shadows: decision.shadows,
            whites: decision.whites,
            blacks: decision.blacks,
        },
        curve: decision.curve.clone(),
        colour: HslSolved {
            hsl: decision.hsl,
            vibrance: decision.vibrance,
            saturation: decision.saturation,
            reasons: Vec::new(),
        },
    };
    skin_guard::apply(&grade, rgb)
}
