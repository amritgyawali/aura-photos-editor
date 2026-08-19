//! One decoded frame in, one exposure and one white balance out.
//!
//! PHASE-15 section 3's data flow, as one function. The pixels are read **once** -
//! [`crate::tone::stats::FrameStats`] carries every statistic the rest of the phase needs -
//! and the two learned heads are called at most once each. Section 11 budgets 25 ms per
//! image and that is only reachable because of it.
//!
//! ## The order, and why it is this order
//!
//! 1. **Statistics.** One pass over the proxy: the linear means, the histogram, the grid,
//!    the thumbnail the white-balance head reads.
//! 2. **Neutrals.** A second, blocked pass. Separate from the first because it needs
//!    per-block variance, which is not a running statistic.
//! 3. **Skin patches.** One per face, from the same decoded buffer.
//! 4. **Hypotheses, then the solve.** Colour before exposure, and that is deliberate: the
//!    skin patches' *reflectance* depends on which light was chosen, so a locus sample
//!    cannot be produced until the white balance is.
//! 5. **Exposure.** Last, because its clipping bound reads the histogram and its subject
//!    anchor reads the luminance plane, neither of which the colour solve touches.
//!
//! ## The two placeholders, said once and plainly
//!
//! The shipped `white_balance` and `exposure_scene` weights are **untrained**, for the
//! reason every model since phase 05 has been: there is no corpus of RAW files paired with
//! expert edits in this repository, and section 8's step 1 - "assemble the training set" -
//! is a DATA task that needs a wedding archive nobody here has.
//!
//! The consequence is handled structurally rather than promised:
//!
//! * [`WB_HEAD_TRAINED`] is false, so [`Analyser::illuminant_hint`] returns `None` and the
//!   learned hypothesis is **never generated**. The solve runs on grey-world, white-patch,
//!   known neutrals, skin and the camera's own value - five real estimators - and the
//!   panel does not claim a learned prediction it did not make.
//! * [`EXPOSURE_HEAD_TRAINED`] is false, so a faceless frame carries
//!   `ToneCode::ExposureUnavailable` and an exposure of zero rather than a random number of
//!   stops. A frame nobody decided anything about is visibly that, which is the difference
//!   between a gap and a silent error.
//!
//! Both are condition C1 of `docs/progress/PHASE-15-EXIT.md`.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_core::contract::people::ImageSubjects;
use aura_core::contract::tone::{
    Illuminant, ImageId, SkinLocus, ToneAlternative, ToneCode, ToneEstimate,
};
use aura_core::{AttrFlags, AuraResult, IdentityId, Priority, SceneId};
use aura_infer::contract::infer::{InferRequest, InferService, ModelRef, TensorView, Version};
use aura_infer::onnx::fixtures::{
    EXPOSURE_FEATURES, EXPOSURE_INPUT_DIM, EXPOSURE_SCENES, EXPOSURE_SCENE_MODEL, WB_INPUT_SIDE,
    WB_OUTPUTS, WHITE_BALANCE_MODEL,
};
use aura_raw::colour::illuminant::{linear_srgb_to_uv, uv_distance};
use aura_raw::contract::pixels::{PixelBuffer, PixelLevel};
use aura_vision::embed::descriptors::{luma_plane, LumaPlane};

use crate::tone::exposure::{self, Bounds, MAX_MOVE_EV};
use crate::tone::illuminant::{self as lights, AsShot, MixedLight};
use crate::tone::neutrals;
use crate::tone::skin_locus::{self, Sample};
use crate::tone::solve;
use crate::tone::stats::{self, FrameStats, SkinPatch};
use crate::tone::store::codec::{self, Explained};
use crate::tone::targets::{SceneTarget, TargetTable};
use crate::tone::wb::{Constraint, Constraints};

