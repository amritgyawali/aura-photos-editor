//! FROZEN CONTRACT. The zero-touch autopilot orchestrator. PHASE-28 section 5.
//!
//! Twenty-seven phases decided things. This one decides **nothing**, and that is the whole of
//! what it is for.
//!
//! Every shape in this module sequences work that other phases own. There is no field here that
//! can hold a keep, a rejection, a strength, a parameter, a threshold or a confidence about a
//! photograph, because the moment the orchestrator can express one of those it has become a
//! twenty-eighth opinion about a wedding - and the product would then have two answers to
//! "why was this frame delivered", one of them belonging to the scheduler.
//!
//! `crates/aura-jobs/tests/no_decisions.rs` is the grep that keeps it true. It is the eighth
//! grep-as-a-test in this repository, after `colour_discipline.rs`, `no_recipe_writes.rs`,
//! `no_template_writes.rs`, `no_render_calls.rs`, `one_choke_point.rs`,
//! `aura-brain-gallery/tests/no_recipe_writes.rs` and `no_pixel_ops.rs`.
//!
//! ## The six properties this contract exists to make structural
//!
//! **The autopilot never grants itself permission.** [`StageId::decision_kind`] names which of
//! phase 13's six kinds a stage's decisions belong to, and the band comes from [`AutonomyGate`] -
//! a port this crate defines and `aura-explain` answers. There is no way to construct a
//! [`StageVerdict::Act`] except from a band: it arrives from the gate. A scheduler that could
//! compute its own band is a scheduler that could act on a wedding unattended by editing a
//! constant, which is exactly what phase 13's [`Autonomy`] exists to prevent.
//!
//! **A stage that could not run is skipped and named, never quietly passed.** [`SkipCause`] is a
//! closed set and every variant names what was absent. Phase 27 wrote this rule for an inspection
//! and this phase inherits it for a whole stage: a wedding whose cleanup stage never ran must not
//! finish as [`RunStatus::Completed`], because `Completed` is a claim and the run has no evidence
//! for it. [`RunStatus::CompletedDegraded`] carries [`RunSummary::degraded_stages`] and each entry
//! says which stage and why.
//!
//! **The governor can only make the product do less.** [`GovernorAction`] is `Proceed`, `Reduce`,
//! `Pause` and `Stop`, and there is no variant that raises concurrency, enlarges a batch or
//! disables a check. An unreadable temperature sensor, a machine on battery, a full disk and a
//! thermally throttled laptop therefore all reach the *same* conservative state, and a governor
//! that is wrong is a run that is slow rather than a run that cooked a photographer's laptop at
//! one in the morning. Phase 24 gave its cloud judgement this property; this is its second
//! application and the reason is identical.
//!
//! **A checkpoint is keyed by what the stage read.** [`Checkpoint::inputs_hash`] is a digest of
//! the stage's declared inputs - upstream stage versions, the policy version, the unit count - so
//! a resumed run replays only what is unfinished, and an upstream change invalidates *that stage
//! only* rather than the wedding. Section 6.1. A checkpoint keyed by time or by a run id would
//! resume happily onto stale work and nobody would find out.
//!
//! **The ETA is measured wherever there is a measurement.** [`Eta`] carries both the declared
//! per-item estimate a stage shipped with and the throughput this machine actually achieved, and
//! [`Eta::remaining_ms`] prefers the second the moment it exists. Section 6.4's accuracy gate is
//! about the *measured* half; the declared half exists so the first minute of a run has a number
//! at all, and [`Eta::measured`] says which one a photographer is looking at.
//!
//! **Cancellation is a boundary, not an interrupt.** The token is polled between units and never
//! inside a write, so a cancelled run leaves the catalog exactly as consistent as a finished one.
//! Section 10.1's "cancellation leaves no partial exports" is a property of where the token is
//! polled rather than of how quickly a thread dies.
//!
//! ## The one thing a later phase can get wrong
//!
//! **This is an orchestrator, and an orchestrator that acquires a fallback has acquired an
//! opinion.** Phase 19 wrote the rule - a phase that consumes another phase's output owns no
//! fallback for it - and here it is at its strongest: when a stage's service is absent the stage
//! is [`SkipCause::ServiceAbsent`], never a simpler version of the same work. Two answers to "what
//! did the cull do" is a gallery nobody can explain, and the second answer would be the
//! scheduler's.

use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use aura_core::contract::ids::{ProjectId, RunId};
use aura_core::contract::ledger::{Autonomy, DecisionKind};
use aura_core::contract::qc::QcReport;
use aura_core::contract::scene::ImageId;
use aura_core::progress::CancelToken;
use aura_core::AuraResult;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The most attempts a stage gets before the run gives up on it.
///
/// Three, which is `Task::max_attempts`'s default from phase 01 and is deliberately the same
/// number: a stage is a task graph, and a stage that retried more times than its own units may
/// would be a bound that contradicts the one underneath it.
pub const MAX_STAGE_ATTEMPTS: u8 = 3;

/// The first retry's delay in milliseconds; each further attempt doubles it.
///
/// Section 6.3's "retry with backoff". Two seconds rather than the hundred milliseconds a network
/// client would use, because nothing a stage retries is a transient packet: it is a driver that
/// has just reset, a disk that has just filled, or a file a backup tool still has open. Retrying
/// those in a hundred milliseconds is retrying them into the same failure.
pub const RETRY_BACKOFF_MS: u64 = 2_000;

/// The share of a run after which the ETA is held to its accuracy bound.
///
/// Section 10.1: "ETA within 20 % after 10 % of the run". Before that point there is not enough
/// measured throughput for an honest number and [`Eta::measured`] is false, which the panel renders
/// as an estimate rather than as a time.
pub const ETA_WARMUP_SHARE: f32 = 0.10;

/// How far the ETA may be from the truth once the warm-up share is past.
pub const ETA_TOLERANCE: f32 = 0.20;

/// Free disk required before a run starts, as a multiple of the estimated output size.
///
/// Section 6.2's own number. 1.6 rather than 1.0 because the estimate is of the *delivered* files
/// and a run also writes proxies, checkpoints, catalog pages and a report - and because a disk that
/// fills at 90 % of a two-hour run is the single most expensive failure this phase can have.
pub const DISK_HEADROOM: f32 = 1.6;

/// The share of video memory a run may occupy.
///
/// Section 11's budget row, expressed here because the governor enforces it rather than a test
/// observing it. Eighty per cent: the remaining fifth is what keeps the photographer's desktop
/// compositor, browser and catalog application alive while the run happens.
pub const VRAM_CEILING: f32 = 0.80;

/// How long a resume may take before it counts as a defect, in milliseconds.
///
/// Section 11. Twenty seconds is the whole of the resume: opening the catalog, reading every
/// stage's checkpoint, re-hashing the declared inputs and rebuilding the ready set.
pub const MAX_RESUME_MS: u64 = 20_000;

/// The temperature in degrees Celsius above which the governor reduces concurrency.
///
/// A policy number, and it lives here rather than in the TOML file for the reason phase 13's
/// autonomy multipliers live in code: a studio that could raise this by editing a file could cook
/// a laptop by editing a file. The TOML may lower it and may not raise it.
pub const THERMAL_REDUCE_C: f32 = 85.0;

/// The temperature above which the governor pauses rather than reduces.
pub const THERMAL_PAUSE_C: f32 = 95.0;

/// The battery share below which heavy stages will not start on battery power.
pub const BATTERY_FLOOR: f32 = 0.30;

// ---------------------------------------------------------------------------
// Which stage
// ---------------------------------------------------------------------------

