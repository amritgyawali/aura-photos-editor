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

// ---------------------------------------------------------------------------
// PHASE-07. The story surface: what each photograph is of, and how the day was
// structured. See `docs/adr/ADR-0016-story-ipc-surface.md`.
//
// Nine commands, thirteen types and one event. Nothing here can change a
// photograph: four commands change a chapter, one changes a label, and four are
// reads. Invariant 1 stays structural rather than remembered.
// ---------------------------------------------------------------------------

/// Ask for a project's ordered story.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryOutlineInput {
    /// The wedding.
    pub project_id: String,
}

/// One entry of a scene posterior's top three.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneScoreDto {
    /// One of the 22 scene slugs, or `unknown`.
    pub scene: String,
    /// Its posterior, `0..1`.
    pub score: f32,
}

/// What one photograph is of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneDto {
    /// The photograph.
    pub photo_id: String,
    /// The chosen scene slug, or `unknown` when the classifier abstained.
    pub scene: String,
    /// Photographer-facing name of that scene.
    pub scene_title: String,
    /// How sure, `0..1`. Present even when the label is `unknown`: an abstention
    /// at 0.46 and one at 0.05 are different situations.
    pub scene_conf: f32,
    /// Always three entries, padded with `unknown` at zero.
    pub top3: Vec<SceneScoreDto>,
    /// Attribute *names*, never the bitfield. ADR-0016 decision 3.
    pub attributes: Vec<String>,
    /// False when the attribute head did not run, which is not the same as no
    /// attributes being set.
    pub attributes_measured: bool,
    /// The named rite's slug, when one was named.
    pub ritual: Option<String>,
    /// How sure the rite is.
    pub ritual_conf: f32,
    /// `local`, `cloud` or `user`.
    pub source: String,
    /// Which classifier produced it.
    pub model_ver: u16,
}

/// One chapter of the wedding's story.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterDto {
    /// The chapter.
    pub segment_id: String,
    /// Position in the strip, zero-based.
    pub ordinal: u32,
    /// One of the nine chapter slugs.
    pub chapter: String,
    /// The photographer's own name, when they renamed it.
    pub label: Option<String>,
    /// Photographer-facing name: `label` when set, otherwise the chapter's title.
    pub title: String,
    /// First frame, milliseconds since the Unix epoch.
    pub start_ms: i64,
    /// Last frame, inclusive.
    pub end_ms: i64,
    /// Whole minutes.
    pub duration_minutes: i64,
    /// The scene most of its frames are.
    pub dominant_scene: String,
    /// How sure the chapter label is, `0..1`.
    pub confidence: f32,
    /// The frame that represents it.
    pub key_frame: String,
    /// Frames in the chapter.
    pub image_count: u32,
    /// Why this chapter. Stored, not re-derived; ADR-0016 decision 3.
    pub reasons: Vec<String>,
    /// True when the photographer renamed, split, merged or moved it.
    pub user_locked: bool,
    /// True when its confidence is below the review threshold.
    pub needs_review: bool,
}

/// A project's ordered story, with its coverage attached.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryOutlineDto {
    /// The chapters, earliest first.
    pub chapters: Vec<ChapterDto>,
    /// Fraction of the project carrying a scene label, `0..1`.
    ///
    /// On the DTO rather than behind a second call. A story drawn over a
    /// 40 %-classified wedding is a story about 40 % of a wedding, and a panel
    /// that has to ask twice will render first and ask later.
    pub coverage: f32,
    /// Chapter ids the photographer should look at.
    pub needs_review: Vec<String>,
    /// Which classifier the labels underneath came from.
    pub scene_ver: u16,
    /// Which ritual taxonomy named the rites.
    pub taxonomy_ver: u16,
}

/// Ask what one photograph is of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageSceneInput {
    /// The photograph.
    pub photo_id: String,
}

/// Rename a chapter, or change which chapter it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetChapterInput {
    /// The chapter.
    pub segment_id: String,
    /// One of the nine chapter slugs.
    pub chapter: String,
    /// The photographer's own name for it, or null to clear.
    pub label: Option<String>,
}

/// Move the boundary between a chapter and the one after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveBoundaryInput {
    /// The earlier of the two chapters.
    pub segment_id: String,
    /// The new end of that chapter, milliseconds since the Unix epoch.
    pub new_end_ms: i64,
}

/// Split a chapter in two at a photograph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitChapterInput {
    /// The chapter.
    pub segment_id: String,
    /// The first photograph of the new later half.
    pub photo_id: String,
}

/// Fold two adjacent chapters into one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeChaptersInput {
    /// One chapter.
    pub segment_id_a: String,
    /// The one immediately before or after it.
    pub segment_id_b: String,
}

/// The id of a chapter a command created or kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterHandleDto {
    /// Prefixed segment id.
    pub id: String,
}

/// One scene's tolerances, with the sentence explaining them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneProfileDto {
    /// The scene slug.
    pub scene: String,
    /// Photographer-facing name.
    pub title: String,
    /// Expected survival ratio, minimum.
    pub keeper_min: f32,
    /// Expected survival ratio, maximum.
    pub keeper_max: f32,
    /// Scene-relative noise tolerance, `0..1`.
    pub max_acceptable_noise: f32,
    /// Scene-relative blur tolerance, `0..1`.
    pub max_acceptable_blur: f32,
    /// How much subject sharpness dominates, `0..1`.
    pub subject_focus_weight: f32,
    /// How much expression matters, `0..1`.
    pub emotion_weight: f32,
    /// How much framing matters, `0..1`.
    pub composition_weight: f32,
    /// `airy`, `neutral`, `warm`, `moody` or `punchy`.
    pub editing_intent: String,
    /// True when phase 12's coverage guarantee applies.
    pub must_cover: bool,
    /// Why these numbers. Section 12's third failure mode, answered on the wire.
    pub rationale: String,
}

/// Start a scene classification pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifyScenesInput {
    /// The wedding.
    pub project_id: String,
}

/// What a classification or segmentation pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryStatusDto {
    /// Photographs in the project.
    pub photos: i64,
    /// Photographs carrying a scene label.
    pub classified: i64,
    /// Fraction, `0..1`.
    pub coverage: f32,
    /// Chapters in the story.
    pub chapters: u32,
    /// Chapters below the review threshold.
    pub needs_review: u32,
    /// Chapters the photographer has locked.
    pub locked: u32,
    /// The PELT penalty the last segmentation settled on.
    pub penalty: f32,
    /// True when the last segmentation fell back to time gaps alone.
    pub gaps_only: bool,
    /// Which classifier produced the labels.
    pub scene_ver: u16,
    /// Which taxonomy named the rites.
    pub taxonomy_ver: u16,
    /// How many rites are declared across every loaded taxonomy file.
    pub rituals_known: u32,
}

/// Story events pushed to the UI, mirroring section 11's four events.
///
/// Typed on both sides in PHASE-07 and not yet emitted, for the same reason
/// `PeopleEvent` is not: the Tauri shell has not been launched on the development
/// machine, so an emitter would be code nobody has run. The four are `tracing`
/// spans today and this is their wire shape for when the shell runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StoryEvent {
    /// `scene.classified` - a batch of frames finished.
    SceneClassified {
        /// Photographs classified so far.
        images: u64,
        /// Milliseconds so far.
        ms: u64,
        /// Mean top-1 posterior over the labelled frames.
        mean_conf: f32,
        /// Frames the classifier refused to label.
        low_conf_count: u64,
    },
    /// `story.segmented` - the chapters are ready.
    StorySegmented {
        /// How many chapters.
        segments: u32,
        /// The penalty the search settled on.
        boundary_penalty: f32,
        /// Distinct chapter kinds on the timeline.
        chapters: u32,
    },
    /// `story.user_edit` - the photographer changed something.
    StoryUserEdit {
        /// `rename`, `boundary`, `split` or `merge`.
        action: String,
        /// The chapter.
        segment: String,
        /// What it was, when the action changed a label.
        from_label: Option<String>,
        /// What it became.
        to_label: Option<String>,
    },
    /// `scene.cloud_used` - a naming pass finished.
    SceneCloudUsed {
        /// Chapters considered.
        segments: u32,
        /// Calls actually made.
        calls: u32,
        /// What they cost.
        cost_usd: f32,
    },
}

// ---------------------------------------------------------------------------
// PHASE-08. The moments surface: what the photographer shot once, and the
// near-identical frames inside it. See
// `docs/adr/ADR-0018-moments-ipc-surface.md`.
//
// Nine commands, eight types and one event. Nothing here can reject a
// photograph: five commands change a grouping, one moves a hint, and three are
// reads. Section 2.2 puts every question about a photograph's fate in phase 12,
// and this surface keeps that structural rather than remembered - there is no
// `cull`, no `reject` and no rank on the wire.
// ---------------------------------------------------------------------------

/// Ask for a project's moments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentsInput {
    /// The wedding.
    pub project_id: String,
    /// Only moments inside this chapter, when the caller wants one chapter's stacks.
    pub segment_id: Option<String>,
}

/// One frame inside a stacked cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentFrameDto {
    /// The photograph.
    pub photo_id: String,
    /// Position in the moment, in timeline order.
    pub position: u32,
    /// Which burst inside the moment. Frames sharing this came off one press of the
    /// shutter.
    pub burst_ix: u32,
    /// True when a duplicate set caps this frame out of the gallery unless it is the
    /// one kept.
    pub suppressed: bool,
}

/// One moment, as the grid's stacked cell draws it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentDto {
    /// The moment.
    pub moment_id: String,
    /// The chapter it sits in, when the wedding has been segmented.
    pub segment_id: Option<String>,
    /// The frame the stack shows when collapsed.
    ///
    /// The keep hint of the moment's first duplicate set when it has one, and the
    /// first frame otherwise. **Not a decision** - see `keepHint` on
    /// [`DuplicateSetDto`] and section 2.2.
    pub cover: String,
    /// Every frame, in timeline order.
    pub frames: Vec<MomentFrameDto>,
    /// How many frames. The count badge.
    pub frame_count: u32,
    /// How many presses of the shutter.
    pub burst_count: u32,
    /// How many bodies contributed. Two or more draws the two-shooter badge.
    pub camera_count: u32,
    /// First frame's timeline time, milliseconds since the Unix epoch.
    pub start_ms: i64,
    /// Last frame's timeline time.
    pub end_ms: i64,
    /// How long the photographer spent on it, in seconds.
    pub duration_s: i64,
    /// Mean pairwise appearance distance inside the moment, `0..1`.
    pub diversity: f32,
    /// How many keepers phase 12 may take before it is arguing with the evidence.
    pub suggested_keepers: u32,
    /// How sure the grouping is, `0..1`. Invariant 2.
    pub confidence: f32,
    /// Why these frames are one moment. Invariant 2.
    pub reasons: Vec<String>,
    /// True when the photographer split, merged or pinned this moment.
    pub user_locked: bool,
    /// How many duplicate sets constrain delivery from this moment.
    pub duplicate_sets: u32,
}

/// A project's moments, with the coverage the conclusion was drawn over.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentListDto {
    /// Every moment, earliest first.
    pub moments: Vec<MomentDto>,
    /// Fraction of the project's groupable frames that are in a moment, `0..1`.
    pub coverage: f32,
    /// Which embedding produced the distances underneath.
    pub embed_ver: u16,
    /// Which grouping implementation produced the moments.
    pub group_ver: u16,
    /// Which threshold table it read.
    pub profile_ver: u16,
}

/// Ask what moment a photograph is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentOfImageInput {
    /// The photograph.
    pub photo_id: String,
}

/// One set of frames that say the same thing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateSetDto {
    /// `identical`, `near_identical` or `variant`.
    pub kind: String,
    /// The frames, in timeline order.
    pub photo_ids: Vec<String>,
    /// The technically strongest frame.
    ///
    /// **A hint, not a decision.** Phase 12 chooses what a client sees; this is where
    /// the review panel puts the left-hand image and where "keep this one" starts.
    pub keep_hint: String,
    /// How sure the classification is, `0..1`.
    pub confidence: f32,
    /// Why these frames are the same photograph. Invariant 2.
    pub reasons: Vec<String>,
    /// True when the photographer pressed "keep this one".
    pub user_chosen: bool,
    /// True when at most one of these frames may reach a gallery.
    pub caps_gallery: bool,
}

/// Start a grouping pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMomentsInput {
    /// The wedding.
    pub project_id: String,
}

/// What a grouping pass did, and what the moments view header shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentStatusDto {
    /// Photographs in the project.
    pub photos: i64,
    /// Photographs that have an embedding and can therefore be grouped.
    pub groupable: i64,
    /// Photographs that are in a moment.
    pub grouped: i64,
    /// Fraction, `0..1`, against `groupable` rather than `photos` - an ungrouped frame
    /// with no embedding is a phase 05 gap, not a phase 08 failure.
    pub coverage: f32,
    /// Moments in the project.
    pub moments: i64,
    /// Moments the photographer has locked.
    pub locked: i64,
    /// Mean frames per moment. The headline number: 3,000 files should become roughly
    /// 700 to 1,100 moments.
    pub mean_size: f32,
    /// Presses of the shutter across every moment.
    pub bursts: i64,
    /// Duplicate sets by class: identical, near identical, variant.
    pub duplicates: [i64; 3],
    /// Median inter-frame interval across the wedding, in milliseconds.
    pub median_interval_ms: i64,
    /// True when the last pass produced a moment count outside the plausible band.
    /// `AURA-ML-5030`.
    pub implausible: bool,
    /// Which embedding produced the distances.
    pub embed_ver: u16,
    /// Which grouping implementation produced the moments.
    pub group_ver: u16,
    /// Which threshold table it read.
    pub profile_ver: u16,
}

/// Break a moment in two at a photograph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitMomentInput {
    /// The moment.
    pub moment_id: String,
    /// The photograph that starts the new later half.
    pub photo_id: String,
}

/// Fold two moments into one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeMomentsInput {
    /// The survivor.
    pub moment_id_a: String,
    /// The one absorbed.
    pub moment_id_b: String,
}

/// Pin a moment against re-analysis, or release it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockMomentInput {
    /// The moment.
    pub moment_id: String,
    /// True to pin, false to release.
    pub locked: bool,
}

/// Record which frame of a duplicate set the photographer would keep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetKeepHintInput {
    /// The moment.
    pub moment_id: String,
    /// The photograph to start from. Nothing is culled; see [`DuplicateSetDto`].
    pub photo_id: String,
}

/// The id of a moment a command created or kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentHandleDto {
    /// Prefixed moment id.
    pub id: String,
}

/// What an undo reversed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentEditDto {
    /// `split`, `merge`, `lock`, `unlock` or `keep_hint`.
    pub action: String,
    /// The moment the photographer acted on.
    pub moment_id: String,
    /// The other moment, for a merge or a split.
    pub other_id: Option<String>,
    /// The photograph, for a split or a keep-hint change.
    pub photo_id: Option<String>,
    /// How many frames the moment held at the time.
    pub moment_size: u32,
}

/// Moment events pushed to the UI, mirroring section 11's three events.
///
/// Typed on both sides in PHASE-08 and not yet emitted, for the same reason
/// `PeopleEvent` and `StoryEvent` are not: the Tauri shell has not been launched
/// on the development machine, so an emitter would be code nobody has run. The
/// three are `tracing` spans today and this is their wire shape for when the
/// shell runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MomentEvent {
    /// `moments.built` - a grouping pass finished.
    MomentsBuilt {
        /// Frames considered.
        images: u64,
        /// Moments written.
        moments: u64,
        /// Presses of the shutter across them.
        bursts: u64,
        /// Mean frames per moment.
        mean_size: f32,
        /// Milliseconds.
        ms: u64,
    },
    /// `duplicates.found` - the three counters, once per pass.
    DuplicatesFound {
        /// Sets of the same file imported twice.
        identical: u64,
        /// Sets of one photograph that exists more than once.
        near_identical: u64,
        /// Alternatives phase 12 chooses between.
        variant: u64,
    },
    /// `moments.user_edit` - the photographer changed a grouping.
    ///
    /// Carries the moment's size and never its frames: a telemetry event carrying
    /// photo ids is a telemetry event carrying a shoot.
    MomentsUserEdit {
        /// `split`, `merge`, `lock`, `unlock` or `keep_hint`.
        action: String,
        /// How many frames the moment held.
        moment_size: u32,
    },
}

// ---------------------------------------------------------------------------
// PHASE-09. The integrity surface: what is technically wrong with a photograph,
// where it is wrong, and what the photographer may say back. Frozen; see
// `docs/adr/ADR-0020-integrity-ipc-surface.md`.
//
// Six commands where phase 08 had nine, and the difference is the point: five of
// phase 08's changed a grouping and exactly one of these changes anything at all.
// `dismiss_flag` clears one mark the photographer disagrees with. There is no
// command on this surface that keeps, rejects, ranks or orders a photograph,
// because section 2.2 puts every one of those in phase 12 - and this is the
// surface where that boundary is most tempting to cross.
// ---------------------------------------------------------------------------

/// One rectangle in a photograph, normalised to the frame.
///
/// The wire form of `CropRect`. Four numbers rather than an object with a name,
/// because the panel does arithmetic with it - it is the zoom target - and a
/// named object would be four property lookups per frame of an animation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropRectDto {
    /// Left edge, `0..1`.
    pub x: f32,
    /// Top edge, `0..1`.
    pub y: f32,
    /// Width, `0..1`.
    pub w: f32,
    /// Height, `0..1`.
    pub h: f32,
}

/// One thing that moved a frame's score, with the pixels that prove it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityReasonDto {
    /// The stable slug. `docs/frame-integrity.md` documents every one.
    pub code: String,
    /// The sentence a photographer reads.
    pub text: String,
    /// Negative for a penalty, zero or positive for an exoneration.
    pub weight: f32,
    /// True when this reason withdraws a claim rather than making one.
    ///
    /// Derived from the code rather than from the weight, and sent rather than
    /// recomputed in the UI, so that the panel and the eval harness cannot
    /// disagree about which reasons are the good news.
    pub exoneration: bool,
    /// The crop to show. Absent when the reason is about the whole frame.
    pub evidence: Option<CropRectDto>,
}

/// One face's eyes, as the card lists them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EyeStateDto {
    /// The face.
    pub face_id: String,
    /// Who it is, when the identity pass has assigned one.
    pub identity_id: Option<String>,
    /// `open`, `squint`, `closed`, `looking_down` or `occluded`.
    pub state: String,
    /// How sure the head is, `0..1`.
    pub confidence: f32,
    /// True when the scene, the expression or the partner justifies a closure.
    pub intentional: bool,
    /// True when this face's eyes decide anything about the frame.
    pub gates: bool,
    /// Fraction of the frame this face covers.
    pub area_frac: f32,
    /// Where to zoom.
    pub crop: CropRectDto,
}

/// One photograph's technical verdict.
///
/// **Nothing in this shape is a decision about delivery.** `technicalScore` is a
/// measurement, `flags` are measurements, and a UI that sorted a delivery by
/// either would be making phase 12's decision three phases early.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityDto {
    /// The photograph.
    pub photo_id: String,
    /// Prominence-weighted subject sharpness, `0..1`.
    pub subject_sharpness: f32,
    /// Background sharpness, `0..1`.
    pub bg_sharpness: f32,
    /// Where the sharpest plane is, `-1..1`. Negative is front focus.
    pub focus_offset: f32,
    /// Where this frame sits among its moment's frames, `0..1`. One is the
    /// sharpest; `0.5` means it has no siblings.
    pub relative_sharpness: f32,
    /// `none`, `camera_shake`, `subject_motion` or `intentional`.
    pub motion: String,
    /// How much, `0..1`.
    pub motion_severity: f32,
    /// `good`, `recoverable`, `marginal` or `lost`.
    pub exposure: String,
    /// Fraction of pixels clipped at the top, speculars excluded.
    pub clip_hi: f32,
    /// Fraction crushed at the bottom.
    pub clip_lo: f32,
    /// Stops from a correct exposure. Negative is under.
    pub ev_offset: f32,
    /// Noise relative to what this scene tolerates. **1.0 is the tolerance.**
    pub noise_sigma_rel: f32,
    /// Fraction of the gating subjects whose eyes are closed.
    pub closed_eye_ratio: f32,
    /// How many faces gate this frame. The denominator of the ratio above, sent
    /// beside it because zero over zero and zero over six are different facts.
    pub gating_faces: u32,
    /// The scene-weighted composite, `0..1`.
    pub technical_score: f32,
    /// Which scene the thresholds were conditioned on.
    pub scene: String,
    /// The set flags, as slugs, in `IntegrityFlags::ALL` order.
    pub flags: Vec<String>,
    /// True when at least one flag describes a defect.
    ///
    /// Sent rather than derived in the UI, because "which of these fourteen are
    /// defects" is the one question the interface must not answer for itself:
    /// `intentional_motion` and `eyes_closed_ok` are the phase's whole argument.
    pub has_defect: bool,
    /// Why, strongest penalty first.
    pub reasons: Vec<IntegrityReasonDto>,
    /// Per-face eye states, in prominence order.
    pub eyes: Vec<EyeStateDto>,
    /// How sure the whole verdict is, `0..1`.
    pub confidence: f32,
    /// True when the photographer has dismissed a flag on this frame.
    pub user_reviewed: bool,
    /// Which learned heads produced it.
    pub model_ver: u16,
    /// Which build's arithmetic produced it.
    pub analysis_ver: u16,
    /// Which calibration table normalised it.
    pub calib_ver: u16,
}

