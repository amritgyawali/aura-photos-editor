//! The difference hash, and why the expensive index is never asked a trivial
//! question.
//!
//! Section 6.2: "dHash catches exact and near-exact duplicates in O(1) with
//! Hamming distance, so the expensive index is never asked trivial questions."
//! A wedding contains hundreds of frames that differ by nothing a 512-d vector
//! can see - the same pose, a third of a stop apart, half a second later - and
//! asking a graph search about them costs a thousand times more than an exclusive
//! or and a pop count.
//!
//! The construction is the standard one and is written out rather than imported
//! because it has to be bit-stable forever: a stored hash whose meaning changed
//! with a dependency update would silently reclassify every duplicate in every
//! catalog.
//!
//! Greyscale, box-resized to nine by eight, then each pixel compared with the one
//! to its right. Sixty-four comparisons, sixty-four bits, most significant bit
//! first in row-major order.

/// Width of the reduced image. One wider than the height, so that eight
/// horizontal comparisons fit in each of eight rows.
pub const DHASH_WIDTH: usize = 9;

/// Height of the reduced image.
pub const DHASH_HEIGHT: usize = 8;

/// How much brighter one cell must be than its neighbour for the bit to be set.
///
/// A textbook difference hash uses a strict comparison. That is a mistake on real
/// data, and the reason is arithmetic rather than photographic: the box resize
/// averages a different *number* of source pixels into each column when the width
/// does not divide by nine, so a perfectly flat frame produces means that differ in
/// their last few bits and hashes to a pattern of rounding noise instead of to
/// zero.
///
/// The noise is deterministic, so duplicate detection still worked - but a hash
/// whose bits are mostly arithmetic artefacts spends its 64 bits of budget on
/// nothing. One part in ten thousand is two orders of magnitude below the smallest
/// difference an 8-bit frame can express between two averaged regions, and two
/// orders of magnitude above the accumulated rounding error of averaging 45
/// samples.
const FLAT_TOLERANCE: f32 = 1e-4;

/// Compute the 64-bit difference hash of a luminance plane.
///
/// `luma` is row-major, `width * height` values in `0..1`. A plane smaller than
/// the reduction target still works - the box resize degenerates to nearest
/// neighbour - which matters because a camera's embedded thumbnail can be tiny.
///
/// Returns zero for an empty plane. Zero is also a legitimate hash for a
/// perfectly flat image, which is why a caller checks the pixel count rather than
/// the hash when it wants to know whether anything was measured.
#[must_use]
pub fn dhash(luma: &[f32], width: usize, height: usize) -> u64 {
    if width == 0 || height == 0 || luma.is_empty() {
        return 0;
    }
    let reduced =
        crate::embed::descriptors::box_resize(luma, width, height, DHASH_WIDTH, DHASH_HEIGHT);

    let mut bits = 0u64;
    let mut position = 0u32;
    for row in 0..DHASH_HEIGHT {
        for column in 0..DHASH_WIDTH - 1 {
            let left = reduced
                .get(row * DHASH_WIDTH + column)
                .copied()
                .unwrap_or(0.0);
            let right = reduced
                .get(row * DHASH_WIDTH + column + 1)
                .copied()
                .unwrap_or(0.0);
            if left > right + FLAT_TOLERANCE {
                bits |= 1u64 << (63 - position);
            }
            position += 1;
        }
    }
    bits
}

/// Hamming distance between two difference hashes, `0..=64`.
///
/// The whole point of the hash: this is two instructions.
#[must_use]
pub const fn hamming(left: u64, right: u64) -> u32 {
    (left ^ right).count_ones()
}

/// True when two frames are the same photograph as far as the hash can tell.
///
/// The threshold is the reporting boundary documented in
/// `aura_index::query::NEAR_DUPLICATE_HAMMING`, not a policy: phase 08 owns the
/// duplicate decision and sets its own number against labelled ground truth.
#[must_use]
pub fn is_near_duplicate(left: u64, right: u64) -> bool {
    hamming(left, right) <= aura_index::query::NEAR_DUPLICATE_HAMMING
}
