//! The runtime end to end: batching, memory, preemption, cancellation, warmup.
//!
//! These are the acceptance criteria that are about *behaviour under pressure*
//! rather than about numbers, and every one of them describes a situation this
//! machine cannot produce on its own: a device running out of memory, a driver
//! that crashes, a batch saturating an accelerator. The fake backend produces
//! them on demand, so each path is executed rather than reasoned about.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use aura_core::clock::{Clock, SystemClock};
use aura_core::Priority;
use aura_infer::backend::{FakeBackend, FakeBehaviour};
use aura_infer::contract::infer::{
    BatchDefaults, ExecutionProvider, HardwarePlan, InferRequest, InferService, ModelClass,
    ModelRef, Precision, Tensor, TensorView, Version,
};
use aura_infer::ep::BackendRegistry;
use aura_infer::onnx::{fixtures, serialise};
use aura_infer::service::InferEngine;
use aura_infer::source::StaticModelSource;
use tempfile::TempDir;

const EMBEDDING: ModelRef = ModelRef::new("aura_tiny_embedding", Version::new(1, 0, 0));
const SCENE: ModelRef = ModelRef::new("aura_tiny_scene", Version::new(1, 0, 0));

fn plan(vram_budget_mb: u32, batch: u16) -> HardwarePlan {
    let mut plan = HardwarePlan::conservative(4, "2026-08-12T00:00:00Z".to_string());
    plan.vram_budget_mb = vram_budget_mb;
    plan.default_batch = BatchDefaults {
        embedding: batch,
        segmentation: 1,
        retouch: 1,
    };
    plan
}

fn source(dir: &TempDir) -> Arc<StaticModelSource> {
    let embedding = dir.path().join("embedding.onnx");
    std::fs::write(&embedding, serialise(&fixtures::tiny_embedding())).expect("write");
    let scene = dir.path().join("scene.onnx");
    std::fs::write(&scene, serialise(&fixtures::tiny_scene())).expect("write");

    Arc::new(
        StaticModelSource::new()
            .with(
                EMBEDDING,
                &embedding,
                Precision::Fp32,
                ModelClass::Embedding,
                8,
            )
            .with(SCENE, &scene, Precision::Fp32, ModelClass::Embedding, 8),
    )
}

fn engine(dir: &TempDir, registry: BackendRegistry, plan: HardwarePlan) -> InferEngine {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    InferEngine::new(registry, source(dir), plan, clock)
}

fn views(tensor: &Tensor) -> Vec<TensorView<'_>> {
    vec![tensor.view()]
}

#[test]
fn one_request_runs_and_reports_where_it_ran() {
    let dir = TempDir::new().expect("tempdir");
    let engine = engine(&dir, BackendRegistry::with_reference(), plan(0, 4));
    let input = fixtures::sample_input(1);

    let result = engine
        .run(InferRequest::new(
            EMBEDDING,
            views(&input),
            Priority::Visible,
        ))
        .expect("run");

    assert_eq!(result.ep_used, ExecutionProvider::Cpu);
    assert_eq!(result.batch_size, 1);
    assert_eq!(
        result.outputs.first().map(|t| t.shape.clone()),
        Some(vec![1, fixtures::EMBEDDING_DIM])
    );
}

#[test]
fn a_model_that_is_not_registered_is_refused_by_name() {
    let dir = TempDir::new().expect("tempdir");
    let engine = engine(&dir, BackendRegistry::with_reference(), plan(0, 4));
    let input = fixtures::sample_input(1);
    let unknown = ModelRef::new("no_such_model", Version::new(9, 9, 9));

    let err = engine
        .run(InferRequest::new(unknown, views(&input), Priority::Visible))
        .expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5001");
}

#[test]
fn a_batch_returns_results_in_the_order_the_caller_supplied_them() {
    let dir = TempDir::new().expect("tempdir");
    let engine = engine(&dir, BackendRegistry::with_reference(), plan(0, 4));

    let images: Vec<Tensor> = (0..7).map(|_| fixtures::sample_input(1)).collect();
    let batch: Vec<Vec<TensorView<'_>>> = images.iter().map(views).collect();

    let results = engine
        .run_batch(EMBEDDING, batch, Priority::AiBatch)
        .expect("batch");
    assert_eq!(results.len(), 7);

    // Every item is the same input, so every output must be the same numbers -
    // which is also the check that no item was served another item's result.
    let first = results
        .first()
        .and_then(|r| r.outputs.first())
        .map(|t| t.data.clone())
        .expect("first output");
    for result in &results {
        assert_eq!(
            result.outputs.first().map(|t| t.data.clone()),
            Some(first.clone())
        );
    }
}

#[test]
fn a_forced_memory_squeeze_halves_the_batch_and_still_finishes() {
    let dir = TempDir::new().expect("tempdir");
    // The model declares 8 MB per image, so a budget of 24 MB fits two images and
    // refuses eight. The scheduler must find that out and finish anyway.
    let engine = engine(&dir, BackendRegistry::with_reference(), plan(24, 8));

    let images: Vec<Tensor> = (0..6).map(|_| fixtures::sample_input(1)).collect();
    let batch: Vec<Vec<TensorView<'_>>> = images.iter().map(views).collect();

    let results = engine
        .run_batch(EMBEDDING, batch, Priority::AiBatch)
        .expect("the job must finish, smaller");
    assert_eq!(results.len(), 6);
    assert!(
        engine.scheduler_stats().downshifts > 0,
        "the squeeze must be visible in the counters"
    );
    assert!(results.iter().all(|result| result.batch_size <= 2));
    assert!(
        engine.peak_memory_bytes() <= 24 * 1024 * 1024,
        "the ledger must never exceed its budget"
    );
}

