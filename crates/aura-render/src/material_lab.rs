//! Independent selection-bound material filters on linear Rec.2020 pixels.
//! Kernels read only selected pixels; no excluded colour can bleed into a repair.
#![allow(clippy::indexing_slicing)]
use crate::cpu::Frame;
use aura_recipe::{
    material_lab::{self, Operation, Tool},
    Recipe,
};
use rayon::prelude::*;

fn luma(p: [f32; 3]) -> f32 {
    p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593
}
fn pixel(rgb: &[f32], i: usize) -> [f32; 3] {
    [rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2]]
}
fn with_luma(p: [f32; 3], value: f32) -> [f32; 3] {
    let y = luma(p);
    if y > 1e-6 {
        p.map(|c| c * value.max(0.0) / y)
    } else {
        p
    }
}
fn median(values: &mut [f32]) -> f32 {
    values.sort_unstable_by(f32::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(0.0)
}
fn near(i: usize, dx: isize, dy: isize, w: usize, h: usize) -> usize {
    let x = (i % w).saturating_add_signed(dx).min(w - 1);
    let y = (i / w).saturating_add_signed(dy).min(h - 1);
    y * w + x
}

fn blur(src: &[f32], weights: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    let denominator = crate::bands::box_mean(weights, w, h, radius);
    let mut out = vec![0.0; src.len()];
    for c in 0..3 {
        let channel: Vec<_> = weights
            .iter()
            .enumerate()
            .map(|(i, a)| src[i * 3 + c] * a)
            .collect();
        let average = crate::bands::box_mean(&channel, w, h, radius);
        for i in 0..weights.len() {
            out[i * 3 + c] = if denominator[i] > 1e-6 {
                average[i] / denominator[i]
            } else {
                src[i * 3 + c]
            };
        }
    }
    out
}

/// Fit a selected illumination plane at bounded sample cost. Singular selections stay neutral.
fn plane(src: &[f32], weights: &[f32], w: usize, h: usize) -> Option<[f32; 3]> {
    let mut matrix = [[0.0_f64; 4]; 3];
    let step = weights.len().div_ceil(65_536).max(1);
    for i in (0..weights.len()).step_by(step) {
        let a = f64::from(weights[i]);
        let position = [
            1.0,
            (i % w) as f64 / w as f64 - 0.5,
            (i / w) as f64 / h as f64 - 0.5,
        ];
        let y = f64::from(luma(pixel(src, i)));
        for r in 0..3 {
            for c in 0..3 {
                matrix[r][c] += a * position[r] * position[c];
            }
            matrix[r][3] += a * position[r] * y;
        }
    }
    for col in 0..3 {
        let pivot =
            (col..3).max_by(|a, b| matrix[*a][col].abs().total_cmp(&matrix[*b][col].abs()))?;
        matrix.swap(col, pivot);
        let divisor = matrix[col][col];
        if divisor.abs() < 1e-8 {
            return None;
        }
        for value in matrix[col].iter_mut().skip(col) {
            *value /= divisor;
        }
        let pivot_row = matrix[col];
        for (row, values) in matrix.iter_mut().enumerate() {
            if row == col {
                continue;
            }
            let scale = values[col];
            for (value, pivot_value) in values.iter_mut().zip(pivot_row).skip(col) {
                *value -= scale * pivot_value;
            }
        }
    }
    Some([
        matrix[0][3] as f32,
        matrix[1][3] as f32,
        matrix[2][3] as f32,
    ])
}

fn biases(
    src: &[f32],
    base: &[f32],
    weights: &[f32],
    w: usize,
    h: usize,
    op: &Operation,
) -> Vec<f32> {
    let vertical = op.angle.to_radians().sin().abs() > 0.707;
    let count = if op.tool == Tool::Checkerboard {
        4
    } else if vertical {
        w
    } else {
        h
    };
    let mut totals = vec![[0.0_f64; 2]; count];
    for (i, &a) in weights.iter().enumerate() {
        let residual = luma(pixel(src, i)) - luma(pixel(base, i));
        if residual.abs() > op.threshold {
            continue;
        }
        let group = if op.tool == Tool::Checkerboard {
            (i % w % 2) + (i / w % 2) * 2
        } else if vertical {
            i % w
        } else {
            i / w
        };
        totals[group][0] += f64::from(a * residual);
        totals[group][1] += f64::from(a);
    }
    totals
        .into_iter()
        .map(|v| {
            if v[1] > 0.0 {
                (v[0] / v[1]) as f32
            } else {
                0.0
            }
        })
        .collect()
}

struct Kernel<'a> {
    src: &'a [f32],
    weights: &'a [f32],
    width: usize,
    height: usize,
    op: &'a Operation,
    radius: usize,
    direction: [f32; 2],
    base: Vec<f32>,
    fine: Vec<f32>,
    fitted: Option<[f32; 3]>,
    bias: Vec<f32>,
}

