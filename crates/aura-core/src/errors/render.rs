//! Develop-engine constructors. Codes 8000-8999, registered in `errors.toml`.
//!
//! The block was reserved under the name `EXPORT` and carried no codes; phase 14 renames it
//! to `RENDER` because an export is a render written to a file, and phase 30 will want codes
//! in the same domain for the same subject. `docs/adr/ADR-0029-render-pipeline.md` section 9
//! records the decision.
//!
//! Two properties of this domain are worth stating once. **Nothing here can lose a
//! photographer's work**: every code either falls back to a correct-but-slower path, or
//! refuses an automated change and leaves what was already there. And **8006 is a refusal
//! rather than a failure** - an automated pass proposing a change to a parameter a person
//! set is the system working, and the code exists so the refusal is visible instead of
//! silent.

use crate::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// No accelerated render backend; the processor reference path was used.
pub const RENDER_NO_GPU_BACKEND: ErrorCode = ErrorCode("AURA-RENDER-8001");
/// The recipe's shape is wrong and it was refused.
pub const RENDER_RECIPE_INVALID: ErrorCode = ErrorCode("AURA-RENDER-8002");
/// The recipe comes from a newer schema than this engine understands.
pub const RENDER_RECIPE_FROM_FUTURE: ErrorCode = ErrorCode("AURA-RENDER-8003");
/// The XMP sidecar could not be read.
pub const RENDER_XMP_UNREADABLE: ErrorCode = ErrorCode("AURA-RENDER-8004");
/// The AURA sidecar could not be read and the edit was not recovered.
pub const RENDER_SIDECAR_UNREADABLE: ErrorCode = ErrorCode("AURA-RENDER-8005");
/// An automated pass proposed a change to a parameter a person set.
pub const RENDER_USER_FIELD_PROTECTED: ErrorCode = ErrorCode("AURA-RENDER-8006");
/// A re-render did not reproduce the hash the export recorded.
pub const RENDER_HASH_MISMATCH: ErrorCode = ErrorCode("AURA-RENDER-8007");
/// The camera profile is unknown; the reference profile was used.
pub const RENDER_PROFILE_UNKNOWN: ErrorCode = ErrorCode("AURA-RENDER-8008");
/// The render did not fit the memory budget and was streamed in tiles.
pub const RENDER_TILED_FALLBACK: ErrorCode = ErrorCode("AURA-RENDER-8009");
/// The processor and accelerator paths disagreed beyond the documented tolerance.
pub const RENDER_PARITY_DRIFT: ErrorCode = ErrorCode("AURA-RENDER-8010");

/// This build has no GPU backend. Raised once per session, at `Degraded`.
#[must_use]
pub fn no_gpu_backend(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        RENDER_NO_GPU_BACKEND,
        Severity::Degraded,
        Recovery::Fallback,
        detail,
        "AURA is developing photographs using the processor rather than the graphics card. \
         The result is the same; it takes longer.",
    )
}

/// The recipe cannot be rendered because its shape is wrong.
#[must_use]
pub fn recipe_invalid(field: &str, why: &str) -> AuraError {
    AuraError::new(
        RENDER_RECIPE_INVALID,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("recipe field {field} is invalid: {why}"),
        "One edit could not be read, so that photograph was left as it was. Nothing has been \
         changed.",
    )
    .with_context("field", field)
    .with_context("why", why)
}

/// A newer build wrote this recipe. Render what is understood, keep the rest.
#[must_use]
pub fn recipe_from_future(found: u16, understood: u16) -> AuraError {
    AuraError::new(
        RENDER_RECIPE_FROM_FUTURE,
        Severity::Warning,
        Recovery::Fallback,
        format!("recipe schema {found} is newer than {understood}"),
        "This edit was made in a newer version of AURA. What this version understands has been \
         applied, and the rest has been kept untouched.",
    )
    .with_context("found", found.to_string())
    .with_context("understood", understood.to_string())
}

/// The XMP could not be parsed. The AURA sidecar is the source of truth anyway.
#[must_use]
pub fn xmp_unreadable(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        RENDER_XMP_UNREADABLE,
        Severity::Warning,
        Recovery::Fallback,
        detail,
        "The Lightroom settings file next to this photograph could not be read, so AURA used \
         its own copy of the edit instead.",
    )
}

/// The AURA sidecar is corrupt. This is the one that costs work.
#[must_use]
pub fn sidecar_unreadable(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        RENDER_SIDECAR_UNREADABLE,
        Severity::ItemFailed,
        Recovery::AskUser,
        detail,
        "The saved edit for this photograph could not be read. The original photograph is \
         untouched; the edit will have to be made again.",
    )
}

/// An automated pass tried to change something a person set. Section 6.4.
#[must_use]
pub fn user_field_protected(paths: &[String]) -> AuraError {
    AuraError::new(
        RENDER_USER_FIELD_PROTECTED,
        Severity::Warning,
        Recovery::Fallback,
        format!(
            "automated pass refused on protected paths: {}",
            paths.join(", ")
        ),
        "AURA left the adjustments you made exactly as they were and applied its own changes \
         only to the settings you had not touched.",
    )
    .with_context("count", paths.len().to_string())
    .with_context("paths", paths.join(","))
}

/// A re-render produced different pixels than the export recorded.
#[must_use]
pub fn hash_mismatch(expected: &str, found: &str) -> AuraError {
    AuraError::new(
        RENDER_HASH_MISMATCH,
        Severity::ItemFailed,
        Recovery::AskUser,
        format!("render hash {found} does not match the recorded {expected}"),
        "AURA cannot re-create this exported file exactly as it was delivered. The delivered \
         file itself is unaffected.",
    )
    .with_context("expected", expected)
    .with_context("found", found)
}

/// The camera has no calibration entry; the reference profile renders it.
#[must_use]
pub fn profile_unknown(camera: &str) -> AuraError {
    AuraError::new(
        RENDER_PROFILE_UNKNOWN,
        Severity::Degraded,
        Recovery::Fallback,
        format!("no camera profile for {camera}; using the reference profile"),
        "AURA does not have a colour profile for this camera yet, so it used a neutral one. \
         Colours may need a small adjustment.",
    )
    .with_context("camera", camera)
}

/// The whole frame would not fit the budget, so it was streamed.
#[must_use]
pub fn tiled_fallback(pixels: u64, budget_bytes: u64) -> AuraError {
    AuraError::new(
        RENDER_TILED_FALLBACK,
        Severity::Warning,
        Recovery::Fallback,
        format!("{pixels} px would exceed the {budget_bytes} byte render budget; streaming tiles"),
        "This photograph is large, so AURA developed it in pieces. The result is the same.",
    )
    .with_context("pixels", pixels.to_string())
    .with_context("budget_bytes", budget_bytes.to_string())
}

/// The two paths disagreed. Section 6.2 says this fails the build.
#[must_use]
pub fn parity_drift(stage: &str, max_delta: f32, tolerance: f32) -> AuraError {
    AuraError::new(
        RENDER_PARITY_DRIFT,
        Severity::RunBlocking,
        Recovery::Halt,
        format!("stage {stage}: max delta {max_delta} exceeds tolerance {tolerance}"),
        "AURA found that two ways of developing the same photograph gave different results, so \
         it stopped rather than deliver something it cannot reproduce.",
    )
    .with_context("stage", stage)
    .with_context("max_delta", format!("{max_delta:.6}"))
    .with_context("tolerance", format!("{tolerance:.6}"))
}
