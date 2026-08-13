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

/// What Settings > AI Keys shows about the configured provider.
///
/// Added in PHASE-04; see `docs/adr/ADR-0010-cloud-ipc-surface.md`.
///
/// `keyFingerprint` is four characters from each end of the key and nothing in
/// between, so a photographer with three keys can tell which one is stored
/// without the panel ever holding the secret. There is no command that returns
/// the key itself, and there never will be.
///
/// The four booleans are deliberately separate rather than a flags enum, for the
/// same reason `ProjectConsent`'s five are: each one is a distinct promise with
/// its own switch in the panel, and a support engineer reading a bug report must
/// be able to see which of them was on.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatusDto {
    /// `anthropic`, `openai`, `google` or `compat`.
    pub provider: String,
    /// The endpoint in use. Never carries a key.
    pub endpoint: String,
    /// True when a key is stored for this provider.
    pub key_present: bool,
    /// `sk-a...9xQz`, or empty when no key is stored.
    pub key_fingerprint: String,
    /// Which credential store answered: `dpapi`, `keychain`, `libsecret`.
    pub key_store: String,
    /// The global switch. When true the crate is inert.
    pub offline_studio_mode: bool,
    /// The per-project switch.
    pub project_enabled: bool,
    /// Whether faces are blurred before upload.
    pub blur_faces: bool,
    /// Which transport is wired in: `http`, `cassette` or `offline`.
    pub transport: String,
    /// Set when the circuit breaker is open, saying why calls stopped.
    pub breaker_reason: Option<String>,
    /// Models the three tiers currently resolve to, cheapest first.
    pub tier_models: Vec<String>,
}

/// Store a key for one provider.
///
/// The key crosses the IPC boundary exactly once, on its way from the text field
/// to the operating system's credential store, and is never returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAiKeyInput {
    /// `anthropic`, `openai`, `google` or `compat`.
    pub provider: String,
    /// The key as pasted. Trimmed before it is stored.
    pub key: String,
    /// Endpoint override, for a compatible or region-pinned server.
    pub endpoint: Option<String>,
}

/// What the Check button found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyCheckDto {
    /// True when the provider accepted the key.
    pub ok: bool,
    /// The model that answered the probe, when one did.
    pub model: String,
    /// A sentence for the panel, whether it worked or not.
    pub message: String,
}

/// Set the spending caps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCloudBudgetInput {
    /// The project whose cap is being set.
    pub project_id: String,
    /// Ceiling for this job, in US dollars.
    pub cap_usd: f64,
    /// Ceiling for the calendar month, in US dollars.
    pub month_cap_usd: f64,
    /// When false, passing the cap warns instead of stopping.
    pub hard_stop: bool,
}

/// Set the privacy switches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCloudPrivacyInput {
    /// The project being changed.
    pub project_id: String,
    /// Cloud AI on for this job.
    pub enabled: bool,
    /// The global switch, which overrides the per-project one.
    pub offline_studio_mode: bool,
    /// Blur faces in every derivative before upload.
    pub blur_faces: bool,
}

/// The live spend meter.
///
/// Not `Eq`: money is a float here because the provider bills in fractions of a
/// cent, and comparing two of them for exact equality is banned by the lint block
/// for the same reason it is everywhere else.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSpendDto {
    /// This job's ceiling.
    pub cap_usd: f64,
    /// Billed against this job.
    pub spent_usd: f64,
    /// The month's ceiling.
    pub month_cap_usd: f64,
    /// Billed against the month.
    pub month_spent_usd: f64,
    /// Calls made for this job.
    pub calls: u64,
    /// Calls answered on a cheaper tier than the task asked for.
    pub downgrades: u64,
    /// Decisions answered by AURA's own models instead.
    pub fallbacks: u64,
    /// Cache hits over cache hits plus provider calls.
    pub cache_hit_rate: f64,
    /// True when the cap has stopped further calls.
    pub stopped: bool,
}

