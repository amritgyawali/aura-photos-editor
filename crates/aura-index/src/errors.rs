//! The four failures this crate can have, and what each one falls back to.
//!
//! Codes live in the `ML` range because an embedding is a model output and the
//! error taxonomy allocates one range per domain, not one per crate. All four are
//! registered in `crates/aura-core/errors.toml` with a runbook.
//!
//! None of them stops a wedding. An index that cannot be built is a wedding that
//! is culled without "find similar"; an embedding that comes back degenerate is
//! one photograph the grouping stages treat as unique. Invariant 9: a typed
//! error, a fallback path, and a telemetry event, every time.

use aura_core::contract::error::{AuraError, ErrorCode, Recovery, Severity};

/// The embedding model returned an all-zero or non-finite vector.
pub const ML_EMBED_DEGENERATE: ErrorCode = ErrorCode("AURA-ML-5013");
/// The index snapshot on disk could not be trusted and was discarded.
pub const ML_SNAPSHOT_UNUSABLE: ErrorCode = ErrorCode("AURA-ML-5014");
/// Stored vectors were produced by a different model or preprocessing version.
pub const ML_EMBED_VERSION_MISMATCH: ErrorCode = ErrorCode("AURA-ML-5015");
/// The project has more images than the in-memory index is documented to hold.
pub const ML_INDEX_TOO_LARGE: ErrorCode = ErrorCode("AURA-ML-5016");

/// A vector that is all zeros, or contains a value that is not finite.
///
/// Quarantine rather than retry: the same pixels through the same model produce
/// the same vector, so retrying is a way of doing nothing twice. The photograph
/// stays in the catalog and stays visible; it is simply not a candidate for
/// similarity until it is re-embedded.
#[must_use]
pub fn embed_degenerate(image: &str, reason: &str) -> AuraError {
    AuraError::new(
        ML_EMBED_DEGENERATE,
        Severity::ItemFailed,
        Recovery::Quarantine,
        format!("embedding for {image} is unusable: {reason}"),
        "AURA could not read one photograph well enough to compare it with the others. It is \
         still in your catalog and still exports normally.",
    )
    .with_context("image", image)
    .with_context("reason", reason)
}

/// The snapshot file is missing, truncated, from another version, or its digest
/// does not match its body.
///
/// A warning, because the fallback is complete: rebuild the graph from the
/// catalog rows, which costs the build budget once. The snapshot is a cache and
/// is treated exactly as the preview cache is - self-healing, never trusted
/// blindly, and never the reason a project will not open.
#[must_use]
pub fn snapshot_unusable(reason: &str) -> AuraError {
    AuraError::new(
        ML_SNAPSHOT_UNUSABLE,
        Severity::Warning,
        Recovery::Fallback,
        format!("index snapshot rejected: {reason}"),
        "AURA is rebuilding its similarity index. This takes a moment and then everything is as \
         it was.",
    )
    .with_context("reason", reason)
}

/// Stored vectors disagree with the model version this build would use.
///
/// Degraded rather than blocking: comparing a vector from one model with a vector
/// from another produces a number, and the number is meaningless. Section 12 of
/// the phase document calls a silent mismatch the failure mode worth engineering
/// against, so the rows are kept, the mismatch is reported, and a background
/// re-embed replaces them with progress the photographer can see.
#[must_use]
pub fn embed_version_mismatch(stored: u16, expected: u16, rows: usize) -> AuraError {
    AuraError::new(
        ML_EMBED_VERSION_MISMATCH,
        Severity::Degraded,
        Recovery::Fallback,
        format!("{rows} rows were embedded by version {stored}, this build uses {expected}"),
        "AURA has improved how it compares photographs, so it is re-reading this wedding in the \
         background. Grouping stays available while it works.",
    )
    .with_context("stored_version", stored.to_string())
    .with_context("expected_version", expected.to_string())
    .with_context("rows", rows.to_string())
}

/// The project is larger than the documented in-memory ceiling.
///
/// Section 12 of the phase document asks for "a documented ceiling" rather than a
/// silent slowdown into swap. The fallback is a flat scan, which is slower per
/// query but has no graph to hold: correctness is never traded for size.
#[must_use]
pub fn index_too_large(vectors: usize, ceiling: usize) -> AuraError {
    AuraError::new(
        ML_INDEX_TOO_LARGE,
        Severity::Degraded,
        Recovery::Fallback,
        format!("{vectors} vectors exceeds the in-memory ceiling of {ceiling}"),
        "This project is larger than AURA keeps in memory for instant comparisons, so finding \
         similar photographs will take a little longer.",
    )
    .with_context("vectors", vectors.to_string())
    .with_context("ceiling", ceiling.to_string())
}
