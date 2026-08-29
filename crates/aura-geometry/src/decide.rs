//! One decoded frame in, one plan out.
//!
//! The order is section 8's, and it is not the order the fields are declared in
//! [`aura_core::contract::geometry::GeometryPlan`]: the lens is decided first because a distortion
//! correction moves every line in the frame and the keystone solver measures lines; the rotation
//! is decided next because it costs a crop; the keystone after it because it costs another; and
//! the crop last, inside whatever both of them left.
//!
//! ## Nothing here writes a recipe
//!
//! Phase 14's rule. `aura_recipe::schema::merge` is the only function in the workspace that
//! writes one recipe into another and it lives in `aura-app`;
//! `crates/aura-geometry/tests/no_recipe_writes.rs` is the grep that keeps this crate away from
//! it. What this module produces is a *decision*, and the decision is turned into recipe fields
//! by the caller - which is what makes "a parameter a person set is never overwritten" a property
//! of one function rather than of every phase that ever fills one.
//!
//! ## Three versions would be two too many
//!
//! [`ANALYSIS_VER`] and the profile tables' combined version, and that is all. This phase ships no
//! model, so there is no `model_ver` that could move - the third phase in the product to be in
//! that position, after 08 and 17. `AURA-ML-5109` carries two numbers for the same reason.
//!
//! ## What the confidence is about
//!
//! Not "how good is this crop". [`GeometryPlan::confidence`] is **how much this phase knew about
//! the photograph when it decided**, which is why a frame it deliberately left alone can be at
//! 0.9 and a frame it left alone because nothing could be measured is at 0.4. The two are the
//! same plan and a caller that could not tell them apart would send the wrong half of a wedding
//! to a review queue.

use aura_core::contract::composition::Box2;
use aura_core::contract::geometry::{
    AspectRatio, CropSafetyReport, CropVariant, GeometryCode, GeometryPlan, GeometryReason,
    ImageId, Keystone, LensCorrection, LensSource, ProtectedRegion, MAX_REASONS,
};
use aura_core::{AuraError, SceneId};
use aura_render::geometry::LensModel;

use crate::crop::{self, Measured, Objective, Search};
use crate::keystone;
use crate::lens::{self, Evidence};
use crate::profiles::{CropRules, LensExif};
use crate::safety::{self, Limits};
use crate::straighten::{self, Horizon};

/// Which build's arithmetic produced the geometry.
///
/// Bump on any change to the objective, the estimator, the keystone measurement, the reduction
/// ladder or the fusion. It is written into `geometry_plan.analysis_ver` and every plan made
/// under an older one is re-decided.
pub const ANALYSIS_VER: u16 = 1;

/// The proxy this phase decides on, in pixels on the long edge.
///
/// Invariant 3. The *decision* is made here and the operators run at full resolution at export,
/// which is why section 11's resampling budget is written about a 45 MP frame and this pass's
/// budget is written about a 2048 px one.
pub const GEOMETRY_LEVEL: u32 = 2048;

/// Below this confidence a plan is worth somebody looking at.
///
/// Two thirds, matching the review thresholds phases 15, 20 and 22 use. A plan below it is
/// usually one where the lens was unknown and the subject was inferred from the frame's own
/// structure, which is exactly the combination a photographer should see before a gallery ships.
pub const REVIEW_BELOW: f32 = 0.66;

