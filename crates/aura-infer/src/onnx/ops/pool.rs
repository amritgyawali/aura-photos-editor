//! Pooling: max, average, and the global average that ends every backbone.

use aura_core::AuraResult;

use crate::contract::infer::Tensor;
use crate::errors::invalid_graph;
use crate::onnx::graph::Value;
use crate::onnx::model::Node;
use crate::onnx::ops::{float_operand, nchw, pair_attr, resolve_pads, sample, spatial_out};

/// `MaxPool` over an NCHW input.
///
/// # Errors
///
/// `AURA-ML-5010` when the window does not fit the padded input.
pub(super) fn max_pool(node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    pool(node, inputs, Reduction::Max)
}

/// `AveragePool` over an NCHW input.
///
/// `count_include_pad` is honoured; the ONNX default of 0 divides by the number
/// of real samples, which is what makes a padded average agree with an unpadded
/// one at the edges.
///
/// # Errors
///
/// `AURA-ML-5010` when the window does not fit the padded input.
pub(super) fn average_pool(node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let include_pad = node.attr_int("count_include_pad", 0) != 0;
    pool(node, inputs, Reduction::Average { include_pad })
}

/// `GlobalAveragePool`: one number per channel.
///
/// # Errors
///
/// `AURA-ML-5010` when the input is not four-dimensional.
pub(super) fn global_average_pool(
    _node: &Node,
    inputs: &[Option<&Value>],
) -> AuraResult<Vec<Value>> {
    let input = float_operand(inputs, 0, "GlobalAveragePool")?;
    let (batch, channels, height, width) = nchw(&input.shape, "GlobalAveragePool")?;
    let area = height * width;
    if area == 0 {
        return Err(invalid_graph(
            "GlobalAveragePool over an empty spatial extent".to_string(),
        ));
    }

    let mut output = Tensor::zeros(vec![batch, channels, 1, 1]);
    for image in 0..batch {
        for channel in 0..channels {
            let plane = (image * channels + channel) * area;
            let mut total = 0.0f32;
            for offset in 0..area {
                total += sample(input, plane + offset);
            }
            if let Some(slot) = output.data.get_mut(image * channels + channel) {
                *slot = total / area as f32;
            }
        }
    }
    Ok(vec![Value::Float(output)])
}

/// How a window is reduced to one number.
#[derive(Debug, Clone, Copy)]
enum Reduction {
    Max,
    Average { include_pad: bool },
}

fn pool(node: &Node, inputs: &[Option<&Value>], reduction: Reduction) -> AuraResult<Vec<Value>> {
    let op = match reduction {
        Reduction::Max => "MaxPool",
        Reduction::Average { .. } => "AveragePool",
    };
    let input = float_operand(inputs, 0, op)?;
    let (batch, channels, in_h, in_w) = nchw(&input.shape, op)?;

    let kernel = pair_attr(node, "kernel_shape", 1);
    let stride = {
        let declared = node.attr_ints("strides");
        if declared.is_empty() {
            kernel
        } else {
            pair_attr(node, "strides", 1)
        }
    };
    let dilation = pair_attr(node, "dilations", 1);
    let (pad_top, pad_left, pad_bottom, pad_right) =
        resolve_pads(node, (in_h, in_w), kernel, stride, dilation);

    let out_h = spatial_out(in_h, kernel.0, stride.0, dilation.0, pad_top, pad_bottom);
    let out_w = spatial_out(in_w, kernel.1, stride.1, dilation.1, pad_left, pad_right);
    if out_h == 0 || out_w == 0 {
        return Err(invalid_graph(format!(
            "{op}: a {}x{} window does not fit a {in_h}x{in_w} input",
            kernel.0, kernel.1
        )));
    }

    let mut output = Tensor::zeros(vec![batch, channels, out_h, out_w]);
    for image in 0..batch {
        for channel in 0..channels {
            let in_plane = (image * channels + channel) * in_h * in_w;
            let out_plane = (image * channels + channel) * out_h * out_w;
            for out_y in 0..out_h {
                for out_x in 0..out_w {
                    let mut best = f32::NEG_INFINITY;
                    let mut total = 0.0f32;
                    let mut counted = 0usize;
                    let mut window = 0usize;

                    for ky in 0..kernel.0 {
                        for kx in 0..kernel.1 {
                            window += 1;
                            let in_y = out_y * stride.0 + ky * dilation.0;
                            let in_x = out_x * stride.1 + kx * dilation.1;
                            if in_y < pad_top || in_x < pad_left {
                                continue;
                            }
                            let in_y = in_y - pad_top;
                            let in_x = in_x - pad_left;
                            if in_y >= in_h || in_x >= in_w {
                                continue;
                            }
                            let value = sample(input, in_plane + in_y * in_w + in_x);
                            best = best.max(value);
                            total += value;
                            counted += 1;
                        }
                    }

                    let value = match reduction {
                        Reduction::Max => {
                            if counted == 0 {
                                0.0
                            } else {
                                best
                            }
                        }
                        Reduction::Average { include_pad } => {
                            let divisor = if include_pad { window } else { counted };
                            if divisor == 0 {
                                0.0
                            } else {
                                total / divisor as f32
                            }
                        }
                    };
                    if let Some(slot) = output.data.get_mut(out_plane + out_y * out_w + out_x) {
                        *slot = value;
                    }
                }
            }
        }
    }
    Ok(vec![Value::Float(output)])
}