impl<'a> Kernel<'a> {
    fn new(src: &'a [f32], weights: &'a [f32], w: usize, h: usize, op: &'a Operation) -> Self {
        let radius = (op.radius * w.min(h) as f32).round().max(1.0) as usize;
        let needs_blur = !matches!(
            op.tool,
            Tool::DirectionalSmooth
                | Tool::LumaMoire
                | Tool::ChromaMedian
                | Tool::ImpulseRepair
                | Tool::LineScratch
                | Tool::Ringing
                | Tool::IlluminationPlane
        );
        let base = if needs_blur {
            blur(src, weights, w, h, radius)
        } else {
            Vec::new()
        };
        let fine = if matches!(op.tool, Tool::FabricBands | Tool::ChromaMottle) {
            blur(src, weights, w, h, (radius / 4).max(1))
        } else {
            Vec::new()
        };
        let fitted = (op.tool == Tool::IlluminationPlane)
            .then(|| plane(src, weights, w, h))
            .flatten();
        let bias = if matches!(op.tool, Tool::Destripe | Tool::Checkerboard) {
            biases(src, &base, weights, w, h, op)
        } else {
            Vec::new()
        };
        let (sin, cos) = op.angle.to_radians().sin_cos();
        Self {
            src,
            weights,
            width: w,
            height: h,
            op,
            radius,
            direction: [sin, cos],
            base,
            fine,
            fitted,
            bias,
        }
    }

    fn nearby(&self, i: usize, dx: isize, dy: isize) -> usize {
        near(i, dx, dy, self.width, self.height)
    }

