//! The decode worker pool.
//!
//! Sizing is the interesting decision. Decoding is CPU-bound and embarrassingly
//! parallel, so the obvious choice is one worker per core - and it is wrong,
//! because it is precisely when every core is busy proxying 4,000 files that the
//! photographer scrolls the grid. The pool therefore leaves one core free:
//! `cores - 1` workers, minimum one. That single reserved core is what turns
//! "visible work has the highest priority" from a statement about a queue into a
//! promise about latency.
//!
//! Backpressure is the queue's capacity. When it is full, `prefetch` drops
//! requests silently, which is correct: speculative work that cannot be
//! scheduled is work that should not exist.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use aura_raw::contract::pixels::PixelLevel;
use parking_lot::{Condvar, Mutex};

use crate::contract::service::{ImageId, Priority};
use crate::priority::PriorityQueue;
use crate::request::Request;

/// Work the pool knows how to perform.
pub trait Handler: Send + Sync + 'static {
    /// Produce one preview. Errors are handled by the handler itself - the pool
    /// has no opinion about them, because quarantine policy belongs to the
    /// service, not to the scheduler.
    fn handle(&self, request: &Request);
}

impl<F> Handler for F
where
    F: Fn(&Request) + Send + Sync + 'static,
{
    fn handle(&self, request: &Request) {
        self(request);
    }
}

#[derive(Debug)]
struct Shared {
    queue: Mutex<PriorityQueue>,
    ready: Condvar,
    running: AtomicBool,
    in_flight: AtomicU64,
    completed: AtomicU64,
    dropped: AtomicU64,
}

/// A fixed set of worker threads draining a priority queue.
#[derive(Debug)]
pub struct DecodePool {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

impl DecodePool {
    /// How many workers this machine should run: every core but one.
    #[must_use]
    pub fn default_worker_count() -> usize {
        std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(4)
            .saturating_sub(1)
            .max(1)
    }

    /// Start a pool.
    #[must_use]
    pub fn new<H: Handler>(workers: usize, capacity: usize, handler: Arc<H>) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(PriorityQueue::with_capacity(capacity)),
            ready: Condvar::new(),
            running: AtomicBool::new(true),
            in_flight: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        });

        let mut handles = Vec::with_capacity(workers.max(1));
        for index in 0..workers.max(1) {
            let shared = Arc::clone(&shared);
            let handler = Arc::clone(&handler);
            let spawned = std::thread::Builder::new()
                .name(format!("aura-preview-{index}"))
                .spawn(move || worker_loop(&shared, handler.as_ref()));
            match spawned {
                Ok(handle) => handles.push(handle),
                // A machine that cannot start a thread will still work, just
                // serially: the service falls back to decoding inline.
                Err(error) => tracing::warn!(
                    target: "preview",
                    error = %error,
                    "could not start a decode worker"
                ),
            }
        }

        Self {
            shared,
            workers: handles,
        }
    }

    /// Submit work. Returns false when the request was dropped or merged.
    pub fn submit(&self, id: ImageId, level: PixelLevel, priority: Priority) -> bool {
        let accepted = {
            let mut queue = self.shared.queue.lock();
            queue.push(id, level, priority)
        };
        if accepted {
            self.shared.ready.notify_one();
        } else {
            self.shared.dropped.fetch_add(1, Ordering::Relaxed);
        }
        accepted
    }

    /// Abandon queued work for these images.
    pub fn cancel(&self, ids: &[ImageId]) -> usize {
        self.shared.queue.lock().cancel(ids)
    }

    /// How many requests are waiting.
    #[must_use]
    pub fn queued(&self) -> usize {
        self.shared.queue.lock().len()
    }

    /// How many requests are being worked on right now.
    #[must_use]
    pub fn in_flight(&self) -> u64 {
        self.shared.in_flight.load(Ordering::Relaxed)
    }

    /// How many requests have finished.
    #[must_use]
    pub fn completed(&self) -> u64 {
        self.shared.completed.load(Ordering::Relaxed)
    }

    /// How many requests were dropped by backpressure or merged into an
    /// existing one.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    /// How many workers are running.
    #[must_use]
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    /// Stop accepting work, discard the queue and wait for the workers.
    pub fn shutdown(&mut self) {
        self.shared.running.store(false, Ordering::SeqCst);
        self.shared.queue.lock().close();
        self.shared.ready.notify_all();
        for handle in self.workers.drain(..) {
            // A worker that has already ended is not an error.
            let _ = handle.join();
        }
    }
}

impl Drop for DecodePool {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker_loop<H: Handler + ?Sized>(shared: &Arc<Shared>, handler: &H) {
    loop {
        let request = {
            let mut queue = shared.queue.lock();
            loop {
                if !shared.running.load(Ordering::SeqCst) {
                    return;
                }
                if let Some(found) = queue.pop() {
                    break Some(found);
                }
                // Waiting with a timeout rather than forever means a lost
                // notification delays a request by 100 ms instead of forever.
                shared
                    .ready
                    .wait_for(&mut queue, std::time::Duration::from_millis(100));
            }
        };

        let Some(request) = request else {
            continue;
        };
        shared.in_flight.fetch_add(1, Ordering::Relaxed);
        handler.handle(&request);
        shared.in_flight.fetch_sub(1, Ordering::Relaxed);
        shared.completed.fetch_add(1, Ordering::Relaxed);
    }
}
