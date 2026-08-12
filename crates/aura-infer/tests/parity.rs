//! Cross-precision and cross-provider parity, and determinism.
//!
//! Acceptance criterion 1 of this phase: "the same model produces equivalent
//! results (within tolerance) on TensorRT, CUDA, DirectML, CoreML and CPU". On a
//! build with one real provider that sentence has to be earned rather than
//! asserted, so it is tested in three parts:
//!
//! * the three precisions the reference backend runs are compared against each
//!   other at the tolerances section 6.4 fixes;
//! * a second provider - a fake claiming CUDA and wrapping the same interpreter -
//!   is compared against the processor path, which is exactly the comparison the
//!   probe makes on a real machine;
//! * a provider whose numbers drift is *caught* by that comparison, because a
//!   parity harness that cannot fail proves nothing.

use std::sync::Arc;

use aura_core::clock::{Clock, FixedClock};
use aura_infer::backend::{Backend, FakeBackend, FakeBehaviour, ReferenceBackend};
use aura_infer::contract::infer::{ExecutionProvider, ModelClass, Precision};
use aura_infer::ep::{assess, reference_output, Verdict, CORRECTNESS_TOLERANCE};
use aura_infer::onnx::{fixtures, serialise};
use aura_infer::source::ResolvedModel;
use aura_infer::tensor::max_abs_diff;
use tempfile::TempDir;
use time::OffsetDateTime;

fn model_on_disk(dir: &TempDir, precision: Precision) -> ResolvedModel {
    let path = dir.path().join("embedding.onnx");
    if !path.exists() {
        std::fs::write(&path, serialise(&fixtures::tiny_embedding())).expect("write");
    }
    ResolvedModel {
        path,
        precision,
        class: ModelClass::Embedding,
        working_set_mb: 1,
    }
}

fn outputs_at(dir: &TempDir, precision: Precision) -> Vec<f32> {
    let backend = ReferenceBackend::new();
    let session = backend.load(&model_on_disk(dir, precision)).expect("load");
    let input = fixtures::sample_input(4);
    let outputs = session.run(&[input.view()]).expect("run");
    outputs.first().map(|t| t.data.clone()).unwrap_or_default()
}

#[test]
fn half_precision_stays_inside_its_tolerance_against_full_precision() {
    let dir = TempDir::new().expect("tempdir");
    let reference = outputs_at(&dir, Precision::Fp32);
    let half = outputs_at(&dir, Precision::Fp16);

    let worst = max_abs_diff(&reference, &half).expect("same length");
    assert!(
        worst <= Precision::Fp16.parity_tolerance(),
        "fp16 drifted by {worst}, tolerance {}",
        Precision::Fp16.parity_tolerance()
    );
    assert!(worst > 0.0, "fp16 must be genuinely different arithmetic");
}

#[test]
fn int8_stays_inside_its_wider_tolerance() {
    let dir = TempDir::new().expect("tempdir");
    let reference = outputs_at(&dir, Precision::Fp32);
    let quantised = outputs_at(&dir, Precision::Int8);

    let worst = max_abs_diff(&reference, &quantised).expect("same length");
    assert!(
        worst <= Precision::Int8.parity_tolerance(),
        "int8 drifted by {worst}, tolerance {}",
        Precision::Int8.parity_tolerance()
    );
}

#[test]
fn the_same_model_returns_bit_identical_numbers_on_every_run() {
    let dir = TempDir::new().expect("tempdir");
    let first = outputs_at(&dir, Precision::Fp32);
    let second = outputs_at(&dir, Precision::Fp32);
    assert_eq!(
        first, second,
        "two runs of one model must be bit-identical; invariant 4"
    );
}

