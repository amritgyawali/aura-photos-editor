//! This crate's own error constructors. Codes ML 5115-5122, registered in
//! `crates/aura-core/errors.toml`.
//!
//! Eight codes and five of them are refusals, which is the highest proportion in the product. That
//! is the phase rather than an accident: removing a distraction is the first thing AURA does that
//! takes away something the camera got right, and refusing costs a photographer two minutes of
//! manual work they were not going to spend anyway.
//!
//! `AURA-ML-5116` and `AURA-ML-5117` live in `aura-core` rather than here, because
//! `CleanupProposal::new` is the only constructor of a proposal and the contract cannot depend on
//! the crate that implements it. The same split phases 16, 22 and 23 made.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// Stored proposals came from different detectors, safety arithmetic or policy.
pub const CLEANUP_VERSION_DRIFT: ErrorCode = ErrorCode("AURA-ML-5115");
/// One photograph could not be examined for distractions.
pub const CLEANUP_ITEM_FAILED: ErrorCode = ErrorCode("AURA-ML-5118");
/// The cleanup policy table was refused.
pub const CLEANUP_POLICY_REFUSED: ErrorCode = ErrorCode("AURA-ML-5119");
/// A scene has no cleanup policy row.
pub const CLEANUP_SCENE_MISSING: ErrorCode = ErrorCode("AURA-ML-5120");
/// A removal was undone by the artefact self-check before it was shown.
pub const CLEANUP_SELF_CHECK_REVERTED: ErrorCode = ErrorCode("AURA-ML-5121");
/// Nothing could be shown safe to remove, because the regions that prove it are absent.
pub const CLEANUP_PROTECTION_UNKNOWN: ErrorCode = ErrorCode("AURA-ML-5122");

/// Stored proposals were produced by a different detector, arithmetic or policy than this build.
#[must_use]
pub fn version_drift(stored: (u16, u16, u16), current: (u16, u16, u16)) -> AuraError {
    AuraError::new(
        CLEANUP_VERSION_DRIFT,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "stored detector/analysis/policy {}/{}/{} against current {}/{}/{}",
            stored.0, stored.1, stored.2, current.0, current.1, current.2
        ),
        "AURA has improved how it spots and removes distractions, so it is re-checking this \
         wedding in the background. Anything you have accepted or rejected yourself is kept.",
    )
}

/// One photograph could not be examined.
///
/// `retry` rather than `fallback`, because there is no fallback: the fallback *is* the default.
/// A photograph nobody examined and a photograph with nothing to remove are delivered identically,
/// which is why `CleanupOutline::examined` exists and why a panel must never render "no
/// distractions found" for a frame that raised this.
#[must_use]
pub fn item_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CLEANUP_ITEM_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        detail,
        "AURA could not look for distractions in one photograph, so it has left it exactly as it \
         was. Everything else in this wedding is unaffected.",
    )
}

/// `cleanup_policy.toml` did not load, or tried to widen a bound the contract owns.
///
/// `run_blocking` rather than `warning`, because a missing *row* falls back to removing nothing
/// from that scene and a missing *file* means no row can be checked at all. Tidying every wedding
/// to a default nobody approved is far worse than tidying nothing.
#[must_use]
pub fn policy_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CLEANUP_POLICY_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        detail,
        "AURA could not load the settings that decide what may be tidied out of a photograph, so \
         it has not removed anything. Restore the file or reinstall.",
    )
}

/// A scene has no row in the policy table.
///
/// No neutral fallback: invariant 7 says no threshold is global, and a default area cap applied
/// to a scene nobody wrote a row for is exactly a global threshold wearing a scene's name.
#[must_use]
pub fn scene_missing(scene: &str) -> AuraError {
    AuraError::new(
        CLEANUP_SCENE_MISSING,
        Severity::Warning,
        Recovery::Fallback,
        format!("no cleanup policy row for scene {scene}"),
        "AURA has no tidying guidance recorded for this kind of photograph yet, so it is leaving \
         those ones exactly as they were shot. They are all still usable.",
    )
}

/// The self-check found an artefact and put the photograph back.
///
/// A warning rather than a failure: it is the mechanism working. A photographer watching this
/// number is watching the product decline to ship an artefact.
#[must_use]
pub fn self_check_reverted(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CLEANUP_SELF_CHECK_REVERTED,
        Severity::Warning,
        Recovery::Fallback,
        detail,
        "AURA tried tidying something out of a photograph, did not like its own result, and put \
         the photograph back exactly as it was.",
    )
}

/// The masks that would prove a region is safe to remove did not arrive.
///
/// **Distinct from finding a person, on purpose.** One says the product checked and found
/// somebody; this says it could not check. Only the first is a claim, and rendering them the same
/// way would let a build with no segmenter look like a build that examined every photograph and
/// found them all clear.
#[must_use]
pub fn protection_unknown(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        CLEANUP_PROTECTION_UNKNOWN,
        Severity::Warning,
        Recovery::Fallback,
        detail,
        "AURA cannot yet tell where people, dresses and rings are in some photographs, so it will \
         not tidy anything out of them. They are all still usable and nothing has been changed.",
    )
}
