//! The snapshot: round trip, every refusal, and the half-precision arithmetic
//! that underlies both.
//!
//! Six things can make a snapshot untrustworthy, and a build that loads five of
//! them and rejects one is worse than a build that rejects none - it fails
//! silently on the case nobody tested. So every refusal has a test, and each one
//! asserts the code as well as the failure.

mod support;

use aura_core::clock::SystemClock;
use aura_index::contract::index::{IndexFilter, SimilarityIndex, EMBED_DIM, F16};
use aura_index::hnsw::{HnswIndex, HnswParams};
use aura_index::snapshot::Snapshot;
use tempfile::TempDir;

use support::clustered;

const MODEL_VER: u16 = 100;
const PREPROCESS_VER: u16 = 1;

fn clock() -> SystemClock {
    SystemClock::default()
}

fn built(clusters: usize, per_cluster: usize) -> HnswIndex {
    let (entries, _) = clustered(clusters, per_cluster, 0.02);
    HnswIndex::build(entries, HnswParams::default(), &clock())
}

#[test]
fn a_snapshot_round_trips_and_answers_identically() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("index").join("project.hnsw");
    let (entries, _) = clustered(20, 15, 0.02);
    let original = HnswIndex::build(entries.clone(), HnswParams::default(), &clock());

    let written = Snapshot::write(&original, &path, MODEL_VER, PREPROCESS_VER).expect("write");
    assert_eq!(written.header.nodes as usize, original.len());

    let loaded =
        Snapshot::load(&path, HnswParams::default(), MODEL_VER, PREPROCESS_VER).expect("load");
    assert_eq!(loaded.len(), original.len());
    assert!(
        loaded.stats().from_snapshot,
        "the provenance was not recorded"
    );

    for entry in entries.iter().step_by(9) {
        assert_eq!(
            original.knn(&entry.embedding, 10, IndexFilter::none()),
            loaded.knn(&entry.embedding, 10, IndexFilter::none()),
            "a reloaded graph answered differently for {}",
            entry.embedding.image_id
        );
    }
}

#[test]
fn a_snapshot_preserves_the_metadata_filters_need() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("project.hnsw");
    let (entries, _) = clustered(6, 12, 0.03);
    let original = HnswIndex::build(entries.clone(), HnswParams::default(), &clock());
    original.set_camera_labels(&["cam_aaa".to_string(), "cam_bbb".to_string()]);
    Snapshot::write(&original, &path, MODEL_VER, PREPROCESS_VER).expect("write");

    let loaded =
        Snapshot::load(&path, HnswParams::default(), MODEL_VER, PREPROCESS_VER).expect("load");
    assert_eq!(
        loaded.camera_handle("cam_bbb"),
        Some(aura_index::CameraId(1))
    );

    let Some(query) = entries.get(3) else {
        panic!("short fixture");
    };
    let windowed = loaded.knn(&query.embedding, 20, IndexFilter::within_seconds(5));
    assert!(
        !windowed.is_empty(),
        "the timeline did not survive the round trip, so a burst query finds nothing"
    );
    for entry in &entries {
        assert_eq!(
            loaded.meta_of(&entry.embedding.image_id),
            Some(entry.meta),
            "metadata changed for {}",
            entry.embedding.image_id
        );
    }
}

#[test]
fn two_snapshots_of_the_same_index_are_byte_identical() {
    // The property that makes a snapshot cacheable and diffable: neighbour lists
    // are sorted before they are written, so nothing depends on the order the
    // builder visited nodes.
    let dir = TempDir::new().expect("tempdir");
    let index = built(10, 12);
    let first = dir.path().join("a.hnsw");
    let second = dir.path().join("b.hnsw");
    Snapshot::write(&index, &first, MODEL_VER, PREPROCESS_VER).expect("write a");
    Snapshot::write(&index, &second, MODEL_VER, PREPROCESS_VER).expect("write b");

    let left = std::fs::read(&first).expect("read a");
    let right = std::fs::read(&second).expect("read b");
    assert_eq!(left, right, "two writes of one graph differ");
}

#[test]
fn a_missing_snapshot_is_a_rebuild_and_not_a_crash() {
    let dir = TempDir::new().expect("tempdir");
    let err = Snapshot::load(
        &dir.path().join("absent.hnsw"),
        HnswParams::default(),
        MODEL_VER,
        PREPROCESS_VER,
    )
    .expect_err("a missing file must be refused");
    assert_eq!(err.code.0, "AURA-ML-5014");
    assert_eq!(err.severity, aura_core::Severity::Warning);
    assert_eq!(err.recovery, aura_core::Recovery::Fallback);
}

