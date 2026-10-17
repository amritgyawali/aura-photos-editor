//! Lateral chromatic aberration, measured from the photograph itself. ADR-0125.
//!
//! A lens bends red, green and blue by slightly different amounts, so away from the centre the
//! three images of the same edge land a fraction of a pixel apart: a red rim on one side of
//! every edge and a cyan or blue one on the other, growing toward the corners. It is a radial
//! magnification of one channel against another, which is the whole of the model the renderer
//! corrects (`aura_render::geometry::correct_ca`, a radial scale per channel). Phase 23 can only
//! fill that model from a measured lens profile, and with no profile the fringes were left as
//! shot. A raw converter does not need the profile: the photograph shows the misregistration.
//!
//! **Read.** Only edges that cross the radius away from the centre say anything: lateral
//! aberration moves a channel along the radius, so an edge that runs along it hides the shift.
//! Each such edge is read across, a quarter of a pixel at a time, in all three channels; a
//! channel's edge is where it crosses half way between its own two sides. Only clean, single,
//! roughly colourless steps count - every channel stepping the same way by a comparable share
//! of its own level - because the edge of a red dress against a green lawn moves its red and
//! its green differently for reasons that have nothing to do with the lens.
//!
//! **Fit.** Each edge gives a shift in pixels at a radius. Lateral aberration is a shift
//! proportional to the radius, so the scale is the least-squares slope through the origin,
//! after a robust trim. A crossing at half height is unmoved by a channel that is merely
//! softer, so the purple haze of axial aberration and the soft chroma of a JPEG do not register
//! as a shift; a constant shift of one channel (a chroma siting offset) points the same way
//! everywhere, so opposite sides of the frame cancel and disagree.
//!
//! **Act only on what is visible, consistent and confirmed.** A channel is corrected when its
//! shift at the frame's corner is a third of a pixel or more, three standard errors from zero,
//! and the same sign in at least three quarters of the directions around the frame. Then
//! [`STRENGTH`] of it is corrected - an under-correction leaves a fainter fringe of the same
//! colour, an over-correction paints one of the opposite colour, and only the second looks like
//! a mistake. Finally the correction is rendered through the renderer's own resampler and the
//! edges measured again: if what is left is not less than [`MAX_LEFT`] of what was there, the
//! correction did not do what the measurement claimed and nothing is changed.
//!
//! Measured on the renderer's working frame (linear Rec.2020), because that is the space the
//! correction runs in. The same shift measured on display sRGB is not the same number: a
//! working-space red is a third sRGB green.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    // Pixel arithmetic uses the conventional short names (x, y, w, h, k, s).
    clippy::many_single_char_names
)]

use rayon::prelude::*;

/// Edges nearer the centre than this share of the half-diagonal are not read: the shift grows
/// with the radius, and near the centre it is smaller than the noise in reading it.
const MIN_RADIUS: f32 = 0.30;
/// Each edge is read this many pixels to either side ...
const REACH: f32 = 4.0;
/// ... at this spacing.
const STEP: f32 = 0.25;
/// Samples in one reading across an edge.
const SAMPLES: usize = 33;
/// Most candidate edges read, taken evenly through the frame. A busy frame has hundreds of
/// thousands; a few tens of thousands fit a scale to a hundredth of a pixel.
const MAX_CANDIDATES: usize = 40_000;
/// The fewest clean edges an answer may come from.
pub const MIN_EDGES: usize = 80;
/// Directions around the frame, and the fewest of them that must hold edges.
const SECTORS: usize = 8;
const MIN_SECTORS: usize = 4;
/// Edges a direction needs before its own estimate counts.
const SECTOR_EDGES: usize = 6;
/// A shift smaller than this at the frame's corner, in pixels of the frame measured, is not
/// visible and is not corrected.
pub const VISIBLE: f32 = 0.30;
/// Further from one than this, a measured scale is not a lens: fourteen pixels at the corner of
/// a 24-megapixel frame.
const MAX_DEVIATION: f32 = 0.004;
/// How much of a measured misregistration is corrected.
pub const STRENGTH: f32 = 0.85;
/// What may be left of a corrected channel's shift when it is measured again.
pub const MAX_LEFT: f32 = 0.6;
/// The two channels measured against green, in the order every pair here is kept.
const NAMES: [&str; 2] = ["red", "blue"];
/// The share of directions that must agree with the overall sign.
const AGREEMENT: f32 = 0.75;
/// A strong edge: the green step across about two pixels is at least this share of its level.
const EDGE_CONTRAST: f32 = 0.25;

