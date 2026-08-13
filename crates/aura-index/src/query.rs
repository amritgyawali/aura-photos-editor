//! The question the rest of the product actually asks.
//!
//! [`SimilarityIndex`] is the frozen minimum: ids and distances. Every caller
//! then wants the same three things on top of it - how alike is that in human
//! terms, is it a near duplicate, and how long did the query take - so they are
//! computed once here instead of five times in five phases.
//!
//! Nothing in this module decides anything. A distance is evidence; whether two
//! frames are "the same shot" is phase 08's policy, whether they are "the same
//! scene" is phase 07's, and both of those phases own their thresholds. The one
//! number here that looks like a threshold, [`NEAR_DUPLICATE_HAMMING`], is a
//! *reporting* boundary for the debug panel and is documented as such.

use crate::contract::index::{ImageEmbedding, ImageId, IndexFilter, SimilarityIndex};
use crate::hnsw::HnswIndex;

/// Hamming distance at or below which the debug panel calls two frames near
/// duplicates.
///
/// Five bits of 64 is the value the perceptual-hash literature uses for "the same
/// photograph, differently compressed or exposed". It is a label in a panel, not
/// a decision: phase 08 owns the duplicate policy and will set its own number
/// against labelled ground truth.
pub const NEAR_DUPLICATE_HAMMING: u32 = 5;

/// One neighbour, with everything a caller would otherwise ask a second question
/// to find out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Neighbour {
    /// The frame.
    pub id: ImageId,
    /// Cosine distance, `0..2`. Zero is identical.
    pub distance: f32,
    /// `1 - distance`, clamped to `0..1`. The number a human reads.
    pub similarity: f32,
    /// Hamming distance between the two difference hashes, `0..=64`.
    pub dhash_distance: u32,
    /// True when `dhash_distance <= NEAR_DUPLICATE_HAMMING`.
    pub near_duplicate: bool,
}

/// A query and its answer, with the cost of asking.
#[derive(Debug, Clone, PartialEq)]
pub struct SimilarQuery {
    /// The frame that was asked about.
    pub query: ImageId,
    /// Neighbours, nearest first.
    pub neighbours: Vec<Neighbour>,
    /// Wall-clock microseconds. Microseconds rather than milliseconds because the
    /// budget is 5 ms and a 5 ms budget measured in whole milliseconds is measured
    /// in units of itself.
    pub elapsed_us: u64,
    /// Stable text for the `index.query` telemetry event.
    pub filter_kind: &'static str,
}

impl SimilarQuery {
    /// Milliseconds, for the budget assertion and the panel.
    #[must_use]
    pub fn elapsed_ms(&self) -> f32 {
        self.elapsed_us as f32 / 1_000.0
    }

    /// Neighbours the difference hash calls near duplicates.
    #[must_use]
    pub fn near_duplicates(&self) -> Vec<Neighbour> {
        self.neighbours
            .iter()
            .copied()
            .filter(|neighbour| neighbour.near_duplicate)
            .collect()
    }
}

/// Ask what looks like this, and package the answer.
///
/// The clock is injected for the same reason it is everywhere else in this
/// workspace: a test that has to wait for real time is flaky on a loaded machine,
/// and reading a wall clock directly is a determinism risk `check-banned.sh`
/// refuses.
#[must_use]
pub fn find_similar(
    index: &HnswIndex,
    query: &ImageEmbedding,
    k: usize,
    filter: IndexFilter<'_>,
    clock: &dyn aura_core::clock::Clock,
) -> SimilarQuery {
    let started = clock.monotonic_us();
    let found = index.knn(query, k, filter);
    let elapsed_us = clock.monotonic_us().saturating_sub(started);

    let neighbours = found
        .into_iter()
        .map(|(id, distance)| {
            let dhash_distance = index
                .dhash_of(&id)
                .map_or(64, |other| (other ^ query.dhash).count_ones());
            Neighbour {
                id,
                distance,
                similarity: (1.0 - distance).clamp(0.0, 1.0),
                dhash_distance,
                near_duplicate: dhash_distance <= NEAR_DUPLICATE_HAMMING,
            }
        })
        .collect();

    SimilarQuery {
        query: query.image_id,
        neighbours,
        elapsed_us,
        filter_kind: filter.kind(),
    }
}

/// The most representative frame of a set.
///
/// Phases 25 and 26 call this the reference frame: the one whose look the rest of
/// a scene or a camera is matched to. It is the medoid rather than the centroid
/// because a centroid is a vector no photograph is, and a photographer cannot be
/// shown one.
#[must_use]
pub fn reference_frame(index: &HnswIndex, ids: &[ImageId]) -> Option<ImageId> {
    index.medoid(ids)
}

/// The centroid of a set, as a plain vector.
///
/// Section 2.1 asks for "centroid/medoid computation for any set". The centroid
/// is what a clustering algorithm iterates on; the medoid is what a human is
/// shown. Both exist, and the difference is documented here so that phase 07 does
/// not have to rediscover it.
#[must_use]
pub fn centroid(index: &HnswIndex, ids: &[ImageId], vectors: &[ImageEmbedding]) -> Vec<f32> {
    let mut sum = vec![0.0f32; crate::contract::index::EMBED_DIM];
    let mut count = 0usize;
    for embedding in vectors
        .iter()
        .filter(|embedding| ids.contains(&embedding.image_id))
    {
        if !index.contains(&embedding.image_id) {
            continue;
        }
        for (slot, value) in sum.iter_mut().zip(embedding.to_f32()) {
            *slot += value;
        }
        count += 1;
    }
    if count == 0 {
        return sum;
    }
    let scale = 1.0 / count as f32;
    for slot in &mut sum {
        *slot *= scale;
    }
    normalise(&mut sum);
    sum
}

/// Scale a vector to unit length in place. A zero vector is left alone: there is
/// no unit direction for "nothing", and dividing by its length would produce
/// 512 values that are not numbers.
pub fn normalise(vector: &mut [f32]) {
    let mut sum = 0.0f32;
    for value in vector.iter() {
        sum += value * value;
    }
    let length = sum.sqrt();
    if length <= f32::EPSILON {
        return;
    }
    let scale = 1.0 / length;
    for value in vector.iter_mut() {
        *value *= scale;
    }
}
