//! Per-precision latency and throughput for the runtime.
//!
//! Four numbers, each one budgeted in section 11 of the phase document:
//!
//! * **cold load** - parse, compile and round weights, with nothing cached;
//! * **single inference** - one image, the interactive path;
//! * **batch throughput** - eight images at once, the analysis path;
//! * **admission overhead** - the scheduler's own cost, budgeted at 0.4 ms.
//!
//! The models are the placeholders from `onnx::fixtures`, so these numbers say
//! how fast the *runtime* is, not how fast phase 05's models will be. The exit
//! report says so wherever it quotes one.

use std::sync::Arc;

use aura_core::clock::{Clock, SystemClock};
use aura_core::Priority;
use aura_infer::backend::{Backend, ReferenceBackend};
use aura_infer::batch::{BatchScheduler, CancelToken, MemoryLedger};
use aura_infer::contract::infer::{ModelClass, Precision};
use aura_infer::onnx::{fixtures, serialise};
use aura_infer::source::ResolvedModel;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};

fn write_model(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write model fixture");
    path
}

fn resolved(path: std::path::PathBuf, precision: Precision) -> ResolvedModel {
    ResolvedModel {
        path,
        precision,
        class: ModelClass::Embedding,
        working_set_mb: 1,
    }
}

fn decode_benchmarks(c: &mut Criterion) {
    let dir = tempfile::tempdir().expect("tempdir");
    let embedding = write_model(
        dir.path(),
        "embedding.onnx",
        &serialise(&fixtures::tiny_embedding()),
    );
    let backend = ReferenceBackend::new();

    let mut load = c.benchmark_group("cold_load");
    for precision in [Precision::Fp32, Precision::Fp16, Precision::Int8] {
        let model = resolved(embedding.clone(), precision);
        load.bench_function(precision.as_str(), |b| {
            b.iter(|| backend.load(&model).expect("load"));
        });
    }
    load.finish();

    let mut single = c.benchmark_group("single_inference");
    let input = fixtures::sample_input(1);
    for precision in [Precision::Fp32, Precision::Fp16, Precision::Int8] {
        let session = backend
            .load(&resolved(embedding.clone(), precision))
            .expect("load");
        single.throughput(Throughput::Elements(1));
        single.bench_function(precision.as_str(), |b| {
            b.iter(|| session.run(&[input.view()]).expect("run"));
        });
    }
    single.finish();

    let mut batched = c.benchmark_group("batch_of_eight");
    let batch_input = fixtures::sample_input(8);
    for precision in [Precision::Fp32, Precision::Int8] {
        let session = backend
            .load(&resolved(embedding.clone(), precision))
            .expect("load");
        batched.throughput(Throughput::Elements(8));
        batched.bench_function(precision.as_str(), |b| {
            b.iter(|| session.run(&[batch_input.view()]).expect("run"));
        });
    }
    batched.finish();

    let scene = write_model(
        dir.path(),
        "scene.onnx",
        &serialise(&fixtures::tiny_scene()),
    );
    let session = backend
        .load(&resolved(scene, Precision::Fp32))
        .expect("load");
    c.bench_function("scene_classifier_single", |b| {
        b.iter(|| session.run(&[input.view()]).expect("run"));
    });
}

fn scheduler_benchmarks(c: &mut Criterion) {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let scheduler = BatchScheduler::new(
        Arc::clone(&clock),
        Arc::new(MemoryLedger::new(4 * 1024 * 1024 * 1024)),
    );
    let cancel = CancelToken::new();

    c.bench_function("admission_overhead", |b| {
        b.iter_batched(
            || (),
            |()| {
                scheduler
                    .admit(Priority::AiBatch, 1024, "bench", 1, &cancel, || Ok(()))
                    .expect("admit")
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, decode_benchmarks, scheduler_benchmarks);
criterion_main!(benches);
