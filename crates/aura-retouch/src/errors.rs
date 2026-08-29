//! This crate own error constructors. Codes ML 5096-5101, registered in
//! `crates/aura-core/errors.toml`.
//!
//! The split every phase since 09 has kept: `aura-core` owns the shapes and the predicates,
//! and the crate that makes the decisions owns the registry. It is what lets
//! [`aura_core::contract::retouch::RetouchPlan::broken_guarantee`] be asked by the solver, the
//! store, the IPC layer and the eval harness without any of them disagreeing about what a sound
//! plan is.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// Stored plans came from different heads, different arithmetic or a different preset table.
pub const ML_RETOUCH_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5096");
/// A retouch override or a protected-feature change was refused.
pub const ML_RETOUCH_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5097");
/// One photograph could not be retouched.
pub const ML_RETOUCH_FAILED: ErrorCode = ErrorCode("AURA-ML-5098");
/// The retouch preset table was refused.
pub const ML_PRESETS_REFUSED: ErrorCode = ErrorCode("AURA-ML-5099");
/// A scene has no retouch preset row.
pub const ML_SCENE_UNPRESET: ErrorCode = ErrorCode("AURA-ML-5100");
/// The texture guarantee forced a gentler retouch, or withdrew one.
pub const ML_TEXTURE_GUARD: ErrorCode = ErrorCode("AURA-ML-5101");

/// Stored plans disagree with the running build about a version.
///
/// Degraded rather than fatal, as `AURA-ML-5033`, `AURA-ML-5060` and `AURA-ML-5084` are. Three
/// numbers: the heads, the arithmetic and the preset file, which invalidate the detections, the
/// measurements and the strengths respectively.
#[must_use]
pub fn retouch_version_mismatch(
    stored: (u16, u16, u16),
    current: (u16, u16, u16),
    rows: usize,
) -> AuraError {
    AuraError::new(
        ML_RETOUCH_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{rows} retouch plans were made under model {}/analysis {}/presets {} and this build \
             is model {}/analysis {}/presets {}",
            stored.0, stored.1, stored.2, current.0, current.1, current.2
        ),
        "AURA has improved how it retouches skin, so it is re-checking this wedding in the \
         background. Anything you have already adjusted is kept.",
    )
    .with_context("rows", rows.to_string())
}

/// An override or a protected-feature change could not be recorded.
///
/// The message names the reason rather than the caller. One of the five causes is not a fault
/// at all and is the reason this code exists: a photographer asked to stop protecting a tattoo,
/// and this product does not do that.
#[must_use]
pub fn retouch_edit_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_RETOUCH_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail.into(),
        "AURA could not record that change. Nothing was changed. Tattoos are always kept and \
         cannot be turned off.",
    )
}

/// One photograph could not be retouched.
///
/// **A refused plan is stored as no plan rather than as a weak one.** Three of the seven
/// guarantees `RetouchPlan::broken_guarantee` checks describe a photograph that would look
/// visibly retouched, and the other four are ways a stored row would lie to phases 21, 25 and
/// 27 about what happened to somebody skin.
#[must_use]
pub fn retouch_failed(photo: &str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_RETOUCH_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{photo}: {}", detail.into()),
        "AURA could not work out the skin retouching for one photograph, and has left that \
         photograph untouched. Everything else in this wedding is unaffected.",
    )
    .with_context("photo", photo)
}

/// The preset table would not load.
///
/// Whole-file refusal, as `AURA-ML-5087` and `AURA-ML-5063` are. Half a preset table would
/// retouch the ceremony against measured strengths and the reception against nothing, and that
/// inconsistency is invisible in the delivered gallery - which is the one failure a
/// gallery-consistent retoucher must not have.
#[must_use]
pub fn presets_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_PRESETS_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: `{key}` {rule}"),
        "AURA could not load the settings that decide how much retouching each kind of \
         photograph gets, so it has not retouched anything. Restore the file or reinstall.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// A scene with no preset row. The neutral row was used and the plan says so.
#[must_use]
pub fn scene_unpreset(scene: &str) -> AuraError {
    AuraError::new(
        ML_SCENE_UNPRESET,
        Severity::Warning,
        Recovery::Fallback,
        format!("no retouch preset row for scene `{scene}`; the neutral row was used"),
        "AURA has no retouching guidance recorded for this kind of photograph yet, so it is \
         treating those ones very gently. They are all still usable.",
    )
    .with_context("scene", scene)
}

/// The texture guarantee forced a gentler retouch, or withdrew one.
///
/// A warning rather than a failure, and it is registered so that the mechanism is **visible**
/// rather than because something is wrong. A frame that could not be retouched without losing
/// texture ships unretouched, which is a much smaller failure than a frame that ships plastic.
#[must_use]
pub fn texture_guard(photo: &str, ratio: f32, floor: f32, withdrawn: bool) -> AuraError {
    let what = if withdrawn {
        "withdrew the retouch"
    } else {
        "reduced the strength"
    };
    AuraError::new(
        ML_TEXTURE_GUARD,
        Severity::Warning,
        Recovery::Fallback,
        format!(
            "{photo}: the texture guard {what}: band ratio {ratio:.3} against floor {floor:.3}"
        ),
        "AURA used gentler retouching on some photographs, or none at all, so that skin kept \
         its own texture. Nothing has been damaged.",
    )
    .with_context("photo", photo)
    .with_context("band_ratio", format!("{ratio:.4}"))
}

