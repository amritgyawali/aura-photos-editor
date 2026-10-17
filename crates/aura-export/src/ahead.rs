//! Rendering ahead of the writer.
//!
//! A pass renders and encodes frames on several workers and writes them on one thread, in the
//! job's order, because names, the manifest and the progress a photographer reads all follow
//! that order. Done in batches, every worker waits for the slowest frame of each batch and then
//! for every file of it to be written, synced and read back - and the processor sits idle for
//! exactly as long as the disk is busy. Here the workers never wait for the writer unless they
//! are a whole window ahead of it, so the disk and the processor work at the same time.
//!
//! What a frame becomes does not depend on which worker made it or when: each frame is prepared
//! on its own, and the writer takes them strictly in order.

use std::collections::BTreeMap;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

/// Frames being worked on ahead of the writer, and the ones ready for it.
pub(crate) struct Ahead<T> {
    state: Mutex<State<T>>,
    changed: Condvar,
    /// How many frames past the writer a worker may start.
    window: usize,
    total: usize,
}

struct State<T> {
    /// The next frame a worker will start.
    next: usize,
    /// Frames the writer has taken.
    taken: usize,
    ready: BTreeMap<usize, T>,
    /// The writer has gone: workers stop at the next frame.
    stopped: bool,
}

impl<T> Ahead<T> {
    pub(crate) fn new(total: usize, window: usize) -> Self {
        Self {
            state: Mutex::new(State {
                next: 0,
                taken: 0,
                ready: BTreeMap::new(),
                stopped: false,
            }),
            changed: Condvar::new(),
            window: window.max(1),
            total,
        }
    }

    fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait<'g>(&self, guard: MutexGuard<'g, State<T>>) -> MutexGuard<'g, State<T>> {
        self.changed
            .wait(guard)
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// One worker: prepare frames until there are none left or the writer has gone. `failed`
    /// stands in for a frame whose preparation panicked, so the writer is never left waiting.
    pub(crate) fn work(&self, prepare: impl Fn(usize) -> T, failed: impl Fn() -> T) {
        loop {
            let index = {
                let mut state = self.lock();
                loop {
                    if state.stopped || state.next >= self.total {
                        return;
                    }
                    if state.next < state.taken + self.window {
                        break;
                    }
                    state = self.wait(state);
                }
                state.next += 1;
                state.next - 1
            };
            let claim = Claim {
                ahead: self,
                index,
                failed: &failed,
                done: false,
            };
            let value = prepare(index);
            claim.finish(value);
        }
    }

    /// The writer: frame `index`, once a worker has prepared it.
    pub(crate) fn take(&self, index: usize) -> Option<T> {
        let mut state = self.lock();
        loop {
            if let Some(value) = state.ready.remove(&index) {
                state.taken = state.taken.max(index + 1);
                self.changed.notify_all();
                return Some(value);
            }
            if state.stopped {
                return None;
            }
            state = self.wait(state);
        }
    }

    /// The writer has finished, or stopped early: let every worker go.
    pub(crate) fn stop(&self) {
        self.lock().stopped = true;
        self.changed.notify_all();
    }

    fn put(&self, index: usize, value: T) {
        self.lock().ready.insert(index, value);
        self.changed.notify_all();
    }
}

/// A frame a worker has started. Dropped without being finished - its preparation panicked -
/// it hands the writer the stand-in instead.
struct Claim<'a, T, F: Fn() -> T> {
    ahead: &'a Ahead<T>,
    index: usize,
    failed: &'a F,
    done: bool,
}

impl<T, F: Fn() -> T> Claim<'_, T, F> {
    fn finish(mut self, value: T) {
        self.done = true;
        self.ahead.put(self.index, value);
    }
}

impl<T, F: Fn() -> T> Drop for Claim<'_, T, F> {
    fn drop(&mut self) {
        if !self.done {
            self.ahead.put(self.index, (self.failed)());
        }
    }
}

/// Stops the workers when the writer leaves, however it leaves.
pub(crate) struct StopOnDrop<'a, T>(pub(crate) &'a Ahead<T>);

impl<T> Drop for StopOnDrop<'_, T> {
    fn drop(&mut self) {
        self.0.stop();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_writer_gets_every_frame_in_order_and_workers_stay_within_the_window() {
        let ahead = Ahead::new(50, 3);
        let furthest = std::sync::atomic::AtomicUsize::new(0);
        let mut got = Vec::new();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    ahead.work(
                        |i| {
                            furthest.fetch_max(i, std::sync::atomic::Ordering::Relaxed);
                            i * 10
                        },
                        || usize::MAX,
                    );
                });
            }
            let _stop = StopOnDrop(&ahead);
            for i in 0..50 {
                // A worker never starts a frame more than a window past what was taken.
                assert!(furthest.load(std::sync::atomic::Ordering::Relaxed) < i + 3);
                got.push(ahead.take(i).unwrap());
            }
        });
        assert_eq!(got, (0..50).map(|i| i * 10).collect::<Vec<_>>());
    }

    #[test]
    fn a_writer_that_stops_early_releases_the_workers() {
        let ahead = Ahead::new(1000, 2);
        std::thread::scope(|scope| {
            for _ in 0..3 {
                scope.spawn(|| ahead.work(|i| i, || 0));
            }
            let _stop = StopOnDrop(&ahead);
            assert_eq!(ahead.take(0), Some(0));
            // Returning here drops the guard; the scope then joins three workers that would
            // otherwise wait forever for the writer to take frame 2.
        });
    }

    #[test]
    fn a_frame_whose_preparation_panics_reaches_the_writer_as_the_stand_in() {
        let ahead = Ahead::new(3, 3);
        let result = std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                ahead.work(
                    |i| {
                        assert!(i != 1, "deliberate");
                        i
                    },
                    || 99,
                );
            });
            let _stop = StopOnDrop(&ahead);
            let first = ahead.take(0);
            let second = ahead.take(1);
            let _ = worker.join();
            (first, second)
        });
        assert_eq!(result, (Some(0), Some(99)));
    }
}