/// One stage of the wedding pipeline.
///
/// Section 3's DAG, with `scene/story` and `tone/colour` written as the separate phases they are:
/// phases 15 and 16 store different rows, carry different version columns and fail independently,
/// and a stage that ran both would be a stage that could not report which of them was skipped.
///
/// Twenty-five variants and the order is the pipeline's own. It is *not* the execution order -
/// that comes from `StageDecl::depends_on` and the scheduler - but it is the order a panel lists
/// them in and the order [`StageId::ALL`] returns, so a photographer reading the stage list reads
/// the wedding's shape rather than a topological accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageId {
    /// Phase 01. Discover files, hash them, read EXIF, write the journal.
    Ingest,
    /// Phase 02. Embedded previews and 2048 px proxies.
    Previews,
    /// Phase 05. One embedding and five descriptors per photograph.
    Embed,
    /// Phase 06. Detection, clustering, identities, roles.
    Faces,
    /// Phase 07. Scene classification and story segmentation.
    Story,
    /// Phase 08. Moments, bursts and duplicates.
    Moments,
    /// Phase 09. Sharpness, motion, exposure, noise, eyes.
    Integrity,
    /// Phase 10. Expression, interaction, peaks, ranking.
    Emotion,
    /// Phase 11. Horizon, cuts, headroom, balance, aesthetics.
    Composition,
    /// Phase 12. The gallery.
    Cull,
    /// Phase 18. Twenty classes, mattes, allowances.
    Masks,
    /// Phase 15. Illuminant, skin locus, exposure.
    Tone,
    /// Phase 16. Curve, bands, skin guard.
    Colour,
    /// Phase 17. The photographer's own residual.
    Style,
    /// Phase 19. Six local operations against one allowance.
    LocalLight,
    /// Phase 20. Skin, under-eye, evening, blemishes.
    Retouch,
    /// Phase 21. Hair, teeth, eyes, clothing, glare.
    Micro,
    /// Phase 22. Denoise, sharpen, face recovery.
    Restoration,
    /// Phase 23. Lens, straighten, crop, variants.
    Geometry,
    /// Phase 24. Safe generative cleanup.
    Cleanup,
    /// Phase 26. Two bodies, one visual result.
    CameraMatch,
    /// Phase 25. One wedding as one body of work.
    Consistency,
    /// Phase 27. Ten inspections and the bounded re-edit loop.
    Qc,
    /// Phase 29. Albums, heroes, black and white, social crops.
    Curation,
    /// Phase 30. JPEG, TIFF, XMP, and where they went.
    Export,
}

impl StageId {
    /// Every stage, in pipeline order.
    pub const ALL: [Self; 25] = [
        Self::Ingest,
        Self::Previews,
        Self::Embed,
        Self::Faces,
        Self::Story,
        Self::Moments,
        Self::Integrity,
        Self::Emotion,
        Self::Composition,
        Self::Cull,
        Self::Masks,
        Self::Tone,
        Self::Colour,
        Self::Style,
        Self::LocalLight,
        Self::Retouch,
        Self::Micro,
        Self::Restoration,
        Self::Geometry,
        Self::Cleanup,
        Self::CameraMatch,
        Self::Consistency,
        Self::Qc,
        Self::Curation,
        Self::Export,
    ];

    /// How many stages there are.
    pub const COUNT: usize = 25;

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ingest => "ingest",
            Self::Previews => "previews",
            Self::Embed => "embed",
            Self::Faces => "faces",
            Self::Story => "story",
            Self::Moments => "moments",
            Self::Integrity => "integrity",
            Self::Emotion => "emotion",
            Self::Composition => "composition",
            Self::Cull => "cull",
            Self::Masks => "masks",
            Self::Tone => "tone",
            Self::Colour => "colour",
            Self::Style => "style",
            Self::LocalLight => "local_light",
            Self::Retouch => "retouch",
            Self::Micro => "micro",
            Self::Restoration => "restoration",
            Self::Geometry => "geometry",
            Self::Cleanup => "cleanup",
            Self::CameraMatch => "camera_match",
            Self::Consistency => "consistency",
            Self::Qc => "qc",
            Self::Curation => "curation",
            Self::Export => "export",
        }
    }

    /// The words a photographer reads in the stage list.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Ingest => "Importing",
            Self::Previews => "Building previews",
            Self::Embed => "Looking at every photograph",
            Self::Faces => "Finding people",
            Self::Story => "Working out the day",
            Self::Moments => "Grouping what you shot once",
            Self::Integrity => "Checking focus and eyes",
            Self::Emotion => "Reading the moment",
            Self::Composition => "Reading the framing",
            Self::Cull => "Choosing the gallery",
            Self::Masks => "Finding regions",
            Self::Tone => "Judging the light",
            Self::Colour => "Grading",
            Self::Style => "Applying your look",
            Self::LocalLight => "Shaping the light",
            Self::Retouch => "Retouching skin",
            Self::Micro => "Hair, teeth and eyes",
            Self::Restoration => "Cleaning up noise",
            Self::Geometry => "Straightening and cropping",
            Self::Cleanup => "Removing distractions",
            Self::CameraMatch => "Matching your cameras",
            Self::Consistency => "Making it one gallery",
            Self::Qc => "Checking the work",
            Self::Curation => "Building the album",
            Self::Export => "Writing the files",
        }
    }

    /// Parse the stored slug. Unknown text is `None`.
    ///
    /// `None` rather than a default. A stage read as the wrong one would resume a wedding onto
    /// somebody else's checkpoint, and the store refuses the row instead.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == text)
    }

    /// Which of phase 13's six kinds this stage's decisions belong to.
    ///
    /// `None` for the stages that measure rather than decide. Phase 13 wrote the rule - analysis
    /// is not a decision - and this is where it becomes a scheduling fact: a measurement has no
    /// autonomy band, so it runs whenever its dependencies are met and no gate is consulted.
    #[must_use]
    pub const fn decision_kind(self) -> Option<DecisionKind> {
        match self {
            Self::Ingest
            | Self::Previews
            | Self::Embed
            | Self::Faces
            | Self::Story
            | Self::Moments
            | Self::Integrity
            | Self::Emotion
            | Self::Composition
            | Self::Masks => None,
            Self::Cull => Some(DecisionKind::Cull),
            Self::Tone
            | Self::Colour
            | Self::Style
            | Self::LocalLight
            | Self::Restoration
            | Self::Geometry
            | Self::Cleanup
            | Self::CameraMatch
            | Self::Consistency => Some(DecisionKind::Edit),
            Self::Retouch | Self::Micro => Some(DecisionKind::Retouch),
            Self::Qc => Some(DecisionKind::Qc),
            Self::Curation => Some(DecisionKind::Curate),
            Self::Export => Some(DecisionKind::Export),
        }
    }
}

impl fmt::Display for StageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// What a stage is
// ---------------------------------------------------------------------------

/// What a stage's unit of work is.
///
/// Section 5's `StageScope`. The scope decides the denominator every progress number and every
/// checkpoint is counted against, which is why it is a declared property rather than something the
/// runner infers: a stage that reported per-photograph progress while working per-gallery would
/// show a photographer a bar that sat at zero for four minutes and then jumped to done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageScope {
    /// Every photograph in the project.
    AllImages,
    /// Only the frames phase 12 selected.
    SelectedImages,
    /// The wedding as one object; the unit count is one.
    Gallery,
}

impl StageScope {
    /// Every scope.
    pub const ALL: [Self; 3] = [Self::AllImages, Self::SelectedImages, Self::Gallery];

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AllImages => "all_images",
            Self::SelectedImages => "selected_images",
            Self::Gallery => "gallery",
        }
    }

    /// Parse the stored slug.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == text)
    }
}

/// How often a stage commits its progress.
///
/// Section 6.1: "checkpoint granularity is per stage's natural unit". Three kinds because the cost
/// of a checkpoint and the cost of losing one are different for each: an analysis stage's unit is
/// cheap and frequent, a GPU stage's unit is a batch whose partial state is meaningless, and a
/// gallery solver has no half-finished state that could be resumed at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointKind {
    /// One commit per photograph.
    PerImage,
    /// One commit per batch of photographs.
    PerBatch,
    /// One commit when the stage finishes; a kill mid-stage replays the whole stage.
    PerStage,
}

impl CheckpointKind {
    /// Every kind.
    pub const ALL: [Self; 3] = [Self::PerImage, Self::PerBatch, Self::PerStage];

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PerImage => "per_image",
            Self::PerBatch => "per_batch",
            Self::PerStage => "per_stage",
        }
    }

    /// Parse the stored slug.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == text)
    }
}

/// What one stage needs from the machine.
///
/// Section 5's `ResourceNeeds`. The governor reads these to decide what may run beside what; they
/// are declarations rather than reservations, because nothing in this product can actually reserve
/// video memory from another process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceNeeds {
    /// Video memory in megabytes, zero for a stage that never touches the GPU.
    pub vram_mb: u32,
    /// Host memory in megabytes.
    pub ram_mb: u32,
    /// Whether the stage wants a GPU at all.
    pub gpu: bool,
    /// How many CPU threads it will use at full concurrency.
    pub cpu_threads: u16,
}