/// One row of the audit viewer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudCallDto {
    /// Identifier of the call, also carried on the decision it produced.
    pub id: String,
    /// Registry name of the task.
    pub task: String,
    /// Task version, which pins the prompt and the schema.
    pub task_version: u32,
    /// The model that answered, or `local`.
    pub model: String,
    /// `cloud`, `cache` or `local_fallback`.
    pub source: String,
    /// Set when the answer was local, saying why.
    pub fallback_reason: Option<String>,
    /// Prompt tokens billed.
    pub tokens_in: u32,
    /// Completion tokens billed.
    pub tokens_out: u32,
    /// What it cost, in US dollars.
    pub cost_usd: f64,
    /// Wall clock, including retries.
    pub latency_ms: u64,
    /// `ok`, `repaired`, `schema_invalid`, `fallback`, `cache` or `refused`.
    pub status: String,
    /// Repairs attempted. Zero or one.
    pub retry_count: u32,
    /// Hash of the prompt this answer was made from.
    pub prompt_hash: String,
    /// Confidence of the decision.
    pub confidence: f32,
    /// The decision this call justifies, when the caller supplied one.
    pub decision_ref: Option<String>,
}

/// What the response cache is holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudCacheStatsDto {
    /// Entries stored.
    pub entries: u64,
    /// Bytes of stored responses.
    pub bytes: u64,
    /// Lifetime hits.
    pub hits: u64,
}

/// Cloud events pushed to the UI, mirroring section 11's telemetry.
///
/// Typed on both sides in PHASE-04 and not yet emitted, for the same reason
/// `InferEvent` was not in phase 03: the Tauri shell has not been launched on the
/// development machine, so an emitter would be code nobody has run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CloudEvent {
    /// One call finished, billed or not.
    Call {
        /// Registry name of the task.
        task: String,
        /// The model that answered.
        model: String,
        /// What it cost.
        cost_usd: f64,
        /// Wall clock.
        latency_ms: u64,
        /// The outcome.
        status: String,
    },
    /// One decision used AURA's own models instead.
    Fallback {
        /// Registry name of the task.
        task: String,
        /// Why.
        reason: String,
    },
    /// A cap stopped further calls.
    BudgetStop {
        /// The ceiling that was reached.
        cap_usd: f64,
        /// What had been spent.
        spent_usd: f64,
    },
    /// Periodic cache accounting, so the panel is live.
    Cache {
        /// Hits over lookups.
        hit_rate: f64,
        /// Entries stored.
        entries: u64,
        /// Bytes stored.
        bytes: u64,
    },
}

/// Ask what looks like one photograph.
///
/// Added in PHASE-05; see `docs/adr/ADR-0012-similarity-ipc-surface.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindSimilarInput {
    /// The project to search.
    pub project_id: String,
    /// The photograph to search around.
    pub photo_id: String,
    /// How many neighbours to return.
    pub k: u32,
    /// Only frames within plus or minus this many seconds, when set.
    pub time_window_s: Option<u32>,
    /// Only frames from this camera, when set. The catalog's `cam_...` id.
    pub camera_id: Option<String>,
    /// Frames a previous query already claimed.
    pub exclude: Vec<String>,
}

/// One neighbour, with everything the panel would otherwise need a second
/// round trip for.
///
/// `distance` and `similarity` are two spellings of one fact on purpose: the
/// conversion done once in Rust is the reason two panels cannot disagree about
/// which direction is better.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarNeighbourDto {
    /// The neighbour.
    pub photo_id: String,
    /// Cosine distance, zero is identical.
    pub distance: f32,
    /// `1 - distance`, clamped to `0..1`. The number a human reads.
    pub similarity: f32,
    /// Hamming distance between the two difference hashes, `0..=64`.
    pub dhash_distance: u32,
    /// True when the difference hash calls this the same photograph. Evidence,
    /// not a verdict: the duplicate policy is phase 08's.
    pub near_duplicate: bool,
}

