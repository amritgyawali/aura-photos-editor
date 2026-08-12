//! Shape movement and the two quantisation operators.
//!
//! None of these do arithmetic worth mentioning, and all of them are where a
//! subtly wrong index turns a working model into a plausible wrong answer. Each
//! one therefore refuses rather than guesses: a reshape whose element count does
//! not match, a transpose whose permutation is not a permutation, and a concat
//! along an axis the tensors disagree on are all `AURA-ML-5010`.

use aura_core::AuraResult;

use crate::contract::infer::Tensor;
use crate::errors::invalid_graph;
use crate::onnx::graph::Value;
use crate::onnx::model::Node;
use crate::onnx::ops::{element_count, float_operand, operand, optional_float, sample};
use crate::tensor::QuantParams;

/// `Concat` along one axis.
///
/// # Errors
///
/// `AURA-ML-5010` when the axis is outside the rank or the other dimensions
/// disagree.
pub(super) fn concat(node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let first = float_operand(inputs, 0, "Concat")?;
    let rank = first.shape.len();
    let declared = node.attr_int("axis", 0);
    let axis = normalise_axis(declared, rank, "Concat")?;

    let mut tensors = Vec::new();
    for index in 0..inputs.len() {
        if let Some(value) = inputs.get(index).copied().flatten() {
            tensors.push(value.as_float()?);
        }
    }

    let mut shape = first.shape.clone();
    let mut axis_total = 0usize;
    for tensor in &tensors {
        if tensor.shape.len() != rank {
            return Err(invalid_graph(format!(
                "Concat: rank {} does not match rank {rank}",
                tensor.shape.len()
            )));
        }
        for (index, (a, b)) in first.shape.iter().zip(tensor.shape.iter()).enumerate() {
            if index != axis && a != b {
                return Err(invalid_graph(format!(
                    "Concat: dimension {index} differs, {a} against {b}"
                )));
            }
        }
        axis_total += tensor.shape.get(axis).copied().unwrap_or(0);
    }
    if let Some(slot) = shape.get_mut(axis) {
        *slot = axis_total;
    }

    let outer = element_count(first.shape.get(..axis).unwrap_or(&[]));
    let inner = element_count(first.shape.get(axis + 1..).unwrap_or(&[]));
    let mut output = Tensor::zeros(shape);

    let mut axis_offset = 0usize;
    for tensor in &tensors {
        let axis_len = tensor.shape.get(axis).copied().unwrap_or(0);
        for o in 0..outer {
            for a in 0..axis_len {
                for i in 0..inner {
                    let from = (o * axis_len + a) * inner + i;
                    let to = ((o * axis_total) + axis_offset + a) * inner + i;
                    if let Some(slot) = output.data.get_mut(to) {
                        *slot = sample(tensor, from);
                    }
                }
            }
        }
        axis_offset += axis_len;
    }
    Ok(vec![Value::Float(output)])
}

/// `Reshape`, with the usual `0` (keep) and `-1` (infer) conventions.
///
/// # Errors
///
/// `AURA-ML-5010` when the requested shape does not hold the same number of
/// samples, or names more than one inferred dimension.
pub(super) fn reshape(_node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let input = float_operand(inputs, 0, "Reshape")?;
    let target = operand(inputs, 1, "Reshape")?.as_ints()?;

    let mut shape = Vec::with_capacity(target.len());
    let mut inferred: Option<usize> = None;
    for (index, dim) in target.iter().enumerate() {
        match dim {
            -1 => {
                if inferred.is_some() {
                    return Err(invalid_graph(
                        "Reshape names more than one inferred dimension".to_string(),
                    ));
                }
                inferred = Some(index);
                shape.push(1);
            }
            0 => shape.push(input.shape.get(index).copied().unwrap_or(1)),
            value if *value > 0 => shape.push(*value as usize),
            other => {
                return Err(invalid_graph(format!("Reshape to a negative size {other}")));
            }
        }
    }

    let known: usize = shape.iter().product();
    if let Some(index) = inferred {
        if known == 0 || input.data.len() % known != 0 {
            return Err(invalid_graph(format!(
                "Reshape cannot infer a dimension: {} samples into {shape:?}",
                input.data.len()
            )));
        }
        if let Some(slot) = shape.get_mut(index) {
            *slot = input.data.len() / known;
        }
    }

    if element_count(&shape) != input.data.len() {
        return Err(invalid_graph(format!(
            "Reshape to {shape:?} would change the sample count from {}",
            input.data.len()
        )));
    }

    Ok(vec![Value::Float(Tensor {
        shape,
        data: input.data.clone(),
    })])
}

/// `Flatten` into two dimensions at an axis.
///
/// # Errors
///
/// `AURA-ML-5010` when the axis is outside the rank.
pub(super) fn flatten(node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let input = float_operand(inputs, 0, "Flatten")?;
    let rank = input.shape.len();
    let declared = node.attr_int("axis", 1);
    let axis = if declared < 0 {
        normalise_axis(declared, rank, "Flatten")?
    } else {
        usize::try_from(declared)
            .ok()
            .filter(|value| *value <= rank)
            .ok_or_else(|| invalid_graph(format!("Flatten axis {declared} outside rank {rank}")))?
    };

    let rows = element_count(input.shape.get(..axis).unwrap_or(&[])).max(1);
    let columns = element_count(input.shape.get(axis..).unwrap_or(&[])).max(1);
    Ok(vec![Value::Float(Tensor {
        shape: vec![rows, columns],
        data: input.data.clone(),
    })])
}

