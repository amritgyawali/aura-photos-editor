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
                input: Placeholder::image(fixtures::INPUT_SIDE),
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
                input: Placeholder::image(fixtures::INPUT_SIDE),
                output: BTreeMap::from([(
                    "scene_probs".to_string(),
                    vec![1, fixtures::SCENE_CLASSES],
                )]),
                // A scene model conditions colour decisions downstream, so it is
                // the example of a model whose card forbids int8 (section 12).
                precision_policy: PrecisionPolicy::no_int8(),
            },
        ),
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::WEDDING_EMBEDDING_MODEL,
                version: Version::new(1, 0, 0),
                task: "embedding",
                class: ModelClass::Embedding,
                model: fixtures::wedding_embedding(),
                input: Placeholder::image(fixtures::WEDDING_INPUT_SIDE),
                output: BTreeMap::from([(
                    "embedding".to_string(),
                    vec![1, fixtures::WEDDING_EMBEDDING_DIM],
                )]),
                // int8 is allowed and is the variant the processor path uses: a
                // 512-d direction survives per-tensor quantisation, and phase 05's
                // whole argument is that this model runs 4,000 times.
                precision_policy: PrecisionPolicy::permissive(),
            },
        ),
        // PHASE-06. Three models, and their precision policies differ for reasons
        // that are worth stating rather than copying.
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::FACE_DETECT_MODEL,
                version: Version::new(1, 0, 0),
                task: "detection",
                // Segmentation rather than embedding: the batch memory ledger cares
                // about a dense 640 px activation map, not about a 512-wide vector,
                // and putting a detector in the embedding column would make the
                // scheduler start it at a batch size no laptop can hold.
                class: ModelClass::Segmentation,
                model: fixtures::face_detect(),
                input: Placeholder::image(fixtures::FACE_DETECT_INPUT_SIDE),
                output: BTreeMap::from([
                    (
                        "head_8".to_string(),
                        vec![
                            1,
                            fixtures::FACE_HEAD_CHANNELS,
                            fixtures::face_head_side(8),
                            fixtures::face_head_side(8),
                        ],
                    ),
                    (
                        "head_16".to_string(),
                        vec![
                            1,
                            fixtures::FACE_HEAD_CHANNELS,
                            fixtures::face_head_side(16),
                            fixtures::face_head_side(16),
                        ],
                    ),
                    (
                        "head_32".to_string(),
                        vec![
                            1,
                            fixtures::FACE_HEAD_CHANNELS,
                            fixtures::face_head_side(32),
                            fixtures::face_head_side(32),
                        ],
                    ),
                ]),
                // int8 is forbidden. A detection head regresses box distances in
                // stride units, and per-tensor int8 quantisation of a regression
                // output moves a 40 px face's box by several pixels - which is the
                // difference between recall and a missed guest. Section 10.1 asks
                // for 0.90 recall on small faces and this is part of how it is kept.
                precision_policy: PrecisionPolicy::no_int8(),
            },
        ),
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::FACE_EMBED_MODEL,
                version: Version::new(1, 0, 0),
                task: "embedding",
                class: ModelClass::Embedding,
                model: fixtures::face_embed(),
                input: Placeholder::image(fixtures::FACE_CROP_SIDE),
                output: BTreeMap::from([(
                    "embedding".to_string(),
                    vec![1, fixtures::FACE_EMBED_DIM],
                )]),
                // int8 is allowed here, for the same reason it is allowed on
                // `wedding_embedding`: a normalised 512-d direction survives
                // per-tensor quantisation, and this model runs once per face.
                precision_policy: PrecisionPolicy::permissive(),
            },
        ),
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::FACE_QUALITY_MODEL,
                version: Version::new(1, 0, 0),
                task: "multiclass",
                class: ModelClass::Embedding,
                model: fixtures::face_quality(),
                input: Placeholder::image(fixtures::FACE_CROP_SIDE),
                output: BTreeMap::from([(
                    "quality".to_string(),
                    vec![1, fixtures::FACE_QUALITY_OUTPUTS],
                )]),
                // int8 is forbidden. Four sigmoid outputs quantised per tensor lose
                // most of their resolution near 0 and 1, and this head's whole job is
                // to decide whether a face is above 0.4 - a gate whose inputs are
                // quantised to sixteen levels is a coin toss for anything near it.
                precision_policy: PrecisionPolicy::no_int8(),
            },
        ),
        // PHASE-07. Two heads, and neither of them takes pixels: section 6.1's
        // "shared trunk = the Phase 05 embedding (frozen) plus a small trainable
        // adapter, so scene inference costs almost nothing extra per image".
        // That is what makes section 11's 35-second budget for four thousand
        // images achievable at all - the expensive part already ran in phase 05.
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::SCENE_CLASSIFIER_MODEL,
                version: Version::new(1, 0, 0),
                task: "multiclass",
                // Embedding rather than segmentation: the memory ledger is
                // reasoning about a 528-wide vector and a 256-wide adapter, which
                // is the cheapest thing in the product. Filing it as a
                // segmentation model would make the scheduler reserve a dense
                // activation map that never exists.
                class: ModelClass::Embedding,
                model: fixtures::scene_classifier(),
                input: Placeholder::features(fixtures::SCENE_INPUT_DIM),
                output: BTreeMap::from([
                    (
                        "scene_probs".to_string(),
                        vec![1, fixtures::SCENE_CLASSIFIER_CLASSES],
                    ),
                    (
                        "attr_probs".to_string(),
                        vec![1, fixtures::SCENE_ATTRIBUTES],
                    ),
                ]),
                // int8 is forbidden, and the argument is `face_quality`'s. A
                // 22-way softmax quantised per tensor loses most of its
                // resolution exactly where the decision is made - around the
                // top-1 margin that decides `SceneId::Unknown` - and every
                // threshold in phases 09 to 29 hangs off which class wins.
                precision_policy: PrecisionPolicy::no_int8(),
            },
        ),
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::RITUAL_CLASSIFIER_MODEL,
                version: Version::new(1, 0, 0),
                task: "multiclass",
                class: ModelClass::Embedding,
                model: fixtures::ritual_classifier(),
                input: Placeholder::features(fixtures::RITUAL_INPUT_DIM),
                output: BTreeMap::from([(
                    "ritual_probs".to_string(),
                    vec![1, fixtures::RITUAL_SLOTS],
                )]),
                // int8 is forbidden for a sharper version of the same reason. A
                // 160-way softmax spends most of its mass on slot 0 and the rest
                // spread thinly, so the difference between `saptapadi_pheras` and
                // `saat_phera` lives in the third decimal place. Quantising that
                // does not make the head faster in any way a photographer would
                // notice; it makes the abstention margin meaningless.
                precision_policy: PrecisionPolicy::no_int8(),
            },
        ),
        // PHASE-09. Two heads, and both of them exist for cases where a classical
        // measurement is right about the pixels and wrong about the photograph.
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::FOCUS_HEAD_MODEL,
                version: Version::new(1, 0, 0),
                task: "multiclass",
                class: ModelClass::Embedding,
                model: fixtures::focus_head(),
                input: Placeholder::grey(fixtures::FOCUS_CROP_SIDE),
                output: BTreeMap::from([(
                    "focus_probs".to_string(),
                    vec![1, fixtures::FOCUS_CLASSES],
                )]),
                // int8 is allowed, and this is the first head in the product where
                // it is allowed on a softmax. Three classes over a 64 px crop is a
                // coarse question - sharp, soft, deliberately out of focus - and the
                // decision it feeds is deliberately asymmetric: `focus::apply_head`
                // lets the head *withdraw* a softness claim at 0.70 confidence and
                // never lets it make one. Quantisation noise around a threshold that
                // can only ever exonerate a photograph costs nothing a photographer
                // would see, and this head runs on several regions of every frame.
                precision_policy: PrecisionPolicy::permissive(),
            },
        ),
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::EYE_STATE_MODEL,
                version: Version::new(1, 0, 0),
                task: "multiclass",
                class: ModelClass::Embedding,
                model: fixtures::eye_state(),
                input: Placeholder::image(fixtures::EYE_CROP_SIDE),
                output: BTreeMap::from([("eye_probs".to_string(), vec![1, fixtures::EYE_CLASSES])]),
                // int8 is forbidden, and the contrast with the focus head above is
                // the point. This head *can* convict a photograph: a confident
                // `closed` on a gating face raises `EYES_CLOSED`, and section 12's
                // first failure mode is that false rejections destroy trust
                // instantly. `eyes::ACT_ON_CLOSED` is 0.55, so the decision lives
                // exactly where per-tensor int8 has least resolution.
                precision_policy: PrecisionPolicy::no_int8(),
            },
        ),
        // PHASE-10. Two heads, and both of them end in a `Sigmoid` rather than a
        // `Softmax`. Section 2.1 requires the expression outputs to be "all
        // continuous, not one-hot" and section 5 spells interactions as a list of
        // (kind, strength) pairs: a face can be laughing and crying at once, and a
        // frame can be a hug and tears being wiped. A softmax would make the
        // classes compete for one unit of probability, which is the modelling
        // mistake that produces emotionally flat galleries - so it is prevented in
        // the graph rather than warned about in a comment.
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::EXPRESSION_HEAD_MODEL,
                version: Version::new(1, 0, 0),
                // `multilabel`, not `multiclass`. The task string is read by
                // nothing at run time and by a person every time somebody asks
                // what a model does, and calling eight independent sigmoids a
                // multiclass head is the one-hot mistake written down.
                task: "multilabel",
                class: ModelClass::Embedding,
                model: fixtures::expression_head(),
                input: Placeholder::image(fixtures::EXPRESSION_CROP_SIDE),
                output: BTreeMap::from([(
                    "expression".to_string(),
                    vec![1, fixtures::EXPRESSION_CHANNELS],
                )]),
                // int8 is forbidden, and the argument is `face_quality`'s
                // sharpened by one case. Eight independent sigmoids quantised per
                // tensor lose most of their resolution near 0 and 1, and one of
                // the eight - `tears` - is read against a 0.85 threshold that
                // decides whether the product says the word "crying" to a
                // photographer and whether phase 09 exonerates a closed eye.
                // Section 12's fourth failure mode is a false tear; this is where
                // a quantisation choice would cause one.
                precision_policy: PrecisionPolicy::no_int8(),
            },
        ),
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::INTERACTION_HEAD_MODEL,
                version: Version::new(1, 0, 0),
                task: "multilabel",
                class: ModelClass::Embedding,
                model: fixtures::interaction_head(),
                // Four channels: three of colour and one person-prior plane.
                // Section 6.2's "person boxes as spatial priors", as a plane
                // rather than as coordinates appended to a vector, because where
                // the people are is a spatial fact and a convolution is what reads
                // spatial facts.
                input: Placeholder::planes(
                    fixtures::INTERACTION_CHANNELS,
                    fixtures::INTERACTION_SIDE,
                ),
                output: BTreeMap::from([(
                    "interaction_probs".to_string(),
                    vec![1, fixtures::INTERACTION_CLASSES],
                )]),
                // int8 is allowed, and this is the second head in the product
                // where it is. Nine coarse pose questions over a 160 px frame -
                // are two bodies in contact, is a glass raised - are decided far
                // from any threshold that matters: an interaction at 0.31 and one
                // at 0.34 both read as "weakly present" and feed one of nine
                // ranker terms. Nothing here can convict a photograph, and this
                // head runs once on every frame in the wedding.
                precision_policy: PrecisionPolicy::permissive(),
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
    /// What one sample of this model's input looks like, without the batch
    /// dimension.
    ///
    /// A full spec rather than a side length, because PHASE-07's two heads do not
    /// take pixels. They sit on the frozen phase 05 embedding - section 6.1's
    /// "shared trunk = the Phase 05 embedding (frozen) plus a small trainable
    /// adapter" - so their input is a feature vector and their manifest shape has
    /// to say so, or every call fails the runtime's shape check with
    /// `AURA-ML-5007`.
    input: InputSpec,
    output: BTreeMap<String, Vec<usize>>,
    precision_policy: PrecisionPolicy,
}

