//! The spatial nearest/asymmetric/floor subset of ONNX Resize (opset 11+).
use aura_core::AuraResult;

use super::{float_operand, nchw, optional_float};
use crate::{
    contract::infer::Tensor,
    errors::invalid_graph,
    onnx::{
        graph::Value,
        model::{AttrValue, Node},
    },
};

pub(super) fn resize(node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    for (name, expected) in [
        ("mode", "nearest"),
        ("coordinate_transformation_mode", "asymmetric"),
        ("nearest_mode", "floor"),
    ] {
        if !matches!(node.attr(name), Some(AttrValue::Str(value)) if value == expected) {
            return Err(invalid_graph(format!("Resize requires {name}={expected}")));
        }
    }
    if optional_float(inputs, 1)?.is_some_and(|roi| !roi.data.is_empty())
        || inputs.get(3).is_some_and(Option::is_some)
    {
        return Err(invalid_graph(
            "Resize supports scales with an empty ROI only",
        ));
    }
    let input = float_operand(inputs, 0, "Resize")?;
    let (n, c, h, w) = nchw(&input.shape, "Resize")?;
    if n.checked_mul(c)
        .and_then(|v| v.checked_mul(h))
        .and_then(|v| v.checked_mul(w))
        != Some(input.data.len())
    {
        return Err(invalid_graph(
            "Resize input shape does not match its buffer",
        ));
    }
    let scales = float_operand(inputs, 2, "Resize")?;
    let [sn, sc, sy, sx] = scales.data.as_slice() else {
        return Err(invalid_graph("Resize needs four NCHW scales"));
    };
    if sn.to_bits() != 1.0_f32.to_bits()
        || sc.to_bits() != 1.0_f32.to_bits()
        || !sy.is_finite()
        || !sx.is_finite()
        || *sy <= 0.0
        || *sx <= 0.0
        || h == 0
        || w == 0
    {
        return Err(invalid_graph(
            "Resize requires finite positive spatial scales and unchanged batch/channels",
        ));
    }
    let oh = scaled_size(h, *sy)?;
    let ow = scaled_size(w, *sx)?;
    let count = n
        .checked_mul(c)
        .and_then(|v| v.checked_mul(oh))
        .and_then(|v| v.checked_mul(ow))
        .filter(|v| *v <= 16_777_216)
        .ok_or_else(|| invalid_graph("Resize output exceeds tensor limit"))?;
    let mut data = Vec::with_capacity(count);
    for plane in 0..n * c {
        for y in 0..oh {
            let iy = source_index(y, *sy, h);
            for x in 0..ow {
                let ix = source_index(x, *sx, w);
                data.push(
                    *input
                        .data
                        .get((plane * h + iy) * w + ix)
                        .ok_or_else(|| invalid_graph("Resize input buffer is truncated"))?,
                );
            }
        }
    }
    Ok(vec![Value::Float(Tensor {
        shape: vec![n, c, oh, ow],
        data,
    })])
}

// Dimensions are checked before conversion; the fixed tensor cap prevents allocation overflow.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn scaled_size(size: usize, scale: f32) -> AuraResult<usize> {
    let output = (size as f64 * f64::from(scale)).floor();
    if !(1.0..=16_777_216.0).contains(&output) {
        return Err(invalid_graph("Invalid Resize output dimension"));
    }
    Ok(output as usize)
}

// Coordinates are nonnegative and clamped to a validated, nonempty dimension.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn source_index(index: usize, scale: f32, size: usize) -> usize {
    ((index as f64 / f64::from(scale)).floor() as usize).min(size - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onnx::model::Attribute;
    fn node() -> Node {
        Node {
            op_type: "Resize".into(),
            attributes: [
                ("mode", "nearest"),
                ("coordinate_transformation_mode", "asymmetric"),
                ("nearest_mode", "floor"),
            ]
            .into_iter()
            .map(|(name, value)| Attribute {
                name: name.into(),
                value: AttrValue::Str(value.into()),
            })
            .collect(),
            ..Node::default()
        }
    }
    #[test]
    fn noninteger_nearest_resize_keeps_planes_and_floor_coordinates() {
        let image = Value::Float(Tensor {
            shape: vec![1, 2, 2, 2],
            data: vec![1., 2., 3., 4., 10., 20., 30., 40.],
        });
        let scales = Value::Float(Tensor {
            shape: vec![4],
            data: vec![1., 1., 1.5, 1.5],
        });
        let result = resize(&node(), &[Some(&image), None, Some(&scales)]).unwrap();
        assert_eq!(result[0].as_float().unwrap().shape, vec![1, 2, 3, 3]);
        assert_eq!(
            result[0].as_float().unwrap().data,
            vec![1., 1., 2., 1., 1., 2., 3., 3., 4., 10., 10., 20., 10., 10., 20., 30., 30., 40.]
        );
        let mut unsupported = node();
        unsupported.attributes.pop();
        assert!(resize(&unsupported, &[Some(&image), None, Some(&scales)]).is_err());
        let invalid = Value::Float(Tensor {
            shape: vec![4],
            data: vec![1., 1., f32::NAN, 2.],
        });
        assert!(resize(&node(), &[Some(&image), None, Some(&invalid)]).is_err());
    }
}