    fn neighbors(&self, i: usize) -> ([[f32; 3]; 24], usize) {
        let mut neighbors = [[0.0; 3]; 24];
        let mut count = 0;
        for dy in -2..=2 {
            for dx in -2..=2 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let j = self.nearby(i, dx, dy);
                if j != i && self.weights[j] > 0.01 {
                    neighbors[count] = pixel(self.src, j);
                    count += 1;
                }
            }
        }
        (neighbors, count)
    }

    fn directional(&self, i: usize, p: [f32; 3]) -> [f32; 3] {
        let mut sum = [0.0; 3];
        let mut total = 0.0;
        let steps = if self.op.tool == Tool::LumaMoire {
            1
        } else {
            6
        };
        let [sin, cos] = self.direction;
        for k in -steps..=steps {
            let distance = k as f32 * self.radius as f32 / steps as f32;
            let j = self.nearby(
                i,
                (cos * distance).round() as isize,
                (sin * distance).round() as isize,
            );
            let q = pixel(self.src, j);
            let range = if self.op.tool == Tool::LumaMoire {
                1.0
            } else {
                (1.0 - (luma(q) - luma(p)).abs() / self.op.threshold).max(0.0)
            };
            let a = self.weights[j] * range;
            for (sum, value) in sum.iter_mut().zip(q) {
                *sum += value * a;
            }
            total += a;
        }
        let average = if total > 1e-6 {
            sum.map(|v| v / total)
        } else {
            p
        };
        if self.op.tool == Tool::LumaMoire {
            with_luma(p, luma(average))
        } else {
            average
        }
    }

    fn chroma_median(&self, i: usize, p: [f32; 3]) -> [f32; 3] {
        let (neighbors, count) = self.neighbors(i);
        if count < 3 {
            return p;
        }
        let mut channels = [[0.0; 24]; 3];
        for (n, q) in neighbors.iter().take(count).enumerate() {
            let qy = luma(*q);
            for (c, channel) in channels.iter_mut().enumerate() {
                channel[n] = q[c] - qy;
            }
        }
        let y = luma(p);
        let q = channels.map(|mut values| y + median(&mut values[..count]));
        let delta = y - luma(q);
        q.map(|c| c + delta)
    }

    fn impulse_or_ringing(&self, i: usize, p: [f32; 3]) -> [f32; 3] {
        let (neighbors, count) = self.neighbors(i);
        if count < 4 {
            return p;
        }
        let mut values = [0.0_f32; 24];
        for (value, q) in values.iter_mut().zip(neighbors.iter()).take(count) {
            *value = luma(*q);
        }
        let middle = median(&mut values[..count]);
        let lo = values[count / 4];
        let hi = values[count * 3 / 4];
        let y = luma(p);
        if self.op.tool == Tool::ImpulseRepair {
            if (y - middle).abs() <= self.op.threshold || hi - lo >= self.op.threshold * 0.5 {
                return p;
            }
            let mut q = [0.0; 3];
            for (c, v) in q.iter_mut().enumerate() {
                for (value, neighbor) in values.iter_mut().zip(neighbors.iter()).take(count) {
                    *value = neighbor[c];
                }
                *v = median(&mut values[..count]);
            }
            q
        } else if hi - lo > self.op.threshold && (y < lo || y > hi) {
            with_luma(p, y.clamp(lo, hi))
        } else {
            p
        }
    }

    fn scratch(&self, i: usize, p: [f32; 3]) -> [f32; 3] {
        let [sin, cos] = self.direction;
        let (dx, dy) = if cos.abs() >= sin.abs() {
            (0, 1)
        } else {
            (1, 0)
        };
        let a_id = self.nearby(i, dx, dy);
        let b_id = self.nearby(i, -dx, -dy);
        let a = pixel(self.src, a_id);
        let b = pixel(self.src, b_id);
        let ay = luma(a);
        let by = luma(b);
        if self.weights[a_id] > 0.01
            && self.weights[b_id] > 0.01
            && (ay - by).abs() < self.op.threshold * 0.25
            && (luma(p) - (ay + by) * 0.5).abs() > self.op.threshold
        {
            [0, 1, 2].map(|c| (a[c] + b[c]) * 0.5)
        } else {
            p
        }
    }

    fn silver_hair(&self, p: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        let y = luma(p);
        let by = luma(b);
        let max = b.into_iter().fold(0.0_f32, f32::max).max(1e-6);
        let min = b.into_iter().fold(f32::MAX, f32::min);
        let neutral = (1.0 - (max - min) / max * 3.0).clamp(0.0, 1.0);
        let high = ((by - self.op.threshold) / (by + self.op.threshold)).clamp(0.0, 1.0);
        let specular_guard = by / (by + (y - by).max(0.0) * 4.0).max(1e-6);
        with_luma(p, y - by * high * neutral * specular_guard * 0.65)
    }

    fn specular_chroma(&self, p: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        let y = luma(p);
        let peak = p.into_iter().fold(0.0_f32, f32::max).max(1e-6);
        let chroma = (peak - p.into_iter().fold(f32::MAX, f32::min)) / peak;
        let gate = ((y - luma(b)) / self.op.threshold).clamp(0.0, 1.0) * (1.0 - chroma).powi(2);
        let diffuse = with_luma(b, y);
        [0, 1, 2].map(|c| p[c] + (diffuse[c] - p[c]) * gate)
    }

    fn channel_detail(&self, p: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        let mut residuals = [0, 1, 2].map(|c| (p[c] - b[c]) / b[c].max(0.01));
        let shared = median(&mut residuals).clamp(-0.9, 1.0);
        // Only inconsistent channels borrow structure. Replacing all three with
        // one common gain would cancel that gain when restoring luminance.
        let q = [0, 1, 2].map(|c| {
            let residual = (p[c] - b[c]) / b[c].max(0.01);
            let gate = ((residual - shared).abs() / self.op.threshold - 1.0).clamp(0.0, 1.0);
            let replacement = b[c] * (1.0 + shared * self.op.detail).max(0.0);
            p[c] + (replacement - p[c]) * gate
        });
        with_luma(q, luma(p))
    }

    fn target(&self, i: usize, p: [f32; 3]) -> [f32; 3] {
        let y = luma(p);
        let b = if self.base.is_empty() {
            p
        } else {
            pixel(&self.base, i)
        };
        let by = luma(b);
        let op = self.op;
        match op.tool {
            Tool::DirectionalSmooth | Tool::LumaMoire => self.directional(i, p),
            Tool::ChromaMedian => self.chroma_median(i, p),
            Tool::ImpulseRepair | Tool::Ringing => self.impulse_or_ringing(i, p),
            Tool::LineScratch => self.scratch(i, p),
            Tool::SilverHair => self.silver_hair(p, b),
            Tool::SpecularChroma => self.specular_chroma(p, b),
            Tool::ChannelDetail => self.channel_detail(p, b),
            Tool::FabricBands => {
                let f = pixel(&self.fine, i);
                [0, 1, 2].map(|c| b[c] + (f[c] - b[c]) * op.coarse + (p[c] - f[c]) * op.detail)
            }
            Tool::Deband => {
                let gate = (1.0 - (y - by).abs() / op.threshold)
                    .clamp(0.0, 1.0)
                    .powi(2);
                [0, 1, 2].map(|c| p[c] + (b[c] - p[c]) * gate)
            }
            Tool::Destripe | Tool::Checkerboard => {
                let group = if op.tool == Tool::Checkerboard {
                    (i % self.width % 2) + (i / self.width % 2) * 2
                } else if self.direction[0].abs() > 0.707 {
                    i % self.width
                } else {
                    i / self.width
                };
                with_luma(p, y - self.bias[group])
            }
            Tool::ChromaMottle => {
                let f = pixel(&self.fine, i);
                let fy = luma(f);
                [0, 1, 2].map(|c| p[c] - ((f[c] - fy) - (b[c] - by)))
            }
            Tool::IlluminationPlane => self.fitted.map_or(p, |coeff| {
                let slope = coeff[1] * ((i % self.width) as f32 / self.width as f32 - 0.5)
                    + coeff[2] * ((i / self.width) as f32 / self.height as f32 - 0.5);
                with_luma(p, y - slope.clamp(-op.threshold, op.threshold))
            }),
        }
    }
}