impl Placeholder {
    /// The input spec for a square image model at `side` pixels.
    fn image(side: usize) -> InputSpec {
        InputSpec {
            shape: vec![1, 3, side, side],
            layout: "NCHW".to_string(),
            range: "0..1".to_string(),
            colour: "srgb".to_string(),
        }
    }

    /// The input spec for a single-channel image model at `side` pixels.
    ///
    /// PHASE-09's focus head. One channel because focus is a luminance question and
    /// the analyser already holds the luminance plane; a three-channel input would
    /// triple the first convolution to carry chroma the question does not use.
    fn grey(side: usize) -> InputSpec {
        InputSpec {
            shape: vec![1, 1, side, side],
            layout: "NCHW".to_string(),
            range: "0..1".to_string(),
            colour: "luma".to_string(),
        }
    }

    /// The input spec for a square model with `channels` planes at `side` pixels.
    ///
    /// PHASE-10's interaction head, which reads three planes of colour plus one of
    /// person prior. `colour` says `srgb+prior` rather than `srgb`, because a
    /// manifest that claimed three sRGB channels for a four-channel tensor would be
    /// documenting the wrong preprocessing - and the preprocessing is the half of a
    /// model contract that has no code to check it.
    fn planes(channels: usize, side: usize) -> InputSpec {
        InputSpec {
            shape: vec![1, channels, side, side],
            layout: "NCHW".to_string(),
            range: "0..1".to_string(),
            colour: "srgb+prior".to_string(),
        }
    }

    /// The input spec for a feature-vector model of `width` samples.
    ///
    /// `range` is `unbounded` rather than `0..1`: an embedding's components are a
    /// direction on the unit sphere and roughly half of them are negative. A
    /// manifest that claimed `0..1` would be documenting a normalisation nobody
    /// performs.
    fn features(width: usize) -> InputSpec {
        InputSpec {
            shape: vec![1, width],
            layout: "NC".to_string(),
            range: "unbounded".to_string(),
            colour: "none".to_string(),
        }
    }
}

/// Build one manifest entry, writing its variant files.
fn build_entry(directory: &Path, spec: &Placeholder) -> ModelEntry {
    let Placeholder {
        name,
        version,
        task,
        class,
        model,
        input,
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
        input: input.clone(),
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
