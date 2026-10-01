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
#[allow(clippy::too_many_lines)]
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
    if input.data.len() != batch * in_channels * in_h * in_w {
        return Err(invalid_graph("Conv: input buffer does not match its shape"));
    }

    // One rayon task per output row: (batch, channel, row) is unique per slice.
    //
    // Each output sample still sums over input channel, then kernel row, then kernel
    // column, skipping exactly the padded taps - so the result is bit-identical to the
    // per-sample loop this replaced. Only the loop nesting moved: the output column is
    // innermost, which turns a stride-1 tap into a contiguous multiply-add over the row.
    let rows = batch * out_channels * out_h;
    let fill = |row_index: usize, row: &mut [f32]| {
        let out_y = row_index % out_h;
        let channel = (row_index / out_h) % out_channels;
        let image = row_index / (out_h * out_channels);
        let group_index = channel / out_per_group;
        let channel_base = group_index * group_channels;

        let bias_value = bias.map_or(0.0, |tensor| sample(tensor, channel));
        row.fill(bias_value);

        // The output columns whose tap `kx` lands inside the unpadded input row:
        // in_x = out_x * stride + kx * dilation - pad_left must lie in [0, in_w).
        let columns = |kx: usize| -> (usize, usize) {
            let offset = kx * dilation.1;
            let first = pad_left.saturating_sub(offset).div_ceil(stride.1);
            let last = (in_w + pad_left)
                .checked_sub(offset)
                .map_or(0, |limit| limit.div_ceil(stride.1))
                .min(out_w);
            (first.min(last), last)
        };

        for kernel_channel in 0..group_channels {
            let in_channel = channel_base + kernel_channel;
            let weight_base = ((channel * group_channels + kernel_channel) * kernel_h) * kernel_w;
            let input_plane = (image * in_channels + in_channel) * in_h * in_w;

            for ky in 0..kernel_h {
                let in_y = out_y * stride.0 + ky * dilation.0;
                if in_y < pad_top || in_y - pad_top >= in_h {
                    continue;
                }
                let in_row = input_plane + (in_y - pad_top) * in_w;
                let Some(source) = input.data.get(in_row..in_row + in_w) else {
                    continue;
                };
                for kx in 0..kernel_w {
                    let weight_value = sample(weight, weight_base + ky * kernel_w + kx);
                    let (first, last) = columns(kx);
                    if first >= last {
                        continue;
                    }
                    let start = first * stride.1 + kx * dilation.1 - pad_left;
                    if stride.1 == 1 {
                        let count = last - first;
                        if let (Some(out), Some(src)) =
                            (row.get_mut(first..last), source.get(start..start + count))
                        {
                            for (slot, value) in out.iter_mut().zip(src) {
                                *slot += weight_value * value;
                            }
                        }
                    } else {
                        for (step, out_x) in (first..last).enumerate() {
                            let value = source.get(start + step * stride.1).copied().unwrap_or(0.0);
                            if let Some(slot) = row.get_mut(out_x) {
                                *slot += weight_value * value;
                            }
                        }
                    }
                }
            }
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

/// `ConvTranspose`: NCHW input, `[C, M, kh, kw]` weights, optional `[M]` bias.
///
/// Written as a gather rather than the usual scatter: every output sample sums its
/// contributing taps in a fixed order (input channel, kernel row, kernel column), so
/// the parallel-over-rows rule that keeps `Conv` deterministic holds here too.
///
/// # Errors
///
/// `AURA-ML-5010` for a rank other than four, more than one group, output padding,
/// or an output that the declared padding would make empty.
#[allow(clippy::similar_names)]
pub(super) fn conv_transpose(node: &Node, inputs: &[Option<&Value>]) -> AuraResult<Vec<Value>> {
    let input = float_operand(inputs, 0, "ConvTranspose")?;
    let weight = float_operand(inputs, 1, "ConvTranspose")?;
    let bias = optional_float(inputs, 2)?;

    let (batch, in_channels, in_h, in_w) = nchw(&input.shape, "ConvTranspose")?;
    let (weight_in, out_channels, kernel_h, kernel_w) =
        nchw(&weight.shape, "ConvTranspose weights")?;
    if node.attr_int("group", 1) != 1 || weight_in != in_channels {
        return Err(invalid_graph(format!(
            "ConvTranspose: {in_channels} input channels against {weight_in} weight rows \
             (only one group is supported)"
        )));
    }
    if node.attr_ints("output_padding").iter().any(|v| *v != 0)
        || !node.attr_ints("output_shape").is_empty()
    {
        return Err(invalid_graph(
            "ConvTranspose: output_padding and output_shape are not supported",
        ));
    }
    if input.data.len() != batch * in_channels * in_h * in_w {
        return Err(invalid_graph(
            "ConvTranspose: input buffer does not match its shape",
        ));
    }
    let stride = pair_attr(node, "strides", 1);
    let dilation = pair_attr(node, "dilations", 1);
    let pads = node.attr_ints("pads");
    let pad =
        |index: usize| usize::try_from(pads.get(index).copied().unwrap_or(0).max(0)).unwrap_or(0);
    let (pad_top, pad_left, pad_bottom, pad_right) = (pad(0), pad(1), pad(2), pad(3));

    let full = |size: usize, kernel: usize, stride: usize, dilation: usize| {
        size.saturating_sub(1) * stride + dilation * kernel.saturating_sub(1) + 1
    };
    let out_h = full(in_h, kernel_h, stride.0, dilation.0).saturating_sub(pad_top + pad_bottom);
    let out_w = full(in_w, kernel_w, stride.1, dilation.1).saturating_sub(pad_left + pad_right);
    if out_h == 0 || out_w == 0 {
        return Err(invalid_graph("ConvTranspose: empty output"));
    }

    let mut output = Tensor::zeros(vec![batch, out_channels, out_h, out_w]);
    let fill = |row_index: usize, row: &mut [f32]| {
        let out_y = row_index % out_h;
        let channel = (row_index / out_h) % out_channels;
        let image = row_index / (out_h * out_channels);
        let bias_value = bias.map_or(0.0, |tensor| sample(tensor, channel));
        for (out_x, slot) in row.iter_mut().enumerate() {
            let mut total = bias_value;
            for in_channel in 0..in_channels {
                let plane = (image * in_channels + in_channel) * in_h * in_w;
                let weight_base = (in_channel * out_channels + channel) * kernel_h * kernel_w;
                for ky in 0..kernel_h {
                    // out_y + pad_top = in_y * stride + ky * dilation
                    let Some(y) = (out_y + pad_top).checked_sub(ky * dilation.0) else {
                        continue;
                    };
                    if y % stride.0 != 0 || y / stride.0 >= in_h {
                        continue;
                    }
                    let in_y = y / stride.0;
                    for kx in 0..kernel_w {
                        let Some(x) = (out_x + pad_left).checked_sub(kx * dilation.1) else {
                            continue;
                        };
                        if x % stride.1 != 0 || x / stride.1 >= in_w {
                            continue;
                        }
                        let in_x = x / stride.1;
                        total += sample(weight, weight_base + ky * kernel_w + kx)
                            * sample(input, plane + in_y * in_w + in_x);
                    }
                }
            }
            *slot = total;
        }
    };
    if output.data.len() >= PARALLEL_MIN {
        output
            .data
            .par_chunks_mut(out_w)
            .enumerate()
            .for_each(|(index, row)| fill(index, row));
    } else {
        for (index, row) in output.data.chunks_mut(out_w).enumerate() {
            fill(index, row);
        }
    }
    Ok(vec![Value::Float(output)])
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::many_single_char_names,
    clippy::indexing_slicing,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
mod tests {
    use super::*;
    use crate::onnx::model::{AttrValue, Attribute};

    fn node(op: &str, attrs: &[(&str, Vec<i64>)]) -> Node {
        Node {
            op_type: op.into(),
            attributes: attrs
                .iter()
                .map(|(name, value)| Attribute {
                    name: (*name).into(),
                    value: AttrValue::Ints(value.clone()),
                })
                .collect(),
            ..Node::default()
        }
    }

    fn float(shape: Vec<usize>, data: Vec<f32>) -> Value {
        Value::Float(Tensor { shape, data })
    }

    /// The direct definition, for comparison with the row-major rewrite.
    fn reference(x: &Tensor, w: &Tensor, stride: usize, pad: usize, group: usize) -> Tensor {
        let (n, c, h, wi) = (x.shape[0], x.shape[1], x.shape[2], x.shape[3]);
        let (m, cg, kh, kw) = (w.shape[0], w.shape[1], w.shape[2], w.shape[3]);
        let oh = (h + 2 * pad - kh) / stride + 1;
        let ow = (wi + 2 * pad - kw) / stride + 1;
        let mut out = vec![0.0f32; n * m * oh * ow];
        for b in 0..n {
            for o in 0..m {
                let g = o / (m / group);
                for oy in 0..oh {
                    for ox in 0..ow {
                        let mut total = 0.0f32;
                        for k in 0..cg {
                            let ic = g * cg + k;
                            for ky in 0..kh {
                                for kx in 0..kw {
                                    let iy = (oy * stride + ky) as isize - pad as isize;
                                    let ix = (ox * stride + kx) as isize - pad as isize;
                                    if iy < 0 || ix < 0 || iy >= h as isize || ix >= wi as isize {
                                        continue;
                                    }
                                    total += w.data[((o * cg + k) * kh + ky) * kw + kx]
                                        * x.data
                                            [((b * c + ic) * h + iy as usize) * wi + ix as usize];
                                }
                            }
                        }
                        out[((b * m + o) * oh + oy) * ow + ox] = total;
                    }
                }
            }
        }
        Tensor {
            shape: vec![n, m, oh, ow],
            data: out,
        }
    }

    #[test]
    fn row_major_convolution_is_bit_identical_to_the_definition() {
        let pattern =
            |n: usize, k: f32| (0..n).map(|i| ((i * 37 % 23) as f32 - 11.0) * k).collect();
        for (stride, pad, group, c, m) in [
            (1, 1, 1, 3, 4),
            (2, 1, 1, 2, 3),
            (1, 0, 1, 2, 2),
            (2, 1, 4, 4, 4),
            (1, 2, 2, 4, 2),
        ] {
            let x = Tensor {
                shape: vec![1, c, 7, 9],
                data: pattern(c * 63, 0.1),
            };
            let w = Tensor {
                shape: vec![m, c / group, 3, 3],
                data: pattern(m * c / group * 9, 0.07),
            };
            let mut n = node(
                "Conv",
                &[
                    ("strides", vec![stride as i64; 2]),
                    ("pads", vec![pad as i64; 4]),
                ],
            );
            n.attributes.push(Attribute {
                name: "group".into(),
                value: AttrValue::Int(group as i64),
            });
            let got = conv(
                &n,
                &[
                    Some(&Value::Float(x.clone())),
                    Some(&Value::Float(w.clone())),
                ],
            )
            .unwrap();
            let got = got[0].as_float().unwrap();
            let want = reference(&x, &w, stride, pad, group);
            assert_eq!(got.shape, want.shape);
            let bits = |t: &Tensor| t.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
            assert_eq!(
                bits(got),
                bits(&want),
                "stride {stride} pad {pad} group {group}"
            );
        }
    }

    #[test]
    fn transposed_convolution_scatters_each_input_into_its_kernel_footprint() {
        // One input channel, two output channels, a 2x2 kernel at stride 2: every input
        // sample becomes a 2x2 block of itself times the kernel, plus the bias.
        let x = float(vec![1, 1, 2, 2], vec![1.0, 2.0, 3.0, 4.0]);
        let w = float(
            vec![1, 2, 2, 2],
            vec![1.0, 2.0, 3.0, 4.0, -1.0, 0.0, 0.0, 1.0],
        );
        let b = float(vec![2], vec![0.5, 0.0]);
        let n = node("ConvTranspose", &[("strides", vec![2, 2])]);
        let out = conv_transpose(&n, &[Some(&x), Some(&w), Some(&b)]).unwrap();
        let out = out[0].as_float().unwrap();
        assert_eq!(out.shape, vec![1, 2, 4, 4]);
        assert_eq!(
            &out.data[..16],
            &[1.5, 2.5, 2.5, 4.5, 3.5, 4.5, 6.5, 8.5, 3.5, 6.5, 4.5, 8.5, 9.5, 12.5, 12.5, 16.5]
        );
        assert_eq!(&out.data[16..20], &[-1.0, 0.0, -2.0, 0.0]);
        // Overlapping taps at stride 1 sum in a fixed order.
        let n = node("ConvTranspose", &[("strides", vec![1, 1])]);
        let one = float(vec![1, 1, 1, 2], vec![1.0, 1.0]);
        let w = float(vec![1, 1, 1, 2], vec![1.0, 1.0]);
        let out = conv_transpose(&n, &[Some(&one), Some(&w)]).unwrap();
        assert_eq!(out[0].as_float().unwrap().data, vec![1.0, 2.0, 1.0]);
        let padded = node("ConvTranspose", &[("output_padding", vec![1, 1])]);
        assert!(conv_transpose(&padded, &[Some(&one), Some(&w)]).is_err());
    }
}
