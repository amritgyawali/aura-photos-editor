//! One decoded frame in, one plan out.
//!
//! The module the other seven feed into. [`Analyser::plan`] is the only place a
//! [`RestorePlan`] is constructed, so there is exactly one order in which the four decisions of
//! this phase are made, and it is the order the render graph executes them in.
//!
//! ## The order, and why it is this one
//!
//! 1. **Denoise**, because everything after it reads what it left. The tier comes from phase 09's
//!    measured `noise_sigma_rel` against the scene's own ceiling.
//! 2. **Face recovery**, because the identity constraint has to measure a face that is otherwise
//!    finished, and because its result feeds the self-check's third number.
//! 3. **Sharpen**, because its amount is capped by the noise the denoiser left - the one coupling
//!    between the operations of this phase, and it runs in one direction only.
//! 4. **The self-check**, which measures the whole plan on the rendered result and reduces until
//!    it is inside its bounds, or withdraws.
//!
//! ## Nothing here opens a file, and nothing here writes a recipe
//!
//! The frame arrives decoded. Invariant 1 is a property of the dependency list - see this crate's
//! `Cargo.toml` - and phase 14's rule that only `aura_recipe::schema::merge` writes a recipe is
//! kept by `tests/no_recipe_writes.rs`.
//!
//! ## Versions
//!
//! Three, and they invalidate three different things: [`MODEL_VER`] the learned decisions,
//! [`ANALYSIS_VER`] the arithmetic, and the profile tables' own version the ceilings. Phase 09's
//! rule for the ninth phase running, and `AURA-ML-5102` is what stops a comparison across any of
//! them happening silently.

use aura_core::contract::composition::Box2;
use aura_core::contract::integrity::MotionKind;
use aura_core::contract::restore::{
    ArtefactReport, RestoreCode, RestoreField, RestorePlan, RestoreReason, RestoreRegion,
    RestoreWhen, RunWhere, REVIEW_BELOW,
};
use aura_core::{AuraResult, PhotoId, SceneId};
use aura_render::restore::RestoreContext;
use std::collections::BTreeMap;

use crate::denoise::{self, NoiseEvidence};
use crate::errors;
use crate::face_recovery::{self, FaceCandidate, IdentityProbe};
use crate::kernel;
use crate::profiles::{NoiseTable, RestoreProfiles};
use crate::schedule::{self, Capacity};
use crate::selfcheck::{self, SelfCheckInput};
use crate::sharpen::{self, SharpenEvidence};

/// Which learned heads produced the decisions in a stored plan.
///
/// Zero, and it stays zero while neither shipped head is consulted. A build that starts
/// consulting one bumps this, `AURA-ML-5102` fires, and every stored plan is re-made - which is
/// the correct behaviour, because a plan made without a denoiser is not comparable with one made
/// with it.
pub const MODEL_VER: u16 = 0;

/// Which build's arithmetic produced them.
pub const ANALYSIS_VER: u16 = 1;

/// Which render level this phase reads.
///
/// The 2048 px proxy, as phases 06 to 21 do. Invariant 3's middle tier: restoration *decisions*
/// are made here and the operators are applied at full resolution at export, which is what makes
/// section 11's per-image budget about a decision rather than about a 45 MP denoise.
pub const RESTORE_LEVEL: u32 = 2048;

/// One decoded frame, with everything the four decisions read.
#[derive(Debug, Clone)]
pub struct RestoreFrame {
    /// The photograph.
    pub image_id: PhotoId,
    /// Interleaved linear RGB, `width * height * 3` long.
    pub pixels: Vec<f32>,
    /// Frame width in pixels.
    pub width: usize,
    /// Frame height in pixels.
    pub height: usize,
    /// The scene, from phase 07. Invariant 7.
    pub scene: SceneId,
    /// The camera make, as EXIF spells it.
    pub make: String,
    /// The camera model, as EXIF spells it.
    pub model: String,
    /// The frame's ISO.
    pub iso: u32,
    /// Phase 09's `noise_sigma_rel`. `None` when the frame has no integrity verdict.
    pub noise_sigma_rel: Option<f32>,
    /// Phase 09's motion verdict.
    pub motion: MotionKind,
    /// How much of it, `0..1`.
    pub motion_severity: f32,
    /// Phase 09's focus offset, `-1..1`.
    pub focus_offset: f32,
    /// Phase 06's subject prominence, `0..1`.
    pub prominence: f32,
    /// The delivery's long edge in pixels.
    pub output_long_edge: u32,
    /// Phase 18's regions, as this phase's port carries them.
    pub regions: Vec<RestoreField>,
    /// The faces phase 06 found, with their measured sharpness.
    pub faces: Vec<FaceCandidate>,
}

