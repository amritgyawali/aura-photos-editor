//! Typed rows. Nothing outside this crate writes raw SQL against the schema.

use serde::{Deserialize, Serialize};

/// A wedding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRow {
    /// Prefixed id, `prj_...`.
    pub project_id: String,
    /// Photographer's name for the job.
    pub name: String,
    /// Optional couple label, class C2.
    pub couple_label: Option<String>,
    /// Event date, date only.
    pub event_date: Option<String>,
    /// IANA timezone used for timeline ordering.
    pub timezone: String,
    /// `active`, `archived` or `delivered`.
    pub status: String,
    /// RFC-3339 UTC creation stamp.
    pub created_at: String,
    /// RFC-3339 UTC update stamp.
    pub updated_at: String,
}

/// One camera body seen in a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraRow {
    /// Prefixed id, `cam_...`.
    pub camera_id: String,
    /// Owning project.
    pub project_id: String,
    /// Manufacturer as recorded in EXIF.
    pub make: Option<String>,
    /// Model as recorded in EXIF.
    pub model: Option<String>,
    /// Body serial; the clustering key for clock alignment.
    pub body_serial: String,
    /// `primary`, `second` or free text.
    pub shooter_label: String,
    /// Milliseconds to add to this body's EXIF time to reach the reference clock.
    pub clock_offset_ms: i64,
    /// Confidence in the estimate, 0.0 to 1.0.
    pub offset_confidence: f64,
    /// `none`, `estimated` or `manual`.
    pub offset_source: String,
    /// Frames attributed to this body.
    pub photo_count: i64,
}

/// Where a capture time came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureTimeSource {
    /// `DateTimeOriginal`.
    ExifOriginal,
    /// `DateTimeDigitized`.
    ExifDigitized,
    /// Filesystem modification time, used only when EXIF has nothing.
    FileMtime,
    /// No usable time at all.
    Unknown,
}

impl CaptureTimeSource {
    /// The text form stored in the catalog.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExifOriginal => "exif_original",
            Self::ExifDigitized => "exif_digitized",
            Self::FileMtime => "file_mtime",
            Self::Unknown => "unknown",
        }
    }
}

/// One photograph, which may own several files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhotoRow {
    /// Prefixed id, `pht_...`.
    pub photo_id: String,
    /// Owning project.
    pub project_id: String,
    /// EXIF capture time in RFC-3339 UTC, if any.
    pub capture_time: Option<String>,
    /// Where `capture_time` came from.
    pub capture_time_source: CaptureTimeSource,
    /// Clock-aligned capture time; the authoritative ordering key.
    pub timeline_time: Option<String>,
    /// Camera manufacturer.
    pub camera_make: Option<String>,
    /// Camera model.
    pub camera_model: Option<String>,
    /// Camera body serial, when the maker records one.
    pub camera_serial: Option<String>,
    /// Lens model.
    pub lens_model: Option<String>,
    /// ISO speed.
    pub iso: Option<i64>,
    /// Aperture as an f-number.
    pub aperture: Option<f64>,
    /// Shutter time in microseconds; integer so burst grouping never compares floats.
    pub shutter_us: Option<i64>,
    /// Focal length in millimetres.
    pub focal_len_mm: Option<f64>,
    /// Whether the flash fired.
    pub flash_fired: Option<bool>,
    /// EXIF orientation, 1 to 8.
    pub orientation: i64,
    /// Pixel width.
    pub width_px: Option<i64>,
    /// Pixel height.
    pub height_px: Option<i64>,
    /// Sub-second field, used to break ties inside a burst.
    pub sub_sec: Option<i64>,
}

/// What kind of file this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    /// A camera RAW file.
    Raw,
    /// A JPEG, usually the companion of a RAW.
    Jpeg,
    /// HEIF or HEIC.
    Heif,
    /// TIFF.
    Tiff,
    /// PNG.
    Png,
    /// An XMP sidecar.
    SidecarXmp,
    /// A movie file.
    Video,
    /// Anything else we chose to index.
    Other,
}

