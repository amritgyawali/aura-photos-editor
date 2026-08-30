//! This crate's own error constructors. Codes ML 5123-5129, registered in
//! `crates/aura-core/errors.toml`.
//!
//! Seven codes. `AURA-ML-5124` and `AURA-ML-5125` live in `aura-core`'s `errors::ml` rather than
//! here, because the frozen `GalleryService` documents them on `pin_anchor`, `reject_anchor`,
//! `set_override` and `set_enabled`, and a contract cannot depend on the crate that implements it.
//! The same split phases 16, 22, 23 and 24 made.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// One project's consistency pass could not run at all.
pub const GALLERY_PASS_FAILED: ErrorCode = ErrorCode("AURA-ML-5123");
/// Stored nodes, anchors or deltas came from different arithmetic or a different policy.
pub const GALLERY_VERSION_DRIFT: ErrorCode = ErrorCode("AURA-ML-5127");
/// The consistency policy table was refused.
pub const GALLERY_POLICY_REFUSED: ErrorCode = ErrorCode("AURA-ML-5129");
/// A scene has no consistency policy row.
pub const GALLERY_SCENE_MISSING: ErrorCode = ErrorCode("AURA-ML-5126");
/// A skin target could not be built for an identity that should have had one.
pub const GALLERY_SKIN_TARGET_REFUSED: ErrorCode = ErrorCode("AURA-ML-5128");

/// One project's consistency pass failed.
///
/// `retry` rather than `fallback`, because there is no fallback: without this pass every frame
/// keeps exactly the per-frame answer phases 15 and 16 gave it, which is the state the product was
/// in before this phase existed. That is a worse gallery and not a broken one, and a caller that
/// rendered it as "consistent" would be making a claim nobody measured.
#[must_use]
pub fn pass_failed(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        GALLERY_PASS_FAILED,
        Severity::ItemFailed,
        Recovery::Retry,
        detail,
        "AURA could not match this wedding together as one gallery. Every photograph still has \
         the edit it was given on its own, and nothing has been lost.",
    )
}

/// Stored rows were produced by a different build's arithmetic or a different policy table.
#[must_use]
pub fn version_drift(stored: (u16, u16), current: (u16, u16)) -> AuraError {
    AuraError::new(
        GALLERY_VERSION_DRIFT,
        Severity::Degraded,
        Recovery::Fallback,
        format!(
            "stored analysis/policy {}/{} against current {}/{}",
            stored.0, stored.1, current.0, current.1
        ),
        "AURA has improved how it matches a wedding together, so it is re-checking this one in \
         the background. Anything you pinned or set yourself is kept.",
    )
}

/// The consistency policy table would not load, or tried to widen a bound the contract owns.
///
/// `halt` rather than `fallback`, and that is the decision worth remembering: a policy file that
/// tries to raise a ceiling is not a file with a typo in it, it is a file that would let this pass
/// move a photograph further than the product promises. Falling back on defaults would run the
/// pass under settings nobody chose while a studio believed their own were in force. Phase 21 and
/// phase 22 made the same call about their own tables.
#[must_use]
pub fn policy_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        GALLERY_POLICY_REFUSED,
        Severity::RunBlocking,
        Recovery::Halt,
        detail,
        "AURA could not load the settings that decide how far it may move a photograph to match \
         the rest of a wedding, so it has not matched anything. Restore the file or reinstall.",
    )
}

/// A scene has no policy row, so nothing about it could be scene-conditioned.
#[must_use]
pub fn scene_missing(scene: &str) -> AuraError {
    AuraError::new(
        GALLERY_SCENE_MISSING,
        Severity::Warning,
        Recovery::Fallback,
        format!("no consistency policy row for scene {scene}"),
        "AURA has no matching guidance recorded for this kind of photograph yet, so it has used \
         its most careful settings on them. They are all still usable.",
    )
}

/// An identity's gallery skin target could not be built.
#[must_use]
pub fn skin_target_refused(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        GALLERY_SKIN_TARGET_REFUSED,
        Severity::Warning,
        Recovery::Fallback,
        detail,
        "AURA has not seen one of the people in this wedding in enough well-lit photographs to \
         know how their skin should look, so it has left their skin exactly as it was.",
    )
}
