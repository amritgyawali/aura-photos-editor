//! Filenames at a wedding are class C2: `Sarah-and-Tom-Wedding/getting-ready/IMG_0421.CR2`
//! identifies real people. Logs are class C1. Therefore paths are redacted before they
//! enter a log line, and the full path lives only in the local catalog.

use std::path::Path;

/// Keep the shape of a path for debugging, remove the identifying content.
#[must_use]
pub fn redact_path(path: &Path) -> String {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("none");
    let depth = path.components().count();
    format!("<redacted depth={depth} ext={ext}>")
}

/// For the rare case where a support engineer genuinely needs the filename,
/// the photographer opts in per diagnostics bundle. Never automatic.
#[must_use]
pub fn path_with_consent(path: &Path, consented: bool) -> String {
    if consented {
        path.display().to_string()
    } else {
        redact_path(path)
    }
}
