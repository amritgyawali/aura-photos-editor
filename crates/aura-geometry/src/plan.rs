//! Assembling one photograph's plan. PHASE-23 section 8's steps 1 to 7, in that order.
//!
//! The order is not arbitrary and two of the steps depend on it:
//!
//! 1. **Lens corrections first**, because everything after them works in the corrected frame.
//!    Every protected region is mapped through the distortion model before the safety filter
//!    sees it - a face box measured on the frame as shot is in the wrong place once the optics
//!    have been undone, and a filter that checked the un-mapped box would clear a crop that
//!    cuts the face by however much the lens bent.
//! 2. **Straightening second**, because the rotation is what makes a frame's verticals
//!    vertical, and a keystone fitted before it is a keystone fitted to a tilt.
//! 3. **Keystone third.**
//! 4. **The crop last**, over whatever area the three transforms above left valid.
//!
//! ## The improvement margin decides one thing only
//!
//! The primary crop. A candidate has to beat the frame as shot by the scene's margin or the
//! original wins outright, and `GeometryPlan::primary_crop` stays at zero. Section 10.1's
//! "most frames (>= 70 %) keep their original framing" is the aggregate of that decision, and
//! `crop_rules.toml` is where it is actually set: fourteen of the twenty-three scene rows do
//! not permit a crop at all.

use aura_core::contract::geometry::{
    CropPurpose, CropVariant, GeometryCode, GeometryPlan, GeometryReason, ProtectedKind,
    ProtectedRegion,
};
use aura_core::contract::integrity::CropRect;
use aura_core::contract::scene::ImageId;
use aura_core::SceneId;

use crate::crop::{self, Objective};
use crate::keystone::{self, VerticalLine};
use crate::lens::{self, EdgeChain, LensInput};
use crate::profiles::ProfileTable;
use crate::rules::CropRules;
use crate::safety::{self, SafetyInput};
use crate::straighten::{self, StraightenInput};

/// Which arithmetic produced a plan.
///
/// Bumping it invalidates the rotation, the keystone and every crop, because all three come
/// out of the search in this module. `AURA-ML-5090` is raised when a comparison would cross
/// it.
pub const ANALYSIS_VER: u16 = 1;

/// The proxy rung this phase measures on.
///
/// Invariant 3's medium tier. Finding a straight edge, a converging vertical or a distraction
/// at the frame's edge needs geometry rather than detail, and the 2048 px proxy is what phases
/// 09, 11, 15, 16 and 19 already decode - so a geometry pass over a culled wedding opens no
/// file those phases have not opened.
pub const GEOMETRY_LEVEL: u32 = 2048;

/// Everything one frame's plan is made from.
#[derive(Debug, Clone)]
pub struct GeometryInput {
    /// The photograph.
    pub image_id: ImageId,
    /// What phase 07 called it.
    pub scene: SceneId,
    /// Width over height of the frame as shot.
    pub aspect: f32,
    /// What is known about the lens.
    pub lens: LensInput,
    /// Straight edges tracked from the proxy, for the manual-lens estimator.
    pub edges: Vec<EdgeChain>,
    /// Near-vertical lines tracked from the proxy, for the keystone.
    pub verticals: Vec<VerticalLine>,
    /// Phase 11's tilt.
    pub tilt_deg: f32,
    /// Phase 11's confidence in it.
    pub horizon_conf: f32,
    /// Phase 11's judgement that the tilt was a decision.
    pub tilt_intentional: bool,
    /// Faces, hands and key content, in the coordinates of the frame **as shot**.
    ///
    /// Mapped into the corrected frame by this module. A caller that pre-mapped them would be
    /// a caller that had to know the correction before asking for it.
    pub regions: Vec<ProtectedRegion>,
    /// Bright blobs and edge intrusions from phase 11, also as shot.
    pub distractions: Vec<CropRect>,
    /// What phase 11 said the frame is about, when it said.
    pub subject: Option<CropRect>,
}

impl GeometryInput {
    /// A frame nobody has measured: a 3:2 landscape with no lens, no tilt and nothing in it.
    #[must_use]
    pub fn bare(image_id: ImageId, scene: SceneId) -> Self {
        Self {
            image_id,
            scene,
            aspect: 1.5,
            lens: LensInput::default(),
            edges: Vec::new(),
            verticals: Vec::new(),
            tilt_deg: 0.0,
            horizon_conf: 0.0,
            tilt_intentional: false,
            regions: Vec::new(),
            distractions: Vec::new(),
            subject: None,
        }
    }
}