/// What the Integrity panel's header shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs with a verdict.
    pub scored: u32,
    /// Fraction of the project with a verdict, `0..1`. **Denominator: every
    /// photograph**, unlike the moments view's.
    pub coverage: f32,
    /// Fraction of *scored* frames that had a subject to be judged against.
    ///
    /// A wedding at 100 % coverage and 2 % subject-aware has been judged on
    /// frame-wide sharpness nearly everywhere, which is the ordinary global
    /// measure this phase exists to replace. The header says so.
    pub subject_aware: f32,
    /// Frames the photographer has overruled.
    pub reviewed: u32,
    /// How many frames carry each flag, in `IntegrityFlags::ALL` order.
    pub flag_counts: Vec<u32>,
    /// The flag slugs, in the same order, so the chips do not hard-code them.
    pub flag_names: Vec<String>,
    /// Mean technical score over scored frames.
    pub mean_score: f32,
    /// An upper bound on the frames carrying a defect: one frame can be soft
    /// *and* noisy, so the counts overlap and this is a sum of them.
    pub defective_at_most: u32,
    /// Camera bodies with no calibration row.
    pub uncalibrated: Vec<String>,
    /// Which learned heads produced the numbers. The **lowest** present.
    pub model_ver: u16,
    /// Which build's arithmetic produced them.
    pub analysis_ver: u16,
    /// Which calibration table normalised them.
    pub calib_ver: u16,
}

/// Ask for the frames carrying any of these flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlaggedInput {
    /// The project.
    pub project_id: String,
    /// Flag slugs. Any of them matching is a hit.
    pub flags: Vec<String>,
    /// How many to return, worst score first.
    pub limit: Option<u32>,
}

/// Ask a moment's frames to be ranked by subject sharpness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithinMomentInput {
    /// The moment.
    pub moment_id: String,
}

/// One frame's place in its moment's sharpness ranking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedFrameDto {
    /// The photograph.
    pub photo_id: String,
    /// One for the sharpest of the moment, zero for the softest.
    pub relative_sharpness: f32,
}

/// Tell AURA that one technical mark is wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DismissFlagInput {
    /// The photograph.
    pub photo_id: String,
    /// Exactly one flag slug.
    pub flag: String,
}

/// Start a technical pass over a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyseIntegrityInput {
    /// The project.
    pub project_id: String,
    /// Cancel token id, when the caller wants to be able to stop it.
    pub cancel_id: Option<String>,
}

/// What a technical pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityPassDto {
    /// Photographs analysed.
    pub scored: u32,
    /// Photographs that could not be analysed. Each is logged with a code and
    /// **no row is written**, so the next pass tries again.
    pub failed: u32,
    /// Faces whose eyes were judged.
    pub faces: u32,
    /// Faces whose eyes were closed.
    pub closed: u32,
    /// Closures the intent rules justified.
    pub closed_ok: u32,
    /// Mean technical score over this pass.
    pub mean_score: f32,
    /// Bodies with no calibration row.
    pub uncalibrated: Vec<String>,
    /// Milliseconds.
    pub elapsed_ms: u64,
    /// True when the pass was stopped.
    pub cancelled: bool,
}

/// Integrity events pushed to the UI, mirroring section 11's three events.
///
/// Typed on both sides and not yet emitted, for the reason `MomentEvent`,
/// `StoryEvent` and `PeopleEvent` are not: the Tauri shell has not been launched
/// on the development machine, so an emitter would be code nobody has run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IntegrityEvent {
    /// `integrity.scored` - a pass finished.
    IntegrityScored {
        /// Frames analysed.
        images: u64,
        /// Milliseconds.
        ms: u64,
        /// Mean technical score.
        mean_score: f32,
        /// How many frames carry each flag, in `IntegrityFlags::ALL` order.
        flag_histogram: Vec<u32>,
    },
    /// `integrity.eyes` - the four eye counters, once per pass.
    IntegrityEyes {
        /// Faces judged.
        faces: u64,
        /// Eyes closed.
        closed: u64,
        /// Closures the intent rules justified.
        closed_ok: u64,
        /// Faces squinting.
        squint: u64,
    },
    /// `integrity.camera_uncalibrated` - one body, once per pass.
    IntegrityCameraUncalibrated {
        /// The make, as EXIF wrote it.
        make: String,
        /// The model.
        model: String,
    },
}

// ---------------------------------------------------------------------------
// PHASE-10. The emotion surface: what a photograph is worth, why, and what the
// photographer may say back. Frozen; see
// `docs/adr/ADR-0022-emotion-ipc-surface.md`.
//
// Seven commands. Five are reads, and the two that change anything are both the
// photographer telling the product it is wrong: `prefer_frame` records a pairwise
// comparison, and `set_moment_peak` overrides a peak choice.
//
// **There is no command here that keeps, delivers, or builds a gallery.**
// `ranked_by_emotion` comes closest and deliberately stops short: it returns an
// ordering, which is this phase's headline feature, and section 2.2 puts the
// choosing in phase 12. An ordering looks even more like a selection than phase
// 09's score did, which is why the boundary is restated on the types here.
//
// **The panel is told which reasons are caveats.** `EmotionReasonDto::caveat` is
// computed here and sent rather than derived in the interface from a list of
// slugs. Three of the twenty codes say how much to trust the number rather than
// what is in the photograph, and a UI that worked that out for itself would work
// it out wrong exactly once - which is the argument
// `IntegrityReasonDto::exoneration` already made one phase ago.
// ---------------------------------------------------------------------------

/// One thing that moved a frame's emotion score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmotionReasonDto {
    /// The stable slug. `docs/emotion-and-moments.md` documents every one.
    pub code: String,
    /// The sentence a photographer reads.
    pub text: String,
    /// Positive for something the frame earned, negative for something it cost.
    ///
    /// The opposite sign convention from phase 09's, and that is the two phases
    /// rather than an inconsistency: a technical verdict explains penalties and
    /// an emotion score explains what it found.
    pub weight: f32,
    /// True when this reason is a note about the reading rather than about the
    /// photograph.
    pub caveat: bool,
    /// The face to show. Absent when the reason is about the whole frame.
    pub evidence: Option<CropRectDto>,
}

/// One face's expression, as the card lists them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceExpressionDto {
    /// The face phase 06 found.
    pub face_id: String,
    /// Who it is, when the identity pass has assigned somebody.
    pub identity_id: Option<String>,
    /// The eight continuous channels, in `FaceExpression::channels` order.
    ///
    /// An array rather than eight named fields, because the card draws them as
    /// eight bars in a fixed order and `channelNames` beside it is what names
    /// them. Eight named fields would make adding a ninth a wire change in three
    /// places.
    pub channels: Vec<f32>,
    /// Where this face is looking: `unknown`, `camera`, `partner`, `officiant`
    /// or `away`.
    pub gaze: String,
    /// How sure the head is about this face, `0..1`.
    pub confidence: f32,
    /// True when the tear reading is above the certainty gate.
    ///
    /// Sent rather than compared in the UI against a threshold the UI would then
    /// own. Section 12's fourth failure mode is a false tear, and the constant
    /// that guards it lives in `aura_core::contract::emotion::TEARS_CERTAIN`.
    pub reads_as_crying: bool,
    /// True when the smile reads as held for the camera rather than caught.
    pub posed_smile: bool,
    /// The crop to show.
    pub crop: CropRectDto,
}

/// One detected interaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractionDto {
    /// The stable slug.
    pub kind: String,
    /// The photographer-facing label, for the chip.
    pub title: String,
    /// How strongly the head detected it, `0..1`.
    pub strength: f32,
    /// True when this is one of the four milestones a client buys a print of.
    pub milestone: bool,
}

/// One photograph's emotion reading, for the Emotion card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmotionDto {
    /// The photograph.
    pub photo_id: String,
    /// Every face, in phase 06's detection order.
    pub faces: Vec<FaceExpressionDto>,
    /// The names of the eight channels, in `FaceExpressionDto::channels` order.
    ///
    /// Sent once per reading rather than hard-coded in the interface, so that the
    /// order can never drift between the model, the store and the bars a
    /// photographer looks at.
    pub channel_names: Vec<String>,
    /// The interactions detected, strongest first.
    pub interactions: Vec<InteractionDto>,
    /// True when two people are looking at each other.
    pub mutual_gaze: bool,
    /// Where the frame sits on its moment's curve, `0..1`.
    pub peak_proximity: f32,
    /// The frame this one reacts to, when it reacts to one.
    pub reaction_of: Option<String>,
    /// The scene-weighted, calibrated composite, `0..1`.
    ///
    /// **Not a keep decision.** A frame at 0.22 may be the only photograph of the
    /// ring exchange, and phase 12 knows that.
    pub emotion_score: f32,
    /// How important the moment is to the story, raised only by a cloud answer.
    pub narrative_weight: f32,
    /// Which scene the weights were conditioned on.
    pub scene: String,
    /// Why, strongest credit first.
    pub reasons: Vec<EmotionReasonDto>,
    /// How sure the whole reading is, `0..1`.
    pub confidence: f32,
    /// `local`, `cloud` or `user`.
    pub source: String,
    /// Which heads produced the readings.
    pub model_ver: u16,
    /// Which build's arithmetic produced the derived numbers.
    pub analysis_ver: u16,
    /// Which weight and ranker table produced the score.
    pub weights_ver: u16,
}

/// One moment's peak, for the browser's indicator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentPeakDto {
    /// The moment.
    pub moment_id: String,
    /// The strongest frame.
    pub photo_id: String,
    /// Its zero-based position in the moment.
    pub index: u32,
    /// How many frames the moment held when this was computed.
    pub frames: u32,
    /// How clearly the peak wins, `0..1`.
    pub margin: f32,
    /// `expression`, `kiss_apex`, `tear_release`, `bouquet_in_air`, `ring_slide`
    /// or `flat`.
    pub kind: String,
    /// True when the margin cleared the floor and the kind is not `flat`.
    ///
    /// Sent rather than compared in the UI. A moment with no separated peak is a
    /// common and correct answer, and the indicator has to be able to draw "no
    /// clear best frame" rather than pointing at a rounding error.
    pub resolved: bool,
    /// How sure the choice is, `0..1`.
    pub confidence: f32,
    /// Why this frame.
    pub reasons: Vec<EmotionReasonDto>,
    /// True when the photographer picked it.
    pub user_chosen: bool,
}

/// One reaction link, for the pair viewer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionLinkDto {
    /// The frame something happened in.
    pub action: String,
    /// The frame somebody reacted in.
    pub reaction: String,
    /// Milliseconds from action to reaction, signed.
    pub gap_ms: i64,
    /// How much this adds to the reaction's score, `0..1`.
    pub bonus: f32,
    /// How sure the link is, `0..1`.
    pub confidence: f32,
    /// Why these two frames.
    pub reasons: Vec<EmotionReasonDto>,
}

/// One frame in an emotion ordering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedByEmotionDto {
    /// The photograph.
    pub photo_id: String,
    /// Its emotion score, `0..1`.
    pub emotion_score: f32,
}

/// What the Emotion panel's header shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmotionStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs with a reading.
    pub scored: u32,
    /// Fraction of the project with a reading, `0..1`. Denominator: every photo.
    pub coverage: f32,
    /// Fraction of scored frames that carried at least one face, `0..1`.
    ///
    /// The second number, and the one that matters most when it is low: seven of
    /// the nine ranker terms come from faces, so a wedding at 3 % face-aware has
    /// been ranked on very nearly nothing.
    pub face_aware: f32,
    /// Moments in the project.
    pub moments: u32,
    /// Moments whose peak separated itself.
    pub peaked: u32,
    /// Fraction of moments with a resolved peak, `0..1`.
    pub peak_rate: f32,
    /// Reaction links found.
    pub links: u32,
    /// How many frames carry each interaction, in `Interaction::ALL` order.
    pub interaction_counts: Vec<u32>,
    /// The slugs those counts are in, in the same order.
    pub interaction_names: Vec<String>,
    /// Mean emotion score over scored frames.
    pub mean_score: f32,
    /// Mean peak margin over resolved peaks.
    pub mean_margin: f32,
    /// Pairwise preferences the photographer has recorded.
    pub preferences: u32,
    /// Which heads produced the readings. The lowest present.
    pub model_ver: u16,
    /// Which build's arithmetic. The lowest present.
    pub analysis_ver: u16,
    /// Which weight table. The lowest present.
    pub weights_ver: u16,
}

/// Ask for a project's emotion ordering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedInput {
    /// The wedding.
    pub project_id: String,
    /// How many frames to return. Clamped.
    pub limit: Option<u32>,
}

/// Record that the photographer would deliver one of two frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferInput {
    /// The frame they would deliver.
    pub winner_id: String,
    /// The frame they would not.
    pub loser_id: String,
}

/// Record that the photographer disagrees with a moment's peak frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPeakInput {
    /// The moment.
    pub moment_id: String,
    /// The frame they would lead with.
    pub photo_id: String,
}

/// Start a whole-project emotion pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreEmotionInput {
    /// The wedding.
    pub project_id: String,
    /// A handle `cancel_job` can reach the pass by.
    pub cancel_id: Option<String>,
}

/// What one emotion pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmotionPassDto {
    /// Photographs scored.
    pub scored: u32,
    /// Photographs that could not be scored.
    pub failed: u32,
    /// Faces read.
    pub faces: u32,
    /// Moments whose curve was built.
    pub moments: u32,
    /// Moments whose peak separated itself.
    pub peaked: u32,
    /// Reaction links written.
    pub links: u32,
    /// Mean emotion score over the frames this pass scored.
    pub mean_score: f32,
    /// Milliseconds.
    pub elapsed_ms: u64,
    /// True when the pass was stopped.
    pub cancelled: bool,
}

/// Emotion events pushed to the UI, mirroring section 11's four events.
///
/// Typed on both sides and not yet emitted, for the reason `IntegrityEvent`,
/// `MomentEvent`, `StoryEvent` and `PeopleEvent` are not: the Tauri shell has not
/// been launched on the development machine, so an emitter would be code nobody
/// has run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EmotionEvent {
    /// `emotion.scored` - a pass finished.
    EmotionScored {
        /// Frames scored.
        images: u64,
        /// Milliseconds.
        ms: u64,
        /// Mean emotion score.
        mean_score: f32,
        /// How many frames carry each interaction, in `Interaction::ALL` order.
        interaction_histogram: Vec<u32>,
    },
    /// `emotion.peaks` - the peak counters, once per pass.
    EmotionPeaks {
        /// Moments whose curve was built.
        moments: u64,
        /// Mean margin over resolved peaks.
        mean_margin: f32,
    },
    /// `emotion.reactions` - the link counters, once per pass.
    EmotionReactions {
        /// Links written.
        links: u64,
        /// Mean bonus.
        mean_bonus: f32,
    },
    /// `emotion.cloud_used` - the `MomentSignificance` calls a project made.
    EmotionCloudUsed {
        /// Calls made.
        calls: u64,
        /// What they cost.
        cost_usd: f32,
    },
}

// ---------------------------------------------------------------------------
// PHASE-11. The composition surface: how a photograph is framed, where the
// evidence is, and what the photographer may say back.
//
// Five commands. Three read, one records that a mark is wrong, and one runs the
// resumable pass. Nothing here crops, straightens, removes a distraction, keeps
// or rejects a photograph; phases 12, 23 and 24 own those decisions.
//
// The DTO deliberately carries the backend's decisions about which flags are
// violations and which reasons are exonerations. The interface draws those
// answers; it does not keep a second list of thresholds or reason-code meanings.
// ---------------------------------------------------------------------------

/// One place where the frame cuts a body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionJointCutDto {
    /// `neck`, `shoulder`, `elbow`, `wrist`, `hip`, `knee` or `ankle`.
    pub joint: String,
    /// `top`, `right`, `bottom` or `left`.
    pub edge: String,
    /// True when the cut lands on the joint rather than between joints.
    pub at_joint: bool,
    /// Scene-conditioned cost, `0..1`.
    pub severity: f32,
    /// True when the severity clears the backend's flag threshold.
    pub flagged: bool,
    /// The pixels that prove the cut.
    pub area: CropRectDto,
}

/// One thing that moved the composition score, with the pixels that prove it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionReasonDto {
    /// Stable reason slug.
    pub code: String,
    /// The exact photographer-facing explanation produced by the analyser.
    pub text: String,
    /// Negative for a penalty; zero or positive for an exoneration.
    pub weight: f32,
    /// True when this reason withdraws a claim rather than making one.
    pub exoneration: bool,
    /// The evidence rectangle, absent only for a frame-wide reason.
    pub evidence: Option<CropRectDto>,
}

/// The region phase 23 should preserve, without applying a crop here.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionCropHintDto {
    /// What must remain in frame.
    pub region: CropRectDto,
    /// How far the region may safely be tightened on each side.
    pub safe_margin: f32,
    /// Rotation phase 23 may apply, absent below the horizon confidence gate.
    pub straighten_deg: Option<f32>,
    /// Confidence in the hint, `0..1`.
    pub confidence: f32,
    /// True when the hint offers a meaningful crop or straighten operation.
    pub actionable: bool,
}

/// Everything phase 11 knows about how one photograph is framed.
///
/// Every field in `CompositionResult` crosses this boundary. Besides keeping the
/// Explain card honest, that makes this DTO the complete read surface for phases
/// 12 and 23 without letting either reach through the frozen service into SQL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct CompositionDto {
    /// The photograph.
    pub photo_id: String,
    /// Degrees off level. Positive is clockwise.
    pub tilt_deg: f32,
    /// True when the tilt reads as a deliberate choice.
    pub tilt_intentional: bool,
    /// Confidence in the horizon estimate, `0..1`.
    pub horizon_conf: f32,
    /// `none`, `gradient`, `vanishing_lines` or `gravity`.
    pub horizon_source: String,
    /// Space above the subject, as a fraction of frame height.
    pub headroom: f32,
    /// Distance to the nearest rule-of-thirds power point, `0..1`.
    pub thirds_offset: f32,
    /// Visual balance, `0..1`.
    pub balance: f32,
    /// Empty-frame fraction, `0..1`.
    pub negative_space: f32,
    /// Every detected body cut, worst first.
    pub joint_cuts: Vec<CompositionJointCutDto>,
    /// True when a head crop is a violation for this scene.
    pub head_crop: bool,
    /// Objects entering from a frame edge.
    pub edge_intrusions: Vec<CropRectDto>,
    /// Background clutter relative to the scene tolerance; `1.0` is the limit.
    pub clutter: f32,
    /// Bright regions that compete with the subject.
    pub bright_blobs: Vec<CropRectDto>,
    /// True when a background structure merges with a head.
    pub head_merge: bool,
    /// Background colour competition, `0..1`.
    pub colour_competition: f32,
    /// Learned, scene-conditioned aesthetic reading, `0..1`.
    pub aesthetic: f32,
    /// Fused composition score, `0..1`.
    pub composition_score: f32,
    /// A hint for phase 23. It is never applied by this surface.
    pub crop_suggestion_hint: Option<CompositionCropHintDto>,
    /// Scene used to choose rule bands.
    pub scene: String,
    /// Position within the moment's composition ranking, `0..1`.
    pub relative_composition: f32,
    /// People for whom the crop audit had keypoints.
    pub keypoint_subjects: u32,
    /// Set flags, in contract order.
    pub flags: Vec<String>,
    /// True when at least one flag describes a framing violation.
    pub has_violation: bool,
    /// Exact explanations, strongest penalty first.
    pub reasons: Vec<CompositionReasonDto>,
    /// Confidence in the complete judgement, `0..1`.
    pub confidence: f32,
    /// True when the photographer has dismissed a mark.
    pub user_reviewed: bool,
    /// Learned-head version.
    pub model_ver: u16,
    /// Geometry and score-arithmetic version.
    pub analysis_ver: u16,
    /// Scene-rule table version.
    pub rules_ver: u16,
}

/// What the Composition panel's project header shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs with a judgement.
    pub scored: u32,
    /// Fraction judged; denominator is every photograph.
    pub coverage: f32,
    /// Fraction of scored frames whose subjects had keypoints.
    pub keypoint_aware: f32,
    /// How many frames carry each flag, in the same order as `flagNames`.
    pub flag_counts: Vec<u32>,
    /// Stable flag slugs.
    pub flag_names: Vec<String>,
    /// Mean composition score.
    pub mean_score: f32,
    /// Mean absolute tilt among frames with a measurable horizon.
    pub mean_abs_tilt: f32,
    /// Fraction of tilted frames read as deliberate.
    pub intentional_ratio: f32,
    /// Frames with a crop hint for phase 23.
    pub hinted: u32,
    /// Frames whose marks a photographer has overruled.
    pub reviewed: u32,
    /// Upper bound on frames with a violation; flag counts overlap.
    pub violating_at_most: u32,
    /// Scene slugs judged with the neutral rule row.
    pub unruled_scenes: Vec<String>,
    /// Lowest learned-head version present.
    pub model_ver: u16,
    /// Lowest analysis version present.
    pub analysis_ver: u16,
    /// Lowest rule-table version present.
    pub rules_ver: u16,
}

/// Ask for frames carrying any of the named composition flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlaggedCompositionInput {
    /// The project.
    pub project_id: String,
    /// Flag slugs. Any matching flag is a hit.
    pub flags: Vec<String>,
    /// Maximum rows, clamped by the backend.
    pub limit: Option<u32>,
}

/// Tell AURA that one composition mark is wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DismissCompositionFlagInput {
    /// The photograph.
    pub photo_id: String,
    /// Exactly one violation flag slug.
    pub flag: String,
}

/// Start a resumable composition pass over a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyseCompositionInput {
    /// The project.
    pub project_id: String,
    /// Handle that `cancel_job` can signal.
    pub cancel_id: Option<String>,
}