// ---------------------------------------------------------------------------
// PHASE-21. The micro-retouch suite, ML 5102-5107.
// ---------------------------------------------------------------------------

/// Stored micro plans came from different heads, different arithmetic or a different matrix.
pub const ML_MICRO_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5102");
/// One photograph's small fixes could not be planned, or a plan broke a guarantee.
pub const ML_MICRO_FAILED: ErrorCode = ErrorCode("AURA-ML-5103");
/// A micro-retouch matrix change was refused.
pub const ML_MICRO_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5104");
/// The micro-retouch matrix file was refused.
pub const ML_MICRO_MATRIX_REFUSED: ErrorCode = ErrorCode("AURA-ML-5105");
/// A region was unusable, so an operation was skipped.
pub const ML_MICRO_REGION_UNUSABLE: ErrorCode = ErrorCode("AURA-ML-5106");
/// The naturalness guard made an operation gentler, or withdrew a family of them.
pub const ML_NATURALNESS_GUARD: ErrorCode = ErrorCode("AURA-ML-5107");

/// Stored micro plans disagree with the running build about a version.
///
/// Degraded rather than fatal, as `AURA-ML-5033`, `AURA-ML-5060`, `AURA-ML-5084` and
/// `AURA-ML-5096` are. Three numbers: the heads, the arithmetic and the matrix file, which
/// invalidate the detections, the measurements and the switches respectively.
#[must_use]
pub fn micro_version_mismatch(
    stored: (u16, u16, u16),
    current: (u16, u16, u16),
    rows: usize,
) -> AuraError {
    AuraError::new(
        ML_MICRO_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "{rows} micro plans were made under model {}/analysis {}/matrix {} and this build is \
             model {}/analysis {}/matrix {}",
            stored.0, stored.1, stored.2, current.0, current.1, current.2
        ),
        "AURA has improved the small fixes it makes, so it is re-checking this wedding in the \
         background. Anything you have already changed is kept.",
    )
    .with_context("rows", rows.to_string())
}

/// One photograph's small fixes could not be worked out.
///
/// **A refused plan is stored as no plan rather than as a weak one.** Every guarantee
/// `MicroPlan::broken_guarantee` checks describes either a photograph that would look worked on
/// or a stored row that would lie to phases 25, 27 and 28 about what happened to somebody's
/// face - and one of them, an operation that ran while the matrix forbade it, describes a
/// delivery a studio did not agree to.
#[must_use]
pub fn micro_failed(photo: &str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_MICRO_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        format!("{photo}: {}", detail.into()),
        "AURA could not work out the small fixes for one photograph, and has left that \
         photograph alone. Everything else in this wedding is unaffected.",
    )
    .with_context("photo", photo)
}

/// A change to which operations are permitted could not be recorded.
#[must_use]
pub fn micro_edit_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_MICRO_EDIT_REFUSED,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail.into(),
        "AURA could not record that change. Nothing was changed.",
    )
}

/// The matrix file would not load.
///
/// Whole-file refusal, as `AURA-ML-5099`, `AURA-ML-5087` and `AURA-ML-5063` are. Half a matrix
/// would clean the ceremony against measured ceilings and the reception against nothing, and
/// that inconsistency is invisible in a delivered gallery.
#[must_use]
pub fn micro_matrix_refused(file: &str, key: &str, rule: &str) -> AuraError {
    AuraError::new(
        ML_MICRO_MATRIX_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("{file}: `{key}` {rule}"),
        "AURA could not load the settings that decide which small fixes it may make, so it has \
         not made any. Restore the file or reinstall.",
    )
    .with_context("file", file)
    .with_context("key", key)
}

/// A region arrived from phase 18 that this phase could not act through.
///
/// A warning rather than a failure, and it is the code that separates "there was nothing to fix"
/// from "AURA could not see where the teeth were". Those two look identical in a coverage report
/// otherwise, and they send a support engineer to two different places.
#[must_use]
pub fn micro_region_unusable(photo: &str, region: &str, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        ML_MICRO_REGION_UNUSABLE,
        Severity::Warning,
        Recovery::Fallback,
        format!("{photo}: the `{region}` region {}", detail.into()),
        "AURA is not sure enough where hair, teeth, eyes or clothing are in some photographs, so \
         it left those areas alone. Nothing has been damaged.",
    )
    .with_context("photo", photo)
    .with_context("region", region)
}

/// The naturalness guard made an operation gentler, or withdrew a family of them.
///
/// A warning rather than a failure, and it is registered so that the mechanism is **visible**
/// rather than because something is wrong. A frame whose teeth could not be evened without
/// leaving the locus ships with its teeth as they were, which is a much smaller failure than a
/// frame that ships with fluorescent teeth.
#[must_use]
pub fn naturalness_guard(
    photo: &str,
    family: &str,
    measured: f32,
    floor: f32,
    withdrawn: bool,
) -> AuraError {
    let what = if withdrawn {
        "withdrew the operations"
    } else {
        "reduced the strength"
    };
    AuraError::new(
        ML_NATURALNESS_GUARD,
        Severity::Warning,
        Recovery::Fallback,
        format!(
            "{photo}: the naturalness guard {what} for `{family}`: measured {measured:.4} against \
             floor {floor:.4}"
        ),
        "AURA made some of its small fixes more gently, or not at all, so that hair, teeth and \
         eyes still look like themselves. Nothing has been damaged.",
    )
    .with_context("photo", photo)
    .with_context("family", family)
    .with_context("measured", format!("{measured:.4}"))
}
