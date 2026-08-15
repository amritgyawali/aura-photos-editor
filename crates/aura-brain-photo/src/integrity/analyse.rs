//! One decoded frame in, one technical verdict out.
//!
//! The shape phase 05 and phase 06 both argued for, a third time: **the pixels are
//! decoded once**. From one 2048 px proxy come the luminance plane, the region
//! sharpnesses, the structure tensors, the clipping counts, the noise tiles, every
//! aligned face crop and both models' inputs. Nothing here opens a file twice, and
//! section 11's 45 ms budget is only reachable because of it.
//!
//! Section 8's step 10 asks PERF to "fuse passes to avoid re-reading pixels". The fusion
//! is structural rather than an optimisation pass: [`Analyser::analyse`] takes a
//! `PixelBuffer` and every measurement in the crate takes either that buffer or the one
//! `LumaPlane` derived from it.
//!
//! ## Two batched model calls per frame, never two per face
//!
//! A group photograph has thirty faces. Thirty calls into the runtime would discard the
//! memory ledger and the batch scheduler phase 03 built, which is the mistake
//! `aura_vision::face::FacePipeline::analyse` documents at its step 3 and this module
//! copies deliberately.
//!
//! ## What is measured against what
//!
//! | Measurement | Reads | Conditioned by |
//! |---|---|---|
//! | subject sharpness | eye regions, then faces, then bodies | `expected_mtf50` for the body |
//! | motion | structure tensor on subject and background | shutter, focal length, scene |
//! | exposure | clipping with speculars excluded | measured headroom for the body |
//! | noise | the flattest quarter of the frame's tiles | ISO, body, `max_acceptable_noise` |
//! | eyes | one aligned crop per face | prominence, scene, the partner's eyes |
//!
//! Every row's third column is either phase 07's profile or phase 09's calibration
//! table. Invariant 7 - no threshold is global - is not a thing this module remembers to
//! do; it is a thing it cannot avoid, because none of the analysers takes a constant it
//! could have used instead.

use std::sync::Arc;

use aura_core::contract::integrity::{CropRect, IntegrityResult};
use aura_core::contract::people::{FaceRef, ImageSubjects};
use aura_core::{AuraResult, IdentityId, PhotoId, Priority, SceneId, SceneProfile};
use aura_infer::contract::infer::InferService;
use aura_raw::contract::pixels::{PixelBuffer, PixelLevel};
use aura_vision::embed::descriptors::{luma_plane, LumaPlane};

use crate::integrity::calibration::{Calibration, CalibrationTable};
use crate::integrity::exposure;
use crate::integrity::eyes::{self, EyeStateModel, IntentInput};
use crate::integrity::flags::{self, EyeVerdict, Evidence};
use crate::integrity::focus::{self, FocusHead, PixelRect, RegionSet};
use crate::integrity::motion::{self, MotionContext};
use crate::integrity::noise;
use crate::integrity::score::{self, SceneCalibration};

/// Which build's arithmetic produced a verdict.
///
/// Bumped on any change to a threshold, a sub-score curve, a flag rule or the way the
/// composite is combined - which is to say on almost any change in this crate that is
/// not a comment. It is written into `image_integrity.analysis_ver`, and two verdicts
/// made under different values of it are not comparable: `AURA-ML-5033` exists so that
/// comparison never happens silently.
pub const ANALYSIS_VER: u16 = 1;

/// The pixel rung the integrity pass reads.
///
/// Tier 2, the 2048 px proxy - the same rung the face pass uses, and for a related
/// reason. Invariant 3 puts medium analysis here, and this is medium analysis: a 384 px
/// thumbnail cannot answer whether an eye is sharp, because at that size an eye is nine
/// pixels across and *every* eye is sharp. The proxy is already built and cached by phase
/// 02, so the cost is a cache read rather than a decode - and it is the same cache read
/// phase 06 already made, which is why running the two passes back to back is nearly
/// free.
///
/// Tier 3 was considered and rejected. Full sensor resolution would measure the lens
/// rather than the photograph, would cost 1.8 s a frame against a 220 ms budget, and
/// would make every threshold in the crate a function of megapixels in a way the
/// calibration table could not fully divide out.
pub const INTEGRITY_LEVEL: PixelLevel = PixelLevel::Proxy2048;