/// What one composition pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionPassDto {
    /// Photographs judged.
    pub scored: u32,
    /// Photographs that could not be judged; no row was written for them.
    pub failed: u32,
    /// Subjects for whom keypoints were available.
    pub keypoint_subjects: u32,
    /// Frames with at least one flagged body cut.
    pub cut: u32,
    /// Frames whose tilt read as deliberate.
    pub intentional_tilts: u32,
    /// Frames with a measurable horizon.
    pub horizons: u32,
    /// Mean absolute tilt among those measurable horizons.
    pub mean_abs_tilt: f32,
    /// Flag counts in the same order as `flagNames`.
    pub flag_counts: Vec<u32>,
    /// Stable flag slugs.
    pub flag_names: Vec<String>,
    /// Mean score over this pass.
    pub mean_score: f32,
    /// Frames with a crop hint.
    pub hinted: u32,
    /// Scenes judged with neutral rules.
    pub unruled_scenes: Vec<String>,
    /// Milliseconds for the pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped after saving completed rows.
    pub cancelled: bool,
}

// ---------------------------------------------------------------------------
// PHASE-12. The culling surface: what is being delivered, why, and what the
// photographer may say back.
//
// Seven commands. Three read, three change the decision, and one runs the cull.
// Nothing here deletes, moves, exports or uploads a photograph; phases 14, 27,
// 29 and 30 own what happens to the gallery afterwards.
//
// The DTOs deliberately carry the backend's own answers - which reasons are
// keeps, which state a guarantee is in, which frame is the alternative - so the
// interface draws them rather than keeping a second copy of the vocabulary. It
// is the same boundary the composition surface draws and it matters more here,
// because a web view that decided for itself what `covered_weak` meant would be
// a web view that could tell a photographer their gallery was complete when it
// was not.
// ---------------------------------------------------------------------------

/// One thing that put a photograph in the gallery, or kept it out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CullReasonDto {
    /// Stable reason slug.
    pub code: String,
    /// The exact photographer-facing sentence the engine produced.
    pub text: String,
    /// Positive keeps, negative rejects.
    pub weight: f32,
    /// True when this reason put the photograph in the gallery.
    pub keep: bool,
    /// True when it fired before any arithmetic. Section 6.1's hard vetoes.
    pub veto: bool,
}

/// One photograph that is in the gallery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedDto {
    /// The photograph.
    pub photo_id: String,
    /// The moment it came from, absent when it is in none.
    pub moment_id: Option<String>,
    /// The fused keep score, `0..1`.
    pub keep_score: f32,
    /// Confidence in the decision, `0..1`.
    pub confidence: f32,
    /// Why it is here, strongest first.
    pub reasons: Vec<CullReasonDto>,
    /// The best alternative from the same moment, when one is not itself delivered.
    pub runner_up: Option<String>,
    /// The guarantee holding it here, when one is.
    pub coverage_role: Option<String>,
    /// True when a guarantee holds it, so the size slider may not drop it.
    pub protected: bool,
}

/// One photograph that is not in the gallery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectedDto {
    /// The photograph.
    pub photo_id: String,
    /// The moment it came from, absent when it is in none.
    pub moment_id: Option<String>,
    /// The fused keep score, `0..1`. Zero when a veto fired.
    pub keep_score: f32,
    /// Why it is not here, strongest first. Never empty.
    pub reasons: Vec<CullReasonDto>,
    /// The frame that is in the gallery instead, when there is a specific one.
    pub kept_instead: Option<String>,
    /// True when this frame was its moment's peak and lost anyway.
    pub was_peak: bool,
    /// True when no arithmetic was involved.
    pub vetoed: bool,
}

/// How one guarantee came out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageRuleDto {
    /// The must-have slug.
    pub rule: String,
    /// The words the panel shows.
    pub title: String,
    /// `covered`, `covered_weak` or `missing`.
    pub state: String,
    /// True when the gallery contains the rule's frames, however weakly.
    pub satisfied: bool,
}

/// One identity's presence in the gallery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityCoverageDto {
    /// The identity.
    pub identity_id: String,
    /// How many gallery photographs they are in.
    pub frames: u32,
}

/// One chapter's share of the gallery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterCountDto {
    /// The chapter slug.
    pub chapter: String,
    /// The words the panel shows.
    pub title: String,
    /// How many frames were delivered.
    pub delivered: u32,
    /// How many were targeted.
    pub target: u32,
}

/// What the gallery guarantees, and where it could not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageReportDto {
    /// Every guarantee, in the order of the wedding day.
    pub must_haves: Vec<CoverageRuleDto>,
    /// How many gallery photographs each identity appears in, including the zeros.
    pub identity_coverage: Vec<IdentityCoverageDto>,
    /// Delivered and targeted counts per chapter.
    pub chapters: Vec<ChapterCountDto>,
    /// Everything a photographer should read before delivering.
    pub warnings: Vec<String>,
}

/// One complete selection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionDto {
    /// The gallery, in timeline order.
    pub selected: Vec<SelectedDto>,
    /// Everything else, in timeline order.
    pub rejected: Vec<RejectedDto>,
    /// What the gallery guarantees.
    pub coverage: CoverageReportDto,
    /// How many frames were aimed for.
    pub target_count: u32,
    /// How many were delivered.
    pub actual_count: u32,
    /// `conservative`, `balanced` or `aggressive`.
    pub mode: String,
    /// The hash of the inputs and configuration, as a hex string.
    ///
    /// Text rather than a number because JavaScript cannot hold a `u64` exactly, and a
    /// support case quoting a rounded hash would be a support case about the wrong run.
    pub deterministic_hash: String,
    /// Lowest model version among the sub-scores underneath.
    pub model_ver: u16,
    /// Selection-pass version.
    pub analysis_ver: u16,
    /// Per-scene calibration version. `0` is the unfitted identity map.
    pub calibration_ver: u16,
}

/// What the culling view's header shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CullStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs that carried a technical verdict and could be considered.
    pub eligible: u32,
    /// Photographs in the gallery.
    pub selected: u32,
    /// Fraction of the project that was eligible; the denominator is every photograph.
    pub coverage: f32,
    /// Fraction of eligible frames that also had an emotion reading.
    pub emotion_aware: f32,
    /// ...a composition judgement.
    pub composition_aware: f32,
    /// ...a moment.
    pub grouped: f32,
    /// Guarantees fully covered.
    pub covered: u32,
    /// Guarantees covered only by forcing weak frames in.
    pub covered_weak: u32,
    /// Guarantees with no candidates at all.
    pub missing: u32,
    /// Frames the photographer forced into the gallery.
    pub user_kept: u32,
    /// Frames they forced out of it.
    pub user_rejected: u32,
    /// The stored mode.
    pub mode: String,
    /// The stored determinism hash, as hex.
    pub deterministic_hash: String,
    /// Lowest model version present.
    pub model_ver: u16,
    /// Selection-pass version.
    pub analysis_ver: u16,
    /// Calibration version.
    pub calibration_ver: u16,
}

/// What was decided about one photograph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionDto {
    /// True when the photograph is in the gallery.
    pub kept: bool,
    /// The keeper, when it is one.
    pub selected: Option<SelectedDto>,
    /// The rejection, when it is one.
    pub rejected: Option<RejectedDto>,
}

/// Run or re-run the cull over a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CullProjectInput {
    /// The project.
    pub project_id: String,
    /// `conservative`, `balanced` or `aggressive`. Absent keeps the stored mode.
    pub mode: Option<String>,
    /// How many frames to aim for. Absent asks the size model to predict one.
    pub target: Option<u32>,
    /// Handle that `cancel_job` can signal.
    pub cancel_id: Option<String>,
}

/// Move the size slider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeGalleryInput {
    /// The project.
    pub project_id: String,
    /// How many frames to aim for.
    ///
    /// The result may exceed it: coverage runs last and a guarantee outranks a slider.
    pub target: u32,
}

/// Switch autonomy mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCullModeInput {
    /// The project.
    pub project_id: String,
    /// `conservative`, `balanced` or `aggressive`.
    pub mode: String,
}

/// Keep or remove one photograph by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverrideDecisionInput {
    /// The photograph.
    pub photo_id: String,
    /// `keep`, `reject` or `clear`.
    ///
    /// Three values rather than a boolean, because "I have no opinion" is a distinct
    /// statement from "I want this out", and a nullable boolean is how the two get
    /// conflated on a wire.
    pub action: String,
}

/// What one cull did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CullPassDto {
    /// Photographs considered.
    pub photos: u32,
    /// Photographs eligible.
    pub eligible: u32,
    /// Photographs delivered.
    pub selected: u32,
    /// Veto counts, in the order of `vetoNames`.
    pub veto_counts: Vec<u32>,
    /// Stable veto slugs.
    pub veto_names: Vec<String>,
    /// Improving swaps the chapter local search made.
    pub swaps: u32,
    /// Frames the coverage guard forced in.
    pub coverage_added: u32,
    /// Frames the diversity pass removed.
    pub diversity_dropped: u32,
    /// Frames the size reconciliation added.
    pub size_added: u32,
    /// ...and removed.
    pub size_trimmed: u32,
    /// Moment peaks that were rejected, each saying so.
    pub peaks_rejected: u32,
    /// Guarantees that ended weakly covered.
    pub coverage_weak: u32,
    /// Guarantees with no candidates.
    pub coverage_missing: u32,
    /// Scenes with no weight row.
    pub unweighted_scenes: Vec<String>,
    /// Milliseconds for the selection passes.
    pub elapsed_ms: u64,
}

// ---------------------------------------------------------------------------
// PHASE-13. The explainability surface: why anything happened, how sure it was,
// what it looked at, and the record that lets somebody ask again a year later.
//
// Eight commands. Six read, one records the gallery's decisions into the ledger,
// and one exports a support bundle. Nothing here decides anything: the Explain
// panel is a reader, and a surface that could change a decision from inside an
// explanation of it would be a surface where the explanation and the decision
// could disagree.
//
// The DTOs carry the backend's own reading of every code - which severity it is,
// which domain it came from, whether the band was raised and why - for the reason
// the culling surface does: a web view that decided for itself whether
// `keypoints_unavailable` is bad news would be a web view that could tell a
// photographer their photograph is badly framed because AURA did not look at it.
// ---------------------------------------------------------------------------

/// A normalised rectangle to show the photographer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCropDto {
    /// Left edge, `0..1`.
    pub x: f32,
    /// Top edge, `0..1`.
    pub y: f32,
    /// Width, `0..1`.
    pub w: f32,
    /// Height, `0..1`.
    pub h: f32,
}

/// One named parameter and how far it moved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamDeltaDto {
    /// The parameter's stable name, as the develop engine spells it.
    pub name: String,
    /// The delta. Signed, and rendered with its sign.
    pub value: f32,
}

/// One reason, with everything the panel needs to draw it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerReasonDto {
    /// Stable reason slug, documented in `docs/reason-codes.md`.
    pub code: String,
    /// The exact sentence the deciding code produced.
    pub text: String,
    /// How much it moved the decision. Positive toward it, negative against.
    pub weight: f32,
    /// `credit`, `note`, `caveat` or `fault`, as the registry reads the code.
    pub severity: String,
    /// Which vocabulary it came from: `technical`, `emotion`, `composition`,
    /// `selection` or `ledger`.
    pub domain: String,
    /// `none`, `crop`, `frames` or `params`.
    pub evidence_kind: String,
    /// The region to show, when the evidence is a crop.
    pub crop: Option<EvidenceCropDto>,
    /// The photographs to show, when the evidence is frames.
    pub frames: Vec<String>,
    /// The parameter deltas, when the evidence is parameters.
    pub params: Vec<ParamDeltaDto>,
}

/// One recorded decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerDecisionDto {
    /// The decision's own id. What `aura-cli replay` takes.
    pub decision_id: String,
    /// `cull`, `edit`, `retouch`, `qc`, `curate` or `export`.
    pub kind: String,
    /// The words the tab shows.
    pub kind_title: String,
    /// `image`, `moment`, `segment` or `gallery`.
    pub subject_kind: String,
    /// The subject's id.
    pub subject_id: String,
    /// What the deciding code believed, `0..1`.
    pub raw_confidence: f32,
    /// What that belief is worth after calibration, `0..1`.
    pub calibrated_confidence: f32,
    /// Which calibration mapped it. `0` is the unfitted identity map.
    pub calibration_ver: u16,
    /// True when a fitted calibration produced the second number.
    pub calibrated: bool,
    /// `auto`, `auto_zero_touch`, `suggest` or `require_review`.
    pub autonomy: String,
    /// The words the badge shows.
    pub autonomy_title: String,
    /// What the band means, in a sentence.
    pub autonomy_text: String,
    /// True when a person has to look before anything happens.
    pub needs_review: bool,
    /// `local`, `cloud` or `user`.
    pub source: String,
    /// Why, strongest first.
    pub reasons: Vec<LedgerReasonDto>,
    /// What was decided, as canonical JSON.
    pub outputs_json: String,
    /// The hash of the question, as hex.
    ///
    /// Text rather than a number, because JavaScript cannot hold a `u64` exactly
    /// and a support case quoting a rounded hash is a support case about the wrong
    /// run. The same decision the culling surface made about its own hash.
    pub inputs_hash: String,
    /// Every model underneath it, as `[name, version]`.
    pub model_versions: Vec<(String, u16)>,
    /// Every config table underneath it, as `[name, version]`.
    pub config_versions: Vec<(String, u16)>,
    /// Milliseconds the deciding code took.
    pub ms: u32,
    /// When it was recorded, in milliseconds since the epoch.
    pub created_at: i64,
    /// The decision this one replaced, when it replaced one.
    pub supersedes: Option<String>,
}

/// One tab of the Explain panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainTabDto {
    /// `selection`, `technical`, `emotion`, `composition`, `edit` or `qc`.
    pub id: String,
    /// The words on the tab.
    pub title: String,
    /// True when this build has something to show here.
    pub available: bool,
    /// Why not, when it is not.
    ///
    /// Rendered instead of an empty tab, because a blank panel reads as "nothing
    /// wrong" and a phase that does not exist yet is not the same thing.
    pub unavailable_reason: Option<String>,
    /// The reasons, strongest first.
    pub reasons: Vec<LedgerReasonDto>,
    /// The tab's own score, when it has one.
    pub score: Option<f32>,
    /// The tab's own confidence, when it has one.
    pub confidence: Option<f32>,
}

/// The frame that nearly won, with its score breakdown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlternativeDto {
    /// The alternative photograph.
    pub photo_id: String,
    /// Its fused keep score, `0..1`.
    pub keep_score: f32,
    /// Phase 09's technical score.
    pub technical: f32,
    /// Phase 10's emotion score.
    pub emotion: f32,
    /// Phase 11's composition score.
    pub composition: f32,
    /// Phase 06's prominence-weighted subject presence.
    pub prominence: f32,
    /// True when this frame is the one that was delivered.
    pub delivered: bool,
}

/// Everything the Explain panel draws for one photograph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainPanelDto {
    /// The photograph.
    pub photo_id: String,
    /// The tabs, in a fixed order.
    pub tabs: Vec<ExplainTabDto>,
    /// The recorded decision, when one exists.
    pub decision: Option<LedgerDecisionDto>,
    /// A short line above the paragraph.
    pub headline: Option<String>,
    /// Two to four sentences assembled from the reasons.
    pub summary: String,
    /// True when a language model wrote the summary rather than the template.
    pub summary_from_cloud: bool,
    /// The delivered frame and the one that nearly won, for side-by-side compare.
    pub alternatives: Vec<AlternativeDto>,
}

/// What one wedding's ledger holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerStatusDto {
    /// Decisions recorded.
    pub decisions: u32,
    /// Decisions carrying at least one reason. The gate is `== decisions`.
    pub explained: u32,
    /// Fraction explained, `0..1`.
    pub explanation_coverage: f32,
    /// Decisions still in force.
    pub current: u32,
    /// Decisions a later one replaced.
    pub superseded: u32,
    /// Counts by kind, in the contract's order.
    pub by_kind: Vec<u32>,
    /// The kind slugs those counts are in.
    pub kind_names: Vec<String>,
    /// Counts by autonomy band, in the contract's order.
    pub by_autonomy: Vec<u32>,
    /// The band slugs those counts are in.
    pub autonomy_names: Vec<String>,
    /// Counts by source, in the contract's order.
    pub by_source: Vec<u32>,
    /// The source slugs those counts are in.
    pub source_names: Vec<String>,
    /// Decisions whose confidence went through a fitted calibration.
    pub calibrated: u32,
    /// Which calibration set produced them. `0` is the unfitted identity map.
    pub calibration_ver: u16,
    /// Reasons carrying something to look at.
    pub evidenced: u32,
    /// Reasons in total.
    pub reasons: u32,
    /// Bytes the ledger occupies for this project.
    pub bytes: u64,
}

/// What a support bundle came out as.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportBundleDto {
    /// The anonymised document.
    pub json: String,
    /// How many decisions it carries.
    pub decisions: u32,
    /// How many identifiers were replaced by handles.
    pub anonymised: u32,
    /// True when the safety scan found nothing. Always true, and checked anyway.
    pub safe: bool,
}

/// Ask for one photograph's explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainImageInput {
    /// The photograph.
    pub photo_id: String,
}

/// Ask for a project's review queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewQueueInput {
    /// The project.
    pub project_id: String,
    /// `auto`, `auto_zero_touch`, `suggest` or `require_review`.
    ///
    /// Absent means every band that needs review, which is what the queue is for.
    pub band: Option<String>,
    /// At most this many, newest first.
    pub limit: Option<u32>,
}

/// Record the stored gallery's decisions into the ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordDecisionsInput {
    /// The project.
    pub project_id: String,
}

/// What recording a gallery's decisions did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordDecisionsDto {
    /// Decisions written.
    pub recorded: u32,
    /// Decisions refused because they could not explain themselves.
    ///
    /// Zero on any healthy project. It is on the wire rather than only in a log
    /// because a gallery whose reasons went missing is a gallery whose panel will
    /// be empty, and the photographer should be told once rather than discovering
    /// it one frame at a time.
    pub refused: u32,
    /// Counts by autonomy band, in the contract's order.
    pub by_autonomy: Vec<u32>,
    /// The band slugs those counts are in.
    pub autonomy_names: Vec<String>,
    /// True when no fitted calibration exists, so every band was raised one step.
    pub uncalibrated: bool,
    /// Milliseconds for the whole pass.
    pub elapsed_ms: u64,
}

/// Export an anonymised support bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportBundleInput {
    /// The project.
    pub project_id: String,
    /// At most this many decisions, newest first.
    pub limit: Option<u32>,
}

// ---------------------------------------------------------------------------
// PHASE-14. The develop surface: the edit recipe, the renderer and the history.
//
// Nine commands, and the boundary is the phase's own. The UI may read a recipe, change a
// parameter, undo, redo, snapshot, reset, ask for a proxy render and ask what the renderer
// can do. It may **not** name a destination, ask for a file to be written, or overwrite a
// parameter a person set - the last of those is refused inside
// `aura_recipe::schema::merge` rather than on the wire, so a caller cannot route around it.
//
// `docs/adr/ADR-0030-develop-ipc-surface.md` records the shape.
// ---------------------------------------------------------------------------

/// One parameter of an edit, as the develop panel renders it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopParamDto {
    /// The dotted path, e.g. `global.exposure`. The identity of the control.
    pub path: String,
    /// The current value, as a JSON scalar so one shape carries floats and integers.
    pub value: serde_json::Value,
    /// True when a person set this and no automated pass may change it.
    pub protected: bool,
    /// Which stage re-runs when this moves. `null` when it is inert for rendering.
    pub stage: Option<String>,
}

/// One photograph's edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeDto {
    /// The photograph.
    pub photo_id: String,
    /// The canonical JSON. The document itself, exactly as hashed and stored.
    pub body: String,
    /// BLAKE3 over `body`.
    pub recipe_hash: String,
    /// Schema version.
    pub schema: u16,
    /// The engine that wrote it.
    pub engine: String,
    /// `ai`, `user`, `qc`, `preset` or `default`.
    pub source: String,
    /// Calibrated confidence, `0..1`. Invariant 2.
    pub confidence: f32,
    /// The ledger row that decided this, when one did.
    pub decision_id: Option<String>,
    /// Dotted paths a person has touched, sorted.
    pub user_edited_fields: Vec<String>,
    /// The parameters, flattened for the panel.
    pub params: Vec<DevelopParamDto>,
}

/// One stage that did not run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderNoteDto {
    /// The stage slug.
    pub stage: String,
    /// The reason slug.
    pub reason: String,
    /// What was being asked for, when naming it helps.
    pub detail: Option<String>,
    /// True when this is worth showing the photographer.
    pub is_caveat: bool,
}

/// A rendered proxy, ready for an image tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderDto {
    /// Output width.
    pub width: u32,
    /// Output height.
    pub height: u32,
    /// The pixels, base64 of interleaved 8-bit RGB.
    ///
    /// Base64 rather than a path, because there is no path: invariant 1 says the original is
    /// opened read-only and this phase writes no image file at all. The develop panel turns
    /// this into a data URL.
    pub rgb_base64: String,
    /// `srgb`, `adobe_rgb` or `display_p3`.
    pub colour_space: String,
    /// The ICC profile the viewer should assume.
    pub icc: String,
    /// The hash of the four inputs that produced this.
    pub render_hash: String,
    /// `cpu`, or the accelerator's name.
    pub backend: String,
    /// Stages that ran, in order.
    pub stages_run: Vec<String>,
    /// Stages that did not, with their reason and detail.
    pub notes: Vec<RenderNoteDto>,
    /// Wall-clock milliseconds.
    pub ms: u32,
}

