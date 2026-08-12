//! The index: what is in the cache, how big it is, and what to drop first.
//!
//! Recency is tracked with a monotonic counter rather than a wall clock. A cache
//! whose eviction order depends on the system clock evicts the wrong entries the
//! moment the clock is corrected backwards, and - worse for this project - makes
//! two runs over the same inputs behave differently. A counter is deterministic,
//! survives a clock change and is one atomic increment.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One entry's bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Bytes on disk.
    pub bytes: u64,
    /// BLAKE3 of the stored payload, so corruption is detectable on read.
    pub digest: String,
    /// Value of the access counter when this entry was last used.
    pub last_used: u64,
}

/// The whole index. `BTreeMap` rather than a hash map so that iteration order -
/// and therefore eviction order among equally old entries - is deterministic.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheIndex {
    /// Entries keyed by [`crate::paths::CacheKey::id`].
    pub entries: BTreeMap<String, IndexEntry>,
    /// The access counter.
    pub clock: u64,
    /// Lifetime hit count, for the settings panel.
    pub hits: u64,
    /// Lifetime miss count.
    pub misses: u64,
    /// Lifetime eviction count.
    pub evictions: u64,
}

impl CacheIndex {
    /// Total bytes accounted for.
    #[must_use]
    pub fn bytes_used(&self) -> u64 {
        self.entries.values().map(|entry| entry.bytes).sum()
    }

    /// Record an entry, replacing any previous one with the same key.
    pub fn insert(&mut self, id: String, bytes: u64, digest: String) {
        self.clock += 1;
        self.entries.insert(
            id,
            IndexEntry {
                bytes,
                digest,
                last_used: self.clock,
            },
        );
    }

    /// Mark an entry as just used, and report whether it was there.
    pub fn touch(&mut self, id: &str) -> bool {
        self.clock += 1;
        let clock = self.clock;
        match self.entries.get_mut(id) {
            Some(entry) => {
                entry.last_used = clock;
                self.hits += 1;
                true
            }
            None => {
                self.misses += 1;
                false
            }
        }
    }

    /// Forget an entry.
    pub fn remove(&mut self, id: &str) -> Option<IndexEntry> {
        self.entries.remove(id)
    }

    /// The ids to evict, oldest first, until the total fits inside `budget`.
    ///
    /// Ties are broken by id so the choice is reproducible; without that, two
    /// runs of the same test could evict different entries and only one of them
    /// would fail.
    #[must_use]
    pub fn eviction_order(&self, budget: u64) -> Vec<String> {
        let mut used = self.bytes_used();
        if used <= budget {
            return Vec::new();
        }
        let mut candidates: Vec<(&String, &IndexEntry)> = self.entries.iter().collect();
        candidates.sort_by(|(left_id, left), (right_id, right)| {
            left.last_used
                .cmp(&right.last_used)
                .then_with(|| left_id.cmp(right_id))
        });

        let mut doomed = Vec::new();
        for (id, entry) in candidates {
            if used <= budget {
                break;
            }
            used = used.saturating_sub(entry.bytes);
            doomed.push(id.clone());
        }
        doomed
    }
}
