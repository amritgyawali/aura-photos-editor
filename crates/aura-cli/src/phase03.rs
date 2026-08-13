//! The phase 03 gate, and the small `infer` command the parity scripts drive.
//!
//! `aura-cli verify --phase 03` proves, on this machine, in one run, everything
//! section 13 of the phase document asks for that can be proved without a second
//! machine: the model set verifies, the hardware probe finishes inside its
//! ceiling, both models warm up, the precisions agree inside their tolerances, a
//! memory squeeze degrades the batch instead of crashing, a cancelled run stops,
//! and a model that fails its first use is rolled back.
//!
//! It is deliberately a separate binary path from the tests. The tests prove the
//! pieces; the gate proves the assembly, with the same code CI runs and a human
//! reads the output of.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use aura_core::clock::{Clock, SystemClock};
use aura_core::Priority;
use aura_infer::backend::{FakeBackend, FakeBehaviour};
use aura_infer::contract::infer::{
    ExecutionProvider, InferService, ModelRef, Precision, Tensor, TensorView, Version,
};
use aura_infer::ep::BackendRegistry;
use aura_infer::onnx::fixtures;
use aura_infer::plan::{self, PlanPaths};
use aura_infer::probe::probe;
use aura_infer::service::InferEngine;
use aura_infer::source::ModelSource;
use aura_models::download::FileTransport;
use aura_models::registry::{trusted_public_key, ModelRegistry};

/// The models the gate exercises. Both are placeholders; see ADR-0007.
const EMBEDDING: ModelRef = ModelRef::new("aura_tiny_embedding", Version::new(1, 0, 0));
const SCENE: ModelRef = ModelRef::new("aura_tiny_scene", Version::new(1, 0, 0));

/// How many images the throughput check runs.
const THROUGHPUT_IMAGES: usize = 64;