/// What this machine's renderer can do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderCapsDto {
    /// `cpu` or `wgpu`.
    pub backend: String,
    /// Longest edge rendered in one pass before tiling.
    pub max_texture: u32,
    /// Bits of mantissa the colour maths carries.
    pub precision_bits: u8,
    /// The working-buffer ceiling in bytes.
    pub max_working_bytes: u64,
    /// The engine string in every render hash.
    pub engine: String,
    /// The degradation this backend runs under, or `null`.
    pub degradation: Option<String>,
    /// The photographer-facing sentence for that degradation.
    pub degradation_message: Option<String>,
}

/// One step in a photograph's edit history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntryDto {
    /// Position, from 1.
    pub seq: u64,
    /// Milliseconds since the epoch.
    pub at_ms: i64,
    /// Who made it.
    pub source: String,
    /// The dotted paths it changed.
    pub changed: Vec<String>,
    /// A short line for the panel.
    pub label: String,
}

/// A photograph's history, as the panel renders it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDto {
    /// The photograph.
    pub photo_id: String,
    /// Steps, oldest first.
    pub entries: Vec<HistoryEntryDto>,
    /// Named snapshots.
    pub snapshots: Vec<String>,
    /// True when undo would do something.
    pub can_undo: bool,
    /// True when redo would do something.
    pub can_redo: bool,
    /// True when an automated pass has run, so "reset to AI suggestion" is available.
    pub has_ai_suggestion: bool,
}

/// How much of a wedding has an edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopStatusDto {
    /// Photographs in the project. **The denominator**, and not the delivered gallery.
    pub images: u32,
    /// Photographs carrying a recipe.
    pub with_recipe: u32,
    /// Recipes an automated pass wrote.
    pub from_ai: u32,
    /// Recipes a person wrote.
    pub from_user: u32,
    /// Photographs a person has touched at least one parameter of.
    pub touched_by_hand: u32,
    /// Photographs whose sidecar on disk is behind the catalog.
    pub sidecar_behind: u32,
}

/// Ask for one photograph's edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopImageInput {
    /// The photograph.
    pub photo_id: String,
}

/// Change one parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetParamInput {
    /// The project.
    pub project_id: String,
    /// The photograph.
    pub photo_id: String,
    /// The dotted path.
    pub path: String,
    /// The new value.
    pub value: serde_json::Value,
    /// A short line for the history panel.
    pub label: Option<String>,
}

/// What changing a parameter did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetParamDto {
    /// The edit after the change.
    pub recipe: RecipeDto,
    /// The dotted paths that moved.
    pub changed: Vec<String>,
    /// The first stage that has to re-run, or `null` when nothing does.
    pub invalidated_from: Option<String>,
}

/// Ask for a render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderImageInput {
    /// The photograph.
    pub photo_id: String,
    /// `proxy2048`, `screen` or `full`.
    pub level: Option<String>,
    /// Width and height for `screen`.
    pub screen: Option<(u32, u32)>,
    /// `srgb`, `adobe_rgb` or `display_p3`. Defaults to sRGB.
    pub colour_space: Option<String>,
    /// `interactive`, `analysis` or `export`. Defaults to interactive.
    pub purpose: Option<String>,
}

/// Walk the history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStepInput {
    /// The project.
    pub project_id: String,
    /// The photograph.
    pub photo_id: String,
    /// `undo`, `redo`, `reset_original` or `reset_ai`.
    pub action: String,
}

/// Take or restore a named snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInput {
    /// The project.
    pub project_id: String,
    /// The photograph.
    pub photo_id: String,
    /// The photographer's own name for it.
    pub name: String,
    /// `take` or `restore`.
    pub action: String,
}

// ---------------------------------------------------------------------------
// PHASE-15. Exposure and white balance.
// ---------------------------------------------------------------------------

/// One light the solver found in a frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IlluminantDto {
    /// `daylight`, `tungsten`, `fluorescent`, `led`, `flash`, `candle`, `shade`,
    /// `cloudy`, `mixed_discharge`, `coloured` or `unknown`.
    pub kind: String,
    /// Correlated colour temperature in kelvin, derived from the chromaticity.
    pub cct_k: f32,
    /// Green-magenta tint, derived from the chromaticity.
    pub tint: f32,
    /// How much of the frame this light accounts for, `0..1`.
    pub weight: f32,
    /// How far off neutral the light itself is, `0..1`.
    pub chroma: f32,
    /// Which generator proposed it: `camera_as_shot`, `grey_world`, `white_patch`,
    /// `learned` or `known_neutral`.
    pub source: String,
    /// Where it dominates, or `null` for a light that fills the frame.
    pub region: Option<CropRectDto>,
}

/// A runner-up white balance and what it cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToneAlternativeDto {
    /// The exposure it would have been applied on top of, in stops.
    pub exposure_ev: f32,
    /// Colour temperature in kelvin.
    pub temperature_k: f32,
    /// Green-magenta tint.
    pub tint: f32,
    /// What the solve scored it at. Lower is better.
    pub cost: f32,
    /// Why it lost, as a stable reason code.
    pub why: String,
}

/// One thing that moved an exposure or a white balance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToneReasonDto {
    /// Stable reason code.
    pub code: String,
    /// The sentence, rendered from the code and the numbers the catalog stored.
    pub text: String,
    /// How much confidence it cost. Negative is doubt.
    pub weight: f32,
    /// The pixels behind it, when the reason is about a region.
    pub evidence: Option<CropRectDto>,
}

/// One photograph's exposure and white-balance decision.
///
/// Seven bits rather than one enum, for the frozen contract's reason: `mixedLight`,
/// `backlit` and `colouredLight` are properties of the *light*, `faceAnchored` is a property
/// of the decision, and `userEdited`, `reviewed` and `needsReview` are three different
/// answers about a person. Collapsing any of them together would make a panel guess.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct ToneDto {
    /// The photograph.
    pub photo_id: String,
    /// Exposure offset in stops. Positive brightens.
    pub exposure_ev: f32,
    /// How sure the exposure is, `0..1`.
    pub exposure_conf: f32,
    /// Colour temperature in kelvin.
    pub temperature_k: f32,
    /// Green-magenta tint. Positive is magenta.
    pub tint: f32,
    /// How sure the white balance is, `0..1`.
    pub wb_conf: f32,
    /// The geometric mean of the two confidences, which is what the ledger records.
    pub confidence: f32,
    /// Every light found, strongest weight first.
    pub illuminants: Vec<IlluminantDto>,
    /// True when two lights disagree spatially and phase 18 should correct locally.
    pub mixed_light: bool,
    /// Index into `illuminants` of the light governing the subject, or `null`.
    pub dominant_on_subject: Option<u32>,
    /// Prominence-weighted face luminance before the exposure moved, `0..1`.
    pub subject_luma_before: f32,
    /// Where this scene wants that luminance, `0..1`.
    pub subject_luma_target: f32,
    /// Estimated residual skin error in dE00, against the identities' own loci.
    pub skin_de00_estimate: f32,
    /// Runner-up colour answers, best first.
    pub alternatives: Vec<ToneAlternativeDto>,
    /// Why, strongest doubt first.
    pub reasons: Vec<ToneReasonDto>,
    /// The scene the bands came from.
    pub scene: String,
    /// True when a face anchored the exposure. The bit section 1's argument turns on.
    pub face_anchored: bool,
    /// True when the frame was read as backlit and exposed for the subject anyway.
    pub backlit: bool,
    /// True when a saturated light was preserved rather than corrected.
    pub coloured_light: bool,
    /// Fraction of the frame this exposure newly clips.
    pub clipping_added: f32,
    /// Identities in frame that had a usable skin locus.
    pub constrained_identities: u32,
    /// True when the photographer set these values by hand.
    ///
    /// The three numbers above stay AURA's own. What the photographer set is in the three
    /// fields below, and keeping both is what lets the panel show a disagreement rather than
    /// a replacement.
    pub user_edited: bool,
    /// The exposure the photographer set, when they set one.
    pub user_exposure_ev: Option<f32>,
    /// The temperature the photographer set, when they set one.
    pub user_temperature_k: Option<f32>,
    /// The tint the photographer set, when they set one.
    pub user_tint: Option<f32>,
    /// True when the photographer has looked at this frame in the review queue.
    pub reviewed: bool,
    /// True when this frame belongs in the low-confidence queue.
    pub needs_review: bool,
    /// Which heads produced the prediction.
    pub model_ver: u32,
    /// Which build's arithmetic produced the solve.
    pub analysis_ver: u32,
    /// Which target table the bands came from.
    pub targets_ver: u32,
}

/// What a project's tone pass covered and found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToneStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs with an estimate.
    pub estimated: u32,
    /// Fraction estimated; the denominator is every photograph.
    pub coverage: f32,
    /// Fraction of estimated frames whose exposure was anchored on a face.
    pub face_anchored: f32,
    /// Fraction of estimated frames whose white balance was bounded by a skin locus.
    pub skin_constrained: f32,
    /// Frames marked for phase 18's local correction.
    pub mixed_light: u32,
    /// Frames where a saturated light was preserved.
    pub coloured_light: u32,
    /// Frames below the review threshold that nobody has looked at.
    pub needs_review: u32,
    /// Frames the photographer set by hand.
    pub user_edited: u32,
    /// Mean exposure offset over estimated frames, in stops.
    pub mean_ev: f32,
    /// Mean colour temperature over estimated frames, in kelvin.
    pub mean_cct: f32,
    /// How many frames carry each illuminant kind as their dominant light.
    pub illuminant_counts: Vec<u32>,
    /// Stable illuminant slugs, in the same order.
    pub illuminant_names: Vec<String>,
    /// Segments with enough anchors for phase 25.
    pub segments_anchored: u32,
    /// Segments in the project's story.
    pub segments: u32,
    /// Identities with a usable skin locus.
    pub loci: u32,
    /// Scenes that had no target row and were estimated against neutral bands.
    pub untargeted_scenes: Vec<String>,
    /// Which heads produced the numbers.
    pub model_ver: u32,
    /// Which build's arithmetic produced them.
    pub analysis_ver: u32,
    /// Which target table the bands came from.
    pub targets_ver: u32,
}

/// Start a resumable exposure and white-balance pass over a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateToneInput {
    /// The project.
    pub project_id: String,
    /// Handle that `cancel_job` can signal.
    pub cancel_id: Option<String>,
}

/// What one tone pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TonePassDto {
    /// Photographs estimated.
    pub estimated: u32,
    /// Photographs that could not be estimated; no row was written for them.
    pub failed: u32,
    /// Frames whose exposure was anchored on a face.
    pub face_anchored: u32,
    /// Frames marked mixed-light.
    pub mixed_light: u32,
    /// Frames where a saturated light was preserved.
    pub coloured_light: u32,
    /// Frames below the review threshold.
    pub low_confidence: u32,
    /// Identities that ended the pass with a usable skin locus.
    pub loci: u32,
    /// Segments that ended the pass with enough anchors.
    pub segments_anchored: u32,
    /// Mean exposure offset over this pass, in stops.
    pub mean_ev: f32,
    /// Mean colour temperature over this pass, in kelvin.
    pub mean_cct: f32,
    /// Scenes estimated against neutral bands.
    pub untargeted_scenes: Vec<String>,
    /// Recipes written through the merge.
    pub recipes_written: u32,
    /// Recipes the merge refused to touch because a person had set the field.
    pub recipes_protected: u32,
    /// Milliseconds for the pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

/// Ask for the frames whose white balance is worth a photographer's attention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToneReviewInput {
    /// The project.
    pub project_id: String,
    /// How many to return. Defaults to 200, capped at 5,000.
    pub limit: Option<u32>,
}

/// Record that the photographer has looked at one estimate and agrees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptToneInput {
    /// The photograph.
    pub photo_id: String,
}

/// Record what the photographer set instead, and write it into the recipe.
///
/// Every field is optional and independent: somebody who corrects only the temperature has
/// not made a claim about the exposure, and an override carrying all three would silently
/// freeze the two they did not touch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetToneOverrideInput {
    /// The project.
    pub project_id: String,
    /// The photograph.
    pub photo_id: String,
    /// Exposure offset in stops.
    pub exposure_ev: Option<f32>,
    /// Colour temperature in kelvin.
    pub temperature_k: Option<f32>,
    /// Green-magenta tint.
    pub tint: Option<f32>,
}

/// What recording an override did, on both sides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetToneOverrideDto {
    /// The estimate after the override, with `userEdited` set.
    pub estimate: ToneDto,
    /// The edit after the merge.
    pub recipe: RecipeDto,
    /// The dotted paths that moved.
    pub changed: Vec<String>,
    /// The dotted paths a person now owns.
    pub protected: Vec<String>,
}

/// One of a segment's anchors for gallery consistency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceFrameDto {
    /// The photograph.
    pub photo_id: String,
    /// The chapter it anchors.
    pub segment_id: String,
    /// Its place in the segment's ordering, zero first.
    pub rank: u32,
    /// The white-balance confidence that got it chosen.
    pub wb_conf: f32,
    /// Its solved temperature, which is what phase 25 normalises toward.
    pub temperature_k: f32,
    /// Its solved tint.
    pub tint: f32,
    /// Its subject luminance after the exposure moved, `0..1`.
    pub subject_luma: f32,
    /// How good an anchor it is, `0..1`.
    pub quality: f32,
}

/// Ask for one chapter's anchors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceFramesInput {
    /// The chapter.
    pub segment_id: String,
}

// ---------------------------------------------------------------------------
// PHASE-16. Tone curves, HSL and skin protection.
// ---------------------------------------------------------------------------

/// One control point of a tone curve, in the recipe's 0-255 units.
///
/// A pair rather than an object, because that is what the recipe stores and a second spelling
/// of the same two numbers is a second thing to keep in step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurvePointDto {
    /// Input level, `0..=255`.
    pub x: u16,
    /// Output level, `0..=255`.
    pub y: u16,
}

/// One hue band's shift, in the recipe's units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HslShiftDto {
    /// Which band: `red`, `orange`, `yellow`, `green`, `aqua`, `blue`, `purple` or `magenta`.
    pub band: String,
    /// Hue rotation within the band, `-100..100`.
    pub h: f32,
    /// Saturation within the band.
    pub s: f32,
    /// Luminance within the band.
    pub l: f32,
}

/// What was found of one kind of content in one frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BandReadingDto {
    /// `greenery`, `sky`, `dress`, `wood`, `decor` or `skin`.
    pub band: String,
    /// The fraction of the frame it covers, `0..1`.
    pub area: f32,
    /// Its mean hue, in degrees.
    pub hue_deg: f32,
    /// Its mean saturation, `0..1`.
    pub saturation: f32,
    /// Its mean luminance, `0..1`.
    pub luma: f32,
    /// How sure the inference is, `0..1`.
    ///
    /// On the wire rather than hidden behind the adjustment, because "AURA saw greenery and
    /// was not sure enough to touch it" and "AURA saw no greenery" are different sentences and
    /// only one of them is about this photograph.
    pub confidence: f32,
}

/// What grading actually did to the skin in one frame.
///
/// `measured` false is **not** a perfect score. A frame with nobody in it has no skin to
/// protect and no measurement to report, and a panel that rendered the two the same way would
/// turn a coverage gap into a guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinGuardDto {
    /// The fraction of the frame the skin mask covers, `0..1`.
    pub mask_area: f32,
    /// The largest hue rotation any sampled skin region suffered, in degrees.
    pub max_hue_shift_deg: f32,
    /// The largest relative chroma change.
    pub max_chroma_change: f32,
    /// What every colour operation was scaled by inside the mask, `0..1`.
    pub attenuation: f32,
    /// How many times the grade was re-solved to meet the ceilings.
    pub resolves: u32,
    /// True when there was skin to measure and it was measured.
    pub measured: bool,
    /// True when both ceilings were met.
    pub within_ceilings: bool,
}

/// One thing that moved a grade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColourReasonDto {
    /// Stable reason code.
    pub code: String,
    /// The sentence, rendered from the code and the numbers the catalog stored.
    pub text: String,
    /// How much confidence it cost. Negative is doubt.
    pub weight: f32,
    /// The pixels behind it, when the reason is about a region.
    pub evidence: Option<CropRectDto>,
}

/// A complete alternative grade.
///
/// **Whole parameter sets, never deltas.** Every one has been through the clipping guard and
/// the skin guard, which is what makes the switcher safe rather than only fast.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColourVariantDto {
    /// `flatter`, `punchier` or `warmer`.
    pub kind: String,
    /// Contrast, `-100..100`.
    pub contrast: f32,
    /// Highlight recovery.
    pub highlights: f32,
    /// Shadow lift.
    pub shadows: f32,
    /// White point.
    pub whites: f32,
    /// Black point.
    pub blacks: f32,
    /// Vibrance.
    pub vibrance: f32,
    /// Flat saturation.
    pub saturation: f32,
    /// Its own curve.
    pub curve: Vec<CurvePointDto>,
    /// Its own eight bands.
    pub hsl: Vec<HslShiftDto>,
    /// Its skin guard report. Every variant is guarded.
    pub skin_guard: SkinGuardDto,
}

/// One photograph's tone and colour decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct ColourDto {
    /// The photograph.
    pub photo_id: String,
    /// Contrast, `-100..100`.
    pub contrast: f32,
    /// Highlight recovery. Negative pulls highlights down.
    pub highlights: f32,
    /// Shadow lift. Positive opens shadows.
    pub shadows: f32,
    /// White point.
    pub whites: f32,
    /// Black point.
    pub blacks: f32,
    /// Vibrance.
    pub vibrance: f32,
    /// Flat saturation.
    pub saturation: f32,
    /// The fitted point curve, monotone by construction.
    pub curve: Vec<CurvePointDto>,
    /// The eight bands, in the recipe's order.
    pub hsl: Vec<HslShiftDto>,
    /// What the content pass read.
    pub bands: Vec<BandReadingDto>,
    /// What grading did to the skin, measured.
    pub skin_guard: SkinGuardDto,
    /// Highlight clipping before the grade, as a fraction of the frame.
    pub clipping_before: f32,
    /// Highlight clipping after it.
    pub clipping_after: f32,
    /// How much new highlight clipping the grade added.
    pub clipping_added: f32,
    /// Complete alternatives, at most three.
    pub alternatives: Vec<ColourVariantDto>,
    /// Why, strongest doubt first.
    pub reasons: Vec<ColourReasonDto>,
    /// How sure the grade is, `0..1`.
    pub confidence: f32,
    /// The total adjustment magnitude, `0..1`. Lower is subtler.
    pub subtlety: f32,
    /// The scene the intents came from.
    pub scene: String,
    /// True when there was skin in the frame and the guarantee was checked on it.
    pub skin_measured: bool,
    /// True when the photographer set these values by hand.
    pub user_edited: bool,
    /// True when the photographer has looked at this frame.
    pub reviewed: bool,
    /// True when this frame belongs in the review queue.
    pub needs_review: bool,
    /// Which learned head produced the prediction.
    pub model_ver: u32,
    /// Which build's arithmetic produced the solve.
    pub analysis_ver: u32,
    /// Which intent table the targets came from.
    pub intent_ver: u32,
}

/// What the Develop panel's project header shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColourStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs with a stored grade.
    pub decided: u32,
    /// `decided / photos`.
    pub coverage: f64,
    /// Photographs where there was skin to protect and it was measured.
    pub skin_measured: u32,
    /// Photographs where the skin guard had to intervene.
    pub skin_guard_triggered: u32,
    /// Photographs where the clipping guard re-solved.
    pub clip_guard_resolved: u32,
    /// Photographs whose grade was capped by the subtlety ceiling.
    pub subtlety_capped: u32,
    /// Photographs below the review threshold.
    pub needs_review: u32,
    /// Photographs the photographer has set by hand.
    pub user_edited: u32,
    /// Mean contrast over graded frames.
    pub mean_contrast: f32,
    /// Mean shadow lift over graded frames.
    pub mean_shadow_lift: f32,
    /// Mean subtlety over graded frames.
    pub mean_subtlety: f32,
    /// The largest skin hue shift anywhere in the project, in degrees.
    ///
    /// **The one number that falsifies this phase's headline guarantee.** On the wire rather
    /// than derived, so a support engineer can ask for it directly.
    pub worst_skin_hue_shift: f32,
    /// True when every stored grade met the ceilings.
    pub guarantee_held: bool,
    /// Scenes graded against the neutral intent row.
    pub untargeted_scenes: Vec<String>,
    /// The lowest model version present.
    pub model_ver: u32,
    /// The lowest analysis version present.
    pub analysis_ver: u32,
    /// The lowest intent-table version present.
    pub intent_ver: u32,
}

/// Run the resumable grading pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateColourInput {
    /// The project.
    pub project_id: String,
    /// A token the UI can cancel with.
    pub cancel_id: Option<String>,
}

/// What one grading pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColourPassDto {
    /// Photographs graded.
    pub decided: u32,
    /// Photographs that could not be graded.
    pub failed: u32,
    /// Frames where the guarantee was actually checked.
    pub skin_measured: u32,
    /// Frames where the skin guard had to intervene.
    pub skin_guard_triggered: u32,
    /// Frames where every colour operation was withdrawn to keep skin where it was.
    pub skin_guard_withdrew: u32,
    /// Frames where the clipping guard re-solved.
    pub clip_guard_resolved: u32,
    /// Frames whose grade was scaled back by the subtlety cap.
    pub subtlety_capped: u32,
    /// Frames below the review threshold.
    pub low_confidence: u32,
    /// Mean contrast.
    pub mean_contrast: f32,
    /// Mean shadow lift.
    pub mean_shadow_lift: f32,
    /// Mean subtlety.
    pub mean_subtlety: f32,
    /// The worst skin hue shift in the run, in degrees.
    pub worst_skin_hue_shift: f32,
    /// Scenes graded against the neutral intent row.
    pub untargeted_scenes: Vec<String>,
    /// Recipes the pass wrote.
    pub recipes_written: u32,
    /// Recipe paths the merge refused because a person already owned them.
    pub recipes_protected: u32,
    /// Wall clock.
    pub elapsed_ms: u64,
    /// True when the pass was cancelled.
    pub cancelled: bool,
}