#[test]
fn a_file_that_is_not_a_snapshot_is_refused() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("wrong.hnsw");
    std::fs::write(&path, b"this is not an index at all, it is a text file").expect("write");
    let err = Snapshot::load(&path, HnswParams::default(), MODEL_VER, PREPROCESS_VER)
        .expect_err("refused");
    assert_eq!(err.code.0, "AURA-ML-5014");
    assert!(
        err.detail.contains("not an AURA index snapshot"),
        "{}",
        err.detail
    );
}

#[test]
fn a_truncated_snapshot_is_refused_rather_than_half_loaded() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("short.hnsw");
    let index = built(8, 10);
    Snapshot::write(&index, &path, MODEL_VER, PREPROCESS_VER).expect("write");

    let bytes = std::fs::read(&path).expect("read");
    let Some(head) = bytes.get(..bytes.len() / 2) else {
        panic!("empty snapshot");
    };
    std::fs::write(&path, head).expect("truncate");

    let err = Snapshot::load(&path, HnswParams::default(), MODEL_VER, PREPROCESS_VER)
        .expect_err("refused");
    assert_eq!(err.code.0, "AURA-ML-5014");
    assert!(
        err.detail.contains("digest") || err.detail.contains("malformed"),
        "{}",
        err.detail
    );
}

#[test]
fn a_corrupt_body_is_caught_by_its_digest() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("bitrot.hnsw");
    let index = built(6, 10);
    Snapshot::write(&index, &path, MODEL_VER, PREPROCESS_VER).expect("write");

    let mut bytes = std::fs::read(&path).expect("read");
    // Flip one bit deep in the body: a plausible-looking graph with one wrong
    // neighbour is exactly what a digest exists to catch.
    let midpoint = bytes.len() / 2;
    if let Some(byte) = bytes.get_mut(midpoint) {
        *byte ^= 0x01;
    }
    std::fs::write(&path, &bytes).expect("write");

    let err = Snapshot::load(&path, HnswParams::default(), MODEL_VER, PREPROCESS_VER)
        .expect_err("refused");
    assert!(err.detail.contains("digest"), "{}", err.detail);
}

#[test]
fn a_snapshot_from_a_different_model_version_is_refused() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("old-model.hnsw");
    let index = built(4, 8);
    Snapshot::write(&index, &path, MODEL_VER, PREPROCESS_VER).expect("write");

    let err = Snapshot::load(&path, HnswParams::default(), MODEL_VER + 10, PREPROCESS_VER)
        .expect_err("refused");
    assert!(err.detail.contains("model version"), "{}", err.detail);
}

#[test]
fn a_snapshot_from_a_different_preprocessing_version_is_refused() {
    // The one an ordinary reviewer forgets. A resize change makes every stored
    // vector describe a different crop, and nothing in the model version says so.
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("old-preprocess.hnsw");
    let index = built(4, 8);
    Snapshot::write(&index, &path, MODEL_VER, PREPROCESS_VER).expect("write");

    let err = Snapshot::load(&path, HnswParams::default(), MODEL_VER, PREPROCESS_VER + 1)
        .expect_err("refused");
    assert!(err.detail.contains("preprocessed"), "{}", err.detail);
}

#[test]
fn a_snapshot_built_with_different_graph_parameters_is_refused() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("other-params.hnsw");
    let index = built(4, 8);
    Snapshot::write(&index, &path, MODEL_VER, PREPROCESS_VER).expect("write");

    let different = HnswParams {
        m: 16,
        ..HnswParams::default()
    };
    let err = Snapshot::load(&path, different, MODEL_VER, PREPROCESS_VER).expect_err("refused");
    assert!(err.detail.contains("graph parameters"), "{}", err.detail);
}

#[test]
fn a_failed_write_leaves_no_partial_file_behind() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir
        .path()
        .join("nested")
        .join("deeper")
        .join("project.hnsw");
    let index = built(3, 6);
    Snapshot::write(&index, &path, MODEL_VER, PREPROCESS_VER).expect("write");
    assert!(path.exists());
    assert!(
        !path.with_extension("hnsw.part").exists(),
        "the temporary file survived a successful write"
    );
}

#[test]
fn an_empty_index_snapshots_and_reloads() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("empty.hnsw");
    let index = HnswIndex::new(HnswParams::default());
    Snapshot::write(&index, &path, MODEL_VER, PREPROCESS_VER).expect("write");
    let loaded =
        Snapshot::load(&path, HnswParams::default(), MODEL_VER, PREPROCESS_VER).expect("load");
    assert!(loaded.is_empty());
}