/// Which build's arithmetic produced an estimate.
///
/// Bumped on any change to a statistic, a hypothesis generator, a scoring weight, a bound or
/// the way the confidence is combined. It is written into
/// `image_tone_estimate.analysis_ver`, and two estimates made under different values of it
/// are not comparable: `AURA-ML-5060` exists so that comparison never happens silently.
///
/// 1 -> 2: the preserve-mood policy stopped keying on whichever hypothesis won the cost race
/// and started asking the frame's own reading of the room (`illuminant::ambient`), and the
/// correction between two lights moved from an interpolation in kelvin to one in `u'v'`.
/// Both change the temperature and tint written on a coloured-light frame, so every stored
/// estimate is re-measured. Condition C5 in `docs/progress/PHASE-15-EXIT.md`.
pub const ANALYSIS_VER: u16 = 2;

/// The pixel rung the tone pass reads.
///
/// Tier 2, the 2048 px proxy, and for once the choice is not about detail. An illuminant
/// estimate, a grey-world mean and a face's mean luminance are all *averages*, and every one
/// of them is identical at 512 px. What forces the proxy is the **skin patch**: the middle
/// half of a face box in a group photograph is forty pixels across on a proxy and ten on a
/// thumbnail, and a mean chromaticity over ten pixels of an 8-bit JPEG is noise.
///
/// It is also the rung phases 06, 09 and 11 already read, so the cost is a cache hit rather
/// than a decode - which is what makes running the four passes back to back nearly free and
/// is why section 11's budget is 25 ms rather than 250.
pub const TONE_LEVEL: PixelLevel = PixelLevel::Proxy2048;

/// Pinned version of the white-balance head.
pub const WB_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// Pinned version of the faceless exposure head.
pub const EXPOSURE_MODEL_VERSION: Version = Version::new(1, 0, 0);

/// The version stamped on every `image_tone_estimate.model_ver`.
///
/// The two heads ship as one pack and move together, so one number invalidates both - phase
/// 09's `focus::MODEL_VER` and phase 11's `keypoints::MODEL_VER` make the same choice for the
/// same reason: no consumer of a tone estimate cares *which* head moved, only that the
/// numbers are not comparable across the move.
pub const MODEL_VER: u16 =
    WB_MODEL_VERSION.major * 100 + WB_MODEL_VERSION.minor * 10 + WB_MODEL_VERSION.patch;

/// Whether the pinned white-balance weights have passed the section 10.1 gate.
///
/// A compile-time release assertion rather than an inference-success check, exactly as
/// phase 11's `KEYPOINT_HEAD_TRAINED` is. The registered fixture is a valid ONNX graph and
/// executes successfully; executing successfully does not make its output an estimate of
/// anything. While this is false the learned hypothesis is never generated at all.
pub const WB_HEAD_TRAINED: bool = false;

/// Whether the pinned faceless-exposure weights have passed the section 10.1 gate.
pub const EXPOSURE_HEAD_TRAINED: bool = false;

/// The `u'` range the white-balance head's sigmoid is mapped onto.
///
/// Every illuminant a wedding can contain - a 1800 K candle to a 20000 K blue-hour sky, on
/// or off the locus - sits inside this. A bounded output is a head that cannot propose a
/// chromaticity outside the diagram, which a linear output trained on a small dataset
/// certainly would.
pub const WB_U_RANGE: (f32, f32) = (0.14, 0.34);

/// The `v'` range the white-balance head's sigmoid is mapped onto.
pub const WB_V_RANGE: (f32, f32) = (0.42, 0.55);

