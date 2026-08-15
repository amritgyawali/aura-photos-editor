//! The tightest sub-group: one press and hold of the shutter.
//!
//! PHASE-08 section 2.1's two-tier structure. A **moment** is a shot of the same
//! subject and action; a **burst** is the run of frames a camera produced without the
//! photographer's finger leaving the button. Fourteen frames of a bouquet toss are one
//! moment, and the six that came off at 10 fps as it left her hand are one burst
//! inside it.
//!
//! ## Why the distinction is worth a column
//!
//! Because they answer different questions, and phase 12 asks both.
//!
//! * *"How many keepers may this moment produce?"* is a question about the moment, and
//!   `Moment::diversity` answers it.
//! * *"Which of these frames is the same instant?"* is a question about the burst.
//!   Six frames of a 10 fps burst differ by 100 ms; at most one of them is a
//!   photograph, and the others are the same photograph with the eyes at a different
//!   point in a blink.
//!
//! A moment with three bursts of five is a photographer who tried three times. A moment
//! with one burst of fifteen is a photographer who tried once and held the button. Those
//! are different situations and a culling engine that could not tell them apart would
//! deliver three near-identical frames from the first and none of the alternatives from
//! the second.
//!
//! ## How a burst is recognised
//!
//! In the order the evidence is trustworthy, which is [`super::cadence::Cadence::drive_pair`]'s
//! order:
//!
//! 1. the camera recorded a continuous release;
//! 2. two frames share a whole second and differ in the sub-second field;
//! 3. the gap is under [`super::cadence::DRIVE_INTERVAL_MS`];
//! 4. the gap is under the local burst window *and* the two frames are visually very
//!    close - which is the inferred case, and the only one that uses a threshold.
//!
//! Bursts are **per camera** and always contiguous in time. Two photographers cannot
//! share a burst: a burst is a fact about one shutter.
//!
//! ## The partition rule
//!
//! Every frame of a moment is in exactly one burst, and a frame with no close enough
//! neighbour is a burst of one. `Moment::bursts` is therefore a partition of
//! `Moment::image_ids`, which is what lets a caller iterate bursts without silently
//! dropping frames - and what makes `burst_count` a number a photographer can reconcile
//! with the stack they are looking at.

use aura_core::contract::moment::ImageId;

use crate::moments::cadence::Cadence;
use crate::moments::graph::Frame;

/// How visually close two frames must be to be inferred as one burst.
///
/// A cosine distance, so smaller is closer. 0.08 is roughly "the same composition with
/// something small moving in it" - a blink, a hand, a step - and is deliberately far
/// tighter than the moment's own edge threshold, because a burst is a stronger claim
/// than a moment.
///
/// Only reached by evidence path 4; the three above it do not consult appearance at
/// all, because the camera already answered the question.
pub const BURST_EMBED_MAX: f32 = 0.08;

/// One burst, as indices into the moment's frame list.
pub type Burst = Vec<usize>;

/// Partition one moment's frames into bursts.
///
/// `members` are indices into `frames`, in timeline order - which is how
/// [`super::graph::group`] returns them.
///
/// Contiguity is enforced per camera rather than globally: with two shooters
/// interleaved, camera 1's frames are at positions 0, 2, 4 and camera 2's at 1, 3, 5,
/// and a run that required adjacency in the merged list would find no bursts at all.
/// This is the frame-level half of the two-shooter case section 10.1 gates.
#[must_use]
pub fn partition(frames: &[Frame], members: &[usize], cadence: &Cadence) -> Vec<Burst> {
    if members.is_empty() {
        return Vec::new();
    }

    // Per camera, in the order the cameras first appear, so the output order is a
    // function of the wedding rather than of a hash map's iteration order.
    let mut cameras: Vec<u32> = Vec::new();
    for index in members {
        if let Some(frame) = frames.get(*index) {
            if !cameras.contains(&frame.camera) {
                cameras.push(frame.camera);
            }
        }
    }

    let mut bursts: Vec<Burst> = Vec::new();
    for camera in cameras {
        let run: Vec<usize> = members
            .iter()
            .copied()
            .filter(|index| {
                frames
                    .get(*index)
                    .is_some_and(|frame| frame.camera == camera)
            })
            .collect();
        bursts.extend(partition_one_camera(frames, &run, cadence));
    }

    // Ordered by their first frame, so bursts read in timeline order even when two
    // cameras interleave.
    bursts.sort_by_key(|burst| burst.first().copied().unwrap_or(usize::MAX));
    bursts
}

/// Partition one camera's contiguous run.
fn partition_one_camera(frames: &[Frame], run: &[usize], cadence: &Cadence) -> Vec<Burst> {
    let mut out: Vec<Burst> = Vec::new();
    let mut current: Burst = Vec::new();

    for index in run {
        let Some(frame) = frames.get(*index) else {
            continue;
        };
        let joins = current
            .last()
            .and_then(|last| frames.get(*last))
            .is_some_and(|previous| is_one_burst(previous, frame, cadence));
        if joins {
            current.push(*index);
        } else {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            current.push(*index);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// True when two consecutive frames of one body are one press of the shutter.
///
/// The four evidence paths in this module's header, in order. Public because the eval
/// harness measures the fourth path's threshold against the burst fixtures.
#[must_use]
pub fn is_one_burst(previous: &Frame, next: &Frame, cadence: &Cadence) -> bool {
    if previous.camera != next.camera {
        return false;
    }
    // Paths 1 to 3: the camera said so, directly or by arithmetic no photographer can
    // beat. No appearance test, because appearance cannot contradict the shutter.
    if Cadence::drive_pair(&previous.shot(), &next.shot()) {
        return true;
    }

    // Path 4: inferred. Both conditions, and both are strict.
    let gap = next.ts - previous.ts;
    let window = cadence.window_ms(previous.camera, previous.ts);
    if gap > window {
        return false;
    }
    let distance =
        aura_index::contract::index::cosine_distance(&previous.embedding, &next.embedding);
    distance <= BURST_EMBED_MAX
}

/// The frames of each burst, as photograph ids.
///
/// The shape `Moment::bursts` is frozen as. Kept beside [`partition`] rather than in
/// the caller so that the partition property - every frame in exactly one burst - is
/// provable in one place.
#[must_use]
pub fn to_image_ids(frames: &[Frame], bursts: &[Burst]) -> Vec<Vec<ImageId>> {
    bursts
        .iter()
        .map(|burst| {
            burst
                .iter()
                .filter_map(|index| frames.get(*index).map(|frame| frame.image))
                .collect()
        })
        .collect()
}
