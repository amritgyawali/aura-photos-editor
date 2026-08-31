//! This phase's own error constructors. Codes ML 5130 and 5132-5135, registered in
//! `crates/aura-core/errors.toml`.
//!
//! Five codes. `AURA-ML-5131` lives in `aura-core`'s `errors::ml` rather than here, because the
//! frozen `CameraMatchService` documents it on `set_reference`, `set_enabled` and `set_override`,
//! and a contract cannot depend on the crate that implements it. The same split phases 16, 22, 23,
//! 24 and 25 made.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// One project's camera matching pass could not run at all.
pub const CAMERA_PASS_FAILED: ErrorCode = ErrorCode("AURA-ML-5130");
/// Stored fingerprints or transforms came from different arithmetic or a different policy.
pub const CAMERA_VERSION_DRIFT: ErrorCode = ErrorCode("AURA-ML-5132");
/// The camera matching policy table was refused.
pub const CAMERA_POLICY_REFUSED: ErrorCode = ErrorCode("AURA-ML-5133");
/// A bundled brand baseline could not be loaded.
pub const CAMERA_BASELINE_REFUSED: ErrorCode = ErrorCode("AURA-ML-5134");
/// A solved transform failed held-out verification.
pub const CAMERA_HELDOUT_FAILED: ErrorCode = ErrorCode("AURA-ML-5135");

/// One project's camera matching pass failed.
///
/// `retry` rather than `fallback`, because there is no fallback: without this pass every body keeps
/// exactly the colour science it came with, which is the state the product was in before this phase
/// existed. That is a gallery that looks like two weddings and not a broken one, and a caller that
/// rendered it as "matched" would be making a claim nobody measured.
#[must_use]
pub fn pass_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CAMERA_PASS_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        detail,
        "AURA could not match the cameras at this wedding to each other. Every photograph still \
         has the edit it was given on its own, and nothing has been lost.",
    )
}

/// Stored rows were produced by a different build's arithmetic or a different policy table.
#[must_use]
pub fn version_drift(stored: (u16, u16), current: (u16, u16)) -> AuraError {
    AuraError::new(
        CAMERA_VERSION_DRIFT,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "stored analysis/policy {}/{} against current {}/{}",
            stored.0, stored.1, current.0, current.1
        ),
        "AURA has improved how it matches cameras to each other, so it is re-checking this wedding \
         in the background. Anything you chose or set yourself is kept.",
    )
}

/// The matching policy table would not load, or tried to widen a bound the contract owns.
///
/// `halt` rather than `fallback`, and it is the same call phases 21, 22 and 25 made about their own
/// tables: a file that tries to raise a ceiling is not a file with a typo in it, it is a file that
/// would let this pass move a whole body's worth of photographs further than the product promises.
/// Falling back on defaults would run the pass under settings nobody chose while a studio believed
/// their own were in force.
#[must_use]
pub fn policy_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CAMERA_POLICY_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        detail,
        "AURA could not load the settings that decide how far it may correct a camera, so it has \
         not matched anything. Restore the file or reinstall.",
    )
}

/// A bundled brand baseline would not load, or declared a movement outside the contract's bounds.
///
/// `warning` and `fallback` rather than `halt`, and the difference from the policy table above is
/// the whole argument: a policy file governs **every** body in the wedding and a baseline governs
/// one brand, so a bad baseline degrades one camera to the neutral transform - which changes
/// nothing - while a bad policy would silently re-scope the phase.
#[must_use]
pub fn baseline_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CAMERA_BASELINE_REFUSED,
        Severity::Warning,
        Recovery::Fallback,
        detail,
        "AURA could not read what it knows about this camera's manufacturer, so it has changed \
         nothing about that camera rather than guess.",
    )
}

/// A solved transform did not improve appearance distance on pairs the solver had not seen.
///
/// This is the one error in the phase that means the product **worked**. Section 6.2 asks for
/// held-out verification precisely so an overfitted transform is caught, and a build that never
/// raised this would be a build whose check was not running. It is a warning with a fallback
/// because the answer is the brand baseline and the report says so.
#[must_use]
pub fn heldout_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CAMERA_HELDOUT_FAILED,
        Severity::Warning,
        Recovery::Fallback,
        detail,
        "The correction for this camera did not hold up when AURA checked it against photographs \
         it had not used, so it fell back on what it knows about the brand.",
    )
}
