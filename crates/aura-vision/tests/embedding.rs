//! The embedding runner, end to end, through the real inference service.
//!
//! Section 10.1 asks for throughput, determinism, "no drift after app restart",
//! and the refusals. The first three are the same property looked at three ways:
//! the same pixels through the same model produce the same vector, whatever the
//! batch size and whatever ran before.

mod support;

use aura_core::Priority;
use aura_index::contract::index::EMBED_DIM;
use aura_raw::contract::pixels::{PixelData, PixelLevel, PixelSource};
use aura_vision::embed::model::{
    quantise, EmbeddingModel, EMBED_INPUT_SIDE, EMBED_MODEL, MODEL_VER, PREPROCESS_VER,
};
use aura_vision::embed::{descriptors, hash, run_one};
use tempfile::TempDir;

use support::{engine, flat, synthetic};

/// A reproducible photograph id, so a failure names the same frame every run.
fn photo(n: u64) -> aura_core::PhotoId {
    let text = format!("pht_05550000-0000-0001-0000-{n:012x}");
    match aura_core::PhotoId::from_db(&text) {
        Ok(id) => id,
        Err(err) => panic!("{text} is not a photo id: {err}"),
    }
}

#[test]
fn the_model_and_the_preprocessing_agree_about_their_versions() {
    assert_eq!(EMBED_MODEL, "wedding_embedding");
    assert_eq!(MODEL_VER, 100, "1.0.0 is stored as 100");
    assert_eq!(PREPROCESS_VER, 1);
    assert_eq!(EMBED_INPUT_SIDE, 384);
}

#[test]
fn preprocessing_produces_the_tensor_the_manifest_declares() {
    let frame = synthetic(1200, 800, 0.1, 0.7, 40);
    let Some(rgb) = frame.as_srgb8() else {
        panic!("not sRGB");
    };
    let tensor = EmbeddingModel::preprocess(rgb, 1200, 800);
    assert_eq!(tensor.len(), 3 * EMBED_INPUT_SIDE * EMBED_INPUT_SIDE);
    assert!(
        tensor.iter().all(|value| (0.0..=1.0).contains(value)),
        "a sample left the 0..1 range the manifest declares"
    );
    // Not a constant tensor: a preprocessing bug that returned zeros would produce
    // a vector, and the vector would look plausible.
    let first = tensor.first().copied().unwrap_or(0.0);
    assert!(tensor.iter().any(|value| (*value - first).abs() > 0.01));
}

#[test]
fn preprocessing_centre_crops_rather_than_squashing() {
    // A landscape frame and the same content in portrait must agree about the
    // middle. Squashing would make the two look different for a reason that has
    // nothing to do with what is in them.
    let landscape = synthetic(800, 400, 0.0, 0.8, 40);
    let Some(rgb) = landscape.as_srgb8() else {
        panic!("not sRGB");
    };
    let tensor = EmbeddingModel::preprocess(rgb, 800, 400);

    // The crop is the central 400x400 of an 800x400 frame, so the first sampled
    // column comes from x = 200 and not from x = 0.
    let plane = descriptors::luma_plane(rgb, 800, 400);
    let cropped = EmbeddingModel::preprocess_luma(&plane);
    assert_eq!(cropped.len(), EMBED_INPUT_SIDE * EMBED_INPUT_SIDE);

    let corner = tensor.first().copied().unwrap_or(0.0);
    let leftmost_source = f32::from(rgb.first().copied().unwrap_or(0)) / 255.0;
    assert!(
        (corner - leftmost_source).abs() > 0.05,
        "the tensor starts at the frame's left edge, so the crop did not happen"
    );
}

#[test]
fn one_frame_produces_a_normalised_vector_and_every_descriptor() {
    let dir = TempDir::new().expect("tempdir");
    let infer = engine(&dir, 8);
    let model = EmbeddingModel::new(infer);
    model.warmup().expect("warmup");

    let frame = synthetic(900, 600, 0.15, 0.75, 60);
    let analysed = run_one(
        &model,
        photo(1),
        &frame,
        PixelLevel::Thumb(384),
        Priority::Interactive,
    )
    .expect("embed");

    let embedding = analysed.embedding();
    assert_eq!(embedding.image_id, photo(1));
    assert_eq!(embedding.model_ver, MODEL_VER);
    assert!(!embedding.is_degenerate());

    // L2-normalised: the widened vector has unit length.
    let widened = embedding.to_f32();
    assert_eq!(widened.len(), EMBED_DIM);
    let length: f32 = widened
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    assert!(
        (length - 1.0).abs() < 0.01,
        "the vector has length {length}, so it was not normalised"
    );

    // The descriptors rode along on the same decode.
    assert!(embedding.hsv_hist.iter().any(|bin| *bin > 0));
    assert!(embedding.luma.mean > 0.0);
    assert!(analysed.stored.descriptors.edge_energy > 0.0);
    assert_eq!(analysed.stored.preprocess_ver, PREPROCESS_VER);
    assert_eq!(analysed.stored.pixel_tier, 1, "a thumbnail is tier 1");
    assert_eq!(analysed.stored.pixel_source, "embedded");
}

