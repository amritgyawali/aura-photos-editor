//! Two small models, built in memory, deterministic to the last bit.
//!
//! Phase 03 has to prove a runtime, a registry, a scheduler and a signing chain
//! before a single real model exists - the first trained weights arrive in phase
//! 05. Waiting for them would mean shipping all of this untested, so the phase
//! document asks for "two placeholder models to exercise the whole path" and this
//! module is where they come from.
//!
//! They are *placeholders*, and the exit report says so wherever a number derived
//! from them appears. What they are not is arbitrary: both are built from the
//! same operators the real backbones will use, both are generated from a fixed
//! seed so every machine produces byte-identical files, and both are exercised at
//! all three precisions by the parity harness.
//!
//! The generator is a 64-bit xorshift rather than anything from a random-number
//! crate. A model file whose bytes changed when a dependency updated its
//! generator would break `models.lock` on every machine at once.

use crate::contract::infer::Tensor;
use crate::onnx::model::{
    AttrValue, Attribute, Graph, InitTensor, Node, OnnxModel, TensorData, ValueInfo, IR_VERSION,
    OPSET,
};

/// Name of the embedding placeholder.
pub const EMBEDDING_MODEL: &str = "aura_tiny_embedding";
/// Name of the classifier placeholder.
pub const SCENE_MODEL: &str = "aura_tiny_scene";
/// Name of the phase 05 perceptual embedding model.
pub const WEDDING_EMBEDDING_MODEL: &str = "wedding_embedding";

/// Side of the square input both phase 03 placeholders take.
pub const INPUT_SIDE: usize = 32;
/// Length of the embedding the first placeholder produces.
pub const EMBEDDING_DIM: usize = 32;
/// Number of classes the second placeholder scores.
pub const SCENE_CLASSES: usize = 6;

/// Side of the square input the phase 05 embedding model takes. Section 2.1 of
/// PHASE-05 specifies 384 px.
pub const WEDDING_INPUT_SIDE: usize = 384;
/// Width of the vector the phase 05 embedding model produces. Section 2.1: 512.
pub const WEDDING_EMBEDDING_DIM: usize = 512;
/// Width of the trunk between the backbone and the projection head.
///
/// The design in section 6.1 is `768 -> 1024 -> 512`. Those widths belong to a
/// ViT-B/16 backbone, which this build cannot run; the shipped graph keeps the
/// two-layer shape and scales the widths to what the deterministic interpreter can
/// execute in a wedding's worth of time. See
/// `docs/adr/ADR-0011-embeddings-and-similarity-index.md` section 3.
pub const WEDDING_TRUNK_DIM: usize = 256;

/// A deterministic weight generator: xorshift64*, fixed seed per tensor.
#[derive(Debug)]
struct Weights {
    state: u64,
}

impl Weights {
    fn new(seed: u64) -> Self {
        Self {
            state: seed | 1, // A zero state would produce only zeros.
        }
    }

