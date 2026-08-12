//! `cargo xtask models` - generate, sign and check the pinned model set.
//!
//! Two jobs, and they are deliberately in the same place so they cannot drift.
//!
//! `--generate` writes the placeholder models, computes their digests, renders
//! `models/models.lock` and signs it with the development key. It is
//! reproducible: the fixtures come from a fixed seed, so two machines produce
//! byte-identical files and therefore identical digests.
//!
//! `--check` is the CI gate. It verifies the manifest signature, every file's
//! size and digest, and - the rule that is skipped most often in this industry
//! and costs the most when it is - that every model has a model card with real
//! content in it. Article VI rule M1: no card, no model.
//!
//! Panics are permitted here: this is a developer tool, not product code.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use aura_infer::contract::infer::{ExecutionProvider, ModelClass, Precision, Version};
use aura_infer::onnx::model::{OnnxModel, TensorData};
use aura_infer::onnx::{fixtures, parse, serialise, OPSET};
use aura_infer::tensor::{round_to_f16, QuantParams};
use aura_models::contract::manifest::{
    InputSpec, ModelEntry, ModelsLock, PrecisionPolicy, Variant, LOCK_SCHEMA,
};
use aura_models::registry::{trusted_public_key, LOCK_FILE, SIGNATURE_FILE};
use aura_models::verify::{from_hex, sha256_file, verify_manifest};

/// Where the pinned models live.
const MODELS_DIR: &str = "models";

/// Headings every model card must carry, from Article VI rule M1.
const REQUIRED_CARD_SECTIONS: &[&str] = &[
    "## Purpose",
    "## Architecture",
    "## Training data",
    "## Latency",
    "## Quality gate",
    "## Known failure modes",
    "## Fallback",
];

/// Entry point for `cargo xtask models`.
pub fn run(args: &[String]) -> ExitCode {
    if args.iter().any(|arg| arg == "--generate") {
        generate()
    } else {
        check()
    }
}

/// Write the placeholder models, the manifest and its signature.
fn generate() -> ExitCode {
    let directory = PathBuf::from(MODELS_DIR);
    fs::create_dir_all(&directory).expect("create models directory");

    let entries = vec![
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::EMBEDDING_MODEL,
                version: Version::new(1, 0, 0),
                task: "embedding",
                class: ModelClass::Embedding,
                model: fixtures::tiny_embedding(),
                output: BTreeMap::from([(
                    "embedding".to_string(),
                    vec![1, fixtures::EMBEDDING_DIM],
                )]),
                precision_policy: PrecisionPolicy::permissive(),
            },
        ),
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::SCENE_MODEL,
                version: Version::new(1, 0, 0),
                task: "multiclass",
                class: ModelClass::Embedding,
                model: fixtures::tiny_scene(),
                output: BTreeMap::from([(
                    "scene_probs".to_string(),
                    vec![1, fixtures::SCENE_CLASSES],
                )]),
                // A scene model conditions colour decisions downstream, so it is
                // the example of a model whose card forbids int8 (section 12).
                precision_policy: PrecisionPolicy::no_int8(),
            },
        ),
    ];

    let lock = ModelsLock {
        schema: LOCK_SCHEMA,
        // Fixed rather than "now": the manifest is an artefact whose bytes must be
        // reproducible, and a timestamp would change its digest on every run.
        generated_at: "2026-08-12T00:00:00Z".to_string(),
        models: entries,
    };

    let rendered = serde_json::to_string_pretty(&lock).expect("render models.lock") + "\n";
    let lock_path = directory.join(LOCK_FILE);
    fs::write(&lock_path, rendered.as_bytes()).expect("write models.lock");

    let status = std::process::Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "-p",
            "model-sign",
            "--",
            "sign",
            "--dev",
            &lock_path.display().to_string(),
            "--out",
            &directory.join(SIGNATURE_FILE).display().to_string(),
        ])
        .status()
        .expect("spawn model-sign");
    if !status.success() {
        eprintln!("models: signing failed");
        return ExitCode::FAILURE;
    }

    println!(
        "models: {} entries written to {}",
        lock.models.len(),
        directory.display()
    );
    ExitCode::SUCCESS
}

