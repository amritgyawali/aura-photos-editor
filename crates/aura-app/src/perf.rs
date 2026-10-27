//! Where the time goes, when somebody asks: `AURA_PERF=1` records how long each named step of
//! the batch pipeline took, and [`take`] hands the records to whoever is measuring.
//!
//! Off, it costs one environment read per process and nothing per step. It measures with the
//! application's own clock, so it never reads wall time directly.

use std::sync::atomic::{AtomicU8, Ordering};

use parking_lot::Mutex;

use crate::AppState;

static RECORDS: Mutex<Vec<(&'static str, u64)>> = Mutex::new(Vec::new());
/// 0 = not read yet, 1 = off, 2 = on.
static ON: AtomicU8 = AtomicU8::new(0);

/// True when timing is being recorded.
#[must_use]
pub fn enabled() -> bool {
    match ON.load(Ordering::Relaxed) {
        0 => {
            let on = std::env::var("AURA_PERF").is_ok_and(|v| v.trim() == "1");
            ON.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            on
        }
        state => state == 2,
    }
}

/// A step being timed; records itself when dropped.
#[derive(Debug)]
#[must_use = "the step is recorded when this is dropped"]
pub struct Step<'a> {
    state: &'a AppState,
    label: &'static str,
    started: Option<u64>,
}

/// Start timing a step.
pub fn step<'a>(state: &'a AppState, label: &'static str) -> Step<'a> {
    Step {
        state,
        label,
        started: enabled().then(|| state.clock().monotonic_ms()),
    }
}

impl Drop for Step<'_> {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            let ms = self.state.clock().monotonic_ms().saturating_sub(started);
            RECORDS.lock().push((self.label, ms));
        }
    }
}

/// Every record so far, emptied.
pub fn take() -> Vec<(&'static str, u64)> {
    std::mem::take(&mut *RECORDS.lock())
}