impl RestoreFrame {
    /// The frame's size in megapixels.
    #[must_use]
    pub fn megapixels(&self) -> f32 {
        (self.width * self.height) as f32 / 1_000_000.0
    }

    /// True when the buffer is the size the dimensions claim.
    #[must_use]
    pub fn is_readable(&self) -> bool {
        self.width > 0 && self.height > 0 && self.pixels.len() >= self.width * self.height * 3
    }
}

/// What one frame's planning did, beyond the plan itself.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RestoreOutcome {
    /// True when at least one usable region arrived from phase 18.
    pub region_covered: bool,
    /// Faces skipped because the embedding moved too far.
    pub identity_refusals: usize,
    /// True when the self-check reduced or withdrew something.
    pub reduced: bool,
    /// True when the camera's noise model was never measured.
    pub unmeasured_camera: bool,
    /// The camera key the noise model was looked up under.
    pub camera: String,
}

/// The one place a restoration plan is made.
#[derive(Debug)]
pub struct Analyser {
    profiles: RestoreProfiles,
    cameras: NoiseTable,
    capacity: Capacity,
}

impl Analyser {
    /// Load the two tables compiled into this build.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5105` when either table will not load.
    pub fn embedded(capacity: Capacity) -> AuraResult<Self> {
        Ok(Self {
            profiles: RestoreProfiles::embedded()?,
            cameras: NoiseTable::embedded()?,
            capacity,
        })
    }

    /// The three versions this build produces.
    #[must_use]
    pub fn versions(&self) -> (u16, u16, u16) {
        (
            MODEL_VER,
            ANALYSIS_VER,
            // One number for both tables: they are loaded together, refused together and
            // invalidate the same stored decisions, so two columns would be two ways to say the
            // same thing and one of them would eventually be wrong.
            self.profiles.version().max(self.cameras.version()),
        )
    }

    /// The scene profiles, for the panel and the gate.
    #[must_use]
    pub const fn profiles(&self) -> &RestoreProfiles {
        &self.profiles
    }

    /// The camera table, for the panel and the gate.
    #[must_use]
    pub const fn cameras(&self) -> &NoiseTable {
        &self.cameras
    }

