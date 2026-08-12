//! The ONNX subset: the round trip, the refusals, and the operator arithmetic.
//!
//! The round trip is the load-bearing test in this file. With no third-party
//! runtime in the build, "the reader is correct" cannot be shown against someone
//! else's implementation, so it is shown against ours going the other way: every
//! model this runtime can execute survives `parse(serialise(model))` unchanged,
//! and a mistake in a field number desynchronises the pair immediately.

use aura_infer::contract::infer::{Precision, Tensor};
use aura_infer::onnx::model::{
    AttrValue, Attribute, Graph, InitTensor, Node, OnnxModel, TensorData, ValueInfo, IR_VERSION,
    OPSET,
};
use aura_infer::onnx::{fixtures, parse, serialise, Executable};

fn value(name: &str, shape: &[usize]) -> ValueInfo {
    ValueInfo {
        name: name.to_string(),
        shape: shape.iter().map(|dim| Some(*dim)).collect(),
    }
}

fn single_node_model(node: Node, inputs: Vec<ValueInfo>, outputs: Vec<ValueInfo>) -> OnnxModel {
    OnnxModel {
        ir_version: IR_VERSION,
        producer_name: "test".to_string(),
        opset: OPSET,
        graph: Graph {
            name: "single".to_string(),
            nodes: vec![node],
            initializers: Vec::new(),
            inputs,
            outputs,
        },
    }
}

fn node(op_type: &str, inputs: &[&str], outputs: &[&str], attributes: Vec<Attribute>) -> Node {
    Node {
        name: format!("{op_type}_node"),
        op_type: op_type.to_string(),
        inputs: inputs.iter().map(|s| (*s).to_string()).collect(),
        outputs: outputs.iter().map(|s| (*s).to_string()).collect(),
        attributes,
    }
}

fn run_single(model: &OnnxModel, input: &Tensor) -> Vec<Tensor> {
    let executable = Executable::compile(model, Precision::Fp32).expect("compile");
    executable.run(&[input.view()]).expect("run")
}

#[test]
fn every_shipped_model_survives_a_write_and_read_round_trip() {
    for model in [fixtures::tiny_embedding(), fixtures::tiny_scene()] {
        let bytes = serialise(&model);
        let reparsed = parse(&bytes).expect("parse");
        assert_eq!(reparsed, model, "{} did not round trip", model.graph.name);
    }
}

#[test]
fn a_round_tripped_model_produces_identical_numbers() {
    let original = fixtures::tiny_embedding();
    let reparsed = parse(&serialise(&original)).expect("parse");
    let input = fixtures::sample_input(2);

    let first = run_single(&original, &input);
    let second = run_single(&reparsed, &input);
    assert_eq!(first, second);
}

#[test]
fn an_unsupported_operator_is_refused_at_load_and_names_itself() {
    let err = Executable::compile(&fixtures::unsupported_operator_model(), Precision::Fp32)
        .expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5006");
    assert!(
        err.context
            .iter()
            .any(|(key, value)| key == "op" && value == "Einsum"),
        "the refusal must name the operator: {:?}",
        err.context
    );
}

#[test]
fn a_model_from_a_future_opset_is_refused() {
    let mut model = fixtures::tiny_embedding();
    model.opset = OPSET + 1;
    let err = Executable::compile(&model, Precision::Fp32).expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5006");
}

#[test]
fn a_truncated_file_is_an_error_rather_than_a_panic() {
    let bytes = serialise(&fixtures::tiny_scene());
    for cut in [1usize, 8, 64, bytes.len() / 2, bytes.len() - 1] {
        let partial: Vec<u8> = bytes.iter().take(cut).copied().collect();
        match parse(&partial) {
            Ok(model) => {
                // A prefix that happens to be a valid message must still fail to
                // compile into something runnable, or we would run half a graph.
                let compiled = Executable::compile(&model, Precision::Fp32);
                assert!(
                    compiled.is_err() || model.graph.nodes.len() < 6,
                    "a {cut} byte prefix compiled into a complete graph"
                );
            }
            Err(err) => assert_eq!(err.code.0, "AURA-ML-5010"),
        }
    }
}

#[test]
fn a_graph_whose_input_is_never_produced_is_refused() {
    let model = single_node_model(
        node("Relu", &["missing"], &["out"], Vec::new()),
        vec![value("pixels", &[1, 2])],
        vec![value("out", &[1, 2])],
    );
    let err = Executable::compile(&model, Precision::Fp32).expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5010");
}

