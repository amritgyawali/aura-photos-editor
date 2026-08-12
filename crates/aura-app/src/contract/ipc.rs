//! FROZEN CONTRACT. The IPC surface. `ui/src/ipc/types.ts` is generated from
//! these types and both files are digested in `contracts.lock`; changing either
//! without the other fails CI.

use serde::{Deserialize, Serialize};

/// Create a wedding project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    /// Photographer's name for the job.
    pub name: String,
    /// Optional couple label.
    pub couple_names: Option<String>,
    /// Optional event date, date only.
    pub event_date: Option<String>,
}

/// The id of a newly created project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHandle {
    /// Prefixed project id.
    pub id: String,
}

/// One row in the project switcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    /// Prefixed project id.
    pub id: String,
    /// Photographer's name for the job.
    pub name: String,
    /// Optional event date.
    pub event_date: Option<String>,
    /// Photographs indexed so far.
    pub photo_count: i64,
}

/// Start an import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartIngestInput {
    /// The project the files land in.
    pub project_id: String,
    /// Absolute folder paths to walk.
    pub roots: Vec<String>,
}

/// A handle to a running import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobHandle {
    /// Prefixed import id, used to cancel and to correlate events.
    pub job_id: String,
}

/// Page request for the virtualised grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListImagesInput {
    /// The project to read.
    pub project_id: String,
    /// Zero-based row offset.
    pub offset: i64,
    /// Page size; the UI asks for two screens at a time.
    pub limit: i64,
    /// `timeline` or `filename`.
    pub order_by: Option<String>,
}

/// One grid cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageRowLite {
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

/// Rename a body or nudge its clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCameraLabelInput {
    /// The camera row to update.
    pub camera_id: String,
    /// `primary`, `second` or free text.
    pub shooter_label: String,
    /// Milliseconds to add to this body's EXIF time.
    pub clock_offset_ms: i64,
}

/// One entry in the Problems list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemRow {
    /// Absolute path of the file that was set aside.
    pub path: String,
    /// Registered error code.
    pub code: String,
    /// Photographer-facing sentence.
    pub message: String,
}

/// Ask for the pixels of one photograph.
///
/// Added in PHASE-02; see `docs/adr/ADR-0005-preview-ipc-surface.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPreviewInput {
    /// The project the photograph belongs to.
    pub project_id: String,
    /// The photograph.
    pub photo_id: String,
    /// `thumb` or `proxy`. Full-resolution renders never cross the IPC boundary.
    pub level: String,
    /// `visible`, `interactive`, `aiBatch` or `background`.
    pub priority: String,
}

/// Pixels, encoded for the web view.
///
/// The bytes are a JPEG in a `data:` URL rather than an array, because a
/// 4,000-cell grid crossing the IPC boundary as JSON arrays of numbers costs
/// roughly eight times the bytes and a great deal of garbage collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPayload {
    /// The photograph these pixels belong to.
    pub photo_id: String,
    /// Which tier produced them: 1, 2 or 3.
    pub tier: i64,
    /// Width in pixels.
    pub width: i64,
    /// Height in pixels.
    pub height: i64,
    /// `embedded` when the camera rendered it, `decoded` when AURA did.
    pub source: String,
    /// A complete `data:image/jpeg;base64,...` URL.
    pub data_url: String,
}

/// Ask for several photographs to be produced in the background.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefetchInput {
    /// The project the photographs belong to.
    pub project_id: String,
    /// The photographs.
    pub photo_ids: Vec<String>,
    /// `thumb` or `proxy`.
    pub level: String,
}

/// What the cache is doing, for the settings panel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStatsDto {
    /// Bytes currently stored.
    pub bytes_used: u64,
    /// The ceiling those bytes are measured against.
    pub budget_bytes: u64,
    /// How many artefacts are stored.
    pub entries: u64,
    /// Lifetime hits.
    pub hits: u64,
    /// Lifetime misses.
    pub misses: u64,
    /// Lifetime evictions.
    pub evictions: u64,
    /// Hits divided by requests, 0.0 to 1.0.
    pub hit_rate: f64,
}

/// Change the cache ceiling from the settings panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCacheBudgetInput {
    /// The project whose cache is being resized.
    pub project_id: String,
    /// The new ceiling in bytes. Values below the floor are clamped, and the
    /// clamped value is what `preview_stats` reports back.
    pub budget_bytes: u64,
}

/// Preview progress pushed to the UI while thumbnails fill in.
///
/// Not `Eq`: the cache statistics carry a hit rate, and comparing two floats for
/// exact equality is banned by the lint block for good reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PreviewEvent {
    /// Pixels for one photograph are now available at this tier.
    Ready {
        /// The photograph.
        photo_id: String,
        /// Which tier finished.
        tier: i64,
    },
    /// One photograph could not be decoded; the rest continue.
    Failed {
        /// The photograph.
        photo_id: String,
        /// Registered error code.
        code: String,
        /// Photographer-facing sentence.
        message: String,
    },
    /// Periodic cache accounting, so the settings panel is live.
    CacheStats {
        /// Bytes stored.
        bytes_used: u64,
        /// The ceiling.
        budget_bytes: u64,
        /// Hits divided by requests.
        hit_rate: f64,
    },
}

/// What the UI is told when a command fails. Never a Rust type name, never a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    /// Registered error code, for example `AURA-IO-1008`.
    pub code: String,
    /// One sentence for the photographer.
    pub message: String,
    /// Where to read more.
    pub runbook_url: String,
    /// Whether the UI may offer a retry button.
    pub retryable: bool,
}

impl From<aura_core::AuraError> for IpcError {
    fn from(err: aura_core::AuraError) -> Self {
        Self {
            code: err.code.0.to_string(),
            message: err.user_message.clone(),
            runbook_url: err.code.runbook_url(),
            retryable: err.is_retryable(),
        }
    }
}

/// Progress and completion events pushed to the UI during an import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IngestEvent {
    /// The walker has an estimate of how much work there is.
    Discovered {
        /// Files the walker expects to consider.
        total_hint: u64,
    },
    /// Periodic progress during hashing and insertion.
    Progress {
        /// Files finished.
        done: u64,
        /// Files expected.
        total: u64,
        /// Already redacted description of the current unit.
        current: String,
    },
    /// A batch of rows is now visible in the catalog.
    Batch {
        /// The rows the grid should add.
        rows: Vec<ImageRowLite>,
    },
    /// Something went wrong for one file; the run continues.
    Warning {
        /// Registered error code.
        code: String,
        /// Photographer-facing sentence.
        message: String,
    },
    /// The run ended, for any reason.
    Finished {
        /// File rows created.
        inserted: u64,
        /// Files already present.
        skipped: u64,
        /// Files set aside.
        failed: u64,
        /// Wall clock for the run.
        elapsed_ms: u64,
    },
}
