//! This crate own error constructors. Codes ML 5090-5095, registered in
//! `crates/aura-core/errors.toml`.
//!
//! The split every phase since 09 has kept: `aura-core` owns the shapes and the predicates,
//! and the crate that makes the decisions owns the registry. It is what lets
//! [`aura_core::contract::retouch::RetouchPlan::broken_guarantee`] be asked by the solver, the
//! store, the IPC layer and the eval harness without any of them disagreeing about what a sound
//! plan is.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// Stored plans came from different heads, different arithmetic or a different preset table.
pub const ML_RETOUCH_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5090");
/// A retouch override or a protected-feature change was refused.
pub const ML_RETOUCH_EDIT_REFUSED: ErrorCode = ErrorCode("AURA-ML-5091");
/// One photograph could not be retouched.
pub const ML_RETOUCH_FAILED: ErrorCode = ErrorCode("AURA-ML-5092");
/// The retouch preset table was refused.
pub const ML_PRESETS_REFUSED: ErrorCode = ErrorCode("AURA-ML-5093");
/// A scene has no retouch preset row.
pub const ML_SCENE_UNPRESET: ErrorCode = ErrorCode("AURA-ML-5094");
/// The texture guarantee forced a gentler retouch, or withdrew one.
pub const ML_TEXTURE_GUARD: ErrorCode = ErrorCode("AURA-ML-5095");

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
