//! The session pool: load a model once, run it many times.
//!
//! Loading is the expensive part. Parsing a graph, rounding its weights to the
//! session's precision and measuring its working set costs tens of milliseconds
//! for a placeholder and will cost hundreds for a real backbone; doing it per
//! request would dominate every batch in the product.
//!
//! So sessions are pooled by model *and* provider - the same model on two
//! providers is two sessions, because the compiled artefacts have nothing in
//! common - and evicted least-recently-used when the pool is full. Eviction is
//! by an injected clock rather than by wall time, so a test can prove the policy
//! without sleeping.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::AuraResult;
use parking_lot::Mutex;

use crate::backend::{Backend, LoadedModel};
use crate::contract::infer::{ExecutionProvider, ModelRef};
use crate::source::ResolvedModel;

/// How many sessions stay resident before the least recently used is dropped.
///
/// Eight is one more than the largest number of models any single phase 05-29
/// stage uses at once, so a stage never evicts its own working set. It is a
/// count rather than a byte budget because the byte budget lives in the VRAM
/// ledger, and two budgets over one resource is how double-counting starts.
pub const DEFAULT_CAPACITY: usize = 8;

/// What the pool has been doing, for Settings and telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PoolStats {
    /// Sessions currently resident.
    pub resident: usize,
    /// Requests served from a resident session.
    pub hits: u64,
    /// Requests that had to load.
    pub loads: u64,
    /// Sessions dropped to make room.
    pub evictions: u64,
}

/// One resident session and its bookkeeping.
#[derive(Debug)]
struct Entry {
    session: Arc<dyn LoadedModel>,
    last_used_ms: u64,
    working_set_bytes: u64,
}

/// A pool of loaded models, keyed by model and provider.
#[derive(Debug)]
pub struct SessionPool {
    entries: Mutex<BTreeMap<String, Entry>>,
    stats: Mutex<PoolStats>,
    capacity: usize,
    clock: Arc<dyn Clock>,
}

impl SessionPool {
    /// A pool holding [`DEFAULT_CAPACITY`] sessions.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self::with_capacity(clock, DEFAULT_CAPACITY)
    }

    /// A pool of a chosen size. One is a legal size and is what the parity
    /// harness uses to force a reload between precisions.
    #[must_use]
    pub fn with_capacity(clock: Arc<dyn Clock>, capacity: usize) -> Self {
        Self {
            entries: Mutex::new(BTreeMap::new()),
            stats: Mutex::new(PoolStats::default()),
            capacity: capacity.max(1),
            clock,
        }
    }

    /// Fetch a session, loading it if it is not resident.
    ///
    /// # Errors
    ///
    /// Whatever the backend raised while loading: `AURA-ML-5010` for a file that
    /// will not parse, `AURA-ML-5006` for an operator we do not implement.
    pub fn session(
        &self,
        model: ModelRef,
        backend: &dyn Backend,
        resolved: &ResolvedModel,
    ) -> AuraResult<Arc<dyn LoadedModel>> {
        let key = pool_key(model, backend.provider());
        let now = self.clock.monotonic_ms();

        if let Some(entry) = self.entries.lock().get_mut(&key) {
            entry.last_used_ms = now;
            let mut stats = self.stats.lock();
            stats.hits += 1;
            return Ok(Arc::clone(&entry.session));
        }

        // Load outside the lock: a slow load must not block a request for a model
        // that is already resident.
        let session = backend.load(resolved)?;
        let working_set_bytes = session.working_set_bytes();

        let mut entries = self.entries.lock();
        let mut stats = self.stats.lock();
        stats.loads += 1;
        entries.insert(
            key,
            Entry {
                session: Arc::clone(&session),
                last_used_ms: now,
                working_set_bytes,
            },
        );
        while entries.len() > self.capacity {
            let oldest = entries
                .iter()
                .min_by_key(|(key, entry)| (entry.last_used_ms, (*key).clone()))
                .map(|(key, _)| key.clone());
            match oldest {
                Some(key) => {
                    entries.remove(&key);
                    stats.evictions += 1;
                }
                None => break,
            }
        }
        stats.resident = entries.len();
        Ok(session)
    }

    /// Drop every resident session, releasing their memory.
    ///
    /// Used when the plan changes provider: sessions compiled for one provider
    /// are meaningless on another, and keeping them would hold memory nothing
    /// will ever ask for again.
    pub fn clear(&self) {
        let mut entries = self.entries.lock();
        let dropped = entries.len() as u64;
        entries.clear();
        let mut stats = self.stats.lock();
        stats.evictions += dropped;
        stats.resident = 0;
    }

    /// Total resident working set, in bytes.
    #[must_use]
    pub fn resident_bytes(&self) -> u64 {
        self.entries
            .lock()
            .values()
            .map(|entry| entry.working_set_bytes)
            .sum()
    }

    /// A snapshot of the counters.
    #[must_use]
    pub fn stats(&self) -> PoolStats {
        let mut stats = *self.stats.lock();
        stats.resident = self.entries.lock().len();
        stats
    }
}

/// The pool key: model, version and provider.
fn pool_key(model: ModelRef, ep: ExecutionProvider) -> String {
    format!("{}::{}", model.key(), ep.as_str())
}
