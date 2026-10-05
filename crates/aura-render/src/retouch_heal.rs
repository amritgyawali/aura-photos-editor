//! Texture transfer with bounded donor search and a coarse harmonic tone correction.
//! This is an independent local algorithm, not learned blemish classification. ADR-0076.
// Buffers come from dimension-checked input; all coordinates are bounded before indexing.
#![allow(clippy::indexing_slicing)]
use crate::retouch_mask::Coverage;
use aura_recipe::retouch_tools::Edit;

mod texture;

const RING_SAMPLES: usize = 32;
const GRID_LIMIT: usize = 64;
const RELAXATION_PASSES: usize = 96;

#[derive(Debug, Clone, Copy)]
struct Image<'a> {
    rgb: &'a [f32],
    w: usize,
    h: usize,
}

impl Image<'_> {
    fn at(self, x: f32, y: f32) -> Option<[f32; 3]> {
        if x < 0.0 || y < 0.0 || x > (self.w - 1) as f32 || y > (self.h - 1) as f32 {
            return None;
        }
        Some(self.sample(x, y))
    }
    fn sample(self, x: f32, y: f32) -> [f32; 3] {
        let x = x.clamp(0.0, (self.w - 1) as f32);
        let y = y.clamp(0.0, (self.h - 1) as f32);
        let ix = x.floor() as usize;
        let iy = y.floor() as usize;
        let nx = (ix + 1).min(self.w - 1);
        let ny = (iy + 1).min(self.h - 1);
        let fx = x - ix as f32;
        let fy = y - iy as f32;
        std::array::from_fn(|c| {
            let a = self.rgb[(iy * self.w + ix) * 3 + c] * (1.0 - fx)
                + self.rgb[(iy * self.w + nx) * 3 + c] * fx;
            let b = self.rgb[(ny * self.w + ix) * 3 + c] * (1.0 - fx)
                + self.rgb[(ny * self.w + nx) * 3 + c] * fx;
            a * (1.0 - fy) + b * fy
        })
    }
}

fn difference(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|c| a[c] - b[c])
}
fn energy(p: [f32; 3]) -> f32 {
    p.iter().map(|v| v * v).sum::<f32>() / 3.0
}

fn donor_score(image: Image<'_>, edit: &Edit, offset: [f32; 2]) -> Option<f32> {
    let [cx, cy, rx, ry] = edit.region;
    let center = [cx * image.w as f32 - 0.5, cy * image.h as f32 - 0.5];
    let mut ring = Vec::with_capacity(RING_SAMPLES);
    let mut bias = [0.0; 3];
    for n in 0..RING_SAMPLES {
        let angle = std::f32::consts::TAU * n as f32 / RING_SAMPLES as f32;
        let x = center[0] + angle.cos() * rx * image.w as f32 * 1.3;
        let y = center[1] + angle.sin() * ry * image.h as f32 * 1.3;
        if let (Some(target), Some(source)) =
            (image.at(x, y), image.at(x + offset[0], y + offset[1]))
        {
            let delta = difference(target, source);
            for c in 0..3 {
                bias[c] += delta[c];
            }
            ring.push(delta);
        }
    }
    if ring.len() < 6 {
        return None;
    }
    bias = bias.map(|v| v / ring.len() as f32);
    let residual = ring
        .iter()
        .map(|p| energy(difference(*p, bias)))
        .sum::<f32>()
        / ring.len() as f32;
    // Penalize a donor's isolated center defect without comparing to the damaged target center.
    let sx = center[0] + offset[0];
    let sy = center[1] + offset[1];
    let donor = image.at(sx, sy)?;
    let step = (rx * image.w as f32).min(ry * image.h as f32).max(1.0) * 0.6;
    let mut neighbors = [0.0; 3];
    for (dx, dy) in [(step, 0.0), (-step, 0.0), (0.0, step), (0.0, -step)] {
        let p = image.at(sx + dx, sy + dy)?;
        for c in 0..3 {
            neighbors[c] += p[c] * 0.25;
        }
    }
    Some(residual + 0.2 * energy(bias) + 0.25 * energy(difference(donor, neighbors)))
}

