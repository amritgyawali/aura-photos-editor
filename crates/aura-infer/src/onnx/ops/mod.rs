//! The operator subset, and the dispatch that refuses everything else.
//!
//! Nineteen operators. That number is a decision, not an accident: it is what the
//! backbones planned for phases 05 to 11 need, and every one of them is
//! implemented here with a fixed accumulation order so that two machines agree
//! bit for bit. Adding an operator is a normal change with tests; running one we
//! have not implemented is not possible, because [`is_supported`] is checked at
//! load and the dispatch below has no fall-through.
//!
//! Activations are rounded to the session's precision after every operator rather
//! than only at the end. A half-precision device rounds at every store, so a
//! parity harness comparing against a runtime that rounded once would flatter us.

mod conv;
mod elementwise;
mod linear;
mod pool;
mod shape;

use aura_core::AuraResult;

use crate::contract::infer::{Precision, Tensor};
use crate::errors::{invalid_graph, op_unsupported};
use crate::onnx::graph::Value;
use crate::onnx::model::Node;
use crate::tensor::apply_precision;

/// Every operator this build can run. Ordered as in ADR-0007.
pub const SUPPORTED: &[&str] = &[
    "Conv",
    "Relu",
    "Clip",
    "MaxPool",
    "AveragePool",
    "GlobalAveragePool",
    "Gemm",
    "MatMul",
    "Add",
    "Mul",
    "Concat",
    "Reshape",
    "Flatten",
    "Transpose",
    "Softmax",
    "Sigmoid",
    "BatchNormalization",
    "QuantizeLinear",
    "DequantizeLinear",
];

/// True when this build implements the operator.
#[must_use]
pub fn is_supported(op_type: &str) -> bool {
    SUPPORTED.contains(&op_type)
}

/// Run one node.
///
/// # Errors
///
/// `AURA-ML-5006` for an operator outside the subset - which should already have
/// been caught at load - and `AURA-ML-5010` when a node's operands do not make
/// sense for it.
pub fn eval(
    node: &Node,
    inputs: &[Option<&Value>],
    precision: Precision,
) -> AuraResult<Vec<Value>> {
    let mut outputs = match node.op_type.as_str() {
        "Conv" => conv::conv(node, inputs)?,
        "MaxPool" => pool::max_pool(node, inputs)?,
        "AveragePool" => pool::average_pool(node, inputs)?,
        "GlobalAveragePool" => pool::global_average_pool(node, inputs)?,
        "Gemm" => linear::gemm(node, inputs)?,
        "MatMul" => linear::matmul(node, inputs)?,
        "Relu" => elementwise::relu(node, inputs)?,
        "Sigmoid" => elementwise::sigmoid(node, inputs)?,
        "Clip" => elementwise::clip(node, inputs)?,
        "Add" => elementwise::add(node, inputs)?,
        "Mul" => elementwise::mul(node, inputs)?,
        "Softmax" => elementwise::softmax(node, inputs)?,
        "BatchNormalization" => elementwise::batch_norm(node, inputs)?,
        "Concat" => shape::concat(node, inputs)?,
        "Reshape" => shape::reshape(node, inputs)?,
        "Flatten" => shape::flatten(node, inputs)?,
        "Transpose" => shape::transpose(node, inputs)?,
        "QuantizeLinear" => shape::quantize_linear(node, inputs)?,
        "DequantizeLinear" => shape::dequantize_linear(node, inputs)?,
        other => {
            return Err(op_unsupported(
                other,
                format!("no implementation for {other}"),
            ))
        }
    };

    for value in &mut outputs {
        if let Value::Float(tensor) = value {
            apply_precision(&mut tensor.data, precision);
        }
    }
    Ok(outputs)
}

/// Fetch a required operand.
pub(crate) fn operand<'a>(
    inputs: &[Option<&'a Value>],
    index: usize,
    op: &str,
) -> AuraResult<&'a Value> {
    inputs
        .get(index)
        .copied()
        .flatten()
        .ok_or_else(|| invalid_graph(format!("{op} needs input {index}")))
}

/// Fetch a required float operand.
pub(crate) fn float_operand<'a>(
    inputs: &[Option<&'a Value>],
    index: usize,
    op: &str,
) -> AuraResult<&'a Tensor> {
    operand(inputs, index, op)?.as_float()
}