/// The answer to one similarity query, and the cost of asking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarResultDto {
    /// The photograph that was asked about.
    pub photo_id: String,
    /// Neighbours, nearest first.
    pub neighbours: Vec<SimilarNeighbourDto>,
    /// Wall clock. A float because the budget is 5 ms, and a 5 ms budget measured
    /// in whole milliseconds is measured in units of itself.
    pub elapsed_ms: f32,
    /// `none`, `time`, `camera`, `scene` or `composite`.
    pub filter_kind: String,
}

/// What the similarity index is holding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatusDto {
    /// Vectors in the graph.
    pub vectors: u32,
    /// Photographs in the project.
    pub photos: u32,
    /// Embedded over total. The number that explains an empty result list.
    pub coverage: f32,
    /// Vectors that a filtered query can reach.
    pub filterable: u32,
    /// Model version the graph was built at.
    pub model_ver: u32,
    /// Versions still present in the catalog that are not the current one.
    pub stale_model_versions: Vec<u32>,
    /// Milliseconds the current graph took to build.
    pub build_ms: u32,
    /// True when the graph was loaded from a snapshot rather than rebuilt.
    pub from_snapshot: bool,
}

/// Embed everything in a project that has no current vector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedProjectInput {
    /// The project to walk.
    pub project_id: String,
}

/// What one embedding pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedProgressDto {
    /// Photographs that gained a vector.
    pub embedded: u32,
    /// Photographs that could not be embedded; each one is logged with a code.
    pub failed: u32,
    /// Photographs still without a current vector.
    pub remaining: u32,
    /// Wall clock for the pass.
    pub elapsed_ms: u64,
    /// Batches submitted to the runtime.
    pub batches: u32,
    /// True when the pass was stopped early.
    pub cancelled: bool,
}

/// The cheap descriptors of one photograph, for the debug panel's readout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DescriptorsDto {
    /// The photograph.
    pub photo_id: String,
    /// The difference hash as sixteen hex characters.
    pub dhash_hex: String,
    /// Mean luminance, `0..1`.
    pub luma_mean: f32,
    /// First percentile.
    pub luma_p1: f32,
    /// Median.
    pub luma_p50: f32,
    /// Ninety-ninth percentile.
    pub luma_p99: f32,
    /// Fraction of pixels at or below the black point.
    pub clip_lo: f32,
    /// Fraction of pixels at or above the white point.
    pub clip_hi: f32,
    /// Mean gradient magnitude.
    pub edge_energy: f32,
    /// Five dominant colours as `#rrggbb`, most frequent first.
    pub palette: Vec<String>,
    /// Which model version produced this frame's vector.
    pub model_ver: u32,
    /// Which pixel tier it was read from, and whether those pixels were the
    /// camera's JPEG or AURA's render.
    pub pixel_source: String,
}

/// Similarity events pushed to the UI, mirroring section 11's three events.
///
/// Typed on both sides in PHASE-05 and not yet emitted, for the same reason
/// `IngestEvent`, `InferEvent` and `CloudEvent` are not: the Tauri shell has not
/// been launched on the development machine, so an emitter would be code nobody
/// has run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IndexEvent {
    /// `embed.batch` - one batch finished.
    EmbedProgress {
        /// Photographs finished.
        done: u64,
        /// Photographs in this pass.
        total: u64,
        /// Size of the batch just submitted.
        batch_size: u32,
        /// Which provider ran it.
        ep: String,
    },
    /// `index.build` - the graph is ready.
    IndexBuilt {
        /// Vectors in it.
        vectors: u32,
        /// Milliseconds it took.
        ms: u32,
        /// True when a snapshot was loaded rather than a graph built.
        snapshot_used: bool,
    },
    /// `index.query` - one query finished. Never the filter's contents: a time
    /// window plus a camera identifies a shoot.
    QueryTimed {
        /// Neighbours asked for.
        k: u32,
        /// Wall clock.
        ms: f32,
        /// `none`, `time`, `camera`, `scene` or `composite`.
        filter_kind: String,
    },
}

