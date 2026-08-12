//! Activations, arithmetic and normalisation.
//!
//! `Softmax` is the one to read carefully: it subtracts the row maximum before
//! exponentiating. That is not an optimisation, it is the difference between a
//! confidence of 0.97 and a `NaN` on a logit of 90, and every AI decision in this
//! product carries a confidence.

use aura_core::AuraResult;

use crate::contract::infer::Tensor;
use crate::errors::invalid_graph;
use crate::onnx::graph::Value;
use crate::onnx::model::Node;
use crate::onnx::ops::{element_count, float_operand, optional_float, sample};

/// `Relu`.
///
/// # Errors
///
/// `AURA-ML-5010` when the operand is missing.
pub(super) fn relu(_node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    map(inputs, "Relu", |value| value.max(0.0))
}

/// `Sigmoid`.
///
/// # Errors
///
/// `AURA-ML-5010` when the operand is missing.
pub(super) fn sigmoid(_node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    map(inputs, "Sigmoid", |value| 1.0 / (1.0 + (-value).exp()))
}

/// `Clip`, in both spellings: opset 11 and later take the bounds as inputs,
/// earlier files carry them as attributes. Both appear in the wild.
///
/// # Errors
///
/// `AURA-ML-5010` when the operand is missing.
pub(super) fn clip(node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let input = float_operand(inputs, 0, "Clip")?;
    let min = match optional_float(inputs, 1)? {
        Some(tensor) => sample(tensor, 0),
        None => node.attr_float("min", f32::MIN),
    };
    let max = match optional_float(inputs, 2)? {
        Some(tensor) => sample(tensor, 0),
        None => node.attr_float("max", f32::MAX),
    };
    let clamped = input.data.iter().map(|v| v.clamp(min, max)).collect();
    Ok(vec![Value::Float(Tensor {
        shape: input.shape.clone(),
        data: clamped,
    })])
}

/// `Add`, with numpy broadcasting.
///
/// # Errors
///
/// `AURA-ML-5010` when the shapes cannot be broadcast together.
pub(super) fn add(_node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    binary(inputs, "Add", |a, b| a + b)
}

/// `Mul`, with numpy broadcasting.
///
/// # Errors
///
/// `AURA-ML-5010` when the shapes cannot be broadcast together.
pub(super) fn mul(_node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    binary(inputs, "Mul", |a, b| a * b)
}

/// `Softmax` along one axis, numerically stabilised.
///
/// # Errors
///
/// `AURA-ML-5010` when the axis is outside the tensor.
pub(super) fn softmax(node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let input = float_operand(inputs, 0, "Softmax")?;
    let rank = input.shape.len();
    if rank == 0 {
        return Err(invalid_graph("Softmax over a scalar".to_string()));
    }
    // Opset 13 defaults to the last axis; negative axes count from the end.
    let declared = node.attr_int("axis", -1);
    let axis = if declared < 0 {
        i64::try_from(rank).unwrap_or(i64::MAX) + declared
    } else {
        declared
    };
    let axis = usize::try_from(axis)
        .ok()
        .filter(|value| *value < rank)
        .ok_or_else(|| invalid_graph(format!("Softmax axis {declared} outside rank {rank}")))?;

    let axis_len = input.shape.get(axis).copied().unwrap_or(1);
    let inner: usize = element_count(input.shape.get(axis + 1..).unwrap_or(&[]));
    let outer: usize = element_count(input.shape.get(..axis).unwrap_or(&[]));

    let mut output = Tensor::zeros(input.shape.clone());
    for o in 0..outer {
        for i in 0..inner {
            let offset = |index: usize| (o * axis_len + index) * inner + i;

            let mut largest = f32::NEG_INFINITY;
            for index in 0..axis_len {
                largest = largest.max(sample(input, offset(index)));
            }
            let mut total = 0.0f32;
            for index in 0..axis_len {
                total += (sample(input, offset(index)) - largest).exp();
            }
            let total = if total == 0.0 { 1.0 } else { total };
            for index in 0..axis_len {
                let value = (sample(input, offset(index)) - largest).exp() / total;
                if let Some(slot) = output.data.get_mut(offset(index)) {
                    *slot = value;
                }
            }
        }
    }
    Ok(vec![Value::Float(output)])
}