/// What one frame hands the analyser.
///
/// Everything is supplied rather than fetched. This crate opens no file and holds no service; the
/// pass in [`crate::api`] reads phase 02's proxies through the frozen `PreviewService` and phase
/// 06's faces through the frozen `PeopleService`, and hands the results here as values. Invariant
/// 1 as a property of a struct.
#[derive(Debug, Clone)]
pub struct GeometryFrame {
    /// The photograph.
    pub image_id: ImageId,
    /// The scene it was classified as. Invariant 7.
    pub scene: SceneId,
    /// The proxy, interleaved linear RGB.
    pub rgb: Vec<f32>,
    /// Proxy width in pixels.
    pub width: usize,
    /// Proxy height in pixels.
    pub height: usize,
    /// The width the frame's geometry is reasoned about at, in pixels.
    ///
    /// It is used for one thing - `rotation_crop` - and `rotation_crop` depends on the frame's
    /// aspect **ratio** and on nothing else. So the original's dimensions and the proxy's answer
    /// the same question, and `crate::api::GeometryPass` fills these from the proxy because that
    /// is what it has: a phase 02 buffer carries pixels and dimensions rather than EXIF.
    ///
    /// The field is named for what it is used for rather than for where it came from, because a
    /// field called `full_width` filled from a proxy would be a lie in a struct.
    pub full_width: u32,
    /// The height the frame's geometry is reasoned about at, in pixels.
    pub full_height: u32,
    /// What the file says about its lens.
    pub lens: LensExif,
    /// What phase 11 measured about the horizon.
    pub horizon: Horizon,
    /// What may not be cut.
    ///
    /// **The input port, and the only one.** Faces arrive from phase 06 and the moment's key
    /// content from phase 08; hands arrive from phase 11's keypoints, whose head is a placeholder,
    /// so on this build `ProtectedContent::Hands` and `JoinedHands` are never present. That is a
    /// gap in what is protected rather than a permission - the scenes where hands matter most are
    /// the scenes `crop_rules.toml` switches cropping off for, which is the mitigation and is
    /// written down as such.
    pub protected: Vec<ProtectedRegion>,
    /// What phase 11 says must stay in frame, when it said anything.
    pub hint: Option<Box2>,
    /// True when the photographer has already framed this photograph by hand.
    ///
    /// Checked here as well as in the store. A re-analysis that recomputed a hand-set crop and
    /// then discarded the answer at the last moment would still have spent the time, and would
    /// be one refactor away from writing it.
    pub user_edited: bool,
}

/// What the analyser noticed while planning, for the pass report.
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryOutcome {
    /// True when at least one pixel moves.
    pub acted: bool,
    /// True when the frame keeps the framing it was shot at.
    pub kept_original: bool,
    /// True when the scene had no row of its own in the table.
    pub unlisted_scene: bool,
    /// The lens id nothing could be found for, when there was one.
    pub lens_missing: Option<String>,
    /// True when the correction came from a row nobody measured.
    ///
    /// **True on every corrected frame this build produces.** See
    /// `assets/lens_profiles/ATTRIBUTION.md`.
    pub reference_profile: bool,
    /// How many crop candidates were refused, by code.
    pub refusals: Vec<GeometryCode>,
    /// How many aspect variants came back safe.
    pub variants: usize,
}

/// The analyser: the two tables, and the arithmetic over them.
#[derive(Debug)]
pub struct Analyser {
    rules: CropRules,
    lens_ver: u16,
}

impl Analyser {
    /// Load the shipped tables.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5112` when either table will not load.
    pub fn embedded() -> Result<Self, AuraError> {
        let rules = CropRules::embedded()?;
        let database = aura_render::geometry::database();
        if database.rows.is_empty() {
            return Err(crate::errors::profile_refused(
                "lens_profiles/profiles.toml",
                "lens",
                "did not parse, so no lens in this build can be corrected",
            ));
        }
        Ok(Self {
            rules,
            lens_ver: database.version,
        })
    }

    /// The two versions this build produces.
    #[must_use]
    pub fn versions(&self) -> (u16, u16) {
        (
            ANALYSIS_VER,
            crate::profiles::profile_ver(self.rules.version, self.lens_ver),
        )
    }

    /// The crop rules underneath, for the gate and the panel.
    #[must_use]
    pub const fn rules(&self) -> &CropRules {
        &self.rules
    }

