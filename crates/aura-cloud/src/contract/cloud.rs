//! FROZEN CONTRACT. The one way to ask a cloud model anything.
//!
//! Phase 04 section 5 freezes this before any provider exists, and the ordering
//! is the point: phases 07, 10, 12, 13, 24, 27, 28, 29 and 30 are all written
//! against [`CloudTask`], so none of them can grow a dependency on *which*
//! vendor answered, on *whether* one answered at all, or on what it cost.
//!
//! What is deliberately **not** frozen is the `Transport` port in
//! [`crate::provider`]. `CloudTask` and [`CloudResult`] are what callers see; how
//! many providers sit behind them, and whether one of them speaks TLS, is an
//! implementation detail that must be free to change without an ADR. See
//! `docs/adr/ADR-0009-cloud-ai-policy.md`.
//!
//! Three properties are encoded in the types rather than in prose.
//!
//! **A task without a local fallback cannot be written.** `local_fallback` is a
//! required method, not a defaulted one, so invariant 6 - the product completes a
//! full wedding with no network - is enforced by the compiler rather than by
//! review.
//!
//! **Every answer says where it came from.** [`CloudResult::source`] is not
//! optional and not a log field; the Explain panel in phase 13 reads it, and a
//! decision that cannot say whether a cloud, a cache or a local heuristic made it
//! is a decision nobody can defend to a client.
//!
//! **Inputs are hashable, therefore integral.** The `Hash` bound on
//! [`CloudTask::Input`] means an input struct cannot hold an `f32`. That is not
//! an inconvenience to work around: scores that reach a cache key must be
//! quantised - per-mille `u16` is the house convention - so that a 1e-9
//! difference between two runs of the same local classifier cannot produce a
//! cache miss and a second bill.
//!
//! Changing anything in this file requires an ADR.

use std::fmt;
use std::hash::Hash;

use aura_core::AuraError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A semantic check the JSON Schema cannot express.
///
/// The schema proves the shape; this proves the meaning. "`confidence` is a
/// number between 0 and 1" is a schema rule. "a decision with `confidence` above
/// 0.8 must give at least one reason" is invariant 2, and only the type knows it.
pub trait Validate {
    /// Check the semantics of a decoded response.
    ///
    /// # Errors
    ///
    /// A sentence naming the field and the rule it broke. That sentence is what
    /// the repair retry sends back to the model, so it is written for a reader,
    /// not for a log.
    fn validate(&self) -> Result<(), String>;
}

/// Where an answer came from. Always recorded, never inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A provider answered this call.
    Cloud,
    /// A previous identical call answered it, from the on-disk cache.
    Cache,
    /// The task's own deterministic local function answered it.
    LocalFallback,
}

impl Source {
    /// Stable text for the audit row, telemetry and the Explain panel.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cloud => "cloud",
            Self::Cache => "cache",
            Self::LocalFallback => "local_fallback",
        }
    }

    /// True when the user was billed for this answer.
    #[must_use]
    pub const fn was_billed(self) -> bool {
        matches!(self, Self::Cloud)
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The capability class a task needs, and the lever the cost governor pulls.
///
/// Three tiers rather than model names, because a task that named
/// `claude-sonnet-4-5` would have to be edited every time a vendor ships. A task
/// declares the cheapest tier that can do its job; the governor picks the
/// cheapest *acceptable* one, and downgrades one step when the budget runs low.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Cheapest. Classification, extraction, short structured answers.
    Cheap,
    /// The default. Vision plus multi-step structure.
    Balanced,
    /// Most capable, most expensive. Long-context judgement and planning.
    Reasoning,
}

impl Tier {
    /// Stable text for the manifest, the price table and telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cheap => "cheap",
            Self::Balanced => "balanced",
            Self::Reasoning => "reasoning",
        }
    }

    /// The next cheaper tier, or `None` at the floor.
    ///
    /// Downgrading is one step at a time and never below the task's declared
    /// minimum, so a task that genuinely needs judgement is refused rather than
    /// answered badly by a model that cannot do it.
    #[must_use]
    pub const fn cheaper(self) -> Option<Self> {
        match self {
            Self::Reasoning => Some(Self::Balanced),
            Self::Balanced => Some(Self::Cheap),
            Self::Cheap => None,
        }
    }
}