#[test]
fn convolution_matches_a_hand_computed_result() {
    // A 1x1 kernel of 2.0 with a bias of 0.5 over a 2x2 input is arithmetic a
    // reader can check without running anything.
    let mut model = single_node_model(
        node(
            "Conv",
            &["pixels", "w", "b"],
            &["out"],
            vec![Attribute {
                name: "kernel_shape".to_string(),
                value: AttrValue::Ints(vec![1, 1]),
            }],
        ),
        vec![value("pixels", &[1, 1, 2, 2])],
        vec![value("out", &[1, 1, 2, 2])],
    );
    model.graph.initializers = vec![
        InitTensor {
            name: "w".to_string(),
            dims: vec![1, 1, 1, 1],
            data: TensorData::Float(vec![2.0]),
        },
        InitTensor {
            name: "b".to_string(),
            dims: vec![1],
            data: TensorData::Float(vec![0.5]),
        },
    ];

    let input = Tensor::new(vec![1, 1, 2, 2], vec![1.0, 2.0, 3.0, 4.0]).expect("input");
    let outputs = run_single(&model, &input);
    assert_eq!(
        outputs.first().map(|t| t.data.clone()),
        Some(vec![2.5, 4.5, 6.5, 8.5])
    );
}

#[test]
fn padding_contributes_zeros_rather_than_edge_samples() {
    let mut model = single_node_model(
        node(
            "Conv",
            &["pixels", "w"],
            &["out"],
            vec![
                Attribute {
                    name: "kernel_shape".to_string(),
                    value: AttrValue::Ints(vec![3, 3]),
                },
                Attribute {
                    name: "pads".to_string(),
                    value: AttrValue::Ints(vec![1, 1, 1, 1]),
                },
            ],
        ),
        vec![value("pixels", &[1, 1, 3, 3])],
        vec![value("out", &[1, 1, 3, 3])],
    );
    model.graph.initializers = vec![InitTensor {
        name: "w".to_string(),
        dims: vec![1, 1, 3, 3],
        data: TensorData::Float(vec![1.0; 9]),
    }];

    let input = Tensor::new(vec![1, 1, 3, 3], vec![1.0; 9]).expect("input");
    let outputs = run_single(&model, &input);
    // The corner sees a 2x2 window of ones; the centre sees all nine.
    let data = outputs.first().map(|t| t.data.clone()).expect("output");
    assert_eq!(data.first().copied(), Some(4.0));
    assert_eq!(data.get(4).copied(), Some(9.0));
}

#[test]
fn softmax_is_a_probability_distribution_even_on_large_logits() {
    let model = single_node_model(
        node(
            "Softmax",
            &["logits"],
            &["probs"],
            vec![Attribute {
                name: "axis".to_string(),
                value: AttrValue::Int(-1),
            }],
        ),
        vec![value("logits", &[1, 3])],
        vec![value("probs", &[1, 3])],
    );

    let input = Tensor::new(vec![1, 3], vec![90.0, 89.0, 1.0]).expect("input");
    let outputs = run_single(&model, &input);
    let data = outputs.first().map(|t| t.data.clone()).expect("output");
    let total: f32 = data.iter().sum();
    assert!(
        (total - 1.0).abs() < 1e-5,
        "probabilities summed to {total}"
    );
    assert!(data.iter().all(|value| value.is_finite()));
}

#[test]
fn reshape_refuses_to_change_the_sample_count() {
    let mut model = single_node_model(
        node("Reshape", &["pixels", "shape"], &["out"], Vec::new()),
        vec![value("pixels", &[1, 4])],
        vec![value("out", &[1, 3])],
    );
    model.graph.initializers = vec![InitTensor {
        name: "shape".to_string(),
        dims: vec![2],
        data: TensorData::Int64(vec![1, 3]),
    }];

    let executable = Executable::compile(&model, Precision::Fp32).expect("compile");
    let input = Tensor::new(vec![1, 4], vec![1.0, 2.0, 3.0, 4.0]).expect("input");
    let err = executable.run(&[input.view()]).expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5010");
}

#[test]
fn an_input_of_the_wrong_shape_is_refused_before_execution() {
    let executable =
        Executable::compile(&fixtures::tiny_embedding(), Precision::Fp32).expect("compile");
    let wrong = Tensor::new(vec![1, 3, 16, 16], vec![0.0; 3 * 16 * 16]).expect("input");
    let err = executable.run(&[wrong.view()]).expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5007");
}