impl ResourceNeeds {
    /// A CPU-only stage.
    #[must_use]
    pub const fn cpu(ram_mb: u32, cpu_threads: u16) -> Self {
        Self {
            vram_mb: 0,
            ram_mb,
            gpu: false,
            cpu_threads,
        }
    }

    /// A stage that wants a GPU.
    #[must_use]
    pub const fn accelerated(vram_mb: u32, ram_mb: u32, cpu_threads: u16) -> Self {
        Self {
            vram_mb,
            ram_mb,
            gpu: true,
            cpu_threads,
        }
    }
}

/// One stage's declaration. Section 5's `Stage`, renamed.
///
/// `StageDecl` rather than `Stage` because this crate has carried a [`crate::graph::Task`] since
/// phase 01 and a bare `Stage` beside it reads as the thing a task belongs to rather than as its
/// declaration. The rename is recorded in ADR-0057 section 4; nothing else about the shape moved.
///
/// Every field is `const`-constructible so the whole table in [`crate::stages`] is a compile-time
/// array. A stage list built at run time from a config file would be a stage list a studio could
/// reorder, and the DAG's correctness is the one thing in this phase nobody may edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageDecl {
    /// Which stage.
    pub id: StageId,
    /// The internal name, matching [`StageId::as_str`].
    pub name: &'static str,
    /// Stages that must have finished first.
    pub depends_on: &'static [StageId],
    /// What a unit of work is.
    pub scope: StageScope,
    /// How often progress is committed.
    pub checkpoint: CheckpointKind,
    /// Whether a failure here degrades the run instead of failing it.
    pub optional: bool,
    /// The shipped per-item estimate, in milliseconds, used until this machine has measured one.
    pub est_ms_per_item: u32,
    /// What the stage needs from the machine.
    pub resources: ResourceNeeds,
}

// ---------------------------------------------------------------------------
// What happened to a stage
// ---------------------------------------------------------------------------

/// Why a stage did not run.
///
/// A closed set, and every variant names something that was *absent* rather than something that
/// was wrong. Phase 27 established the distinction for an inspection - clean and skipped are
/// different values - and here it decides what a whole run may claim: a wedding whose cleanup
/// stage found no service must not finish as `Completed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipCause {
    /// The photographer switched this stage off in the checklist.
    TurnedOff,
    /// The stage's phase is not built in this release.
    ///
    /// Phases 29 and 30 on this build. Deliberately distinct from [`SkipCause::ServiceAbsent`]:
    /// one says the product does not have the feature and the other says this installation could
    /// not reach it, and they send a photographer to two different places.
    PhaseNotBuilt,
    /// The stage's service could not be constructed on this machine.
    ServiceAbsent,
    /// The stage's model is untrained, so running it would produce a number nobody may use.
    ModelUntrained,
    /// An upstream stage produced nothing for this stage to work on.
    NoInput,
    /// The autonomy gate did not permit unattended action.
    AwaitingReview,
    /// The governor stopped the run before this stage started.
    ResourceStopped,
    /// The photographer cancelled before this stage started.
    Cancelled,
}

impl SkipCause {
    /// Every cause.
    pub const ALL: [Self; 8] = [
        Self::TurnedOff,
        Self::PhaseNotBuilt,
        Self::ServiceAbsent,
        Self::ModelUntrained,
        Self::NoInput,
        Self::AwaitingReview,
        Self::ResourceStopped,
        Self::Cancelled,
    ];

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TurnedOff => "turned_off",
            Self::PhaseNotBuilt => "phase_not_built",
            Self::ServiceAbsent => "service_absent",
            Self::ModelUntrained => "model_untrained",
            Self::NoInput => "no_input",
            Self::AwaitingReview => "awaiting_review",
            Self::ResourceStopped => "resource_stopped",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parse the stored slug.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == text)
    }

    /// The sentence a photographer reads beside the stage.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::TurnedOff => "You turned this off",
            Self::PhaseNotBuilt => "This release does not include this step yet",
            Self::ServiceAbsent => "AURA could not start this step on this machine",
            Self::ModelUntrained => {
                "This step has no trained model in this release, so it did nothing rather \
                 than guess"
            }
            Self::NoInput => "There was nothing for this step to work on",
            Self::AwaitingReview => {
                "This step is waiting for you, because AURA is not confident enough to do it \
                 on its own"
            }
            Self::ResourceStopped => "The run stopped before this step started",
            Self::Cancelled => "You stopped the run before this step started",
        }
    }

    /// Whether a run that skipped for this reason is still a complete run.
    ///
    /// Only [`SkipCause::TurnedOff`] is, and that is the whole of the list on purpose: a stage the
    /// photographer switched off is a stage nobody expected to run, and every other cause is
    /// something the product could not do. A build that added a second `true` here would be a
    /// build that could report a wedding as `Completed` while three of its stages had never
    /// started.
    #[must_use]
    pub const fn is_expected(self) -> bool {
        matches!(self, Self::TurnedOff)
    }
}

/// What one stage did.
///
/// There is no variant carrying a result. A stage's *output* is rows in the catalog written by the
/// phase that owns it; what comes back to the orchestrator is how many units finished and whether
/// it may go on. Anything richer would be the scheduler holding a copy of another phase's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageOutcome {
    /// Every unit finished.
    Completed {
        /// How many units the stage processed.
        items: u32,
    },
    /// The stage ran and some units could not be processed.
    ///
    /// `Partial` is not a failure and not a success: the run continues and the summary says so.
    Partial {
        /// Units that finished.
        items: u32,
        /// Units that did not.
        failed: u32,
        /// Why, in the words of the error the units raised.
        detail: String,
    },
    /// The stage did not run.
    Skipped(SkipCause),
    /// The stage ran and could not finish.
    Failed {
        /// The AURA error code the stage raised.
        code: String,
        /// The detail behind it.
        detail: String,
    },
}

impl StageOutcome {
    /// The units this outcome finished.
    #[must_use]
    pub const fn items(&self) -> u32 {
        match self {
            Self::Completed { items } | Self::Partial { items, .. } => *items,
            Self::Skipped(_) | Self::Failed { .. } => 0,
        }
    }

    /// Whether the run may treat this stage as satisfied for dependency purposes.
    ///
    /// A skipped stage satisfies its dependents. That is a decision rather than an oversight and
    /// it is the one that makes degraded completion possible: a wedding whose cleanup was switched
    /// off must still reach QC. What stops that becoming a silent lie is that the skip is
    /// recorded, named, counted in [`RunSummary::degraded_stages`] and shown, so a dependent stage
    /// that then produces nothing has a visible reason upstream of it.
    #[must_use]
    pub const fn unblocks_dependents(&self) -> bool {
        !matches!(self, Self::Failed { .. })
    }

    /// Whether this outcome is a complete, undegraded success.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        match self {
            Self::Completed { .. } => true,
            Self::Skipped(cause) => cause.is_expected(),
            Self::Partial { .. } | Self::Failed { .. } => false,
        }
    }

    /// The stable slug for the stored row.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Completed { .. } => "completed",
            Self::Partial { .. } => "partial",
            Self::Skipped(_) => "skipped",
            Self::Failed { .. } => "failed",
        }
    }
}

// ---------------------------------------------------------------------------
// May the product act
// ---------------------------------------------------------------------------

/// What the autonomy gate said about one stage.
///
/// Three values rather than a boolean, because "act" and "act and queue for review" are different
/// products: the first is what Zero-Touch promises and the second is what an honest build with an
/// uncalibrated confidence can actually offer. Phase 13's [`Autonomy::Suggest`] is exactly the
/// second, and this build lands most editing stages there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageVerdict {
    /// Run it, and nothing goes in the review queue because of it.
    Act,
    /// Run it, and the decisions it makes are queued for a person to look at.
    ActAndReview,
    /// Do not run it unattended. The stage is skipped as [`SkipCause::AwaitingReview`].
    Hold,
}