/// One image attached to a prompt.
///
/// The bytes are always an encoded derivative - JPEG or PNG - produced by
/// [`crate::payload`]. There is no variant of this type that can hold a RAW
/// buffer, which is how "no RAW ever leaves the machine" is enforced by the type
/// system rather than by a review checklist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePart {
    /// IANA media type, e.g. `image/jpeg`.
    pub media_type: String,
    /// The encoded bytes as they will be uploaded.
    pub bytes: Vec<u8>,
    /// blake3 of `bytes`, hex. Part of the cache key and of the audit row.
    pub content_hash: String,
    /// Width of the encoded image, for the token estimate.
    pub width: u32,
    /// Height of the encoded image, for the token estimate.
    pub height: u32,
}

/// A rendered prompt: everything that will be sent, and nothing that will not.
///
/// `temperature` is a field rather than a constant so a future task can argue for
/// a different value in an ADR. Every task in this phase leaves it at zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptSpec {
    /// The system prompt. Fixed per task version; part of the prompt hash.
    pub system: String,
    /// The user turn, rendered from the input with sorted JSON keys.
    pub user: String,
    /// Images, in a fixed order.
    pub images: Vec<ImagePart>,
    /// Ceiling on the response. A truncated JSON object is a schema failure.
    pub max_tokens: u32,
    /// Zero everywhere in phase 04. See `docs/adr/ADR-0009-cloud-ai-policy.md`.
    pub temperature: f32,
    /// The cheapest tier that can do this job.
    pub min_tier: Tier,
}

impl PromptSpec {
    /// A deterministic prompt at temperature zero.
    #[must_use]
    pub fn new(system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            system: system.into(),
            user: user.into(),
            images: Vec::new(),
            max_tokens: 1024,
            temperature: 0.0,
            min_tier: Tier::Balanced,
        }
    }

    /// Attach the images, in the order they will be sent.
    #[must_use]
    pub fn with_images(mut self, images: Vec<ImagePart>) -> Self {
        self.images = images;
        self
    }

    /// Set the response ceiling.
    #[must_use]
    pub const fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Declare the cheapest tier that can answer.
    #[must_use]
    pub const fn with_min_tier(mut self, tier: Tier) -> Self {
        self.min_tier = tier;
        self
    }

    /// The hash committed to the audit row and to the cache key.
    ///
    /// It covers the system prompt, the user turn and every image's content hash
    /// so re-ordering the images, or changing one pixel of one thumbnail, is a
    /// different call.
    ///
    /// It deliberately does **not** cover `max_tokens` or `temperature`: those
    /// are properties of the task version, and a task version bump already
    /// invalidates the cache.
    #[must_use]
    pub fn prompt_hash(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.system.as_bytes());
        hasher.update(b"\x1f");
        hasher.update(self.user.as_bytes());
        for image in &self.images {
            hasher.update(b"\x1f");
            hasher.update(image.content_hash.as_bytes());
        }
        hasher.finalize().to_hex().to_string()
    }
}

/// One versioned, schema-bound question a later phase may ask a cloud model.
///
/// # Implementing one
///
/// ```ignore
/// impl CloudTask for MyTask {
///     const NAME: &'static str = "my_task";
///     const VERSION: u16 = 1;
///     type Input = MyInput;      // Serialize + Hash: no floats, quantise them
///     type Output = MyOutput;    // DeserializeOwned + Validate
///     fn prompt(&self, input: &MyInput) -> PromptSpec { .. }
///     fn output_schema(&self) -> &'static str { MY_SCHEMA }
///     fn local_fallback(&self, input: &MyInput) -> Result<MyOutput, AuraError> { .. }
/// }
/// ```
///
/// `VERSION` is part of the cache key and of the audit row. **Bump it whenever
/// the prompt text, the schema or the token ceiling changes**, or the cache will
/// serve answers made under the old contract.
pub trait CloudTask: Send + Sync {
    /// Stable registry name, `snake_case`. Appears in the audit table.
    const NAME: &'static str;
    /// Bumped on any change to the prompt, the schema or the ceilings.
    const VERSION: u16;
    /// What the caller supplies. `Hash` forbids floats; quantise to `u16`.
    type Input: Serialize + Hash;
    /// What the model must return. `Validate` is the semantic check.
    type Output: DeserializeOwned + Validate;

