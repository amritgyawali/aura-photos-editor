//! How many expensive image passes may run at once in one process.
//!
//! A full-resolution decode, render or retouch plan holds several working buffers the size of
//! the photograph. The gate bounds how many of them exist at the same time, so a folder run, an
//! export and an open preview cannot together exceed the memory one machine has. It changes
//! scheduling only: what any pass computes is the same with one permit or with eight.
//!
//! One permit is the old behaviour - one pass at a time - and is what a small machine gets. A
//! larger machine gets a few, because a wedding of a thousand photographs processed one at a
//! time leaves all but one of its cores idle for hours.

use std::sync::{Condvar, Mutex, PoisonError};

/// The most permits [`WorkGate::for_this_machine`] grants without being told to.
pub const DEFAULT_MAX_PERMITS: usize = 4;
/// The most permits `AURA_IMAGE_WORKERS` may ask for.
pub const MAX_PERMITS: usize = 8;

/// A counting gate over expensive image work.
#[derive(Debug)]
pub struct WorkGate {
    permits: usize,
    busy: Mutex<usize>,
    freed: Condvar,
}

/// One pass's place in the gate, given back when it is dropped.
#[derive(Debug)]
#[must_use = "the permit is released as soon as it is dropped"]
pub struct WorkPermit<'a> {
    gate: &'a WorkGate,
}

impl WorkGate {
    /// A gate that lets `permits` passes run at once (at least one).
    #[must_use]
    pub fn new(permits: usize) -> Self {
        Self {
            permits: permits.clamp(1, MAX_PERMITS),
            busy: Mutex::new(0),
            freed: Condvar::new(),
        }
    }

    /// A gate sized for this machine: `AURA_IMAGE_WORKERS` when it is set to a number,
    /// otherwise from the number of hardware threads.
    #[must_use]
    pub fn for_this_machine() -> Self {
        let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        let asked = std::env::var("AURA_IMAGE_WORKERS")
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok());
        Self::new(asked.unwrap_or_else(|| permits_for(threads)))
    }

    /// How many passes may run at once.
    #[must_use]
    pub const fn permits(&self) -> usize {
        self.permits
    }

    /// How many passes are running now.
    #[must_use]
    pub fn busy(&self) -> usize {
        *self.busy.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Wait for a place and take it.
    pub fn acquire(&self) -> WorkPermit<'_> {
        let mut busy = self.busy.lock().unwrap_or_else(PoisonError::into_inner);
        while *busy >= self.permits {
            busy = self
                .freed
                .wait(busy)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *busy += 1;
        WorkPermit { gate: self }
    }

    /// Take a place only if one is free now.
    pub fn try_acquire(&self) -> Option<WorkPermit<'_>> {
        let mut busy = self.busy.lock().unwrap_or_else(PoisonError::into_inner);
        if *busy >= self.permits {
            return None;
        }
        *busy += 1;
        Some(WorkPermit { gate: self })
    }
}

impl Default for WorkGate {
    fn default() -> Self {
        Self::new(1)
    }
}

impl Drop for WorkPermit<'_> {
    fn drop(&mut self) {
        let mut busy = self
            .gate
            .busy
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *busy = busy.saturating_sub(1);
        self.gate.freed.notify_one();
    }
}

/// Permits for a machine with `threads` hardware threads. A pass on a ten-megapixel frame holds
/// about 1.4 GB at its peak, and a machine with eight or more threads almost always has the
/// memory for two or three of them; below that the old one-at-a-time behaviour is kept.
#[must_use]
pub const fn permits_for(threads: usize) -> usize {
    if threads >= 12 {
        DEFAULT_MAX_PERMITS
    } else if threads >= 8 {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn no_more_than_the_permits_run_at_once() {
        let gate = Arc::new(WorkGate::new(2));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let workers: Vec<_> = (0..6)
            .map(|_| {
                let (gate, active, peak) = (gate.clone(), active.clone(), peak.clone());
                std::thread::spawn(move || {
                    let _permit = gate.acquire();
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    active.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(peak.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn try_acquire_refuses_only_while_every_permit_is_held() {
        let gate = WorkGate::new(1);
        let held = gate.acquire();
        assert!(gate.try_acquire().is_none());
        drop(held);
        assert!(gate.try_acquire().is_some());
    }

    #[test]
    fn small_machines_keep_one_pass_at_a_time() {
        assert_eq!(permits_for(4), 1);
        assert_eq!(permits_for(8), 2);
        assert_eq!(permits_for(12), 4);
        assert_eq!(permits_for(64), 4);
        assert_eq!(WorkGate::new(0).permits(), 1);
        assert_eq!(WorkGate::new(99).permits(), MAX_PERMITS);
    }
}