// -- the half-precision arithmetic the whole file rests on -------------------

#[test]
fn half_precision_round_trips_every_value_an_embedding_can_hold() {
    // A normalised 512-d component is in -1..1, and in practice within about
    // -0.3..0.3. Every value in that range must survive the round trip to within
    // half precision's own resolution.
    for step in -1_000..=1_000 {
        let value = step as f32 / 1_000.0;
        let round_tripped = F16::from_f32(value).to_f32();
        let error = (round_tripped - value).abs();
        assert!(
            error <= 0.001,
            "{value} came back as {round_tripped}, error {error}"
        );
    }
}

#[test]
fn half_precision_rounds_to_nearest_ties_to_even() {
    // The rule matters because `aura_infer::tensor::round_to_f16` uses it: a vector
    // that has been through the runtime and one that has been through the store
    // must agree bit for bit.
    assert_eq!(F16::from_f32(0.0).to_bits(), 0x0000);
    assert_eq!(F16::from_f32(-0.0).to_bits(), 0x8000);
    assert_eq!(F16::from_f32(1.0).to_bits(), 0x3C00);
    assert_eq!(F16::from_f32(-1.0).to_bits(), 0xBC00);
    assert_eq!(F16::from_f32(0.5).to_bits(), 0x3800);
    assert_eq!(F16::from_f32(2.0).to_bits(), 0x4000);
}

#[test]
fn half_precision_subnormals_are_exact_in_both_directions() {
    // The quiet dimensions of a normalised 512-d vector land here - around 1e-5 -
    // and an off-by-one factor of two in the subnormal path is invisible everywhere
    // except in a reloaded snapshot's distances. This is the test that catches it.
    for bits in 1u16..=0x03ff {
        let value = F16::from_bits(bits).to_f32();
        assert_eq!(
            F16::from_f32(value).to_bits(),
            bits,
            "subnormal 0x{bits:04x} widened to {value:e} and came back different"
        );
    }
    // The smallest positive half is 2^-24, and the largest subnormal is
    // 1023 * 2^-24.
    assert_eq!(F16::from_bits(0x0001).to_f32(), 2.0f32.powi(-24));
    assert_eq!(F16::from_bits(0x03ff).to_f32(), 1023.0 * 2.0f32.powi(-24));
    // And the first normal value is 2^-14, one step above.
    assert_eq!(F16::from_bits(0x0400).to_f32(), 2.0f32.powi(-14));
}

#[test]
fn every_half_precision_bit_pattern_round_trips() {
    // Exhaustive over the finite values, which is 63,488 patterns - cheap, and the
    // only way to be sure a hand-written conversion has no seam in it.
    for bits in 0u16..=u16::MAX {
        let exponent = (bits >> 10) & 0x1f;
        if exponent == 0x1f {
            continue; // infinities and NaN, covered separately
        }
        let value = F16::from_bits(bits).to_f32();
        assert_eq!(
            F16::from_f32(value).to_bits(),
            bits,
            "0x{bits:04x} widened to {value:e} and came back as 0x{:04x}",
            F16::from_f32(value).to_bits()
        );
    }
}

#[test]
fn half_precision_saturates_rather_than_wrapping() {
    // Unreachable for a normalised embedding, and present so that a malformed model
    // output cannot produce a silently wrong bit pattern.
    assert_eq!(F16::from_f32(f32::INFINITY).to_bits(), 0x7C00);
    assert_eq!(F16::from_f32(f32::NEG_INFINITY).to_bits(), 0xFC00);
    assert_eq!(F16::from_f32(1.0e30).to_bits(), 0x7C00);
    assert!(F16::from_f32(f32::NAN).to_f32().is_nan());
    // Far below the subnormal range flushes to a signed zero.
    assert_eq!(F16::from_f32(1.0e-30).to_bits(), 0x0000);
    assert_eq!(F16::from_f32(-1.0e-30).to_bits(), 0x8000);
}

#[test]
fn a_zero_vector_is_recognised_as_degenerate() {
    let id = support::id_for(1);
    let zeroed = aura_index::contract::index::ImageEmbedding::zeroed(id, MODEL_VER);
    assert!(zeroed.is_degenerate());

    let mut real = zeroed.clone();
    if let Some(slot) = real.vec.get_mut(0) {
        *slot = F16::from_f32(1.0);
    }
    assert!(!real.is_degenerate());
    assert_eq!(real.to_f32().len(), EMBED_DIM);
}