    /// Plan one frame.
    ///
    /// `probe` is phase 06's recogniser, for the identity constraint. `None` means no face
    /// recovery is attempted at all - a guarantee that cannot be measured is a guarantee that
    /// cannot be kept - and every face records [`RestoreCode::RecoveryHeadUntrained`].
    ///
    /// # Errors
    ///
    /// `AURA-ML-5103` when the buffer cannot be read as pixels, or when the plan the solver
    /// produced breaks one of the nine guarantees.
    #[allow(clippy::too_many_lines)]
    pub fn plan(
        &self,
        frame: &RestoreFrame,
        probe: Option<&dyn IdentityProbe>,
        during_export: bool,
    ) -> AuraResult<(RestorePlan, RestoreOutcome)> {
        if !frame.is_readable() {
            return Err(errors::restore_failed(
                &frame.image_id.to_db(),
                "the proxy is not the size its dimensions claim",
            ));
        }

        let row = self.profiles.row(frame.scene);
        let model = self.cameras.model_for(&frame.make, &frame.model);
        let mut outcome = RestoreOutcome {
            unmeasured_camera: !model.measured,
            camera: model.camera.clone(),
            ..RestoreOutcome::default()
        };
        let mut reasons: Vec<RestoreReason> = Vec::new();

        // A frame too large for the processor path is a stored no-op with a reason rather than a
        // wait a photographer would kill the application during.
        if !self.capacity.gpu && !schedule::fits_on_cpu(frame.megapixels()) {
            let plan = RestorePlan::nothing(
                frame.image_id,
                frame.scene,
                RestoreReason::plain(RestoreCode::ScheduledOffInteractive, 0.0),
            );
            return Ok((plan, outcome));
        }

        // --- 1. denoise --------------------------------------------------------------------
        let choice = denoise::choose(
            NoiseEvidence {
                relative: frame.noise_sigma_rel,
                prominence: frame.prominence,
                output_long_edge: frame.output_long_edge,
                iso: frame.iso,
            },
            &model,
            row.max_tier,
            &self.profiles,
        );
        reasons.extend(choice.reasons.iter().cloned());

        // The regions, and the context every later step measures through.
        let usable: Vec<&RestoreField> = frame
            .regions
            .iter()
            .filter(|field| field.is_readable() && field.is_usable())
            .collect();
        outcome.region_covered = !usable.is_empty();
        if frame.regions.iter().any(|field| !field.is_readable()) {
            reasons.push(RestoreReason::plain(RestoreCode::RegionUnusable, -0.2));
        }
        let context = self.context(frame, &usable, choice.spec.as_ref().map(|s| s.sigma));

        // --- 2. face recovery --------------------------------------------------------------
        let mut recovery = face_recovery::solve(
            &frame.faces,
            row.face_recovery,
            self.profiles.max_face_recovery(),
        );
        reasons.extend(recovery.reasons.iter().cloned());
        let mut enforced = face_recovery::EnforceReport::default();
        match probe {
            Some(probe) if recovery.strength.is_some() => {
                enforced = face_recovery::enforce(
                    &frame.pixels,
                    frame.width,
                    frame.height,
                    &mut recovery.faces,
                    &context,
                    probe,
                );
                if enforced.skipped_for_identity > 0 {
                    reasons.push(RestoreReason::plain(
                        RestoreCode::IdentityDriftSkipped,
                        -1.0,
                    ));
                }
                if enforced.reduced > 0 {
                    reasons.push(RestoreReason::plain(
                        RestoreCode::StrengthReducedForIdentity,
                        -0.4,
                    ));
                }
            }
            _ => {
                // No probe means no face recovery, whatever the solver wanted. See `plan`'s doc
                // comment: a guarantee that cannot be measured is a guarantee that cannot be kept.
                for face in &mut recovery.faces {
                    if !face.skipped {
                        face.skipped = true;
                        face.strength = 0.0;
                        face.skipped_because = Some(RestoreCode::RecoveryHeadUntrained);
                    }
                }
            }
        }
        outcome.identity_refusals = enforced.skipped_for_identity;
        let face_strength = recovery
            .faces
            .iter()
            .filter(|face| !face.skipped)
            .map(|face| face.strength)
            .fold(0.0_f32, f32::max);

        // --- 3. sharpen --------------------------------------------------------------------
        let estimate = kernel::estimate(&frame.pixels, frame.width, frame.height);
        let residual = sharpen::residual(
            model.sigma_at(denoise::QUOTE_AT, frame.iso),
            choice.spec.as_ref().map_or(0.0, |spec| spec.luminance),
        );
        let sharpening = sharpen::choose(
            estimate,
            SharpenEvidence {
                motion: frame.motion,
                motion_severity: frame.motion_severity,
                focus_offset: frame.focus_offset,
                residual_sigma: residual,
            },
            &frame.regions,
            row.sharpen,
            &self.profiles,
        );
        reasons.extend(sharpening.reasons.iter().cloned());

        // --- 4. the self-check -------------------------------------------------------------
        let checked = selfcheck::enforce(
            &SelfCheckInput {
                pixels: &frame.pixels,
                width: frame.width,
                height: frame.height,
                context: &context,
                model: &model,
                iso: frame.iso,
                identity_drift: enforced.worst_kept_drift,
                face_recovery: face_strength,
                face_skipped: enforced.skipped_for_identity > 0,
                identity_resolves: enforced.resolves,
            },
            choice.tier,
            choice.spec,
            sharpening.spec,
        );
        reasons.extend(checked.reasons.iter().cloned());
        outcome.reduced = checked.report.denoise_reduced || checked.report.sharpen_reduced;

        let (run_where, mut where_reasons) =
            schedule::where_to_run(self.capacity, frame.megapixels());
        reasons.append(&mut where_reasons);

        // The plan, and then the nine checks. A plan that breaks one is refused rather than
        // stored: `RestorePlan::broken_guarantee` is asked here as well as in the store, because
        // the two are different callers and one of them will eventually be somebody else's.
        let is_noop = checked.tier == aura_core::contract::restore::DenoiseTier::Off
            && checked.sharpen.is_none()
            && face_strength <= 0.0;
        let plan = RestorePlan {
            image_id: frame.image_id,
            denoise: checked.tier,
            denoise_spec: checked.spec,
            sharpen: checked.sharpen,
            face_recovery: (face_strength > 0.0).then_some(face_strength),
            recovered: recovery.faces,
            run_where,
            when: schedule::when_to_run(during_export),
            selfcheck: if is_noop { None } else { Some(checked.report) },
            reasons: rank(reasons),
            confidence: confidence(&checked.report, outcome.region_covered, &model),
            scene: frame.scene,
            region_covered: outcome.region_covered,
            user_edited: false,
            reviewed: false,
            model_ver: MODEL_VER,
            analysis_ver: ANALYSIS_VER,
            profile_ver: self.versions().2,
        };
        if let Some(problem) = plan.broken_guarantee() {
            return Err(errors::restore_failed(&frame.image_id.to_db(), problem));
        }
        Ok((plan, outcome))
    }