/// Ask for the frames whose grade is worth a photographer's attention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColourReviewInput {
    /// The project.
    pub project_id: String,
    /// How many, at most.
    pub limit: Option<u32>,
}

/// Record that the photographer has looked at one grade and agrees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptColourInput {
    /// The photograph.
    pub photo_id: String,
}

/// Promote one stored alternative to the primary grade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectVariantInput {
    /// The project.
    pub project_id: String,
    /// The photograph.
    pub photo_id: String,
    /// `flatter`, `punchier` or `warmer`.
    pub kind: String,
}

/// Record what the photographer set instead, and write it into the recipe.
///
/// Every field is optional and independent: somebody who reduced the contrast has not made a
/// claim about the greenery. The curve and the HSL block are whole-or-nothing, because a curve
/// is not a set of independent numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetColourOverrideInput {
    /// The project.
    pub project_id: String,
    /// The photograph.
    pub photo_id: String,
    /// Contrast.
    pub contrast: Option<f32>,
    /// Highlight recovery.
    pub highlights: Option<f32>,
    /// Shadow lift.
    pub shadows: Option<f32>,
    /// White point.
    pub whites: Option<f32>,
    /// Black point.
    pub blacks: Option<f32>,
    /// Vibrance.
    pub vibrance: Option<f32>,
    /// Flat saturation.
    pub saturation: Option<f32>,
    /// The whole curve, or nothing.
    pub curve: Option<Vec<CurvePointDto>>,
    /// The whole HSL block, or nothing.
    pub hsl: Option<Vec<HslShiftDto>>,
}

/// What recording a colour override did, on both sides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetColourOverrideDto {
    /// The decision after the override, with `userEdited` set.
    pub decision: ColourDto,
    /// The edit after the merge.
    pub recipe: RecipeDto,
    /// The dotted paths that moved.
    pub changed: Vec<String>,
    /// The dotted paths a person now owns.
    pub protected: Vec<String>,
}

// ---------------------------------------------------------------------------
// PHASE-17. Style learning: scene-conditional personal AI profiles.
// ---------------------------------------------------------------------------

/// One leaf of the style tree, as the matrix draws it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleBucketDto {
    /// `group/lighting`, the catalog's own key.
    pub key: String,
    /// `preparation`, `details`, `ceremony`, `portraits`, `reception`, `dance`, `candid` or
    /// `other`.
    pub group: String,
    /// `unknown`, `daylight`, `golden_hour`, `shade`, `overcast`, `tungsten`, `artificial`,
    /// `flash`, `candle` or `stage`.
    pub lighting: String,
    /// What the matrix calls it.
    pub title: String,
    /// How many of the photographer's own pairs landed here.
    pub samples: u32,
    /// How many were held out and used to measure the fit.
    pub held_out: u32,
    /// The measured style-match error in dE00, or `null` when nothing was held out.
    ///
    /// **Never zero for "not measured".** A bucket trained on eleven pairs and evaluated on
    /// none has no measurement, and zero would render as a perfect match - which is the one
    /// thing a report about accuracy must not do where it knows least. ADR-0036 decision 2.
    pub match_de00: Option<f32>,
    /// Which level of the tree answers here: `bucket`, `group`, `global` or `factory`.
    pub level: String,
    /// True when this leaf has too few pairs to be trusted on its own.
    pub weak: bool,
}

/// One profile, as the list shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleProfileDto {
    /// The profile.
    pub profile_id: String,
    /// What the photographer calls it.
    pub name: String,
    /// Which training produced it.
    pub version: u32,
    /// `candidate`, `adopted` or `retired`.
    pub status: String,
    /// How many pairs it was trained on.
    pub trained_pairs: u32,
    /// Its strength, `0..1`, for the meter section 12 asks for instead of a ready state.
    pub strength: f32,
    /// The measured style-match error over every held-out pair, in dE00.
    pub overall_de00: f32,
    /// How many leaves carry the photographer's own evidence.
    pub taught_buckets: u32,
    /// True when it has enough evidence for AURA to be confident.
    pub usable: bool,
    /// The render engine it was fitted against.
    pub engine_ver: String,
    /// When, in milliseconds since the Unix epoch.
    pub trained_at: i64,
}

/// The honest report a photographer reads before adopting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileReportDto {
    /// Which profile.
    pub profile: StyleProfileDto,
    /// Every populated leaf, in matrix order.
    pub per_bucket: Vec<StyleBucketDto>,
    /// The leaves the photographer should add weddings for, worst first.
    pub weak_buckets: Vec<String>,
    /// What to add next, as a sentence generated from the actual gap.
    pub recommendation: String,
    /// How many pairs the fit accepted.
    pub accepted_pairs: u32,
    /// How many it rejected. **On the wire beside the acceptance**, because a report that
    /// showed only the accepted count could claim a hundred percent on any archive.
    pub rejected_pairs: u32,
    /// The fraction that survived the residual check, `0..1`.
    pub acceptance: f32,
    /// True when the overall figure met section 10.1's ceiling.
    pub met_ceiling: bool,
}

/// Point the scanner at folders of the photographer's own work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanArchiveInput {
    /// The profile name to scan for. Versions of one look share a name.
    pub name: String,
    /// Absolute paths to the folders, one per wedding.
    ///
    /// Paths **in**, and nothing but names out: `StylePairDto` carries two file names and a
    /// verdict. ADR-0036 decision 1.
    pub roots: Vec<String>,
}

/// What one archive scan found, before anything is fitted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanArchiveDto {
    /// Camera originals found.
    pub originals: u32,
    /// Delivered finals found.
    pub finals: u32,
    /// Pairs the matcher made.
    pub matched: u32,
    /// Originals with no final. Usually a culled frame, which is normal.
    pub unmatched_originals: u32,
    /// Finals with no original. **The one worth reporting**: it means the RAWs are missing, on
    /// another disk, or in a format this build does not decode.
    pub unmatched_finals: u32,
    /// How many pairs each strategy found, by slug.
    pub by_method: Vec<(String, u32)>,
    /// The weakest strategy any pair needed, which is how much to trust the whole pairing.
    pub weakest_method: String,
    /// True when there are enough pairs for a training run to produce anything.
    pub enough: bool,
}

/// One original-and-final pair, as the report lists it.
///
/// **No pixels.** There is no field here that could hold image bytes, which is what makes
/// "AURA never uploads your archive" a property of the shape rather than a promise about the
/// code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StylePairDto {
    /// The original's file name, without its directory.
    pub original: String,
    /// The final's file name.
    pub final_image: String,
    /// `content_hash`, `filename_stem`, `capture_time`, `perceptual` or `unmatched`.
    pub matched_by: String,
    /// `xmp`, `fitted` or `none`.
    pub extracted_from: String,
    /// Which leaf it landed in.
    pub bucket: String,
    /// What the fit could not explain, in dE00.
    pub residual_de00: f32,
    /// True when it was used.
    pub accepted: bool,
    /// The reason code when it was not.
    pub rejection: Option<String>,
}

/// Train a profile from whatever the scan already stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainProfileInput {
    /// The profile name.
    pub name: String,
    /// A token the UI can cancel with.
    pub cancel_id: Option<String>,
}

/// What one training run did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainProfileDto {
    /// The profile it produced, at `candidate`. **Adoption is a separate command.**
    pub profile: Option<StyleProfileDto>,
    /// Pairs the matcher found.
    pub matched: u32,
    /// Pairs the fit accepted.
    pub accepted: u32,
    /// Pairs it rejected, each stored with a reason.
    pub rejected: u32,
    /// Pairs already fitted at this version and skipped. Invariant 5, on the wire.
    pub reused: u32,
    /// Pairs whose parameters came from a sidecar rather than from a fit.
    pub from_xmp: u32,
    /// Leaves the tree populated.
    pub buckets: u32,
    /// The measured style-match error, in dE00.
    pub overall_de00: f32,
    /// The same figure for the unstyled baseline, so the improvement is visible rather than
    /// asserted.
    pub baseline_de00: f32,
    /// Wall clock.
    pub elapsed_ms: u64,
    /// True when the run was cancelled.
    pub cancelled: bool,
}

/// Adopt one profile: it becomes what the product edits with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptProfileInput {
    /// The profile.
    pub profile_id: String,
}

/// One thing that moved, or did not move, a styled edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleReasonDto {
    /// Stable reason code.
    pub code: String,
    /// The sentence.
    pub text: String,
    /// How much confidence it cost. Negative is doubt.
    pub weight: f32,
}

/// One leaf's three answers, for the side-by-side before adoption.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleComparisonDto {
    /// Which leaf.
    pub bucket: String,
    /// What the matrix calls it.
    pub title: String,
    /// The exposure, temperature, contrast and vibrance the baseline would use.
    ///
    /// Four numbers rather than a whole parameter set, because the comparison is a *summary*
    /// and the develop surface already owns showing a recipe. A panel that drew a whole set
    /// here would be a second Develop panel that could drift from the first.
    pub baseline: Vec<f32>,
    /// What the currently adopted profile would use, or empty when there is none.
    pub current: Vec<f32>,
    /// What the candidate would use.
    pub candidate: Vec<f32>,
    /// Which level of the tree the candidate answered from.
    pub level: String,
    /// How sure the candidate is.
    pub confidence: f32,
    /// Why, strongest doubt first.
    pub reasons: Vec<StyleReasonDto>,
}

/// Ask for the side-by-side of the baseline, the adopted profile and a candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareProfilesInput {
    /// The project the comparison is for.
    pub project_id: String,
    /// The candidate.
    pub candidate_id: String,
    /// How many leaves, at most.
    pub limit: Option<u32>,
}

/// Write a signed, portable profile to a file the photographer names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProfileInput {
    /// The profile.
    pub profile_id: String,
    /// Where to write it.
    pub path: String,
}

/// What exporting a profile produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProfileDto {
    /// Where it was written.
    pub path: String,
    /// How many bytes.
    pub bytes: u64,
    /// The signing key's fingerprint, in groups of four.
    pub fingerprint: String,
}

/// Read a signed profile bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProfileInput {
    /// The `.auraprofile` file.
    pub path: String,
}

/// What importing a profile produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProfileDto {
    /// The profile that was read, at `candidate` whatever it said about itself.
    pub profile: StyleProfileDto,
    /// The signing key's fingerprint.
    pub fingerprint: String,
    /// True when the document is unchanged since it was signed.
    ///
    /// **Not called `verified`.** With the public key inside the bundle, this proves integrity
    /// and not provenance: there is no key distribution in this product and nothing to check a
    /// key against. ADR-0035 decision 8, and `ProfileReport.tsx` never renders the word.
    pub unchanged_since_signing: bool,
}

/// Which profile a project, and optionally one chapter of it, uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetProjectProfileInput {
    /// The project.
    pub project_id: String,
    /// One of phase 07's nine chapter slugs, or `null` for the project default.
    pub chapter: Option<String>,
    /// The profile, or `null` to clear the selection.
    pub profile_id: Option<String>,
}

/// What the style panel's project header shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleStatusDto {
    /// Profiles in the catalog, adopted and candidate.
    pub profiles: u32,
    /// The profile this project uses, when one is selected.
    pub active: Option<String>,
    /// What it is called.
    pub active_name: String,
    /// Which version.
    pub active_version: u32,
    /// How many pairs it was trained on.
    pub trained_pairs: u32,
    /// Its strength, `0..1`.
    pub strength: f32,
    /// Its measured style-match error, in dE00.
    pub overall_de00: f32,
    /// Which chapters have an override, by slug.
    pub chapter_overrides: Vec<String>,
    /// How many of this project's leaves resolve at each level, by slug.
    ///
    /// **The number that matters when it is skewed.** A wedding whose frames all resolve at
    /// `global` has had its scene conditioning do nothing, which is the quiet version of "one
    /// global style" - the exact thing this phase exists to beat.
    pub level_counts: Vec<(String, u32)>,
    /// The fraction that resolved at their own leaf, `0..1`.
    pub bucket_ratio: f64,
    /// Which build's fitter produced the active profile.
    pub analysis_ver: u32,
}

// ---------------------------------------------------------------------------
// PHASE-18. Local mask AI: the regions every later phase edits inside.
// ---------------------------------------------------------------------------

/// What the mask panel's project header shows.
///
/// **`selected` and `masked` are two numbers rather than a ratio**, and this is the first
/// status shape in the product where that matters. The denominator is *selected* frames, not
/// every photograph: a mask over a rejected frame is not a gap, it is a frame nobody asked
/// about. A photographer looking at a project where the cull has not run sees `selected: 0`
/// rather than a coverage figure computed against a denominator that does not exist.
/// See `docs/adr/ADR-0038-mask-ipc-surface.md` decision 5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskStatusDto {
    /// Frames the cull kept.
    pub selected: u64,
    /// How many of those carry masks at the current versions.
    pub masked: u64,
    /// How many masks exist in total.
    pub masks: u64,
    /// How many of those a photographer has edited by hand.
    pub user_edited: u64,
    /// How many are below the aggressive floor.
    pub low_quality: u64,
    /// Mean class confidence, weighted by area.
    pub mean_confidence: f32,
    /// Mean edge quality, weighted by area.
    pub mean_edge_quality: f32,
    /// Total stored bytes for this project.
    pub payload_bytes: u64,
    /// Mean stored bytes per masked frame. What the 180 KB budget bounds.
    pub bytes_per_image: f32,
    /// The model set the stored masks were produced under.
    pub model_ver: u32,
    /// The analysis version the stored masks were produced under.
    pub analysis_ver: u32,
    /// False in this build. The learned segmentation head is registered and never consulted.
    pub head_trained: bool,
}

/// One reason a region is the way it is, with the sentence the panel renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskReasonDto {
    /// The stable code from the frozen `MaskReason` vocabulary.
    pub code: String,
    /// One sentence, in the product's own voice.
    pub text: String,
}

/// One region of one photograph.
///
/// **`confidence` and `edgeQuality` are never collapsed into one number.** They fail
/// independently and are fixed by different things - a photographer can re-brush a boundary and
/// cannot re-brush a class - so the panel shows two bars and names which of the two is limiting.
/// ADR-0038 decision 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskDto {
    /// Prefixed mask id.
    pub id: String,
    /// Prefixed photo id.
    pub image_id: String,
    /// The class slug, from the frozen twenty.
    pub kind: String,
    /// Which person, when the region belongs to one. Absent is a real answer.
    pub identity_id: Option<String>,
    /// The identity's display name, when there is one.
    pub identity_name: Option<String>,
    /// `rle` or `alpha8`.
    pub form: String,
    /// The stored plane's width.
    pub width: u32,
    /// The stored plane's height.
    pub height: u32,
    /// Stored bytes.
    pub bytes: u64,
    /// Edge softness applied on top of the payload.
    pub feather: f32,
    /// How sure the class assignment is.
    pub confidence: f32,
    /// How well determined the boundary is.
    pub edge_quality: f32,
    /// The word for the boundary: `matted`, `soft`, `binary` or `unknown`.
    pub edge: String,
    /// The strength ceiling phases 19 to 24 multiply by.
    ///
    /// Computed once, in Rust, and sent. The panel could derive it from the two quality numbers
    /// and must not: two implementations of a gating rule is two answers to "may this mask carry
    /// skin smoothing". ADR-0038 decision 3.
    pub allowance: f32,
    /// False when this mask may not carry skin smoothing or generative cleanup.
    pub allows_aggressive: bool,
    /// Why the region is the way it is. Never empty.
    pub reasons: Vec<MaskReasonDto>,
    /// True when a photographer brushed it. Automation never regenerates one of these.
    pub user_edited: bool,
    /// The model set this mask was produced under.
    pub model_ver: u32,
}

/// A region as a plane the panel can draw.
///
/// Quarter-resolution eight-bit alpha, base64, capped at `OVERLAY_MAX_EDGE` on the long edge.
/// The panel draws over a preview that is itself a proxy, so a full-resolution plane is detail
/// nobody can see costing bytes everybody pays - and the brush wants the alpha values
/// themselves rather than an image of them. ADR-0038 decision 1.
///
/// **There is no field here that could hold a photograph.** This is derived geometry about a
/// region; the pixels of the frame reach the panel through the preview surface and nowhere else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskOverlayDto {
    /// Prefixed mask id.
    pub id: String,
    /// Plane width.
    pub width: u32,
    /// Plane height.
    pub height: u32,
    /// `width * height` alpha bytes, base64.
    pub alpha_base64: String,
    /// Which render level it was resolved at.
    pub level: String,
}

/// Ask for a photograph's regions, producing any that are missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnsureMasksInput {
    /// The project the photograph is in.
    ///
    /// Needed because producing a region reads pixels, and the preview cache is opened per
    /// project. Every other command on this surface is a query over one table and takes no
    /// project, which is the difference this field makes visible.
    pub project_id: String,
    /// The photograph.
    pub image_id: String,
    /// Which classes. Empty means all twenty.
    #[serde(default)]
    pub kinds: Vec<String>,
}

/// One step of a mask composition.
///
/// A whole edit arrives as one command with an explicit op rather than as a stream of brush
/// points: a per-point command would be a command per animation frame, which breaks the 50 ms
/// rule by volume rather than by latency, and it would make undo a replay of two hundred rows.
/// ADR-0038 decision 4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskOpDto {
    /// `source`, `plane`, `union`, `intersect`, `subtract`, `invert`, `feather`, `grow` or
    /// `shrink`.
    pub op: String,
    /// The mask this step pushes, for `source`.
    #[serde(default)]
    pub mask_id: Option<String>,
    /// A stroke plane, for `plane`: `width`, `height` and base64 alpha.
    #[serde(default)]
    pub width: Option<u32>,
    /// The stroke plane's height.
    #[serde(default)]
    pub height: Option<u32>,
    /// The stroke plane's alpha bytes, base64.
    #[serde(default)]
    pub alpha_base64: Option<String>,
    /// The amount, for `feather`.
    #[serde(default)]
    pub amount: Option<f32>,
    /// The radius in analysis pixels, for `grow` and `shrink`.
    #[serde(default)]
    pub radius: Option<u32>,
}

/// Apply a composition to one of a photograph's regions and keep the result.
///
/// Sets `userEdited`, and there is no argument here that clears it. The one thing that clears it
/// is `regenerate_mask`, which is a separate deliberate act. ADR-0037 decision 7.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditMaskInput {
    /// The mask being edited.
    pub mask_id: String,
    /// The program, in postfix order.
    pub ops: Vec<MaskOpDto>,
    /// The feather to store with the result.
    #[serde(default)]
    pub feather: Option<f32>,
}

/// What one operation may do through one region.
///
/// The shape phases 19 to 24 read before they apply anything. It is on this surface as well so
/// the panel can say *why* an operation is unavailable rather than only that it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskAllowanceDto {
    /// Prefixed mask id.
    pub mask_id: String,
    /// The operation asked about.
    pub operation: String,
    /// The strength ceiling. Multiply by it; do not compare against it.
    pub ceiling: f32,
    /// False when the operation is refused outright.
    pub permitted: bool,
    /// Why the ceiling is below one. Empty when nothing is limiting.
    pub reasons: Vec<MaskReasonDto>,
}

// ---------------------------------------------------------------------------
// PHASE-19. Local light sculpting.
// ---------------------------------------------------------------------------

/// One face, and what the light on it was moved by.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceLightDto {
    /// Whose face, when phase 06 knows. `null` for the guests nobody has named.
    pub identity_id: Option<String>,
    /// Exposure inside the face mask, in stops.
    pub exposure_ev: f32,
    /// Shadow lift inside the face mask.
    pub shadows: i32,
    /// Highlight restraint. Never positive.
    pub highlights: i32,
    /// The face's mean luminance before, `0..1`.
    pub luma_before: f32,
    /// Where the scene's band wanted it, `0..1`.
    pub luma_target: f32,
    /// Where it ended up, `0..1`.
    pub luma_after: f32,
    /// The largest lift this frame's noise would have tolerated, in stops.
    ///
    /// **The number the panel shows when a lift stopped short.** "AURA lifted her face 0.4 EV
    /// and would have lifted it 0.9" is a sentence; "+0.4" is a number somebody argues with.
    pub noise_cap_ev: f32,
    /// The mask confidence and edge quality this face's move was scaled by, `0..1`.
    pub mask_scale: f32,
}

/// One shaping move, as a retoucher would name it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapingZoneDto {
    /// The zone's stable slug, e.g. `under_eye`.
    pub zone: String,
    /// Centre in frame coordinates, `0..1`.
    pub cx: f32,
    /// Centre in frame coordinates, `0..1`.
    pub cy: f32,
    /// Radius as a fraction of the frame's longer side.
    pub radius: f32,
    /// Gain in stops. Positive lifts, negative deepens.
    pub gain_ev: f32,
}

/// One reason the local work came out the way it did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalReasonDto {
    /// The stable slug.
    pub code: String,
    /// The sentence a photographer reads.
    pub text: String,
    /// How much this moved the confidence. Negative for a doubt.
    pub weight: f32,
    /// Which operation it is about, or `null` for a reason about the whole plan.
    pub operation: Option<String>,
    /// True when the code withdraws a claim rather than making one.
    ///
    /// On the wire rather than derived in the panel, because fourteen of the thirty codes are
    /// withdrawals and a panel that grouped them by parsing the slug would get
    /// `mask_unavailable` wrong.
    pub withdrawal: bool,
    /// The pixels to show, when there are any more specific than the whole frame.
    pub evidence: Option<CropRectDto>,
}