    /// Decide one photograph's geometry.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5110` when the frame carries no pixels to decide from. A frame that decides
    /// *nothing* is not an error and never has been - it is the state at least seventy per cent
    /// of a wedding is expected to be in.
    #[allow(clippy::too_many_lines)]
    pub fn plan(&self, frame: &GeometryFrame) -> Result<(GeometryPlan, GeometryOutcome), AuraError> {
        if frame.width < 16 || frame.height < 16 || frame.rgb.len() < frame.width * frame.height * 3
        {
            return Err(crate::errors::geometry_failed(
                &frame.image_id.to_db(),
                "the proxy is too small to measure anything from",
            ));
        }

        let rule = self.rules.scene(frame.scene);
        let frame_aspect = if frame.full_height == 0 {
            frame.width as f32 / frame.height.max(1) as f32
        } else {
            frame.full_width as f32 / frame.full_height as f32
        };
        let limits = Limits {
            frame_aspect,
            margin: self.rules.bounds.safety_margin,
            min_long_edge: self.rules.bounds.min_long_edge,
        }
        .floored();

        let mut reasons: Vec<GeometryReason> = Vec::new();
        let mut outcome = GeometryOutcome {
            acted: false,
            kept_original: true,
            unlisted_scene: !self.rules.has_row(frame.scene),
            lens_missing: None,
            reference_profile: false,
            refusals: Vec::new(),
            variants: 0,
        };

        // A photographer's framing is unbeatable, for the twelfth phase running, and here it is
        // the strongest form of the rule in the product: a re-crop of a frame somebody framed by
        // hand throws away work that cannot be recovered from anything.
        if frame.user_edited {
            let mut plan = GeometryPlan::untouched(frame.image_id, frame.scene);
            plan.reasons = vec![GeometryReason::plain(GeometryCode::UserFramed, 0.0)];
            plan.user_edited = true;
            plan.confidence = 1.0;
            let (analysis, profile) = self.versions();
            plan.analysis_ver = analysis;
            plan.profile_ver = profile;
            return Ok((plan, outcome));
        }

        // --- the lens ------------------------------------------------------------------
        let evidence = Evidence::of_proxy(&frame.rgb, frame.width, frame.height);
        let matched = crate::profiles::resolve_lens(&frame.lens, aura_render::geometry::database());
        let estimated = if matched.source.is_available() {
            None
        } else {
            lens::estimate(&frame.rgb, frame.width, frame.height)
        };
        let lens_decision = lens::decide(&matched, evidence, estimated);
        if lens_decision.correction.source == LensSource::None && !frame.lens.name.trim().is_empty()
        {
            outcome.lens_missing = Some(frame.lens.name.clone());
        }
        outcome.reference_profile = lens_decision
            .model
            .is_some_and(|model: LensModel| !model.measured)
            && lens_decision.correction.source == LensSource::Database;
        for code in &lens_decision.codes {
            reasons.push(match lens::evidence_box(*code) {
                Some(area) => GeometryReason::at(*code, weight_of(*code), area),
                None => GeometryReason::plain(*code, weight_of(*code)),
            });
        }

        // --- the rotation --------------------------------------------------------------
        let rotation = straighten::solve(
            frame.horizon,
            frame.full_width.max(1),
            frame.full_height.max(1),
            &frame.protected,
            limits,
            self.rules.bounds.max_rotate_deg,
        );
        reasons.push(GeometryReason::plain(
            rotation.code,
            weight_of(rotation.code),
        ));

        // The regions move with the rotation, and everything after this point compares against
        // the projected ones. Projecting the regions rather than un-rotating the rectangle is
        // the direction that keeps `CropVariant::rect` in the space the recipe stores it in.
        let projected: Vec<ProtectedRegion> = frame
            .protected
            .iter()
            .map(|region| ProtectedRegion {
                area: straighten::project(
                    region.area,
                    rotation.bounds,
                    rotation.applied_deg,
                    frame_aspect,
                ),
                ..region.clone()
            })
            .collect();

        // --- the keystone --------------------------------------------------------------
        let luma = aura_render::spatial::luma_plane(&frame.rgb, frame.width, frame.height);
        let (gx, gy) = aura_render::spatial::sobel_planes(&luma, frame.width, frame.height);
        let verticals = keystone::measure(&gx, &gy, frame.width, frame.height, &projected);
        let correction = keystone::solve(verticals, frame_aspect, &projected, limits);
        reasons.push(GeometryReason::plain(
            correction.code,
            weight_of(correction.code),
        ));

        // Both costs, together. A rotation and a keystone each take a centred bite out of the
        // frame and the crop search runs inside what is left of both - which is why they are
        // intersected here rather than applied one after the other.
        let bounds = intersect(rotation.bounds, correction.bounds);

        // --- the crop ------------------------------------------------------------------
        let measured = Measured::of_proxy(&frame.rgb, frame.width, frame.height);
        let subject = subject_of(frame, &projected, &measured);
        let search = Search {
            objective: Objective {
                frame: &measured,
                subject,
                placement: rule.placement,
                headroom_target: rule.headroom,
            },
            protected: &projected,
            limits,
            rule: rule.clone(),
            bounds,
        };

        let original = CropVariant {
            aspect: AspectRatio::Original,
            rect: bounds,
            purpose: aura_core::contract::geometry::CropPurpose::Primary,
            score: crop::objective(&search.objective, bounds).total,
            safe: true,
        };

        // **A subject nobody identified is not a subject.** When neither phase 06 nor phase 11
        // named what the photograph is of, `subject_of` falls back on the frame's own energy
        // centroid - which is a measurement of where the detail is rather than of what the
        // photograph is about. Three of the objective's four terms still mean something over such
        // a frame and the placement term does not, so the search would be optimising a rectangle
        // against an artefact and comparing the result to `MIN_IMPROVEMENT` as though it meant
        // something.
        //
        // So the crop search does not run. Phase 19's rule - a phase that consumes another
        // phase's output owns no fallback for it - and phase 22's - a repair that cannot be
        // measured is not performed. The aspect *variants* are still generated, because a variant
        // is an option phase 29 may take rather than a decision about the delivery, and a centred
        // one over an unidentified subject is a reasonable option and a bad delivery.
        let subject_known = !projected.is_empty() || frame.hint.is_some();
        let (primary, crop_code) = if rule.crop && subject_known {
            let (best, refusals) = crop::search(&search, AspectRatio::Original);
            for code in refusals {
                if !outcome.refusals.contains(&code) {
                    outcome.refusals.push(code);
                }
            }
            match best {
                // The improvement margin. Section 6.3: "a proposed crop must improve the
                // composition score by a minimum margin, otherwise the original framing wins".
                // The scene's own margin, which is at or above the contract's.
                Some(candidate)
                    if candidate.score.total >= original.score + rule.min_improvement =>
                {
                    (
                        Some(crop::into_variant(&candidate, AspectRatio::Original, true)),
                        GeometryCode::CropProposed,
                    )
                }
                Some(_) => (None, GeometryCode::CropNoImprovement),
                None => (None, GeometryCode::CropKeptOriginal),
            }
        } else {
            (None, GeometryCode::CropKeptOriginal)
        };
        reasons.push(GeometryReason::at(
            crop_code,
            weight_of(crop_code),
            primary.map_or(bounds, |variant| variant.rect),
        ));
        for code in &outcome.refusals {
            reasons.push(GeometryReason::plain(*code, weight_of(*code)));
        }

        // --- the variants --------------------------------------------------------------
        let attempts = crate::variants::generate(&search, &self.rules.variants);
        outcome.variants = attempts.iter().filter(|a| a.variant.safe).count();
        for attempt in &attempts {
            if let Some(code) = attempt.refusal {
                if !outcome.refusals.contains(&code) {
                    outcome.refusals.push(code);
                }
                reasons.push(GeometryReason::at(
                    GeometryCode::VariantUnsafe,
                    weight_of(GeometryCode::VariantUnsafe),
                    attempt.variant.rect,
                ));
            }
        }

        let delivered = primary.or({
            // No proposal. The frame is delivered at whatever the rotation and the keystone left,
            // which is the whole frame on a photograph neither of them touched.
            if bounds.w >= 1.0 - 1e-6 && bounds.h >= 1.0 - 1e-6 {
                None
            } else {
                Some(original)
            }
        });
        let (crops, primary_index) =
            crate::variants::assemble(original.score, delivered, &attempts);

        // --- the report ----------------------------------------------------------------
        let delivered_rect = crops
            .get(primary_index)
            .map_or(Box2::FULL, |variant| variant.rect);
        let checked = safety::check(delivered_rect, &projected, limits);
        let safety_report = if projected.is_empty() {
            CropSafetyReport::nothing_protected(checked.report.long_edge_fraction)
        } else {
            checked.report
        };

        outcome.acted = !lens_decision.correction.is_identity()
            || rotation.is_applied()
            || correction.keystone.is_some()
            || crops
                .get(primary_index)
                .is_some_and(|variant| !variant.is_full_frame());
        outcome.kept_original = crops
            .get(primary_index)
            .is_some_and(CropVariant::is_full_frame);

        reasons.sort_by(|left, right| left.weight.total_cmp(&right.weight));
        reasons.dedup_by(|left, right| left.code == right.code);
        reasons.truncate(MAX_REASONS);

        let confidence = confidence_of(
            &lens_decision.correction,
            rotation.code,
            frame.horizon,
            &projected,
            frame.hint.is_some(),
            rule.crop,
        );
        let (analysis_ver, profile_ver) = self.versions();

        Ok((
            GeometryPlan {
                image_id: frame.image_id,
                scene: frame.scene,
                lens: lens_decision.correction,
                rotate_deg: rotation.applied_deg,
                rotate_conf: frame.horizon.confidence.clamp(0.0, 1.0),
                keystone: correction.keystone.map(Keystone::clamped),
                crops,
                primary_crop: primary_index,
                safety: safety_report,
                reasons,
                confidence,
                user_edited: false,
                reviewed: false,
                analysis_ver,
                profile_ver,
            },
            outcome,
        ))
    }
}