    /// Turn the port's fields into the per-pixel planes the renderer measures through.
    #[allow(clippy::unused_self)]
    fn context(
        &self,
        frame: &RestoreFrame,
        usable: &[&RestoreField],
        sigma: Option<f32>,
    ) -> RestoreContext {
        let mut regions: BTreeMap<RestoreRegion, Vec<f32>> = BTreeMap::new();
        for field in usable {
            let plane = upsample(field, frame.width, frame.height);
            regions
                .entry(field.region)
                .and_modify(|existing| {
                    for (slot, value) in existing.iter_mut().zip(plane.iter()) {
                        *slot = slot.max(*value);
                    }
                })
                .or_insert(plane);
        }
        let faces = frame
            .faces
            .iter()
            .filter_map(|face| face_recovery::to_pixels(face.bounds, frame.width, frame.height))
            .collect();
        RestoreContext {
            regions,
            sigma,
            faces,
        }
    }
}

/// One field's grid, at frame resolution.
///
/// Bilinear, and it reads the **edge sample** outside the grid rather than zero. Phase 18's
/// defect: a resampler that reads zero outside a plane darkens the outermost half-pixel of every
/// region it delivers, which is a one-pixel rim around every mask produced by the code that
/// delivers a boundary rather than the code that finds it.
#[must_use]
pub fn upsample(field: &RestoreField, width: usize, height: usize) -> Vec<f32> {
    let mut plane = vec![0.0f32; width * height];
    if field.width == 0 || field.height == 0 || width == 0 || height == 0 {
        return plane;
    }
    let gw = usize::from(field.width);
    let gh = usize::from(field.height);
    for y in 0..height {
        let fy = ((y as f32 + 0.5) / height as f32 * gh as f32 - 0.5).max(0.0);
        let y0 = (fy.floor() as usize).min(gh - 1);
        let y1 = (y0 + 1).min(gh - 1);
        let ty = fy - y0 as f32;
        for x in 0..width {
            let fx = ((x as f32 + 0.5) / width as f32 * gw as f32 - 0.5).max(0.0);
            let x0 = (fx.floor() as usize).min(gw - 1);
            let x1 = (x0 + 1).min(gw - 1);
            let tx = fx - x0 as f32;
            let sample = |gx: usize, gy: usize| -> f32 {
                field
                    .alpha
                    .get(gy * gw + gx)
                    .map_or(0.0, |a| f32::from(*a) / 255.0)
            };
            let top = sample(x0, y0) + (sample(x1, y0) - sample(x0, y0)) * tx;
            let bottom = sample(x0, y1) + (sample(x1, y1) - sample(x0, y1)) * tx;
            if let Some(slot) = plane.get_mut(y * width + x) {
                *slot = (top + (bottom - top) * ty).clamp(0.0, 1.0);
            }
        }
    }
    plane
}

