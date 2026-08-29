//! This crate's own error constructors. Codes ML 5109-5114, registered in
//! `crates/aura-core/errors.toml`.
//!
//! The split every phase since 09 has kept: `aura-core` owns the shapes and the predicates, and
//! the crate that makes the decisions owns the registry.
//!
//! Two of the six codes are this phase's own rather than a shape an earlier phase established,
//! and both **describe the product declining to act**. [`crop_refused`] fires when a proposed
//! rectangle was dropped because it would have cut somebody, and [`straighten_refused`] fires
//! when levelling a frame would have cost more of it than the tilt was worth. Neither is a fault.
//! They are registered rather than logged quietly because the commonest question a photographer
//! has about this phase is why a particular photograph was left alone, and a product that could
//! only report what it *did* would have no answer.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// Stored geometry plans came from different arithmetic or different profile tables.
pub const ML_GEOMETRY_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5109");
/// One photograph's geometry could not be worked out.
pub const ML_GEOMETRY_FAILED: ErrorCode = ErrorCode("AURA-ML-5110");
/// A geometry override or acceptance was refused.
pub const ML_GEOMETRY_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5111");
/// A lens profile database or crop rule file was refused.
pub const ML_GEOMETRY_PROFILE_REFUSED: ErrorCode = ErrorCode("AURA-ML-5112");
/// A crop candidate was dropped because it would have cut something that matters.
pub const ML_GEOMETRY_CROP_REFUSED: ErrorCode = ErrorCode("AURA-ML-5113");
/// A rotation or a perspective correction was reduced or abandoned.
pub const ML_GEOMETRY_STRAIGHTEN_REFUSED: ErrorCode = ErrorCode("AURA-ML-5114");

/// Stored geometry plans disagree with the running build about a version.
///
/// Degraded rather than fatal, as `AURA-ML-5033`, `AURA-ML-5060`, `AURA-ML-5084`, `AURA-ML-5090`,
/// `AURA-ML-5096` and `AURA-ML-5102` are. **Two numbers rather than three**, and the missing one
/// is the point: this phase ships no model, so there is no `model_ver` that could move. What can
/// move is the arithmetic and the two tables - the lens profile database and the crop rules -
/// and those invalidate different things: new arithmetic re-measures every frame, a new lens
/// profile re-corrects only the frames shot on that lens.
#[must_use]
pub fn geometry_version_mismatch(
    stored: (u16, u16),
    current: (u16, u16),
    rows: usize,
) -> AuraError {
    AuraError::new(
        ML_GEOMETRY_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{rows} geometry plans were made under analysis {}/profiles {} and this build is \
             analysis {}/profiles {}",
            stored.0, stored.1, current.0, current.1
        ),
        "AURA has improved how it straightens and crops photographs, so it is re-checking this \
         wedding in the background. Any framing you set yourself is kept.",
    )
    .with_context("rows", rows.to_string())
}

/// One photograph's geometry could not be worked out.
///
/// **A refused plan is stored as no plan rather than as a weak one.** A stored geometry row is a
/// statement about which pixels are delivered, and a half-made one is a rectangle that phases 27,
/// 29 and 30 would all believe.
#[must_use]
pub fn geometry_failed(photo: &str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_GEOMETRY_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{photo}: {}", detail.into()),
        "AURA could not work out how to finish one photograph's framing, and has left that \
         photograph exactly as it was shot. Everything else in this wedding is unaffected.",
    )
    .with_context("photo", photo)
}

/// An override or an acceptance could not be recorded.
///
/// The rectangle a photographer set is the one thing in this phase that is not derivable from
/// anything, so a refusal here is louder than a refusal anywhere else in the crate: losing it
/// means losing work that cannot be recomputed.
#[must_use]
pub fn geometry_edit_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_GEOMETRY_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail.into(),
        "AURA could not save that change to the framing. Nothing has been altered - try again.",
    )
}

/// A lens profile database or a crop rule table will not load.
///
/// Whole-file refusal, as `AURA-ML-5087`, `AURA-ML-5093`, `AURA-ML-5099` and `AURA-ML-5105` are.
/// Half a profile database is worse than none: a lens whose distortion row loaded and whose
/// vignette row did not would be corrected in one axis and not the other, which looks exactly
/// like a lens that behaves that way.
#[must_use]
pub fn profile_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_GEOMETRY_PROFILE_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: {key} {rule}"),
        "AURA cannot read its lens profile or cropping settings, so it will not straighten or \
         crop anything until they are fixed. Your photographs are untouched.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// A crop candidate was dropped because it would have cut something that matters.
///
/// **A warning that describes the product working**, and the one this phase most needs to be able
/// to say out loud. Section 12's first failure mode is an auto-crop that cuts something
/// important, and the mitigation is a hard constraint - so every time the constraint fires, that
/// is the mitigation doing its job rather than a fault to be diagnosed.
///
/// `Severity::Warning` rather than `ItemFailed`: the frame is delivered, at the framing it was
/// shot at, which is the correct outcome and not a failure.
#[must_use]
pub fn crop_refused(photo: &str, aspect: &str, code: &str) -> AuraError {
    AuraError::new(
        ML_GEOMETRY_CROP_REFUSED,
        Severity::Warning,
        Recovery::Fallback,
        format!("{photo}: the {aspect} crop was dropped ({code})"),
        "AURA did not crop this photograph, because a tighter frame would have cut somebody. \
         It is delivered at the framing you shot.",
    )
    .with_context("photo", photo)
    .with_context("aspect", aspect)
    .with_context("code", code)
}

/// A rotation or a perspective correction was reduced or abandoned.
///
/// The second warning that describes the product working. A rotation costs a crop, and a crop
/// that would cut somebody is a crop this phase will not make - so the choices are a smaller
/// angle or no angle, and both are recorded here with the angle that was wanted beside the angle
/// that survived. `applied` of zero is the abandonment.
#[must_use]
pub fn straighten_refused(photo: &str, wanted_deg: f32, applied_deg: f32, why: &str) -> AuraError {
    AuraError::new(
        ML_GEOMETRY_STRAIGHTEN_REFUSED,
        Severity::Warning,
        Recovery::Fallback,
        format!(
            "{photo}: wanted {wanted_deg:.2} deg, applied {applied_deg:.2} deg ({why})"
        ),
        "AURA levelled this photograph less than it wanted to, because levelling it fully would \
         have cropped into somebody.",
    )
    .with_context("photo", photo)
    .with_context("wanted_deg", format!("{wanted_deg:.3}"))
    .with_context("applied_deg", format!("{applied_deg:.3}"))
    .with_context("why", why)
}