/// What EXIF said about one frame.
///
/// Read from the catalog by the caller rather than from the file by this crate: phase 01
/// owns EXIF, it is already in `exif`, and opening the original to re-read a shutter
/// speed would break invariant 1's spirit and section 11's budget at the same time.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrameExif {
    /// Camera make, as written.
    pub make: String,
    /// Camera model, as written.
    pub model: String,
    /// ISO. Zero when EXIF had none, which is treated as base ISO.
    pub iso: u32,
    /// Exposure time in seconds.
    pub shutter_s: Option<f32>,
    /// Focal length in millimetres, 35 mm equivalent.
    pub focal_mm: Option<f32>,
    /// True when stabilisation was reported active.
    pub stabilised: bool,
    /// The original's pixel count in megapixels, for the uncalibrated fallback.
    pub megapixels: f32,
}

/// Everything outside the pixels that one frame's verdict depends on.
///
/// Assembled by the caller from the four frozen services - `StoryService` for the scene
/// and the profile, `PeopleService` for the subjects, and nothing else. This crate takes
/// no dependency on the crates that implement them, so the only way a scene or an
/// identity can reach an analyser is through this struct.
#[derive(Debug, Clone)]
pub struct FrameContext {
    /// What phase 07 says the photograph is of.
    pub scene: SceneId,
    /// The tolerances and weights that scene is judged by.
    pub profile: SceneProfile,
    /// Who is in the frame, from phase 06.
    pub subjects: ImageSubjects,
    /// The identities phase 06 calls primary, for the both-partners intent rule.
    pub couple: Vec<IdentityId>,
    /// What EXIF said.
    pub exif: FrameExif,
    /// True when a scene label was actually present.
    ///
    /// Distinguished from `scene == SceneId::Unknown` because the two mean different
    /// things: a classified-as-unknown frame has been looked at and an unclassified one
    /// has not, and only the second should cost the verdict confidence.
    pub scene_known: bool,
}

impl Default for FrameContext {
    /// A frame nobody has classified and nobody has scanned for faces.
    ///
    /// Hand-written rather than derived because `SceneProfile` has no `Default` - phase
    /// 07 refused one deliberately, so that a profile always names the scene it describes
    /// rather than silently describing nothing. `SceneProfile::neutral` is the documented
    /// substitute and this is where it is substituted.
    fn default() -> Self {
        Self {
            scene: SceneId::Unknown,
            profile: SceneProfile::neutral(SceneId::Unknown),
            subjects: ImageSubjects::default(),
            couple: Vec::new(),
            exif: FrameExif::default(),
            scene_known: false,
        }
    }
}

impl FrameContext {
    /// True when this scene expects motion in it.
    ///
    /// Section 6.2 names `dance_floor` and `exit`. `first_dance` and the reception
    /// entrance are added on the same argument: both are people moving under a slow
    /// shutter on purpose, and a photographer who panned an entrance would not thank a
    /// product that called it shake.
    #[must_use]
    pub const fn expects_motion(&self) -> bool {
        matches!(
            self.scene,
            SceneId::DanceFloor
                | SceneId::Exit
                | SceneId::FirstDance
                | SceneId::ReceptionEntrance
        )
    }

    /// The faces that will be judged, in prominence order.
    #[must_use]
    pub fn faces(&self) -> Vec<(FaceRef, f32)> {
        let mut out: Vec<(FaceRef, f32)> = self
            .subjects
            .faces
            .iter()
            .map(|face| {
                let prominence = face
                    .identity_id
                    .and_then(|identity| self.subjects.prominence.get(&identity).copied())
                    .unwrap_or(0.0);
                (*face, prominence)
            })
            .collect();
        out.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then(right.0.area_frac.total_cmp(&left.0.area_frac))
        });
        out
    }
}

