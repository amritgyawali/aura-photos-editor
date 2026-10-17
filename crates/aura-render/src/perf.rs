//! Where a render spends its time, when somebody asks: `AURA_PERF=1` records how long each
//! named stage took and [`take`] hands the records over. Off, it costs one environment read
//! per process. It measures with the engine's own clock.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use aura_core::clock::{Clock, SystemClock};

fn clock() -> &'static SystemClock {
    static CLOCK: OnceLock<SystemClock> = OnceLock::new();
    CLOCK.get_or_init(SystemClock::default)
}

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

/// A stage being timed; records itself when dropped.
#[must_use = "the stage is recorded when this is dropped"]
pub struct Step {
    label: &'static str,
    started: Option<u64>,
}

impl std::fmt::Debug for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label)
    }
}

/// Start timing a stage.
pub fn step(label: &'static str) -> Step {
    Step {
        label,
        started: enabled().then(|| clock().monotonic_ms()),
    }
}

impl Drop for Step {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            let ms = clock().monotonic_ms().saturating_sub(started);
            if let Ok(mut records) = RECORDS.lock() {
                records.push((self.label, ms));
            }
        }
    }
}

/// Every record so far, emptied.
pub fn take() -> Vec<(&'static str, u64)> {
    RECORDS
        .lock()
        .map(|mut records| std::mem::take(&mut *records))
        .unwrap_or_default()
}
