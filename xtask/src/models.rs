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
        // PHASE-11. Two heads that could hardly be less alike: one reads a person
        // crop and emits geometry, the other reads twelve numbers and emits taste.
        // They ship as one pack and move together, which is why
        // `composition::keypoints::MODEL_VER` is one number - no consumer of a
        // framing judgement cares which of the two moved, only that the judgements
        // are not comparable across the move.
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::POSE_MODEL,
                version: Version::new(1, 0, 0),
                // `keypoints`, not `detection`. The head localises joints inside a
                // box it is given; it does not find people. Calling it a detector
                // in the one field a person reads would be documenting a capability
                // this build does not have - phase 06 finds the faces and
                // `composition::keypoints::person_box` derives the body.
                task: "keypoints",
                class: ModelClass::Embedding,
                model: fixtures::pose_keypoints(),
                input: Placeholder::image(fixtures::POSE_CROP_SIDE),
                output: BTreeMap::from([(
                    "keypoints".to_string(),
                    vec![1, fixtures::POSE_OUTPUTS],
                )]),
                // int8 is forbidden. The output is a *coordinate*, and the whole
                // question this phase asks of it is which side of a frame edge that
                // coordinate falls on. Per-tensor int8 over a 0..1 coordinate is a
                // quantisation step of about four thousandths of the crop, which at
                // 192 px is most of a wrist - so the one decision the head exists to
                // support is the one the quantisation would blur. Section 10.1 asks
                // for a limb-crop F1 of 0.90.
                precision_policy: PrecisionPolicy::no_int8(),
            },
        ),
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::AESTHETIC_MODEL,
                version: Version::new(1, 0, 0),
                // `ranker`, and the word is chosen against `regression`. It is
                // trained on pairwise photographer choices and shipped as a
                // pointwise scorer; the absolute value of its output means nothing
                // and only the ordering of two outputs does. A manifest that said
                // `regression` would invite somebody to read 0.62 as "62 % good".
                task: "ranker",
                class: ModelClass::Embedding,
                model: fixtures::aesthetic_head(),
                // Twelve geometric measures plus a twenty-three-way scene one-hot.
                // Not pixels: section 6.3 puts the geometry in the input so that the
                // head learns how much each violation matters rather than
                // re-deriving a horizon this build already measures to a tenth of a
                // degree.
                input: Placeholder::features(fixtures::AESTHETIC_FEATURES),
                output: BTreeMap::from([("aesthetic".to_string(), vec![1, 1])]),
                // int8 is allowed, and this is the third head in the product where
                // it is. `score::AESTHETIC_CAP` bounds what this number can do to a
                // composite at a quarter, in either direction, whatever any rule
                // file asks for - so a quantisation error of a few thousandths moves
                // a composition score by less than a thousandth. Nothing here can
                // convict a photograph, and the head runs once on every frame in the
                // wedding.
                precision_policy: PrecisionPolicy::permissive(),
            },
        ),
        // PHASE-15. Two heads, and the pair is the clearest illustration in the
        // product of "put the question in the input". One reads pixels because the
        // colour of a light is not recoverable from any summary of them; the other
        // reads a summary because what it has to learn is a conditional mean over
        // scene classes, and giving it pixels would make it spend its capacity
        // rediscovering a median this build already computes exactly.
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::WHITE_BALANCE_MODEL,
                version: Version::new(1, 0, 0),
                // `regression`, and the word is chosen against `multiclass`. The
                // output is a point in a continuous chromaticity plane; a manifest
                // that said `multiclass` would invite somebody to read the two
                // numbers as a two-way softmax, which they emphatically are not.
                task: "regression",
                class: ModelClass::Embedding,
                model: fixtures::white_balance(),
                // `colour` says `linear_srgb` rather than `srgb`, and the
                // distinction is invariant 8 rather than pedantry: an illuminant
                // estimate is a statement about a ratio of channel energies, and a
                // transfer curve applied before the first convolution makes that
                // ratio depend on brightness. A manifest that claimed `srgb` would
                // document the wrong preprocessing, which is the half of a model
                // contract that has no code to check it.
                input: InputSpec {
                    shape: vec![1, 3, fixtures::WB_INPUT_SIDE, fixtures::WB_INPUT_SIDE],
                    layout: "NCHW".to_string(),
                    range: "0..1".to_string(),
                    colour: "linear_srgb".to_string(),
                },
                output: BTreeMap::from([(
                    "illuminant_uv".to_string(),
                    vec![1, fixtures::WB_OUTPUTS],
                )]),
                // int8 is forbidden, and this is the sharpest case for it in the
                // product. The output is a chromaticity, and section 10.1's gate is
                // 200 K - which near daylight is about 0.004 in `u'v'`, on an axis
                // whose whole useful span is about 0.17. Per-tensor int8 quantises
                // that span into steps of roughly 0.0013, so three quantisation
                // steps is the entire tolerance the phase is measured against.
                precision_policy: PrecisionPolicy::no_int8(),
            },
        ),
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::EXPOSURE_SCENE_MODEL,
                version: Version::new(1, 0, 0),
                task: "regression",
                class: ModelClass::Embedding,
                model: fixtures::exposure_scene(),
                input: Placeholder::features(fixtures::EXPOSURE_INPUT_DIM),
                output: BTreeMap::from([("exposure".to_string(), vec![1, 1])]),
                // int8 is forbidden, for a version of the same argument. One
                // sigmoid mapped onto six stops means a quantisation step of about
                // 0.024 stops - which is comfortably inside section 10.1's 0.15 EV
                // tolerance, so the reason is not the tolerance. It is that this
                // head decides the exposure of every faceless frame in the wedding:
                // the details, the venue, the flat-lays. A systematic bias of two
                // hundredths of a stop across four hundred frames is a visible step
                // between the chapters that have faces in them and the ones that do
                // not, and gallery consistency is what phase 25 is then asked to fix.
                precision_policy: PrecisionPolicy::no_int8(),
            },
        ),
        // PHASE-16. One head, and it is the smallest table-shaped model in the
        // product: a conditional mean over twenty-three scene classes with
        // eleven covariates. It is registered, signed and carded, and
        // `colour::tone::TONE_HEAD_TRAINED` is false so nothing consults it -
        // ADR-0033 decision 5 records why that is a decision rather than a
        // fallback.
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::TONE_MODEL,
                version: Version::new(1, 0, 0),
                task: "regression",
                class: ModelClass::Embedding,
                model: fixtures::tone_model(),
                input: Placeholder::features(fixtures::TONE_INPUT_DIM),
                output: BTreeMap::from([("tone".to_string(), vec![1, fixtures::TONE_OUTPUTS])]),
                // int8 is forbidden. Five sigmoids mapped onto the recipe's
                // ranges means a quantisation step of about 0.4 units of
                // contrast, which is invisible on one frame - and that is not
                // the reason. It is that a systematic bias of half a unit
                // across four thousand frames is a gallery that is uniformly
                // slightly flatter than the album, and phase 25 is then asked
                // to reconcile two things that were never the same.
                precision_policy: PrecisionPolicy::no_int8(),
            },
        ),
        // PHASE-18. Two heads, and neither is consulted anywhere in this build:
        // `segment::SEG_HEAD_TRAINED` and `matting::MATTING_HEAD_TRAINED` are both
        // false, so no photograph is segmented by a random projection.
        // ADR-0037 decision 2 records why that is a decision rather than a
        // fallback, and it matters more here than in phases 15 and 16 for one
        // reason: a wrong tone parameter shows up in the histogram, and a wrong
        // class label on the pixels behind somebody's ear shows up only after
        // phase 20 has smoothed them.
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::SEGMENT_MODEL,
                version: Version::new(1, 0, 0),
                task: "segmentation",
                class: ModelClass::Segmentation,
                model: fixtures::semantic_segment(),
                input: Placeholder::linear_image(fixtures::SEGMENT_INPUT_SIDE),
                output: BTreeMap::from([(
                    "logits".to_string(),
                    vec![
                        1,
                        fixtures::SEGMENT_CLASSES,
                        fixtures::SEGMENT_HEAD_SIDE,
                        fixtures::SEGMENT_HEAD_SIDE,
                    ],
                )]),
                // int8 is forbidden, and the reason is not the class decision - an
                // argmax over twenty logits survives quantisation comfortably. It is
                // the *margin* between the top two, which is what
                // `Mask::confidence` is built from and what decides whether a region
                // may carry skin smoothing. Per-tensor int8 quantises a logit range
                // of about six into steps of 0.05, and a systematic shift of that
                // size across a gallery moves masks across `AGGRESSIVE_FLOOR` in
                // one direction or the other - which is a retouch that happens on
                // some frames and not others, for a reason nobody can see.
                precision_policy: PrecisionPolicy::no_int8(),
            },
        ),
        build_entry(
            &directory,
            &Placeholder {
                name: fixtures::MATTING_MODEL,
                version: Version::new(1, 0, 0),
                task: "matting",
                class: ModelClass::Segmentation,
                model: fixtures::alpha_matting(),
                input: Placeholder::trimap_patch(
                    fixtures::MATTING_CHANNELS,
                    fixtures::MATTING_PATCH_SIDE,
                ),
                output: BTreeMap::from([(
                    "alpha".to_string(),
                    vec![
                        1,
                        1,
                        fixtures::MATTING_OUTPUT_SIDE,
                        fixtures::MATTING_OUTPUT_SIDE,
                    ],
                )]),
                // int8 is forbidden, and this is the sharpest case for it in the
                // product after phase 15's white balance. The output *is* the
                // boundary: an alpha quantised into 256 steps is fine, and a
                // per-tensor int8 quantisation of the activations that produce it
                // is not - it lands as banding along a soft edge, which at 100 %
                // zoom on a veil is exactly the artefact section 10.1 audits for.
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

    /// The input spec for a square image model at `side` pixels in linear light.
    ///
    /// PHASE-18's segmentation head. `linear_srgb` rather than `srgb`, for the reason phase
    /// 15's white-balance head gives: every decision this network makes is about a ratio of
    /// channel energies - is this pixel the same colour as that face, is this region flatter
    /// than the scene - and a transfer curve applied before the first convolution makes those
    /// ratios depend on brightness. A manifest that claimed `srgb` would document the wrong
    /// preprocessing, which is the half of a model contract that has no code to check it.
    fn linear_image(side: usize) -> InputSpec {
        InputSpec {
            shape: vec![1, 3, side, side],
            layout: "NCHW".to_string(),
            range: "0..1".to_string(),
            colour: "linear_srgb".to_string(),
        }
    }

    /// The input spec for PHASE-18's matting head: three colour planes and a trimap.
    ///
    /// `colour` says `linear_srgb+trimap` rather than `linear_srgb`, because the fourth channel
    /// is not colour and a manifest that claimed four colour planes would be documenting a
    /// normalisation nobody performs on it. Phase 10's `planes` makes the same distinction for
    /// its person prior.
    fn trimap_patch(channels: usize, side: usize) -> InputSpec {
        InputSpec {
            shape: vec![1, channels, side, side],
            layout: "NCHW".to_string(),
            range: "0..1".to_string(),
            colour: "linear_srgb+trimap".to_string(),
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