/// Everything phase 19 decided about the light inside one photograph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct LocalPlanDto {
    /// The photograph.
    pub photo_id: String,
    /// The strength each operation ran at, `0..1`, in priority order.
    pub strengths: Vec<f32>,
    /// The operations' stable slugs, in the same order.
    ///
    /// Sent rather than hard-coded in the panel, so adding an operation is one change rather
    /// than two that can disagree.
    pub operations: Vec<String>,
    /// What each face was moved by.
    pub faces: Vec<FaceLightDto>,
    /// Clarity on the subject.
    pub subject_clarity: i32,
    /// Texture on the subject.
    pub subject_texture: i32,
    /// Contrast on the subject.
    pub subject_contrast: i32,
    /// Exposure on the background, in stops. Zero or negative.
    pub background_ev: f32,
    /// Saturation on the background. Zero or negative.
    pub background_saturation: i32,
    /// The measured background/subject luminance ratio that triggered the pair.
    pub competition_ratio: f32,
    /// The measured background chroma energy.
    pub chroma_energy: f32,
    /// The frame's mean luminance before the paired operations, `0..1`.
    pub mean_luma_before: f32,
    /// And after. The two must agree within three per cent, which is section 10.1's own
    /// acceptance criterion and is why both are on the wire rather than their difference.
    pub mean_luma_after: f32,
    /// How many specular regions were reduced.
    pub shine_regions: u32,
    /// The luminance reduction applied to them, in stops. Zero or negative.
    pub shine_ev: f32,
    /// Where they were.
    pub shine_boxes: Vec<CropRectDto>,
    /// The shaping moves, by face ordinal.
    pub shaping: Vec<Vec<ShapingZoneDto>>,
    /// The largest luminance difference between two faces after lighting.
    pub face_spread: f32,
    /// True when everybody in this frame ended up consistently lit.
    pub group_fair: bool,
    /// How much of the allowed perceptual change was spent, `0..1`.
    pub budget_used: f32,
    /// Operations that were reduced or skipped, as `operation` and `maskKind` pairs.
    pub gated: Vec<GateDto>,
    /// Why, strongest doubt first.
    pub reasons: Vec<LocalReasonDto>,
    /// How much the plan trusts itself, `0..1`.
    pub confidence: f32,
    /// The scene it was decided under.
    pub scene: String,
    /// True when a photographer set the strengths by hand.
    pub user_edited: bool,
    /// True when a photographer has looked at this plan and agreed.
    pub reviewed: bool,
    /// True when it is below the review threshold and nobody has looked.
    pub needs_review: bool,
    /// Which learned head produced the targets.
    pub model_ver: u32,
    /// Which build's arithmetic produced the decisions.
    pub analysis_ver: u32,
    /// Which policy file the strengths came from.
    pub policy_ver: u32,
    /// Which build's derivation turns zones into grids.
    pub shaping_ver: u32,
}

/// One operation that did not run at full strength, and the mask that stopped it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateDto {
    /// The operation's stable slug.
    pub operation: String,
    /// The mask kind that was missing or weak.
    pub mask_kind: String,
}

/// What a project's local light pass covered and found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs with a plan.
    pub planned: u32,
    /// Fraction planned; the denominator is every photograph.
    pub coverage: f32,
    /// Fraction of planned frames where at least one operation actually ran.
    ///
    /// **The number that matters when it is low.** Because the work is meant to be invisible,
    /// a wedding at 100 % coverage and 4 % acted-on looks exactly like a wedding that was
    /// worked on.
    pub acted_on: f32,
    /// Fraction of planned frames where every mask an operation wanted arrived.
    pub mask_covered: f32,
    /// How many frames each operation ran on, in priority order.
    pub op_counts: Vec<u32>,
    /// The operations' stable slugs, in the same order.
    pub op_names: Vec<String>,
    /// How many operations each mask kind gated.
    pub gated_counts: Vec<u32>,
    /// The mask kinds' stable slugs, in the same order.
    pub gated_names: Vec<String>,
    /// Mean fraction of the per-image allowance spent.
    pub mean_budget_used: f32,
    /// Frames where shine was reduced.
    pub shine_reduced: u32,
    /// Mean shine reduction over those frames, in stops.
    pub mean_shine_ev: f32,
    /// Frames where faces were solved jointly.
    pub group_solved: u32,
    /// Frames below the review threshold that nobody has looked at.
    pub needs_review: u32,
    /// Frames the photographer set by hand.
    pub user_edited: u32,
    /// Scenes that had no policy row and were shaped against the neutral strengths.
    pub unpolicied_scenes: Vec<String>,
    /// Which learned head produced the targets.
    pub model_ver: u32,
    /// Which build's arithmetic produced the decisions.
    pub analysis_ver: u32,
    /// Which policy file the strengths came from.
    pub policy_ver: u32,
    /// Which build's derivation turns zones into grids.
    pub shaping_ver: u32,
}

/// Start a resumable local light pass over a project's selected photographs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SculptLocalInput {
    /// The project.
    pub project_id: String,
    /// The photographs to plan.
    ///
    /// Empty means every photograph with no current plan. **The list is the normal path**:
    /// invariant 3, and section 11's own budget is written about a thousand selected images
    /// rather than about a wedding.
    #[serde(default)]
    pub photo_ids: Vec<String>,
    /// Handle that `cancel_job` can signal.
    pub cancel_id: Option<String>,
}

/// What one local light pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPassDto {
    /// Photographs planned.
    pub planned: u32,
    /// Photographs that could not be planned; no row was written for them.
    pub failed: u32,
    /// Photographs where at least one operation ran.
    pub acted_on: u32,
    /// How many frames each operation ran on, in priority order.
    pub op_counts: Vec<u32>,
    /// How many operations were gated over the whole pass.
    pub gated: u32,
    /// Frames where every mask an operation wanted arrived.
    pub fully_masked: u32,
    /// Frames where faces were solved jointly.
    pub group_solved: u32,
    /// Frames where shine was reduced.
    pub shine_reduced: u32,
    /// Frames below the review threshold.
    pub low_confidence: u32,
    /// Mean fraction of the allowance spent.
    pub mean_budget_used: f32,
    /// Scenes planned against the neutral row.
    pub unpolicied_scenes: Vec<String>,
    /// Recipes written through the merge.
    pub recipes_written: u32,
    /// Recipes the merge refused to touch because a person had set the field.
    pub recipes_protected: u32,
    /// Milliseconds for the pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

/// Ask for the frames whose local work is worth a photographer's attention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalReviewInput {
    /// The project.
    pub project_id: String,
    /// How many to return. Defaults to 200, capped at 5,000.
    pub limit: Option<u32>,
}

/// Record that the photographer has looked at one plan and agrees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptLocalInput {
    /// The photograph.
    pub photo_id: String,
}

/// Record what the photographer set instead, and write it into the recipe.
///
/// Every strength is optional and independent: somebody who turned the shaping off has not
/// made a claim about the face lighting, and an override carrying all six would silently
/// freeze the five they did not touch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLocalStrengthInput {
    /// The project.
    pub project_id: String,
    /// The photograph.
    pub photo_id: String,
    /// The operation's stable slug.
    pub operation: String,
    /// The strength, `0..1`.
    pub strength: f32,
}

/// What recording a strength override did, on both sides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLocalStrengthDto {
    /// The plan after the override, with `userEdited` set.
    pub plan: LocalPlanDto,
    /// The edit after the merge.
    pub recipe: RecipeDto,
    /// The dotted paths that moved.
    pub changed: Vec<String>,
    /// The dotted paths a person now owns.
    pub protected: Vec<String>,
}

// ---------------------------------------------------------------------------
// PHASE-20. Portrait retouch.
// ---------------------------------------------------------------------------

/// One thing that was done to somebody skin.
///
/// The rectangle is present for a blemish and absent for the two operations that act through a
/// mask or a landmark region, which is a real answer rather than a missing one: the panel draws
/// a marker for a blemish and highlights a region for the others.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetouchOpDto {
    /// The operator name: `blemish`, `under_eye`, `tone_evening` or `shine_reduce`.
    pub kind: String,
    /// How strongly it ran, `0..1`.
    pub strength: f32,
    /// Where it acted, when it names a rectangle.
    pub area: Option<CropRectDto>,
    /// How a blemish was removed: `patch` or `learned`.
    pub method: Option<String>,
    /// Whose face, when the operation names a person.
    pub identity_id: Option<String>,
    /// Luminance lift in stops, for an under-eye correction. Bounded at 0.25.
    pub luma_ev: f32,
    /// Chroma separation reduction, for the same. Bounded at 0.12.
    pub chroma: f32,
}

/// Something about a person this product will not remove.
///
/// The rectangle is in **face-normalised** coordinates - origin between the eyes, x along the
/// eye-to-eye line, unit the inter-ocular distance - which is what lets one row protect the same
/// mole in four hundred photographs. A panel drawing it on a specific frame projects it through
/// that frame own landmarks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectedFeatureDto {
    /// Whose face.
    pub identity_id: String,
    /// `mole`, `freckle`, `birthmark`, `scar`, `tattoo` or `dimple`.
    pub kind: String,
    /// Where on that face, face-normalised. `x` and `y` may be negative.
    pub area: CropRectDto,
    /// How sure the product is, `0..1`. One when a photographer said so.
    pub confidence: f32,
    /// `cross_frame`, `classifier` or `user`, in ascending order of authority.
    pub source: String,
    /// How many frames it was measured on.
    pub frames: u32,
    /// The span those frames covered, in minutes.
    pub span_minutes: f32,
    /// The first photograph it was seen on, for the evidence crop.
    pub first_seen_photo: String,
    /// True when nothing may clear it.
    ///
    /// On the wire rather than derived from the kind in the panel, because the consequence is a
    /// control that must not be rendered as a toggle: a tattoo is always protected and the UI
    /// has to be able to say so without knowing the vocabulary.
    pub absolute: bool,
}

/// What the retouch did to the texture of the skin, measured through the renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureReportDto {
    /// High-band skin energy after the retouch over the same energy before it.
    pub band_ratio: f32,
    /// The floor this frame was held to.
    pub floor: f32,
    /// True when the stored plan is inside the floor.
    pub passed: bool,
    /// How many skin samples the ratio was measured over.
    ///
    /// The panel shows the ratio to three decimal places only when this is large enough to mean
    /// something. A number measured over eleven samples is arithmetic rather than evidence.
    pub measured_on: u32,
    /// How many times the solver gave up strength to reach the floor.
    pub resolves: u8,
    /// True when the retouch was withdrawn entirely because the floor could not be met.
    pub withdrawn: bool,
}

/// One reason the retouch came out the way it did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetouchReasonDto {
    /// The stable slug.
    pub code: String,
    /// The sentence a photographer reads.
    pub text: String,
    /// How much this moved the confidence. Negative for a doubt.
    pub weight: f32,
    /// True when the code withdraws a claim rather than making one.
    ///
    /// **Half the codes in this phase are withdrawals**, which is the highest proportion in the
    /// product, so the panel groups by this rather than by parsing the slug.
    pub withdrawal: bool,
    /// The pixels to show, when there are any more specific than the whole frame.
    pub evidence: Option<CropRectDto>,
}

/// Everything phase 20 decided about one photograph skin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct RetouchPlanDto {
    /// The photograph.
    pub photo_id: String,
    /// What was done.
    pub ops: Vec<RetouchOpDto>,
    /// The gallery-constant strength for each identity in this frame.
    pub identity_strengths: Vec<IdentityStrengthDto>,
    /// What must not be removed from the people in this frame.
    pub protected: Vec<ProtectedFeatureDto>,
    /// What it cost the texture.
    pub texture: TextureReportDto,
    /// `off`, `light`, `natural` or `polished`.
    pub preset: String,
    /// Why. Never empty.
    pub reasons: Vec<RetouchReasonDto>,
    /// How much the plan trusts itself, `0..1`.
    pub confidence: f32,
    /// The scene it was decided under.
    pub scene: String,
    /// How much of the shared per-image allowance it spent, `0..1`.
    ///
    /// **Phase 19 allowance, not a second one.** Six local operations and a retouch that each
    /// stay inside their own budget still add up to a photograph that looks worked on.
    pub budget_used: f32,
    /// True when a photographer changed the preset or a strength by hand.
    pub user_edited: bool,
    /// True when a photographer has looked at this plan and agreed.
    pub reviewed: bool,
    /// True when the frame is worth a photographer attention.
    pub needs_review: bool,
    /// Which learned heads produced the detections.
    pub model_ver: u32,
    /// Which build arithmetic produced the decisions.
    pub analysis_ver: u32,
    /// Which preset file the strengths came from.
    pub preset_ver: u32,
}

/// One person gallery-wide retouch strength.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityStrengthDto {
    /// Who.
    pub identity_id: String,
    /// What every frame in this wedding retouches them at, `0..1`.
    pub strength: f32,
}

/// What the retouch panel project header shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetouchStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs with a plan.
    pub planned: u32,
    /// Fraction planned; the denominator is every photograph.
    pub coverage: f32,
    /// Fraction of planned frames where at least one operation ran.
    pub acted_on: f32,
    /// Fraction of planned frames whose skin mask arrived.
    pub mask_covered: f32,
    /// Blemishes removed across the project.
    pub blemishes_removed: u32,
    /// Anomalies deliberately left alone.
    ///
    /// **The number a photographer asks about**, because it is the answer to "why is that mark
    /// still there". A retoucher that only reported what it removed would look like one that
    /// missed things.
    pub anomalies_left: u32,
    /// How many features are protected, by kind.
    pub protected_counts: Vec<u32>,
    /// The kinds stable slugs, in the same order.
    pub protected_kinds: Vec<String>,
    /// Frames where the texture guard reduced a strength.
    pub texture_resolved: u32,
    /// Frames where it withdrew the retouch entirely.
    pub texture_withdrawn: u32,
    /// Mean band ratio over the frames that were retouched.
    pub mean_band_ratio: f32,
    /// Mean gallery-constant strength over the people in the project.
    pub mean_strength: f32,
    /// The largest spread of one identity strength across the gallery.
    ///
    /// Zero by construction while strength is a gallery constant, and on the wire anyway so a
    /// change that made it per-frame would be visible in the product rather than only in a diff.
    pub max_identity_spread: f32,
    /// How many frames each preset was used on.
    pub preset_counts: Vec<u32>,
    /// The presets stable slugs, in the same order.
    pub preset_names: Vec<String>,
    /// Frames nobody has reviewed that are below the review threshold.
    pub needs_review: u32,
    /// Frames a photographer has set by hand.
    pub user_edited: u32,
    /// Scenes with no preset row.
    pub unpreset_scenes: Vec<String>,
    /// Which learned heads produced the detections.
    pub model_ver: u32,
    /// Which build arithmetic produced the decisions.
    pub analysis_ver: u32,
    /// Which preset file the strengths came from.
    pub preset_ver: u32,
}

/// What one retouch pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetouchPassDto {
    /// Photographs planned.
    pub planned: u32,
    /// Photographs that could not be planned.
    pub failed: u32,
    /// Photographs where at least one operation ran.
    pub acted_on: u32,
    /// Photographs where a skin mask arrived.
    pub mask_covered: u32,
    /// Blemishes removed.
    pub blemishes: u32,
    /// Frames where the texture guard reduced a strength.
    pub texture_resolved: u32,
    /// Frames where it withdrew the retouch.
    pub texture_withdrawn: u32,
    /// Features protected by cross-frame evidence.
    pub protected: u32,
    /// Frames below the review threshold.
    pub low_confidence: u32,
    /// Mean band ratio over the frames that were retouched.
    pub mean_band_ratio: f32,
    /// Scenes planned against the neutral row.
    pub unpreset_scenes: Vec<String>,
    /// Recipes written.
    pub recipes_written: u32,
    /// Recipes left alone because a photographer had taken them over.
    pub recipes_protected: u32,
    /// Milliseconds for the pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

/// Run the resumable retouch pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetouchPassInput {
    /// The project.
    pub project_id: String,
    /// The photographs to plan. Empty means everything that is not planned at these versions.
    #[serde(default)]
    pub photo_ids: Vec<String>,
    /// The preset to run under. Absent means the default, which is Natural.
    #[serde(default)]
    pub preset: Option<String>,
    /// A cancel handle.
    #[serde(default)]
    pub cancel_id: Option<String>,
}

/// Ask for the frames whose retouch is worth a photographer attention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetouchReviewInput {
    /// The project.
    pub project_id: String,
    /// How many at most.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Record that the photographer has looked at one plan and agrees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptRetouchInput {
    /// The photograph.
    pub photo_id: String,
}

/// Record what the photographer set instead.
///
/// Both fields optional and independent. **A strength is gallery-wide**: setting one person
/// strength on one frame and not on the rest is how a gallery ends up with a bride whose skin
/// changes character between the ceremony and the reception.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRetouchInput {
    /// The project.
    pub project_id: String,
    /// The photograph.
    pub photo_id: String,
    /// The preset for this photograph.
    #[serde(default)]
    pub preset: Option<String>,
    /// A person, when setting their gallery-wide strength.
    #[serde(default)]
    pub identity_id: Option<String>,
    /// Their strength, `0..1`.
    #[serde(default)]
    pub strength: Option<f32>,
}

/// What recording a retouch override did, on both sides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRetouchDto {
    /// The stored plan, after the override.
    pub plan: RetouchPlanDto,
    /// The recipe, after the merge.
    pub recipe: RecipeDto,
    /// Which recipe fields changed.
    pub changed: Vec<String>,
    /// Which fields the merge refused to touch because a person owns them.
    pub protected: Vec<String>,
}

/// Add or clear one protected feature.
///
/// `protect = false` clears a feature. **It cannot clear an absolute one**: a tattoo is always
/// protected, `AURA-ML-5097` says so, and the panel renders it without a control rather than
/// with a disabled one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetProtectionInput {
    /// The project.
    pub project_id: String,
    /// Whose face.
    pub identity_id: String,
    /// The photograph the rectangle was drawn on, which is how it becomes face-normalised.
    pub photo_id: String,
    /// `mole`, `freckle`, `birthmark`, `scar`, `tattoo` or `dimple`.
    pub kind: String,
    /// The region, in **frame** coordinates as the panel drew it.
    pub area: CropRectDto,
    /// True to protect, false to clear.
    pub protect: bool,
}

// ---------------------------------------------------------------------------
// PHASE-21. The micro-retouch surface.
// ---------------------------------------------------------------------------

/// One small fix, as the panel draws it.
///
/// The five operators, flattened into one shape: `kind` says which, and only the fields that
/// operator uses are non-zero. A tagged union on the wire would be more precise and would make
/// the panel's list rendering a switch with five arms over shapes that differ in one field each.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroOpDto {
    /// `flyaway`, `teeth`, `eyes`, `clothing` or `glare`.
    pub kind: String,
    /// How strongly it ran, `0..1`, as a fraction of that operator's own ceiling.
    pub strength: f32,
    /// Where it acted, for the three operators that name a rectangle.
    pub region: Option<CropRectDto>,
    /// Whose face, for the two that name a person.
    pub identity_id: Option<String>,
    /// Teeth luminance lift in stops. Bounded at 0.20.
    pub luma_ev: f32,
    /// Teeth yellow reduction, as a share of the measured excess. Bounded at 0.35.
    pub yellow_reduce: f32,
    /// Sclera redness reduction, as a share of the measured excess. Bounded at 0.30.
    pub sclera: f32,
    /// Iris local contrast gain. Bounded at 0.25.
    pub iris_clarity: f32,
    /// `lint`, `thread`, `stain`, `strap` or `crease`, for a clothing operation.
    pub clothing_kind: Option<String>,
    /// `reduce` or `borrow`, for a glare operation.
    pub method: Option<String>,
    /// **The disclosure.** The photograph these pixels came from, for a borrow.
    ///
    /// Never absent on an operation whose `method` is `borrow`; the schema refuses one, and the
    /// panel renders a borrowed region with a visible marker rather than as an ordinary edit.
    pub borrowed_from: Option<String>,
    /// How well the two regions aligned, `0..1`, for a borrow.
    pub alignment: f32,
}

/// What the naturalness guard measured on the rendered result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NaturalnessReportDto {
    /// Peak iris luminance after the plan over the same before it. Held at or above 0.98.
    pub catchlight_ratio: f32,
    /// Hair-region edge energy after over before. Held at or above 0.94.
    pub hair_energy_ratio: f32,
    /// How much further outside the locus the plan pushed the teeth. Held below 0.003.
    pub teeth_excursion: f32,
    /// How many pixels the three measurements were taken over, summed.
    ///
    /// The panel shows the ratios to three decimal places only when this is large enough to mean
    /// something. A ratio over eleven samples is arithmetic rather than evidence.
    pub measured_on: u32,
    /// How many times a family gave up strength to reach its bound.
    pub resolves: u8,
    /// Which families were withdrawn, in `hair`, `teeth`, `eyes` order.
    pub withdrawn: Vec<bool>,
    /// The names of those three families, so the panel never hard-codes the order.
    pub families: Vec<String>,
}

/// One reason the plan came out the way it did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroReasonDto {
    /// The stable slug.
    pub code: String,
    /// The sentence a photographer reads.
    pub text: String,
    /// How much this moved the confidence. Negative for a doubt.
    pub weight: f32,
    /// True when the code withdraws a claim rather than making one.
    pub doubt: bool,
    /// The rectangle this reason is about, when it is about one.
    pub evidence: Option<CropRectDto>,
}