// ---------------------------------------------------------------------------
// PHASE-06. People. See `docs/adr/ADR-0014-people-ipc-surface.md`.
//
// Three rules shape this half of the surface, and the third one is the reason it
// looks different from the similarity surface.
//
// **No template, ever.** There is no `get_face_embedding`. A 512-d recognition
// template is a biometric identifier, a web view has no use for a number it cannot
// compare, and a template in a JSON payload is a template in a crash log. Every
// comparison happens in Rust.
//
// **A face crop is a photograph of a person, so it is behind a command that says
// so.** `identity_cover` decodes exactly one sealed crop and returns it as a data
// URL. It is the only route from the sealed store to a screen, and it exists because
// a People panel with no faces in it is unusable.
//
// **Every DTO that carries a decision carries its reasons.** Invariant 2 is not
// satisfied by a confidence alone, and the People panel is the surface where a
// photographer is most likely to disagree with the product - so `roleReasons` and a
// face's `reasons` are on the wire rather than reconstructed in the UI.
// ---------------------------------------------------------------------------

/// What the People panel opens with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeopleStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs scanned for faces.
    pub scanned: u32,
    /// Scanned over total. The number that explains an empty panel.
    pub coverage: f32,
    /// Faces found.
    pub faces: u32,
    /// Faces that passed the quality gate and may vote on identity.
    pub voting_faces: u32,
    /// Identities the grouping produced.
    pub identities: u32,
    /// Frames that earned the tiled detection pass.
    pub tiled_frames: u32,
    /// True until a photographer has confirmed one half of the couple.
    pub couple_unconfirmed: bool,
    /// Detector and recogniser versions still present that are not the current pair,
    /// as `detector*1000 + recogniser`.
    pub stale_versions: Vec<u32>,
    /// True when this project's biometric data has been erased.
    pub erased: bool,
    /// Which credential store holds the key, for the Settings panel.
    pub key_store: String,
    /// Which prominence weight table version produced the scores.
    pub weights_ver: u32,
}

/// Somebody this wedding photographed, for one card in the People panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityCardDto {
    /// Prefixed identity id.
    pub id: String,
    /// The photographer's name for them, when they gave one.
    pub label: Option<String>,
    /// `bride`, `groom`, `couple`, `family_close`, `family_extended`, `vip`, `guest`,
    /// `vendor`, `child` or `unknown`.
    pub role: String,
    /// How sure, `0..1`.
    pub role_confidence: f32,
    /// Why. Never empty above zero confidence.
    pub role_reasons: Vec<String>,
    /// True when a photographer set the role and automation may not change it.
    pub user_locked: bool,
    /// The importance slider, `0..1`. 0.5 is neutral.
    pub importance: f32,
    /// Faces assigned.
    pub faces: u32,
    /// Faces that may vote on identity.
    pub voting_faces: u32,
    /// Distinct photographs.
    pub frames: u32,
    /// First appearance, RFC 3339.
    pub first_seen: Option<String>,
    /// Last appearance, RFC 3339.
    pub last_seen: Option<String>,
    /// Mean fused quality of their faces.
    pub mean_quality: f32,
    /// The face to show on the card, best quality first.
    pub cover_face_id: Option<String>,
    /// Mean member-to-centroid distance. High means two looks of one person.
    pub variance: f32,
    /// Sub-centroids this identity grew for a change of look.
    pub sub_count: u32,
    /// Everybody they were photographed with, most often first.
    pub companions: Vec<CompanionDto>,
}

/// One entry in an identity's "seen with" list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionDto {
    /// The other identity.
    pub id: String,
    /// Their name, when they have one.
    pub label: Option<String>,
    /// Photographs both appear in.
    pub frames: u32,
}

