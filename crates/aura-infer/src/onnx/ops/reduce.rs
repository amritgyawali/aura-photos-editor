//! `ReduceSum` (opset 13: axes as an optional second input).
//!
//! Each output sample sums its reduced elements in increasing index order, one
//! running `f32` total, so two machines agree bit for bit.
use aura_core::AuraResult;

use super::{element_count, float_operand, sample};
use crate::{
    contract::infer::Tensor,
    errors::invalid_graph,
    onnx::{graph::Value, model::Node},
};

/// `ReduceSum` over the named axes, or over every axis when none are named.
///
/// # Errors
///
/// `AURA-ML-5010` when an axis is outside the tensor or `noop_with_empty_axes` is set.
pub(super) fn reduce_sum(node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let input = float_operand(inputs, 0, "ReduceSum")?;
    let rank = input.shape.len();
    if node.attr_int("noop_with_empty_axes", 0) != 0 {
        return Err(invalid_graph(
            "ReduceSum: noop_with_empty_axes is not supported",
        ));
    }
    let declared = match inputs.get(1).copied().flatten() {
        Some(value) => value.as_ints()?,
        None => Vec::new(),
    };
    let mut reduced = vec![declared.is_empty(); rank];
    for axis in declared {
        let index = if axis < 0 {
            i64::try_from(rank).unwrap_or(i64::MAX) + axis
        } else {
            axis
        };
        let slot = usize::try_from(index)
            .ok()
            .and_then(|i| reduced.get_mut(i))
            .ok_or_else(|| invalid_graph(format!("ReduceSum axis {axis} outside rank {rank}")))?;
        *slot = true;
    }
    if element_count(&input.shape) != input.data.len() {
        return Err(invalid_graph("ReduceSum: buffer does not match its shape"));
    }
    let keep = node.attr_int("keepdims", 1) != 0;
    let kept_shape: Vec<usize> = input
        .shape
        .iter()
        .zip(&reduced)
        .map(|(size, r)| if *r { 1 } else { *size })
        .collect();
    let mut total = Tensor::zeros(kept_shape.clone());
    // Strides of the output, with zero along reduced axes, walked by an odometer over the
    // input in row-major order - so every output's terms arrive in increasing index order.
    let mut out_strides = vec![0usize; rank];
    let mut stride = 1usize;
    for axis in (0..rank).rev() {
        if let (Some(slot), Some(size)) = (out_strides.get_mut(axis), kept_shape.get(axis)) {
            *slot = if reduced.get(axis).copied().unwrap_or(false) {
                0
            } else {
                stride
            };
            stride *= *size;
        }
    }
    let mut coordinate = vec![0usize; rank];
    let mut target = 0usize;
    for index in 0..input.data.len() {
        if let Some(slot) = total.data.get_mut(target) {
            *slot += sample(input, index);
        }
        for axis in (0..rank).rev() {
            let (Some(c), Some(size), Some(step)) = (
                coordinate.get_mut(axis),
                input.shape.get(axis),
                out_strides.get(axis),
            ) else {
                break;
            };
            *c += 1;
            target += step;
            if *c < *size {
                break;
            }
            target -= step * *c;
            *c = 0;
        }
    }
    if !keep {
        total.shape = input
            .shape
            .iter()
            .zip(&reduced)
            .filter(|(_, r)| !**r)
            .map(|(size, _)| *size)
            .collect();
    }
    Ok(vec![Value::Float(total)])
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;
    use crate::onnx::model::{AttrValue, Attribute};

    #[test]
    fn sums_the_named_axis_and_keeps_or_drops_it() {
        let x = Value::Float(Tensor {
            shape: vec![1, 2, 3],
            data: vec![1.0, 2.0, 3.0, 10.0, 20.0, 30.0],
        });
        let axes = Value::Int(vec![-1]);
        let node = Node::default();
        let out = reduce_sum(&node, &[Some(&x), Some(&axes)]).unwrap();
        let out = out[0].as_float().unwrap();
        assert_eq!(out.shape, vec![1, 2, 1]);
        assert_eq!(out.data, vec![6.0, 60.0]);
        let middle = Value::Int(vec![1]);
        let dropped = Node {
            attributes: vec![Attribute {
                name: "keepdims".into(),
                value: AttrValue::Int(0),
            }],
            ..Node::default()
        };
        let out = reduce_sum(&dropped, &[Some(&x), Some(&middle)]).unwrap();
        let out = out[0].as_float().unwrap();
        assert_eq!(out.shape, vec![1, 3]);
        assert_eq!(out.data, vec![11.0, 22.0, 33.0]);
        let all = reduce_sum(&node, &[Some(&x), None]).unwrap();
        assert_eq!(all[0].as_float().unwrap().data, vec![66.0]);
        assert!(reduce_sum(&node, &[Some(&x), Some(&Value::Int(vec![3]))]).is_err());
    }
}