/// How far out of register red and blue are against green.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Registration {
    /// The size of the red and of the blue image relative to green: one is in register, more
    /// than one is a channel imaged larger, so its rim sits outside green's.
    pub scale: [f32; 2],
    /// The shift that makes at the frame's corner, in pixels of the frame measured.
    pub corner_shift: [f32; 2],
    /// Clean edges the fit kept.
    pub edges: usize,
    /// Directions around the frame that held edges.
    pub sectors: usize,
}

/// What was decided about a photograph's colour fringing.
#[derive(Debug, Clone, PartialEq)]
pub enum Fringing {
    /// Out of register by a visible, consistent amount, and the correction confirmed by
    /// measuring the corrected frame. `scales` are the renderer's `ca_red` and `ca_blue`.
    Corrected {
        scales: [f32; 2],
        before: Registration,
        after: Registration,
    },
    /// Measured, and already in register to within what anybody could see.
    InRegister(Registration),
    /// Not enough to go on, or a measurement the correction did not confirm.
    Unsure(String),
}

impl Fringing {
    /// The renderer's `ca_red` and `ca_blue`, when something is to be corrected.
    #[must_use]
    pub const fn scales(&self) -> Option<[f32; 2]> {
        match self {
            Self::Corrected { scales, .. } => Some(*scales),
            _ => None,
        }
    }

    /// One sentence for the explanation.
    #[must_use]
    pub fn sentence(&self) -> String {
        match self {
            Self::Corrected { before, after, .. } => format!(
                "Colour fringing corrected: {} clean edges in {} directions show red {:+.2} px and blue {:+.2} px out of register with green at the corners; {:.0}% of it is corrected, measured again at {:+.2} / {:+.2} px.",
                before.edges,
                before.sectors,
                before.corner_shift[0],
                before.corner_shift[1],
                STRENGTH * 100.0,
                after.corner_shift[0],
                after.corner_shift[1]
            ),
            Self::InRegister(r) => format!(
                "Colour fringing: {} clean edges in {} directions show the colour channels in register (red {:+.2} px, blue {:+.2} px at the corners), so nothing is corrected.",
                r.edges, r.sectors, r.corner_shift[0], r.corner_shift[1]
            ),
            Self::Unsure(why) => format!("Colour fringing left as shot: {why}."),
        }
    }
}

/// Measure, decide and confirm. `rgb` is the renderer's working frame, interleaved.
#[must_use]
pub fn decide(rgb: &[f32], width: usize, height: usize) -> Fringing {
    let before = match register(rgb, width, height) {
        Ok(fit) => fit,
        Err(why) => return Fringing::Unsure(why),
    };
    let channels = || {
        NAMES
            .iter()
            .zip(before.slope)
            .zip(before.visible)
            .zip(before.registration.corner_shift)
            .map(|(((name, slope), visible), shift)| (*name, slope, visible, shift))
    };
    if let Some((name, _, _, shift)) =
        channels().find(|(_, slope, _, _)| slope.abs() > MAX_DEVIATION)
    {
        return Fringing::Unsure(format!(
            "the {name} channel measured {shift:+.2} px out of register at the corners, more than any lens does, so the edges are not saying what a lens did"
        ));
    }
    if !before.visible.iter().any(|v| *v) {
        return Fringing::InRegister(before.registration);
    }
    let correct = |slope: f32, visible: bool| {
        if visible {
            1.0 + STRENGTH * slope
        } else {
            1.0
        }
    };
    let ([red, blue], [red_seen, blue_seen]) = (before.slope, before.visible);
    let scales = [correct(red, red_seen), correct(blue, blue_seen)];
    let mut corrected = rgb.to_vec();
    aura_render::geometry::correct_ca(&mut corrected, width, height, scales);
    let after = match register(&corrected, width, height) {
        Ok(fit) => fit,
        Err(why) => {
            return Fringing::Unsure(format!(
                "the corrected frame could not be measured again ({why})"
            ))
        }
    };
    let left = channels()
        .zip(after.slope.into_iter().zip(after.registration.corner_shift))
        .filter(|((_, _, visible, _), _)| *visible)
        .find(|((_, slope, _, _), (now, _))| now.abs() > MAX_LEFT * slope.abs());
    if let Some(((name, _, _, shift), (_, still))) = left {
        return Fringing::Unsure(format!(
            "the {name} channel measured {shift:+.2} px at the corners and still {still:+.2} px after correcting it, so the measurement is not a lens's"
        ));
    }
    Fringing::Corrected {
        scales,
        before: before.registration,
        after: after.registration,
    }
}

