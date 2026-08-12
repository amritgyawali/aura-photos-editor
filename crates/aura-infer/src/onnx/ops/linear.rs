//! The two matrix operators: `Gemm` for classifier heads, `MatMul` for
//! everything an exporter decides not to fuse into a `Gemm`.

use aura_core::AuraResult;

use crate::contract::infer::Tensor;
use crate::errors::invalid_graph;
use crate::onnx::graph::Value;
use crate::onnx::model::Node;
use crate::onnx::ops::{element_count, float_operand, optional_float, sample};

/// `Gemm`: `alpha * A' * B' + beta * C`, with optional transposes.
///
/// # Errors
///
/// `AURA-ML-5010` when the operands are not two-dimensional or their inner
/// dimensions disagree.
pub(super) fn gemm(node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let left = float_operand(inputs, 0, "Gemm")?;
    let right = float_operand(inputs, 1, "Gemm")?;
    let bias = optional_float(inputs, 2)?;

    let alpha = node.attr_float("alpha", 1.0);
    let beta = node.attr_float("beta", 1.0);
    let trans_a = node.attr_int("transA", 0) != 0;
    let trans_b = node.attr_int("transB", 0) != 0;

    let (a_rows, a_cols) = two_dimensional(&left.shape, "Gemm A")?;
    let (b_rows, b_cols) = two_dimensional(&right.shape, "Gemm B")?;
    let (rows, inner) = if trans_a {
        (a_cols, a_rows)
    } else {
        (a_rows, a_cols)
    };
    let (right_inner, columns) = if trans_b {
        (b_cols, b_rows)
    } else {
        (b_rows, b_cols)
    };
    if inner != right_inner {
        return Err(invalid_graph(format!(
            "Gemm: inner dimensions {inner} and {right_inner} disagree"
        )));
    }

    let mut output = Tensor::zeros(vec![rows, columns]);
    for row in 0..rows {
        for column in 0..columns {
            let mut total = 0.0f32;
            for index in 0..inner {
                let left_offset = if trans_a {
                    index * a_cols + row
                } else {
                    row * a_cols + index
                };
                let right_offset = if trans_b {
                    column * b_cols + index
                } else {
                    index * b_cols + column
                };
                total += sample(left, left_offset) * sample(right, right_offset);
            }
            let mut value = alpha * total;
            if let Some(bias) = bias {
                value += beta * broadcast_c(bias, row, column, columns);
            }
            if let Some(slot) = output.data.get_mut(row * columns + column) {
                *slot = value;
            }
        }
    }
    Ok(vec![Value::Float(output)])
}

/// `MatMul` over two- and higher-dimensional operands.
///
/// Higher ranks are handled by iterating the leading dimensions of the left
/// operand and broadcasting a two-dimensional right operand across them, which is
/// the only batched form our exporter produces.
///
/// # Errors
///
/// `AURA-ML-5010` when the inner dimensions disagree or the right operand is not
/// two-dimensional.
pub(super) fn matmul(_node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let a = float_operand(inputs, 0, "MatMul")?;
    let b = float_operand(inputs, 1, "MatMul")?;

    let (b_rows, b_cols) = two_dimensional(&b.shape, "MatMul B")?;
    let (&a_cols, leading) = a
        .shape
        .split_last()
        .ok_or_else(|| invalid_graph("MatMul: left operand has no dimensions".to_string()))?;
    if a_cols != b_rows {
        return Err(invalid_graph(format!(
            "MatMul: inner dimensions {a_cols} and {b_rows} disagree"
        )));
    }

    let rows = element_count(leading);
    let mut shape = leading.to_vec();
    shape.push(b_cols);
    let mut output = Tensor::zeros(shape);

    for row in 0..rows {
        for column in 0..b_cols {
            let mut total = 0.0f32;
            for index in 0..a_cols {
                total += sample(a, row * a_cols + index) * sample(b, index * b_cols + column);
            }
            if let Some(slot) = output.data.get_mut(row * b_cols + column) {
                *slot = total;
            }
        }
    }
    Ok(vec![Value::Float(output)])
}

fn two_dimensional(shape: &[usize], what: &str) -> AuraResult<(usize, usize)> {
    match shape {
        [rows, columns] => Ok((*rows, *columns)),
        other => Err(invalid_graph(format!(
            "{what} must be two-dimensional, got {other:?}"
        ))),
    }
}

/// `C` in a `Gemm` may be a scalar, a row, a column or the full matrix.
fn broadcast_c(c: &Tensor, row: usize, column: usize, columns: usize) -> f32 {
    match c.shape.as_slice() {
        [] | [1] => sample(c, 0),
        [n] if *n == columns => sample(c, column),
        [m, 1] if *m > 0 => sample(c, row),
        [_, n] if *n == columns => sample(c, row * columns + column),
        _ => sample(c, (row * columns + column) % c.data.len().max(1)),
    }
}
