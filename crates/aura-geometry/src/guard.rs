//! Turning the contract's predicates into this phase's errors.
//!
//! `aura-core` owns the shapes and the predicates; `aura-geometry` owns the error registry.
//! The split is the one every phase since 09 has kept, and it is what lets
//! [`aura_core::contract::geometry::GeometryPlan::broken_guarantee`] be asked by the planner,
//! the store, the IPC layer and the evaluation harness without any of them being able to
//! disagree about what a sound plan is.

use aura_core::contract::error::AuraError;
use aura_core::contract::geometry::{GeometryOverride, GeometryPlan};

use crate::errors;

/// Refuse a plan that breaks one of this phase's own guarantees.
///
/// **A refused plan is stored as no plan rather than as a weak one.** Four of the six clauses
/// are the crop safety filter restated as a post-condition, and a plan that fails one of those
/// is a delivered photograph with somebody's hands cropped off it.
///
/// # Errors
///
/// `AURA-ML-5092` naming the photograph and the clause.
pub fn check_plan(plan: &GeometryPlan) -> Result<(), AuraError> {
    match plan.broken_guarantee() {
        None => Ok(()),
        Some(problem) => Err(errors::plan_failed(&plan.image_id.to_db(), problem)),
    }
}

/// Refuse an override that cannot be applied.
///
/// # Errors
///
/// `AURA-ML-5091` naming the problem.
pub fn check_override(over: &GeometryOverride) -> Result<(), AuraError> {
    match over.problem() {
        None => Ok(()),
        Some(problem) => Err(errors::framing_refused(problem)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::geometry::{CropPurpose, CropVariant};
    use aura_core::contract::integrity::CropRect;
    use aura_core::{PhotoId, SceneId};

    fn photo() -> PhotoId {
        PhotoId::from_db("pht_00000000-0000-4000-8000-000000000023").expect("a photo id")
    }

    fn plan() -> GeometryPlan {
        GeometryPlan::new(photo(), SceneId::CouplePortrait)
    }

    #[test]
    fn a_fresh_plan_is_sound() {
        assert!(check_plan(&plan()).is_ok());
    }

    #[test]
    fn a_plan_whose_first_crop_is_not_the_original_is_refused() {
        let mut plan = plan();
        plan.crops = vec![CropVariant {
            aspect: aura_core::contract::geometry::Aspect::Square,
            rect: CropRect {
                x: 0.1,
                y: 0.1,
                w: 0.6,
                h: 0.6,
            },
            purpose: CropPurpose::Social,
            score: 0.7,
            safe: true,
        }];
        let err = check_plan(&plan).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5092");
    }

    #[test]
    fn a_plan_keeping_an_unsafe_crop_is_refused() {
        let mut plan = plan();
        plan.crops.push(CropVariant {
            aspect: aura_core::contract::geometry::Aspect::Square,
            rect: CropRect {
                x: 0.1,
                y: 0.1,
                w: 0.6,
                h: 0.6,
            },
            purpose: CropPurpose::Social,
            score: 0.9,
            safe: false,
        });
        assert!(check_plan(&plan).is_err());
    }

    #[test]
    fn a_plan_rotated_below_the_confidence_gate_is_refused() {
        let mut plan = plan();
        plan.rotate_deg = 2.0;
        plan.rotate_conf = 0.61;
        assert!(check_plan(&plan).is_err());
        plan.rotate_conf = 0.90;
        assert!(check_plan(&plan).is_ok());
    }

    #[test]
    fn a_plan_rotated_outside_the_band_is_refused() {
        let mut plan = plan();
        plan.rotate_conf = 0.95;
        plan.rotate_deg = 12.0;
        assert!(check_plan(&plan).is_err());
        plan.rotate_deg = 0.05;
        assert!(check_plan(&plan).is_err());
    }

    #[test]
    fn a_plan_whose_primary_index_addresses_nothing_is_refused() {
        let mut plan = plan();
        plan.primary_crop = 4;
        assert!(check_plan(&plan).is_err());
    }

    #[test]
    fn a_plan_with_no_reason_is_refused() {
        let mut plan = plan();
        plan.reasons.clear();
        assert!(check_plan(&plan).is_err());
    }

    #[test]
    fn a_degenerate_override_and_an_absurd_angle_are_both_refused() {
        let mut over = GeometryOverride::revert(photo());
        over.rect = CropRect {
            x: 0.5,
            y: 0.5,
            w: 0.0,
            h: 0.2,
        };
        let err = check_override(&over).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5091");

        let mut angled = GeometryOverride::revert(photo());
        angled.rotate_deg = 89.0;
        assert!(check_override(&angled).is_err());
    }

    #[test]
    fn a_revert_is_a_valid_override() {
        assert!(check_override(&GeometryOverride::revert(photo())).is_ok());
    }
}
