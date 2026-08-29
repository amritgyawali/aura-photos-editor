//! This crate's own error constructors. Codes ML 5108-5114, registered in
//! `crates/aura-core/errors.toml`.
//!
//! The split every phase since 09 has kept: `aura-core` owns the shapes and the predicates, and
//! the crate that makes the decisions owns the registry. It is what lets
//! [`aura_core::contract::restore::RestorePlan::broken_guarantee`] be asked by the solver, the
//! store, the IPC layer and the eval harness without any of them disagreeing about what a sound
//! plan is.
//!
//! Two of the seven codes are this phase's own rather than a shape an earlier phase established,
//! and both are **warnings that describe the product working**. [`selfcheck_reduced`] fires when
//! the artefact self-check made an operation gentler, and [`identity_declined`] fires when face
//! recovery was refused to keep somebody looking like themselves. Neither is a fault, and both
//! are registered rather than logged quietly, because "AURA stopped short of changing what this
//! person looks like" is the single sentence this phase most needs to be able to say out loud.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// Stored restoration plans came from different heads, arithmetic or profile tables.
pub const ML_RESTORE_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5108");
/// One photograph could not be restored, or a plan broke a guarantee.
pub const ML_RESTORE_FAILED: ErrorCode = ErrorCode("AURA-ML-5109");
/// A restoration override was refused.
pub const ML_RESTORE_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5110");
/// A restoration profile or camera noise-model file was refused.
pub const ML_RESTORE_PROFILE_REFUSED: ErrorCode = ErrorCode("AURA-ML-5111");
/// A region was unusable, so sharpening was skipped.
pub const ML_RESTORE_REGION_UNUSABLE: ErrorCode = ErrorCode("AURA-ML-5112");
/// The artefact self-check made a restoration gentler, or withdrew one.
pub const ML_RESTORE_SELF_CHECK: ErrorCode = ErrorCode("AURA-ML-5113");
/// Face recovery was declined to keep somebody looking like themselves.
pub const ML_RESTORE_IDENTITY_DECLINED: ErrorCode = ErrorCode("AURA-ML-5114");

/// Stored restoration plans disagree with the running build about a version.
///
/// Degraded rather than fatal, as `AURA-ML-5033`, `AURA-ML-5060`, `AURA-ML-5084`, `AURA-ML-5096`
/// and `AURA-ML-5102` are. Three numbers: the heads, the arithmetic and the profile tables,
/// which invalidate the learned decisions, the measurements and the ceilings respectively.
#[must_use]
pub fn restore_version_mismatch(
    stored: (u16, u16, u16),
    current: (u16, u16, u16),
    rows: usize,
) -> AuraError {
    AuraError::new(
        ML_RESTORE_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{rows} restoration plans were made under model {}/analysis {}/profiles {} and this \
             build is model {}/analysis {}/profiles {}",
            stored.0, stored.1, stored.2, current.0, current.1, current.2
        ),
        "AURA has improved how it cleans up noisy and soft photographs, so it is re-checking \
         this wedding in the background. Anything you have already changed is kept.",
    )
    .with_context("rows", rows.to_string())
}

/// One photograph's restoration could not be worked out.
///
/// **A refused plan is stored as no plan rather than as a weak one.** Every guarantee
/// `RestorePlan::broken_guarantee` checks describes either a photograph that would ship with
/// information removed from it or a stored row that would lie to phases 25, 27 and 28 about what
/// was repaired - and one of them, a kept face above the identity ceiling, describes a delivered
/// photograph of somebody who does not quite look like themselves.
#[must_use]
pub fn restore_failed(photo: &str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_RESTORE_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{photo}: {}", detail.into()),
        "AURA could not work out how to clean up one photograph, and has left that photograph \
         alone. Everything else in this wedding is unaffected.",
    )
    .with_context("photo", photo)
}

/// An override or an acceptance could not be recorded.
#[must_use]
pub fn restore_edit_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_RESTORE_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail.into(),
        "AURA could not record that change. Nothing was changed.",
    )
}

/// A profile or noise-model file would not load.
///
/// Whole-file refusal, as `AURA-ML-5087`, `AURA-ML-5099` and `AURA-ML-5105` are. Half a profile
/// table would denoise the ceremony against measured ceilings and the reception against nothing,
/// and that inconsistency is invisible in the delivered gallery until somebody prints it.
#[must_use]
pub fn profile_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_RESTORE_PROFILE_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: `{key}` {rule}"),
        "AURA could not load the settings that decide how much noise it may remove, so it has \
         not removed any. Restore the file or reinstall.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// A region arrived from phase 18 that could not be used, or none arrived at all.
///
/// A warning rather than a failure. Denoising and face recovery do not need a region; only the
/// deconvolution does, and it refuses rather than running blind - ADR-0047 section 4.
#[must_use]
pub fn region_unusable(photo: &str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_RESTORE_REGION_UNUSABLE,
        Severity::Warning,
        Recovery::Fallback,
        format!("{photo}: {}", detail.into()),
        "AURA is not sure enough where skin, sky and out-of-focus background are in some \
         photographs, so it did not sharpen them. Nothing has been damaged.",
    )
    .with_context("photo", photo)
}

/// The artefact self-check made an operation gentler, or withdrew one.
///
/// A warning rather than a failure, registered so that the mechanism is **visible** rather than
/// because something is wrong. A frame that could not be sharpened without ringing ships
/// unsharpened, which is a much smaller failure than a frame that ships with outlines drawn
/// along its edges.
#[must_use]
pub fn selfcheck_reduced(
    photo: &str,
    what: &str,
    measured: f32,
    bound: f32,
    withdrawn: bool,
) -> AuraError {
    let action = if withdrawn {
        "withdrew the operation"
    } else {
        "reduced the strength"
    };
    AuraError::new(
        ML_RESTORE_SELF_CHECK,
        Severity::Warning,
        Recovery::Fallback,
        format!("{photo}: the self-check {action} for {what}: {measured:.4} against {bound:.4}"),
        "AURA cleaned up some photographs more gently, or not at all, so that fabric keeps its \
         texture and edges keep their shape. Nothing has been damaged.",
    )
    .with_context("photo", photo)
    .with_context("measurement", what)
    .with_context("measured", format!("{measured:.4}"))
}

/// Face recovery was declined to keep somebody looking like themselves.
///
/// **The guarantee of this phase, as a code a photographer can find.** A warning rather than a
/// failure, because a face that was left alone is the correct outcome and not a broken one - and
/// registered rather than silent, because a product that quietly declines is a product nobody
/// knows declined.
#[must_use]
pub fn identity_declined(photo: &str, drift: f32, ceiling: f32, resolves: u8) -> AuraError {
    AuraError::new(
        ML_RESTORE_IDENTITY_DECLINED,
        Severity::Warning,
        Recovery::Fallback,
        format!(
            "{photo}: a face was skipped after {resolves} reductions: identity distance \
             {drift:.4} against ceiling {ceiling:.4}"
        ),
        "AURA stopped short of recovering detail in some faces, because going further would \
         have started to change what those people look like. Those photographs are unchanged.",
    )
    .with_context("photo", photo)
    .with_context("identity_drift", format!("{drift:.4}"))
}