impl StageVerdict {
    /// Whether the stage runs at all.
    #[must_use]
    pub const fn runs(self) -> bool {
        matches!(self, Self::Act | Self::ActAndReview)
    }

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Act => "act",
            Self::ActAndReview => "act_and_review",
            Self::Hold => "hold",
        }
    }

    /// The verdict phase 13's band implies in this mode.
    ///
    /// The only way a [`StageVerdict`] is ever built from a band, and the whole of the mapping.
    ///
    /// `Auto` and `AutoZeroTouch` are phase 13's own answer and are read literally through
    /// [`Autonomy::acts`]. `Suggest` is the interesting one: phase 13's word for it is "applied,
    /// and put in the review queue so somebody looks", and `acts` returns false for it because a
    /// *silent* application is what it forbids. In an attended session that means the product
    /// waits. In Zero-Touch it means the product goes ahead **and tells somebody**, which is
    /// exactly what `Suggest`'s own `user_text` describes - and it is the band an uncalibrated
    /// build lands almost every editing stage in, so holding it would ship a Zero-Touch button
    /// that does nothing at all. ADR-0057 section 6 has the argument.
    ///
    /// `RequireReview` holds in every mode. There is no switch anywhere in this phase that
    /// changes that.
    #[must_use]
    pub const fn from_band(band: Autonomy, zero_touch: bool) -> Self {
        match band {
            Autonomy::Auto => Self::Act,
            Autonomy::AutoZeroTouch => {
                if zero_touch {
                    Self::Act
                } else {
                    Self::Hold
                }
            }
            Autonomy::Suggest => {
                if zero_touch {
                    Self::ActAndReview
                } else {
                    Self::Hold
                }
            }
            Autonomy::RequireReview => Self::Hold,
        }
    }
}

/// Where a band comes from.
///
/// A port rather than a dependency. `aura-jobs` cannot depend on `aura-explain` without becoming a
/// crate that knows how confidence is calibrated, and a scheduler that knew that would be a
/// scheduler one edit away from computing its own. `aura-app` implements this over phase 13's
/// `AutonomyPolicy`, which is the same indirection phase 27 used for its readings.
pub trait AutonomyGate: Send + Sync + fmt::Debug {
    /// The band this project's decisions of `kind` currently sit in.
    ///
    /// # Errors
    ///
    /// Whatever the policy raises when its table cannot be read.
    fn band(&self, project: ProjectId, kind: DecisionKind) -> AuraResult<Autonomy>;

    /// Whether this build's confidences have been calibrated.
    ///
    /// Phase 13's `calibration_ver = 0` makes this false, and it is the single most consequential
    /// number in this phase: while it is false every band is raised one step and almost nothing
    /// acts quietly. The run summary says so in the photographer's own words rather than leaving
    /// them to wonder why Zero-Touch queued four hundred frames.
    fn calibrated(&self, project: ProjectId) -> bool;
}

// ---------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------

/// What a run is doing right now. Section 5's `RunProgress`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunProgress {
    /// The stage currently executing.
    pub stage: StageId,
    /// Its position in the enabled stage list, from zero.
    pub stage_index: u32,
    /// How many stages are enabled in this run.
    pub stage_total: u32,
    /// Units finished in this stage.
    pub items_done: u32,
    /// Units this stage has to do.
    pub items_total: u32,
    /// Seconds remaining for the whole run.
    pub eta_s: u32,
    /// Units per second, measured over this stage.
    pub throughput_per_s: f32,
    /// What the run has spent on cloud calls, in US dollars.
    ///
    /// Section 7 of the phase document: this phase makes no cloud call of its own. The meter is
    /// here because the *stages* can - phase 24's judgement, phase 27's planner - and a run that
    /// spent a photographer's money without a meter would be a run they found out about on a bill.
    pub spend_usd: f32,
    /// Anything worth saying that is not a failure.
    pub warnings: Vec<String>,
    /// The photograph being worked on, for the thumbnail.
    pub current_image: Option<ImageId>,
}

impl RunProgress {
    /// A run that has not started.
    #[must_use]
    pub const fn starting(stage: StageId, stage_total: u32) -> Self {
        Self {
            stage,
            stage_index: 0,
            stage_total,
            items_done: 0,
            items_total: 0,
            eta_s: 0,
            throughput_per_s: 0.0,
            spend_usd: 0.0,
            warnings: Vec::new(),
            current_image: None,
        }
    }

    /// The share of this stage that is done, in the range 0.0 to 1.0.
    #[must_use]
    pub fn stage_fraction(&self) -> f32 {
        if self.items_total == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            (self.items_done as f32 / self.items_total as f32).clamp(0.0, 1.0)
        }
    }
}

/// A cloneable read side of the run's progress.
///
/// Section 5 writes `watch::Receiver<RunProgress>`. This product's pipeline is synchronous - rayon
/// and `parking_lot`, with `tokio` reaching only as far as the Tauri boundary - so a `tokio::sync`
/// receiver in the orchestrator's frozen signature would put an async runtime inside the one crate
/// that must be drivable from a plain test. `RunWatch` has the semantics the signature needed:
/// cheap to clone, always readable, always the newest value, and a version counter so a poller can
/// tell a change from a repeat. ADR-0057 section 4 records the substitution.
#[derive(Debug, Clone)]
pub struct RunWatch {
    inner: Arc<RwLock<RunProgress>>,
    version: Arc<AtomicU64>,
}

impl RunWatch {
    /// A watch holding an initial value.
    #[must_use]
    pub fn new(initial: RunProgress) -> Self {
        Self {
            inner: Arc::new(RwLock::new(initial)),
            version: Arc::new(AtomicU64::new(0)),
        }
    }

    /// The newest value.
    #[must_use]
    pub fn borrow(&self) -> RunProgress {
        self.inner.read().clone()
    }

    /// How many times the value has been replaced.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// Replace the value and bump the version.
    pub fn publish(&self, progress: RunProgress) {
        {
            let mut guard = self.inner.write();
            *guard = progress;
        }
        self.version.fetch_add(1, Ordering::Release);
    }

    /// Read, modify and publish in one lock acquisition.
    pub fn update(&self, f: impl FnOnce(&mut RunProgress)) {
        {
            let mut guard = self.inner.write();
            f(&mut guard);
        }
        self.version.fetch_add(1, Ordering::Release);
    }
}

/// The handle a caller holds while a run is in flight. Section 5's `RunHandle`.
#[derive(Debug, Clone)]
pub struct RunHandle {
    /// The run this handle belongs to.
    pub run_id: RunId,
    /// The progress stream.
    pub progress: RunWatch,
    /// The cooperative cancel flag, polled between units.
    pub cancel: CancelToken,
}

/// How a run ended. Section 5's `RunStatus`, with `Running` added for the row that is still open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Still going. Not one of section 5's four; a run is a row before it is a result, and a
    /// resume has to be able to tell an unfinished run from a failed one.
    Running,
    /// Every enabled stage finished and nothing was skipped for a reason nobody chose.
    Completed,
    /// The run finished and at least one stage did not do what it was meant to.
    CompletedDegraded,
    /// The photographer stopped it.
    Cancelled,
    /// A mandatory stage could not finish.
    Failed,
}

impl RunStatus {
    /// Every status.
    pub const ALL: [Self; 5] = [
        Self::Running,
        Self::Completed,
        Self::CompletedDegraded,
        Self::Cancelled,
        Self::Failed,
    ];

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::CompletedDegraded => "completed_degraded",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    /// Parse the stored slug. Unknown text is `None`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == text)
    }

    /// Whether the run is over.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }

    /// Whether pressing the button again continues this run rather than starting a new one.
    ///
    /// A stopped wedding and a delivered wedding are different things, and this is where the
    /// product says so. `Cancelled` and `Failed` both mean "the work is part done and every
    /// finished stage is committed", so continuing is what a photographer means by pressing start:
    /// section 6.3 asks a failed mandatory stage to leave "a resumable checkpoint - never a
    /// half-written gallery", and a cancel is the same state reached deliberately.
    ///
    /// `Completed` and `CompletedDegraded` are finished. Pressing start on one of those mints a
    /// **new** run, which correctly redoes the wedding from the beginning - because "run it again"
    /// on a delivered gallery means run it again.
    ///
    /// The distinction matters more than it looks. Checkpoints are keyed `(run_id, stage)`, so a
    /// resume that minted a new id would find no checkpoints and repeat every finished stage -
    /// which on a two-hour wedding is the whole failure this phase exists to prevent, arrived at
    /// through a bookkeeping decision rather than through a bug.
    #[must_use]
    pub const fn is_resumable(self) -> bool {
        matches!(self, Self::Running | Self::Cancelled | Self::Failed)
    }

    /// Whether the run's record is closed to further writes.
    ///
    /// The database enforces this with `autopilot_run_no_reopen`. A delivered wedding's record is
    /// what a photographer was told happened, and a correction to it is a new run.
    #[must_use]
    pub const fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::CompletedDegraded)
    }

    /// The words a photographer reads.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Running => "Working",
            Self::Completed => "Finished",
            Self::CompletedDegraded => "Finished, with some steps skipped",
            Self::Cancelled => "Stopped",
            Self::Failed => "Could not finish",
        }
    }
}