/// One face box, with the quality verdict that decided whether it votes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceBoxDto {
    /// Prefixed face id.
    pub face_id: String,
    /// Who it belongs to, when it belongs to anybody.
    pub identity_id: Option<String>,
    /// Left edge, normalised.
    pub x: f32,
    /// Top edge, normalised.
    pub y: f32,
    /// Width, normalised.
    pub w: f32,
    /// Height, normalised.
    pub h: f32,
    /// Detector objectness.
    pub det_score: f32,
    /// Fused usability.
    pub quality: f32,
    /// Blur, `0..1`.
    pub blur: f32,
    /// Occlusion, `0..1`.
    pub occlusion: f32,
    /// Yaw in degrees, positive towards the image's right.
    pub yaw: f32,
    /// Pitch in degrees, positive looking down.
    pub pitch: f32,
    /// Roll in degrees, positive clockwise in the frame.
    pub roll: f32,
    /// Height in source pixels.
    pub px_height: u32,
    /// True when this face may vote on identity.
    pub votes: bool,
    /// `full` or `tile`.
    pub found_by: String,
    /// Why it does or does not vote.
    pub reasons: Vec<String>,
}

/// One identity's prominence in one frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProminenceEntryDto {
    /// Whose.
    pub identity_id: String,
    /// Prominence within this frame, `0..1`.
    pub prominence: f32,
}

/// Who is in one photograph, and how much of it is about them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageSubjectsDto {
    /// The photograph.
    pub photo_id: String,
    /// Every face in the frame, strongest detection first.
    pub faces: Vec<FaceBoxDto>,
    /// The identity this photograph is most about.
    pub dominant: Option<String>,
    /// Per-identity prominence. An array rather than a map so the order is the
    /// server's and two panels cannot disagree about it.
    pub prominence: Vec<ProminenceEntryDto>,
    /// The prominence-weighted sharpness of the faces present. The number phases 09
    /// and 12 use instead of global sharpness.
    pub subject_focus_score: f32,
    /// People in the frame, including the ones whose faces are not visible.
    pub people_count: u32,
    /// Which weight table version produced these numbers.
    pub weights_ver: u32,
}

/// Scan a project for faces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanFacesInput {
    /// The project to walk.
    pub project_id: String,
}

/// What one face pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanFacesDto {
    /// Photographs looked at.
    pub scanned: u32,
    /// Faces found.
    pub faces: u32,
    /// Faces that may vote on identity.
    pub voting: u32,
    /// Faces below the gate. Stored, visible, and excluded from voting.
    pub gated: u32,
    /// Body boxes found.
    pub person_boxes: u32,
    /// Bodies with no visible face.
    pub headless: u32,
    /// Photographs that could not be scanned; each one is logged with a code.
    pub failed: u32,
    /// Photographs still unscanned.
    pub remaining: u32,
    /// Frames that earned the tiled pass.
    pub tiled_frames: u32,
    /// Tiled frames over scanned frames, `0..1`. Section 12's cost measurement.
    pub tile_ratio: f32,
    /// Wall clock for the pass.
    pub elapsed_ms: u64,
    /// True when the pass was stopped early.
    pub cancelled: bool,
}

/// Group a project's faces into identities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupPeopleInput {
    /// The project to group.
    pub project_id: String,
}

/// What one grouping pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupPeopleDto {
    /// Identities created.
    pub identities: u32,
    /// Faces assigned to one.
    pub assigned: u32,
    /// Faces deliberately left unassigned rather than guessed at.
    pub unassigned: u32,
    /// The clustering threshold used.
    pub threshold: f32,
    /// Merges the threshold allowed and cohesion verification refused.
    pub refused_merges: u32,
    /// Identities that grew sub-centroids for a change of look.
    pub sub_clustered: u32,
    /// The couple, as zero or two identity ids.
    pub couple: Vec<String>,
    /// The couple decision's confidence.
    pub couple_confidence: f32,
    /// True when the top two candidate pairs were within the ambiguity margin.
    pub couple_ambiguous: bool,
    /// True when no scene labels were available, so the couple rests on
    /// co-occurrence and frequency alone.
    pub scene_starved: bool,
    /// Photographer decisions replayed onto the new grouping.
    pub decisions_replayed: u32,
    /// Decisions whose faces had scattered too far to replay.
    pub decisions_orphaned: u32,
    /// Fraction of the project scanned, `0..1`.
    pub coverage: f32,
    /// Wall clock.
    pub elapsed_ms: u64,
}