fn filtered(src: &[f32], weights: &[f32], w: usize, h: usize, op: &Operation) -> Vec<f32> {
    let kernel = Kernel::new(src, weights, w, h, op);
    let mut output = src.to_vec();
    output.par_chunks_mut(3).enumerate().for_each(|(i, dst)| {
        let alpha = weights[i] * op.amount;
        if alpha <= 0.0 {
            return;
        }
        let p = pixel(src, i);
        let target = kernel.target(i, p);
        for (c, value) in dst.iter_mut().enumerate() {
            let blended = p[c] + (target[c] - p[c]) * alpha;
            if blended.is_finite() {
                *value = blended.max(0.0);
            }
        }
    });
    output
}

/// Material selections honor their mask's enabled state and opacity. Missing masks do nothing.
pub(crate) fn apply(rgb: &mut [f32], w: u32, h: u32, recipe: &Recipe, frame: Option<&Frame>) {
    if w == 0 || h == 0 || rgb.len() != w as usize * h as usize * 3 {
        return;
    }
    let Ok(operations) = material_lab::read(recipe) else {
        return;
    };
    let Ok(masks) = aura_recipe::local_masks::read(recipe) else {
        return;
    };
    for op in operations.iter().filter(|op| op.enabled && op.amount > 0.0) {
        let Some(mask) = masks
            .iter()
            .find(|m| m.id == op.mask_id && m.enabled && m.amount > 0.0)
        else {
            continue;
        };
        let Some(mut weights) = crate::local_masks::coverage(rgb, w, h, recipe, frame, &op.mask_id)
        else {
            continue;
        };
        for weight in &mut weights {
            *weight *= mask.amount;
        }
        let next = filtered(rgb, &weights, w as usize, h as usize, op);
        rgb.copy_from_slice(&next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn operation(tool: Tool) -> Operation {
        Operation {
            id: "one".into(),
            mask_id: "m".into(),
            tool,
            enabled: true,
            amount: 0.8,
            radius: 0.05,
            angle: 0.0,
            threshold: 0.04,
            detail: 1.0,
            coarse: 0.25,
        }
    }
    const TOOLS: [Tool; 15] = [
        Tool::DirectionalSmooth,
        Tool::SilverHair,
        Tool::FabricBands,
        Tool::LumaMoire,
        Tool::Deband,
        Tool::ChromaMedian,
        Tool::ImpulseRepair,
        Tool::LineScratch,
        Tool::Destripe,
        Tool::Checkerboard,
        Tool::Ringing,
        Tool::SpecularChroma,
        Tool::ChromaMottle,
        Tool::IlluminationPlane,
        Tool::ChannelDetail,
    ];
    fn chart() -> Vec<f32> {
        (0..32 * 32)
            .flat_map(|i| {
                let v = 0.25 + (i % 32) as f32 * 0.003 + (i / 32) as f32 * 0.001;
                [v + if i % 2 == 0 { 0.008 } else { -0.008 }, v, v * 0.7]
            })
            .collect()
    }
    #[test]
    fn every_tool_is_deterministic_neutral_at_zero_and_preserves_excluded_pixels() {
        let src = chart();
        let weights: Vec<_> = (0..1024)
            .map(|i| if i % 32 < 16 { 1.0 } else { 0.0 })
            .collect();
        for tool in TOOLS {
            let mut op = operation(tool);
            op.amount = 0.0;
            assert_eq!(filtered(&src, &weights, 32, 32, &op), src, "{tool:?}");
            op.amount = 0.8;
            let out = filtered(&src, &weights, 32, 32, &op);
            assert_eq!(out, filtered(&src, &weights, 32, 32, &op));
            assert!(out.iter().all(|v| v.is_finite() && *v >= 0.0));
            for (i, weight) in weights.iter().enumerate() {
                if *weight <= 0.0 {
                    assert_eq!(
                        pixel(&out, i).map(f32::to_bits),
                        pixel(&src, i).map(f32::to_bits)
                    );
                }
            }
        }
    }
    #[test]
    fn each_operator_changes_its_target_fixture() {
        for tool in TOOLS {
            let mut src = chart();
            if tool == Tool::SilverHair {
                src.fill(0.6);
            }
            if matches!(
                tool,
                Tool::ImpulseRepair | Tool::LineScratch | Tool::Ringing
            ) {
                src.fill(0.2);
                src[16 * 32 * 3..(16 * 32 + 32) * 3].fill(0.6);
                if tool != Tool::LineScratch {
                    src[(16 * 32 + 16) * 3..(16 * 32 + 17) * 3].fill(0.95);
                }
            }
            if tool == Tool::SpecularChroma {
                src[(16 * 32 + 16) * 3..(16 * 32 + 17) * 3].fill(0.9);
            }
            if tool == Tool::Ringing {
                for row in 0..32 {
                    for col in 17..32 {
                        for c in 0..3 {
                            src[(row * 32 + col) * 3 + c] = 0.6;
                        }
                    }
                }
                src[(16 * 32 + 16) * 3..(16 * 32 + 17) * 3].fill(0.95);
            }
            if tool == Tool::Destripe {
                for x in 0..32 {
                    for c in 0..3 {
                        src[(16 * 32 + x) * 3 + c] += 0.02;
                    }
                }
            }
            let weights = vec![1.0; 1024];
            let mut op = operation(tool);
            if tool == Tool::ImpulseRepair {
                src.fill(0.2);
                src[(16 * 32 + 16) * 3..(16 * 32 + 17) * 3].fill(0.95);
            }
            if tool == Tool::ChannelDetail {
                src[(16 * 32 + 16) * 3] += 0.15;
            }
            if tool == Tool::IlluminationPlane {
                op.threshold = 1.0;
            }
            let out = filtered(&src, &weights, 32, 32, &op);
            assert!(
                out.iter().zip(&src).any(|(a, b)| (a - b).abs() > 1e-5),
                "{tool:?} did not change its fixture"
            );
        }
    }
    #[test]
    fn kernels_cannot_read_excluded_color() {
        let mut a = chart();
        let mut b = a.clone();
        let weights: Vec<_> = (0..1024)
            .map(|i| if i % 32 < 16 { 1.0 } else { 0.0 })
            .collect();
        for (i, weight) in weights.iter().enumerate() {
            if *weight <= 0.0 {
                for c in 0..3 {
                    a[i * 3 + c] = 1.0;
                    b[i * 3 + c] = 20.0;
                }
            }
        }
        for tool in TOOLS {
            let x = filtered(&a, &weights, 32, 32, &operation(tool));
            let y = filtered(&b, &weights, 32, 32, &operation(tool));
            for (i, weight) in weights.iter().enumerate() {
                if *weight > 0.0 {
                    for c in 0..3 {
                        assert!((x[i * 3 + c] - y[i * 3 + c]).abs() < 1e-5, "{tool:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn channel_reconstruction_controls_affect_inconsistent_shared_detail() {
        let mut src = vec![0.25; 32 * 32 * 3];
        for row in 0..32 {
            for col in 14..19 {
                let i = (row * 32 + col) * 3;
                src[i..i + 3].copy_from_slice(&[0.4, 0.45, 0.45]);
            }
        }
        let center = 16 * 32 + 16;
        src[center * 3] = 0.1;
        let weights = vec![1.0; 32 * 32];
        let mut op = operation(Tool::ChannelDetail);
        op.amount = 1.0;
        op.radius = 0.1;
        op.threshold = 0.02;
        op.detail = 0.0;
        let zero_detail = filtered(&src, &weights, 32, 32, &op);
        op.detail = 1.5;
        let detail = filtered(&src, &weights, 32, 32, &op);
        op.threshold = 1.0;
        let guarded = filtered(&src, &weights, 32, 32, &op);
        assert!((detail[center * 3] - zero_detail[center * 3]).abs() > 1e-4);
        assert!((detail[center * 3] - guarded[center * 3]).abs() > 1e-4);
        for output in [&zero_detail, &detail, &guarded] {
            assert!(output
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0));
            assert!((luma(pixel(output, center)) - luma(pixel(&src, center))).abs() < 1e-5);
        }
    }

    #[test]
    fn fractional_selection_scales_a_real_correction_once() {
        let src = vec![0.6; 8 * 8 * 3];
        let op = operation(Tool::SilverHair);
        let full = filtered(&src, &[1.0; 64], 8, 8, &op);
        let quarter = filtered(&src, &[0.25; 64], 8, 8, &op);
        assert!((full[0] - src[0]).abs() > 0.01);
        for ((full, quarter), original) in full.iter().zip(&quarter).zip(&src) {
            assert!((quarter - (original + (full - original) * 0.25)).abs() < 1e-5);
        }
    }

    #[test]
    fn degenerate_dark_and_hdr_selections_stay_finite_and_unselected_pixels_are_exact() {
        for src in [[0.0; 3], [1e-10; 3], [8.0, 4.0, 2.0]] {
            for tool in TOOLS {
                let op = operation(tool);
                let output = filtered(&src, &[1.0], 1, 1, &op);
                assert!(
                    output
                        .iter()
                        .all(|value| value.is_finite() && *value >= 0.0),
                    "{tool:?}"
                );
                let excluded = filtered(&src, &[0.0], 1, 1, &op);
                assert_eq!(
                    excluded.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    src.map(f32::to_bits)
                );
            }
        }
    }
}