/// Everything outside the pixels that one frame's estimate depends on.
///
/// Assembled by the caller from the frozen services - `StoryService` for the scene and its
/// attributes, `PeopleService` for the subjects - so that this crate keeps taking no
/// dependency on the crates that implement them.
#[derive(Debug, Clone)]
pub struct FrameContext {
    /// What phase 07 says the photograph is of.
    pub scene: SceneId,
    /// True when a scene service actually answered.
    pub scene_known: bool,
    /// Phase 07's fourteen attribute bits. Read for `STAGE`, `TUNGSTEN`, `FLASH` and
    /// `BACKLIT`, which are the four this phase acts on.
    pub attrs: AttrFlags,
    /// Who is in the frame, from phase 06.
    pub subjects: ImageSubjects,
    /// What the camera itself decided.
    pub as_shot: AsShot,
    /// Phase 09's measured shadow headroom for this body at this ISO, in stops.
    ///
    /// Taken rather than recomputed. `IntegrityService` owns what a lift costs in noise, and
    /// a second answer to "how far can this body be pushed" is two exposure decisions that
    /// disagree.
    pub shadow_headroom_ev: f32,
}

impl FrameContext {
    /// True when phase 07 marked the frame backlit.
    #[must_use]
    pub fn scene_says_backlit(&self) -> bool {
        self.attrs.contains(AttrFlags::BACKLIT)
    }
}

/// What one frame's analysis produced.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameOutcome {
    /// The estimate to store.
    pub estimate: ToneEstimate,
    /// Skin samples this frame contributed, by identity.
    ///
    /// Returned rather than accumulated internally, because the analyser is `Sync` and
    /// borrowed by four call sites while the accumulation belongs to one pass over one
    /// project. A locus builder inside the analyser would be a mutable global with a
    /// friendlier name.
    pub samples: Vec<(IdentityId, Sample)>,
    /// True when a face anchored the exposure and the subject landed in the scene's band.
    ///
    /// The reference-frame gate, computed here because it needs the band and the solved
    /// exposure together and neither is stored.
    pub subject_in_band: bool,
    /// The numbers each of the estimate's reasons names, in the same order.
    ///
    /// Parallel to `estimate.reasons` rather than carried on `ToneReason`, because the frozen
    /// contract has no field for them and adding one would be a contract change made for a
    /// storage optimisation. `tone::store::codec` is the only consumer.
    pub reason_numbers: Vec<Vec<f32>>,
}

/// The two learned heads.
#[derive(Debug)]
pub struct Heads {
    infer: Arc<dyn InferService>,
    wb: ModelRef,
    exposure: ModelRef,
}

impl Heads {
    /// Wrap the one way to run a model.
    #[must_use]
    pub fn new(infer: Arc<dyn InferService>) -> Self {
        Self {
            infer,
            wb: ModelRef::new(WHITE_BALANCE_MODEL, WB_MODEL_VERSION),
            exposure: ModelRef::new(EXPOSURE_SCENE_MODEL, EXPOSURE_MODEL_VERSION),
        }
    }