/// Keep the reasons that explain the most, strongest first.
///
/// The cap is [`RestorePlan::MAX_REASONS`] and the ranking is by absolute weight, so a refusal at
/// full weight outranks a mild confirmation - which is the right way round, because the commonest
/// question this phase generates is why something did *not* happen.
#[must_use]
pub fn rank(mut reasons: Vec<RestoreReason>) -> Vec<RestoreReason> {
    reasons.sort_by(|a, b| {
        b.weight
            .abs()
            .partial_cmp(&a.weight.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.code.as_str().cmp(b.code.as_str()))
    });
    reasons.dedup_by(|a, b| a.code == b.code);
    reasons.truncate(RestorePlan::MAX_REASONS);
    if reasons.is_empty() {
        reasons.push(RestoreReason::plain(RestoreCode::NoiseWithinTolerance, 0.0));
    }
    reasons
}

/// How much a plan trusts itself.
///
/// Lowered by three things, each of which is a piece of evidence that was missing rather than a
/// piece that was unfavourable: no regions from phase 18, an unmeasured camera, and a self-check
/// that had to reduce something. A plan drawn with two of its three inputs missing has to be able
/// to say so - invariant 2, and phase 09's rule about coverage read at the frame level.
#[must_use]
pub fn confidence(
    report: &ArtefactReport,
    region_covered: bool,
    model: &aura_core::contract::restore::NoiseModel,
) -> f32 {
    let mut confidence = 1.0f32;
    if !region_covered {
        confidence *= 0.70;
    }
    if !model.measured {
        confidence *= 0.85;
    }
    if report.resolves > 0 {
        confidence *= 0.90;
    }
    if report.measured_on == 0 {
        // Nothing was rendered, which is either a frame that needed nothing - confidence stays
        // high - or a frame nothing could be measured on. The two are told apart by whether the
        // plan is a no-op, which the caller knows and this function does not, so the safe reading
        // is a small reduction rather than either extreme.
        confidence *= 0.95;
    }
    confidence.clamp(0.0, 1.0)
}

/// True when a plan is worth a photographer's attention.
#[must_use]
pub fn needs_review(confidence: f32) -> bool {
    confidence < REVIEW_BELOW
}

/// A face candidate from a box and a sharpness, for the callers that build one.
#[must_use]
pub fn candidate(
    bounds: Box2,
    sharpness: f32,
    identity: Option<aura_core::IdentityId>,
) -> FaceCandidate {
    FaceCandidate {
        identity,
        bounds,
        sharpness,
    }
}

/// The occasion a pass was triggered by, for the store.
#[must_use]
pub const fn occasion(during_export: bool) -> RestoreWhen {
    schedule::when_to_run(during_export)
}

