//! Turning the contract predicates into this phase errors.
//!
//! `aura-core` owns the shapes and the predicates; this crate owns the error registry. The
//! split is the one every phase since 09 has kept.

use aura_core::contract::error::AuraError;
use aura_core::contract::retouch::{ProtectedFeature, RetouchOverride, RetouchPlan};

use crate::errors;

/// Refuse a plan that breaks one of this phase own guarantees.
///
/// **A refused plan is stored as no plan rather than as a weak one.** Three of the seven
/// guarantees describe a photograph that would look visibly retouched, and one of them - an
/// operation overlapping a protected feature - describes a photograph of somebody else.
///
/// # Errors
///
/// `AURA-ML-5098` naming the photograph and the guarantee.
pub fn check_plan(plan: &RetouchPlan) -> Result<(), AuraError> {
    match plan.broken_guarantee() {
        None => Ok(()),
        Some(problem) => Err(errors::retouch_failed(&plan.image_id.to_db(), problem)),
    }
}

/// Refuse an override that cannot be applied.
///
/// # Errors
///
/// `AURA-ML-5097` naming the problem.
pub fn check_override(values: &RetouchOverride) -> Result<(), AuraError> {
    match values.problem() {
        None => Ok(()),
        Some(problem) => Err(errors::retouch_edit_refused(problem)),
    }
}

/// Refuse a protected-feature change that this product does not make.
///
/// Two refusals, and the second is the one that matters: **an absolute protection cannot be
/// cleared.** Section 10.1 gates tattoo removal at zero per cent and section 11 of
/// `docs/plan/CLAUDE.md` forbids operations that change a person identity permanently, so this
/// is a property of the kind rather than a setting. Migration 21 carries the same refusal as a
/// trigger, because a promise enforced in one layer is a promise until somebody writes a second
/// caller.
///
/// # Errors
///
/// `AURA-ML-5097` when the rectangle is empty, or when clearing an absolute feature.
pub fn check_protection(feature: &ProtectedFeature, protect: bool) -> Result<(), AuraError> {
    if feature.area.w <= 0.0 || feature.area.h <= 0.0 {
        return Err(errors::retouch_edit_refused(
            "a protected feature with no area",
        ));
    }
    if !protect && feature.is_absolute() {
        return Err(errors::retouch_edit_refused(format!(
            "a {} is always protected and cannot be cleared",
            feature.kind.as_str()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::composition::Box2;
    use aura_core::contract::retouch::{
        ProtectedKind, ProtectedSource, RetouchCode, RetouchPreset, RetouchReason,
    };
    use aura_core::{IdentityId, PhotoId, SceneId};

    fn photo() -> PhotoId {
        PhotoId::from_db("pht_00000000-0000-4000-8000-000000000020").expect("a photo id")
    }

    fn identity() -> IdentityId {
        IdentityId::from_db("idt_00000000-0000-4000-8000-000000000020").expect("an identity")
    }

    fn feature(kind: ProtectedKind) -> ProtectedFeature {
        ProtectedFeature {
            identity: identity(),
            kind,
            area: Box2 {
                x: -0.2,
                y: 0.3,
                w: 0.05,
                h: 0.05,
            },
            confidence: 0.9,
            source: ProtectedSource::CrossFrame,
            frames: 12,
            span_minutes: 240.0,
            first_seen: photo(),
        }
    }

    #[test]
    fn a_sound_plan_passes_and_an_unsound_one_is_5098() {
        let good = RetouchPlan::nothing(
            photo(),
            SceneId::Ceremony,
            RetouchReason::plain(RetouchCode::NoBlemishFound, 0.0),
        );
        assert!(check_plan(&good).is_ok());

        let mut bad = good;
        bad.reasons.clear();
        let err = check_plan(&bad).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5098");
    }

    #[test]
    fn an_empty_override_is_5097() {
        let err = check_override(&RetouchOverride::default()).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5097");
        assert!(check_override(&RetouchOverride::preset(RetouchPreset::Light)).is_ok());
    }

    #[test]
    fn a_tattoo_cannot_be_unprotected_and_a_mole_can() {
        let tattoo = feature(ProtectedKind::Tattoo);
        assert!(check_protection(&tattoo, true).is_ok());
        let err = check_protection(&tattoo, false).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5097");
        assert!(err.detail.contains("tattoo"));

        let mole = feature(ProtectedKind::Mole);
        assert!(check_protection(&mole, false).is_ok());
    }
}