    /// Load both heads before a photographer is waiting on them.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.infer.warmup(&[self.wb, self.exposure])
    }

    /// Which execution provider the hardware plan selected.
    #[must_use]
    pub fn ep(&self) -> &'static str {
        self.infer.plan().selected_ep().as_str()
    }

    /// The learned illuminant chromaticity for one frame.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the tensor shape disagrees with the manifest.
    pub fn illuminant(&self, thumbnail: &[f32], prio: Priority) -> AuraResult<[f32; 2]> {
        let expected = 3 * WB_INPUT_SIDE * WB_INPUT_SIDE;
        if thumbnail.len() != expected {
            return Err(aura_core::errors::ml::shape_mismatch(
                &format!("{expected} thumbnail samples"),
                &format!("{}", thumbnail.len()),
            ));
        }
        let view = TensorView::new(vec![1, 3, WB_INPUT_SIDE, WB_INPUT_SIDE], thumbnail)?;
        let result = self
            .infer
            .run(InferRequest::new(self.wb, vec![view], prio))?;
        let Some(tensor) = result.outputs.first() else {
            return Err(aura_core::errors::ml::shape_mismatch(
                "one chromaticity tensor",
                "no outputs",
            ));
        };
        if tensor.data.len() < WB_OUTPUTS {
            return Err(aura_core::errors::ml::shape_mismatch(
                &format!("{WB_OUTPUTS} chromaticity values"),
                &format!("{}", tensor.data.len()),
            ));
        }
        let u = tensor.data.first().copied().unwrap_or(0.5);
        let v = tensor.data.get(1).copied().unwrap_or(0.5);
        Ok([
            WB_U_RANGE.0 + u.clamp(0.0, 1.0) * (WB_U_RANGE.1 - WB_U_RANGE.0),
            WB_V_RANGE.0 + v.clamp(0.0, 1.0) * (WB_V_RANGE.1 - WB_V_RANGE.0),
        ])
    }

    /// The learned exposure for a frame with no faces in it, in stops.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5007` when the tensor shape disagrees with the manifest.
    pub fn scene_exposure(&self, features: &[f32], prio: Priority) -> AuraResult<f32> {
        if features.len() != EXPOSURE_INPUT_DIM {
            return Err(aura_core::errors::ml::shape_mismatch(
                &format!("{EXPOSURE_INPUT_DIM} exposure features"),
                &format!("{}", features.len()),
            ));
        }
        let view = TensorView::new(vec![1, EXPOSURE_INPUT_DIM], features)?;
        let result = self
            .infer
            .run(InferRequest::new(self.exposure, vec![view], prio))?;
        let Some(tensor) = result.outputs.first() else {
            return Err(aura_core::errors::ml::shape_mismatch(
                "one exposure tensor",
                "no outputs",
            ));
        };
        let unit = tensor.data.first().copied().unwrap_or(0.5).clamp(0.0, 1.0);
        // The sigmoid's `0..1` maps onto plus or minus the largest move this phase makes.
        Ok((unit * 2.0 - 1.0) * MAX_MOVE_EV)
    }
}

/// The twelve measurements the faceless exposure head reads, plus the scene one-hot.
///
/// Assembled here rather than in the head, so that the feature order lives beside the code
/// that computes each one. Renumbering it relabels a trained model, exactly as phase 10's
/// `FaceExpression::channels` and phase 11's `Feature` do.
#[must_use]
pub fn exposure_features(stats: &FrameStats, plane: &LumaPlane, scene: SceneId) -> Vec<f32> {
    let mut out = Vec::with_capacity(EXPOSURE_INPUT_DIM);
    let percentile = |fraction: f32| -> f32 {
        let mut histogram = [0u32; 256];
        let mut count = 0u32;
        for value in &plane.values {
            let bin = ((value.clamp(0.0, 1.0) * 255.0) as usize).min(255);
            if let Some(slot) = histogram.get_mut(bin) {
                *slot += 1;
            }
            count += 1;
        }
        if count == 0 {
            return 0.0;
        }
        let target = (count as f32 * fraction.clamp(0.0, 1.0)) as u32;
        let mut seen = 0u32;
        for (bin, hits) in histogram.iter().enumerate() {
            seen += *hits;
            if seen > target {
                return bin as f32 / 255.0;
            }
        }
        1.0
    };

    out.push(stats.luma_median);
    out.push(exposure::linear_to_perceptual(stats.linear_luma));
    out.push(percentile(0.05));
    out.push(percentile(0.25));
    out.push(percentile(0.75));
    out.push(percentile(0.95));
    out.push(stats.centre_luma);
    out.push(stats.border_luma);
    out.push(stats.clipped);
    out.push(stats.crushed);
    out.push(stats.cast);
    // The centre-to-border ratio, bounded. The backlit signal, handed to the head directly
    // rather than left for it to discover from the two means it already has - a head with
    // thirty-five inputs and a few thousand training rows should be spending its capacity on
    // the scene conditioning, not on rediscovering a division.
    out.push((stats.border_luma / stats.centre_luma.max(1e-3)).clamp(0.0, 4.0) / 4.0);
    debug_assert_eq!(out.len(), EXPOSURE_FEATURES);

    let index = scene.as_index();
    for slot in 0..EXPOSURE_SCENES {
        out.push(f32::from(u8::from(slot == index)));
    }
    out
}