/// Where a pass would run, for the panel.
#[must_use]
pub fn destination(capacity: Capacity, megapixels: f32) -> RunWhere {
    schedule::where_to_run(capacity, megapixels).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use aura_core::contract::restore::DenoiseTier;

    fn analyser() -> Analyser {
        Analyser::embedded(Capacity::default()).expect("the embedded tables load")
    }

    #[test]
    fn a_clean_frame_is_planned_and_nothing_is_done_to_it() {
        let frame = fixtures::clean_frame();
        let (plan, outcome) = analyser().plan(&frame, None, true).expect("a plan");
        assert!(plan.is_sound(), "{:?}", plan.broken_guarantee());
        assert_eq!(plan.denoise, DenoiseTier::Off);
        assert!(plan.is_noop());
        assert!(!plan.reasons.is_empty());
        assert!(!outcome.reduced);
    }

    #[test]
    fn a_noisy_frame_is_denoised_and_the_plan_says_what_with() {
        let frame = fixtures::noisy_frame();
        let (plan, outcome) = analyser().plan(&frame, None, true).expect("a plan");
        assert!(plan.is_sound(), "{:?}", plan.broken_guarantee());
        assert_ne!(plan.denoise, DenoiseTier::Off);
        let spec = plan.denoise_spec.as_ref().expect("a tier carries a spec");
        assert!(spec.colour >= spec.luminance);
        assert!(spec.sigma > 0.0);
        assert!(!spec.measured_model, "no model in this build is measured");
        assert!(outcome.unmeasured_camera);
        assert!(plan.selfcheck.is_some());
        assert!(plan
            .selfcheck
            .as_ref()
            .is_some_and(ArtefactReport::is_clean));
    }

    #[test]
    fn a_frame_with_no_regions_is_never_sharpened() {
        let mut frame = fixtures::soft_frame();
        frame.regions.clear();
        let (plan, outcome) = analyser().plan(&frame, None, true).expect("a plan");
        assert!(plan.sharpen.is_none());
        assert!(!outcome.region_covered);
        assert!(plan
            .reasons
            .iter()
            .any(|r| r.code == RestoreCode::SharpenNoRegions));
        assert!(plan.is_sound(), "{:?}", plan.broken_guarantee());
    }

    #[test]
    fn a_soft_frame_with_regions_is_sharpened_and_stays_inside_the_ringing_bound() {
        let frame = fixtures::soft_frame();
        let (plan, _) = analyser().plan(&frame, None, true).expect("a plan");
        assert!(plan.is_sound(), "{:?}", plan.broken_guarantee());
        if let Some(spec) = &plan.sharpen {
            assert!(spec.problem().is_none(), "{:?}", spec.problem());
            assert!(plan.region_covered);
            let report = plan.selfcheck.as_ref().expect("a plan that acted measured");
            assert!(report.ringing <= aura_core::contract::restore::MAX_RINGING);
        }
    }

    #[test]
    fn no_face_is_recovered_without_a_probe_and_every_one_says_why() {
        let frame = fixtures::soft_face_frame();
        let (plan, outcome) = analyser().plan(&frame, None, true).expect("a plan");
        assert!(plan.face_recovery.is_none());
        assert_eq!(outcome.identity_refusals, 0);
        assert!(!plan.recovered.is_empty());
        for face in &plan.recovered {
            assert!(face.skipped);
            assert!(face.skipped_because.is_some());
        }
    }

    #[test]
    fn a_frame_that_will_not_read_is_refused_rather_than_planned_empty() {
        let mut frame = fixtures::clean_frame();
        frame.pixels.truncate(10);
        let error = analyser()
            .plan(&frame, None, true)
            .expect_err("a truncated buffer is refused");
        assert_eq!(error.code.0, "AURA-ML-5103");
    }

    #[test]
    fn the_reason_list_is_capped_and_the_strongest_survive() {
        let many: Vec<RestoreReason> = RestoreCode::ALL
            .iter()
            .enumerate()
            .map(|(index, code)| RestoreReason::plain(*code, index as f32 / 30.0))
            .collect();
        let ranked = rank(many);
        assert_eq!(ranked.len(), RestorePlan::MAX_REASONS);
        for pair in ranked.windows(2) {
            assert!(pair[0].weight.abs() >= pair[1].weight.abs());
        }
        // And an empty list still produces a reason, because a plan with no reason is a bug.
        assert_eq!(rank(Vec::new()).len(), 1);
    }

    #[test]
    fn confidence_falls_for_each_missing_input() {
        let model = aura_core::contract::restore::NoiseModel::reference();
        let mut measured = model.clone();
        measured.measured = true;
        let clean = ArtefactReport {
            measured_on: 1000,
            ..ArtefactReport::UNTOUCHED
        };
        let best = confidence(&clean, true, &measured);
        assert!(best > 0.99);
        assert!(confidence(&clean, false, &measured) < best);
        assert!(confidence(&clean, true, &model) < best);
        let resolved = ArtefactReport {
            resolves: 2,
            measured_on: 1000,
            ..ArtefactReport::UNTOUCHED
        };
        assert!(confidence(&resolved, true, &measured) < best);
        assert!(needs_review(0.2));
        assert!(!needs_review(0.9));
    }

    #[test]
    fn the_upsampler_does_not_manufacture_a_rim() {
        // Phase 18's defect, and the reason this function reads the edge sample rather than zero:
        // a resampler that reads outside the plane darkens the outermost half-pixel of every
        // region, which is a one-pixel dark rim around every mask.
        let field = RestoreField {
            region: RestoreRegion::Subject,
            identity: None,
            bounds: Box2 {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            width: 4,
            height: 4,
            alpha: vec![255; 16],
            confidence: 0.9,
            edge_quality: 0.9,
            model_ver: 1,
        };
        let plane = upsample(&field, 32, 32);
        assert_eq!(plane.len(), 32 * 32);
        for (index, value) in plane.iter().enumerate() {
            assert!(
                (*value - 1.0).abs() < 1e-5,
                "sample {index} is {value}, not 1.0"
            );
        }
    }
}