/// Fetch an optional float operand.
pub(crate) fn optional_float<'a>(
    inputs: &[Option<&'a Value>],
    index: usize,
) -> AuraResult<Option<&'a Tensor>> {
    match inputs.get(index).copied().flatten() {
        Some(value) => Ok(Some(value.as_float()?)),
        None => Ok(None),
    }
}

/// Read one sample, or zero outside the tensor.
///
/// Reads outside a tensor happen in exactly one place - convolution padding - and
/// there the ONNX default is a zero, so returning zero rather than refusing keeps
/// the padded path branch-free.
pub(crate) fn sample(tensor: &Tensor, offset: usize) -> f32 {
    tensor.data.get(offset).copied().unwrap_or(0.0)
}

/// The product of a shape's dimensions.
pub(crate) fn element_count(shape: &[usize]) -> usize {
    shape.iter().product()
}

/// Split a shape into `[N, C, H, W]`, refusing anything else.
///
/// Only four-dimensional NCHW images reach the spatial operators. A model that
/// wants 1D or 3D convolution is a model this build refuses at load, with the
/// operator named, rather than one it silently mis-executes.
pub(crate) fn nchw(shape: &[usize], op: &str) -> AuraResult<(usize, usize, usize, usize)> {
    match shape {
        [n, c, h, w] => Ok((*n, *c, *h, *w)),
        other => Err(invalid_graph(format!(
            "{op} needs a 4-dimensional NCHW tensor, got {other:?}"
        ))),
    }
}

/// Resolve the padding for a spatial operator.
///
/// Returns `(top, left, bottom, right)`. `auto_pad` is honoured because exporters
/// still emit it: `SAME_UPPER` pads the bottom and right first, `SAME_LOWER` the
/// top and left, `VALID` not at all.
pub(crate) fn resolve_pads(
    node: &Node,
    input: (usize, usize),
    kernel: (usize, usize),
    stride: (usize, usize),
    dilation: (usize, usize),
) -> (usize, usize, usize, usize) {
    let explicit = node.attr_ints("pads");
    if explicit.len() == 4 {
        let get = |index: usize| explicit.get(index).copied().unwrap_or(0).max(0) as usize;
        return (get(0), get(1), get(2), get(3));
    }

    let auto_pad = match node.attr("auto_pad") {
        Some(crate::onnx::model::AttrValue::Str(text)) => text.clone(),
        _ => "NOTSET".to_string(),
    };
    if auto_pad == "NOTSET" || auto_pad == "VALID" {
        return (0, 0, 0, 0);
    }

    let needed = |size: usize, kernel: usize, stride: usize, dilation: usize| -> usize {
        let effective = dilation * (kernel - 1) + 1;
        let out = size.div_ceil(stride);
        (out.saturating_sub(1) * stride + effective).saturating_sub(size)
    };
    let vertical = needed(input.0, kernel.0, stride.0, dilation.0);
    let horizontal = needed(input.1, kernel.1, stride.1, dilation.1);

    if auto_pad == "SAME_LOWER" {
        (
            vertical.div_ceil(2),
            horizontal.div_ceil(2),
            vertical / 2,
            horizontal / 2,
        )
    } else {
        (
            vertical / 2,
            horizontal / 2,
            vertical.div_ceil(2),
            horizontal.div_ceil(2),
        )
    }
}

/// The output size of a spatial dimension.
pub(crate) fn spatial_out(
    input: usize,
    kernel: usize,
    stride: usize,
    dilation: usize,
    pad_before: usize,
    pad_after: usize,
) -> usize {
    let effective = dilation * (kernel.saturating_sub(1)) + 1;
    let padded = input + pad_before + pad_after;
    if padded < effective || stride == 0 {
        return 0;
    }
    (padded - effective) / stride + 1
}

/// A two-element attribute with a default, e.g. `strides` or `dilations`.
pub(crate) fn pair_attr(node: &Node, name: &str, default: usize) -> (usize, usize) {
    let values = node.attr_ints(name);
    match values.as_slice() {
        [vertical, horizontal] => ((*vertical).max(1) as usize, (*horizontal).max(1) as usize),
        [both] => ((*both).max(1) as usize, (*both).max(1) as usize),
        _ => (default, default),
    }
}