fn choose_offset(image: Image<'_>, edit: &Edit, bounds: [usize; 4]) -> Option<[f32; 2]> {
    let [cx, cy, rx, ry] = edit.region;
    if let Some(p) = edit.source {
        return Some([(p[0] - cx) * image.w as f32, (p[1] - cy) * image.h as f32]);
    }
    let [x0, y0, x1, y1] = bounds;
    let mut best = None;
    let mut best_score = f32::INFINITY;
    for distance in [2.8, 4.2, 6.0, 8.0] {
        for direction in 0..16 {
            let angle = std::f32::consts::TAU * direction as f32 / 16.0;
            let offset = [
                angle.cos() * rx * image.w as f32 * distance,
                angle.sin() * ry * image.h as f32 * distance,
            ];
            // Automatic donors must be disjoint and fit in the image. Manual donors can overlap.
            if (offset[0].abs() < (x1 - x0) as f32 && offset[1].abs() < (y1 - y0) as f32)
                || x0 as f32 + offset[0] < 0.0
                || y0 as f32 + offset[1] < 0.0
                || (x1 - 1) as f32 + offset[0] > (image.w - 1) as f32
                || (y1 - 1) as f32 + offset[1] > (image.h - 1) as f32
            {
                continue;
            }
            if let Some(score) = donor_score(image, edit, offset) {
                // Nearer patches win near-ties, and fixed traversal makes every result repeatable.
                let score = score * (1.0 + distance * 0.01);
                if score < best_score {
                    best_score = score;
                    best = Some(offset);
                }
            }
        }
    }
    best
}

#[derive(Debug)]
struct ToneField {
    pixels: Vec<f32>,
    w: usize,
    h: usize,
    origin: [usize; 2],
    step: [f32; 2],
}

impl ToneField {
    fn at(&self, x: usize, y: usize) -> [f32; 3] {
        let px = ((x - self.origin[0]) as f32 / self.step[0]).min((self.w - 1) as f32);
        let py = ((y - self.origin[1]) as f32 / self.step[1]).min((self.h - 1) as f32);
        Image {
            rgb: &self.pixels,
            w: self.w,
            h: self.h,
        }
        .sample(px, py)
    }
}

#[derive(Clone, Copy)]
struct Donor {
    offset: [f32; 2],
    centre: [f32; 2],
    scale: f32,
    half_extent: [f32; 2],
}

impl Donor {
    fn at(self, image: Image<'_>, x: f32, y: f32) -> Option<[f32; 3]> {
        if self.scale >= 1.0 {
            return image.at(x + self.offset[0], y + self.offset[1]);
        }
        // Reflect the smaller clean source rather than stretching its pores into
        // large blurred blobs. Tone matching still follows the target boundary.
        let reflect = |v: f32, radius: f32| {
            let t = (v + radius).rem_euclid(4.0 * radius);
            if t <= 2.0 * radius {
                t - radius
            } else {
                3.0 * radius - t
            }
        };
        image.at(
            self.centre[0] + reflect(x - self.centre[0], self.half_extent[0]) + self.offset[0],
            self.centre[1] + reflect(y - self.centre[1], self.half_extent[1]) + self.offset[1],
        )
    }
}