/// What a finished run did. Section 5's `RunSummary`.
#[derive(Debug, Clone, PartialEq)]
pub struct RunSummary {
    /// The run.
    pub run_id: RunId,
    /// How it ended.
    pub status: RunStatus,
    /// How many photographs phase 12 selected.
    pub selected: u32,
    /// How many files were written.
    pub exported: u32,
    /// How many frames a person is being asked to look at.
    pub needs_review: u32,
    /// Phase 27's report, or `None` when the QC stage did not run.
    ///
    /// Section 5 writes `qc: QcReport`. It is an `Option` here for the reason phase 27 made
    /// `Outcome::Skipped` a variant: a run whose QC stage was switched off must not carry an empty
    /// report that reads as a clean bill of health. ADR-0057 section 4.
    pub qc: Option<QcReport>,
    /// How long each stage took, in milliseconds, in execution order.
    pub stage_timings: Vec<(StageId, u64)>,
    /// What the run spent on cloud calls.
    pub spend_usd: f32,
    /// Where the delivered files are.
    pub output_path: PathBuf,
    /// Every stage that did not do what it was meant to, with the reason.
    pub degraded_stages: Vec<(StageId, String)>,
}

impl RunSummary {
    /// Total wall clock across every stage, in milliseconds.
    #[must_use]
    pub fn total_ms(&self) -> u64 {
        self.stage_timings.iter().map(|(_, ms)| *ms).sum()
    }
}

// ---------------------------------------------------------------------------
// Resumption
// ---------------------------------------------------------------------------

/// What a stage had finished when the process last stopped.
///
/// One row per stage per run. The `inputs_hash` is the whole of section 6.1's invalidation rule:
/// a resumed stage whose declared inputs hash to a different value has had something change
/// underneath it, and only that stage re-runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    /// The run.
    pub run_id: RunId,
    /// The stage.
    pub stage: StageId,
    /// Units finished.
    pub items_done: u32,
    /// Units the stage had to do when it started.
    pub items_total: u32,
    /// A digest of what the stage read: upstream stage versions, the policy version, the unit
    /// count and the stage's own version.
    pub inputs_hash: String,
    /// How many times the stage has been attempted in this run.
    pub attempts: u8,
    /// The outcome slug, or `None` while the stage is still going.
    pub outcome: Option<String>,
    /// Milliseconds of wall clock spent in the stage so far.
    pub elapsed_ms: u64,
}

/// Why a resumed stage has to start again from nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Invalidation {
    /// Nothing changed; the stage continues from `items_done`.
    None,
    /// The stage's declared inputs hash differently than when it last ran.
    InputsMoved,
    /// The checkpoint names a stage this build does not have.
    UnknownStage,
    /// The checkpoint's unit count does not match the project's.
    ScopeChanged,
}

impl Invalidation {
    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::InputsMoved => "inputs_moved",
            Self::UnknownStage => "unknown_stage",
            Self::ScopeChanged => "scope_changed",
        }
    }

    /// Whether the stage has to start from zero.
    #[must_use]
    pub const fn restarts(self) -> bool {
        !matches!(self, Self::None)
    }
}

// ---------------------------------------------------------------------------
// The ETA
// ---------------------------------------------------------------------------

/// How long is left, and whether the number came from this machine.
///
/// Section 6.4. Two sources rather than one, and [`Eta::measured`] says which is in use, because a
/// declared estimate and a measured one are different claims: the first is what a stage shipped
/// with on a reference machine and the second is what this laptop is actually doing. A panel that
/// showed them identically would be a panel that promised two hours on a machine doing four.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Eta {
    /// Milliseconds remaining across every unenabled-but-pending stage and the current one.
    pub remaining_ms: u64,
    /// Units per second measured over the current stage; zero before the first unit.
    pub throughput_per_s: f32,
    /// True once the current stage has measured its own throughput past the warm-up share.
    pub measured: bool,
}

impl Eta {
    /// The seconds a panel shows.
    #[must_use]
    pub const fn remaining_s(&self) -> u32 {
        #[allow(clippy::cast_possible_truncation)]
        {
            (self.remaining_ms / 1_000) as u32
        }
    }
}

// ---------------------------------------------------------------------------
// The governor
// ---------------------------------------------------------------------------

/// What kind of pressure the machine is under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    /// The GPU is out of, or close to out of, memory.
    Vram,
    /// Host memory is close to exhausted.
    Ram,
    /// The machine is hot.
    Thermal,
    /// The machine is on battery.
    Battery,
    /// The disk is close to full.
    Disk,
    /// The photographer is working in another application.
    Quiet,
    /// The GPU stopped answering.
    DeviceLost,
}

impl ResourceKind {
    /// Every kind.
    pub const ALL: [Self; 7] = [
        Self::Vram,
        Self::Ram,
        Self::Thermal,
        Self::Battery,
        Self::Disk,
        Self::Quiet,
        Self::DeviceLost,
    ];

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vram => "vram",
            Self::Ram => "ram",
            Self::Thermal => "thermal",
            Self::Battery => "battery",
            Self::Disk => "disk",
            Self::Quiet => "quiet",
            Self::DeviceLost => "device_lost",
        }
    }

    /// Parse the stored slug.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == text)
    }
}

/// What the governor decided to do about it.
///
/// **There is no variant that makes the product do more.** An unreadable sensor, a hot machine, a
/// full disk and a laptop on battery therefore all reach the same conservative state, and a
/// governor that is wrong slows a run down rather than crashing a machine. Phase 24's cloud
/// judgement has the same property for the same reason: a component whose failure modes all point
/// one way has no unsafe failure mode.
///
/// The ordering is the strength of the response and other code depends on it: `Proceed < Reduce <
/// Pause < Stop`, so combining two readings is `max`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum GovernorAction {
    /// Carry on at the current concurrency.
    #[default]
    Proceed,
    /// Halve the batch size and the thread count, to a floor of one.
    Reduce,
    /// Stop starting units until the pressure clears.
    Pause,
    /// End the run cleanly at the next checkpoint.
    Stop,
}

impl GovernorAction {
    /// Every action, from weakest to strongest.
    pub const ALL: [Self; 4] = [Self::Proceed, Self::Reduce, Self::Pause, Self::Stop];

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proceed => "proceed",
            Self::Reduce => "reduce",
            Self::Pause => "pause",
            Self::Stop => "stop",
        }
    }

    /// Parse the stored slug. Unknown text is [`GovernorAction::Pause`].
    ///
    /// The cautious direction, and not `Proceed`. An unreadable action that defaulted to carrying
    /// on would be a build that resumed a throttled run at full speed because it could not read
    /// its own row.
    #[must_use]
    pub fn from_str_or_pause(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|a| a.as_str() == text)
            .unwrap_or(Self::Pause)
    }

    /// The sentence a photographer reads. Section 6.2: "reducing speed to protect your machine"
    /// rather than silently slowing.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::Proceed => "Running at full speed",
            Self::Reduce => "Reducing speed to protect your machine",
            Self::Pause => "Paused until your machine is ready again",
            Self::Stop => "Stopped, so nothing is lost",
        }
    }
}

/// One thing the governor noticed and what it did about it.
///
/// Section 11's `autopilot.resource_event` telemetry, as a stored row rather than a log line: a
/// photographer whose two-hour run took four hours is owed the list of times the machine asked the
/// product to slow down.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceEvent {
    /// What kind of pressure.
    pub kind: ResourceKind,
    /// What the governor did.
    pub action: GovernorAction,
    /// The reading behind it, in the kind's own units.
    pub reading: f32,
    /// The threshold it was compared against.
    pub threshold: f32,
    /// The stage that was running.
    pub stage: StageId,
}