/// One photograph's micro-retouch plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroPlanDto {
    /// The photograph.
    pub photo_id: String,
    /// What was done.
    pub ops: Vec<MicroOpDto>,
    /// What the guard measured.
    pub naturalness: NaturalnessReportDto,
    /// Which operations the matrix permitted on this frame, in operator order.
    pub allowed: Vec<bool>,
    /// The operator names, so the panel never hard-codes the order.
    pub operators: Vec<String>,
    /// Why. Never empty.
    pub reasons: Vec<MicroReasonDto>,
    /// How much the plan trusts itself, `0..1`.
    pub confidence: f32,
    /// What the photograph is of.
    pub scene: String,
    /// Share of the shared per-image perceptual allowance this plan spent.
    pub budget_used: f32,
    /// **The disclosure, per frame.** Every photograph this plan borrowed pixels from.
    pub borrowed_from: Vec<String>,
    /// True when a photographer changed what may run.
    pub user_edited: bool,
    /// True when a photographer has looked at this plan and agreed.
    pub reviewed: bool,
    /// Which heads produced the detections.
    pub model_ver: u32,
    /// Which build's arithmetic produced the decisions.
    pub analysis_ver: u32,
    /// Which matrix file the switches came from.
    pub matrix_ver: u32,
}

/// What the Micro-Retouch panel's project header shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs with a plan.
    pub planned: u32,
    /// Fraction of the project with a plan, `0..1`. The denominator is every photograph.
    pub coverage: f32,
    /// Photographs where at least one operation ran.
    pub acted_on: u32,
    /// Photographs where the regions this phase needs arrived from phase 18.
    pub region_covered: u32,
    /// How many operations of each kind ran, in operator order.
    pub op_counts: Vec<u32>,
    /// The operator names.
    pub operators: Vec<String>,
    /// **How many frames in this gallery composited pixels from another.**
    ///
    /// On the project header rather than buried per frame, because the question a photographer is
    /// asked by a client is whether any of this got composited, not whether one frame did.
    pub borrows: u32,
    /// How many families were withdrawn across the project, in family order.
    pub withdrawn_counts: Vec<u32>,
    /// The family names.
    pub families: Vec<String>,
    /// Frames where a family gave up strength to reach its bound.
    pub resolved: u32,
    /// Mean catchlight ratio over frames that had eye work.
    pub mean_catchlight_ratio: f32,
    /// Mean hair energy ratio over frames that had hair work.
    pub mean_hair_energy_ratio: f32,
    /// Frames below the review threshold.
    pub needs_review: u32,
    /// Frames a photographer has changed by hand.
    pub user_edited: u32,
    /// Scenes with no row in the matrix file.
    pub unlisted_scenes: Vec<String>,
    /// The versions the stored plans were made under.
    pub model_ver: u32,
    /// The build arithmetic they were made under.
    pub analysis_ver: u32,
    /// The matrix file they were made under.
    pub matrix_ver: u32,
}

/// Which operations a project permits.
///
/// **There is no strength field and no ceiling field, and there never will be.** A photographer
/// chooses which small fixes run; how far each may go is bounded by the contract, and a surface
/// that could raise a ceiling would make `docs/retouch-ethics.md` a description of the defaults
/// rather than a promise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroMatrixDto {
    /// Which operations may run, in operator order.
    pub allowed: Vec<bool>,
    /// The operator names.
    pub operators: Vec<String>,
    /// Which clothing issues may be cleaned, in issue order.
    pub clothing: Vec<bool>,
    /// The issue names.
    pub clothing_kinds: Vec<String>,
    /// Which of those issues are opt-in only and start switched off.
    pub clothing_opt_in: Vec<bool>,
    /// Whether cross-frame borrowing is permitted at all.
    ///
    /// Separate from the glare switch deliberately: a studio can want reflections calmed and want
    /// no composited pixels in a delivery.
    pub borrowing: bool,
}

/// Record which operations a project permits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetMicroMatrixInput {
    /// The project.
    pub project_id: String,
    /// Which operations may run, in operator order. Absent leaves them alone.
    #[serde(default)]
    pub allowed: Option<Vec<bool>>,
    /// Which clothing issues may be cleaned. Absent leaves them alone.
    #[serde(default)]
    pub clothing: Option<Vec<bool>>,
    /// Whether cross-frame borrowing is permitted. Absent leaves it alone.
    #[serde(default)]
    pub borrowing: Option<bool>,
}

/// Run the resumable micro-retouch pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroPassInput {
    /// The project.
    pub project_id: String,
    /// `visible`, `interactive`, `ai_batch` or `background`.
    #[serde(default)]
    pub priority: Option<String>,
    /// Switch the whole stage off for this run. The kill switch hard rule 8 requires.
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// What one pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroPassDto {
    /// Photographs planned.
    pub planned: u32,
    /// Photographs that could not be planned.
    pub failed: u32,
    /// Photographs where at least one operation ran.
    pub acted_on: u32,
    /// Photographs where at least one usable region arrived.
    pub region_covered: u32,
    /// Operations of each kind, in operator order.
    pub ops: Vec<u32>,
    /// Frames that borrowed pixels.
    pub borrows: u32,
    /// Mean alignment over the borrows that happened.
    pub mean_alignment: f32,
    /// Families withdrawn, in family order.
    pub withdrawn: Vec<u32>,
    /// Frames where a family gave up strength.
    pub resolved: u32,
    /// Frames below the review threshold.
    pub low_confidence: u32,
    /// Scenes planned against the neutral row.
    pub unlisted_scenes: Vec<String>,
    /// Milliseconds the pass took.
    pub elapsed_ms: u64,
    /// True when the pass stopped early.
    pub cancelled: bool,
}

/// Ask for the frames worth a photographer's attention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroReviewInput {
    /// The project.
    pub project_id: String,
    /// How many at most.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Record that a photographer has looked at one plan and agrees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptMicroInput {
    /// The photograph.
    pub photo_id: String,
}

/// One frame that composited pixels from another, and where they came from.
///
/// **The disclosure list.** Read by the panel, by the delivery report and by phase 27, all through
/// the same view, so no two of them can disagree about what was composited.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroCompositeDto {
    /// The photograph that was repaired.
    pub photo_id: String,
    /// The photographs its pixels were borrowed from.
    pub source_photo_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// PHASE-22. The restoration surface.
// ---------------------------------------------------------------------------

/// What happened to one face the restoration pass considered.
///
/// **Every face gets one of these, whether it was recovered or not.** ADR-0048 section 4: a panel
/// that only listed what happened would make a careful product look like a careless one, and two
/// thirds of this phase's reason codes are refusals.
///
/// `identityDrift` is on the wire whether the face was kept or skipped, so the panel can show a
/// measured distance beside the sentence rather than a bare refusal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreFaceDto {
    /// Whose face, when phase 06 has assigned one.
    pub identity_id: Option<String>,
    /// Where it is, in **frame** coordinates as the panel draws it.
    pub area: CropRectDto,
    /// The measured sharpness that decided whether it was inside the soft band, `0..1`.
    pub sharpness: f32,
    /// The recovery strength that survived, `0..0.4`. Zero when the face was skipped.
    pub strength: f32,
    /// How far the phase 06 embedding moved, `0..1`. **Never above 0.08 on a kept face.**
    pub identity_drift: f32,
    /// How many times the strength was reduced to bring it back.
    pub resolves: u8,
    /// True when nothing was applied to this face.
    pub skipped: bool,
    /// Why it was skipped, as a `RestoreCode` slug.
    pub skipped_because: Option<String>,
}

/// What the artefact self-check measured on the rendered result.
///
/// Three numbers rather than one score, and ADR-0047 section 2.1 has the argument: smearing is
/// fixed by lowering the denoise tier, ringing by reducing the sharpen amount, and drift by the
/// identity constraint. A photographer whose complaint is that an edge looks crunchy needs the
/// ringing figure rather than a score that averaged it with something else.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtefactReportDto {
    /// High-band energy outside the face, after over before. Held at or above 0.90.
    pub texture_retention: f32,
    /// Mean edge overshoot on the strongest edges, `0..1`. Held below 0.020.
    pub ringing: f32,
    /// The largest identity movement over the faces that were kept, `0..1`. Held at or below 0.08.
    pub identity_drift: f32,
    /// How many pixels the first two were measured over.
    ///
    /// The panel shows three decimal places only when this is large enough to mean something. A
    /// ratio over eleven samples is arithmetic rather than evidence - phase 21's rule.
    pub measured_on: u32,
    /// How many times a strength was reduced to reach the bounds.
    pub resolves: u8,
    /// True when the denoise tier was stepped down by the self-check.
    pub denoise_reduced: bool,
    /// True when sharpening was reduced or withdrawn.
    pub sharpen_reduced: bool,
    /// True when at least one face was skipped for drift.
    pub face_skipped: bool,
}

/// One reason a restoration came out the way it did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReasonDto {
    /// The stable slug.
    pub code: String,
    /// The sentence a photographer reads.
    pub text: String,
    /// Which of the four decisions this is about: `denoise`, `sharpen`, `face_recovery`, `plan`.
    ///
    /// The panel groups thirty codes into four groups with this, rather than hard-coding which
    /// code belongs where.
    pub subject: String,
    /// How much this reason moved the decision, `-1..1`.
    pub weight: f32,
    /// True when this says something did **not** happen.
    pub restraint: bool,
    /// Where in the frame, when naming a place helps.
    pub area: Option<CropRectDto>,
}

/// One photograph's restoration plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct RestorePlanDto {
    /// The photograph.
    pub photo_id: String,
    /// `off`, `light`, `standard` or `strong`.
    pub denoise: String,
    /// Luminance noise reduction, `0..1`. Absent when the tier is `off`.
    pub denoise_luminance: Option<f32>,
    /// Chroma noise reduction, `0..1`. Never below the luminance figure.
    pub denoise_colour: Option<f32>,
    /// The sensor sigma the tier was chosen from, in linear working-space units.
    pub denoise_sigma: Option<f32>,
    /// The camera body whose noise model conditioned it.
    pub denoise_camera: Option<String>,
    /// True when that model was measured rather than derived from a specification.
    ///
    /// **False on every body in this build.** An unmeasured model caps the tier at `standard`.
    pub denoise_measured: bool,
    /// The estimated blur kernel, in pixels of Gaussian sigma. Absent when nothing was sharpened.
    pub sharpen_kernel: Option<f32>,
    /// How much of the deconvolution was applied, `0..0.5`.
    pub sharpen_amount: f32,
    /// The share of that amount withheld on skin, `0..1`.
    pub sharpen_skin_attenuation: f32,
    /// Fraction of the frame the sharpening acted on, `0..1`.
    pub sharpen_coverage: f32,
    /// The plan-wide face-recovery strength, `0..0.4`.
    pub face_recovery: f32,
    /// One record per face considered.
    pub faces: Vec<RestoreFaceDto>,
    /// How many faces were recovered.
    pub faces_recovered: u32,
    /// **How many were declined to keep somebody looking like themselves.**
    pub faces_skipped_identity: u32,
    /// What the self-check measured. Absent on a frame nothing was done to.
    pub selfcheck: Option<ArtefactReportDto>,
    /// `local_gpu`, `local_cpu` or `cloud`. Never `cloud` in this build.
    pub run_where: String,
    /// `export` or `background`. There is no interactive value.
    pub run_when: String,
    /// True when phase 18 supplied at least one usable region.
    pub region_covered: bool,
    /// Why, strongest first.
    pub reasons: Vec<RestoreReasonDto>,
    /// How much the plan trusts itself, `0..1`.
    pub confidence: f32,
    /// The scene it was decided under.
    pub scene: String,
    /// True when a photographer changed it.
    pub user_edited: bool,
    /// True when a photographer has looked at it and agreed.
    pub reviewed: bool,
}

/// What the Restore panel's project header shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs with a plan.
    pub planned: u32,
    /// Fraction of the project with a plan, `0..1`. **The denominator is every photograph.**
    pub coverage: f32,
    /// Photographs where at least one operation ran.
    pub acted_on: u32,
    /// Photographs where the regions this phase needs arrived from phase 18.
    pub region_covered: u32,
    /// How many frames got each tier, in `off`, `light`, `standard`, `strong` order.
    pub tiers: Vec<u32>,
    /// The names of those four tiers, so the panel never hard-codes the order.
    pub tier_names: Vec<String>,
    /// Frames that were sharpened.
    pub sharpened: u32,
    /// Why sharpening was refused, as `(code, count)` pairs, commonest first.
    ///
    /// A histogram rather than a count: "AURA sharpened nothing in this wedding" has six causes
    /// and five of them are somebody else's bug. ADR-0048 section 7.
    pub sharpen_refusals: Vec<RestoreRefusalDto>,
    /// Faces recovered across the project.
    pub faces_recovered: u32,
    /// **Faces declined to keep somebody looking like themselves.**
    pub faces_skipped_identity: u32,
    /// The largest identity movement over every kept face in the project, `0..1`.
    ///
    /// Section 10.1's gate, as one number: it is never above 0.08 on a sound catalog.
    pub worst_identity_drift: f32,
    /// Mean texture retention over frames that were denoised.
    pub mean_texture_retention: f32,
    /// Mean ringing over frames that were sharpened.
    pub mean_ringing: f32,
    /// Frames where the self-check reduced or withdrew something.
    pub reduced: u32,
    /// Frames below the review threshold.
    pub needs_review: u32,
    /// Frames a photographer has changed by hand.
    pub user_edited: u32,
    /// Camera bodies denoised against a synthetic noise model, by name.
    ///
    /// **Every body in this build.** A studio that sees its main camera here knows why its
    /// dance-floor frames are capped at `standard`. ADR-0048 section 7.
    pub unmeasured_cameras: Vec<String>,
    /// Scenes with no row in the profile file.
    pub unlisted_scenes: Vec<String>,
    /// The versions the stored plans were made under: heads, arithmetic, profiles.
    pub versions: Vec<u16>,
}

/// One reason sharpening was refused, and how often.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreRefusalDto {
    /// The stable slug.
    pub code: String,
    /// The sentence a photographer reads.
    pub text: String,
    /// How many frames.
    pub count: u32,
}

/// Ask for the frames worth a photographer's attention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReviewInput {
    /// The project.
    pub project_id: String,
    /// How many at most.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Record that a photographer has looked at one plan and agrees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptRestoreInput {
    /// The photograph.
    pub photo_id: String,
}

/// Record what a photographer chose for one photograph.
///
/// **A tier and two switches, and no other number.** ADR-0048 section 3: the line is between
/// *which of four* and *how far each goes*. A photographer choosing `standard` on a frame AURA put
/// at `light` is making a judgement about their own photograph; one setting a luminance amount is
/// overriding a decision conditioned on the camera's noise model, and the number would mean
/// something different on their other body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRestoreOverrideInput {
    /// The photograph.
    pub photo_id: String,
    /// `off`, `light`, `standard` or `strong`. Absent leaves AURA's own choice alone.
    #[serde(default)]
    pub denoise: Option<String>,
    /// Whether sharpening may run. Absent leaves the decision alone.
    #[serde(default)]
    pub sharpen: Option<bool>,
    /// Whether face recovery may run. Absent leaves the decision alone.
    ///
    /// Separate from `sharpen` because a photographer can want a frame sharpened and want no model
    /// near anybody's face.
    #[serde(default)]
    pub face_recovery: Option<bool>,
}

/// Run the resumable restoration pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePassInput {
    /// The project.
    pub project_id: String,
    /// `export` or `background`. **There is no interactive value**, and there is no variant of
    /// `RestoreWhen` that could carry one.
    #[serde(default)]
    pub when: Option<String>,
    /// `visible`, `interactive`, `ai_batch` or `background`.
    #[serde(default)]
    pub priority: Option<String>,
    /// The delivery's long edge in pixels, for the output-size modifier.
    #[serde(default)]
    pub output_long_edge: Option<u32>,
    /// Switch the whole stage off for this run. The kill switch hard rule 8 requires.
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// What one pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePassDto {
    /// Photographs planned.
    pub planned: u32,
    /// Photographs that could not be planned.
    pub failed: u32,
    /// Photographs where at least one operation ran.
    pub acted_on: u32,
    /// Photographs where at least one usable region arrived.
    pub region_covered: u32,
    /// How many frames got each tier, in tier order.
    pub tiers: Vec<u32>,
    /// Frames that were sharpened.
    pub sharpened: u32,
    /// Faces recovered.
    pub faces_recovered: u32,
    /// Faces declined to keep somebody looking like themselves.
    pub faces_skipped_identity: u32,
    /// Frames where the self-check reduced or withdrew something.
    pub reduced: u32,
    /// Frames below the review threshold.
    pub low_confidence: u32,
    /// Camera bodies denoised against a synthetic noise model.
    pub unmeasured_cameras: Vec<String>,
    /// Scenes planned against the neutral row.
    pub unlisted_scenes: Vec<String>,
    /// Milliseconds the pass took.
    pub elapsed_ms: u64,
    /// True when the pass stopped early.
    pub cancelled: bool,
}

/// One frame whose face recovery was declined to keep somebody looking like themselves.
///
/// **The guarantee's own list**, and it is on the surface deliberately. Section 10.1 gates
/// identity preservation at 100 %, and a gate that can only be checked by opening four hundred
/// plans one at a time is a gate nobody checks. ADR-0048 section 4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreIdentityRefusalDto {
    /// The photograph.
    pub photo_id: String,
    /// The largest distance a face in it moved before being declined, `0..1`.
    pub worst_drift: f32,
    /// How many faces in it were declined.
    pub faces: u32,
}

// ---------------------------------------------------------------------------
// PHASE-23 - geometry
//
// Six commands. Three read - the project's coverage, one photograph's plan and the review
// queue - one runs the resumable pass, and two record what the photographer decided.
//
// **No command returns a pixel and none returns a lens profile table.** What the panel gets is
// rectangles, an angle, a set of coefficients and reason codes; the profile table is an input
// to a decision and reaches the wire only as the name of the profile that matched and the
// `lensSynthetic` flag that says whether anybody measured it.
// ---------------------------------------------------------------------------

/// One crop the plan carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropVariantDto {
    /// `CropPurpose::as_str`: original, primary, album, social, wide.
    pub purpose: String,
    /// What the panel calls it.
    pub title: String,
    /// `Aspect::as_str`: original, 4:5, 5:4, 1:1, 16:9.
    pub aspect: String,
    /// The rectangle, normalised to the corrected frame.
    pub rect: CropRectDto,
    /// The objective's score, comparable with the original's by construction.
    pub score: f32,
    /// True when it passed every safety rule. False only on a row re-checked under newer rules.
    pub safe: bool,
}

/// What the safety filter checked and found.
///
/// Three booleans plus two counts, which is one more boolean than
/// `clippy::struct_excessive_bools` likes. The lint is allowed rather than obeyed: all three
/// are in section 5's frozen `CropSafetyReport`, and its remedy - gathering them into a
/// sub-struct - would rename frozen fields on the wire to satisfy a style rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct CropSafetyDto {
    /// Every detected face and every primary pair of hands is inside the delivered crop.
    pub faces_intact: bool,
    /// The crop keeps enough of the original long edge.
    pub resolution_ok: bool,
    /// What the frame is about is inside.
    pub content_kept: bool,
    /// How many faces were checked. **Zero is "nothing was checked", never "nothing was cut".**
    pub faces_checked: u32,
    /// How many pairs of hands were checked. Zero on every photograph in this build.
    pub hands_checked: u32,
    /// True when at least one region was actually checked. The predicate the panel asks before
    /// it says "safe".
    pub is_evidence: bool,
    /// Refusals in `GeometryCode::REFUSALS` order: face, hands, resolution, content.
    pub refused: Vec<u32>,
    /// The names of those four, so the panel does not keep its own copy of the order.
    pub refused_names: Vec<String>,
}

/// One reason, with the pixels behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeometryReasonDto {
    /// `GeometryCode::as_str`.
    pub code: String,
    /// The sentence a photographer reads.
    pub text: String,
    /// How much this moved the confidence. Negative for a doubt.
    pub weight: f32,
    /// True when this describes something the product declined to do.
    pub restraint: bool,
    /// What would have been lost, when the reason is about a region.
    pub evidence: Option<CropRectDto>,
}

/// One photograph's finished frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeometryPlanDto {
    /// The photograph.
    pub photo_id: String,
    /// Which scene the bands were conditioned on.
    pub scene: String,
    /// `LensSource::as_str`: none, embedded, profile, estimated.
    pub lens_source: String,
    /// The lens as EXIF named it, whether or not it matched.
    pub lens_id: Option<String>,
    /// The profile that produced the numbers, when one did.
    pub lens_profile: Option<String>,
    /// True when that profile was fabricated rather than measured. **On this build, always.**
    pub lens_synthetic: bool,
    /// Brown-Conrady radial terms.
    pub distortion: Vec<f32>,
    /// Vignette correction strength, `0..1`.
    pub vignette: f32,
    /// Per-channel radial scale for red and blue relative to green.
    pub ca: Vec<f32>,
    /// Rotation in degrees, positive clockwise. Zero when nothing was levelled.
    pub rotate_deg: f32,
    /// How sure the rotation is.
    pub rotate_conf: f32,
    /// Vertical keystone in `-100..100`, or `None`.
    pub keystone_vertical: Option<f32>,
    /// Horizontal keystone.
    pub keystone_horizontal: Option<f32>,
    /// The largest per-axis stretch the keystone applies.
    pub keystone_stretch: Option<f32>,
    /// How many verticals it was fitted from.
    pub keystone_verticals: u32,
    /// Every crop. Never empty; index zero is always the frame as shot.
    pub crops: Vec<CropVariantDto>,
    /// Which entry is delivered. Zero when the framing was kept.
    pub primary_crop: u32,
    /// True when the framing as shot survived.
    pub kept_original: bool,
    /// What the filter checked and found.
    pub safety: CropSafetyDto,
    /// Why, worst first.
    pub reasons: Vec<GeometryReasonDto>,
    /// How sure the whole plan is.
    pub confidence: f32,
    /// Which lens profile table.
    pub profile_ver: u32,
    /// Which arithmetic.
    pub analysis_ver: u32,
    /// Which rules file.
    pub rules_ver: u32,
    /// True when the photographer set the framing themselves.
    pub user_edited: bool,
}