/// One frame in, one estimate out.
#[derive(Debug)]
pub struct Analyser {
    heads: Heads,
    targets: TargetTable,
}

impl Analyser {
    /// Assemble an analyser over the embedded target table.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5063` when the embedded table will not load.
    pub fn new(infer: Arc<dyn InferService>) -> Result<Self, aura_core::AuraError> {
        Ok(Self {
            heads: Heads::new(infer),
            targets: TargetTable::embedded()?,
        })
    }

    /// Use a different target table.
    #[must_use]
    pub fn with_targets(mut self, targets: TargetTable) -> Self {
        self.targets = targets;
        self
    }

    /// The target table underneath.
    #[must_use]
    pub const fn targets(&self) -> &TargetTable {
        &self.targets
    }

    /// The heads underneath.
    #[must_use]
    pub const fn heads(&self) -> &Heads {
        &self.heads
    }

    /// Which execution provider the hardware plan selected.
    #[must_use]
    pub fn ep(&self) -> &'static str {
        self.heads.ep()
    }

    /// Load both heads.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.heads.warmup()
    }

    /// The learned illuminant hypothesis, or `None` when there is no trained head.
    ///
    /// **Returns `None` in this build**, because [`WB_HEAD_TRAINED`] is false. See the
    /// module header: a random projection of a thumbnail offered as one of six hypotheses
    /// would be given a 0.60 prior and would win on the frames where the honest estimators
    /// disagree, which is precisely the set of frames that most need a right answer.
    #[must_use]
    pub fn illuminant_hint(&self, stats: &FrameStats, prio: Priority) -> Option<[f32; 2]> {
        if !WB_HEAD_TRAINED {
            return None;
        }
        self.heads.illuminant(&stats.thumbnail, prio).ok()
    }

    /// The learned faceless exposure, or `None` when there is no trained head.
    #[must_use]
    pub fn scene_exposure_hint(
        &self,
        stats: &FrameStats,
        plane: &LumaPlane,
        scene: SceneId,
        prio: Priority,
    ) -> Option<f32> {
        if !EXPOSURE_HEAD_TRAINED {
            return None;
        }
        let features = exposure_features(stats, plane, scene);
        self.heads.scene_exposure(&features, prio).ok()
    }

    /// Analyse one frame.
    ///
    /// `loci` is every usable skin locus in the project, read once per pass by the caller.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5062` when the buffer is not 8-bit sRGB, which is the one shape this
    /// analyser cannot work around: every statistic in the phase is defined over an encoded
    /// sRGB proxy and reading a 16-bit linear buffer as one would produce a plausible,
    /// wrong answer rather than a failure.
    #[allow(clippy::too_many_lines)]
    pub fn analyse(
        &self,
        image: ImageId,
        buffer: &PixelBuffer,
        context: &FrameContext,
        loci: &BTreeMap<IdentityId, SkinLocus>,
        prio: Priority,
    ) -> AuraResult<FrameOutcome> {
        let Some(rgb) = buffer.as_srgb8() else {
            return Err(crate::errors::tone_failed(
                &image.to_db(),
                "the proxy is not 8-bit sRGB and this phase's statistics are defined over one",
            ));
        };
        let width = buffer.width as usize;
        let height = buffer.height as usize;
        let plane = luma_plane(rgb, width, height);
        let table = stats::linear_table();
        let stats = FrameStats::measure(rgb, width, height, &plane);

        let target = self.targets.get(context.scene);
        let patches = neutrals::find(rgb, width, height, &table);

        // Skin patches, one per face, from the buffer that is already decoded.
        let mut skin: Vec<(Option<IdentityId>, SkinPatch)> = Vec::new();
        for face in &context.subjects.faces {
            if let Some(patch) = stats::sample_skin(rgb, width, height, &table, face) {
                skin.push((face.identity_id, patch));
            }
        }

        // The hard constraints: people in this frame who have a measured locus.
        let mut constraints = Constraints::default();
        for (identity, patch) in &skin {
            let Some(identity) = identity else { continue };
            let Some(locus) = loci.get(identity) else {
                continue;
            };
            if !locus.is_usable() {
                continue;
            }
            constraints.people.push(Constraint {
                identity: *identity,
                observed: patch.linear,
                locus: *locus,
                prominence: context
                    .subjects
                    .prominence
                    .get(identity)
                    .copied()
                    .unwrap_or(0.0),
            });
        }
        constraints
            .people
            .sort_by(|a, b| b.prominence.total_cmp(&a.prominence));

        // The skin hypothesis: the light that would put the most prominent constrained
        // person exactly on their locus. Only the most prominent, deliberately - two people
        // with different loci imply two different lights, and averaging them produces a
        // chromaticity that satisfies neither.
        let skin_hint = constraints
            .people
            .first()
            .map(|person| lights::skin_implied_light(person.observed, person.locus.uv));

        let hypotheses = lights::generate(
            &stats,
            &patches,
            context.as_shot,
            self.illuminant_hint(&stats, prio),
            target.neutral_prior,
            skin_hint,
        );
        let mixed = lights::detect_mixed(&stats);

        let Some(chosen) = solve::choose(&hypotheses, &constraints, &patches) else {
            return Err(crate::errors::tone_failed(
                &image.to_db(),
                "no illuminant hypothesis could be formed",
            ));
        };
        let mut wb = solve::correct(
            chosen,
            &constraints,
            target,
            context.as_shot,
            &mixed,
            context.attrs,
            &hypotheses,
            &patches,
        );

        // Exposure, last. Its clipping bound reads the histogram and its anchor reads the
        // luminance plane; neither is touched by the colour solve, so the order is a
        // dependency rather than a convention.
        let subject = exposure::subject_luminance(&context.subjects.faces, &plane);
        let backlit = stats.looks_backlit() || context.scene_says_backlit();
        let bounds = Bounds {
            target,
            shadow_headroom_ev: context.shadow_headroom_ev,
            backlit,
        };
        let scene_ev = if subject.is_some() {
            None
        } else {
            self.scene_exposure_hint(&stats, &plane, context.scene, prio)
        };
        let solved = exposure::solve(subject, &stats, bounds, scene_ev);

        // The alternatives carry the exposure the frame actually got: a runner-up is a
        // different *colour* answer applied on top of the same exposure, and section 5's
        // triple would otherwise report a zero that a panel would render as "no exposure".
        for alternative in &mut wb.alternatives {
            alternative.exposure_ev = solved.ev;
        }

        // The samples this frame contributes to the loci. Produced from the light that was
        // actually chosen, which is why this happens after the solve and not before it.
        let chosen_uv = wb
            .illuminants
            .first()
            .map_or(chosen.hypothesis.uv, |light| light.uv);
        let mut samples = Vec::new();
        for (identity, patch) in &skin {
            let Some(identity) = identity else { continue };
            if let Some(sample) = skin_locus::sample_from(patch, chosen_uv, wb.confidence) {
                samples.push((*identity, sample));
            }
        }

        let dominant =
            lights::dominant_on_subject(&wb.illuminants, &context.subjects.faces, &stats);
        if dominant.is_some_and(|index| index != 0) {
            wb.reasons
                .push(codec::plain(ToneCode::DominantIlluminantChosen, -0.04));
        }

        let mut reasons = solved.reasons;
        reasons.extend(wb.reasons);
        if !target.measured {
            reasons.push(codec::plain(ToneCode::NoTargetRow, -0.05));
        }
        // Invariant 2, and migration 15's `reason_count` CHECK. Nothing below can produce an
        // empty list, and the assertion is here anyway because the schema's refusal is a
        // failed write in the middle of a 4,000-image pass and this is a bug caught in a test.
        if reasons.is_empty() {
            reasons.push(codec::plain(ToneCode::SubjectExposed, 0.0));
        }
        let (reasons, reason_numbers) = rank_reasons(reasons);

        let subject_after = if solved.face_anchored {
            exposure::linear_to_perceptual(
                exposure::perceptual_to_linear(solved.subject_before) * solved.ev.exp2(),
            )
        } else {
            0.0
        };
        let subject_in_band = solved.face_anchored && target.in_band(subject_after);

        let estimate = ToneEstimate {
            image_id: image,
            exposure_ev: solved.ev,
            exposure_conf: solved.confidence,
            temperature_k: wb.temperature_k,
            tint: wb.tint,
            wb_conf: wb.confidence,
            illuminants: wb.illuminants,
            mixed_light: mixed.mixed,
            dominant_on_subject: dominant,
            subject_luma_before: solved.subject_before,
            subject_luma_target: solved.subject_target,
            skin_de00_estimate: wb.skin_de00,
            alternatives: wb.alternatives,
            reasons,
            scene: if context.scene_known {
                context.scene
            } else {
                SceneId::Unknown
            },
            face_anchored: solved.face_anchored,
            backlit: solved.backlit,
            coloured_light: wb.coloured_light,
            clipping_added: solved.clipping_added,
            constrained_identities: constraints.people.len() as u32,
            user_edited: false,
            reviewed: false,
            model_ver: MODEL_VER,
            analysis_ver: ANALYSIS_VER,
            targets_ver: self.targets.version(),
        };

        Ok(FrameOutcome {
            estimate,
            samples,
            subject_in_band,
            reason_numbers,
        })
    }

    /// The band one scene is judged by. For the reference-frame selection and the gate.
    #[must_use]
    pub fn target_for(&self, scene: SceneId) -> SceneTarget {
        self.targets.get(scene)
    }
}

