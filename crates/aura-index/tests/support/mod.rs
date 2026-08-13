#![allow(dead_code)]

//! Synthetic vectors with known structure, so a test can assert what the index
//! *should* have found rather than only that it found something.
//!
//! Section 8 step 1 of the phase document asks for the table and the graph to be
//! landed "with synthetic vectors", before a model exists. That ordering is what
//! makes these tests meaningful: they test the index, not the embedding.

use std::collections::BTreeMap;

use aura_index::contract::index::{ImageEmbedding, ImageId, LumaStats, EMBED_DIM, F16};
use aura_index::hnsw::{EntryMeta, IndexEntry};
use aura_index::query::normalise;
use aura_index::CameraId;
use uuid::Uuid;

/// A deterministic generator, seeded per corpus. Not `rand`: a corpus whose bytes
/// changed when a dependency updated would make every recall figure in the exit
/// report unreproducible.
#[derive(Debug)]
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub fn next_bits(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Uniform in `-1..1`.
    pub fn unit(&mut self) -> f32 {
        let bits = (self.next_bits() >> 40) as f32 / 16_777_216.0;
        bits * 2.0 - 1.0
    }
}

/// An id derived from an index, so a failure names a reproducible photograph.
#[must_use]
pub fn id_for(n: u64) -> ImageId {
    let mut bytes = [0u8; 16];
    if let Some(tail) = bytes.get_mut(8..16) {
        tail.copy_from_slice(&n.to_be_bytes());
    }
    if let Some(head) = bytes.get_mut(..8) {
        head.copy_from_slice(&0x0555_0000_0000_0001u64.to_be_bytes());
    }
    ImageId::from_uuid(Uuid::from_bytes(bytes))
}

/// One embedding from an explicit direction, L2-normalised and quantised the way
/// the runner would have done it.
#[must_use]
pub fn embedding_from(id: ImageId, mut vector: Vec<f32>, dhash: u64) -> ImageEmbedding {
    vector.resize(EMBED_DIM, 0.0);
    normalise(&mut vector);
    let mut out = ImageEmbedding::zeroed(id, 100);
    for (slot, value) in out.vec.iter_mut().zip(&vector) {
        *slot = F16::from_f32(*value);
    }
    out.dhash = dhash;
    out.luma = LumaStats {
        mean: 0.5,
        p1: 0.02,
        p50: 0.48,
        p99: 0.97,
        clip_lo: 0.0,
        clip_hi: 0.0,
    };
    out
}

/// A corpus with real cluster structure: `clusters` groups, `per_cluster` frames
/// each, every frame a cluster centre plus a small perturbation.
///
/// The perturbation is what makes recall a meaningful measurement. A corpus of
/// pure random directions in 512 dimensions is nearly equidistant everywhere -
/// every method scores the same on it, including a broken one.
///
/// `spread` is per component, and the scale it must be compared against is a
/// component of a unit 512-d vector: about 0.044. A spread of 0.02 perturbs a frame
/// by roughly a quarter of its centre and gives tight clusters; a spread of 0.3
/// drowns the centre entirely and leaves a corpus with no structure in it at all -
/// on which every metric still returns a plausible number, which is why the figure
/// is spelled out here.
#[must_use]
pub fn clustered(
    clusters: usize,
    per_cluster: usize,
    spread: f32,
) -> (Vec<IndexEntry>, BTreeMap<ImageId, u32>) {
    let mut rng = Rng::new(0x0505_1234_5678_9abc);
    let mut centres = Vec::with_capacity(clusters);
    for _ in 0..clusters {
        let mut centre: Vec<f32> = (0..EMBED_DIM).map(|_| rng.unit()).collect();
        normalise(&mut centre);
        centres.push(centre);
    }

    let mut entries = Vec::with_capacity(clusters * per_cluster);
    let mut truth = BTreeMap::new();
    let mut serial = 0u64;
    for (cluster, centre) in centres.iter().enumerate() {
        for member in 0..per_cluster {
            serial += 1;
            let id = id_for(serial);
            let vector: Vec<f32> = centre
                .iter()
                .map(|value| value + rng.unit() * spread)
                .collect();
            // A dHash that is identical inside a cluster for the first two frames
            // and distinct otherwise, so the near-duplicate tests have something
            // truthful to find.
            let dhash = if member < 2 {
                0xA5A5_0000_0000_0000u64 | cluster as u64
            } else {
                rng.next_bits()
            };
            entries.push(IndexEntry {
                embedding: embedding_from(id, vector, dhash),
                meta: EntryMeta {
                    // One frame per second inside a cluster, clusters an hour apart.
                    timeline_ms: Some(
                        1_700_000_000_000i64 + cluster as i64 * 3_600_000 + member as i64 * 1_000,
                    ),
                    camera: Some(CameraId((member % 2) as u32)),
                    scene: None,
                },
            });
            truth.insert(id, cluster as u32);
        }
    }
    (entries, truth)
}

/// The plain embeddings of a corpus, for the brute-force comparison.
#[must_use]
pub fn embeddings_of(entries: &[IndexEntry]) -> Vec<ImageEmbedding> {
    entries
        .iter()
        .map(|entry| entry.embedding.clone())
        .collect()
}
