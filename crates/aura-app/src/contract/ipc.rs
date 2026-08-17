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