fn tone_field(image: Image<'_>, coverage: &Coverage, source: Donor) -> ToneField {
    let [x0, y0, x1, y1] = coverage.bounds;
    let origin = [x0.saturating_sub(1), y0.saturating_sub(1)];
    let end = [x1.min(image.w - 1), y1.min(image.h - 1)];
    let w = (end[0] - origin[0] + 1).min(GRID_LIMIT);
    let h = (end[1] - origin[1] + 1).min(GRID_LIMIT);
    let step = [
        (end[0] - origin[0]) as f32 / (w - 1).max(1) as f32,
        (end[1] - origin[1]) as f32 / (h - 1).max(1) as f32,
    ]
    .map(|v| v.max(1.0));
    let mut nodes = vec![[0.0; 3]; w * h];
    let mut fixed = vec![false; w * h];
    let mut boundary_sum = [0.0; 3];
    let mut boundary_count = 0;
    for y in 0..h {
        for x in 0..w {
            let px = origin[0] as f32 + x as f32 * step[0];
            let py = origin[1] as f32 + y as f32 * step[1];
            let old = image.at(px, py);
            let donor = source.at(image, px, py);
            let boundary = x == 0
                || y == 0
                || x + 1 == w
                || y + 1 == h
                || coverage.at(px.round() as usize, py.round() as usize, image.w, image.h) <= 0.0;
            fixed[y * w + x] = boundary || old.is_none() || donor.is_none();
            if fixed[y * w + x] {
                if let (Some(a), Some(b)) = (old, donor) {
                    nodes[y * w + x] = difference(a, b);
                    for c in 0..3 {
                        boundary_sum[c] += nodes[y * w + x][c];
                    }
                    boundary_count += 1;
                }
            }
        }
    }
    let average = boundary_sum.map(|v| v / boundary_count.max(1) as f32);
    for (p, &is_fixed) in nodes.iter_mut().zip(&fixed) {
        if !is_fixed {
            *p = average;
        }
    }
    let mut next = nodes.clone();
    let wx = 1.0 / (step[0] * step[0]);
    let wy = 1.0 / (step[1] * step[1]);
    for _ in 0..RELAXATION_PASSES {
        for y in 1..h.saturating_sub(1) {
            for x in 1..w.saturating_sub(1) {
                let i = y * w + x;
                if fixed[i] {
                    continue;
                }
                for c in 0..3 {
                    next[i][c] = ((nodes[i - 1][c] + nodes[i + 1][c]) * wx
                        + (nodes[i - w][c] + nodes[i + w][c]) * wy)
                        / (2.0 * (wx + wy));
                }
            }
        }
        std::mem::swap(&mut nodes, &mut next);
    }
    ToneField {
        pixels: nodes.into_iter().flatten().collect(),
        w,
        h,
        origin,
        step,
    }
}

pub(crate) fn apply(rgb: &mut [f32], w: usize, h: usize, edit: &Edit, coverage: &Coverage) {
    let [x0, y0, x1, y1] = coverage.bounds;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let image = Image { rgb, w, h };
    let Some(offset) = choose_offset(image, edit, coverage.bounds) else {
        return;
    };
    let source = Donor {
        offset,
        centre: [
            edit.region[0] * w as f32 - 0.5,
            edit.region[1] * h as f32 - 0.5,
        ],
        scale: edit.source_scale,
        half_extent: [
            edit.region[2] * w as f32 * edit.source_scale,
            edit.region[3] * h as f32 * edit.source_scale,
        ],
    };
    let texture = edit
        .texture_heal
        .then(|| texture::TextureHeal::new(image, source))
        .flatten();
    let field = texture
        .is_none()
        .then(|| tone_field(image, coverage, source));
    let mut patches = Vec::with_capacity((x1 - x0) * (y1 - y0));
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * w + x) * 3;
            let old = [rgb[i], rgb[i + 1], rgb[i + 2]];
            let alpha = coverage.at(x, y, w, h) * edit.amount;
            let mut value = old;
            if alpha > 0.0 {
                if let Some(donor) = source.at(image, x as f32, y as f32) {
                    let target = if let Some(ref heal) = texture {
                        heal.at(image, source, x as f32, y as f32).unwrap_or(donor)
                    } else if let Some(ref field) = field {
                        let bias = field.at(x, y);
                        std::array::from_fn(|c| (donor[c] + bias[c]).max(0.0))
                    } else {
                        donor
                    };
                    value = std::array::from_fn(|c| old[c] + alpha * (target[c] - old[c]));
                }
            }
            patches.push(value);
        }
    }
    // Commit only after sampling is complete: overlapping manual donors never feed on new pixels.
    for (n, pixel) in patches.into_iter().enumerate() {
        let i = ((y0 + n / (x1 - x0)) * w + x0 + n % (x1 - x0)) * 3;
        rgb[i..i + 3].copy_from_slice(&pixel);
    }
}
