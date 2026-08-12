//! Phase 03 budgets, asserted against a real runtime rather than against hope.
//!
//! The models here are the placeholders, so these numbers prove the *shape* of
//! the cost - that loading is a parse rather than something accidentally
//! quadratic, that admission is a few atomic operations rather than a lock
//! convoy - and they catch a regression the moment one lands. They do not
//! predict what a real backbone will cost, and `docs/progress/PHASE-03-EXIT.md`
//! says so rather than implying otherwise.
//!
//! The two GPU throughput budgets from section 11 are absent on purpose: this
//! build has no GPU backend, and asserting a number nothing can measure would be
//! a green test that means nothing. They are waived in ADR-0007.

use std::path::Path;
use std::sync::Arc;

use aura_core::clock::{Clock, SystemClock};
use aura_core::Priority;
use aura_infer::backend::{Backend, ReferenceBackend};
use aura_infer::batch::{BatchScheduler, CancelToken, MemoryLedger};
use aura_infer::contract::infer::{
    HardwarePlan, InferService, ModelClass, ModelRef, Precision, Tensor, TensorView, Version,
};
use aura_infer::ep::BackendRegistry;
use aura_infer::onnx::{fixtures, serialise};
use aura_infer::plan::PlanPaths;
use aura_infer::probe::probe;
use aura_infer::service::InferEngine;
use aura_infer::source::{ResolvedModel, StaticModelSource};
use aura_perf::{Budgets, Measurement, StageTimer};

const EMBEDDING: ModelRef = ModelRef::new("aura_tiny_embedding", Version::new(1, 0, 0));
const SCENE: ModelRef = ModelRef::new("aura_tiny_scene", Version::new(1, 0, 0));

fn budgets() -> Budgets {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../perf/budgets.toml");
    Budgets::load(&path).expect("perf/budgets.toml must parse")
}

fn clock() -> Arc<dyn Clock> {
    Arc::new(SystemClock::default())
}

struct Models {
    directory: tempfile::TempDir,
}

impl Models {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory");
        std::fs::write(
            directory.path().join("embedding.onnx"),
            serialise(&fixtures::tiny_embedding()),
        )
        .expect("write embedding");
        std::fs::write(
            directory.path().join("scene.onnx"),
            serialise(&fixtures::tiny_scene()),
        )
        .expect("write scene");
        Self { directory }
    }

    fn resolved(&self, name: &str, precision: Precision) -> ResolvedModel {
        ResolvedModel {
            path: self.directory.path().join(name),
            precision,
            class: ModelClass::Embedding,
            working_set_mb: 8,
        }
    }

    fn source(&self) -> Arc<StaticModelSource> {
        Arc::new(
            StaticModelSource::new()
                .with(
                    EMBEDDING,
                    &self.directory.path().join("embedding.onnx"),
                    Precision::Fp32,
                    ModelClass::Embedding,
                    8,
                )
                .with(
                    SCENE,
                    &self.directory.path().join("scene.onnx"),
                    Precision::Fp32,
                    ModelClass::Embedding,
                    8,
                ),
        )
    }
}

#[test]
fn every_phase_03_budget_is_declared() {
    let budgets = budgets();
    for stage in [
        "cold_model_load",
        "infer_admission_1000",
        "infer_embedding_cpu_int8",
        "hardware_probe",
        "warmup_startup_models",
    ] {
        assert!(
            budgets.stage.contains_key(stage),
            "{stage} is missing from perf/budgets.toml"
        );
    }
}

#[test]
fn a_cold_model_load_is_inside_its_budget() {
    let models = Models::new();
    let clock = clock();
    let backend = ReferenceBackend::new();
    let model = models.resolved("embedding.onnx", Precision::Fp32);

    let timer = StageTimer::start("cold_model_load", clock.as_ref());
    let session = backend.load(&model).expect("load");
    let measurement = timer.finish(1);

    assert!(session.working_set_bytes() > 0);
    budgets()
        .check(&measurement)
        .expect("cold load must stay inside its budget");
}

#[test]
fn admission_overhead_is_inside_its_budget() {
    let clock = clock();
    let scheduler = BatchScheduler::new(
        Arc::clone(&clock),
        Arc::new(MemoryLedger::new(1024 * 1024 * 1024)),
    );
    let cancel = CancelToken::new();

    let timer = StageTimer::start("infer_admission_1000", clock.as_ref());
    for _ in 0..1000 {
        scheduler
            .admit(Priority::AiBatch, 4096, "budget", 1, &cancel, || Ok(()))
            .expect("admit");
    }
    let measurement = timer.finish(1000);

    budgets()
        .check(&measurement)
        .expect("admission must stay inside its budget");
}

#[test]
fn the_quantised_processor_path_meets_its_throughput_budget() {
    let models = Models::new();
    let clock = clock();
    let backend = ReferenceBackend::new();
    let session = backend
        .load(&models.resolved("embedding.onnx", Precision::Int8))
        .expect("load");

    let images = 16usize;
    let input = fixtures::sample_input(1);
    let timer = StageTimer::start("infer_embedding_cpu_int8", clock.as_ref());
    for _ in 0..images {
        session.run(&[input.view()]).expect("run");
    }
    let measurement: Measurement = timer.finish(images as u64);

    budgets()
        .check(&measurement)
        .expect("the processor path must meet 18 images per second");
}

#[test]
fn the_hardware_probe_is_inside_its_ceiling() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let clock = clock();

    let timer = StageTimer::start("hardware_probe", clock.as_ref());
    let outcome = probe(
        &BackendRegistry::with_reference(),
        &PlanPaths::new(directory.path()),
        &clock,
    )
    .expect("probe");
    let measurement = timer.finish(1);

    assert!(outcome.plan.probed);
    budgets()
        .check(&measurement)
        .expect("the probe must finish inside 15 seconds");
}

#[test]
fn warming_the_startup_models_is_inside_its_budget() {
    let models = Models::new();
    let clock = clock();
    let engine = InferEngine::new(
        BackendRegistry::with_reference(),
        models.source(),
        HardwarePlan::conservative(4, "2026-08-12T00:00:00Z".to_string()),
        Arc::clone(&clock),
    );

    let timer = StageTimer::start("warmup_startup_models", clock.as_ref());
    engine.warmup(&[EMBEDDING, SCENE]).expect("warmup");
    let measurement = timer.finish(2);

    budgets()
        .check(&measurement)
        .expect("warmup must stay inside its budget");
}

#[test]
fn the_memory_ledger_never_overshoots_its_budget() {
    let models = Models::new();
    let clock = clock();
    let mut plan = HardwarePlan::conservative(4, "2026-08-12T00:00:00Z".to_string());
    plan.vram_budget_mb = 32;
    plan.default_batch.embedding = 8;

    let engine = InferEngine::new(
        BackendRegistry::with_reference(),
        models.source(),
        plan,
        Arc::clone(&clock),
    );

    let images: Vec<Tensor> = (0..24).map(|_| fixtures::sample_input(1)).collect();
    let batch: Vec<Vec<TensorView<'_>>> = images.iter().map(|image| vec![image.view()]).collect();
    let results = engine
        .run_batch(EMBEDDING, batch, Priority::AiBatch)
        .expect("the job must finish");

    assert_eq!(results.len(), 24);
    assert!(
        engine.peak_memory_bytes() <= 32 * 1024 * 1024,
        "peak {} bytes exceeded a 32 MB budget; section 11 allows zero overshoots",
        engine.peak_memory_bytes()
    );
}
