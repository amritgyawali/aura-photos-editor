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
