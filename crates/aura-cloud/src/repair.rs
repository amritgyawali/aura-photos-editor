//! Exactly one repair attempt, and the sentence it is made of.
//!
//! Section 6.2 says "on failure send exactly one repair message containing the
//! validator error and the original response, then fall back locally". One, not
//! a loop, and the count is a constant here rather than a configurable so that no
//! later phase can quietly make it three.
//!
//! The reasoning is a cost argument as much as a correctness one. A model that
//! got the shape wrong twice is a model that has misunderstood the task, and a
//! third attempt at the user's expense buys a third malformed answer. A single
//! well-written repair message recovers the ordinary cases - a stray markdown
//! fence, a missing optional, a number outside its range - and everything else is
//! the local fallback's job.
//!
//! The message itself is written for a model rather than for a log. It says what
//! was wrong in the validator's own words, restates the one rule that matters
//! ("only JSON"), and does not apologise, explain the product, or re-send the
//! schema - the schema was in the system prompt and repeating it doubles the
//! input tokens of the repair.

/// The number of repair attempts. One. Not a setting.
pub const REPAIR_ATTEMPTS: u32 = 1;

/// The user turn that follows the model's rejected answer.
///
/// `complaint` is the validator's own sentence - `/confidence: expected at most
/// 1, found 1.4; /reasons: required, but missing` - which is precise enough to
/// act on and short enough not to cost anything.
#[must_use]
pub fn repair_instruction(complaint: &str) -> String {
    format!(
        "That response did not match the required schema: {complaint}.\n\
         Correct those specific problems and return the whole object again.\n\
         Return ONLY the JSON object. No prose, no markdown, no code fence."
    )
}

/// Why a repair was attempted, for the audit row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairOutcome {
    /// The first answer was already valid; no repair was made.
    NotNeeded,
    /// The repair produced a valid answer.
    Repaired,
    /// The repair also failed, and the local fallback answered.
    Failed,
}

impl RepairOutcome {
    /// Stable text for the `status` column of the audit row.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotNeeded => "ok",
            Self::Repaired => "repaired",
            Self::Failed => "schema_invalid",
        }
    }

    /// How many retries this outcome represents.
    #[must_use]
    pub const fn retry_count(self) -> u32 {
        match self {
            Self::NotNeeded => 0,
            Self::Repaired | Self::Failed => REPAIR_ATTEMPTS,
        }
    }
}