/// What the machine looks like right now.
///
/// Every field is an `Option` because every one of them is unreadable on some machine this product
/// runs on, and phase 24's rule applies: an absent reading is ignorance rather than permission. The
/// governor treats `None` as "do not know", which contributes no pressure - the sensor cannot be
/// used to justify going faster either, because nothing can.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MachineState {
    /// Video memory in use, as a share of the total.
    pub vram_used: Option<f32>,
    /// Host memory in use, as a share of the total.
    pub ram_used: Option<f32>,
    /// The hottest sensor, in degrees Celsius.
    pub temperature_c: Option<f32>,
    /// Battery charge as a share, and `None` on a desktop.
    pub battery: Option<f32>,
    /// True when the machine is running on its battery rather than mains.
    pub on_battery: bool,
    /// Free bytes on the volume the project lives on.
    pub disk_free_bytes: Option<u64>,
    /// Bytes the run still expects to write.
    pub disk_needed_bytes: Option<u64>,
    /// True when the photographer is working in another application.
    pub foreground_busy: bool,
    /// True when the GPU stopped answering.
    pub device_lost: bool,
}

// ---------------------------------------------------------------------------
// Pre-flight
// ---------------------------------------------------------------------------

/// One thing checked before a two-hour run starts. Section 6 of the phase document, step 5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreflightCheck {
    /// The project exists, its catalog opens and its schema is current.
    ProjectIntegrity,
    /// There are photographs to work on.
    HasImages,
    /// Enough free disk for the estimated output plus [`DISK_HEADROOM`].
    DiskSpace,
    /// A hardware plan could be made.
    Hardware,
    /// Every model the enabled stages need is installed and verified.
    Models,
    /// The cloud budget, when a stage would use one.
    CloudBudget,
    /// Whether this build's confidences are calibrated.
    Calibration,
    /// Whether the machine is on battery.
    Power,
}

impl PreflightCheck {
    /// Every check, in the order a photographer reads them.
    pub const ALL: [Self; 8] = [
        Self::ProjectIntegrity,
        Self::HasImages,
        Self::DiskSpace,
        Self::Hardware,
        Self::Models,
        Self::CloudBudget,
        Self::Calibration,
        Self::Power,
    ];

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProjectIntegrity => "project_integrity",
            Self::HasImages => "has_images",
            Self::DiskSpace => "disk_space",
            Self::Hardware => "hardware",
            Self::Models => "models",
            Self::CloudBudget => "cloud_budget",
            Self::Calibration => "calibration",
            Self::Power => "power",
        }
    }

    /// Parse the stored slug.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == text)
    }

    /// The words a photographer reads.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::ProjectIntegrity => "The wedding opens",
            Self::HasImages => "There are photographs",
            Self::DiskSpace => "There is room on the disk",
            Self::Hardware => "AURA can use this machine",
            Self::Models => "The models are installed",
            Self::CloudBudget => "The AI budget",
            Self::Calibration => "How much AURA may do on its own",
            Self::Power => "Power",
        }
    }
}

/// How one pre-flight check came out.
///
/// The ordering is the severity and other code depends on it: `Pass < Warn < Block`, so the
/// report's verdict is `max` over its rows.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum PreflightVerdict {
    /// Nothing to say.
    #[default]
    Pass,
    /// Worth knowing before starting, and does not stop the run.
    Warn,
    /// The run will not start.
    Block,
}

impl PreflightVerdict {
    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Block => "block",
        }
    }

    /// Whether a run may start with this verdict.
    #[must_use]
    pub const fn permits_start(self) -> bool {
        !matches!(self, Self::Block)
    }
}

/// One row of the pre-flight report.
///
/// `detail` is the actionable message section 2.1 asks for: "fail fast with actionable messages
/// before starting a two-hour job". A row that said only "disk space: block" would send a
/// photographer to a runbook to find out how many gigabytes they need.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreflightRow {
    /// Which check.
    pub check: PreflightCheck,
    /// How it came out.
    pub verdict: PreflightVerdict,
    /// What to do about it, in the photographer's own words.
    pub detail: String,
}

/// Everything the pre-flight found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreflightReport {
    /// Every row, in [`PreflightCheck::ALL`] order.
    pub rows: Vec<PreflightRow>,
    /// How many photographs the run would work on.
    pub images: u32,
    /// The bytes the run expects to write.
    pub estimated_output_bytes: u64,
    /// The whole run's estimate in milliseconds, from the declared per-item estimates.
    pub estimated_ms: u64,
}

impl PreflightReport {
    /// The strongest verdict in the report.
    #[must_use]
    pub fn verdict(&self) -> PreflightVerdict {
        self.rows
            .iter()
            .map(|row| row.verdict)
            .max()
            .unwrap_or_default()
    }

    /// Whether a run may start.
    #[must_use]
    pub fn permits_start(&self) -> bool {
        self.verdict().permits_start()
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why the autopilot did what it did.
///
/// Invariant 2 requires every AI decision to carry reasons. The autopilot makes no decision about
/// a photograph, so these are reasons about the **run**: which stages ran, which did not, and what
/// the machine had to say about it. They are the sentences the run summary is built from, and they
/// are codes rather than sentences for phase 09's reason: a stored sentence is copy a release can
/// change and a catalog nobody can translate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutopilotCode {
    /// The run finished with every enabled stage complete.
    RunComplete,
    /// The run finished and something was skipped.
    RunDegraded,
    /// The photographer stopped it.
    RunCancelled,
    /// A mandatory stage could not finish.
    RunFailed,
    /// A stage was resumed from a checkpoint.
    StageResumed,
    /// A stage's checkpoint was invalidated and it started again.
    StageReplanned,
    /// A stage retried after a failure.
    StageRetried,
    /// An optional stage failed and the run carried on.
    StageIsolated,
    /// A stage was switched off in the checklist.
    StageDisabled,
    /// A stage's phase is not built in this release.
    StageUnbuilt,
    /// A stage's service could not be built.
    StageUnavailable,
    /// A stage's model is untrained.
    StageUntrained,
    /// A stage was held because the autonomy gate did not permit unattended action.
    StageHeld,
    /// The build is uncalibrated, so every band was raised one step.
    UncalibratedHold,
    /// The governor reduced concurrency.
    ResourceReduced,
    /// The governor paused.
    ResourcePaused,
    /// The governor stopped the run.
    ResourceStopped,
    /// The GPU stopped answering and the run continued on the CPU.
    DeviceLostFallback,
    /// The pre-flight refused to start.
    PreflightBlocked,
    /// The pre-flight had something to say and the run started anyway.
    PreflightWarned,
}

impl AutopilotCode {
    /// Every code.
    pub const ALL: [Self; 20] = [
        Self::RunComplete,
        Self::RunDegraded,
        Self::RunCancelled,
        Self::RunFailed,
        Self::StageResumed,
        Self::StageReplanned,
        Self::StageRetried,
        Self::StageIsolated,
        Self::StageDisabled,
        Self::StageUnbuilt,
        Self::StageUnavailable,
        Self::StageUntrained,
        Self::StageHeld,
        Self::UncalibratedHold,
        Self::ResourceReduced,
        Self::ResourcePaused,
        Self::ResourceStopped,
        Self::DeviceLostFallback,
        Self::PreflightBlocked,
        Self::PreflightWarned,
    ];

    /// How many codes there are.
    pub const COUNT: usize = 20;

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunComplete => "run_complete",
            Self::RunDegraded => "run_degraded",
            Self::RunCancelled => "run_cancelled",
            Self::RunFailed => "run_failed",
            Self::StageResumed => "stage_resumed",
            Self::StageReplanned => "stage_replanned",
            Self::StageRetried => "stage_retried",
            Self::StageIsolated => "stage_isolated",
            Self::StageDisabled => "stage_disabled",
            Self::StageUnbuilt => "stage_unbuilt",
            Self::StageUnavailable => "stage_unavailable",
            Self::StageUntrained => "stage_untrained",
            Self::StageHeld => "stage_held",
            Self::UncalibratedHold => "uncalibrated_hold",
            Self::ResourceReduced => "resource_reduced",
            Self::ResourcePaused => "resource_paused",
            Self::ResourceStopped => "resource_stopped",
            Self::DeviceLostFallback => "device_lost_fallback",
            Self::PreflightBlocked => "preflight_blocked",
            Self::PreflightWarned => "preflight_warned",
        }
    }

    /// Parse the stored slug.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == text)
    }

    /// The sentence a photographer reads.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::RunComplete => "Everything you asked for finished",
            Self::RunDegraded => "Some steps did not run, and they are listed below",
            Self::RunCancelled => "You stopped this run. Everything finished so far is saved",
            Self::RunFailed => "A step AURA could not skip did not finish",
            Self::StageResumed => "Picked up where it left off",
            Self::StageReplanned => "Something this step depends on changed, so it ran again",
            Self::StageRetried => "Failed once and was tried again",
            Self::StageIsolated => "This step failed and the rest of the wedding carried on",
            Self::StageDisabled => "You turned this off",
            Self::StageUnbuilt => "This release does not include this step yet",
            Self::StageUnavailable => "AURA could not start this step on this machine",
            Self::StageUntrained => "This step has no trained model in this release",
            Self::StageHeld => "Waiting for you, because AURA is not confident enough",
            Self::UncalibratedHold => {
                "AURA has not yet learned how often it is right, so it is being careful and \
                 asking about more than it normally would"
            }
            Self::ResourceReduced => "Reduced speed to protect your machine",
            Self::ResourcePaused => "Paused while your machine was busy",
            Self::ResourceStopped => "Stopped because your machine ran out of something",
            Self::DeviceLostFallback => {
                "Your graphics card stopped responding, so AURA used the processor"
            }
            Self::PreflightBlocked => "AURA did not start, and the reason is above",
            Self::PreflightWarned => "AURA started, and there is something you should know",
        }
    }
}