#[test]
fn the_same_pixels_produce_the_same_vector_bit_for_bit() {
    // Invariant 4. The assertion is equality, not a tolerance: the interpreter
    // fixes its accumulation order precisely so this can be an equality.
    let dir = TempDir::new().expect("tempdir");
    let infer = engine(&dir, 4);
    let model = EmbeddingModel::new(infer);
    let frame = synthetic(700, 700, 0.2, 0.6, 50);

    let first = run_one(
        &model,
        photo(2),
        &frame,
        PixelLevel::Thumb(384),
        Priority::AiBatch,
    )
    .expect("first");
    let second = run_one(
        &model,
        photo(2),
        &frame,
        PixelLevel::Thumb(384),
        Priority::AiBatch,
    )
    .expect("second");

    assert_eq!(
        first.embedding().vec,
        second.embedding().vec,
        "two runs of the same frame produced different vectors"
    );
    assert_eq!(first.embedding().dhash, second.embedding().dhash);
    assert_eq!(first.stored.descriptors, second.stored.descriptors);
}

#[test]
fn a_fresh_service_produces_the_same_vector_as_the_one_before_it() {
    // Section 10.1's "no drift after app restart", which is what a second engine
    // over a freshly written model file simulates.
    let first_dir = TempDir::new().expect("tempdir");
    let second_dir = TempDir::new().expect("tempdir");
    let frame = synthetic(512, 512, 0.05, 0.9, 32);

    let first = run_one(
        &EmbeddingModel::new(engine(&first_dir, 8)),
        photo(3),
        &frame,
        PixelLevel::Thumb(384),
        Priority::AiBatch,
    )
    .expect("first");
    let second = run_one(
        &EmbeddingModel::new(engine(&second_dir, 32)),
        photo(3),
        &frame,
        PixelLevel::Thumb(384),
        Priority::AiBatch,
    )
    .expect("second");

    assert_eq!(first.embedding().vec, second.embedding().vec);
}

#[test]
fn a_batch_and_a_sequence_of_single_frames_agree() {
    // The property that makes the batch scheduler safe to change: batching is a
    // performance decision and must not be an arithmetic one.
    let dir = TempDir::new().expect("tempdir");
    let model = EmbeddingModel::new(engine(&dir, 8));

    let frames: Vec<_> = (0..5)
        .map(|n| synthetic(600, 400, n as f32 * 0.08, 0.7, 30 + n * 7))
        .collect();
    let tensors: Vec<Vec<f32>> = frames
        .iter()
        .map(|frame| {
            let Some(rgb) = frame.as_srgb8() else {
                panic!("not sRGB");
            };
            EmbeddingModel::preprocess(rgb, frame.width as usize, frame.height as usize)
        })
        .collect();

    let batched = model
        .run_batch(&tensors, Priority::AiBatch)
        .expect("run the batch");
    assert_eq!(batched.len(), 5);

    for (tensor, from_batch) in tensors.iter().zip(&batched) {
        let alone = model.run_one(tensor, Priority::AiBatch).expect("run one");
        assert_eq!(
            quantise(&alone),
            quantise(from_batch),
            "a frame embedded differently in a batch than on its own"
        );
    }
}

#[test]
fn different_photographs_produce_different_vectors() {
    // The floor under every similarity claim. A model that returned one vector for
    // every input would pass every determinism test in this file.
    let dir = TempDir::new().expect("tempdir");
    let model = EmbeddingModel::new(engine(&dir, 8));

    let left = run_one(
        &model,
        photo(4),
        &synthetic(500, 500, 0.0, 0.4, 20),
        PixelLevel::Thumb(384),
        Priority::AiBatch,
    )
    .expect("left");
    let right = run_one(
        &model,
        photo(5),
        &synthetic(500, 500, 0.5, 0.95, 90),
        PixelLevel::Thumb(384),
        Priority::AiBatch,
    )
    .expect("right");

    assert_ne!(left.embedding().vec, right.embedding().vec);
    let distance = aura_index::contract::index::cosine_distance(
        &left.embedding().to_f32(),
        &right.embedding().to_f32(),
    );
    assert!(
        distance > 1e-4,
        "two unrelated frames landed {distance} apart, which is nothing"
    );
}

