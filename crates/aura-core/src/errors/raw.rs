//! Decode-domain constructors. Codes 2000-2999, registered in `errors.toml`.
//!
//! Every one of these carries a fallback in its `Recovery`, because the phase 02
//! rule is that no single file may stop a wedding: a RAW that will not decode
//! becomes a Problems-list row, never a dialog and never a crash.

use std::path::Path;

use crate::contract::error::{AuraError, ErrorCode, Recovery, Severity};
use crate::redact::redact_path;

/// File is not a RAW format AURA can read.
pub const RAW_UNSUPPORTED: ErrorCode = ErrorCode("AURA-RAW-2001");
/// RAW file is corrupt or truncated.
pub const RAW_CORRUPT: ErrorCode = ErrorCode("AURA-RAW-2002");
/// No embedded preview; one was rendered instead.
pub const RAW_NO_EMBEDDED_PREVIEW: ErrorCode = ErrorCode("AURA-RAW-2003");
/// Decode exceeded its per-file time limit.
pub const RAW_TIMEOUT: ErrorCode = ErrorCode("AURA-RAW-2004");
/// Decode would exceed the per-file memory ceiling.
pub const RAW_TOO_LARGE: ErrorCode = ErrorCode("AURA-RAW-2005");
/// No colour profile for this camera; a generic matrix was used.
pub const RAW_PROFILE_MISSING: ErrorCode = ErrorCode("AURA-RAW-2006");
/// Mosaic compression unsupported; the proxy came from the embedded preview.
pub const RAW_MOSAIC_UNSUPPORTED: ErrorCode = ErrorCode("AURA-RAW-2007");
/// The decode worker stopped before returning a result.
pub const RAW_WORKER_LOST: ErrorCode = ErrorCode("AURA-RAW-2008");

/// The first bytes of the file match no container we know.
#[must_use]
pub fn unsupported(path: &Path, detail: impl Into<String>) -> AuraError {
    AuraError::new(
        RAW_UNSUPPORTED,
        Severity::ItemFailed,
        Recovery::Quarantine,
        detail,
        "AURA does not recognise this file as a photo it can develop. It has been set aside; \
         the rest of the import continues.",
    )
    .with_context("path", redact_path(path))
}

/// The container opened but its internal structure does not hold together.
#[must_use]
pub fn corrupt(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        RAW_CORRUPT,
        Severity::ItemFailed,
        Recovery::Quarantine,
        detail,
        "This photo file is damaged and could not be opened. Copy it from the card again, then \
         re-run the import.",
    )
}

/// The file has no embedded JPEG, so tier 1 had to render one.
#[must_use]
pub fn no_embedded_preview(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        RAW_NO_EMBEDDED_PREVIEW,
        Severity::Warning,
        Recovery::Fallback,
        detail,
        "This camera did not store a preview inside the file, so AURA rendered one. The first \
         thumbnails may appear more slowly.",
    )
}

/// The watchdog fired before the decode worker returned.
#[must_use]
pub fn timed_out(stage: &str, budget_ms: u64) -> AuraError {
    AuraError::new(
        RAW_TIMEOUT,
        Severity::ItemFailed,
        Recovery::Quarantine,
        format!("{stage} exceeded its {budget_ms} ms deadline"),
        "One photo took too long to open and was set aside so the rest could finish. You can \
         retry it from the Problems list.",
    )
    .with_context("stage", stage.to_string())
    .with_context("budget_ms", budget_ms.to_string())
}

/// The declared dimensions would allocate more than the ceiling allows.
#[must_use]
pub fn too_large(pixels: u64, ceiling: u64) -> AuraError {
    AuraError::new(
        RAW_TOO_LARGE,
        Severity::ItemFailed,
        Recovery::Quarantine,
        format!("declared {pixels} pixels exceeds the {ceiling} pixel ceiling"),
        "One photo is larger than AURA is allowed to open in one piece and was set aside. \
         Contact support with the camera model.",
    )
    .with_context("declared_pixels", pixels.to_string())
    .with_context("ceiling_pixels", ceiling.to_string())
}

/// No camera-specific matrix was available, so the generic one was used.
#[must_use]
pub fn profile_missing(make: &str, model: &str) -> AuraError {
    AuraError::new(
        RAW_PROFILE_MISSING,
        Severity::Degraded,
        Recovery::Fallback,
        format!("no colour profile for {make} {model}"),
        "AURA has no colour profile for this camera yet and used a generic one. Colours may \
         drift slightly until a profile ships.",
    )
    .with_context("make", make.to_string())
    .with_context("model", model.to_string())
}

/// The sensor data is present but its compression is not implemented.
#[must_use]
pub fn mosaic_unsupported(detail: impl Into<String>) -> AuraError {
    AuraError::new(
        RAW_MOSAIC_UNSUPPORTED,
        Severity::Degraded,
        Recovery::Fallback,
        detail,
        "AURA cannot develop this camera's raw data yet, so it used the picture the camera \
         stored inside the file. Editing is limited.",
    )
}

/// The worker channel closed without a result.
#[must_use]
pub fn worker_lost(stage: &str) -> AuraError {
    AuraError::new(
        RAW_WORKER_LOST,
        Severity::ItemFailed,
        Recovery::Quarantine,
        format!("{stage} worker ended without producing a result"),
        "One photo stopped the decoder and was set aside. AURA kept running and the rest of the \
         import is unaffected.",
    )
    .with_context("stage", stage.to_string())
}
