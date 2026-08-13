#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    missing_debug_implementations,
    unreachable_pub,
    rust_2018_idioms
)]
#![warn(clippy::pedantic)]
#![allow(
    clippy::module_name_repetitions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

//! One vector substrate, asked the same question by seven later phases.
//!
//! Almost every wedding-intelligence question is a similarity question. "Which
//! frames are this burst?", "which of these forty ceremony shots is the one?",
//! "has this moment already been covered?", "do these two cameras agree about
//! what the room looked like?" - all of them are `knn` with a different filter.
//! Computing one good embedding per image and reusing it everywhere is the
//! difference between an eight-minute pipeline and an hour-long one.
//!
//! This crate owns everything after the vector exists:
//!
//! * [`store`] persists vectors and descriptors as catalog rows, transactionally
//!   with the photographs they describe;
//! * [`hnsw`] is the graph - `M = 32`, `ef_construction = 200`, `ef_search = 64`,
//!   cosine distance on L2-normalised vectors, built in parallel at project open;
//! * [`snapshot`] writes that graph to a self-healing cache file so the second
//!   open is instant and a corrupt file costs a rebuild rather than a crash;
//! * [`query`] is the filtered API: time windows as a pre-filter over a sorted
//!   timeline, camera and scene restriction, exclusion sets, medoids;
//! * [`metrics`] is the brute-force truth the graph is measured against, plus the
//!   retrieval, purity and duplicate-detection maths the eval gates assert.
//!
//! **Determinism is the design constraint, not a nice property.** HNSW is
//! normally built with a random level per node, which means two runs produce two
//! graphs and clustering built on top of it is irreproducible. Here the level
//! comes from `blake3(image_id)`, every tie is broken by `timeline_ts` then
//! `image_id`, neighbour lists are sorted before they are stored, and distances
//! accumulate in a fixed order. Two machines return the same neighbours in the
//! same order, so `tests/eval/embedding_eval.rs` can assert equality rather than
//! a tolerance. See `docs/adr/ADR-0011-embeddings-and-similarity-index.md`.

pub mod errors;
pub mod hnsw;
pub mod metrics;
pub mod query;
pub mod snapshot;
pub mod store;

/// Frozen contracts. Changing anything in here requires an ADR.
pub mod contract {
    pub mod index;
}

pub use contract::index::{
    cosine_distance, CameraId, ImageEmbedding, ImageId, IndexFilter, IndexStats, LumaStats,
    SceneId, SimilarityIndex, EMBED_DIM, F16, HSV_BINS,
};
pub use hnsw::{EntryMeta, HnswIndex, HnswParams, IndexEntry};
pub use query::{Neighbour, SimilarQuery};
pub use snapshot::{Snapshot, SnapshotHeader};
pub use store::EmbeddingStore;
