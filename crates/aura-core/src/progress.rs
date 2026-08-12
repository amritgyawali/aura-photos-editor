//! Cancellation and progress primitives.
//!
//! Every long-running stage takes a [`CancelToken`] and a [`ProgressSink`]. Both are
//! cheap to clone and safe to share across the rayon pool, which is what lets the UI
//! stop a 4,000-file import in the middle of a batch without corrupting the catalog.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// A cooperative cancel flag. Workers poll it between units of work.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    /// A fresh, uncancelled token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cancellation. Idempotent and safe from any thread.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// True once [`CancelToken::cancel`] has been called.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

/// A monotonically increasing counter of finished units, shared between workers.
#[derive(Debug, Clone, Default)]
pub struct ProgressCounter {
    done: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
}

impl ProgressCounter {
    /// A counter with a known or estimated total.
    #[must_use]
    pub fn with_total(total: u64) -> Self {
        let counter = Self::default();
        counter.set_total(total);
        counter
    }

    /// Replace the total once the walker knows the real file count.
    pub fn set_total(&self, total: u64) {
        self.total.store(total, Ordering::Relaxed);
    }

    /// Record `n` finished units and return the new running total.
    #[must_use = "if the running total is not needed, the return value can be ignored with `let _`"]
    pub fn advance(&self, n: u64) -> u64 {
        self.done.fetch_add(n, Ordering::Relaxed) + n
    }

    /// Units finished so far.
    #[must_use]
    pub fn done(&self) -> u64 {
        self.done.load(Ordering::Relaxed)
    }

    /// Units expected in total. Zero means "not known yet".
    #[must_use]
    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    /// Fraction complete in the range 0.0 to 1.0; zero while the total is unknown.
    #[must_use]
    pub fn fraction(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            (self.done() as f64 / total as f64).clamp(0.0, 1.0)
        }
    }
}

/// One progress observation handed to the UI event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressUpdate {
    /// Pipeline stage that produced the update, e.g. `"ingest.hash"`.
    pub stage: &'static str,
    /// Units finished.
    pub done: u64,
    /// Units expected, zero while unknown.
    pub total: u64,
    /// Short, already redacted description of the current unit.
    pub current: Option<String>,
}

/// Where progress updates go. The UI implements this over the Tauri event bus;
/// tests implement it over a vector.
pub trait ProgressSink: Send + Sync + std::fmt::Debug {
    /// Deliver one update. Implementations must never block the caller.
    fn report(&self, update: ProgressUpdate);
}

/// A sink that discards everything, for benchmarks and unit tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullProgress;

impl ProgressSink for NullProgress {
    fn report(&self, _update: ProgressUpdate) {}
}