/// `BatchNormalization` in inference mode: scale, shift, running mean and
/// variance, per channel.
///
/// # Errors
///
/// `AURA-ML-5010` when an operand is missing or the input has no channel axis.
pub(super) fn batch_norm(node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let input = float_operand(inputs, 0, "BatchNormalization")?;
    let scale = float_operand(inputs, 1, "BatchNormalization")?;
    let shift = float_operand(inputs, 2, "BatchNormalization")?;
    let mean = float_operand(inputs, 3, "BatchNormalization")?;
    let variance = float_operand(inputs, 4, "BatchNormalization")?;
    let epsilon = node.attr_float("epsilon", 1e-5);

    let channels = input.shape.get(1).copied().unwrap_or(1);
    let spatial = element_count(input.shape.get(2..).unwrap_or(&[])).max(1);
    let batch = input.shape.first().copied().unwrap_or(1);

    let mut output = Tensor::zeros(input.shape.clone());
    for image in 0..batch {
        for channel in 0..channels {
            let gamma = sample(scale, channel);
            let beta = sample(shift, channel);
            let mu = sample(mean, channel);
            let sigma = (sample(variance, channel) + epsilon)
                .max(f32::MIN_POSITIVE)
                .sqrt();
            let plane = (image * channels + channel) * spatial;
            for offset in 0..spatial {
                let value = (sample(input, plane + offset) - mu) / sigma * gamma + beta;
                if let Some(slot) = output.data.get_mut(plane + offset) {
                    *slot = value;
                }
            }
        }
    }
    Ok(vec![Value::Float(output)])
}

fn map<F>(inputs: &[Option<&Value>], op: &str, f: F) -> AuraResult<Vec<Value>>
where
    F: Fn(f32) -> f32,
{
    let input = float_operand(inputs, 0, op)?;
    Ok(vec![Value::Float(Tensor {
        shape: input.shape.clone(),
        data: input.data.iter().map(|value| f(*value)).collect(),
    })])
}

fn binary<F>(inputs: &[Option<&Value>], op: &str, f: F) -> AuraResult<Vec<Value>>
where
    F: Fn(f32, f32) -> f32,
{
    let left = float_operand(inputs, 0, op)?;
    let right = float_operand(inputs, 1, op)?;
    let shape = broadcast_shape(&left.shape, &right.shape, op)?;
    let count = element_count(&shape);

    let mut output = Tensor::zeros(shape.clone());
    for index in 0..count {
        let coordinate = unravel(index, &shape);
        let a = sample(left, ravel(&coordinate, &left.shape));
        let b = sample(right, ravel(&coordinate, &right.shape));
        if let Some(slot) = output.data.get_mut(index) {
            *slot = f(a, b);
        }
    }
    Ok(vec![Value::Float(output)])
}

/// Numpy broadcasting: align from the right, each dimension must match or be 1.
fn broadcast_shape(left: &[usize], right: &[usize], op: &str) -> AuraResult<Vec<usize>> {
    let rank = left.len().max(right.len());
    let mut shape = Vec::with_capacity(rank);
    for index in 0..rank {
        let a = dimension_from_right(left, rank - index);
        let b = dimension_from_right(right, rank - index);
        if a == b || a == 1 || b == 1 {
            shape.push(a.max(b));
        } else {
            return Err(invalid_graph(format!(
                "{op}: shapes {left:?} and {right:?} do not broadcast"
            )));
        }
    }
    Ok(shape)
}

fn dimension_from_right(shape: &[usize], from_right: usize) -> usize {
    if from_right > shape.len() {
        1
    } else {
        shape.get(shape.len() - from_right).copied().unwrap_or(1)
    }
}

/// Turn a flat index into coordinates in a shape.
fn unravel(mut index: usize, shape: &[usize]) -> Vec<usize> {
    let mut coordinate = vec![0usize; shape.len()];
    for axis in (0..shape.len()).rev() {
        let size = shape.get(axis).copied().unwrap_or(1).max(1);
        if let Some(slot) = coordinate.get_mut(axis) {
            *slot = index % size;
        }
        index /= size;
    }
    coordinate
}

/// Turn coordinates into a flat index, folding broadcast dimensions to zero.
fn ravel(coordinate: &[usize], shape: &[usize]) -> usize {
    let mut index = 0usize;
    let offset = coordinate.len().saturating_sub(shape.len());
    for (axis, size) in shape.iter().enumerate() {
        let value = coordinate.get(axis + offset).copied().unwrap_or(0);
        let value = if *size == 1 { 0 } else { value };
        index = index * size + value;
    }
    index
}