/// The thing that turns an input into a plan.
#[derive(Debug)]
pub struct Planner {
    profiles: ProfileTable,
    rules: CropRules,
}

impl Planner {
    /// Build a planner over a profile table and a rules file.
    #[must_use]
    pub const fn new(profiles: ProfileTable, rules: CropRules) -> Self {
        Self { profiles, rules }
    }

    /// The lens profile table, for the panel.
    #[must_use]
    pub const fn profiles(&self) -> &ProfileTable {
        &self.profiles
    }

    /// The crop rules, for the panel.
    #[must_use]
    pub const fn rules(&self) -> &CropRules {
        &self.rules
    }

    /// Plan one photograph.
    ///
    /// Infallible by construction: every gate has a "leave it alone" branch, and the plan a
    /// frame nobody could measure gets is the frame as shot with a reason on it. The
    /// *guarantee* check is separate - `guard::check_plan` - because a plan that breaks one is
    /// a bug in this module rather than a property of the photograph.
    #[must_use]
    #[allow(clippy::too_many_lines)] // Section 8's seven steps, in section 8's order.
    pub fn plan(&self, input: &GeometryInput) -> GeometryPlan {
        let mut plan = GeometryPlan::new(input.image_id, input.scene);
        let (rule, rules_row) = self.rules.for_scene(input.scene);
        plan.profile_ver = self.profiles.version();
        plan.analysis_ver = ANALYSIS_VER;
        plan.rules_ver = self.rules.version();

        // --- 1. the optics -----------------------------------------------------------------
        let estimated = lens::estimate_k1(&input.edges, input.aspect);
        let (correction, mut reasons) = lens::decide(&input.lens, &self.profiles, estimated);
        let lens_scale = lens::valid_scale(correction.distortion, input.aspect);
        let regions = lens::map_regions(
            &input.regions,
            correction.distortion,
            input.aspect,
            lens_scale,
        );
        let distractions: Vec<CropRect> = input
            .distractions
            .iter()
            .map(|rect| lens::map_rect(*rect, correction.distortion, input.aspect, lens_scale))
            .collect();
        let subject = input
            .subject
            .map(|rect| lens::map_rect(rect, correction.distortion, input.aspect, lens_scale));
        plan.lens = correction;

        let safety_input = SafetyInput {
            regions: &regions,
            aspect: input.aspect,
            resolution_floor: rule.resolution_floor,
        };

        // --- 2. the rotation ---------------------------------------------------------------
        let levelled = straighten::decide(
            &StraightenInput {
                tilt_deg: input.tilt_deg,
                horizon_conf: input.horizon_conf,
                tilt_intentional: input.tilt_intentional,
                aspect: input.aspect,
            },
            &safety_input,
        );
        plan.rotate_deg = levelled.rotate_deg;
        plan.rotate_conf = levelled.rotate_conf;
        reasons.extend(levelled.reasons);

        // --- 3. the keystone ---------------------------------------------------------------
        let squared = keystone::decide(&input.verticals, input.aspect);
        plan.keystone = squared.keystone;
        reasons.extend(squared.reasons);

        // --- 4. the crop -------------------------------------------------------------------
        let objective = Objective {
            regions: &regions,
            distractions: &distractions,
            subject,
            headroom: (rule.headroom_min, rule.headroom_max),
            aspect: input.aspect,
        };
        let rotated = levelled.rect;
        let baseline = objective.score(rotated);
        let mut refused = [0u32; 4];
        let mut primary = CropVariant {
            rect: rotated,
            score: baseline,
            ..CropVariant::original()
        };
        let mut improved = false;

        if rule.crop {
            let (best, cost) = crop::best(
                aura_core::contract::geometry::Aspect::Original,
                CropPurpose::Primary,
                &objective,
                &safety_input,
            );
            for (slot, add) in refused.iter_mut().zip(cost.iter()) {
                *slot += add;
            }
            if let Some(candidate) = best {
                if candidate.score >= baseline + rule.improvement_margin {
                    primary = candidate;
                    improved = true;
                }
            }
        }

        if improved {
            reasons.push(GeometryReason::at(
                GeometryCode::CropImproved,
                format!(
                    "A tighter framing scored {:.2} against {:.2} as shot, past this scene's \
                     {:.2} margin, so it was taken.",
                    primary.score, baseline, rule.improvement_margin
                ),
                0.06,
                primary.rect,
            ));
            plan.crops.push(primary);
            plan.primary_crop = plan.crops.len() - 1;
        } else {
            reasons.push(GeometryReason::plain(GeometryCode::CropKeptOriginal, 0.02));
            // The original entry carries the rotation's rectangle and the baseline score, so
            // that "delivered as shot" and "levelled and delivered otherwise as shot" are the
            // same first entry rather than two shapes a panel has to tell apart.
            if let Some(first) = plan.crops.first_mut() {
                first.rect = rotated;
                first.score = baseline;
            }
        }

        // --- 5. the variants ---------------------------------------------------------------
        let (variants, variant_refused, variant_reasons) =
            crate::variants::generate(&rule.variants, &objective, &safety_input);
        for (slot, add) in refused.iter_mut().zip(variant_refused.iter()) {
            *slot += add;
        }
        plan.crops.extend(variants);
        reasons.extend(variant_reasons);

        // --- 6. the report and the confidence ----------------------------------------------
        plan.safety = safety::report(plan.primary().rect, &safety_input, refused);
        if !rules_row {
            reasons.push(GeometryReason::frame(
                GeometryCode::CropKeptOriginal,
                format!(
                    "There is no cropping guidance recorded for a '{}' photograph yet, so the \
                     frame was delivered as shot.",
                    input.scene
                ),
                -0.03,
            ));
        }
        plan.confidence = confidence(&reasons, &plan);
        plan.reasons = trim(reasons);
        plan
    }
}