/// `Transpose` by a permutation, defaulting to a full reversal.
///
/// # Errors
///
/// `AURA-ML-5010` when the permutation is not one.
pub(super) fn transpose(node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let input = float_operand(inputs, 0, "Transpose")?;
    let rank = input.shape.len();
    let declared = node.attr_ints("perm");
    let perm: Vec<usize> = if declared.is_empty() {
        (0..rank).rev().collect()
    } else {
        declared
            .iter()
            .map(|axis| usize::try_from(*axis).unwrap_or(usize::MAX))
            .collect()
    };
    if perm.len() != rank || perm.iter().any(|axis| *axis >= rank) {
        return Err(invalid_graph(format!(
            "Transpose perm {perm:?} is not a permutation of rank {rank}"
        )));
    }
    let mut seen = vec![false; rank];
    for axis in &perm {
        match seen.get_mut(*axis) {
            Some(slot) if !*slot => *slot = true,
            _ => {
                return Err(invalid_graph(format!(
                    "Transpose perm {perm:?} repeats an axis"
                )))
            }
        }
    }

    let shape: Vec<usize> = perm
        .iter()
        .map(|axis| input.shape.get(*axis).copied().unwrap_or(1))
        .collect();
    let mut output = Tensor::zeros(shape.clone());

    let source_strides = strides(&input.shape);
    let count = element_count(&shape);
    for index in 0..count {
        // Walk the destination in order, so the writes are sequential and the
        // reads scatter - the cheaper direction for cache and the only one that
        // keeps the loop free of a division per axis on the write side.
        let mut remaining = index;
        let mut source = 0usize;
        for (axis, size) in shape.iter().enumerate() {
            let stride: usize = shape.get(axis + 1..).map_or(1, element_count);
            let extent = (*size).max(1);
            let coordinate = remaining
                .checked_div(stride)
                .map_or(0, |steps| steps % extent);
            remaining -= coordinate * stride;
            let source_axis = perm.get(axis).copied().unwrap_or(0);
            source += coordinate * source_strides.get(source_axis).copied().unwrap_or(1);
        }
        if let Some(slot) = output.data.get_mut(index) {
            *slot = sample(input, source);
        }
    }
    Ok(vec![Value::Float(output)])
}

/// `QuantizeLinear`: real values to the quantised grid and straight back.
///
/// The runtime carries quantised tensors as `f32` snapped to the grid rather than
/// as `i8`. It costs memory we could have saved and buys the one thing that
/// matters here: the same code path executes every precision, so a parity failure
/// is a rounding difference rather than a different implementation.
///
/// # Errors
///
/// `AURA-ML-5010` when the scale operand is missing.
pub(super) fn quantize_linear(_node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let input = float_operand(inputs, 0, "QuantizeLinear")?;
    let params = quant_params(inputs)?;
    let snapped = input
        .data
        .iter()
        .map(|value| params.round_trip(*value))
        .collect();
    Ok(vec![Value::Float(Tensor {
        shape: input.shape.clone(),
        data: snapped,
    })])
}

/// `DequantizeLinear`. Values already sit on the grid, so this is the identity
/// on our representation - kept as a real operator so a graph that names it runs
/// rather than being refused.
///
/// # Errors
///
/// `AURA-ML-5010` when the operand is missing.
pub(super) fn dequantize_linear(_node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let input = float_operand(inputs, 0, "DequantizeLinear")?;
    Ok(vec![Value::Float(input.clone())])
}

fn quant_params(inputs: &[Option<&Value>]) -> AuraResult<QuantParams> {
    let scale = optional_float(inputs, 1)?.map_or(1.0, |tensor| sample(tensor, 0));
    let zero_point = match inputs.get(2).copied().flatten() {
        Some(Value::Int(values)) => {
            i32::try_from(values.first().copied().unwrap_or(0)).unwrap_or(0)
        }
        Some(Value::Float(tensor)) => sample(tensor, 0) as i32,
        None => 0,
    };
    Ok(QuantParams {
        scale: if scale == 0.0 { 1.0 } else { scale },
        zero_point: zero_point.clamp(-128, 127) as i8,
    })
}

fn normalise_axis(declared: i64, rank: usize, op: &str) -> AuraResult<usize> {
    let axis = if declared < 0 {
        i64::try_from(rank).unwrap_or(i64::MAX) + declared
    } else {
        declared
    };
    usize::try_from(axis)
        .ok()
        .filter(|value| *value < rank.max(1))
        .ok_or_else(|| invalid_graph(format!("{op} axis {declared} outside rank {rank}")))
}

fn strides(shape: &[usize]) -> Vec<usize> {
    let mut strides = vec![1usize; shape.len()];
    for axis in (0..shape.len().saturating_sub(1)).rev() {
        let next = shape.get(axis + 1).copied().unwrap_or(1);
        let previous = strides.get(axis + 1).copied().unwrap_or(1);
        if let Some(slot) = strides.get_mut(axis) {
            *slot = previous * next;
        }
    }
    strides
}
