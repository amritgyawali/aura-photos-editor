//! Two-dimensional convolution.
//!
//! The single most expensive operator in every model this product will ship, and
//! the one where determinism is easiest to lose. Two rules keep it honest:
//!
//! * **Fixed accumulation order.** Each output sample sums over input channel,
//!   then kernel row, then kernel column, always in that order. No parallel
//!   reduction, no fused multiply-add reordering, no rayon inside the sum.
//! * **Parallel over output rows only.** Each thread writes its own row slice of
//!   the output, reads only immutable inputs, and never touches another row - so
//!   the result is bit-identical whatever the core count. This is the same shape
//!   as the phase 02 demosaic loops, for the same reason.

use aura_core::AuraResult;
use rayon::prelude::*;

use crate::contract::infer::Tensor;
use crate::errors::invalid_graph;
use crate::onnx::graph::Value;
use crate::onnx::model::Node;
use crate::onnx::ops::{
    float_operand, nchw, optional_float, pair_attr, resolve_pads, sample, spatial_out,
};

/// Output samples below which the loop stays on one thread.
///
/// Scheduling a rayon job costs more than a small feature map's arithmetic, and
/// the placeholder models in this phase are deliberately small.
const PARALLEL_MIN: usize = 1 << 16;

/// `Conv`: NCHW input, `[M, C/group, kh, kw]` weights, optional `[M]` bias.
///
/// # Errors
///
/// `AURA-ML-5010` when the shapes cannot describe a convolution: a rank other
/// than four, a channel count not divisible by the group count, or a kernel
/// larger than its padded input.
pub(super) fn conv(node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let input = float_operand(inputs, 0, "Conv")?;
    let weight = float_operand(inputs, 1, "Conv")?;
    let bias = optional_float(inputs, 2)?;

    let (batch, in_channels, in_h, in_w) = nchw(&input.shape, "Conv")?;
    let (out_channels, group_channels, kernel_h, kernel_w) = nchw(&weight.shape, "Conv weights")?;

    let group = node.attr_int("group", 1).max(1) as usize;
    if in_channels != group_channels * group || out_channels % group != 0 {
        return Err(invalid_graph(format!(
            "Conv: {in_channels} input channels do not divide into {group} groups of \
             {group_channels}, producing {out_channels} outputs"
        )));
    }

    let stride = pair_attr(node, "strides", 1);
    let dilation = pair_attr(node, "dilations", 1);
    let (pad_top, pad_left, pad_bottom, pad_right) =
        resolve_pads(node, (in_h, in_w), (kernel_h, kernel_w), stride, dilation);

    let out_h = spatial_out(in_h, kernel_h, stride.0, dilation.0, pad_top, pad_bottom);
    let out_w = spatial_out(in_w, kernel_w, stride.1, dilation.1, pad_left, pad_right);
    if out_h == 0 || out_w == 0 {
        return Err(invalid_graph(format!(
            "Conv: a {kernel_h}x{kernel_w} kernel does not fit a {in_h}x{in_w} input with the \
             declared padding"
        )));
    }

    let out_per_group = out_channels / group;
    let mut output = Tensor::zeros(vec![batch, out_channels, out_h, out_w]);
    let row_len = out_w;

    // One rayon task per output row: (batch, channel, row) is unique per slice.
    let rows = batch * out_channels * out_h;
    let fill = |row_index: usize, row: &mut [f32]| {
        let out_y = row_index % out_h;
        let channel = (row_index / out_h) % out_channels;
        let image = row_index / (out_h * out_channels);
        let group_index = channel / out_per_group;
        let channel_base = group_index * group_channels;

        let bias_value = bias.map_or(0.0, |tensor| sample(tensor, channel));

        for (out_x, slot) in row.iter_mut().enumerate() {
            let mut total = bias_value;
            for kernel_channel in 0..group_channels {
                let in_channel = channel_base + kernel_channel;
                let weight_base =
                    ((channel * group_channels + kernel_channel) * kernel_h) * kernel_w;
                let input_plane = (image * in_channels + in_channel) * in_h * in_w;

                for ky in 0..kernel_h {
                    let in_y = out_y * stride.0 + ky * dilation.0;
                    if in_y < pad_top {
                        continue;
                    }
                    let in_y = in_y - pad_top;
                    if in_y >= in_h {
                        continue;
                    }
                    for kx in 0..kernel_w {
                        let in_x = out_x * stride.1 + kx * dilation.1;
                        if in_x < pad_left {
                            continue;
                        }
                        let in_x = in_x - pad_left;
                        if in_x >= in_w {
                            continue;
                        }
                        let weight_value = sample(weight, weight_base + ky * kernel_w + kx);
                        let input_value = sample(input, input_plane + in_y * in_w + in_x);
                        total += weight_value * input_value;
                    }
                }
            }
            *slot = total;
        }
    };

    if output.data.len() >= PARALLEL_MIN {
        output
            .data
            .par_chunks_mut(row_len)
            .enumerate()
            .for_each(|(index, row)| fill(index, row));
    } else {
        for (index, row) in output.data.chunks_mut(row_len).enumerate() {
            fill(index, row);
        }
    }
    debug_assert_eq!(rows * row_len, output.data.len());

    Ok(vec![Value::Float(output)])
}