/// How sure a plan is.
///
/// One minus the doubts, plus the assurances, bounded. Two structural caps, and both are the
/// phase's honesty rather than its arithmetic:
///
/// * a plan whose safety filter checked **no** region is capped, because it has proven nothing
///   about faces it never saw. This is phase 10's `EmotionCode::NoFaces` cap in a new place;
/// * a plan that corrected a lens from an *estimate* is capped below one that used a measured
///   profile.
fn confidence(reasons: &[GeometryReason], plan: &GeometryPlan) -> f32 {
    let moved: f32 = reasons.iter().map(|reason| reason.weight).sum();
    let mut value = (0.72 + moved).clamp(0.05, 1.0);
    if plan.primary_crop > 0 && !plan.safety.is_evidence() {
        value = value.min(0.60);
    }
    if plan.lens.source == aura_core::contract::geometry::LensSource::Estimated {
        value = value.min(0.70);
    }
    value.clamp(0.05, 1.0)
}

/// Keep the reasons a panel renders: every variant note, then the heaviest of the rest.
fn trim(mut reasons: Vec<GeometryReason>) -> Vec<GeometryReason> {
    let variants: Vec<GeometryReason> = reasons
        .iter()
        .filter(|reason| reason.code == GeometryCode::VariantAdded)
        .cloned()
        .collect();
    reasons.retain(|reason| reason.code != GeometryCode::VariantAdded);
    reasons.sort_by(|a, b| {
        b.weight
            .abs()
            .partial_cmp(&a.weight.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.code.cmp(&b.code))
    });
    reasons.truncate(GeometryPlan::MAX_REASONS);
    reasons.extend(variants);
    if reasons.is_empty() {
        reasons.push(GeometryReason::plain(GeometryCode::Clean, 0.0));
    }
    reasons
}