/// One reason, with the stage it is about.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutopilotReason {
    /// Which code.
    pub code: AutopilotCode,
    /// The stage it is about, or `None` for a reason about the run.
    pub stage: Option<StageId>,
    /// A short factual detail, never a sentence a panel renders on its own.
    pub detail: String,
}

// ---------------------------------------------------------------------------
// The port a stage runs through
// ---------------------------------------------------------------------------

/// What one stage is being asked to do.
#[derive(Debug, Clone, PartialEq)]
pub struct StageRequest {
    /// The project.
    pub project: ProjectId,
    /// The run.
    pub run_id: RunId,
    /// Which stage.
    pub stage: StageId,
    /// How many units the stage had already finished when the run last stopped.
    pub resume_from: u32,
    /// Whether the run is unattended.
    pub zero_touch: bool,
    /// What the gate said.
    pub verdict: StageVerdict,
    /// The concurrency the governor currently permits, one or more.
    pub concurrency: u16,
}

/// How one stage is actually executed.
///
/// The port. `aura-jobs` depends on `aura-core` and `aura-catalog` and on none of the twenty-two
/// deciding crates, exactly as `aura-qc` depends on none of the thirteen it judges - and for a
/// stronger reason: a scheduler that depended on every phase would be a crate that had to be
/// rebuilt whenever any phase changed, and a crate every phase could reach back into.
///
/// `aura-app` implements this over `AppState`, which already owns every pass.
pub trait StageRunner: Send + Sync + fmt::Debug {
    /// How many units this stage has to do, before it starts.
    ///
    /// Called once per stage per run. Zero means [`SkipCause::NoInput`] rather than a stage that
    /// finished instantly, which is the distinction phase 27 made for an inspection and this phase
    /// makes for a whole stage.
    ///
    /// # Errors
    ///
    /// Whatever the underlying service raises when it cannot count.
    fn unit_count(&self, project: ProjectId, stage: StageId) -> AuraResult<u32>;

    /// Whether this build can run this stage at all.
    ///
    /// Returns the cause when it cannot. Called before `unit_count`, so a stage whose phase is not
    /// built never reaches a counter that would have to invent an answer.
    fn availability(&self, project: ProjectId, stage: StageId) -> Option<SkipCause>;

    /// Run the stage.
    ///
    /// The implementation must poll `cancel` between units and must never observe it inside a
    /// write. `progress` is advanced by the implementation as units finish, which is what lets a
    /// per-image stage show a moving bar without the orchestrator polling the catalog.
    ///
    /// # Errors
    ///
    /// Whatever the underlying pass raises. The orchestrator turns an error into
    /// [`StageOutcome::Failed`] and decides from [`StageDecl::optional`] what that means for the
    /// run.
    fn run(
        &self,
        request: &StageRequest,
        progress: &RunWatch,
        cancel: &CancelToken,
    ) -> AuraResult<StageOutcome>;

    /// A digest of what this stage reads, for [`Checkpoint::inputs_hash`].
    ///
    /// # Errors
    ///
    /// Whatever the underlying service raises when it cannot read its own version columns.
    fn inputs_hash(&self, project: ProjectId, stage: StageId) -> AuraResult<String>;
}

/// Where the machine's readings come from.
///
/// A port for the same reason [`AutonomyGate`] is one: reading a GPU's memory, a battery and a
/// thermal sensor is platform work, and a scheduler that did it would be a scheduler that could
/// not be tested without the hardware it is meant to protect. `aura-app` implements this over
/// phase 03's hardware plan; the gate implements it over an authored fixture.
pub trait MachineProbe: Send + Sync + fmt::Debug {
    /// What the machine looks like right now.
    fn sample(&self) -> MachineState;
}

// ---------------------------------------------------------------------------
// What a project's autopilot looks like
// ---------------------------------------------------------------------------

/// The project header the Autopilot panel shows.
///
/// Coverage here is **stages** rather than photographs, which is the first time in the product an
/// outline has counted something other than frames. Phase 08's rule is the one being followed: say
/// what the denominator is. A run that completed 22 of 25 stages and a run that completed 22 of 22
/// enabled stages are different runs, and both numbers are on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutopilotOutline {
    /// Runs this project has had.
    pub runs: u32,
    /// The newest run's id, when there is one.
    pub latest_run: Option<String>,
    /// The newest run's status.
    pub status: Option<RunStatus>,
    /// Stages enabled in the newest run.
    pub stages_enabled: u32,
    /// Stages that finished cleanly.
    pub stages_completed: u32,
    /// Stages that were skipped for a reason nobody chose.
    pub stages_degraded: u32,
    /// Whether the newest run was unattended.
    pub zero_touch: bool,
    /// Whether this build's confidences are calibrated.
    pub calibrated: bool,
    /// Governor events across the newest run.
    pub resource_events: u32,
    /// Bytes migration 28 holds for this project.
    pub bytes: u64,
    /// The policy file's version.
    pub policy_ver: i64,
    /// The orchestrator's own version, bumped when the DAG or its semantics change.
    pub orchestrator_ver: i64,
}

impl AutopilotOutline {
    /// The share of enabled stages that finished cleanly, or zero when nothing has run.
    #[must_use]
    pub fn completeness(&self) -> f32 {
        if self.stages_enabled == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            (self.stages_completed as f32 / self.stages_enabled as f32).clamp(0.0, 1.0)
        }
    }
}

/// A photographer's own change to the checklist.
///
/// The whole of what a person may write here is which stages run and whether the run is
/// unattended. There is no strength, no threshold and no autonomy field: a surface that let
/// somebody raise a band would be a surface that routed around phase 13, and phase 21's rule
/// applies - a ceiling can be lowered by a studio and raised by nobody.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutopilotOverride {
    /// The project.
    pub project: ProjectId,
    /// Stages the photographer switched off.
    pub disabled: Vec<StageId>,
    /// Whether the run may act unattended where the bands allow.
    pub zero_touch: bool,
    /// Whether heavy stages may run on battery power.
    pub allow_on_battery: bool,
    /// Whether the run yields to foreground work.
    pub quiet_mode: bool,
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask what the product did to a whole wedding, and the one way to start it.
///
/// Twenty-fourth service of its kind and the first whose subject is **a run**. Phase 29 adds a
/// curation stage to this DAG, phase 30 adds an export stage and reads these summaries as its
/// learning signal. No phase may keep its own pipeline runner, its own checkpoint format or its
/// own idea of what a finished wedding is - two answers to "did this wedding finish" is a studio
/// that delivers a gallery the product thinks it never made.
pub trait AutopilotService: Send + Sync + fmt::Debug {
    /// What would happen if the run started now.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the catalog cannot be read.
    fn preflight(&self, project: ProjectId) -> AuraResult<PreflightReport>;

    /// Start a run, or resume the project's unfinished one.
    ///
    /// # Errors
    ///
    /// `AURA-JOB-7004` when the pre-flight blocks, and whatever the catalog raises.
    fn start(&self, project: ProjectId, settings: &AutopilotOverride) -> AuraResult<RunHandle>;

    /// Stop the project's running run.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the catalog cannot be written.
    fn cancel(&self, project: ProjectId) -> AuraResult<bool>;