/// One placeholder model, gathered so `build_entry` takes two arguments.
struct Placeholder {
    name: &'static str,
    version: Version,
    task: &'static str,
    class: ModelClass,
    model: OnnxModel,
    output: BTreeMap<String, Vec<usize>>,
    precision_policy: PrecisionPolicy,
}

/// Build one manifest entry, writing its variant files.
fn build_entry(directory: &Path, spec: &Placeholder) -> ModelEntry {
    let Placeholder {
        name,
        version,
        task,
        class,
        model,
        output,
        precision_policy,
    } = spec;
    let (name, version, task, class, precision_policy) =
        (*name, *version, *task, *class, *precision_policy);

    let mut variants = Vec::new();

    let fp32_file = format!("{name}_{version}.fp32.onnx");
    write_variant(directory, &fp32_file, model);
    variants.push(variant(
        directory,
        ExecutionProvider::Cpu,
        Precision::Fp32,
        &fp32_file,
    ));

    let fp16_file = format!("{name}_{version}.fp16.onnx");
    write_variant(directory, &fp16_file, &requantised(model, Precision::Fp16));
    variants.push(variant(
        directory,
        ExecutionProvider::Cuda,
        Precision::Fp16,
        &fp16_file,
    ));

    if precision_policy.allows(Precision::Int8) {
        let int8_file = format!("{name}_{version}.int8.onnx");
        write_variant(directory, &int8_file, &requantised(model, Precision::Int8));
        variants.push(variant(
            directory,
            ExecutionProvider::Cpu,
            Precision::Int8,
            &int8_file,
        ));
    }

    let parameters: usize = model
        .graph
        .initializers
        .iter()
        .map(aura_infer::onnx::model::InitTensor::element_count)
        .sum();
    // Weights, plus the activation headroom the reference session reserves.
    let working_set_mb = ((parameters * 4) as u64 / (1024 * 1024) + 8) as u32;

    ModelEntry {
        name: name.to_string(),
        version,
        task: task.to_string(),
        class,
        input: InputSpec {
            shape: vec![1, 3, fixtures::INPUT_SIDE, fixtures::INPUT_SIDE],
            layout: "NCHW".to_string(),
            range: "0..1".to_string(),
            colour: "srgb".to_string(),
        },
        output: output.clone(),
        variants,
        licence: "proprietary".to_string(),
        model_card: format!("docs/model-cards/{name}.md"),
        min_app_version: "0.1.0".to_string(),
        working_set_mb,
        precision_policy,
        opset: OPSET,
    }
}

fn write_variant(directory: &Path, file: &str, model: &OnnxModel) {
    fs::write(directory.join(file), serialise(model)).expect("write model variant");
}

fn variant(directory: &Path, ep: ExecutionProvider, precision: Precision, file: &str) -> Variant {
    let path = directory.join(file);
    let bytes = fs::metadata(&path).expect("stat variant").len();
    let sha256 = sha256_file(&path).expect("digest variant");
    Variant {
        ep,
        precision,
        file: file.to_string(),
        sha256,
        bytes,
    }
}

/// Round every weight to a precision at export time, the way `quantise.py` does.
///
/// This is what makes the variants genuinely different files rather than three
/// names for one, which is what the parity harness needs in order to measure
/// anything at all.
fn requantised(model: &OnnxModel, precision: Precision) -> OnnxModel {
    let mut out = model.clone();
    for tensor in &mut out.graph.initializers {
        if let TensorData::Float(values) = &mut tensor.data {
            match precision {
                Precision::Fp32 => {}
                Precision::Fp16 => {
                    for value in values.iter_mut() {
                        *value = round_to_f16(*value);
                    }
                }
                Precision::Int8 => {
                    let params = QuantParams::observe(values);
                    for value in values.iter_mut() {
                        *value = params.round_trip(*value);
                    }
                }
            }
        }
    }
    out
}