/// Keep the six most informative reasons, strongest doubt first.
///
/// Doubts before withdrawals, because a photographer opening a panel wants to know what went
/// wrong before being told what went right - and because the doubts are what decide whether
/// to look at the photograph at all.
fn rank_reasons(mut reasons: Vec<Explained>) -> (Vec<aura_core::ToneReason>, Vec<Vec<f32>>) {
    reasons.sort_by(|a, b| {
        a.0.weight
            .total_cmp(&b.0.weight)
            .then_with(|| a.0.code.as_str().cmp(b.0.code.as_str()))
    });
    reasons.dedup_by(|a, b| a.0.code == b.0.code);
    reasons.truncate(ToneEstimate::MAX_REASONS);
    reasons.into_iter().unzip()
}

/// How far a solved answer sits from a reference frame's, in `u'v'`.
///
/// Used by the gate and by the eval harness rather than by the pass. It is here because it
/// needs the same conversion the solve used, and a second copy in a test file would be a
/// second definition of "these two frames match".
#[must_use]
pub fn answer_distance(a: &Illuminant, b: &Illuminant) -> f32 {
    uv_distance(a.uv, b.uv)
}

/// The chromaticity a linear triple sits at. Re-exported for the fixtures and the gate.
#[must_use]
pub fn chromaticity(linear: [f32; 3]) -> [f32; 2] {
    linear_srgb_to_uv(linear)
}

/// The alternatives one estimate carries, capped.
#[must_use]
pub fn cap_alternatives(mut alternatives: Vec<ToneAlternative>) -> Vec<ToneAlternative> {
    alternatives.truncate(ToneAlternative::MAX);
    alternatives
}

/// What a mixed-light detection found, for the panel and the gate.
#[must_use]
pub fn mixed_summary(mixed: &MixedLight) -> Option<(f32, usize)> {
    if !mixed.mixed {
        return None;
    }
    Some((mixed.separation, mixed.groups.len()))
}