/// What the Geometry panel's project header shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeometryStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs with a plan.
    pub planned: u32,
    /// Fraction of the project with a plan. The denominator is every photograph.
    pub coverage: f32,
    /// Fraction of planned frames delivered as shot. **Above 0.70 is the passing direction** -
    /// the only number in the product where more restraint is a better result.
    pub kept_original: f32,
    /// Fraction whose lens was corrected from a measured profile.
    pub profile_covered: f32,
    /// How many frames were levelled.
    pub levelled: u32,
    /// Mean absolute rotation over those, in degrees.
    pub mean_rotate_deg: f32,
    /// How many were keystoned.
    pub keystoned: u32,
    /// Variants produced, in `CropPurpose::ALL` order.
    pub variant_counts: Vec<u32>,
    /// The names of those, so the panel keeps no copy of the order.
    pub variant_names: Vec<String>,
    /// Refusals, in `GeometryCode::REFUSALS` order.
    pub refused_counts: Vec<u32>,
    /// The names of those four.
    pub refused_names: Vec<String>,
    /// Lens ids with no profile, most frequent first, at most twenty.
    pub missing_profiles: Vec<String>,
    /// Scenes planned with no row in `crop_rules.toml`.
    pub unpolicied_scenes: Vec<String>,
    /// Frames worth a photographer's attention.
    pub needs_review: u32,
    /// Frames the photographer framed themselves.
    pub user_edited: u32,
    /// True when any bundled lens profile was fabricated rather than measured.
    pub profiles_synthetic: bool,
    /// How many lenses the bundled table knows.
    pub profiles_known: u32,
    /// Which lens profile table.
    pub profile_ver: u32,
    /// Which arithmetic.
    pub analysis_ver: u32,
    /// Which rules file.
    pub rules_ver: u32,
}

/// Run the geometry pass over a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanGeometryInput {
    /// The project.
    pub project_id: String,
    /// How many photographs to plan in this call. Defaults to everything outstanding.
    pub limit: Option<u32>,
}

/// What one run of the geometry pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeometryPassDto {
    /// Photographs planned.
    pub planned: u32,
    /// Photographs that could not be planned. `AURA-ML-5092`, one at a time.
    pub failed: u32,
    /// Fraction of the planned frames delivered as shot.
    pub kept_original: f32,
    /// Recipes written through the merge.
    pub recipes_written: u32,
    /// Recipes the merge refused to touch because a person had set the field.
    pub recipes_protected: u32,
    /// Milliseconds for the pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

/// Ask for the frames whose geometry is worth a photographer's attention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeometryReviewInput {
    /// The project.
    pub project_id: String,
    /// How many to return. Defaults to 200, capped at 5,000.
    pub limit: Option<u32>,
}

/// Record the framing the photographer chose, and write it into the recipe.
///
/// **Reverting is this command with the whole frame and zero degrees**, not a separate one.
/// A revert implemented as *clearing* the row would be a revert the next pass undoes; recorded
/// as an override it survives a re-analysis exactly as any other choice does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetFramingInput {
    /// The project.
    pub project_id: String,
    /// The photograph.
    pub photo_id: String,
    /// The rectangle, normalised to the corrected frame.
    pub rect: CropRectDto,
    /// The angle, in degrees. `-45..45`.
    pub rotate_deg: f32,
    /// Which aspect they were working in. `Aspect::as_str`.
    pub aspect: String,
}

/// What recording a framing did, on both sides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetFramingDto {
    /// The plan after the override, with `userEdited` set.
    pub plan: GeometryPlanDto,
    /// The edit after the merge.
    pub recipe: RecipeDto,
    /// The dotted paths that moved.
    pub changed: Vec<String>,
    /// The dotted paths a person now owns.
    pub protected: Vec<String>,
}

/// Record that the photographer has looked at one plan and agrees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptGeometryInput {
    /// The photograph.
    pub photo_id: String,
}

// =================================================================================================
// PHASE-24. The cleanup surface. ADR-0050.
//
// Eight commands. Four read - the project coverage, one photograph's proposals, one photograph's
// refusals, and the delivery report - one runs the resumable pass, and three record what the
// photographer decided.
//
// WHAT IS NOT HERE, AND CANNOT BE ADDED WITHOUT AN ADR
//
// **No command carries a strength, a size or a description.** The only things a person can say on
// this surface are yes, no, and "leave this photograph alone". There is no field a prompt could go
// in, which is how `docs/generative-policy.md`'s promise that AURA never generates from a
// description is kept - as a property of the shapes rather than as a default somebody could change.
//
// **No command returns pixels.** Phase 13's rule, and the panel renders its before-and-after from
// `render_image` with and without the recipe's `cleanup[]`, which is the same region asked for
// twice. What this surface adds is the *rectangle and the method* those two renders differ by.
//
// **No command can raise a cap.** The contract owns `AREA_CAP_DEFAULT`, `DENYLIST_OVERLAP_MAX` and
// `ZERO_TOUCH_CONFIDENCE`, `cleanup_policy.toml` may only tighten them, and nothing on the wire
// touches any of the three.
//
// **`manual_remove` is the one command that acts on a region a person chose**, and it is
// deliberately the most constrained thing here: it still runs the whole safety engine, it still
// refuses a person, and it records `manual_removal` so the delivery report can tell a
// photographer's own removal from AURA's. Section 2.2 makes removing a guest a human decision; it
// does not make it an unchecked one.
// =================================================================================================

/// One proposed removal, as the panel renders it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupProposalDto {
    /// The proposal.
    pub proposal_id: String,
    /// The photograph.
    pub photo_id: String,
    /// Where, normalised to the frame.
    pub region: CropRectDto,
    /// What it is, from the closed vocabulary. `unclassified` on every frame in this build.
    pub class: String,
    /// The words a photographer reads for the class.
    pub class_text: String,
    /// The share of the frame the region covers, `0..1`.
    pub area_frac: f32,
    /// How much attention it draws, `0..1`.
    pub salience: f32,
    /// `borrow`, `fill` or `inpaint`.
    pub method: String,
    /// The photograph the pixels would come from, when the method is `borrow`.
    pub borrowed_from: Option<String>,
    /// Which model would make them up, when the method is `inpaint`. Never set in this build.
    pub model: Option<String>,
    /// How sure the whole proposal is, `0..1`.
    pub confidence: f32,
    /// What the self-check measured on the result, `0..1`, lower is cleaner.
    pub artefact_score: f32,
    /// Phase 13's band, raised one for this phase and again while nothing is calibrated.
    pub autonomy: String,
    /// The scene the thresholds were conditioned on.
    pub scene: String,
    /// Why, worst first.
    pub reasons: Vec<CleanupReasonDto>,
    /// `true` accepted, `false` rejected, absent undecided.
    pub accepted: Option<bool>,
    /// True when the removal has been written into the recipe.
    pub applied: bool,
    /// True when this may be applied without anybody looking. False everywhere in this build.
    pub may_apply_unattended: bool,
    /// The detector, the safety arithmetic and the policy table it was made under.
    pub versions: Vec<u16>,
}

/// One reason, as the panel renders it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupReasonDto {
    /// The stable code.
    pub code: String,
    /// The sentence a photographer reads.
    pub text: String,
    /// How much this reason contributed, `0..1`.
    pub weight: f32,
    /// True when this code records something the product declined to do.
    pub is_refusal: bool,
    /// The pixels behind it, when there are any.
    pub evidence: Option<CropRectDto>,
}

/// One candidate the safety engine refused, as the panel renders it.
///
/// **The panel shows these on request rather than by default**, which is why they are a separate
/// command: the refused set is usually larger than the proposed one, and a queue that rendered
/// forty refusals it will not draw would be slower for no benefit. But they are on the surface at
/// all because teaching a photographer what AURA will never do is most of the trust this feature
/// needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupBlockedDto {
    /// Where.
    pub region: CropRectDto,
    /// Which of the five checks stopped it.
    pub check: String,
    /// The reason code.
    pub code: String,
    /// The sentence a photographer reads.
    pub text: String,
}

/// One removal that happened, for the delivery report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupDisclosureDto {
    /// The proposal.
    pub proposal_id: String,
    /// The photograph.
    pub photo_id: String,
    /// `borrow`, `fill` or `inpaint`.
    pub method: String,
    /// Where the pixels came from, when the method is `borrow`.
    pub borrowed_from: Option<String>,
    /// Which model, when the method is `inpaint`.
    pub model: Option<String>,
    /// Where.
    pub region: CropRectDto,
    /// True when a person accepted it rather than a mode applying it.
    pub accepted_by_user: bool,
    /// What the self-check measured, `0..1`.
    pub artefact_score: f32,
}

/// What the Cleanup panel's project header shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs the pass has examined.
    pub examined: u32,
    /// Fraction examined. **The denominator is every photograph.**
    pub coverage: f32,
    /// Photographs carrying at least one proposal.
    pub with_proposals: u32,
    /// Proposals that were applied.
    pub applied: u32,
    /// Candidates the safety engine refused, by check, in `SafetyCheck::ALL` order.
    pub blocked: Vec<u32>,
    /// The names of those five checks, in the same order.
    pub check_names: Vec<String>,
    /// Applied removals that borrowed real pixels from a sibling frame.
    pub borrowed: u32,
    /// Applied removals filled from this photograph's own texture.
    pub filled: u32,
    /// Applied removals a diffusion model produced. Zero in this build.
    pub inpainted: u32,
    /// Removals the self-check reverted before anybody saw them.
    pub reverted: u32,
    /// Fraction of examined frames whose six protected kinds could all be looked for.
    ///
    /// **The number to read first.** At zero, every candidate was refused for want of evidence
    /// rather than for want of safety, and the blocked histogram says nothing about what is in the
    /// photographs.
    pub mask_covered: f32,
    /// Whether a trained distraction detector is installed. False in this build.
    pub detector_trained: bool,
    /// Whether a diffusion inpainting model pack is installed. False in this build.
    pub inpaint_available: bool,
}

/// Run the cleanup pass over a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPassInput {
    /// The project.
    pub project_id: String,
    /// Only these photographs, or all pending ones when empty.
    ///
    /// The path the job graph uses, with phase 12's keepers: a distraction in a rejected frame is
    /// not a distraction. Invariant 3.
    #[serde(default)]
    pub photo_ids: Vec<String>,
}

/// What one pass did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPassDto {
    /// Photographs examined.
    pub examined: u32,
    /// Photographs carrying at least one proposal.
    pub with_proposals: u32,
    /// Proposals produced.
    pub proposals: u32,
    /// Candidates refused, by check.
    pub blocked: Vec<u32>,
    /// Removals the self-check undid before anybody saw them.
    pub reverted: u32,
    /// Cloud editorial judgements made.
    pub judged: u32,
    /// How many of those declined a removal.
    pub declined: u32,
    /// Photographs that could not be examined.
    pub failed: u32,
    /// True when the run was cancelled part way.
    pub cancelled: bool,
    /// How long it took.
    pub elapsed_ms: u64,
}

/// Accept or reject one proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecideCleanupInput {
    /// The photograph.
    pub photo_id: String,
    /// The proposal.
    pub proposal_id: String,
    /// Yes or no. There is no third thing a person can say here.
    pub accept: bool,
}

/// Switch cleanup off, or back on, for one photograph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisableCleanupInput {
    /// The photograph.
    pub photo_id: String,
    /// True to leave this photograph alone entirely.
    pub disabled: bool,
}

/// Ask for one region of one photograph to be removed, by hand.
///
/// Section 2.2's "manual tool with explicit confirmation". It runs the **whole** safety engine:
/// the size cap, the denylist, the identity check, the structure check and the confidence check,
/// in that order, on a region a person drew. A person choosing a rectangle is a reason to skip the
/// *detector*, not a reason to skip the safety filter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualRemoveInput {
    /// The photograph.
    pub photo_id: String,
    /// The rectangle they drew, normalised to the frame.
    pub region: CropRectDto,
    /// Explicit confirmation. The command refuses without it.
    ///
    /// A separate field rather than an implication of calling the command, because section 2.2
    /// asks for explicit confirmation and a call is not one. A UI that forgot the dialog would get
    /// a refusal rather than a removal.
    pub confirmed: bool,
}

/// What a manual removal did, or refused to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualRemoveDto {
    /// The proposal, when one was produced.
    pub proposal: Option<CleanupProposalDto>,
    /// Which check refused it, when one did.
    pub blocked: Option<CleanupBlockedDto>,
}

// ---------------------------------------------------------------------------
// PHASE-25 - gallery consistency
// ---------------------------------------------------------------------------
//
// Nine commands and eight DTOs. ADR-0052 records the shape and what is deliberately absent from
// it; three things are worth repeating here because they are properties of these types rather than
// of the commands.
//
// **Both denominators are on the wire.** `GalleryStatusDto` carries `nodes` and `anchoredNodes`,
// and a project at 100 % coverage with 20 % anchored has had almost nothing done to it. Phase 05's
// rule at its most consequential: this is the phase where a green number and an untouched gallery
// look identical.
//
// **The spreads are sent, not the reduction.** A panel that received "77 % reduced" could not tell
// 500 K down to 115 K from 20 K down to 4.6 K, and only one of those is worth showing anybody.
//
// **There is no strength field, no damping field and no way to raise a bound.** Five optional
// movements, every one bounded by the frozen contract, refused rather than clamped when it is
// outside. A frame that needs to move further than 450 K is a frame whose *per-frame* estimate is
// wrong, and phase 15's own override is where that is fixed.

/// One reason a frame moved, or did not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryReasonDto {
    /// The stable slug a filter matches on. Never localised.
    pub code: String,
    /// The sentence a photographer reads. Rendered from the code, never stored.
    pub text: String,
    /// True when this code says the product declined to act.
    pub withdraws: bool,
}

/// What a node's anchors say it should look like.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeTargetDto {
    /// Kelvin.
    pub cct_k: f32,
    /// How far a frame may sit from it and still be consistent, in kelvin.
    pub cct_tol: f32,
    /// Tint units.
    pub tint: f32,
    /// How far a frame may sit, in tint units.
    pub tint_tol: f32,
    /// Subject luminance, `0..1`.
    pub subject_luma: f32,
    /// How far a frame may sit, `0..1`.
    pub luma_tol: f32,
    /// Contrast, in the recipe's units.
    pub contrast: f32,
    /// Saturation, in the recipe's units.
    pub saturation: f32,
    /// How many anchors it came from.
    pub anchor_count: u16,
    /// How much they agree, `0..1`.
    pub cohesion: f32,
}

/// One lighting group inside one chapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneNodeDto {
    /// The node.
    pub node_id: String,
    /// What it was split or sub-clustered out of.
    pub parent_id: Option<String>,
    /// The chapter.
    pub segment_id: String,
    /// What a photographer reads: "Ceremony (2 of 3)".
    pub label: String,
    /// The scene it was built under.
    pub scene: String,
    /// How many frames it holds.
    pub image_count: u32,
    /// Its anchors, best first.
    pub anchors: Vec<String>,
    /// What the anchors say, or `null` when the node could not be anchored.
    ///
    /// **Null is not a neutral target.** A panel that rendered it as one would turn "AURA could
    /// not judge this part of the wedding" into "this part needed nothing".
    pub target: Option<NodeTargetDto>,
    /// Why the node is shaped the way it is.
    pub reasons: Vec<GalleryReasonDto>,
}

/// How far one frame moves toward its node.
///
/// Every `d` field is a **residual** on top of phases 15 and 16, and the three `from` fields say
/// what it is a residual from - so a strip can draw an arrow from one to the other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryDeltaDto {
    /// The photograph.
    pub photo_id: String,
    /// Its node.
    pub node_id: String,
    /// Stops.
    pub d_exposure: f32,
    /// Kelvin.
    pub d_cct: f32,
    /// Tint units.
    pub d_tint: f32,
    /// Recipe units.
    pub d_contrast: f32,
    /// Recipe units.
    pub d_saturation: f32,
    /// What the exposure movement is measured from, in stops.
    pub from_exposure_ev: f32,
    /// What the temperature movement is measured from, in kelvin.
    pub from_cct_k: f32,
    /// What the tint movement is measured from.
    pub from_tint: f32,
    /// How much of the distance was travelled.
    pub damping: f32,
    /// Which bound clamped it: `cct`, `tint`, `exposure`, `contrast` or `saturation`.
    pub bounded_by: Option<String>,
    /// How much of the bounds this movement used, `0..1`.
    pub magnitude: f32,
    /// Whose skin was corrected, when any was.
    pub skin_identity: Option<String>,
    /// The dE00 before the skin correction.
    pub skin_de00_before: Option<f32>,
    /// The dE00 after it.
    pub skin_de00_after: Option<f32>,
    /// Invariant 2.
    pub confidence: f32,
    /// Why.
    pub reasons: Vec<GalleryReasonDto>,
    /// True when the photographer set these values.
    pub user_edited: bool,
}

/// A frame that is still out of line after normalising.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryOutlierDto {
    /// The photograph.
    pub photo_id: String,
    /// The node it should have matched.
    pub node_id: String,
    /// The sentence section 6.4 asks for, assembled from the residuals.
    ///
    /// On the wire rather than assembled in the panel, so this and phase 27's QC ticket say the
    /// same thing about the same frame.
    pub description: String,
    /// What is left, in kelvin. Signed: positive is warmer.
    pub residual_cct: f32,
    /// What is left, in tint units.
    pub residual_tint: f32,
    /// What is left, in stops.
    pub residual_exposure: f32,
    /// What is left on the worst identity's skin, in dE00.
    pub residual_skin_de00: f32,
    /// How far out overall, `0..1`.
    pub deviation: f32,
    /// Why.
    pub reasons: Vec<GalleryReasonDto>,
}

/// What the Consistency panel's project header shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryStatusDto {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs with a delta.
    pub normalised: u32,
    /// The first denominator, `0..1`.
    pub coverage: f32,
    /// Nodes in the tree.
    pub nodes: u32,
    /// Nodes with a usable target. **The second denominator, and the one that matters when it is
    /// low.**
    pub anchored_nodes: u32,
    /// Nodes a change point split.
    pub split_nodes: u32,
    /// Anchors a photographer pinned.
    pub pinned_anchors: u32,
    /// Frames a bound clamped.
    pub bounded: u32,
    /// Frames left alone because their light is intentional.
    pub mood_preserved: u32,
    /// Frames the photographer set by hand.
    pub user_edited: u32,
    /// Frames still out of line.
    pub outliers: u32,
    /// Identities with a gallery skin target.
    pub skin_targeted: u32,
    /// Identities seen at all.
    pub identities: u32,
    /// The within-node temperature spread before, in kelvin.
    pub spread_before_cct: f32,
    /// And after.
    pub spread_after_cct: f32,
    /// The within-node exposure spread before, in stops.
    pub spread_before_ev: f32,
    /// And after.
    pub spread_after_ev: f32,
    /// The worst per-identity skin spread after correction, in dE00.
    pub worst_skin_spread: f32,
    /// Scenes with no policy row.
    pub untargeted_scenes: Vec<String>,
    /// Whether this build can read a per-frame skin region.
    ///
    /// False. On the wire rather than inferred: a panel that guessed from `skinTargeted == 0`
    /// would eventually say "everybody's skin is consistent across this wedding" for a build that
    /// cannot look at skin, which is a promise about people.
    pub skin_field_available: bool,
    /// Which policy table the stored rows were bounded by.
    pub policy_ver: u16,
}

/// Run the consistency pass over a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryPassInput {
    /// The project.
    pub project_id: String,
}

/// What one consistency pass did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryPassDto {
    /// Nodes built.
    pub nodes: u32,
    /// Nodes with a usable target.
    pub anchored: u32,
    /// Nodes a change point split.
    pub split: u32,
    /// Frames with a delta.
    pub normalised: u32,
    /// Frames still out of line.
    pub outliers: u32,
    /// Identities with a skin target.
    pub skin_targets: u32,
    /// The within-node temperature spread before, in kelvin.
    pub spread_before_cct: f32,
    /// And after.
    pub spread_after_cct: f32,
    /// The within-node exposure spread before, in stops.
    pub spread_before_ev: f32,
    /// And after.
    pub spread_after_ev: f32,
    /// Photographer decisions carried across the re-pass.
    pub decisions_kept: u32,
    /// True when the run was cancelled part way. Nothing was written.
    pub cancelled: bool,
    /// How long it took.
    pub elapsed_ms: u64,
}

/// Pin or reject one photograph as an anchor of its node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinAnchorInput {
    /// The node.
    pub node_id: String,
    /// The photograph.
    pub photo_id: String,
    /// True to pin it, false to reject it. There is no third thing a person can say here.
    pub pinned: bool,
}

/// What the photographer set instead, on one frame.
///
/// Every field is optional and at least one must be present. Every one is bounded by the frozen
/// contract and a value outside its bound is **refused rather than clamped** - see the block header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryOverrideInput {
    /// The photograph.
    pub photo_id: String,
    /// Kelvin, within 450.
    pub d_cct: Option<f32>,
    /// Tint units, within 12.
    pub d_tint: Option<f32>,
    /// Stops, within 0.35.
    pub d_exposure: Option<f32>,
    /// Recipe units, within 8.
    pub d_contrast: Option<f32>,
    /// Recipe units, within 6.
    pub d_saturation: Option<f32>,
}

/// Switch the consistency pass off for one photograph, or back on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisableGalleryInput {
    /// The photograph.
    pub photo_id: String,
    /// True to leave this photograph out of the gallery match entirely.
    pub disabled: bool,
}