#[test]
fn a_batch_produces_the_same_numbers_as_the_images_run_one_at_a_time() {
    let dir = TempDir::new().expect("tempdir");
    let backend = ReferenceBackend::new();
    let session = backend
        .load(&model_on_disk(&dir, Precision::Fp32))
        .expect("load");

    let batched_input = fixtures::sample_input(3);
    let batched = session.run(&[batched_input.view()]).expect("run");
    let batched = batched.first().map(|t| t.data.clone()).expect("output");

    let stride = batched.len() / 3;
    for index in 0..3usize {
        // sample_input offsets each image by its position, so image `index` of a
        // batch of three is not image 0 of a batch of one - it is reproduced here.
        let whole = fixtures::sample_input(index + 1);
        let single = session.run(&[whole.view()]).expect("run");
        let single = single.first().map(|t| t.data.clone()).expect("output");
        let last_of_single: Vec<f32> = single.iter().skip(single.len() - stride).copied().collect();
        let from_batch: Vec<f32> = batched
            .iter()
            .skip(index * stride)
            .take(stride)
            .copied()
            .collect();
        assert_eq!(
            from_batch, last_of_single,
            "batching changed the numbers for image {index}"
        );
    }
}

#[test]
fn a_second_provider_that_agrees_is_judged_usable() {
    let dir = TempDir::new().expect("tempdir");
    let clock: Arc<dyn Clock> = FixedClock::at(OffsetDateTime::UNIX_EPOCH);
    let model = model_on_disk(&dir, Precision::Fp32);
    let input = fixtures::sample_input(1);

    let cpu = ReferenceBackend::new();
    let reference = reference_output(&cpu, &model, &input).expect("reference");

    let pretend_gpu = FakeBackend::new(ExecutionProvider::Cuda, FakeBehaviour::Correct);
    let verdict = assess(&pretend_gpu, &model, &input, &reference, clock.as_ref());
    assert!(
        matches!(verdict, Verdict::Usable { .. }),
        "a correct provider must be usable, got {verdict:?}"
    );
}

#[test]
fn a_provider_whose_numbers_drift_is_set_aside_with_the_reason() {
    let dir = TempDir::new().expect("tempdir");
    let clock: Arc<dyn Clock> = FixedClock::at(OffsetDateTime::UNIX_EPOCH);
    let model = model_on_disk(&dir, Precision::Fp32);
    let input = fixtures::sample_input(1);

    let cpu = ReferenceBackend::new();
    let reference = reference_output(&cpu, &model, &input).expect("reference");

    let drifting = FakeBackend::new(
        ExecutionProvider::DirectMl,
        FakeBehaviour::Drift(CORRECTNESS_TOLERANCE * 10.0),
    );
    match assess(&drifting, &model, &input, &reference, clock.as_ref()) {
        Verdict::SetAside { reason } => assert!(
            reason.starts_with("mismatch"),
            "the reason must say what was wrong: {reason}"
        ),
        other => panic!("a drifting provider must be set aside, got {other:?}"),
    }
}

#[test]
fn a_provider_that_crashes_is_set_aside_rather_than_retried() {
    let dir = TempDir::new().expect("tempdir");
    let clock: Arc<dyn Clock> = FixedClock::at(OffsetDateTime::UNIX_EPOCH);
    let model = model_on_disk(&dir, Precision::Fp32);
    let input = fixtures::sample_input(1);

    let cpu = ReferenceBackend::new();
    let reference = reference_output(&cpu, &model, &input).expect("reference");

    let crashing = FakeBackend::new(ExecutionProvider::TensorRt, FakeBehaviour::Crash);
    match assess(&crashing, &model, &input, &reference, clock.as_ref()) {
        Verdict::SetAside { reason } => assert!(reason.contains("crash"), "{reason}"),
        other => panic!("a crashing provider must be set aside, got {other:?}"),
    }
}

#[test]
fn a_drift_smaller_than_the_tolerance_is_accepted() {
    let dir = TempDir::new().expect("tempdir");
    let clock: Arc<dyn Clock> = FixedClock::at(OffsetDateTime::UNIX_EPOCH);
    let model = model_on_disk(&dir, Precision::Fp32);
    let input = fixtures::sample_input(1);

    let cpu = ReferenceBackend::new();
    let reference = reference_output(&cpu, &model, &input).expect("reference");

    let close_enough = FakeBackend::new(
        ExecutionProvider::Cuda,
        FakeBehaviour::Drift(CORRECTNESS_TOLERANCE / 10.0),
    );
    assert!(matches!(
        assess(&close_enough, &model, &input, &reference, clock.as_ref()),
        Verdict::Usable { .. }
    ));
}