    /// The project header.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the stored rows cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<AutopilotOutline>;

    /// The newest finished run's summary.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the stored rows cannot be read.
    fn summary(&self, project: ProjectId) -> AuraResult<Option<RunSummary>>;

    /// Every stage of the newest run, with what happened to it.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the stored rows cannot be read.
    fn stages(&self, project: ProjectId) -> AuraResult<Vec<StageReport>>;

    /// Everything the governor did during the newest run.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the stored rows cannot be read.
    fn resource_events(&self, project: ProjectId) -> AuraResult<Vec<ResourceEvent>>;

    /// Record what the photographer chose in the checklist.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be written.
    fn set_settings(&self, settings: &AutopilotOverride) -> AuraResult<()>;
}

/// One stage of one run, as a panel reads it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageReport {
    /// Which stage.
    pub stage: StageId,
    /// The outcome slug.
    pub outcome: String,
    /// Why it did not run, when it did not.
    pub skip_cause: Option<SkipCause>,
    /// What the gate said.
    pub verdict: StageVerdict,
    /// Units finished.
    pub items_done: u32,
    /// Units it had to do.
    pub items_total: u32,
    /// Milliseconds of wall clock.
    pub elapsed_ms: u64,
    /// How many attempts it took.
    pub attempts: u8,
    /// The reasons attached to it.
    pub reasons: Vec<AutopilotReason>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stage_slug_is_unique_and_round_trips() {
        let mut seen = std::collections::BTreeSet::new();
        for stage in StageId::ALL {
            assert!(seen.insert(stage.as_str()), "duplicate slug {stage}");
            assert_eq!(StageId::parse(stage.as_str()), Some(stage));
        }
        assert_eq!(seen.len(), StageId::COUNT);
    }

    #[test]
    fn every_code_slug_is_unique_and_round_trips() {
        let mut seen = std::collections::BTreeSet::new();
        for code in AutopilotCode::ALL {
            assert!(seen.insert(code.as_str()), "duplicate code");
            assert_eq!(AutopilotCode::parse(code.as_str()), Some(code));
        }
        assert_eq!(seen.len(), AutopilotCode::COUNT);
    }

    #[test]
    fn only_a_stage_the_photographer_turned_off_is_an_expected_skip() {
        let expected: Vec<_> = SkipCause::ALL
            .into_iter()
            .filter(|cause| cause.is_expected())
            .collect();
        assert_eq!(expected, vec![SkipCause::TurnedOff]);
    }

    #[test]
    fn the_governor_has_no_action_that_makes_the_product_do_more() {
        // The ordering is the strength of the response, so `Proceed` is the weakest and there is
        // nothing below it. A variant that accelerated would have to sort before `Proceed`.
        assert_eq!(GovernorAction::ALL[0], GovernorAction::Proceed);
        assert!(GovernorAction::Proceed < GovernorAction::Reduce);
        assert!(GovernorAction::Reduce < GovernorAction::Pause);
        assert!(GovernorAction::Pause < GovernorAction::Stop);
        assert_eq!(GovernorAction::ALL.len(), 4);
    }

    #[test]
    fn an_unreadable_governor_action_pauses_rather_than_proceeds() {
        assert_eq!(
            GovernorAction::from_str_or_pause("something else"),
            GovernorAction::Pause
        );
    }

    #[test]
    fn require_review_holds_in_every_mode() {
        for zero_touch in [false, true] {
            assert_eq!(
                StageVerdict::from_band(Autonomy::RequireReview, zero_touch),
                StageVerdict::Hold
            );
        }
    }

    #[test]
    fn zero_touch_is_the_only_thing_that_unlocks_the_middle_two_bands() {
        assert_eq!(
            StageVerdict::from_band(Autonomy::AutoZeroTouch, false),
            StageVerdict::Hold
        );
        assert_eq!(
            StageVerdict::from_band(Autonomy::AutoZeroTouch, true),
            StageVerdict::Act
        );
        assert_eq!(
            StageVerdict::from_band(Autonomy::Suggest, false),
            StageVerdict::Hold
        );
        assert_eq!(
            StageVerdict::from_band(Autonomy::Suggest, true),
            StageVerdict::ActAndReview
        );
    }

    #[test]
    fn auto_acts_without_zero_touch() {
        assert_eq!(
            StageVerdict::from_band(Autonomy::Auto, false),
            StageVerdict::Act
        );
    }

    #[test]
    fn a_failed_stage_is_the_only_thing_that_blocks_its_dependents() {
        assert!(StageOutcome::Completed { items: 3 }.unblocks_dependents());
        assert!(StageOutcome::Skipped(SkipCause::ServiceAbsent).unblocks_dependents());
        assert!(StageOutcome::Partial {
            items: 2,
            failed: 1,
            detail: String::new()
        }
        .unblocks_dependents());
        assert!(!StageOutcome::Failed {
            code: "AURA-JOB-7005".into(),
            detail: String::new()
        }
        .unblocks_dependents());
    }

    #[test]
    fn a_skip_nobody_chose_is_not_clean() {
        assert!(StageOutcome::Skipped(SkipCause::TurnedOff).is_clean());
        for cause in SkipCause::ALL.into_iter().filter(|c| !c.is_expected()) {
            assert!(!StageOutcome::Skipped(cause).is_clean(), "{cause:?}");
        }
    }

    #[test]
    fn a_stopped_run_continues_and_a_delivered_one_does_not() {
        // The two halves have to partition the terminal statuses: a status that was neither
        // resumable nor finished would be a run a photographer could neither continue nor re-run.
        assert!(RunStatus::Cancelled.is_resumable());
        assert!(RunStatus::Failed.is_resumable());
        assert!(RunStatus::Running.is_resumable());
        assert!(!RunStatus::Completed.is_resumable());
        assert!(!RunStatus::CompletedDegraded.is_resumable());
        for status in RunStatus::ALL {
            assert_ne!(
                status.is_resumable(),
                status.is_finished(),
                "{status:?} is neither resumable nor finished, or is both"
            );
        }
    }

    #[test]
    fn preflight_verdict_is_the_strongest_row() {
        let report = PreflightReport {
            rows: vec![
                PreflightRow {
                    check: PreflightCheck::HasImages,
                    verdict: PreflightVerdict::Pass,
                    detail: String::new(),
                },
                PreflightRow {
                    check: PreflightCheck::Calibration,
                    verdict: PreflightVerdict::Warn,
                    detail: String::new(),
                },
                PreflightRow {
                    check: PreflightCheck::DiskSpace,
                    verdict: PreflightVerdict::Block,
                    detail: String::new(),
                },
            ],
            images: 10,
            estimated_output_bytes: 0,
            estimated_ms: 0,
        };
        assert_eq!(report.verdict(), PreflightVerdict::Block);
        assert!(!report.permits_start());
    }

    #[test]
    fn an_empty_preflight_passes_and_permits_a_start() {
        let report = PreflightReport {
            rows: Vec::new(),
            images: 0,
            estimated_output_bytes: 0,
            estimated_ms: 0,
        };
        assert_eq!(report.verdict(), PreflightVerdict::Pass);
        assert!(report.permits_start());
    }

    #[test]
    fn the_watch_publishes_a_new_version_on_every_write() {
        let watch = RunWatch::new(RunProgress::starting(StageId::Ingest, 25));
        assert_eq!(watch.version(), 0);
        watch.update(|progress| progress.items_done = 5);
        assert_eq!(watch.version(), 1);
        assert_eq!(watch.borrow().items_done, 5);
        watch.publish(RunProgress::starting(StageId::Embed, 25));
        assert_eq!(watch.version(), 2);
        assert_eq!(watch.borrow().stage, StageId::Embed);
    }

    #[test]
    fn measuring_stages_have_no_decision_kind_and_deciding_stages_do() {
        assert_eq!(StageId::Embed.decision_kind(), None);
        assert_eq!(StageId::Cull.decision_kind(), Some(DecisionKind::Cull));
        assert_eq!(
            StageId::Retouch.decision_kind(),
            Some(DecisionKind::Retouch)
        );
        assert_eq!(StageId::Export.decision_kind(), Some(DecisionKind::Export));
    }

    #[test]
    fn stage_fraction_is_zero_when_nothing_is_known() {
        let progress = RunProgress::starting(StageId::Ingest, 25);
        assert!((progress.stage_fraction() - 0.0).abs() < f32::EPSILON);
    }
}