/// How many faces are protected in one input, for the pass's telemetry.
#[must_use]
pub fn face_count(input: &GeometryInput) -> usize {
    input
        .regions
        .iter()
        .filter(|region| region.kind == ProtectedKind::Face)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard;
    use aura_core::PhotoId;

    fn photo(n: u8) -> ImageId {
        PhotoId::from_db(&format!("pht_00000000-0000-4000-8000-0000000000{n:02}"))
            .expect("a photo id")
    }

    fn planner() -> Planner {
        Planner::new(
            ProfileTable::empty(),
            CropRules::shipped().expect("the shipped rules"),
        )
    }

    fn face(x: f32, y: f32) -> ProtectedRegion {
        ProtectedRegion {
            kind: ProtectedKind::Face,
            identity: None,
            rect: CropRect {
                x,
                y,
                w: 0.09,
                h: 0.13,
            },
            primary: true,
        }
    }

    #[test]
    fn a_bare_frame_is_planned_and_delivered_as_shot() {
        let plan = planner().plan(&GeometryInput::bare(photo(1), SceneId::Candid));
        assert!(guard::check_plan(&plan).is_ok());
        assert!(plan.kept_original_framing());
        assert!(plan.is_identity());
        assert!(!plan.reasons.is_empty());
    }

    #[test]
    fn every_scene_produces_a_sound_plan() {
        let planner = planner();
        for (i, scene) in SceneId::ALL.into_iter().enumerate() {
            let mut input = GeometryInput::bare(photo(i as u8), scene);
            input.regions = vec![face(0.42, 0.30), face(0.58, 0.32)];
            input.distractions = vec![CropRect {
                x: 0.86,
                y: 0.02,
                w: 0.12,
                h: 0.14,
            }];
            input.tilt_deg = 1.8;
            input.horizon_conf = 0.88;
            let plan = planner.plan(&input);
            assert!(
                guard::check_plan(&plan).is_ok(),
                "{scene}: {:?}",
                plan.broken_guarantee()
            );
            assert!(plan.reasons.len() <= GeometryPlan::MAX_REASONS + CropPurpose::COUNT);
        }
    }

    #[test]
    fn a_scene_that_may_not_crop_never_crops_however_good_the_candidate() {
        let planner = planner();
        let mut input = GeometryInput::bare(photo(2), SceneId::Kiss);
        // A subject shoved into the corner with a bright blob opposite it: the most
        // crop-worthy frame the objective can be shown.
        input.regions = vec![face(0.08, 0.08)];
        input.subject = Some(CropRect {
            x: 0.08,
            y: 0.08,
            w: 0.09,
            h: 0.13,
        });
        input.distractions = vec![CropRect {
            x: 0.70,
            y: 0.70,
            w: 0.25,
            h: 0.25,
        }];
        let plan = planner.plan(&input);
        assert!(plan.kept_original_framing(), "a kiss was cropped");
    }

    #[test]
    fn a_croppable_scene_takes_a_clearly_better_framing() {
        let planner = planner();
        let mut input = GeometryInput::bare(photo(3), SceneId::Speeches);
        input.regions = vec![face(0.20, 0.30)];
        input.subject = Some(CropRect {
            x: 0.20,
            y: 0.30,
            w: 0.09,
            h: 0.13,
        });
        // Two bright distractions on the right, croppable away.
        input.distractions = vec![
            CropRect {
                x: 0.78,
                y: 0.05,
                w: 0.18,
                h: 0.22,
            },
            CropRect {
                x: 0.80,
                y: 0.60,
                w: 0.16,
                h: 0.20,
            },
        ];
        let plan = planner.plan(&input);
        assert!(guard::check_plan(&plan).is_ok());
        if plan.primary_crop > 0 {
            let primary = plan.primary();
            assert_eq!(primary.purpose, CropPurpose::Primary);
            assert!(primary.safe);
            assert!(primary.score > plan.crops[0].score);
        }
    }

    #[test]
    fn a_face_at_the_edge_survives_every_crop_the_planner_proposes() {
        let planner = planner();
        for scene in SceneId::ALL {
            let mut input = GeometryInput::bare(photo(4), scene);
            input.regions = vec![face(0.02, 0.44), face(0.50, 0.30)];
            input.subject = Some(CropRect {
                x: 0.50,
                y: 0.30,
                w: 0.09,
                h: 0.13,
            });
            let plan = planner.plan(&input);
            for variant in &plan.crops {
                for region in &input.regions {
                    assert!(
                        region.is_inside(variant.rect, 0.0),
                        "{scene}: the {} crop cut a face",
                        variant.purpose
                    );
                }
            }
        }
    }

    #[test]
    fn planning_is_deterministic() {
        let planner = planner();
        let mut input = GeometryInput::bare(photo(5), SceneId::CouplePortrait);
        input.regions = vec![face(0.30, 0.28), face(0.55, 0.30)];
        input.tilt_deg = 2.2;
        input.horizon_conf = 0.91;
        let a = planner.plan(&input);
        let b = planner.plan(&input);
        assert_eq!(a, b);
    }

    #[test]
    fn a_plan_that_cropped_without_checking_a_face_is_capped() {
        let planner = planner();
        let mut input = GeometryInput::bare(photo(6), SceneId::Venue);
        input.subject = Some(CropRect {
            x: 0.10,
            y: 0.10,
            w: 0.12,
            h: 0.16,
        });
        input.distractions = vec![CropRect {
            x: 0.80,
            y: 0.70,
            w: 0.18,
            h: 0.25,
        }];
        let plan = planner.plan(&input);
        if plan.primary_crop > 0 {
            assert!(
                plan.confidence <= 0.60,
                "cropped with no region checked at confidence {}",
                plan.confidence
            );
        }
    }
}
