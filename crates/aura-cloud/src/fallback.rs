//! The local answer, and the accounting that makes its absence visible.
//!
//! Invariant 6 says the product completes a full wedding with no network. This
//! module is where that promise is actually kept: every path out of the gateway
//! that is not a cloud answer or a cache hit ends here, in the task's own
//! deterministic function.
//!
//! The part that is easy to get wrong is not the dispatch - it is the
//! bookkeeping. A run that quietly fell back for all 75 of its calls looks
//! exactly like a run that did not need the cloud, unless someone counted. So
//! every fallback carries a [`FallbackReason`], the reasons are counted per run,
//! and [`FallbackLedger::summary`] is what the Explain panel and the QC agent
//! read when they need to say "these decisions would have been better with a
//! working key".

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use aura_core::AuraError;
use parking_lot::Mutex;

/// Why the local function answered instead of a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FallbackReason {
    /// Cloud AI is switched off for this project, or globally.
    Disabled,
    /// The photographer has not granted egress for this data class.
    NoConsent,
    /// There is no key for the selected provider.
    NoKey,
    /// The project or the month is at its cap.
    BudgetReached,
    /// One call would have cost more than the task allows.
    CostCeiling,
    /// The provider could not be reached, or refused.
    ProviderFailed,
    /// The answer did not match the schema, even after the repair.
    SchemaInvalid,
    /// The payload could not be built within the privacy rules.
    PayloadRefused,
    /// The bounded agent loop ran out of steps, tokens or time.
    AgentLimit,
}

impl FallbackReason {
    /// Stable text for the audit row, telemetry and the Explain panel.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NoConsent => "no_consent",
            Self::NoKey => "no_key",
            Self::BudgetReached => "budget_reached",
            Self::CostCeiling => "cost_ceiling",
            Self::ProviderFailed => "provider_failed",
            Self::SchemaInvalid => "schema_invalid",
            Self::PayloadRefused => "payload_refused",
            Self::AgentLimit => "agent_limit",
        }
    }

    /// Read the reason back out of the error that caused it.
    ///
    /// The mapping lives here rather than at each call site so that a new error
    /// code cannot be introduced without someone deciding, in one place, what it
    /// means for the decision that did not get made.
    #[must_use]
    pub fn of(error: &AuraError) -> Self {
        match error.code.0 {
            "AURA-CLOUD-6001" => Self::NoKey,
            "AURA-CLOUD-6005" => Self::SchemaInvalid,
            "AURA-CLOUD-6006" => Self::BudgetReached,
            "AURA-CLOUD-6007" => Self::Disabled,
            "AURA-CLOUD-6008" => Self::NoConsent,
            "AURA-CLOUD-6009" => Self::PayloadRefused,
            "AURA-CLOUD-6011" => Self::AgentLimit,
            "AURA-CLOUD-6014" => Self::CostCeiling,
            _ => Self::ProviderFailed,
        }
    }

    /// True when trying again later could produce a cloud answer.
    ///
    /// The Explain panel uses this to decide between "this decision used AURA's
    /// own models" and "this decision used AURA's own models; re-running with a
    /// working key would improve it".
    #[must_use]
    pub const fn is_recoverable(self) -> bool {
        matches!(
            self,
            Self::NoKey | Self::BudgetReached | Self::ProviderFailed | Self::CostCeiling
        )
    }
}

/// What fell back, how often, and why, for one run.
#[derive(Debug, Default)]
pub struct FallbackLedger {
    counts: Mutex<BTreeMap<(String, FallbackReason), u64>>,
    total: AtomicU64,
    cloud: AtomicU64,
    cache: AtomicU64,
}

impl FallbackLedger {
    /// An empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one local answer.
    pub fn note_fallback(&self, task: &str, reason: FallbackReason) {
        *self
            .counts
            .lock()
            .entry((task.to_string(), reason))
            .or_insert(0) += 1;
        self.total.fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            target: "cloud.fallback",
            task,
            reason = reason.as_str(),
            "a decision used the local model"
        );
    }

    /// Record one answer that came from a provider.
    pub fn note_cloud(&self) {
        self.cloud.fetch_add(1, Ordering::Relaxed);
    }

    /// Record one answer that came from the cache.
    pub fn note_cache(&self) {
        self.cache.fetch_add(1, Ordering::Relaxed);
    }

    /// Decisions answered locally.
    #[must_use]
    pub fn fallbacks(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    /// Decisions answered by a provider.
    #[must_use]
    pub fn cloud_answers(&self) -> u64 {
        self.cloud.load(Ordering::Relaxed)
    }

    /// Decisions answered from the cache.
    #[must_use]
    pub fn cache_answers(&self) -> u64 {
        self.cache.load(Ordering::Relaxed)
    }

    /// Cache hits over all answers that could have been cache hits.
    ///
    /// A local fallback is not a cache miss - nothing was looked for - so the
    /// denominator is cloud plus cache, which is the number section 11's
    /// "cache hit rate on re-run >= 70 %" budget means.
    #[must_use]
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_answers();
        let lookups = hits + self.cloud_answers();
        if lookups == 0 {
            return 0.0;
        }
        hits as f64 / lookups as f64
    }

    /// One line per (task, reason), sorted, for the report and the panel.
    #[must_use]
    pub fn summary(&self) -> Vec<(String, FallbackReason, u64)> {
        self.counts
            .lock()
            .iter()
            .map(|((task, reason), count)| (task.clone(), *reason, *count))
            .collect()
    }

    /// True when at least one decision could be improved by re-running.
    #[must_use]
    pub fn has_recoverable_gaps(&self) -> bool {
        self.counts
            .lock()
            .keys()
            .any(|(_, reason)| reason.is_recoverable())
    }
}
