//! Admission control: memory, priority and cancellation.
//!
//! Three promises are kept here, and they are the three that decide whether a
//! photographer trusts the application with a 4,000-image run.
//!
//! * **It never runs out of memory.** Every session declares its working set at
//!   warmup and the ledger admits work against a budget. Refusing a batch is
//!   cheap; a device out of memory at image 2,800 is the worst outcome in the
//!   product (Article VIII rule P6), so the scheduler halves and retries rather
//!   than hoping.
//! * **A click beats a batch.** Batch work waits at a gate between chunks
//!   whenever an interactive request is waiting, so preemption is bounded by one
//!   chunk rather than by one job.
//! * **Stop means stop.** Cancellation is checked at every chunk boundary, so a
//!   cancelled run releases its memory within a chunk instead of at the end of
//!   the job.
//!
//! Everything here is synchronous and lock-based rather than async. Inference is
//! CPU-bound work on a pool thread; an async runtime would add a scheduler on top
//! of a scheduler, and Article III forbids blocking inside one anyway.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::{AuraResult, Priority};
use parking_lot::{Condvar, Mutex};

/// The share of a memory budget the scheduler will actually use, as a percentage.
///
/// Section 6.2 says 70 per cent of free memory. The remaining 30 per cent is not
/// waste: a driver's own allocations, the compositor, and the fact that "free"
/// was measured a moment ago on a machine the photographer is also using.
///
/// A percentage in integer arithmetic rather than a float fraction, because
/// `0.70f32` is 0.699999988 and a budget that is thirteen bytes short of the
/// documented one is a number nobody can reproduce by hand.
pub const BUDGET_PERCENT: u64 = 70;

/// A ledger of memory promised to sessions.
///
/// Deliberately not a lock around an allocator: nothing here allocates. It counts
/// what sessions *said* they would use, which is the only number available before
/// the allocation happens - and refusing before allocating is the entire point.
#[derive(Debug)]
pub struct MemoryLedger {
    budget_bytes: u64,
    used_bytes: AtomicU64,
    peak_bytes: AtomicU64,
    refusals: AtomicU64,
}

impl MemoryLedger {
    /// A ledger over a budget in bytes. Zero means unbudgeted - the processor
    /// path, where the operating system's own memory management is the ceiling.
    #[must_use]
    pub const fn new(budget_bytes: u64) -> Self {
        Self {
            budget_bytes,
            used_bytes: AtomicU64::new(0),
            peak_bytes: AtomicU64::new(0),
            refusals: AtomicU64::new(0),
        }
    }

    /// A ledger over the usable fraction of a device's free memory.
    #[must_use]
    pub fn from_free_mb(free_mb: u32) -> Self {
        let bytes = u64::from(free_mb) * 1024 * 1024;
        Self::new(bytes / 100 * BUDGET_PERCENT)
    }

    /// Try to reserve; `None` when it would not fit.
    #[must_use]
    pub fn reserve(self: &Arc<Self>, bytes: u64) -> Option<Reservation> {
        if self.budget_bytes == 0 {
            // Unbudgeted: still counted, so the peak is reportable, never refused.
            let used = self.used_bytes.fetch_add(bytes, Ordering::SeqCst) + bytes;
            self.peak_bytes.fetch_max(used, Ordering::SeqCst);
            return Some(Reservation {
                ledger: Arc::clone(self),
                bytes,
            });
        }

        let mut current = self.used_bytes.load(Ordering::SeqCst);
        loop {
            let next = current.saturating_add(bytes);
            if next > self.budget_bytes {
                self.refusals.fetch_add(1, Ordering::SeqCst);
                return None;
            }
            match self.used_bytes.compare_exchange(
                current,
                next,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    self.peak_bytes.fetch_max(next, Ordering::SeqCst);
                    return Some(Reservation {
                        ledger: Arc::clone(self),
                        bytes,
                    });
                }
                Err(observed) => current = observed,
            }
        }
    }

    /// Bytes currently promised.
    #[must_use]
    pub fn used_bytes(&self) -> u64 {
        self.used_bytes.load(Ordering::SeqCst)
    }

    /// The high-water mark since the ledger was created.
    #[must_use]
    pub fn peak_bytes(&self) -> u64 {
        self.peak_bytes.load(Ordering::SeqCst)
    }

    /// How many admissions have been refused.
    #[must_use]
    pub fn refusals(&self) -> u64 {
        self.refusals.load(Ordering::SeqCst)
    }

    /// The ceiling, in bytes. Zero means unbudgeted.
    #[must_use]
    pub const fn budget_bytes(&self) -> u64 {
        self.budget_bytes
    }
}

/// Memory held for the life of one piece of work. Released on drop, including
/// on the error and panic paths - which is what makes "no leaks on any path"
/// (Article X rule R8) structural rather than a review checklist item.
#[derive(Debug)]
pub struct Reservation {
    ledger: Arc<MemoryLedger>,
    bytes: u64,
}

impl Reservation {
    /// How much this reservation holds.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.ledger
            .used_bytes
            .fetch_sub(self.bytes, Ordering::SeqCst);
    }
}

/// A cooperative cancellation flag shared by a job and its workers.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    /// A token that has not been cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask everything holding this token to stop at its next chunk boundary.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// True once [`CancelToken::cancel`] has been called.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

/// The preemption gate.
///
/// Batch work checks in here before every chunk and waits while any interactive
/// request is queued. There is no starvation risk in the other direction: an
/// interactive request never waits at the gate, it only announces itself.
#[derive(Debug, Default)]
struct Gate {
    waiting: Mutex<usize>,
    clear: Condvar,
}

impl Gate {
    fn announce(&self) {
        let mut waiting = self.waiting.lock();
        *waiting += 1;
    }

