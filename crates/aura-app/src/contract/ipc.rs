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

/// One execution provider and what is known about it.
///
/// Added in PHASE-03; see `docs/adr/ADR-0008-inference-ipc-surface.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderNoteDto {
    /// `tensorrt`, `cuda`, `directml`, `coreml` or `cpu`.
    pub ep: String,
    /// A sentence for the panel: why it is unavailable, or why it was set aside.
    pub reason: String,
}

/// How fast a provider was, when it was measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeScoreDto {
    /// Which provider.
    pub ep: String,
    /// Median of the probe runs, in milliseconds.
    pub median_ms: f32,
}

/// What the hardware probe decided, for Settings > Hardware.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwarePlanDto {
    /// Accelerator name, when one was found.
    pub gpu: Option<String>,
    /// Providers that will be tried, most preferred first.
    pub ep_order: Vec<String>,
    /// The one that will actually answer the next request.
    pub selected_ep: String,
    /// The user's override, when they set one.
    pub override_ep: Option<String>,
    /// Providers that are not present, each with a reason.
    pub unavailable: Vec<ProviderNoteDto>,
    /// Providers set aside on this machine after a failed check.
    pub set_aside: Vec<ProviderNoteDto>,
    /// Memory ceiling the scheduler admits work against, in megabytes.
    pub vram_budget_mb: u32,
    /// Worker threads for processor-side work.
    pub cpu_threads: u16,
    /// Probe timings, one per measured provider.
    pub probe_scores_ms: Vec<ProbeScoreDto>,
    /// RFC 3339 timestamp of the measurement.
    pub probed_at: String,
    /// False when this is the conservative plan rather than a measurement.
    pub probed: bool,
}

/// One pinned model and what is installed of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatusDto {
    /// Registry name.
    pub name: String,
    /// Pinned version.
    pub version: String,
    /// What the model does, in one phrase.
    pub task: String,
    /// The version in use, once it has proved itself.
    pub active_version: Option<String>,
    /// A version installed but not yet proved.
    pub pending_version: Option<String>,
    /// Versions that failed their first real use here.
    pub rejected_versions: Vec<String>,
    /// Repository-relative path of the model card.
    pub model_card: String,
    /// Declared peak working set per image, in megabytes.
    pub working_set_mb: u32,
    /// Shipped files for this version.
    pub file_count: u32,
    /// True when this model may not be quantised to int8.
    pub int8_forbidden: bool,
}

/// What warmup did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarmupReportDto {
    /// Models loaded.
    pub loaded: u32,
    /// Wall clock for the whole warmup.
    pub elapsed_ms: u64,
    /// Which provider answered.
    pub ep_used: String,
}

/// Runtime counters for the settings panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferStatsDto {
    /// Sessions currently resident in the pool.
    pub resident_sessions: u32,
    /// Requests served from a resident session.
    pub pool_hits: u64,
    /// Requests that had to load a model.
    pub pool_loads: u64,
    /// Requests admitted by the scheduler.
    pub requests: u64,
    /// Times a batch had to shrink to fit memory.
    pub downshifts: u64,
    /// Mean admission overhead, in milliseconds. Budgeted at 0.4 ms.
    pub mean_overhead_ms: f32,
    /// Memory high-water mark, in megabytes.
    pub peak_memory_mb: u64,
}

/// Choose the execution provider by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetExecutionProviderInput {
    /// A provider name, or `auto` to return to the negotiated order.
    pub ep: String,
}

/// Progress and refusals pushed to the UI while models load.
///
/// Typed on both sides in PHASE-03 and not yet emitted, for the same reason
/// `IngestEvent` was not in phase 01: the Tauri shell has not been launched on
/// the development machine, so an emitter would be code nobody has run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InferEvent {
    /// One model finished loading during warmup.
    WarmupProgress {
        /// Models finished.
        done: u32,
        /// Models in this warmup.
        total: u32,
        /// The model that just finished.
        model: String,
    },
    /// The plan changed: a re-check, an override, or a provider set aside.
    PlanChanged {
        /// The provider that will answer from now on.
        selected_ep: String,
    },
    /// A model was refused by the integrity chain.
    ModelRejected {
        /// Registry name.
        name: String,
        /// Registered error code.
        code: String,
        /// Photographer-facing sentence.
        message: String,
    },
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