/// What the photograph is of, in three preferences.
///
/// The protected regions first, because a face is the most reliable statement anybody has made
/// about what matters in a frame; phase 11's crop hint second, because it is a judgement about
/// the same question made from the whole frame; and the energy centroid last, because it is a
/// measurement of where the detail is rather than of what the photograph is about.
///
/// The three are not interchangeable and the fallback is what the confidence is lowered for.
fn subject_of(frame: &GeometryFrame, protected: &[ProtectedRegion], measured: &Measured) -> Box2 {
    if let Some(hull) = safety::hull(protected, 0.0) {
        return hull;
    }
    if let Some(hint) = frame.hint {
        if !hint.is_empty() {
            return hint;
        }
    }
    let (cx, cy) = measured.centroid();
    // A box rather than a point, because the headroom term measures from a subject's *top* and a
    // point has none. A third of the frame, which is about what a person occupies in a
    // half-length portrait.
    Box2 {
        x: cx - 1.0 / 6.0,
        y: cy - 1.0 / 6.0,
        w: 1.0 / 3.0,
        h: 1.0 / 3.0,
    }
    .clamped()
}

/// The intersection of two rectangles, or the smaller when they do not meet.
fn intersect(left: Box2, right: Box2) -> Box2 {
    let x0 = left.x.max(right.x);
    let y0 = left.y.max(right.y);
    let x1 = (left.x + left.w).min(right.x + right.w);
    let y1 = (left.y + left.h).min(right.y + right.h);
    if x1 <= x0 || y1 <= y0 {
        return if left.w * left.h <= right.w * right.h {
            left
        } else {
            right
        };
    }
    Box2 {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// How much this phase knew about the photograph when it decided.
///
/// A **geometric** mean of three independent ignorances, so one of them being total is a plan
/// nobody should act on unattended however sure the other two were. That is the same fusion phase
/// 09, phase 12 and phase 18 use, and it is the right shape here for the same reason: the three
/// terms describe different things and a sum would let a known lens pay for an unknown subject.
fn confidence_of(
    lens: &LensCorrection,
    rotation: GeometryCode,
    horizon: Horizon,
    protected: &[ProtectedRegion],
    has_hint: bool,
    crop_allowed: bool,
) -> f32 {
    let lens_known = match lens.source {
        LensSource::Embedded => 1.00,
        LensSource::Database => 0.85,
        LensSource::Estimated => 0.55,
        LensSource::None => 0.45,
    };
    let horizon_known = match rotation {
        // A rotation is as sure as the horizon behind it.
        GeometryCode::Straightened | GeometryCode::RotationReduced => {
            horizon.confidence.clamp(0.0, 1.0)
        }
        // A decision, taken with the measurement in hand.
        GeometryCode::TiltNegligible | GeometryCode::TiltIntentional | GeometryCode::TiltTooLarge => {
            0.90
        }
        // A refusal that knew what it was refusing.
        GeometryCode::RotationRefused => 0.75,
        // An admission. `HorizonUnsure` is the lowest of the three because the frame *does* look
        // off level and this build declined to say by how much.
        GeometryCode::HorizonUnsure => 0.55,
        _ => 0.60,
    };
    let subject_known = if !protected.is_empty() {
        1.00
    } else if has_hint {
        0.80
    } else if crop_allowed {
        // The energy centroid, on a scene where a crop was permitted. The weakest position this
        // phase can be in and the one the review queue exists for.
        0.45
    } else {
        // No subject and no crop either, so nothing was decided from the gap.
        0.90
    };
    (lens_known * horizon_known * subject_known)
        .max(0.0)
        .powf(1.0 / 3.0)
        .clamp(0.0, 1.0)
}

/// How much a code moved the plan, `-1..1`.
///
/// Negative when the code cost the frame something and positive when it earned it, which is what
/// lets [`GeometryPlan::reasons`] be ranked without a second field saying which way each points.
/// The magnitudes are a *display order* rather than a model: nothing multiplies by them.
#[must_use]
pub fn weight_of(code: GeometryCode) -> f32 {
    match code {
        // The safety refusals, worst first. These are the reasons a photographer most needs at
        // the top of a panel, because they are the ones that explain an absence.
        GeometryCode::CropCutsFace => -0.90,
        GeometryCode::CropCutsHands => -0.85,
        GeometryCode::CropDropsMomentKey => -0.80,
        GeometryCode::CropBelowResolution => -0.60,
        GeometryCode::VariantUnsafe => -0.55,
        GeometryCode::RotationRefused => -0.50,
        GeometryCode::KeystoneRefused => -0.45,
        GeometryCode::RotationReduced => -0.35,
        GeometryCode::KeystoneStretchCapped => -0.30,
        GeometryCode::LensVignetteReduced => -0.25,
        GeometryCode::LensFocalOutOfRange => -0.22,
        GeometryCode::LensProfileMissing => -0.20,
        GeometryCode::LensCaUnverifiable => -0.15,
        GeometryCode::HorizonUnsure => -0.12,
        GeometryCode::CropNoImprovement => -0.05,
        // The neutral observations. Zero rather than a small negative: a level horizon is not a
        // fault and neither is a photograph nobody needed to change.
        GeometryCode::TiltNegligible
        | GeometryCode::TiltIntentional
        | GeometryCode::TiltTooLarge
        | GeometryCode::HorizonAbsent
        | GeometryCode::KeystoneNotNeeded
        | GeometryCode::KeystoneNoArchitecture
        | GeometryCode::CropKeptOriginal
        | GeometryCode::UserFramed => 0.0,
        // The actions.
        GeometryCode::LensEstimated => 0.20,
        GeometryCode::LensCaCorrected => 0.30,
        GeometryCode::LensProfileMatched => 0.40,
        GeometryCode::LensEmbedded => 0.50,
        GeometryCode::KeystoneApplied => 0.60,
        GeometryCode::Straightened => 0.70,
        GeometryCode::CropProposed => 0.80,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::geometry::ProtectedContent;

    use crate::fixtures;

    fn analyser() -> Analyser {
        Analyser::embedded().expect("the shipped tables must load")
    }

    #[test]
    fn a_plan_always_carries_at_least_one_reason_and_never_more_than_the_bound() {
        let analyser = analyser();
        for scene in [SceneId::Candid, SceneId::Ceremony, SceneId::Details] {
            let frame = fixtures::plain_frame(scene);
            let (plan, _) = analyser.plan(&frame).expect("a plan");
            assert!(!plan.reasons.is_empty(), "{scene:?}");
            assert!(plan.reasons.len() <= MAX_REASONS, "{scene:?}");
        }
    }

    #[test]
    fn a_photographers_framing_is_never_recomputed() {
        let analyser = analyser();
        let mut frame = fixtures::plain_frame(SceneId::Candid);
        frame.user_edited = true;
        let (plan, _) = analyser.plan(&frame).expect("a plan");
        assert!(plan.user_edited);
        assert!(plan.has(GeometryCode::UserFramed));
        assert!(plan.is_identity());
        assert!((plan.confidence - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_scene_that_forbids_cropping_never_gets_one() {
        let analyser = analyser();
        for scene in [
            SceneId::Ceremony,
            SceneId::FamilyPortrait,
            SceneId::FirstDance,
            SceneId::Unknown,
        ] {
            let frame = fixtures::lopsided_frame(scene);
            let (plan, outcome) = analyser.plan(&frame).expect("a plan");
            assert!(
                plan.crops
                    .first()
                    .is_some_and(CropVariant::is_full_frame),
                "{scene:?} was cropped"
            );
            assert!(outcome.kept_original, "{scene:?}");
            assert!(plan.has(GeometryCode::CropKeptOriginal), "{scene:?}");
        }
    }

    #[test]
    fn the_delivered_crop_never_cuts_a_face() {
        // Section 10.1's hard gate at the level of one plan. The pass-level version is in
        // `tests/eval/geometry_eval.rs` and runs over a synthetic wedding.
        let analyser = analyser();
        let frame = fixtures::crowded_frame(SceneId::DanceFloor);
        let (plan, _) = analyser.plan(&frame).expect("a plan");
        let delivered = plan.primary();
        for region in &frame.protected {
            let projected =
                straighten::project(region.area, delivered, plan.rotate_deg, 1.5);
            assert!(
                safety::rect_inside(projected, delivered, 0.01),
                "the delivered crop cuts {:?}",
                region.kind
            );
        }
        assert!(plan.safety.faces_intact);
        assert_eq!(plan.safety.considered as usize, frame.protected.len());
    }

    #[test]
    fn a_frame_with_nothing_to_protect_says_so_rather_than_claiming_it_is_safe() {
        let analyser = analyser();
        let frame = fixtures::plain_frame(SceneId::Venue);
        let (plan, _) = analyser.plan(&frame).expect("a plan");
        assert_eq!(plan.safety.considered, 0);
        assert!(plan.safety.is_safe());
    }

    #[test]
    fn the_versions_are_two_and_neither_is_zero() {
        let (analysis, profile) = analyser().versions();
        assert_eq!(analysis, ANALYSIS_VER);
        assert!(profile > 0);
    }

    #[test]
    fn a_known_lens_is_more_confident_than_an_unknown_one() {
        let analyser = analyser();
        let mut known = fixtures::plain_frame(SceneId::Candid);
        known.lens = LensExif {
            name: "EF24-70mm f/2.8L II USM".into(),
            focal_mm: Some(35.0),
            embedded: false,
        };
        let mut unknown = fixtures::plain_frame(SceneId::Candid);
        unknown.lens = LensExif::default();

        let (with, _) = analyser.plan(&known).expect("a plan");
        let (without, _) = analyser.plan(&unknown).expect("a plan");
        assert!(
            with.confidence > without.confidence,
            "{} !> {}",
            with.confidence,
            without.confidence
        );
    }

    #[test]
    fn every_code_has_a_weight_and_the_refusals_are_negative() {
        for code in GeometryCode::ALL {
            let weight = weight_of(code);
            assert!((-1.0..=1.0).contains(&weight), "{code}");
            if code.is_safety_refusal() {
                assert!(weight < 0.0, "{code} is a safety refusal with weight {weight}");
            }
        }
    }

    #[test]
    fn planning_the_same_frame_twice_produces_the_same_plan() {
        // Invariant 4. The search is a grid with no restart in it and the tables are compiled in,
        // so this is a property rather than a hope - and it is the property a byte-identical
        // recipe depends on.
        let analyser = analyser();
        let frame = fixtures::lopsided_frame(SceneId::Candid);
        let (first, _) = analyser.plan(&frame).expect("a plan");
        let (second, _) = analyser.plan(&frame).expect("a plan");
        assert_eq!(first, second);
    }

    #[test]
    fn a_frame_too_small_to_measure_is_a_failure_rather_than_an_empty_plan() {
        let analyser = analyser();
        let mut frame = fixtures::plain_frame(SceneId::Candid);
        frame.width = 4;
        frame.height = 4;
        frame.rgb.truncate(4 * 4 * 3);
        let err = analyser.plan(&frame).unwrap_err();
        assert_eq!(err.code.0, "AURA-ML-5110");
    }

    #[test]
    fn a_protected_hand_is_absent_on_this_build_and_the_fixture_says_so() {
        // The condition, as a test rather than as a sentence: nothing in the pipeline fills
        // `ProtectedContent::Hands`, so a fixture that carries one is carrying it by hand. If a
        // later phase starts filling them this test is what will need changing, which is the
        // point of writing it.
        let frame = fixtures::crowded_frame(SceneId::DanceFloor);
        assert!(frame
            .protected
            .iter()
            .all(|region| region.kind != ProtectedContent::Hands
                && region.kind != ProtectedContent::JoinedHands));
    }
}