impl FileKind {
    /// The text form stored in the catalog.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Jpeg => "jpeg",
            Self::Heif => "heif",
            Self::Tiff => "tiff",
            Self::Png => "png",
            Self::SidecarXmp => "sidecar_xmp",
            Self::Video => "video",
            Self::Other => "other",
        }
    }
}

/// How a file relates to its photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileRole {
    /// The file the pipeline reads.
    Primary,
    /// The camera JPEG paired with a RAW.
    CompanionJpeg,
    /// An XMP sidecar.
    Sidecar,
    /// Something AURA produced.
    Derivative,
}

impl FileRole {
    /// The text form stored in the catalog.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::CompanionJpeg => "companion_jpeg",
            Self::Sidecar => "sidecar",
            Self::Derivative => "derivative",
        }
    }
}

/// One file on disk, content addressed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhotoFileRow {
    /// Prefixed id, `fil_...`.
    pub file_id: String,
    /// Owning photo.
    pub photo_id: String,
    /// Owning project.
    pub project_id: String,
    /// Source root this path is relative to.
    pub root_id: String,
    /// Path relative to the root, forward slashes.
    pub rel_path: String,
    /// File name including extension.
    pub file_name: String,
    /// Lower-case extension without the dot.
    pub ext: String,
    /// What kind of file this is.
    pub kind: FileKind,
    /// How it relates to its photo.
    pub role: FileRole,
    /// Size in bytes.
    pub size_bytes: i64,
    /// BLAKE3 hex digest, 64 characters.
    pub content_hash: String,
    /// Modification time, RFC-3339 UTC.
    pub mtime: String,
    /// `<size>:<mtime_ms>`; lets a rescan skip rehashing unchanged files.
    pub fast_key: String,
}

/// A source folder the photographer pointed at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRootRow {
    /// Prefixed id, `src_...`.
    pub root_id: String,
    /// Owning project.
    pub project_id: String,
    /// Absolute path, class C2, never egressed.
    pub abs_path: String,
    /// Volume label, when the platform reports one.
    pub volume_label: Option<String>,
    /// Volume serial, so a reconnected drive is recognised.
    pub volume_serial: Option<String>,
    /// Whether the volume is removable.
    pub is_removable: bool,
}

/// The row the grid renders. Deliberately small: 4,000 of these cross the IPC
/// boundary during an import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoGridRow {
    /// Photo id.
    pub id: String,
    /// Primary file name.
    pub file_name: String,
    /// Authoritative ordering time, if known.
    pub timeline_ts: Option<String>,
    /// Owning camera, if identified.
    pub camera_id: Option<String>,
    /// Pixel width, zero when not yet known.
    pub width: i64,
    /// Pixel height, zero when not yet known.
    pub height: i64,
    /// `indexed`, `missing` or `error`.
    pub status: String,
}

/// Progress of one path through the ingest pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalPhase {
    /// The walker has seen the path.
    Discovered,
    /// The content hash is known.
    Hashed,
    /// EXIF has been read.
    Exif,
    /// Rows are committed.
    Inserted,
    /// The path was already in the catalog.
    Skipped,
    /// The path was quarantined.
    Failed,
}

impl JournalPhase {
    /// The text form stored in the catalog.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Discovered => "discovered",
            Self::Hashed => "hashed",
            Self::Exif => "exif",
            Self::Inserted => "inserted",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

/// Counters for one import run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCounters {
    /// Files the walker found.
    pub files_discovered: u64,
    /// Files inserted as new photos or new files.
    pub files_imported: u64,
    /// Files already present by content hash.
    pub files_duplicate: u64,
    /// Files set aside with a reason.
    pub files_quarantined: u64,
    /// Bytes read by the hasher.
    pub bytes_hashed: u64,
}
