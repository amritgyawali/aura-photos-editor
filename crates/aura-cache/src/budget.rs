//! The budget, and what the photographer is told about it.
//!
//! A preview cache that grows without limit fills the disk the wedding is being
//! edited from, which turns a convenience into an outage. So the budget is hard:
//! the cache never knowingly exceeds it, eviction happens on write rather than
//! on a timer, and the number is shown in settings as "previews use X GB of Y"
//! with a one-click purge beside it.

use serde::{Deserialize, Serialize};

/// Default ceiling: 40 GB, which holds roughly 11,000 images at the measured
/// 3.5 GB per 1,000 and therefore three or four weddings at once.
pub const DEFAULT_BUDGET_BYTES: u64 = 40 * 1024 * 1024 * 1024;

/// The smallest budget worth accepting. Below this the cache thrashes: every
/// write evicts the entry the next request needs.
pub const MIN_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

/// How much disk the cache may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheBudget {
    /// Hard ceiling in bytes.
    pub max_bytes: u64,
}

impl Default for CacheBudget {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_BUDGET_BYTES,
        }
    }
}

impl CacheBudget {
    /// A budget clamped to something workable.
    ///
    /// A user who types 10 MB gets [`MIN_BUDGET_BYTES`] rather than a cache that
    /// evicts everything it just wrote; the clamp is visible in the settings
    /// panel rather than silent.
    #[must_use]
    pub fn clamped(max_bytes: u64) -> Self {
        Self {
            max_bytes: max_bytes.max(MIN_BUDGET_BYTES),
        }
    }
}

/// What the cache reports about itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
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
}

impl CacheStats {
    /// Hit rate in the range 0.0 to 1.0; zero before anything has been asked
    /// for, which is honest rather than flattering.
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.hits as f64 / total as f64
        }
    }

    /// Fraction of the budget in use, clamped to 1.0.
    #[must_use]
    pub fn fill_fraction(&self) -> f64 {
        if self.budget_bytes == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            (self.bytes_used as f64 / self.budget_bytes as f64).clamp(0.0, 1.0)
        }
    }
}