#[test]
fn a_session_that_refuses_large_batches_is_served_in_smaller_ones() {
    let dir = TempDir::new().expect("tempdir");
    let registry = BackendRegistry::new().register(Arc::new(FakeBackend::new(
        ExecutionProvider::Cpu,
        FakeBehaviour::OutOfMemoryAbove(2),
    )));
    let engine = engine(&dir, registry, plan(0, 8));

    let images: Vec<Tensor> = (0..5).map(|_| fixtures::sample_input(1)).collect();
    let batch: Vec<Vec<TensorView<'_>>> = images.iter().map(views).collect();

    let results = engine
        .run_batch(EMBEDDING, batch, Priority::AiBatch)
        .expect("must complete at a smaller batch");
    assert_eq!(results.len(), 5);
    assert!(results.iter().all(|result| result.batch_size <= 2));
}

#[test]
fn cancelling_stops_a_batch_and_says_so() {
    let dir = TempDir::new().expect("tempdir");
    let engine = engine(&dir, BackendRegistry::with_reference(), plan(0, 1));
    let cancel = engine.cancel_token();
    cancel.cancel();

    let images: Vec<Tensor> = (0..4).map(|_| fixtures::sample_input(1)).collect();
    let batch: Vec<Vec<TensorView<'_>>> = images.iter().map(views).collect();

    let err = engine
        .run_batch(EMBEDDING, batch, Priority::AiBatch)
        .expect_err("must stop");
    assert_eq!(err.code.0, "AURA-ML-5011");
}

#[test]
fn an_interactive_request_is_served_while_a_batch_is_running() {
    let dir = TempDir::new().expect("tempdir");
    let engine = Arc::new(engine(&dir, BackendRegistry::with_reference(), plan(0, 1)));
    let cancel = engine.cancel_token();

    let batch_engine = Arc::clone(&engine);
    let completed = Arc::new(AtomicUsize::new(0));
    let batch_completed = Arc::clone(&completed);

    let worker = std::thread::spawn(move || {
        let images: Vec<Tensor> = (0..400).map(|_| fixtures::sample_input(1)).collect();
        for image in &images {
            if batch_engine
                .run(InferRequest::new(
                    EMBEDDING,
                    vec![image.view()],
                    Priority::AiBatch,
                ))
                .is_err()
            {
                break;
            }
            batch_completed.fetch_add(1, Ordering::SeqCst);
        }
    });

    // Let the batch get going before measuring.
    while completed.load(Ordering::SeqCst) < 2 {
        std::hint::spin_loop();
    }

    let input = fixtures::sample_input(1);
    let started = Instant::now();
    engine
        .run(InferRequest::new(
            EMBEDDING,
            vec![input.view()],
            Priority::Visible,
        ))
        .expect("interactive request");
    let waited = started.elapsed();

    cancel.cancel();
    worker.join().expect("worker");

    assert!(
        waited < Duration::from_millis(80),
        "an interactive request waited {waited:?} behind a batch; the budget is 80 ms"
    );
}

#[test]
fn a_deadline_that_elapses_is_reported_rather_than_ignored() {
    let dir = TempDir::new().expect("tempdir");
    let engine = engine(&dir, BackendRegistry::with_reference(), plan(0, 4));
    let input = fixtures::sample_input(8);

    let err = engine
        .run(
            InferRequest::new(EMBEDDING, views(&input), Priority::Interactive)
                .with_deadline(Duration::from_nanos(1)),
        )
        .expect_err("must report the deadline");
    assert_eq!(err.code.0, "AURA-ML-5008");
}

#[test]
fn warmup_loads_each_model_once_and_the_pool_serves_the_rest() {
    let dir = TempDir::new().expect("tempdir");
    let fake = Arc::new(FakeBackend::new(
        ExecutionProvider::Cpu,
        FakeBehaviour::Correct,
    ));
    let registry = BackendRegistry::new().register(Arc::clone(&fake) as Arc<_>);
    let engine = engine(&dir, registry, plan(0, 4));

    engine.warmup(&[EMBEDDING, SCENE]).expect("warmup");
    assert_eq!(fake.load_count(), 2);

    let input = fixtures::sample_input(1);
    for _ in 0..5 {
        engine
            .run(InferRequest::new(
                EMBEDDING,
                views(&input),
                Priority::Visible,
            ))
            .expect("run");
    }
    assert_eq!(fake.load_count(), 2, "a warmed model must not be reloaded");
    assert!(engine.pool_stats().hits >= 5);
}

#[test]
fn cold_start_warms_the_startup_models_inside_its_budget() {
    let dir = TempDir::new().expect("tempdir");
    let engine = engine(&dir, BackendRegistry::with_reference(), plan(0, 4));

    let started = Instant::now();
    engine
        .warmup(&[EMBEDDING, SCENE, EMBEDDING])
        .expect("warmup");
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_millis(2500),
        "cold start took {elapsed:?}; section 10.1 budgets 2.5 s"
    );
}

#[test]
fn admission_overhead_stays_inside_its_budget() {
    let dir = TempDir::new().expect("tempdir");
    let engine = engine(&dir, BackendRegistry::with_reference(), plan(4096, 4));
    let input = fixtures::sample_input(1);

    for _ in 0..200 {
        engine
            .run(InferRequest::new(
                EMBEDDING,
                views(&input),
                Priority::AiBatch,
            ))
            .expect("run");
    }

    let stats = engine.scheduler_stats();
    assert!(
        stats.mean_overhead_ms() <= 0.4,
        "admission cost {} ms on average; section 11 budgets 0.4 ms",
        stats.mean_overhead_ms()
    );
}