/// The CI gate: signature, digests, cards, opset.
fn check() -> ExitCode {
    let directory = PathBuf::from(MODELS_DIR);
    let lock_path = directory.join(LOCK_FILE);
    let signature_path = directory.join(SIGNATURE_FILE);

    if !lock_path.exists() {
        eprintln!(
            "models: {} is missing; run `cargo xtask models --generate`",
            lock_path.display()
        );
        return ExitCode::FAILURE;
    }

    let manifest = fs::read(&lock_path).expect("read models.lock");
    let signature_text = fs::read_to_string(&signature_path).expect("read manifest.sig");
    let signature = from_hex(signature_text.trim()).expect("signature is hex");
    let public_key = from_hex(trusted_public_key()).expect("public key is hex");

    if let Err(err) = verify_manifest(&manifest, &signature, &public_key) {
        eprintln!("models: {err}");
        return ExitCode::FAILURE;
    }

    let lock: ModelsLock = serde_json::from_slice(&manifest).expect("parse models.lock");
    let mut failures = 0usize;
    let mut files = 0usize;

    for entry in &lock.models {
        if entry.opset > OPSET {
            eprintln!(
                "models: {} needs opset {} and this build implements {OPSET}",
                entry.name, entry.opset
            );
            failures += 1;
        }
        failures += check_card(&entry.model_card, &entry.name);
        if entry.working_set_mb == 0 {
            eprintln!("models: {} declares no working set", entry.name);
            failures += 1;
        }

        for variant in &entry.variants {
            files += 1;
            let path = directory.join(&variant.file);
            match fs::metadata(&path) {
                Ok(meta) if meta.len() == variant.bytes => {}
                Ok(meta) => {
                    eprintln!(
                        "models: {} is {} bytes, the manifest says {}",
                        variant.file,
                        meta.len(),
                        variant.bytes
                    );
                    failures += 1;
                    continue;
                }
                Err(err) => {
                    eprintln!("models: {} is missing: {err}", variant.file);
                    failures += 1;
                    continue;
                }
            }
            let digest = sha256_file(&path).expect("digest");
            if !digest.eq_ignore_ascii_case(&variant.sha256) {
                eprintln!(
                    "models: {} digest does not match the manifest",
                    variant.file
                );
                failures += 1;
                continue;
            }
            // A file that verifies must also load: a signed, correctly-digested
            // model this build cannot execute is still a broken release.
            let bytes = fs::read(&path).expect("read variant");
            match parse(&bytes) {
                Ok(parsed) => {
                    if let Err(err) =
                        aura_infer::onnx::Executable::compile(&parsed, variant.precision)
                    {
                        eprintln!("models: {} does not compile: {err}", variant.file);
                        failures += 1;
                    }
                }
                Err(err) => {
                    eprintln!("models: {} does not parse: {err}", variant.file);
                    failures += 1;
                }
            }
        }
    }

    if failures == 0 {
        println!(
            "models: {} models, {files} files, signature and cards verified",
            lock.models.len()
        );
        ExitCode::SUCCESS
    } else {
        eprintln!("models: {failures} problems");
        ExitCode::FAILURE
    }
}

/// Article VI rule M1, enforced mechanically.
fn check_card(card: &str, model: &str) -> usize {
    let Ok(text) = fs::read_to_string(card) else {
        eprintln!("models: {model} has no model card at {card}");
        return 1;
    };
    let mut missing = 0usize;
    for section in REQUIRED_CARD_SECTIONS {
        if !text.contains(section) {
            eprintln!("models: card {card} is missing the {section} section");
            missing += 1;
        }
    }
    missing
}