/// Run the phase 03 gate.
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase03-verify".into()),
    );
    let models_dir =
        PathBuf::from(crate::flag(args, "--models").unwrap_or_else(|| "models".into()));
    let card_root = PathBuf::from(crate::flag(args, "--cards").unwrap_or_else(|| ".".into()));

    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. The signed model set.
    let registry = match ModelRegistry::open(&models_dir, &card_root, trusted_public_key()) {
        Ok(registry) => registry,
        Err(err) => {
            eprintln!("models: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match registry.verify_all() {
        Ok(files) => println!(
            "registry: {} models, {files} files, signature and digests verified",
            registry.lock().models.len()
        ),
        Err(err) => {
            eprintln!("registry: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for entry in &registry.lock().models {
        if std::fs::read_to_string(card_root.join(&entry.model_card))
            .map(|text| text.trim().is_empty())
            .unwrap_or(true)
        {
            eprintln!("registry: {} has no usable model card", entry.name);
            failures += 1;
        }
    }

    // 2. The hardware probe and the plan.
    let paths = PlanPaths::new(&work);
    let probe_started = clock.monotonic_ms();
    let outcome = match probe(&BackendRegistry::with_reference(), &paths, &clock) {
        Ok(outcome) => outcome,
        Err(err) => {
            eprintln!("probe: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let probe_ms = clock.monotonic_ms().saturating_sub(probe_started);
    if outcome.persist {
        if let Err(err) = plan::save(&paths, &outcome.plan) {
            eprintln!("plan: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    println!(
        "probe: {probe_ms} ms, providers {:?}, selected {}, {} unavailable",
        outcome
            .plan
            .ep_order
            .iter()
            .map(|ep| ep.as_str())
            .collect::<Vec<_>>(),
        outcome.plan.selected_ep().as_str(),
        outcome.plan.unavailable.len()
    );
    if probe_ms > aura_infer::probe::PROBE_CEILING_MS {
        eprintln!("probe: took {probe_ms} ms against a 15,000 ms ceiling");
        failures += 1;
    }

    // 3. Warmup, and the cold-load cost.
    let source: Arc<dyn ModelSource> = Arc::new(registry);
    let engine = InferEngine::new(
        BackendRegistry::with_reference(),
        Arc::clone(&source),
        outcome.plan.clone(),
        Arc::clone(&clock),
    );
    let warm_started = clock.monotonic_ms();
    if let Err(err) = engine.warmup(&[EMBEDDING, SCENE]) {
        eprintln!("warmup: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }
    let warm_ms = clock.monotonic_ms().saturating_sub(warm_started);
    println!("warmup: 2 models in {warm_ms} ms");

    // 4. Throughput on the analysis path.
    let images: Vec<Tensor> = (0..THROUGHPUT_IMAGES)
        .map(|_| fixtures::sample_input(1))
        .collect();
    let batch: Vec<Vec<TensorView<'_>>> = images.iter().map(|image| vec![image.view()]).collect();
    let batch_started = clock.monotonic_ms();
    match engine.run_batch(EMBEDDING, batch, Priority::AiBatch) {
        Ok(results) => {
            let elapsed = clock.monotonic_ms().saturating_sub(batch_started).max(1);
            let per_second = (THROUGHPUT_IMAGES as f64 * 1000.0) / elapsed as f64;
            println!(
                "throughput: {THROUGHPUT_IMAGES} images in {elapsed} ms ({per_second:.1}/s), \
                 batch {}",
                results.first().map_or(0, |result| result.batch_size)
            );
        }
        Err(err) => {
            eprintln!("throughput: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 5. Parity across the precisions this build runs.
    match parity_report(&source) {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
        }
        Err(message) => {
            eprintln!("{message}");
            failures += 1;
        }
    }

    // 6. A forced memory squeeze.
    let mut squeezed_plan = outcome.plan.clone();
    squeezed_plan.vram_budget_mb = 24;
    squeezed_plan.default_batch.embedding = 8;
    let squeezed = InferEngine::new(
        BackendRegistry::with_reference(),
        Arc::clone(&source),
        squeezed_plan,
        Arc::clone(&clock),
    );
    let squeeze_images: Vec<Tensor> = (0..8).map(|_| fixtures::sample_input(1)).collect();
    let squeeze_batch: Vec<Vec<TensorView<'_>>> = squeeze_images
        .iter()
        .map(|image| vec![image.view()])
        .collect();
    match squeezed.run_batch(EMBEDDING, squeeze_batch, Priority::AiBatch) {
        Ok(results) if results.len() == 8 => {
            let stats = squeezed.scheduler_stats();
            println!(
                "memory: budget 24 MB, {} downshifts, peak {} MB, all {} images finished",
                stats.downshifts,
                squeezed.peak_memory_bytes() / (1024 * 1024),
                results.len()
            );
            if stats.downshifts == 0 {
                eprintln!("memory: the squeeze did not force a downshift");
                failures += 1;
            }
            if squeezed.peak_memory_bytes() > 24 * 1024 * 1024 {
                eprintln!("memory: the ledger exceeded its budget");
                failures += 1;
            }
        }
        Ok(results) => {
            eprintln!("memory: only {} of 8 images finished", results.len());
            failures += 1;
        }
        Err(err) => {
            eprintln!("memory: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 7. Cancellation.
    let cancellable = InferEngine::new(
        BackendRegistry::with_reference(),
        Arc::clone(&source),
        outcome.plan.clone(),
        Arc::clone(&clock),
    );
    cancellable.cancel_token().cancel();
    let cancel_images: Vec<Tensor> = (0..4).map(|_| fixtures::sample_input(1)).collect();
    let cancel_batch: Vec<Vec<TensorView<'_>>> = cancel_images
        .iter()
        .map(|image| vec![image.view()])
        .collect();
    match cancellable.run_batch(EMBEDDING, cancel_batch, Priority::AiBatch) {
        Err(err) if err.code.0 == "AURA-ML-5011" => println!("cancel: stopped with {}", err.code),
        Err(err) => {
            eprintln!("cancel: wrong code [{}]", err.code);
            failures += 1;
        }
        Ok(_) => {
            eprintln!("cancel: a cancelled batch ran to completion");
            failures += 1;
        }
    }

    // 8. A provider that misbehaves is set aside rather than used.
    let drifting = BackendRegistry::with_reference().register(Arc::new(FakeBackend::new(
        ExecutionProvider::Cuda,
        FakeBehaviour::Drift(1.0),
    )));
    match probe(&drifting, &PlanPaths::new(&work.join("drift")), &clock) {
        Ok(drift_outcome) => {
            if drift_outcome.plan.is_set_aside(ExecutionProvider::Cuda) {
                println!("providers: a drifting accelerator was set aside, processor path kept");
            } else {
                eprintln!("providers: a drifting accelerator was accepted");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("providers: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 9. Update and rollback, on a copy so the repository is untouched.
    match rollback_check(&work, &models_dir, &card_root) {
        Ok(line) => println!("{line}"),
        Err(message) => {
            eprintln!("{message}");
            failures += 1;
        }
    }

    if failures == 0 {
        println!("phase-03 verify: all checks clean");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase-03 verify: {failures} failures");
        ExitCode::FAILURE
    }
}

/// Compare the precisions against fp32 on the same input.
fn parity_report(source: &Arc<dyn ModelSource>) -> Result<Vec<String>, String> {
    use aura_infer::backend::{Backend, ReferenceBackend};

    let backend = ReferenceBackend::new();
    let input = fixtures::sample_input(4);
    let mut lines = Vec::new();

    for model in [EMBEDDING, SCENE] {
        let resolved = source
            .resolve(model, ExecutionProvider::Cpu)
            .map_err(|err| format!("parity: [{}] {}", err.code, err.detail))?;

        let mut reference: Vec<f32> = Vec::new();
        let mut worst_line = String::new();
        for precision in [Precision::Fp32, Precision::Fp16, Precision::Int8] {
            let mut variant = resolved.clone();
            variant.precision = precision;
            let session = backend
                .load(&variant)
                .map_err(|err| format!("parity: [{}] {}", err.code, err.detail))?;
            let outputs = session
                .run(&[input.view()])
                .map_err(|err| format!("parity: [{}] {}", err.code, err.detail))?;
            let values = outputs.first().map(|t| t.data.clone()).unwrap_or_default();

            if precision == Precision::Fp32 {
                reference = values;
                continue;
            }
            let worst = aura_infer::tensor::max_abs_diff(&reference, &values)
                .ok_or_else(|| "parity: output shapes differ between precisions".to_string())?;
            if worst > precision.parity_tolerance() {
                return Err(format!(
                    "parity: {} at {} drifted {worst:.3e}, tolerance {:.0e}",
                    model.name,
                    precision.as_str(),
                    precision.parity_tolerance()
                ));
            }
            worst_line.push_str(&format!(" {}={worst:.2e}", precision.as_str()));
        }
        lines.push(format!("parity: {}{worst_line}", model.name));
    }
    Ok(lines)
}

/// Install a model into a copy of the directory, reject it, and check the
/// previous file came back.
fn rollback_check(work: &Path, models_dir: &Path, card_root: &Path) -> Result<String, String> {
    let sandbox = work.join("models");
    drop(std::fs::remove_dir_all(&sandbox));
    std::fs::create_dir_all(&sandbox).map_err(|err| format!("rollback: {err}"))?;

    let mut copied = 0usize;
    for entry in std::fs::read_dir(models_dir).map_err(|err| format!("rollback: {err}"))? {
        let entry = entry.map_err(|err| format!("rollback: {err}"))?;
        if entry.path().is_file() {
            std::fs::copy(entry.path(), sandbox.join(entry.file_name()))
                .map_err(|err| format!("rollback: {err}"))?;
            copied += 1;
        }
    }

    let registry = ModelRegistry::open(&sandbox, card_root, trusted_public_key())
        .map_err(|err| format!("rollback: [{}] {}", err.code, err.detail))?;

    // Every variant of the entry is fetched by an install, so every variant has
    // to be on the transport - the same thing a real model pack has to get right.
    let files: Vec<String> = registry
        .lock()
        .entry(EMBEDDING.name, EMBEDDING.version)
        .map(|entry| {
            entry
                .variants
                .iter()
                .map(|variant| variant.file.clone())
                .collect()
        })
        .ok_or_else(|| "rollback: the embedding model has no variant".to_string())?;
    let file = files
        .first()
        .cloned()
        .ok_or_else(|| "rollback: the embedding model has no variant".to_string())?;

    let original = std::fs::read(sandbox.join(&file)).map_err(|err| format!("rollback: {err}"))?;

    let transport_dir = work.join("transport");
    std::fs::create_dir_all(&transport_dir).map_err(|err| format!("rollback: {err}"))?;
    for name in &files {
        std::fs::copy(sandbox.join(name), transport_dir.join(name))
            .map_err(|err| format!("rollback: {err}"))?;
    }

    registry
        .install(
            &FileTransport::new(&transport_dir),
            EMBEDDING.name,
            EMBEDDING.version,
            &|_| {},
        )
        .map_err(|err| format!("rollback: install [{}] {}", err.code, err.detail))?;
    if registry.state_of(EMBEDDING.name).pending != Some(EMBEDDING.version) {
        return Err("rollback: a fresh install was not staged as pending".to_string());
    }

    registry
        .reject(EMBEDDING.name, EMBEDDING.version)
        .map_err(|err| format!("rollback: reject [{}] {}", err.code, err.detail))?;

    let restored = std::fs::read(sandbox.join(&file)).map_err(|err| format!("rollback: {err}"))?;
    if restored != original {
        return Err("rollback: the restored file is not the one that worked".to_string());
    }
    if !registry
        .state_of(EMBEDDING.name)
        .was_rejected(EMBEDDING.version)
    {
        return Err("rollback: the failed version was not recorded as rejected".to_string());
    }

    Ok(format!(
        "rollback: {copied} files copied, install staged, rejection restored the previous file"
    ))
}

/// `aura-cli infer` - run one model over the deterministic sample batch.
///
/// The output is JSON on stdout so `ml/export_onnx/verify_parity.py` can compare
/// this runtime against ONNX Runtime, which is the only independent check of the
/// interpreter available anywhere in this project.
pub fn infer(args: &[String]) -> ExitCode {
    let Some(model_path) = crate::flag(args, "--model") else {
        eprintln!("usage: aura-cli infer --model FILE [--precision fp32|fp16|int8] [--batch N]");
        return ExitCode::FAILURE;
    };
    let batch: usize = crate::flag(args, "--batch")
        .and_then(|text| text.parse().ok())
        .unwrap_or(8);
    let precision = match crate::flag(args, "--precision").as_deref() {
        Some("fp16") => Precision::Fp16,
        Some("int8") => Precision::Int8,
        _ => Precision::Fp32,
    };

    let resolved = aura_infer::source::ResolvedModel {
        path: PathBuf::from(&model_path),
        precision,
        class: aura_infer::contract::infer::ModelClass::Embedding,
        working_set_mb: 8,
    };

    use aura_infer::backend::{Backend, ReferenceBackend};
    let session = match ReferenceBackend::new().load(&resolved) {
        Ok(session) => session,
        Err(err) => {
            eprintln!("[{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };

    // Which deterministic input to feed. The phase 03 placeholders take 32 px;
    // phase 05's embedding model takes 384, and a 32 px tensor into a 384 px graph
    // is a shape mismatch rather than a smaller test.
    let input = match crate::flag(args, "--input").as_deref() {
        Some("wedding") => fixtures::wedding_sample_input(batch),
        _ => fixtures::sample_input(batch),
    };
    // A stopwatch around one run, so `aura-cli infer` can report what a batch cost.
    // Through the injected clock rather than a wall-clock read, because the second
    // one is a determinism risk the banned-pattern check refuses on sight - and
    // rightly: a monotonic source is what a stopwatch wanted in the first place.
    let stopwatch = aura_core::clock::SystemClock::default();
    let started = aura_core::clock::Clock::monotonic_us(&stopwatch);
    let outputs = match session.run(&[input.view()]) {
        Ok(outputs) => outputs,
        Err(err) => {
            eprintln!("[{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };

    // Serialised through typed structures rather than the `json!` macro: the
    // macro unwraps internally, and R1 bans that in product code even when the
    // unwrap is unreachable.
    let elapsed_ms =
        aura_core::clock::Clock::monotonic_us(&stopwatch).saturating_sub(started) as f64 / 1000.0;
    eprintln!(
        "ran batch {batch} in {elapsed_ms:.1} ms ({:.1} ms per image)",
        elapsed_ms / batch.max(1) as f64
    );

    let report = InferReport {
        model: model_path,
        precision: precision.as_str().to_string(),
        batch,
        outputs: outputs
            .iter()
            .map(|tensor| TensorReport {
                shape: tensor.shape.clone(),
                data: tensor.data.clone(),
            })
            .collect(),
    };

    match serde_json::to_string(&report) {
        Ok(rendered) => {
            println!("{rendered}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("could not render the result: {err}");
            ExitCode::FAILURE
        }
    }
}

/// What `aura-cli infer` prints, for the Python parity scripts to read.
#[derive(serde::Serialize)]
struct InferReport {
    model: String,
    precision: String,
    batch: usize,
    outputs: Vec<TensorReport>,
}

/// One output tensor in that report.
#[derive(serde::Serialize)]
struct TensorReport {
    shape: Vec<usize>,
    data: Vec<f32>,
}