/// The per-frame pipeline.
#[derive(Debug, Clone)]
pub struct Analyser {
    focus_head: FocusHead,
    eye_head: EyeStateModel,
    calibration: CalibrationTable,
    scores: SceneCalibration,
    /// When set, the fixture reader replaces the eye head. The gates and
    /// `tests/eval/integrity_eval.rs` use it; nothing in the product does.
    reference_eyes: bool,
}

impl Analyser {
    /// Assemble the pipeline over one inference service, with the shipped calibration.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5036` when the embedded calibration table will not load, which is a
    /// build bug rather than a deployment one.
    pub fn new(infer: Arc<dyn InferService>) -> Result<Self, aura_core::AuraError> {
        Ok(Self {
            focus_head: FocusHead::new(Arc::clone(&infer)),
            eye_head: EyeStateModel::new(infer),
            calibration: CalibrationTable::embedded()?,
            scores: SceneCalibration::shipped(),
            reference_eyes: false,
        })
    }

    /// Use a different calibration table. The installation override path, and QA's
    /// sweeps.
    #[must_use]
    pub fn with_calibration(mut self, table: CalibrationTable) -> Self {
        self.calibration = table;
        self
    }

    /// Install fitted per-scene score calibration.
    #[must_use]
    pub fn with_score_calibration(mut self, scores: SceneCalibration) -> Self {
        self.scores = scores;
        self
    }

    /// Read eye states from the fixture marker instead of from the head.
    ///
    /// **This is the only way to state a blink accuracy at all** while the shipped head
    /// is a placeholder, and it is the same device phase 06 used for detection recall.
    /// What it measures is everything around the head - the alignment, the gating, the
    /// four intent rules, the group ratio and the flag decision - against an answer that
    /// is known by construction. What it does not measure is the weights, and the phase
    /// 09 exit report says so where the number is reported.
    ///
    /// Explicit at the call site rather than a feature flag, so no product path can
    /// reach it by accident.
    #[must_use]
    pub const fn with_reference_eyes(mut self) -> Self {
        self.reference_eyes = true;
        self
    }

    /// The calibration table in force.
    #[must_use]
    pub const fn calibration(&self) -> &CalibrationTable {
        &self.calibration
    }

    /// Load both heads before a photographer is waiting on them.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised. A warmup failure is a real failure: it is how the
    /// product learns a model pack is broken before a wedding depends on it.
    pub fn warmup(&self) -> AuraResult<()> {
        self.focus_head.warmup()?;
        self.eye_head.warmup()
    }