    /// Render the prompt for one input. Must be a pure function of `input`.
    fn prompt(&self, input: &Self::Input) -> PromptSpec;

    /// The JSON Schema the response is validated against.
    fn output_schema(&self) -> &'static str;

    /// The answer when there is no network, no key, no budget or no consent.
    ///
    /// # Errors
    ///
    /// Only when the input itself is unusable. A fallback that can fail for
    /// ordinary reasons breaks invariant 6, because the pipeline then has no
    /// answer at all.
    fn local_fallback(&self, input: &Self::Input) -> Result<Self::Output, AuraError>;

    /// The most one call of this task may cost, in US dollars.
    fn max_cost_usd(&self) -> f32 {
        0.02
    }
}

/// One answer, with the provenance a later phase needs to defend it.
///
/// `tokens_in`, `tokens_out` and `cost_usd` are zero for a cache hit and for a
/// local fallback, which is exactly how the spend meter stays honest: the number
/// on the screen is the sum of what was actually billed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudResult<T> {
    /// The validated answer.
    pub value: T,
    /// Cloud, cache or local fallback.
    pub source: Source,
    /// 0.0 to 1.0, comparable with local model confidence after phase 13
    /// calibrates it.
    pub confidence: f32,
    /// The model that answered, or `local` for a fallback.
    pub model: String,
    /// Prompt tokens billed.
    pub tokens_in: u32,
    /// Completion tokens billed.
    pub tokens_out: u32,
    /// What this call cost, in US dollars.
    pub cost_usd: f32,
    /// Identifier of the `cloud_calls` row this answer came from.
    pub call_id: Uuid,
}

impl<T> CloudResult<T> {
    /// Wrap a locally computed answer.
    ///
    /// Confidence is the caller's, not a constant: a fallback that guesses well
    /// on this input should say so, and one that is guessing should say that too.
    #[must_use]
    pub fn local(value: T, confidence: f32, call_id: Uuid) -> Self {
        Self {
            value,
            source: Source::LocalFallback,
            confidence,
            model: "local".to_string(),
            tokens_in: 0,
            tokens_out: 0,
            cost_usd: 0.0,
            call_id,
        }
    }

    /// Apply a function to the answer, keeping every piece of provenance.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> CloudResult<U> {
        CloudResult {
            value: f(self.value),
            source: self.source,
            confidence: self.confidence,
            model: self.model,
            tokens_in: self.tokens_in,
            tokens_out: self.tokens_out,
            cost_usd: self.cost_usd,
            call_id: self.call_id,
        }
    }

    /// Whether this answer may overrule a local decision of the given strength.
    ///
    /// The fusion rule from `docs/adr/ADR-0009-cloud-ai-policy.md`, in code so
    /// that every later phase gets the same answer: cloud reasoning may not
    /// override a local decision at confidence 0.90 or above unless it supplies
    /// contradicting evidence, and a local fallback never overrules anything -
    /// it *is* the local answer.
    #[must_use]
    pub fn may_override(&self, local_confidence: f32, cites_evidence: bool) -> bool {
        if self.source == Source::LocalFallback {
            return false;
        }
        local_confidence < LOCAL_EVIDENCE_FLOOR || cites_evidence
    }
}

/// A local decision at or above this confidence is not overruled by a cloud
/// answer that cites no contradicting evidence.
pub const LOCAL_EVIDENCE_FLOOR: f32 = 0.90;

/// How much confidence an unrecognised enum value costs.
///
/// Section 6.2: unknown enum values map to `unknown` and lower the confidence by
/// 0.2. A model that invents a scene name is still telling us it was uncertain,
/// so the answer is kept and discounted rather than thrown away.
pub const UNKNOWN_ENUM_PENALTY: f32 = 0.20;

/// The error type of this crate.
///
/// Article III has exactly one error type crossing a module boundary, so the
/// `CloudError` named in the phase document is an alias rather than a second
/// taxonomy. Codes are `AURA-CLOUD-6xxx`.
pub type CloudError = AuraError;