    fn finished(&self) {
        let mut waiting = self.waiting.lock();
        *waiting = waiting.saturating_sub(1);
        if *waiting == 0 {
            self.clear.notify_all();
        }
    }

    fn wait_turn(&self) {
        let mut waiting = self.waiting.lock();
        while *waiting > 0 {
            self.clear.wait(&mut waiting);
        }
    }
}

/// Counters the settings panel and the performance tests read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SchedulerStats {
    /// Requests admitted.
    pub requests: u64,
    /// Total admission overhead, in microseconds.
    pub overhead_us: u64,
    /// The worst single admission, in microseconds.
    pub worst_overhead_us: u64,
    /// Times a batch was refused and had to shrink.
    pub downshifts: u64,
    /// Times work stopped because its token was cancelled.
    pub cancellations: u64,
}

impl SchedulerStats {
    /// Mean admission overhead in milliseconds; the budgeted number.
    #[must_use]
    pub fn mean_overhead_ms(&self) -> f32 {
        if self.requests == 0 {
            return 0.0;
        }
        (self.overhead_us as f32 / self.requests as f32) / 1000.0
    }
}

/// Admission control for every request the runtime serves.
#[derive(Debug)]
pub struct BatchScheduler {
    ledger: Arc<MemoryLedger>,
    gate: Arc<Gate>,
    clock: Arc<dyn Clock>,
    requests: AtomicU64,
    overhead_us: AtomicU64,
    worst_overhead_us: AtomicU64,
    downshifts: AtomicU64,
    cancellations: AtomicU64,
}

impl BatchScheduler {
    /// A scheduler over a memory budget.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>, ledger: Arc<MemoryLedger>) -> Self {
        Self {
            ledger,
            gate: Arc::new(Gate::default()),
            clock,
            requests: AtomicU64::new(0),
            overhead_us: AtomicU64::new(0),
            worst_overhead_us: AtomicU64::new(0),
            downshifts: AtomicU64::new(0),
            cancellations: AtomicU64::new(0),
        }
    }

    /// The memory ledger, for warmup and for Settings.
    #[must_use]
    pub fn ledger(&self) -> &Arc<MemoryLedger> {
        &self.ledger
    }

    /// Run one piece of work under admission control.
    ///
    /// The closure runs only after the work has a memory reservation and its turn
    /// at the gate. Admission overhead is measured around everything except the
    /// closure, because that is the number section 11 budgets at 0.4 ms.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5011` when the token is already cancelled, `AURA-GPU-4003` when
    /// the work does not fit the budget and the caller should retry smaller, and
    /// whatever the closure returned.
    pub fn admit<T, F>(
        &self,
        prio: Priority,
        bytes: u64,
        model: &str,
        batch: u16,
        cancel: &CancelToken,
        work: F,
    ) -> AuraResult<T>
    where
        F: FnOnce() -> AuraResult<T>,
    {
        let started_us = self.clock.monotonic_us();

        if cancel.is_cancelled() {
            self.cancellations.fetch_add(1, Ordering::SeqCst);
            return Err(crate::errors::cancelled(format!(
                "{model} cancelled before admission"
            )));
        }

        let _high = if prio.preempts() {
            self.gate.announce();
            Some(HighPriorityGuard {
                gate: Arc::clone(&self.gate),
            })
        } else {
            self.gate.wait_turn();
            None
        };

        let Some(_reservation) = self.ledger.reserve(bytes) else {
            self.downshifts.fetch_add(1, Ordering::SeqCst);
            self.record_overhead(started_us);
            return Err(crate::errors::memory_downshift(
                model,
                batch,
                (batch / 2).max(1),
            ));
        };

        self.record_overhead(started_us);
        work()
    }

    /// Record that a batch had to shrink, for the counters.
    pub fn note_downshift(&self) {
        self.downshifts.fetch_add(1, Ordering::SeqCst);
    }

    /// Record that work stopped because it was cancelled.
    pub fn note_cancellation(&self) {
        self.cancellations.fetch_add(1, Ordering::SeqCst);
    }

    /// A snapshot of the counters.
    #[must_use]
    pub fn stats(&self) -> SchedulerStats {
        SchedulerStats {
            requests: self.requests.load(Ordering::SeqCst),
            overhead_us: self.overhead_us.load(Ordering::SeqCst),
            worst_overhead_us: self.worst_overhead_us.load(Ordering::SeqCst),
            downshifts: self.downshifts.load(Ordering::SeqCst),
            cancellations: self.cancellations.load(Ordering::SeqCst),
        }
    }

    fn record_overhead(&self, started_us: u64) {
        let elapsed = self.clock.monotonic_us().saturating_sub(started_us);
        self.requests.fetch_add(1, Ordering::SeqCst);
        self.overhead_us.fetch_add(elapsed, Ordering::SeqCst);
        self.worst_overhead_us.fetch_max(elapsed, Ordering::SeqCst);
    }
}

/// Announces an interactive request at the gate for as long as it is in flight.
#[derive(Debug)]
struct HighPriorityGuard {
    gate: Arc<Gate>,
}

impl Drop for HighPriorityGuard {
    fn drop(&mut self) {
        self.gate.finished();
    }
}

/// Split a batch into chunks no larger than `batch_size`.
///
/// Chunking is what makes preemption and cancellation bounded: both are checked
/// between chunks, so a chunk is the longest a Stop button can take to be obeyed.
#[must_use]
pub fn chunk_ranges(total: usize, batch_size: u16) -> Vec<(usize, usize)> {
    let size = usize::from(batch_size.max(1));
    let mut ranges = Vec::new();
    let mut start = 0usize;
    while start < total {
        let end = (start + size).min(total);
        ranges.push((start, end));
        start = end;
    }
    ranges
}
