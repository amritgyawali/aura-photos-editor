//! Pixel measurements use the source preview; output is coverage, never generated pixels.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use super::{band, byte, check, invalid, neighbors, Action, CancelToken, Frame, Options, Plane};
use aura_core::AuraResult;
use std::{cmp::Reverse, collections::BinaryHeap, f32::consts::TAU};

fn lab(rgb: [f32; 3]) -> [f32; 3] {
    let linear = rgb.map(|v| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    let [red, green, blue] = linear;
    let transfer = |t: f32| {
        if t > 216.0 / 24389.0 {
            t.cbrt()
        } else {
            t * 841.0 / 108.0 + 4.0 / 29.0
        }
    };
    let (x, y, z) = (
        transfer((0.412_456_4 * red + 0.357_576_1 * green + 0.180_437_5 * blue) / 0.95047),
        transfer(0.212_672_9 * red + 0.715_152_2 * green + 0.072_175 * blue),
        transfer((0.019_333_9 * red + 0.119_192 * green + 0.950_304_1 * blue) / 1.08883),
    );
    [116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z)]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn diff(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn matvec(m: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    m.map(|row| dot(row, v))
}
#[allow(clippy::many_single_char_names)] // Conventional scalar entries keep the 3x3 adjugate formula auditable.
fn inverse(m: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let [[a, b, c], [d, e, f], [g, h, i]] = m;
    let adj = [
        [e * i - f * h, c * h - b * i, b * f - c * e],
        [f * g - d * i, a * i - c * g, c * d - a * f],
        [d * h - e * g, b * g - a * h, a * e - b * d],
    ];
    let det = (a * adj[0][0] + b * adj[1][0] + c * adj[2][0]).max(1e-18);
    adj.map(|row| row.map(|v| v / det))
}
fn patch(
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    radius: usize,
) -> impl Iterator<Item = usize> {
    (y.saturating_sub(radius)..(y + radius + 1).min(height)).flat_map(move |sy| {
        (x.saturating_sub(radius)..(x + radius + 1).min(width)).map(move |sx| sy * width + sx)
    })
}
fn otsu(hist: &[usize; 16], fallback: f32) -> f32 {
    let total = hist.iter().sum::<usize>() as f32;
    let sum: f32 = hist
        .iter()
        .enumerate()
        .map(|(i, n)| i as f32 * *n as f32)
        .sum();
    let (mut count, mut low_sum, mut best, mut threshold) = (0.0, 0.0, 0.0, fallback);
    for (i, n) in hist.iter().enumerate() {
        count += *n as f32;
        low_sum += i as f32 * *n as f32;
        if count > 0.0 && count < total {
            let score = count
                * (total - count)
                * (low_sum / count - (sum - low_sum) / (total - count)).powi(2);
            if score > best {
                best = score;
                threshold = (i as f32 + 1.0) / 16.0;
            }
        }
    }
    threshold
}
#[allow(clippy::too_many_lines)] // The measurement dispatch shares one sample covariance and local patch statistics across explicit operators.
pub(super) fn apply(
    plane: &Plane,
    action: Action,
    options: &Options,
    point: [f32; 2],
    frame: &Frame,
    cancel: &CancelToken,
) -> AuraResult<Vec<u8>> {
    let radius = options.radius as usize;
    let sx = ((point[0] * plane.width as f32) as usize).min(plane.width - 1);
    let sy = ((point[1] * plane.height as f32) as usize).min(plane.height - 1);
    let seed = sy * plane.width + sx;
    if action == Action::GeodesicColour {
        return geodesic(plane, options, frame, seed, cancel);
    }
    let target = lab(frame.rgb[seed]);
    let indices: Vec<_> = patch(plane.width, plane.height, sx, sy, radius).collect();
    let mut mean = [0.0; 3];
    for &i in &indices {
        for (v, c) in mean.iter_mut().zip(frame.rgb[i]) {
            *v += c / indices.len() as f32;
        }
    }
    let mut covariance = [[0.0; 3]; 3];
    for &i in &indices {
        let v = diff(frame.rgb[i], mean);
        for (row_index, row) in covariance.iter_mut().enumerate() {
            for (column_index, entry) in row.iter_mut().enumerate() {
                *entry += v[row_index] * v[column_index] / indices.len() as f32;
            }
        }
    }
    // Ridge regularization keeps rank deficient colour patches and flat black finite.
    for (i, row) in covariance.iter_mut().enumerate() {
        row[i] += 0.0001;
    }
    let inv = inverse(covariance);
    let mut axis = [1.0, 0.75, 0.5];
    for _ in 0..16 {
        axis = matvec(covariance, axis);
        let norm = dot(axis, axis).sqrt().max(1e-12);
        axis = axis.map(|v| v / norm);
    }
    let (sn, cs) = options.angle.to_radians().sin_cos();
    let mut out = vec![0; plane.alpha.len()];
    for y in 0..plane.height {
        check(cancel)?;
        for x in 0..plane.width {
            let i = y * plane.width + x;
            let v = frame.luminance[i];
            let metric = match action {
                Action::LabSampleDistance => {
                    let d = diff(lab(frame.rgb[i]), target);
                    // CIE76 distance; user tolerance maps to 150 perceptual units.
                    out[i] = byte((1.0 - dot(d, d).sqrt() / (150.0 * options.tolerance)).max(0.0));
                    continue;
                }
                Action::MahalanobisColour => {
                    let d = diff(frame.rgb[i], mean);
                    let distance = dot(d, matvec(inv, d)).max(0.0).sqrt();
                    out[i] = byte((1.0 - distance / (options.tolerance * 20.0)).max(0.0));
                    continue;
                }
                Action::ColourLineResidual => {
                    let d = diff(frame.rgb[i], mean);
                    (dot(d, d) - dot(d, axis).powi(2)).max(0.0).sqrt() / 3.0_f32.sqrt()
                }
                Action::DirectionalRidge => {
                    let at = |dx: isize, dy: isize| {
                        frame.luminance[y.saturating_add_signed(dy).min(plane.height - 1)
                            * plane.width
                            + x.saturating_add_signed(dx).min(plane.width - 1)]
                    };
                    let xx = at(-1, 0) - 2.0 * v + at(1, 0);
                    let yy = at(0, -1) - 2.0 * v + at(0, 1);
                    let xy = (at(1, 1) - at(1, -1) - at(-1, 1) + at(-1, -1)) * 0.25;
                    (-(xx * cs * cs + 2.0 * xy * sn * cs + yy * sn * sn) * 0.5).clamp(0.0, 1.0)
                }
                _ => {
                    let ids: Vec<_> = patch(plane.width, plane.height, x, y, radius).collect();
                    let n = ids.len() as f32;
                    let avg = ids.iter().map(|&j| frame.luminance[j]).sum::<f32>() / n;
                    let m2 = ids
                        .iter()
                        .map(|&j| (frame.luminance[j] - avg).powi(2))
                        .sum::<f32>()
                        / n;
                    match action {
                        Action::LocalOtsu => {
                            let mut hist = [0; 16];
                            for &j in &ids {
                                hist[((frame.luminance[j] * 16.0) as usize).min(15)] += 1;
                            }
                            out[i] = if v > otsu(&hist, avg) + 1e-6 { 255 } else { 0 };
                            continue;
                        }
                        Action::SauvolaThreshold => {
                            let threshold = avg * (1.0 + options.corner * (m2.sqrt() / 0.5 - 1.0));
                            out[i] = if v > 0.0 && v >= threshold { 255 } else { 0 };
                            continue;
                        }
                        Action::LocalSkewness => {
                            let m3 = ids
                                .iter()
                                .map(|&j| (frame.luminance[j] - avg).powi(3))
                                .sum::<f32>()
                                / n;
                            if m2 < 1e-10 {
                                0.5
                            } else {
                                (0.5 + m3 / m2.powf(1.5) / 12.0).clamp(0.0, 1.0)
                            }
                        }
                        Action::LocalKurtosis => {
                            let m4 = ids
                                .iter()
                                .map(|&j| (frame.luminance[j] - avg).powi(4))
                                .sum::<f32>()
                                / n;
                            if m2 < 1e-10 {
                                0.5
                            } else {
                                (0.5 + (m4 / (m2 * m2) - 3.0) / 12.0).clamp(0.0, 1.0)
                            }
                        }
                        Action::GaborResponse => {
                            let sigma = (radius as f32 * 0.5).max(0.5);
                            let wavelength = 2.0 + options.corner * 14.0;
                            let (mut response, mut weight) = (0.0, 0.0);
                            for &j in &ids {
                                let dx = (j % plane.width) as f32 - x as f32;
                                let dy = (j / plane.width) as f32 - y as f32;
                                let envelope = (-(dx * dx + dy * dy) / (2.0 * sigma * sigma)).exp();
                                response += (frame.luminance[j] - avg)
                                    * envelope
                                    * (TAU * (dx * cs + dy * sn) / wavelength).cos();
                                weight += envelope;
                            }
                            (response.abs() * 4.0 / weight.max(1e-6)).min(1.0)
                        }
                        _ => return Err(invalid("Unsupported source measurement.")),
                    }
                }
            };
            out[i] = byte(band(metric, options));
        }
    }
    Ok(out)
}
fn geodesic(
    p: &Plane,
    o: &Options,
    f: &Frame,
    seed: usize,
    cancel: &CancelToken,
) -> AuraResult<Vec<u8>> {
    // Integer edge costs and (cost,index) heap ties make propagation deterministic.
    let mut costs = vec![u32::MAX; p.alpha.len()];
    costs[seed] = 0;
    let mut queue = BinaryHeap::from([Reverse((0_u32, seed))]);
    let budget = (o.radius as f32 * 1024.0) as u32;
    while let Some(Reverse((cost, i))) = queue.pop() {
        if cost != costs[i] {
            continue;
        }
        check(cancel)?;
        for j in neighbors(i, p.width, p.height) {
            let d = diff(f.rgb[i], f.rgb[j]);
            let step = 1 + (dot(d, d).sqrt() * 1024.0 / o.tolerance) as u32;
            let next = cost.saturating_add(step);
            if next <= budget && next < costs[j] {
                costs[j] = next;
                queue.push(Reverse((next, j)));
            }
        }
    }
    Ok(costs
        .into_iter()
        .map(|c| {
            if c == u32::MAX {
                0
            } else {
                byte(1.0 - c as f32 / budget as f32)
            }
        })
        .collect())
}