    /// Which execution provider the hardware plan selected.
    #[must_use]
    pub fn ep(&self) -> &'static str {
        self.focus_head.ep()
    }

    /// Analyse one frame.
    ///
    /// # Errors
    ///
    /// * `AURA-ML-5007` when the buffer is not 8-bit sRGB or a model's tensors disagree
    ///   with the manifest.
    /// * `AURA-ML-5011` when the work was cancelled.
    /// * Whatever `InferService` raised on first use.
    ///
    /// A face whose crop is unusable is **not** an error: it is recorded with no eye
    /// state, it does not gate, and the frame is scored without it. One bad face must
    /// not fail a frame, let alone a wedding.
    #[allow(clippy::too_many_lines)]
    pub fn analyse(
        &self,
        image_id: PhotoId,
        buffer: &PixelBuffer,
        context: &FrameContext,
        prio: Priority,
    ) -> AuraResult<IntegrityResult> {
        let Some(rgb) = buffer.as_srgb8() else {
            return Err(aura_core::errors::ml::shape_mismatch(
                "an 8-bit sRGB buffer",
                buffer.data.as_str(),
            ));
        };
        let width = buffer.width as usize;
        let height = buffer.height as usize;
        if width == 0 || height == 0 || rgb.len() < width * height * 3 {
            return Err(aura_core::errors::ml::shape_mismatch(
                &format!("{width}x{height}x3 samples"),
                &format!("{} samples", rgb.len()),
            ));
        }

        // 1. The one derived plane. Everything below reads this or the buffer, never a
        //    file.
        let plane = luma_plane(rgb, width, height);
        let calibration = self
            .calibration
            .get(&context.exif.make, &context.exif.model, context.exif.megapixels);

        // 2. Regions. The subject question and the "is there a subject" question are the
        //    same question, asked once.
        let faces = context.faces();
        let subjectless = faces.is_empty();
        let regions = if subjectless {
            RegionSet::subjectless()
        } else {
            let described: Vec<(CropRect, [[f32; 2]; 2], bool)> = faces
                .iter()
                .map(|(face, _)| (face.bbox, face.eyes, face.has_eyes()))
                .collect();
            RegionSet::for_face(&described, &[])
        };
        let weights: Vec<f32> = faces.iter().map(|(_, prominence)| *prominence).collect();

        // 3. Focus, classical.
        let mut focus_result = focus::analyse(&plane, &regions, &weights, &calibration);

        // 4. Focus, learned. One batched call over the subject region, and the head may
        //    only ever withdraw a softness claim - see `focus::HEAD_OVERRULES_AT`.
        let subject_rect = regions.faces.first().copied();
        if let Some(rect) = subject_rect
            .and_then(|rect| PixelRect::from_norm(rect, plane.width, plane.height))
        {
            let tensor = focus::preprocess_region(&plane, rect);
            // A head that will not run costs an *exoneration*, never a verdict - so it
            // degrades rather than failing the frame. That is only safe because of the
            // asymmetry at `focus::HEAD_OVERRULES_AT`: this head can withdraw a softness
            // claim and cannot make one, so its absence can only ever make the product
            // more cautious. The eye head, which can convict, is not treated this way.
            match self.focus_head.run_batch(&[tensor], prio) {
                Ok(verdicts) => {
                    focus_result = focus::apply_head(focus_result, verdicts.first().copied());
                }
                Err(err) => {
                    tracing::debug!(
                        target: "integrity.focus",
                        photo = %image_id,
                        code = %err.code,
                        "the focus head did not run; the classical measures decide alone"
                    );
                }
            }
        }

        // 5. Motion, on the same regions the focus analysis used, so the two cannot
        //    disagree about where the subject is.
        let motion_result = motion::analyse(
            &plane,
            subject_rect,
            regions.far.or(regions.near),
            focus_result.raw_subject,
            focus_result.raw_background,
            MotionContext {
                shutter_s: context.exif.shutter_s,
                focal_mm: context.exif.focal_mm,
                stabilised: context.exif.stabilised,
                expects_motion: context.expects_motion(),
            },
        );

        // 6. Exposure and noise.
        let clipping = exposure::measure(&plane, rgb, width, height);
        let median = plane_median(&plane);
        let exposure_result = exposure::verdict(
            clipping,
            exposure::ev_offset(median),
            context.exif.iso.max(1),
            &calibration,
            context.profile.max_acceptable_noise,
            exposure::mixed_light_risk(rgb, width, height),
        );
        let noise_result = noise::analyse(
            &plane,
            context.exif.iso.max(1),
            &calibration,
            context.profile.max_acceptable_noise,
        );

        // 7. Eyes. One batched call over every usable face crop.
        let (eye_states, eye_verdicts, closed_ratio, gating, unjustified) =
            self.judge_eyes(rgb, width, height, &faces, context, prio)?;

        // 8. Flags and reasons.
        let evidence = Evidence {
            focus: &focus_result,
            motion: &motion_result,
            exposure: &exposure_result,
            noise: &noise_result,
            eyes: &eye_verdicts,
            closed_eye_ratio: closed_ratio,
            subjectless,
            profile: &context.profile,
            calibration: &calibration,
        };
        let verdict = flags::decide(&evidence);

        // 9. The composite.
        let sub = score::sub_scores(
            &focus_result,
            &motion_result,
            &exposure_result,
            &noise_result,
            &context.profile,
            unjustified,
        );
        let technical_score = score::compose(
            sub,
            score::Weights::for_scene(&context.profile),
            &self.scores.get(context.scene),
        );

        Ok(IntegrityResult {
            image_id,
            subject_sharpness: focus_result.subject,
            bg_sharpness: focus_result.background,
            focus_offset: focus_result.offset,
            motion: motion_result.kind,
            motion_severity: motion_result.severity,
            exposure: exposure_result.verdict,
            clip_hi: exposure_result.clipping.high,
            clip_lo: exposure_result.clipping.low,
            ev_offset: exposure_result.ev_offset,
            noise_sigma_rel: noise_result.sigma_rel,
            eyes: eye_states,
            closed_eye_ratio: closed_ratio,
            gating_faces: gating,
            technical_score,
            scene: context.scene,
            // Filled by the store once the frame's moment is known. 0.5 - neither the
            // best nor the worst of no siblings - until then.
            relative_sharpness: 0.5,
            flags: verdict.flags,
            reasons: verdict.reasons,
            confidence: confidence_of(context, &calibration, &noise_result, subjectless),
            user_reviewed: false,
            model_ver: focus::MODEL_VER,
            analysis_ver: ANALYSIS_VER,
            calib_ver: self.calibration.version(),
        })
    }

    /// Classify every face's eyes, apply the intent rules, and summarise.
    ///
    /// Returns the states, the flag-facing verdicts, the closed ratio, the gating count
    /// and the fraction of gating faces whose closure was **not** justified - which is
    /// the only one of the five the score uses.
    fn judge_eyes(
        &self,
        rgb: &[u8],
        width: usize,
        height: usize,
        faces: &[(FaceRef, f32)],
        context: &FrameContext,
        prio: Priority,
    ) -> AuraResult<(
        Vec<aura_core::contract::integrity::EyeState>,
        Vec<EyeVerdict>,
        f32,
        u32,
        f32,
    )> {
        // A face with no landmarks cannot be aligned, so it is skipped rather than
        // cropped from its box: a box crop of a tilted head puts the eyes on a diagonal
        // and the head would be answering about the wrong pixels. Skipping costs an eye
        // reading; guessing costs trust.
        let usable: Vec<usize> = faces
            .iter()
            .enumerate()
            .filter(|(_, (face, _))| face.has_eyes())
            .map(|(index, _)| index)
            .collect();

        let mut tensors = Vec::with_capacity(usable.len());
        let mut crops = Vec::with_capacity(usable.len());
        for index in &usable {
            if let Some((face, _)) = faces.get(*index) {
                let crop = eyes::align_from_eyes(rgb, width, height, face);
                tensors.push(eyes::preprocess_crop(&crop));
                crops.push(crop);
            }
        }
        let readings = if self.reference_eyes {
            crops
                .iter()
                .map(|crop| {
                    crate::fixtures::ReferenceEyeReader::read(
                        crop,
                        aura_infer::onnx::fixtures::EYE_CROP_SIDE,
                    )
                })
                .collect()
        } else {
            self.eye_head.run_batch(&tensors, prio)?
        };

        // The both-partners rule needs every face's reading before any face's intent can
        // be decided, which is why this is two passes rather than one.
        let mut per_face: Vec<(FaceRef, aura_core::contract::integrity::EyeOpenness, f32)> =
            Vec::with_capacity(usable.len());
        for (slot, index) in usable.iter().enumerate() {
            let Some((face, _)) = faces.get(*index) else {
                continue;
            };
            let (state, confidence) = readings
                .get(slot)
                .copied()
                .unwrap_or((aura_core::contract::integrity::EyeOpenness::Open, 0.0));
            per_face.push((*face, state, confidence));
        }
        let openness: Vec<(FaceRef, aura_core::contract::integrity::EyeOpenness)> =
            per_face.iter().map(|(face, state, _)| (*face, *state)).collect();
        let both_closed = eyes::both_partners_closed(&openness, &context.couple);

        let mut states = Vec::with_capacity(per_face.len());
        let mut verdicts = Vec::with_capacity(per_face.len());
        for (face, state, confidence) in &per_face {
            let prominence = faces
                .iter()
                .find(|(candidate, _)| candidate.face_id == face.face_id)
                .map_or(0.0, |(_, prominence)| *prominence);
            let is_partner = face
                .identity_id
                .is_some_and(|identity| context.couple.contains(&identity));
            let (eye_state, rule) = eyes::state_for(
                face,
                (*state, *confidence),
                prominence,
                IntentInput {
                    scene: context.scene,
                    laughing: eyes::laughing_from_geometry(face),
                    // Phase 10's rule. Always false here; see this module's `eyes`
                    // sibling for why the field exists anyway.
                    tears: false,
                    partner_also_closed: both_closed && is_partner,
                },
            );
            verdicts.push(EyeVerdict {
                crop: eye_state.crop,
                closed: eye_state.state.is_closed(),
                squinting: matches!(
                    eye_state.state,
                    aura_core::contract::integrity::EyeOpenness::Squint
                ),
                gates: eye_state.gates,
                rule,
            });
            states.push(eye_state);
        }

        let (ratio, gating) = eyes::closed_ratio(&states);
        let unjustified = if gating == 0 {
            0.0
        } else {
            let count = states
                .iter()
                .filter(|state| state.is_defect())
                .count();
            f64::from(u32::try_from(count).unwrap_or(0)) as f32 / f64::from(gating) as f32
        };
        Ok((states, verdicts, ratio, gating, unjustified))
    }
}

