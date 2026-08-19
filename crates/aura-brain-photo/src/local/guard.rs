//! Turning the contract's predicates into this phase's errors.
//!
//! `aura-core` owns the shapes and the predicates; `aura-brain-photo` owns the error
//! registry. The split is the one every phase since 09 has kept, and it is what lets
//! [`aura_core::contract::local::LocalLightPlan::broken_guarantee`] be asked by the solver,
//! the store, the IPC layer and the eval harness without any of them being able to disagree
//! about what a sound plan is.

use aura_core::contract::error::AuraError;
use aura_core::contract::local::{LocalLightPlan, LocalOverride, MaskField};

use crate::errors;

/// Refuse a mask field that cannot be read.
///
/// # Errors
///
/// `AURA-ML-5071` naming the kind and the problem.
pub fn check_mask(field: &MaskField) -> Result<(), AuraError> {
    match field.problem() {
        None => Ok(()),
        Some(problem) => Err(errors::mask_unusable(field.kind.as_str(), problem)),
    }
}

/// Refuse a plan that breaks one of this phase's own guarantees.
///
/// **A refused plan is stored as no plan rather than as a weak one.** Three of the five
/// guarantees describe a photograph that would look visibly edited, and a visibly edited
/// photograph is the failure this phase exists to avoid.
///
/// # Errors
///
/// `AURA-ML-5068` naming the photograph and the guarantee.
pub fn check_plan(plan: &LocalLightPlan) -> Result<(), AuraError> {
    match plan.broken_guarantee() {
        None => Ok(()),
        Some(problem) => Err(errors::local_failed(&plan.image_id.to_db(), problem)),
    }
}

/// Refuse an override that cannot be applied.
///
/// # Errors
///
/// `AURA-ML-5067` naming the problem.
pub fn check_override(values: &LocalOverride) -> Result<(), AuraError> {
    match values.problem() {
        None => Ok(()),
        Some(problem) => Err(errors::local_edit_refused(problem)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::integrity::CropRect;
    use aura_core::contract::local::{LocalCode, LocalOp, LocalReason, MaskKind};
    use aura_core::{PhotoId, SceneId};

    fn photo() -> PhotoId {
        PhotoId::from_db("pht_00000000-0000-4000-8000-000000000019").expect("a photo id")
    }

    #[test]
    fn a_readable_mask_passes_and_a_broken_one_names_its_kind() {
        let good = MaskField {
            kind: MaskKind::Subject,
            identity: None,
            bounds: CropRect::FULL,
            width: 2,
            height: 2,
            alpha: vec![255; 4],
            confidence: 0.8,
            edge_quality: 0.8,
            model_ver: 1,
        };
        assert!(check_mask(&good).is_ok());

        let mut broken = good;
        broken.alpha.pop();
        let err = check_mask(&broken).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5071");
        assert!(err.detail.contains("subject"));
    }

    #[test]
    fn a_sound_plan_passes_and_an_unsound_one_is_5068() {
        let good = LocalLightPlan::nothing(
            photo(),
            SceneId::Ceremony,
            LocalReason::plain(LocalCode::FaceAlreadyInBand, 0.0),
        );
        assert!(check_plan(&good).is_ok());

        let mut bad = good;
        bad.reasons.clear();
        let err = check_plan(&bad).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5068");
    }

    #[test]
    fn an_empty_override_is_5067() {
        let err = check_override(&LocalOverride::default()).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5067");
        assert!(check_override(&LocalOverride::one(LocalOp::FaceLight, 0.5)).is_ok());
    }
}
