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

/// Side of the square input both placeholders take.
pub const INPUT_SIDE: usize = 32;
/// Length of the embedding the first placeholder produces.
pub const EMBEDDING_DIM: usize = 32;
/// Number of classes the second placeholder scores.
pub const SCENE_CLASSES: usize = 6;

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