#[test]
fn a_buffer_that_is_not_eight_bit_srgb_is_refused() {
    let dir = TempDir::new().expect("tempdir");
    let model = EmbeddingModel::new(engine(&dir, 4));
    let mut frame = synthetic(400, 400, 0.0, 0.8, 40);
    frame.data = PixelData::Linear16(vec![0u16; 400 * 400 * 3]);

    let err = run_one(
        &model,
        photo(6),
        &frame,
        PixelLevel::Proxy2048,
        Priority::AiBatch,
    )
    .expect_err("a linear buffer must be refused, not converted");
    assert_eq!(err.code.0, "AURA-ML-5007");
}

#[test]
fn a_buffer_whose_dimensions_disagree_with_its_payload_is_refused() {
    let dir = TempDir::new().expect("tempdir");
    let model = EmbeddingModel::new(engine(&dir, 4));
    let mut frame = synthetic(64, 64, 0.0, 0.8, 8);
    frame.width = 4_000;

    let err = run_one(
        &model,
        photo(7),
        &frame,
        PixelLevel::Thumb(384),
        Priority::AiBatch,
    )
    .expect_err("refused");
    assert_eq!(err.code.0, "AURA-ML-5007");
}

#[test]
fn a_degenerate_vector_is_refused_with_its_own_code() {
    // `finish` is the guard, so it is tested directly: producing an all-zero model
    // output on demand would mean a fake backend, and the point of the guard is
    // that it does not trust the backend.
    let err = aura_vision::embed::finish(
        photo(8),
        PixelLevel::Thumb(384),
        PixelSource::Embedded,
        &vec![0.0; EMBED_DIM],
        7,
        aura_index::store::Descriptors::empty(1),
        0.0,
    )
    .expect_err("an all-zero vector must be refused");
    assert_eq!(err.code.0, "AURA-ML-5013");
    assert_eq!(err.severity, aura_core::Severity::ItemFailed);
    assert_eq!(err.recovery, aura_core::Recovery::Quarantine);

    let not_a_number = aura_vision::embed::finish(
        photo(9),
        PixelLevel::Thumb(384),
        PixelSource::Embedded,
        &{
            let mut vector = vec![0.1; EMBED_DIM];
            if let Some(slot) = vector.get_mut(3) {
                *slot = f32::NAN;
            }
            vector
        },
        7,
        aura_index::store::Descriptors::empty(1),
        0.0,
    )
    .expect_err("a NaN must be refused");
    assert_eq!(not_a_number.code.0, "AURA-ML-5013");
}

#[test]
fn a_flat_frame_still_produces_a_usable_vector() {
    // The convolution biases keep a uniform frame off the origin, so a mid-grey
    // wall is embedded rather than quarantined. Worth asserting: a wedding contains
    // plenty of nearly featureless frames and none of them should be item failures.
    let dir = TempDir::new().expect("tempdir");
    let model = EmbeddingModel::new(engine(&dir, 4));
    let analysed = run_one(
        &model,
        photo(10),
        &flat(400, 400, 128),
        PixelLevel::Thumb(384),
        Priority::AiBatch,
    )
    .expect("a flat frame is a photograph too");
    assert!(!analysed.embedding().is_degenerate());
    assert_eq!(
        analysed.embedding().dhash,
        0,
        "a flat frame has no structure"
    );
}

#[test]
fn the_hash_and_the_descriptors_do_not_need_the_model() {
    // The fallback in the model card: when the model is unavailable, near-duplicate
    // detection still works, because none of it involves the model.
    let frame = synthetic(600, 400, 0.1, 0.8, 50);
    let Some(rgb) = frame.as_srgb8() else {
        panic!("not sRGB");
    };
    let plane = descriptors::luma_plane(rgb, 600, 400);
    let described = descriptors::describe(rgb, 600, 400, &plane);
    let hashed = hash::dhash(&plane.values, 600, 400);

    assert_ne!(hashed, 0);
    assert!(described.luma.mean > 0.0);
    assert!(described.hsv_hist.iter().any(|bin| *bin > 0));
}
