//! The scheduling queue: four classes, strict priority, bounded.
//!
//! Strict priority rather than weighted fair queueing, because the thing being
//! protected is a human being scrolling a grid. If a visible cell can ever wait
//! behind batch work, the grid stutters, and a stuttering grid is the single
//! most common complaint about every competitor in this category.
//!
//! Starvation of the lower classes is acceptable *and correct* here: background
//! prefetch exists precisely to be given up. What is not acceptable is unbounded
//! growth, so the queue has a capacity and pushes past it are rejected - the
//! caller of `prefetch` is told nothing, because a dropped speculative request
//! is not an error.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use aura_raw::contract::pixels::PixelLevel;

use crate::contract::service::{ImageId, Priority};
use crate::request::Request;

/// How many requests may wait. Two screens of a grid is roughly 500 cells, and
/// a batch stage submits 4,000; anything above this is speculation about
/// speculation.
pub const DEFAULT_CAPACITY: usize = 8_192;

/// A priority queue with de-duplication.
#[derive(Debug)]
pub struct PriorityQueue {
    lanes: BTreeMap<Priority, VecDeque<Request>>,
    queued: BTreeSet<(ImageId, PixelLevel)>,
    capacity: usize,
    sequence: u64,
    closed: bool,
}

impl PriorityQueue {
    /// A queue with the given capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let mut lanes = BTreeMap::new();
        for priority in Priority::all() {
            lanes.insert(priority, VecDeque::new());
        }
        Self {
            lanes,
            queued: BTreeSet::new(),
            capacity: capacity.max(1),
            sequence: 0,
            closed: false,
        }
    }

    /// Submit work.
    ///
    /// Returns `false` when the request was dropped: either the queue is full,
    /// the same work is already waiting, or the queue has been closed. A request
    /// that is already waiting at a lower priority is **promoted** rather than
    /// duplicated, so opening a cell that a batch had already queued jumps it to
    /// the front instead of decoding it twice.
    pub fn push(&mut self, id: ImageId, level: PixelLevel, priority: Priority) -> bool {
        if self.closed {
            return false;
        }
        let key = (id, level);
        if self.queued.contains(&key) {
            return self.promote(key, priority);
        }
        if self.len() >= self.capacity {
            return false;
        }
        self.sequence += 1;
        let request = Request {
            id,
            level,
            priority,
            sequence: self.sequence,
        };
        if let Some(lane) = self.lanes.get_mut(&priority) {
            lane.push_back(request);
            self.queued.insert(key);
            return true;
        }
        false
    }

    fn promote(&mut self, key: (ImageId, PixelLevel), priority: Priority) -> bool {
        let mut found: Option<Request> = None;
        for (lane_priority, lane) in &mut self.lanes {
            if *lane_priority <= priority {
                // Already at least this urgent.
                if lane.iter().any(|request| request.dedupe_key() == key) {
                    return false;
                }
                continue;
            }
            if let Some(position) = lane.iter().position(|request| request.dedupe_key() == key) {
                found = lane.remove(position);
                break;
            }
        }
        let Some(mut request) = found else {
            return false;
        };
        request.priority = priority;
        if let Some(lane) = self.lanes.get_mut(&priority) {
            lane.push_back(request);
            return true;
        }
        false
    }

    /// Take the most urgent request, oldest first within a class.
    pub fn pop(&mut self) -> Option<Request> {
        for priority in Priority::all() {
            if let Some(lane) = self.lanes.get_mut(&priority) {
                if let Some(request) = lane.pop_front() {
                    self.queued.remove(&request.dedupe_key());
                    return Some(request);
                }
            }
        }
        None
    }

    /// Drop every waiting request for these images, whatever their class.
    ///
    /// This is what a grid does when the user scrolls: work for cells that have
    /// left the viewport is not merely deprioritised, it is abandoned.
    pub fn cancel(&mut self, ids: &[ImageId]) -> usize {
        let doomed: BTreeSet<ImageId> = ids.iter().copied().collect();
        let mut removed = 0usize;
        for lane in self.lanes.values_mut() {
            let before = lane.len();
            lane.retain(|request| !doomed.contains(&request.id));
            removed += before - lane.len();
        }
        self.queued.retain(|(id, _)| !doomed.contains(id));
        removed
    }

    /// How many requests are waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lanes.values().map(VecDeque::len).sum()
    }

    /// True when nothing is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many requests are waiting in one class.
    #[must_use]
    pub fn len_of(&self, priority: Priority) -> usize {
        self.lanes.get(&priority).map_or(0, VecDeque::len)
    }

    /// Stop accepting work and discard what is waiting. Used at shutdown.
    pub fn close(&mut self) {
        self.closed = true;
        for lane in self.lanes.values_mut() {
            lane.clear();
        }
        self.queued.clear();
    }

    /// True once [`PriorityQueue::close`] has been called.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }
}

impl Default for PriorityQueue {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }
}