/// Fold one identity into another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeIdentitiesInput {
    /// The survivor.
    pub a: String,
    /// The one absorbed.
    pub b: String,
}

/// Move faces out of an identity into a new one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitIdentityInput {
    /// The identity to split.
    pub identity_id: String,
    /// The faces to move. Must not be all of them.
    pub face_ids: Vec<String>,
}

/// Set a role, and lock it against automation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetIdentityRoleInput {
    /// The identity.
    pub identity_id: String,
    /// The role's stable text.
    pub role: String,
}

/// Rename an identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameIdentityInput {
    /// The identity.
    pub identity_id: String,
    /// The new name, or absent to clear it.
    pub label: Option<String>,
}

/// Move the importance slider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetIdentityImportanceInput {
    /// The identity.
    pub identity_id: String,
    /// The new importance, `0..1`. 0.5 is neutral.
    pub importance: f32,
}

/// The id of an identity a command created or kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityHandleDto {
    /// Prefixed identity id.
    pub id: String,
}

/// Erase a project's biometric data.
///
/// `confirm` must be the project id, spelled out. A destructive command whose only
/// argument is the thing it destroys is one mis-click from a support ticket that
/// cannot be undone, and this is the one operation in the product with no undo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EraseBiometricsInput {
    /// The project.
    pub project_id: String,
    /// The project id again, typed by the photographer.
    pub confirm: String,
}

/// What an erasure did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EraseBiometricsDto {
    /// Face rows deleted.
    pub faces: u32,
    /// Identity rows deleted.
    pub identities: u32,
    /// Sealed crop files deleted.
    pub crops: u32,
    /// True when the credential-store entry was removed.
    pub key_removed: bool,
}

/// A gap in somebody's coverage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageGapDto {
    /// Start, milliseconds since the epoch.
    pub from_ms: i64,
    /// End, milliseconds since the epoch.
    pub to_ms: i64,
    /// How long, in whole minutes.
    pub minutes: i64,
}

/// One identity's timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityTimelineDto {
    /// Whose.
    pub identity_id: String,
    /// First appearance, milliseconds since the epoch.
    pub first_ms: Option<i64>,
    /// Last appearance.
    pub last_ms: Option<i64>,
    /// Minutes between the two.
    pub span_minutes: i64,
    /// Distinct photographs.
    pub frames: u32,
    /// Gaps longer than forty-five minutes.
    pub gaps: Vec<CoverageGapDto>,
}

/// One sealed face crop, decoded for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceCropDto {
    /// The face.
    pub face_id: String,
    /// A `data:image/jpeg;base64,...` URL of the 112 px aligned crop.
    pub data_url: String,
}

/// People events pushed to the UI, mirroring section 11's four events.
///
/// Typed on both sides in PHASE-06 and not yet emitted, for the same reason
/// `IngestEvent`, `InferEvent`, `CloudEvent` and `IndexEvent` are not: the Tauri
/// shell has not been launched on the development machine, so an emitter would be
/// code nobody has run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PeopleEvent {
    /// `face.detect` - one frame finished.
    ScanProgress {
        /// Photographs finished.
        done: u64,
        /// Photographs in this pass.
        total: u64,
        /// Faces found so far.
        faces: u64,
        /// True when the frame just finished earned the tiled pass.
        tiled: bool,
    },
    /// `identity.cluster` - the identities are ready.
    IdentitiesGrouped {
        /// How many.
        identities: u32,
        /// Faces used to build the skeleton.
        faces_used: u32,
        /// The threshold used.
        threshold: f32,
        /// Milliseconds it took.
        ms: u32,
    },
    /// `identity.role_inferred` - one role was decided.
    RoleInferred {
        /// The identity.
        identity_id: String,
        /// The role's stable text.
        role: String,
        /// How sure.
        confidence: f32,
        /// How many kinds of evidence were cited.
        evidence_kinds: u32,
    },
    /// `people.user_edit` - the photographer changed something.
    UserEdit {
        /// `merge`, `split`, `rename`, `role` or `importance`.
        action: String,
        /// How many identities were touched.
        identities: u32,
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
