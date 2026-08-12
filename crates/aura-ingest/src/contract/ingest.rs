//! FROZEN CONTRACT. The ingest boundary. PHASE-02 consumes `ImportedFile` to
//! build the preview pyramid; PHASE-30 consumes `ImportReport` for delivery
//! provenance. Neither may reach inside the importer.

use std::path::PathBuf;

use aura_core::{ContentHash, FileId, ImportId, PhotoId, ProjectId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// What the photographer asked for. Built by the UI, validated once, then
/// handed to the importer. An `ImportPlan` is serialisable so a killed import
/// can be resumed from the plan plus the catalog, with no UI involvement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportPlan {
    /// Identifies this run in `import_run` and in the journal.
    pub import_id: ImportId,
    /// The project the files land in.
    pub project_id: ProjectId,
    /// Folders to walk. Symlinks are never followed.
    pub roots: Vec<PathBuf>,
    /// Copy files into a managed folder, or leave them where they are.
    pub mode: ImportMode,
    /// Extensions to consider, lower case without dots. Empty means the
    /// built-in list. Never a wildcard: unknown formats must be an explicit
    /// decision, not an accident.
    pub extensions: Vec<String>,
    /// Extract embedded JPEG previews during import for instant thumbnails.
    pub extract_embedded_previews: bool,
    /// Treat files whose mtime is within this window as still being written.
    pub settle_window_ms: u64,
}

/// Whether AURA takes custody of the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportMode {
    /// Reference in place. The photographer's folder structure is untouched.
    Reference,
    /// Copy into the project folder, verify the copy by hash, then reference.
    CopyVerified,
}

/// One physical file that made it into the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedFile {
    /// Catalog id of the file row.
    pub file_id: FileId,
    /// The photograph this file belongs to.
    pub photo_id: PhotoId,
    /// Absolute path on this machine. Class C2; never egressed.
    pub abs_path: PathBuf,
    /// What kind of file it is.
    pub kind: FileKind,
    /// How it relates to its photograph.
    pub role: FileRole,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Full-content BLAKE3 digest.
    pub content_hash: ContentHash,
    /// Filesystem modification time.
    pub mtime: OffsetDateTime,
    /// True when this exact file was already in the catalog.
    pub was_already_present: bool,
}

/// The format family of a file, decided by extension and never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    /// Camera RAW.
    Raw,
    /// JPEG.
    Jpeg,
    /// HEIF, HEIC or AVIF.
    Heif,
    /// TIFF.
    Tiff,
    /// PNG.
    Png,
    /// XMP sidecar.
    SidecarXmp,
    /// Movie file.
    Video,
    /// Anything else that matched the allow list.
    Other,
}

/// How a file relates to the photograph it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileRole {
    /// The file the pipeline reads.
    Primary,
    /// The camera JPEG beside a RAW.
    CompanionJpeg,
    /// An XMP sidecar.
    Sidecar,
    /// Something AURA produced.
    Derivative,
}

/// The honest account of what happened. Every number here is shown to the
/// photographer; none of them are estimates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportReport {
    /// Files the walker accepted for consideration.
    pub files_discovered: u64,
    /// File rows newly created.
    pub files_imported: u64,
    /// Files already in the catalog with unchanged bytes.
    pub files_already_present: u64,
    /// Files set aside with a reason.
    pub files_quarantined: u64,
    /// Photographs newly created.
    pub photos_created: u64,
    /// Bytes actually read by the hasher.
    pub bytes_hashed: u64,
    /// Wall clock for the whole run.
    pub duration_ms: u64,
    /// Grouped by error code so the UI can say "3 files unreadable" rather
    /// than listing three identical dialogs.
    pub quarantine_by_code: Vec<(String, u64)>,
}

/// Lifecycle of an import run, mirrored in the `import_run` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportState {
    /// Walking the roots.
    Scanning,
    /// Hashing and inserting.
    Importing,
    /// Stopped by the user, resumable.
    Paused,
    /// Finished normally.
    Completed,
    /// Stopped by an error that ends the run.
    Failed,
    /// Cancelled by the user.
    Cancelled,
}

impl ImportState {
    /// The text form stored in the catalog.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scanning => "scanning",
            Self::Importing => "importing",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}