/// One edge crossing the radius: where it is, which way is out, and how far from the centre.
#[derive(Debug, Clone, Copy)]
struct Edge {
    x: f32,
    y: f32,
    out: [f32; 2],
    radius: f32,
    sector: usize,
}

/// One edge read: red's and blue's shift outward from green's, in pixels.
#[derive(Debug, Clone, Copy)]
struct Reading {
    radius: f32,
    sector: usize,
    shift: [f32; 2],
}

/// The fit over one frame.
#[derive(Debug, Clone, Copy)]
struct Fit {
    /// Shift per pixel of radius: the scale minus one.
    slope: [f32; 2],
    visible: [bool; 2],
    registration: Registration,
}

/// Bilinear sample of one channel at a continuous position (pixel centres at `x + 0.5`).
fn sample(rgb: &[f32], w: usize, h: usize, x: f32, y: f32, channel: usize) -> f32 {
    let fx = (x - 0.5).clamp(0.0, (w - 1) as f32);
    let fy = (y - 0.5).clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let at = |px: usize, py: usize| rgb.get((py * w + px) * 3 + channel).copied().unwrap_or(0.0);
    let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
    let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// Every strong green edge that crosses the radius, at its own centre, away from the middle.
fn edges(rgb: &[f32], w: usize, h: usize) -> Vec<Edge> {
    let margin = REACH as usize + 3;
    if w < 2 * margin + 8 || h < 2 * margin + 8 || rgb.len() < w * h * 3 {
        return Vec::new();
    }
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let half = cx.hypot(cy);
    let green = |x: usize, y: usize| rgb.get((y * w + x) * 3 + 1).copied().unwrap_or(0.0);
    let gradient = |x: usize, y: usize| -> [f32; 2] {
        let gx = green(x + 1, y - 1) + 2.0 * green(x + 1, y) + green(x + 1, y + 1)
            - green(x - 1, y - 1)
            - 2.0 * green(x - 1, y)
            - green(x - 1, y + 1);
        let gy = green(x - 1, y + 1) + 2.0 * green(x, y + 1) + green(x + 1, y + 1)
            - green(x - 1, y - 1)
            - 2.0 * green(x, y - 1)
            - green(x + 1, y - 1);
        [gx, gy]
    };
    let found: Vec<Edge> = (margin..h - margin)
        .into_par_iter()
        .flat_map_iter(|y| {
            let mut row = Vec::new();
            for x in margin..w - margin {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let radius = dx.hypot(dy);
                if radius < MIN_RADIUS * half {
                    continue;
                }
                let out = [dx / radius, dy / radius];
                let [gx, gy] = gradient(x, y);
                let strength = gx.hypot(gy);
                let level =
                    0.25 * (green(x - 1, y) + green(x + 1, y) + green(x, y - 1) + green(x, y + 1));
                // A Sobel response is four times the step across one pixel.
                if strength < 4.0 * EDGE_CONTRAST * (level + 0.01) {
                    continue;
                }
                let along = (gx * out[0] + gy * out[1]).abs();
                if along < 0.85 * strength {
                    continue;
                }
                // The edge's own centre: no stronger response one pixel in or out.
                let step = |sign: f32| -> f32 {
                    let nx = (x as f32 + sign * out[0]).round() as usize;
                    let ny = (y as f32 + sign * out[1]).round() as usize;
                    let [ax, ay] = gradient(nx, ny);
                    (ax * out[0] + ay * out[1]).abs()
                };
                if step(1.0) > along || step(-1.0) >= along {
                    continue;
                }
                let angle = dy.atan2(dx) + std::f32::consts::PI;
                let sector =
                    ((angle / std::f32::consts::TAU * SECTORS as f32) as usize).min(SECTORS - 1);
                row.push(Edge {
                    x: x as f32 + 0.5,
                    y: y as f32 + 0.5,
                    out,
                    radius,
                    sector,
                });
            }
            row
        })
        .collect();
    let stride = found.len().div_ceil(MAX_CANDIDATES).max(1);
    found.into_iter().step_by(stride).collect()
}

/// Read one edge across, in all three channels. `None` unless it is one clean, roughly
/// colourless step in every channel.
fn read(rgb: &[f32], w: usize, h: usize, edge: &Edge) -> Option<Reading> {
    let mut profile = [[0.0_f32; SAMPLES]; 3];
    for k in 0..SAMPLES {
        let s = -REACH + k as f32 * STEP;
        let (x, y) = (edge.x + s * edge.out[0], edge.y + s * edge.out[1]);
        for (c, channel) in profile.iter_mut().enumerate() {
            if let Some(slot) = channel.get_mut(k) {
                *slot = sample(rgb, w, h, x, y, c);
            }
        }
    }
    let mut crossing = [0.0_f32; 3];
    let mut contrast = [0.0_f32; 3];
    let mut rising = [false; 3];
    for c in 0..3 {
        let channel = profile.get(c)?;
        let low = channel.get(..4)?.iter().sum::<f32>() / 4.0;
        let high = channel.get(SAMPLES - 4..)?.iter().sum::<f32>() / 4.0;
        let step = high - low;
        if step.abs() < 1e-5 {
            return None;
        }
        *contrast.get_mut(c)? = step.abs() / (0.5 * (high + low).max(0.0) + 0.005);
        *rising.get_mut(c)? = step > 0.0;
        let norm = |v: f32| (v - low) / step;
        // A clean step: nothing strays far past its own two sides, and it crosses half way
        // exactly once.
        if channel.iter().any(|v| !(-0.3..=1.3).contains(&norm(*v))) {
            return None;
        }
        let mut found = None;
        for k in 1..SAMPLES {
            let (a, b) = (norm(*channel.get(k - 1)?), norm(*channel.get(k)?));
            if (a < 0.5) != (b < 0.5) {
                if found.is_some() {
                    return None;
                }
                let s = -REACH + (k - 1) as f32 * STEP;
                found = Some(s + STEP * (0.5 - a) / (b - a));
            }
        }
        *crossing.get_mut(c)? = found?;
    }
    let [red, green, blue] = contrast;
    if green < 2.0 * EDGE_CONTRAST {
        return None;
    }
    // Every channel steps the same way by a comparable share of its own level: an edge between
    // two colours moves its channels for reasons that are not the lens's.
    for (c, share) in [(0, red), (2, blue)] {
        if rising.get(c)? != rising.get(1)? || share < 0.5 * green || share > 2.0 * green {
            return None;
        }
    }
    let shift = [crossing[0] - crossing[1], crossing[2] - crossing[1]];
    if shift.iter().any(|d| d.abs() > 2.5) {
        return None;
    }
    Some(Reading {
        radius: edge.radius,
        sector: edge.sector,
        shift,
    })
}

fn median(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mid = values.len() / 2;
    let (_, value, _) = values.select_nth_unstable_by(mid, f32::total_cmp);
    *value
}

/// The radial slope of one channel's shift, robustly, with its standard error and the
/// per-direction estimates.
fn slope(readings: &[Reading], c: usize) -> (f32, f32, usize, Vec<f32>) {
    let shift = |r: &Reading| r.shift.get(c).copied().unwrap_or(0.0);
    let mut ratios: Vec<f32> = readings.iter().map(|r| shift(r) / r.radius).collect();
    let centre = median(&mut ratios);
    let mut residuals: Vec<f32> = readings
        .iter()
        .map(|r| (shift(r) - centre * r.radius).abs())
        .collect();
    let tolerance = (3.0 * 1.4826 * median(&mut residuals)).max(0.15);
    let kept: Vec<&Reading> = readings
        .iter()
        .filter(|r| (shift(r) - centre * r.radius).abs() <= tolerance)
        .collect();
    let fit = |set: &[&Reading]| -> f32 {
        let (sr, rr) = set.iter().fold((0.0_f64, 0.0_f64), |(sr, rr), r| {
            (
                sr + f64::from(shift(r)) * f64::from(r.radius),
                rr + f64::from(r.radius) * f64::from(r.radius),
            )
        });
        if rr > 0.0 {
            (sr / rr) as f32
        } else {
            0.0
        }
    };
    let k = fit(&kept);
    let n = kept.len();
    let (rss, rr) = kept.iter().fold((0.0_f64, 0.0_f64), |(rss, rr), r| {
        let e = f64::from(shift(r) - k * r.radius);
        (rss + e * e, rr + f64::from(r.radius) * f64::from(r.radius))
    });
    let se = if n > 1 && rr > 0.0 {
        ((rss / (n - 1) as f64) / rr).sqrt() as f32
    } else {
        f32::INFINITY
    };
    let sectors = (0..SECTORS)
        .filter_map(|s| {
            let set: Vec<&Reading> = kept.iter().copied().filter(|r| r.sector == s).collect();
            (set.len() >= SECTOR_EDGES).then(|| fit(&set))
        })
        .collect();
    (k, se, n, sectors)
}

/// Measure how far out of register red and blue are, over the whole frame.
fn register(rgb: &[f32], w: usize, h: usize) -> Result<Fit, String> {
    let candidates = edges(rgb, w, h);
    let readings: Vec<Reading> = candidates
        .par_iter()
        .filter_map(|edge| read(rgb, w, h, edge))
        .collect();
    if readings.len() < MIN_EDGES {
        return Err(format!(
            "only {} clean, colourless edges cross the radius away from the centre, and {MIN_EDGES} are needed",
            readings.len()
        ));
    }
    let half = (w as f32 * 0.5).hypot(h as f32 * 0.5);
    let mut slopes = [0.0_f32; 2];
    let mut visible = [false; 2];
    let mut kept = usize::MAX;
    let mut sectors = usize::MAX;
    for c in 0..2 {
        let (k, se, n, per_sector) = slope(&readings, c);
        let agree = per_sector
            .iter()
            .filter(|s| s.signum() == k.signum())
            .count() as f32
            / per_sector.len().max(1) as f32;
        if let Some(slot) = slopes.get_mut(c) {
            *slot = k;
        }
        if let Some(slot) = visible.get_mut(c) {
            *slot = per_sector.len() >= MIN_SECTORS
                && k.abs() * half >= VISIBLE
                && k.abs() >= 3.0 * se
                && agree >= AGREEMENT;
        }
        kept = kept.min(n);
        sectors = sectors.min(per_sector.len());
    }
    if sectors < MIN_SECTORS {
        return Err(format!(
            "clean edges were found in only {sectors} of {SECTORS} directions around the frame, and {MIN_SECTORS} are needed to tell a lens from a shifted channel"
        ));
    }
    Ok(Fit {
        slope: slopes,
        visible,
        registration: Registration {
            scale: [1.0 + slopes[0], 1.0 + slopes[1]],
            corner_shift: [slopes[0] * half, slopes[1] * half],
            edges: kept,
            sectors,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scene of many grey discs of different brightness on a grey ground, softened a little
    /// as a lens softens it, in linear light. Discs have edges in every direction.
    fn scene(w: usize, h: usize) -> Vec<f32> {
        let mut grey = vec![0.18_f32; w * h];
        let (cell, mut k) = (46_usize, 0_u32);
        for cy in (cell / 2..h).step_by(cell) {
            for cx in (cell / 2..w).step_by(cell) {
                k = k.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let level = [0.03, 0.06, 0.45, 0.7][(k >> 16) as usize % 4];
                let radius = 9.0 + ((k >> 20) % 9) as f32;
                for y in cy.saturating_sub(20)..(cy + 20).min(h) {
                    for x in cx.saturating_sub(20)..(cx + 20).min(w) {
                        let d = (x as f32 - cx as f32).hypot(y as f32 - cy as f32);
                        // Anti-aliased rim: a lens never draws a hard pixel edge.
                        let a = (radius + 0.5 - d).clamp(0.0, 1.0);
                        grey[y * w + x] = grey[y * w + x] * (1.0 - a) + level * a;
                    }
                }
            }
        }
        let soft = box3(&grey, w, h);
        soft.iter().flat_map(|v| [*v, *v, *v]).collect()
    }

    fn box3(plane: &[f32], w: usize, h: usize) -> Vec<f32> {
        let mut out = plane.to_vec();
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let mut s = 0.0;
                for dy in 0..3 {
                    for dx in 0..3 {
                        s += plane[(y + dy - 1) * w + x + dx - 1];
                    }
                }
                out[y * w + x] = s / 9.0;
            }
        }
        out
    }

    /// The same scene with red imaged `red` times as large as green and blue `blue` times, as a
    /// lens with lateral aberration images it.
    fn aberrated(clean: &[f32], w: usize, h: usize, red: f32, blue: f32) -> Vec<f32> {
        let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
        let mut out = clean.to_vec();
        for y in 0..h {
            for x in 0..w {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                for (c, m) in [(0, red), (2, blue)] {
                    // A channel imaged m times as large shows at p what lies at p / m.
                    let (sx, sy) = (cx + (px - cx) / m, cy + (py - cy) / m);
                    out[(y * w + x) * 3 + c] = sample(clean, w, h, sx, sy, c);
                }
            }
        }
        out
    }

    fn noisy(rgb: &mut [f32]) {
        let mut k = 7_u32;
        for v in rgb.iter_mut() {
            k = k.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *v *= 1.0 + ((k >> 8) as f32 / 16_777_216.0 - 0.5) * 0.02;
        }
    }

    const W: usize = 1200;
    const H: usize = 800;

    #[test]
    fn a_lens_that_images_red_large_and_blue_small_is_measured_and_corrected() {
        let clean = scene(W, H);
        let mut rgb = aberrated(&clean, W, H, 1.0015, 0.9988);
        noisy(&mut rgb);
        let decided = decide(&rgb, W, H);
        let Fringing::Corrected {
            scales,
            before,
            after,
        } = decided
        else {
            panic!("{decided:?}");
        };
        // Within a tenth of what the lens did, and corrected by STRENGTH of it.
        assert!((before.scale[0] - 1.0015).abs() < 0.00015, "{before:?}");
        assert!((before.scale[1] - 0.9988).abs() < 0.00015, "{before:?}");
        assert!(
            (scales[0] - (1.0 + STRENGTH * 0.0015)).abs() < 0.0002,
            "{scales:?}"
        );
        assert!(
            (scales[1] - (1.0 - STRENGTH * 0.0012)).abs() < 0.0002,
            "{scales:?}"
        );
        assert!(after.corner_shift[0].abs() < 0.4 * before.corner_shift[0].abs());
        assert_eq!(before.sectors, SECTORS);
    }

    #[test]
    fn channels_in_register_are_left_alone() {
        let mut rgb = scene(W, H);
        noisy(&mut rgb);
        let decided = decide(&rgb, W, H);
        assert!(matches!(decided, Fringing::InRegister(_)), "{decided:?}");
    }

    #[test]
    fn a_fringe_too_faint_to_see_is_left_alone() {
        // 0.0002 is under a fifth of a pixel at this frame's corner.
        let rgb = aberrated(&scene(W, H), W, H, 1.0002, 1.0);
        let decided = decide(&rgb, W, H);
        assert!(matches!(decided, Fringing::InRegister(_)), "{decided:?}");
    }

    #[test]
    fn coloured_edges_are_not_read_as_a_lens() {
        // Red discs on a green ground: every edge moves red and green in opposite directions,
        // and none of that is the lens.
        let grey = scene(W, H);
        let rgb: Vec<f32> = grey
            .chunks_exact(3)
            .flat_map(|p| {
                let v = p[1];
                [v * 1.6, 0.36 - v * 0.8, 0.05]
            })
            .collect();
        let decided = decide(&rgb, W, H);
        assert!(decided.scales().is_none(), "{decided:?}");
    }

    #[test]
    fn a_channel_shifted_the_same_way_everywhere_is_not_a_lens() {
        // Red half a pixel to the right across the whole frame, as a chroma siting offset
        // leaves it: opposite sides of the frame disagree about which way is out.
        let clean = scene(W, H);
        let mut rgb = clean.clone();
        for y in 0..H {
            for x in 0..W {
                rgb[(y * W + x) * 3] =
                    sample(&clean, W, H, x as f32 + 0.5 - 0.6, y as f32 + 0.5, 0);
            }
        }
        let decided = decide(&rgb, W, H);
        assert!(decided.scales().is_none(), "{decided:?}");
    }

    #[test]
    fn a_softer_blue_channel_is_not_a_shift() {
        // Axial aberration leaves blue softer than green, not displaced.
        let clean = scene(W, H);
        let blue: Vec<f32> = clean.chunks_exact(3).map(|p| p[2]).collect();
        let soft = box3(&box3(&blue, W, H), W, H);
        let mut rgb = clean;
        for (i, v) in soft.iter().enumerate() {
            rgb[i * 3 + 2] = *v;
        }
        let decided = decide(&rgb, W, H);
        assert!(decided.scales().is_none(), "{decided:?}");
    }

    /// The whole path: the planner measures the renderer's frame, writes the correction into a
    /// recipe, and the renderer's own output is then in register where the uncorrected render
    /// of the same frame is not.
    #[test]
    fn the_planner_writes_a_correction_the_renderer_brings_into_register() {
        use aura_render::{
            CpuEngine, Frame, FrameSource, OutputSpec, RenderLevel, RenderPurpose, RenderedData,
        };
        #[derive(Debug)]
        struct Source(Frame);
        impl FrameSource for Source {
            fn frame(
                &self,
                _: &aura_core::PhotoId,
                _: RenderLevel,
            ) -> aura_core::AuraResult<Frame> {
                Ok(self.0.clone())
            }
        }
        let rgb = aberrated(&scene(W, H), W, H, 1.0015, 0.9988);
        let encode = |v: f32| -> u8 {
            (aura_raw::colour::curve::srgb_encode(v.clamp(0.0, 1.0)) * 255.0).round() as u8
        };
        let bytes: Vec<u8> = rgb.iter().map(|v| encode(*v)).collect();
        let px = crate::portrait_features::Pixels::new(&bytes, W as u32, H as u32).unwrap();
        let frame = Frame::working(rgb, W as u32, H as u32, "");
        let plan = crate::smart_edit::analyse(&px, Some(&frame), &[], 0.0);
        assert!(
            plan.fringing.as_ref().and_then(Fringing::scales).is_some(),
            "{:?}",
            plan.fringing
        );
        assert!(plan
            .decisions
            .iter()
            .any(|d| d.starts_with("Colour fringing corrected")));

        let base = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "");
        let mut recipe = base.clone();
        assert!(crate::smart_edit::apply_fringing(&mut recipe, &plan));
        assert!(recipe.lens.ca && recipe.lens.coefficients.is_some());
        // Planned again, the recipe already carries it: nothing more is written.
        let mut again = recipe.clone();
        assert!(!crate::smart_edit::apply_fringing(&mut again, &plan));
        assert_eq!(again, recipe);

        let engine = CpuEngine::new(
            std::sync::Arc::new(Source(frame.clone())),
            aura_core::clock::FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
        );
        let render = |r: &aura_recipe::Recipe| -> Vec<f32> {
            let image = engine
                .render_frame(
                    &frame,
                    r,
                    RenderLevel::Full,
                    RenderPurpose::Analysis,
                    &OutputSpec::default(),
                )
                .unwrap();
            let RenderedData::Eight(data) = image.data else {
                panic!("an 8-bit render")
            };
            // Back into the working space before measuring: an sRGB red is 1.66 working reds
            // less 0.59 greens, and reads a working-space shift two thirds larger than it is.
            crate::photo_frames::srgb8_to_working(&data)
        };
        let as_shot = decide(&render(&base), W, H);
        let corrected = decide(&render(&recipe), W, H);
        assert!(as_shot.scales().is_some(), "{as_shot:?}");
        assert!(
            matches!(corrected, Fringing::InRegister(_)),
            "{corrected:?}"
        );
    }

    #[test]
    fn a_flat_frame_says_there_is_nothing_to_measure() {
        let rgb = vec![0.2_f32; W * H * 3];
        let decided = decide(&rgb, W, H);
        assert!(matches!(decided, Fringing::Unsure(_)), "{decided:?}");
        assert!(decided
            .sentence()
            .starts_with("Colour fringing left as shot"));
    }
}