/// The frame's median luminance, from a 256-bin histogram.
///
/// Exact rather than approximate: an 8-bit proxy has 256 distinct values, so the
/// histogram is the distribution rather than a summary of it.
fn plane_median(plane: &LumaPlane) -> f32 {
    if plane.values.is_empty() {
        return 0.0;
    }
    let mut histogram = [0u32; 256];
    for value in &plane.values {
        let bin = (value.clamp(0.0, 1.0) * 255.0).round() as usize;
        if let Some(slot) = histogram.get_mut(bin.min(255)) {
            *slot += 1;
        }
    }
    let target = u32::try_from(plane.values.len() / 2).unwrap_or(u32::MAX);
    let mut seen = 0u32;
    for (bin, hits) in histogram.iter().enumerate() {
        seen += hits;
        if seen >= target {
            return bin as f32 / 255.0;
        }
    }
    1.0
}

/// How sure the whole verdict is.
///
/// Invariant 2, and the number is built by subtraction rather than by a model: it starts
/// at 0.95 - never 1.0, because a technical verdict from a 2048 px proxy is an inference
/// about an original nobody opened - and loses something for each input that was missing.
///
/// | Missing | Cost | Why |
/// |---|---|---|
/// | calibration row | `Calibration::confidence_penalty` | every normalised number is against a guessed baseline |
/// | scene label | 0.10 | every threshold was the neutral profile's |
/// | any subject | 0.12 | the sharpness figure is about the frame rather than about a person |
/// | noise estimate | 0.05 | there was no flat region to measure |
fn confidence_of(
    context: &FrameContext,
    calibration: &Calibration,
    noise: &noise::NoiseResult,
    subjectless: bool,
) -> f32 {
    let mut confidence = 0.95f32;
    if !calibration.measured {
        confidence -= calibration.confidence_penalty;
    }
    if !context.scene_known {
        confidence -= 0.10;
    }
    if subjectless {
        confidence -= 0.12;
    }
    if !noise.measured {
        confidence -= 0.05;
    }
    confidence.clamp(0.05, 0.95)
}

// There is deliberately no constructor here for "the verdict of a frame that could not
// be analysed". Migration 9's fifth property is that *not checked* is not *clean*, and a
// function that produced an empty verdict would be the obvious way to break it. A frame
// that fails raises `AURA-ML-5035`, writes no row, and is retried by the next pass.