    fn next_bits(&mut self) -> u64 {
        let mut state = self.state;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.state = state;
        state.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// A weight in `-scale..scale`, uniform enough for a placeholder.
    fn next(&mut self, scale: f32) -> f32 {
        let unit = (self.next_bits() >> 40) as f32 / 16_777_216.0; // 24 bits
        (unit * 2.0 - 1.0) * scale
    }

    fn tensor(&mut self, name: &str, dims: Vec<usize>, scale: f32) -> InitTensor {
        let count: usize = dims.iter().product();
        let samples = (0..count).map(|_| self.next(scale)).collect();
        InitTensor {
            name: name.to_string(),
            dims,
            data: TensorData::Float(samples),
        }
    }
}

fn node(op_type: &str, name: &str, inputs: &[&str], outputs: &[&str]) -> Node {
    Node {
        name: name.to_string(),
        op_type: op_type.to_string(),
        inputs: inputs.iter().map(|s| (*s).to_string()).collect(),
        outputs: outputs.iter().map(|s| (*s).to_string()).collect(),
        attributes: Vec::new(),
    }
}

fn with_ints(mut node: Node, name: &str, values: Vec<i64>) -> Node {
    node.attributes.push(Attribute {
        name: name.to_string(),
        value: AttrValue::Ints(values),
    });
    node
}

fn with_int(mut node: Node, name: &str, value: i64) -> Node {
    node.attributes.push(Attribute {
        name: name.to_string(),
        value: AttrValue::Int(value),
    });
    node
}

fn dynamic_batch(name: &str, rest: &[usize]) -> ValueInfo {
    let mut shape = vec![None];
    shape.extend(rest.iter().map(|dim| Some(*dim)));
    ValueInfo {
        name: name.to_string(),
        shape,
    }
}

/// A small convolutional embedding model.
///
/// `pixels [N, 3, 32, 32] -> embedding [N, 32]`, through two convolutions, a
/// pooling stage and a linear head - the same shape as a real backbone, three
/// orders of magnitude smaller.
#[must_use]
pub fn tiny_embedding() -> OnnxModel {
    let mut weights = Weights::new(0x00A0_1234_5678_9ABC);

    let graph = Graph {
        name: EMBEDDING_MODEL.to_string(),
        nodes: vec![
            with_ints(
                with_ints(
                    with_ints(
                        node(
                            "Conv",
                            "conv1",
                            &["pixels", "conv1_w", "conv1_b"],
                            &["conv1_out"],
                        ),
                        "kernel_shape",
                        vec![3, 3],
                    ),
                    "pads",
                    vec![1, 1, 1, 1],
                ),
                "strides",
                vec![1, 1],
            ),
            node("Relu", "relu1", &["conv1_out"], &["relu1_out"]),
            with_ints(
                with_ints(
                    node("MaxPool", "pool1", &["relu1_out"], &["pool1_out"]),
                    "kernel_shape",
                    vec![2, 2],
                ),
                "strides",
                vec![2, 2],
            ),
            with_ints(
                with_ints(
                    with_ints(
                        node(
                            "Conv",
                            "conv2",
                            &["pool1_out", "conv2_w", "conv2_b"],
                            &["conv2_out"],
                        ),
                        "kernel_shape",
                        vec![3, 3],
                    ),
                    "pads",
                    vec![1, 1, 1, 1],
                ),
                "strides",
                vec![1, 1],
            ),
            node("Relu", "relu2", &["conv2_out"], &["relu2_out"]),
            node("GlobalAveragePool", "gap", &["relu2_out"], &["gap_out"]),
            with_int(
                node("Flatten", "flatten", &["gap_out"], &["flat_out"]),
                "axis",
                1,
            ),
            with_int(
                node(
                    "Gemm",
                    "head",
                    &["flat_out", "head_w", "head_b"],
                    &["embedding"],
                ),
                "transB",
                1,
            ),
        ],
        initializers: vec![
            weights.tensor("conv1_w", vec![8, 3, 3, 3], 0.30),
            weights.tensor("conv1_b", vec![8], 0.05),
            weights.tensor("conv2_w", vec![16, 8, 3, 3], 0.20),
            weights.tensor("conv2_b", vec![16], 0.05),
            weights.tensor("head_w", vec![EMBEDDING_DIM, 16], 0.35),
            weights.tensor("head_b", vec![EMBEDDING_DIM], 0.05),
        ],
        inputs: vec![dynamic_batch("pixels", &[3, INPUT_SIDE, INPUT_SIDE])],
        outputs: vec![dynamic_batch("embedding", &[EMBEDDING_DIM])],
    };

    OnnxModel {
        ir_version: IR_VERSION,
        producer_name: "aura-infer fixtures".to_string(),
        opset: OPSET,
        graph,
    }
}

/// A small scene classifier ending in a softmax.
///
/// `pixels [N, 3, 32, 32] -> scene_probs [N, 6]`. The softmax matters: it is what
/// makes the output a confidence, and invariant 2 says every AI decision carries
/// one.
#[must_use]
pub fn tiny_scene() -> OnnxModel {
    let mut weights = Weights::new(0x00B0_9876_5432_1FED);

    let graph = Graph {
        name: SCENE_MODEL.to_string(),
        nodes: vec![
            with_ints(
                with_ints(
                    with_ints(
                        node(
                            "Conv",
                            "stem",
                            &["pixels", "stem_w", "stem_b"],
                            &["stem_out"],
                        ),
                        "kernel_shape",
                        vec![3, 3],
                    ),
                    "pads",
                    vec![1, 1, 1, 1],
                ),
                "strides",
                vec![2, 2],
            ),
            node("Relu", "act", &["stem_out"], &["act_out"]),
            node("GlobalAveragePool", "gap", &["act_out"], &["gap_out"]),
            with_int(
                node("Flatten", "flatten", &["gap_out"], &["flat_out"]),
                "axis",
                1,
            ),
            with_int(
                node(
                    "Gemm",
                    "head",
                    &["flat_out", "head_w", "head_b"],
                    &["logits"],
                ),
                "transB",
                1,
            ),
            with_int(
                node("Softmax", "softmax", &["logits"], &["scene_probs"]),
                "axis",
                -1,
            ),
        ],
        initializers: vec![
            weights.tensor("stem_w", vec![8, 3, 3, 3], 0.30),
            weights.tensor("stem_b", vec![8], 0.05),
            weights.tensor("head_w", vec![SCENE_CLASSES, 8], 0.40),
            weights.tensor("head_b", vec![SCENE_CLASSES], 0.05),
        ],
        inputs: vec![dynamic_batch("pixels", &[3, INPUT_SIDE, INPUT_SIDE])],
        outputs: vec![dynamic_batch("scene_probs", &[SCENE_CLASSES])],
    };

    OnnxModel {
        ir_version: IR_VERSION,
        producer_name: "aura-infer fixtures".to_string(),
        opset: OPSET,
        graph,
    }
}

/// The phase 05 perceptual embedding model.
///
/// `pixels [N, 3, 384, 384] -> embedding [N, 512]` through a strided stem, two
/// more strided convolutions, global average pooling and a two-layer projection
/// head - the shape section 6.1 of PHASE-05 describes, three orders of magnitude
/// smaller than the ViT-B/16 it stands in for.
///
/// **This is a placeholder and the exit report says so.** There is no labelled
/// wedding data in this repository and no GPU backend to train a domain-adaptation
/// head against, so the alternative to a placeholder is not a real model - it is a
/// real-looking model with numbers that describe nothing. What is real is
/// everything the placeholder exercises: the 384 px preprocessing, the 512-d
/// output, the batching, the fp16 storage, the index and the eval harness.
///
/// Two things are deliberately faithful to the real design rather than convenient:
///
/// * **The stem is strided by four.** A real vision transformer patchifies `16x16`; a convolutional
///   stand-in that ran at full 384 px resolution would spend ninety per cent of its
///   arithmetic in the first layer and tell a false story about where the cost of
///   an embedding model is.
/// * **The head is two layers with a rectifier between them**, so the exported
///   graph has the same fusion boundary the trained head will have and
///   `export.py` has something to match.
///
/// L2 normalisation is *not* in the graph: the interpreter has no reduction
/// operator (ADR-0007), so it happens in `aura_vision::embed::model` and is
/// covered by `PREPROCESS_VER`. See ADR-0011 section 4.
#[must_use]
pub fn wedding_embedding() -> OnnxModel {
    let mut weights = Weights::new(0x00C0_0505_1234_5678);

    let strided_conv = |name: &str, inputs: &[&str], outputs: &[&str], stride: i64| -> Node {
        with_ints(
            with_ints(
                with_ints(
                    node("Conv", name, inputs, outputs),
                    "kernel_shape",
                    vec![3, 3],
                ),
                "pads",
                vec![1, 1, 1, 1],
            ),
            "strides",
            vec![stride, stride],
        )
    };

    let graph = Graph {
        name: WEDDING_EMBEDDING_MODEL.to_string(),
        nodes: vec![
            // 384 -> 96
            strided_conv("stem", &["pixels", "stem_w", "stem_b"], &["stem_out"], 4),
            node("Relu", "stem_act", &["stem_out"], &["stem_relu"]),
            // 96 -> 48
            with_ints(
                with_ints(
                    node("MaxPool", "pool", &["stem_relu"], &["pool_out"]),
                    "kernel_shape",
                    vec![2, 2],
                ),
                "strides",
                vec![2, 2],
            ),
            // 48 -> 24
            strided_conv(
                "block1",
                &["pool_out", "block1_w", "block1_b"],
                &["block1_out"],
                2,
            ),
            node("Relu", "block1_act", &["block1_out"], &["block1_relu"]),
            // 24 -> 12
            strided_conv(
                "block2",
                &["block1_relu", "block2_w", "block2_b"],
                &["block2_out"],
                2,
            ),
            node("Relu", "block2_act", &["block2_out"], &["block2_relu"]),
            node("GlobalAveragePool", "gap", &["block2_relu"], &["gap_out"]),
            with_int(
                node("Flatten", "flatten", &["gap_out"], &["features"]),
                "axis",
                1,
            ),
            with_int(
                node(
                    "Gemm",
                    "trunk",
                    &["features", "trunk_w", "trunk_b"],
                    &["trunk_out"],
                ),
                "transB",
                1,
            ),
            node("Relu", "trunk_act", &["trunk_out"], &["trunk_relu"]),
            with_int(
                node(
                    "Gemm",
                    "project",
                    &["trunk_relu", "project_w", "project_b"],
                    &["embedding"],
                ),
                "transB",
                1,
            ),
        ],
        initializers: vec![
            weights.tensor("stem_w", vec![32, 3, 3, 3], 0.25),
            weights.tensor("stem_b", vec![32], 0.05),
            weights.tensor("block1_w", vec![64, 32, 3, 3], 0.12),
            weights.tensor("block1_b", vec![64], 0.05),
            weights.tensor("block2_w", vec![96, 64, 3, 3], 0.10),
            weights.tensor("block2_b", vec![96], 0.05),
            weights.tensor("trunk_w", vec![WEDDING_TRUNK_DIM, 96], 0.18),
            weights.tensor("trunk_b", vec![WEDDING_TRUNK_DIM], 0.05),
            weights.tensor(
                "project_w",
                vec![WEDDING_EMBEDDING_DIM, WEDDING_TRUNK_DIM],
                0.14,
            ),
            weights.tensor("project_b", vec![WEDDING_EMBEDDING_DIM], 0.05),
        ],
        inputs: vec![dynamic_batch(
            "pixels",
            &[3, WEDDING_INPUT_SIDE, WEDDING_INPUT_SIDE],
        )],
        outputs: vec![dynamic_batch("embedding", &[WEDDING_EMBEDDING_DIM])],
    };

    OnnxModel {
        ir_version: IR_VERSION,
        producer_name: "aura-infer fixtures".to_string(),
        opset: OPSET,
        graph,
    }
}

/// A deterministic 384 px input batch for the phase 05 model.
///
/// Structured rather than random: each image is a smooth two-axis gradient with a
/// per-image phase offset and a per-channel tint, so two images in one batch are
/// genuinely different - a batching bug that reuses one member's tensor produces
/// identical vectors and is caught immediately - while remaining byte-identical on
/// every machine.
#[must_use]
pub fn wedding_sample_input(batch: usize) -> Tensor {
    let side = WEDDING_INPUT_SIDE;
    let mut samples = Vec::with_capacity(batch * 3 * side * side);
    for image in 0..batch {
        for channel in 0..3 {
            for y in 0..side {
                for x in 0..side {
                    let across = x as f32 / side as f32;
                    let down = y as f32 / side as f32;
                    let tint = channel as f32 * 0.11;
                    let offset = image as f32 * 0.17;
                    samples.push((across * 0.6 + down * 0.4 + tint + offset).fract());
                }
            }
        }
    }
    Tensor {
        shape: vec![batch, 3, side, side],
        data: samples,
    }
}

// ---------------------------------------------------------------------------
// PHASE-06. The three face models.
//
// Same standing as `wedding_embedding`: the graphs are real, the operators are the
// ones a trained SCRFD and ArcFace export actually use, the weights are
// deterministic, and **the weights carry no face semantics**. A placeholder
// detector finds nothing in a photograph, which is why phase 06 ships a separate
// deterministic reference detector for its fixtures and records the gap as a
// condition in `docs/progress/PHASE-06-EXIT.md`.
//
// What the graphs *do* prove, and what a hand-waved stub would not: the anchor
// layout, the channel order, the three output strides, the 112 px alignment
// contract, the batch memory ledger at 640 px, and the fp16/int8 variant path.
// ---------------------------------------------------------------------------

/// Name of the face detector.
pub const FACE_DETECT_MODEL: &str = "face_detect";
/// Name of the face recogniser.
pub const FACE_EMBED_MODEL: &str = "face_embed";
/// Name of the face quality head.
pub const FACE_QUALITY_MODEL: &str = "face_quality";

/// Side of the square the detector takes. PHASE-06 section 6.1: 640 px.
pub const FACE_DETECT_INPUT_SIDE: usize = 640;

/// Side of the aligned crop the recogniser and the quality head take.
///
/// 112 px, which is not a free parameter: it is the `ArcFace` reference geometry,
/// and the five reference landmarks in `aura_vision::face::align` are expressed in
/// it. Changing it invalidates every stored template.
pub const FACE_CROP_SIDE: usize = 112;

/// Width of the recognition template. PHASE-06 section 2.1: 512-d `ArcFace` class.
pub const FACE_EMBED_DIM: usize = 512;

/// Width of the trunk between the recognition backbone and its projection.
pub const FACE_TRUNK_DIM: usize = 256;

/// The three output strides, coarsest last.
///
/// Three levels rather than one because a wedding is the pathological case for a
/// single-scale detector: the bride's face at a first look fills a third of the
/// frame and a guest's face in a wide ceremony frame is forty pixels tall, and
/// those two cannot be regressed by the same receptive field.
pub const FACE_DETECT_STRIDES: [usize; 3] = [8, 16, 32];

/// Channels each detection head emits. The layout is the contract:
///
/// | Channels | Meaning |
/// |---|---|
/// | 0 | face objectness logit |
/// | 1 | person objectness logit |
/// | 2..6 | face box, as left/top/right/bottom distances from the anchor centre in stride units |
/// | 6..10 | person box, same encoding |
/// | 10..20 | five landmarks, as x/y offsets from the anchor centre in stride units |
///
/// One tensor per stride rather than three, because the interpreter has no `Split`
/// operator and a fused head is what a real export produces anyway.
/// `aura_vision::face::detect::decode` is the only reader of this layout.
///
/// **Faces and bodies are predicted by the same anchor**, which is why phase 06
/// needs no fourth model and no separate person detector. It is also what gives
/// face-to-body association for free: two boxes from one anchor are one person by
/// construction, and only boxes that came from *different* anchors need the
/// geometric fallback in `aura_vision::face::person`. A person channel that fires
/// where the face channel does not is the back-of-head case section 6.1 asks for.
pub const FACE_HEAD_CHANNELS: usize = 20;

/// How many quality numbers the head predicts: usability, blur, occlusion, pose.
pub const FACE_QUALITY_OUTPUTS: usize = 4;

/// The spatial side of one detection head, for a stride.
#[must_use]
pub const fn face_head_side(stride: usize) -> usize {
    if stride == 0 {
        return 0;
    }
    FACE_DETECT_INPUT_SIDE / stride
}

/// A 3x3 convolution with a stride, padded to keep the arithmetic obvious.
fn conv3(name: &str, inputs: &[&str], outputs: &[&str], stride: i64) -> Node {
    with_ints(
        with_ints(
            with_ints(
                node("Conv", name, inputs, outputs),
                "kernel_shape",
                vec![3, 3],
            ),
            "pads",
            vec![1, 1, 1, 1],
        ),
        "strides",
        vec![stride, stride],
    )
}

/// A 1x1 convolution. What a detection head is.
fn conv1(name: &str, inputs: &[&str], outputs: &[&str]) -> Node {
    with_ints(
        with_ints(
            with_ints(
                node("Conv", name, inputs, outputs),
                "kernel_shape",
                vec![1, 1],
            ),
            "pads",
            vec![0, 0, 0, 0],
        ),
        "strides",
        vec![1, 1],
    )
}

/// A 2x2 max pool, stride 2.
fn pool2(name: &str, input: &str, output: &str) -> Node {
    with_ints(
        with_ints(
            node("MaxPool", name, &[input], &[output]),
            "kernel_shape",
            vec![2, 2],
        ),
        "strides",
        vec![2, 2],
    )
}

/// The face detector: an SCRFD-shaped single-stage network with three heads.
///
/// `pixels [N, 3, 640, 640]` to `head_8 [N, 20, 80, 80]`, `head_16 [N, 20, 40, 40]`
/// and `head_32 [N, 20, 20, 20]`.
///
/// Three things about it are deliberate rather than convenient:
///
/// * **The stem strides by four and pools once.** A detector that ran even one
///   full-width convolution at 640 px would spend most of its arithmetic before it
///   had any features, and would tell a false story about where the cost of a face
///   pass is. The cost here is concentrated at stride 8, which is where a real
///   SCRFD's cost is too.
/// * **No sigmoid in the graph.** Two channels want a logistic and fourteen
///   regressions do not, and separating them needs `Split`, which this build does
///   not implement (ADR-0007). The logistic is applied per channel in
///   `aura_vision::face::detect`, which is where the anchor decode already lives.
/// * **Landmark channels are last.** Putting them after both boxes means a future
///   model that drops them is a channel-count change the decoder refuses rather
///   than a silent reinterpretation of a box as a landmark.
#[must_use]
pub fn face_detect() -> OnnxModel {
    let mut weights = Weights::new(0x00FA_CE01_D37E_C701);
    let side = FACE_DETECT_INPUT_SIDE;

    let graph = Graph {
        name: FACE_DETECT_MODEL.to_string(),
        nodes: vec![
            // 640 -> 160
            conv3("stem", &["pixels", "stem_w", "stem_b"], &["stem_out"], 4),
            node("Relu", "stem_act", &["stem_out"], &["stem_relu"]),
            // 160 -> 80. Stride 8 from here on.
            pool2("pool", "stem_relu", "pool_out"),
            conv3("c1", &["pool_out", "c1_w", "c1_b"], &["c1_out"], 1),
            node("Relu", "c1_act", &["c1_out"], &["c1_relu"]),
            conv3("c2", &["c1_relu", "c2_w", "c2_b"], &["c2_out"], 1),
            node("Relu", "c2_act", &["c2_out"], &["p8"]),
            conv1("head8", &["p8", "h8_w", "h8_b"], &["head_8"]),
            // 80 -> 40. Stride 16.
            conv3("d16", &["p8", "d16_w", "d16_b"], &["d16_out"], 2),
            node("Relu", "d16_act", &["d16_out"], &["p16"]),
            conv1("head16", &["p16", "h16_w", "h16_b"], &["head_16"]),
            // 40 -> 20. Stride 32.
            conv3("d32", &["p16", "d32_w", "d32_b"], &["d32_out"], 2),
            node("Relu", "d32_act", &["d32_out"], &["p32"]),
            conv1("head32", &["p32", "h32_w", "h32_b"], &["head_32"]),
        ],
        initializers: vec![
            weights.tensor("stem_w", vec![16, 3, 3, 3], 0.28),
            weights.tensor("stem_b", vec![16], 0.05),
            weights.tensor("c1_w", vec![32, 16, 3, 3], 0.16),
            weights.tensor("c1_b", vec![32], 0.05),
            weights.tensor("c2_w", vec![32, 32, 3, 3], 0.12),
            weights.tensor("c2_b", vec![32], 0.05),
            weights.tensor("h8_w", vec![FACE_HEAD_CHANNELS, 32, 1, 1], 0.10),
            weights.tensor("h8_b", vec![FACE_HEAD_CHANNELS], 0.02),
            weights.tensor("d16_w", vec![48, 32, 3, 3], 0.12),
            weights.tensor("d16_b", vec![48], 0.05),
            weights.tensor("h16_w", vec![FACE_HEAD_CHANNELS, 48, 1, 1], 0.10),
            weights.tensor("h16_b", vec![FACE_HEAD_CHANNELS], 0.02),
            weights.tensor("d32_w", vec![64, 48, 3, 3], 0.12),
            weights.tensor("d32_b", vec![64], 0.05),
            weights.tensor("h32_w", vec![FACE_HEAD_CHANNELS, 64, 1, 1], 0.10),
            weights.tensor("h32_b", vec![FACE_HEAD_CHANNELS], 0.02),
        ],
        inputs: vec![dynamic_batch("pixels", &[3, side, side])],
        outputs: vec![
            dynamic_batch(
                "head_8",
                &[FACE_HEAD_CHANNELS, face_head_side(8), face_head_side(8)],
            ),
            dynamic_batch(
                "head_16",
                &[FACE_HEAD_CHANNELS, face_head_side(16), face_head_side(16)],
            ),
            dynamic_batch(
                "head_32",
                &[FACE_HEAD_CHANNELS, face_head_side(32), face_head_side(32)],
            ),
        ],
    };

    OnnxModel {
        ir_version: IR_VERSION,
        producer_name: "aura-infer fixtures".to_string(),
        opset: OPSET,
        graph,
    }
}

/// The face recogniser: an aligned 112 px crop to a 512-d template.
///
/// `pixels [N, 3, 112, 112] -> embedding [N, 512]`.
///
/// L2 normalisation is not in the graph, for the same reason it is not in
/// `wedding_embedding`: the interpreter has no reduction operator. It happens in
/// `aura_vision::face::embed` and is covered by that module's preprocessing
/// version, so a template that has been through the runtime and one that has been
/// through the store agree bit for bit.
#[must_use]
pub fn face_embed() -> OnnxModel {
    let mut weights = Weights::new(0x00FA_CE02_A2C1_FACE);
    let side = FACE_CROP_SIDE;

    let graph = Graph {
        name: FACE_EMBED_MODEL.to_string(),
        nodes: vec![
            // 112 -> 56
            conv3("stem", &["pixels", "stem_w", "stem_b"], &["stem_out"], 2),
            node("Relu", "stem_act", &["stem_out"], &["stem_relu"]),
            // 56 -> 28
            pool2("pool", "stem_relu", "pool_out"),
            // 28 -> 14
            conv3("b1", &["pool_out", "b1_w", "b1_b"], &["b1_out"], 2),
            node("Relu", "b1_act", &["b1_out"], &["b1_relu"]),
            // 14 -> 7
            conv3("b2", &["b1_relu", "b2_w", "b2_b"], &["b2_out"], 2),
            node("Relu", "b2_act", &["b2_out"], &["b2_relu"]),
            node("GlobalAveragePool", "gap", &["b2_relu"], &["gap_out"]),
            with_int(
                node("Flatten", "flatten", &["gap_out"], &["features"]),
                "axis",
                1,
            ),
            with_int(
                node(
                    "Gemm",
                    "trunk",
                    &["features", "trunk_w", "trunk_b"],
                    &["trunk_out"],
                ),
                "transB",
                1,
            ),
            node("Relu", "trunk_act", &["trunk_out"], &["trunk_relu"]),
            with_int(
                node(
                    "Gemm",
                    "project",
                    &["trunk_relu", "project_w", "project_b"],
                    &["embedding"],
                ),
                "transB",
                1,
            ),
        ],
        initializers: vec![
            weights.tensor("stem_w", vec![24, 3, 3, 3], 0.26),
            weights.tensor("stem_b", vec![24], 0.05),
            weights.tensor("b1_w", vec![48, 24, 3, 3], 0.14),
            weights.tensor("b1_b", vec![48], 0.05),
            weights.tensor("b2_w", vec![96, 48, 3, 3], 0.11),
            weights.tensor("b2_b", vec![96], 0.05),
            weights.tensor("trunk_w", vec![FACE_TRUNK_DIM, 96], 0.17),
            weights.tensor("trunk_b", vec![FACE_TRUNK_DIM], 0.05),
            weights.tensor("project_w", vec![FACE_EMBED_DIM, FACE_TRUNK_DIM], 0.13),
            weights.tensor("project_b", vec![FACE_EMBED_DIM], 0.05),
        ],
        inputs: vec![dynamic_batch("pixels", &[3, side, side])],
        outputs: vec![dynamic_batch("embedding", &[FACE_EMBED_DIM])],
    };

    OnnxModel {
        ir_version: IR_VERSION,
        producer_name: "aura-infer fixtures".to_string(),
        opset: OPSET,
        graph,
    }
}

/// The face quality head: an aligned crop to four independent probabilities.
///
/// `pixels [N, 3, 112, 112] -> quality [N, 4]`, in the order usability, blur,
/// occlusion, pose.
///
/// The sigmoid **is** in this graph, unlike the detector's, because all four
/// outputs are independent probabilities and none of them is a regression. That is
/// the whole difference: an activation belongs in a graph when it applies to every
/// channel of a tensor, and in the decoder when it does not.
#[must_use]
pub fn face_quality() -> OnnxModel {
    let mut weights = Weights::new(0x00FA_CE03_9A11_7ADD);
    let side = FACE_CROP_SIDE;

    let graph = Graph {
        name: FACE_QUALITY_MODEL.to_string(),
        nodes: vec![
            // 112 -> 56
            conv3("stem", &["pixels", "stem_w", "stem_b"], &["stem_out"], 2),
            node("Relu", "stem_act", &["stem_out"], &["stem_relu"]),
            // 56 -> 28
            pool2("pool", "stem_relu", "pool_out"),
            // 28 -> 14
            conv3("b1", &["pool_out", "b1_w", "b1_b"], &["b1_out"], 2),
            node("Relu", "b1_act", &["b1_out"], &["b1_relu"]),
            node("GlobalAveragePool", "gap", &["b1_relu"], &["gap_out"]),
            with_int(
                node("Flatten", "flatten", &["gap_out"], &["features"]),
                "axis",
                1,
            ),
            with_int(
                node(
                    "Gemm",
                    "trunk",
                    &["features", "trunk_w", "trunk_b"],
                    &["trunk_out"],
                ),
                "transB",
                1,
            ),
            node("Relu", "trunk_act", &["trunk_out"], &["trunk_relu"]),
            with_int(
                node(
                    "Gemm",
                    "head",
                    &["trunk_relu", "head_w", "head_b"],
                    &["logits"],
                ),
                "transB",
                1,
            ),
            node("Sigmoid", "squash", &["logits"], &["quality"]),
        ],
        initializers: vec![
            weights.tensor("stem_w", vec![16, 3, 3, 3], 0.24),
            weights.tensor("stem_b", vec![16], 0.05),
            weights.tensor("b1_w", vec![32, 16, 3, 3], 0.15),
            weights.tensor("b1_b", vec![32], 0.05),
            weights.tensor("trunk_w", vec![32, 32], 0.20),
            weights.tensor("trunk_b", vec![32], 0.05),
            weights.tensor("head_w", vec![FACE_QUALITY_OUTPUTS, 32], 0.22),
            weights.tensor("head_b", vec![FACE_QUALITY_OUTPUTS], 0.05),
        ],
        inputs: vec![dynamic_batch("pixels", &[3, side, side])],
        outputs: vec![dynamic_batch("quality", &[FACE_QUALITY_OUTPUTS])],
    };

    OnnxModel {
        ir_version: IR_VERSION,
        producer_name: "aura-infer fixtures".to_string(),
        opset: OPSET,
        graph,
    }
}

/// A deterministic 640 px detector input batch.
///
/// Two blobs per image at known positions, on a smooth background, so that a
/// detector head which has learned anything at all responds differently at
/// different anchors, and a batching bug that reuses one member's tensor produces
/// identical head tensors and is caught immediately.
#[must_use]
pub fn face_detect_sample_input(batch: usize) -> Tensor {
    let side = FACE_DETECT_INPUT_SIDE;
    let mut samples = Vec::with_capacity(batch * 3 * side * side);
    for image in 0..batch {
        // Two centres per image, moved by the image index so no two members agree.
        let centres = [
            (0.30 + 0.05 * image as f32, 0.35),
            (0.70, 0.55 - 0.04 * image as f32),
        ];
        for channel in 0..3 {
            for y in 0..side {
                for x in 0..side {
                    let across = x as f32 / side as f32;
                    let down = y as f32 / side as f32;
                    let mut value = across * 0.25 + down * 0.15 + channel as f32 * 0.08;
                    for (cx, cy) in centres {
                        let dx = across - cx;
                        let dy = down - cy;
                        let radius = (dx * dx + dy * dy).sqrt();
                        if radius < 0.06 {
                            value += 0.55 * (1.0 - radius / 0.06);
                        }
                    }
                    samples.push(value.clamp(0.0, 1.0));
                }
            }
        }
    }
    Tensor {
        shape: vec![batch, 3, side, side],
        data: samples,
    }
}

/// A deterministic 112 px aligned-crop batch, for the recogniser and the quality
/// head.
#[must_use]
pub fn face_crop_sample_input(batch: usize) -> Tensor {
    let side = FACE_CROP_SIDE;
    let mut samples = Vec::with_capacity(batch * 3 * side * side);
    for image in 0..batch {
        for channel in 0..3 {
            for y in 0..side {
                for x in 0..side {
                    let across = x as f32 / side as f32;
                    let down = y as f32 / side as f32;
                    let tint = channel as f32 * 0.09;
                    let offset = image as f32 * 0.19;
                    samples.push((across * 0.5 + down * 0.5 + tint + offset).fract());
                }
            }
        }
    }
    Tensor {
        shape: vec![batch, 3, side, side],
        data: samples,
    }
}

/// A model whose only node is an operator this build refuses.
///
/// The negative fixture for `AURA-ML-5006`. Without one, the refusal path is
/// asserted by reading the code rather than by running it.
#[must_use]
pub fn unsupported_operator_model() -> OnnxModel {
    let graph = Graph {
        name: "unsupported".to_string(),
        nodes: vec![node("Einsum", "exotic", &["pixels"], &["out"])],
        initializers: Vec::new(),
        inputs: vec![dynamic_batch("pixels", &[3, INPUT_SIDE, INPUT_SIDE])],
        outputs: vec![dynamic_batch("out", &[EMBEDDING_DIM])],
    };
    OnnxModel {
        ir_version: IR_VERSION,
        producer_name: "aura-infer fixtures".to_string(),
        opset: OPSET,
        graph,
    }
}

/// A deterministic input batch, the same on every machine.
///
/// A smooth gradient plus a per-image offset, so different batch members produce
/// different outputs and a batching bug that reuses one member's tensor shows up
/// immediately.
#[must_use]
pub fn sample_input(batch: usize) -> Tensor {
    let mut samples = Vec::with_capacity(batch * 3 * INPUT_SIDE * INPUT_SIDE);
    for image in 0..batch {
        for channel in 0..3 {
            for y in 0..INPUT_SIDE {
                for x in 0..INPUT_SIDE {
                    let across = x as f32 / INPUT_SIDE as f32;
                    let down = y as f32 / INPUT_SIDE as f32;
                    let tint = channel as f32 * 0.13;
                    let offset = image as f32 * 0.07;
                    samples.push(((across + down) * 0.5 + tint + offset).fract());
                }
            }
        }
    }
    Tensor {
        shape: vec![batch, 3, INPUT_SIDE, INPUT_SIDE],
        data: samples,
    }
}

// ---------------------------------------------------------------------------
// PHASE-07. The two heads that read a wedding as a story.
//
// Both are *adapter* models rather than backbones, and that is the whole design
// of section 6.1: the trunk is the phase 05 embedding, frozen, and a scene head
// is a small trainable adapter on top of it. So neither of these takes pixels.
// They take a 512-d vector that has already been computed for every photograph
// in the project, plus a handful of context numbers, and that is why section 11
// can budget 35 seconds for four thousand images - the expensive part already
// happened.
//
// A consequence worth stating: these two models are versioned against the
// embedding they sit on. `image_scenes.embed_ver` records which trunk produced
// the vector, and a mismatch is `AURA-ML-5022` rather than a plausible wrong
// answer.
// ---------------------------------------------------------------------------

/// Name of the phase 07 multi-head scene and attribute classifier.
pub const SCENE_CLASSIFIER_MODEL: &str = "scene_classifier";
/// Name of the phase 07 ritual head.
pub const RITUAL_CLASSIFIER_MODEL: &str = "ritual_classifier";

/// How many context numbers ride alongside the embedding.
///
/// Section 6.1's list: hour-of-day offset from the first frame, flash flag, ISO
/// bucket, face count, dominant-identity presence, and an indoor/outdoor prior
/// from luminance and colour temperature. Sixteen slots for six families of
/// feature, because several of them are one-hot: the ISO bucket is four slots
/// and the hour offset is two - a sine and a cosine, so that 23:00 and 01:00 are
/// close together rather than a day apart.
pub const SCENE_CONTEXT_FEATURES: usize = 16;

/// Width of the scene classifier's input: the embedding plus the context.
pub const SCENE_INPUT_DIM: usize = WEDDING_EMBEDDING_DIM + SCENE_CONTEXT_FEATURES;

/// Width of the trainable adapter between the frozen trunk and the two heads.
pub const SCENE_ADAPTER_DIM: usize = 256;

/// How many scene classes the head emits. `SceneId::CLASSES`.
pub const SCENE_CLASSIFIER_CLASSES: usize = 22;

/// How many attribute probabilities the head emits. `AttrFlags::COUNT`.
pub const SCENE_ATTRIBUTES: usize = 14;

/// How many tradition slots condition the ritual head.
///
/// `aura_cloud::tasks::ALLOWED_TRADITIONS` has eight entries and this is the same
/// eight, one-hot. Section 6.1 calls the ritual head tradition-conditioned: a
/// `saptapadi` and a `communion` are not competing hypotheses about the same
/// photograph, and telling the head which taxonomy is in play is what stops it
/// spending its capacity separating them.
pub const RITUAL_TRADITIONS: usize = 8;

/// Width of the ritual head's input.
pub const RITUAL_INPUT_DIM: usize =
    WEDDING_EMBEDDING_DIM + SCENE_CONTEXT_FEATURES + RITUAL_TRADITIONS;

/// Width of the ritual head's adapter.
pub const RITUAL_ADAPTER_DIM: usize = 192;

/// How many ritual slots the head emits.
///
/// **The slot IS the `RitualId` authored in the taxonomy file**, which is the
/// reason ADR-0015 insists the id is written down rather than derived from file
/// order: renumbering a rite would relabel a model output. Slot 0 is
/// `RitualId::NONE`, which is the abstention, so a head that is unsure answers
/// "no ritual" through the same softmax as every other answer rather than
/// through a threshold bolted on outside it.
///
/// 160 covers the five shipped traditions' reserved ranges - 10 to 159 - with the
/// next tradition starting at 160, at which point this constant and the head both
/// change. That is a retrain, which is correct.
pub const RITUAL_SLOTS: usize = 160;

/// The multi-head scene and attribute classifier.
///
/// `features [N, 528] -> scene_probs [N, 22]`, `attr_probs [N, 14]`.
///
/// Two heads on one adapter, exactly section 6.1: a 22-way softmax over scenes
/// and fourteen independent sigmoids over attributes. Independent sigmoids and
/// not a second softmax, because a frame can be indoors *and* backlit *and* have
/// a crowd, and a mutually exclusive encoding would force the model to choose
/// between three true things.
///
/// Both activations are **in the graph**, by the rule [`face_quality`] states: an
/// activation belongs in a graph when it applies to every channel of a tensor,
/// and in the decoder when it does not. A softmax over all 22 scene classes and a
/// sigmoid over all 14 attributes both qualify. The abstention is *not* in the
/// graph - `SceneId::Unknown` is a decision the decoder makes from the top-1
/// margin, and a model cannot be trained to say "I am not sure" through a slot
/// that competes with the classes it is unsure between.
#[must_use]
pub fn scene_classifier() -> OnnxModel {
    let mut weights = Weights::new(0x0000_5CE7_E01A_BE15);

    let graph = Graph {
        name: SCENE_CLASSIFIER_MODEL.to_string(),
        nodes: vec![
            with_int(
                node(
                    "Gemm",
                    "adapter",
                    &["features", "adapter_w", "adapter_b"],
                    &["adapter_out"],
                ),
                "transB",
                1,
            ),
            node("Relu", "adapter_act", &["adapter_out"], &["adapter_relu"]),
            with_int(
                node(
                    "Gemm",
                    "scene_head",
                    &["adapter_relu", "scene_w", "scene_b"],
                    &["scene_logits"],
                ),
                "transB",
                1,
            ),
            with_int(
                node("Softmax", "scene_norm", &["scene_logits"], &["scene_probs"]),
                "axis",
                -1,
            ),
            with_int(
                node(
                    "Gemm",
                    "attr_head",
                    &["adapter_relu", "attr_w", "attr_b"],
                    &["attr_logits"],
                ),
                "transB",
                1,
            ),
            node("Sigmoid", "attr_squash", &["attr_logits"], &["attr_probs"]),
        ],
        initializers: vec![
            weights.tensor("adapter_w", vec![SCENE_ADAPTER_DIM, SCENE_INPUT_DIM], 0.06),
            weights.tensor("adapter_b", vec![SCENE_ADAPTER_DIM], 0.02),
            weights.tensor(
                "scene_w",
                vec![SCENE_CLASSIFIER_CLASSES, SCENE_ADAPTER_DIM],
                0.09,
            ),
            weights.tensor("scene_b", vec![SCENE_CLASSIFIER_CLASSES], 0.02),
            weights.tensor("attr_w", vec![SCENE_ATTRIBUTES, SCENE_ADAPTER_DIM], 0.09),
            weights.tensor("attr_b", vec![SCENE_ATTRIBUTES], 0.02),
        ],
        inputs: vec![dynamic_batch("features", &[SCENE_INPUT_DIM])],
        outputs: vec![
            dynamic_batch("scene_probs", &[SCENE_CLASSIFIER_CLASSES]),
            dynamic_batch("attr_probs", &[SCENE_ATTRIBUTES]),
        ],
    };

    OnnxModel {
        ir_version: IR_VERSION,
        producer_name: "aura-infer fixtures".to_string(),
        opset: OPSET,
        graph,
    }
}

/// The tradition-conditioned ritual head.
///
/// `features [N, 536] -> ritual_probs [N, 160]`.
///
/// One softmax over every rite in every shipped taxonomy plus slot 0 for "no
/// ritual". Section 6.1 asks for a "don't guess" abstain threshold and slot 0 is
/// half of it; the other half is `ritual::ABSTAIN_MARGIN` in the decoder, because
/// a head that is split evenly between `saptapadi_pheras` and `saat_phera` has
/// not abstained - it has told you the tradition and not the rite, and that is a
/// different answer from silence.
#[must_use]
pub fn ritual_classifier() -> OnnxModel {
    let mut weights = Weights::new(0x0000_217A_11A5_10B5);

    let graph = Graph {
        name: RITUAL_CLASSIFIER_MODEL.to_string(),
        nodes: vec![
            with_int(
                node(
                    "Gemm",
                    "adapter",
                    &["features", "adapter_w", "adapter_b"],
                    &["adapter_out"],
                ),
                "transB",
                1,
            ),
            node("Relu", "adapter_act", &["adapter_out"], &["adapter_relu"]),
            with_int(
                node(
                    "Gemm",
                    "ritual_head",
                    &["adapter_relu", "ritual_w", "ritual_b"],
                    &["ritual_logits"],
                ),
                "transB",
                1,
            ),
            with_int(
                node(
                    "Softmax",
                    "ritual_norm",
                    &["ritual_logits"],
                    &["ritual_probs"],
                ),
                "axis",
                -1,
            ),
        ],
        initializers: vec![
            weights.tensor(
                "adapter_w",
                vec![RITUAL_ADAPTER_DIM, RITUAL_INPUT_DIM],
                0.06,
            ),
            weights.tensor("adapter_b", vec![RITUAL_ADAPTER_DIM], 0.02),
            weights.tensor("ritual_w", vec![RITUAL_SLOTS, RITUAL_ADAPTER_DIM], 0.07),
            weights.tensor("ritual_b", vec![RITUAL_SLOTS], 0.02),
        ],
        inputs: vec![dynamic_batch("features", &[RITUAL_INPUT_DIM])],
        outputs: vec![dynamic_batch("ritual_probs", &[RITUAL_SLOTS])],
    };

    OnnxModel {
        ir_version: IR_VERSION,
        producer_name: "aura-infer fixtures".to_string(),
        opset: OPSET,
        graph,
    }
}

/// A deterministic batch of scene-classifier features, for benchmarks and parity.
#[must_use]
pub fn scene_sample_input(batch: usize) -> Tensor {
    let mut weights = Weights::new(0x0000_5CE7_5A11_9127);
    let samples = (0..batch * SCENE_INPUT_DIM)
        .map(|_| weights.next(0.5) + 0.5)
        .collect();
    Tensor {
        shape: vec![batch, SCENE_INPUT_DIM],
        data: samples,
    }
}

/// A deterministic batch of ritual-head features.
#[must_use]
pub fn ritual_sample_input(batch: usize) -> Tensor {
    let mut weights = Weights::new(0x0000_217A_5A11_9127);
    let samples = (0..batch * RITUAL_INPUT_DIM)
        .map(|_| weights.next(0.5) + 0.5)
        .collect();
    Tensor {
        shape: vec![batch, RITUAL_INPUT_DIM],
        data: samples,
    }
}
