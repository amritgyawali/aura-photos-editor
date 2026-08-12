//! Direct wall-clock reads are banned by Clippy and by the banned-pattern gate.
//! All time reads go through this module, so tests can freeze time and the chaos
//! suite can jump it backwards.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use time::{Duration, OffsetDateTime};

/// The only sanctioned source of time in the product.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// Current wall-clock time in UTC. May jump when the operating system syncs.
    fn now_utc(&self) -> OffsetDateTime;

    /// Monotonic milliseconds since process start. Never goes backwards, so
    /// lease expiry and progress ETAs are safe against NTP corrections.
    fn monotonic_ms(&self) -> u64;
}

/// Production clock: real wall time plus a real monotonic base.
#[derive(Debug)]
pub struct SystemClock {
    start: std::time::Instant, // DETERMINISM: monotonic base, never serialised
}

impl Default for SystemClock {
    fn default() -> Self {
        Self {
            start: std::time::Instant::now(), // DETERMINISM: monotonic base, never serialised
        }
    }
}

impl Clock for SystemClock {
    fn now_utc(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc() // DETERMINISM: single permitted wall-clock read
    }

    fn monotonic_ms(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// Test and chaos clock. Time only moves when you move it.
#[derive(Debug, Default)]
pub struct FixedClock {
    utc: Mutex<Option<OffsetDateTime>>,
    mono: AtomicU64,
}

impl FixedClock {
    /// Start a frozen clock at `utc`.
    #[must_use]
    pub fn at(utc: OffsetDateTime) -> Arc<Self> {
        Arc::new(Self {
            utc: Mutex::new(Some(utc)),
            mono: AtomicU64::new(0),
        })
    }

    /// Move both the monotonic counter and the wall clock forward.
    pub fn advance_ms(&self, ms: u64) {
        self.mono.fetch_add(ms, Ordering::SeqCst);
        let mut guard = self.utc.lock();
        if let Some(t) = guard.as_mut() {
            *t += Duration::milliseconds(i64::try_from(ms).unwrap_or(0));
        }
    }

    /// Simulate an NTP correction or a user changing the system clock.
    pub fn set_wall_clock(&self, utc: OffsetDateTime) {
        *self.utc.lock() = Some(utc);
    }
}

impl Clock for FixedClock {
    fn now_utc(&self) -> OffsetDateTime {
        self.utc.lock().unwrap_or(OffsetDateTime::UNIX_EPOCH)
    }

    fn monotonic_ms(&self) -> u64 {
        self.mono.load(Ordering::SeqCst)
    }
}
