//! The Studio's finishing tools, rendered: feature colour and makeup, background replacement,
//! and face, body and liquify reshaping. ADR-0108.
//!
//! Runs after the retouch stack and before the Studio's local masks, on the whole frame:
//!
//! 1. **Colours** - hair, eyes, lips and brows take a chosen colour while keeping their own
//!    light and shade; blush and eyeshadow are a soft multiply placed from the face landmarks.
//! 2. **Background** - everything behind the people (or only the sky) is replaced by a colour, a
//!    gradient or a blur of itself. The person is kept by the portrait parse's background plane,
//!    feathered so a stray hair is blended rather than cut.
//! 3. **Shape** - every face slider, every body slider and every liquify stroke adds into one
//!    displacement field, and the frame is resampled through it once. One resample rather than
//!    one per slider, so twenty moves cost the same softness as one.
//!
//! The regions and landmarks come from `aura_portrait` and the frame as it arrived, exactly as
//! the portrait operators' do, so the four values a delivered file is re-created from still
//! decide every pixel. Nothing here runs unless a photographer moved a control: there is no
//! automatic reshape, and the automatic passes never write this extension.
//!
//! Every displacement primitive is bounded so the field cannot fold: a translation never moves
//! further than two fifths of its own radius, and a scale stays inside `-0.5..=0.5`. A slider at
//! 100 is a strong edit, never a tear.
// Every plane is `w * h` long and every index is clamped into it.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::similar_names
)]

use aura_portrait::{FaceGeometry, PortraitMap, Region};
use aura_raw::colour::curve::filmic_lite_inverse;
use aura_recipe::studio_finish::{
    self, Background, BackgroundMode, BodyShape, FaceShape, LiquifyMode, LiquifyStroke, Tint,
};
use aura_recipe::Recipe;
use rayon::prelude::*;

use crate::colour::{luma, set_luma};
use crate::cpu::Frame;

/// True when this recipe carries a finish that changes pixels.
#[must_use]
pub fn wants(recipe: &Recipe) -> bool {
    studio_finish::read(recipe).is_ok_and(|f| !f.is_identity())
}

/// Apply the finish a recipe carries. A no-op for a recipe without one, and for any part that
/// needs the portrait parse when the frame cannot supply one.
pub(crate) fn apply(
    rgb: &mut [f32],
    width: u32,
    height: u32,
    recipe: &Recipe,
    frame: Option<&Frame>,
) {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgb.len() != w * h * 3 {
        return;
    }
    let Ok(finish) = studio_finish::read(recipe) else {
        return;
    };
    if finish.is_identity() {
        return;
    }
    let map = if finish.needs_parse() {
        frame.and_then(|f| crate::portrait::parse(f, &crate::portrait::hints(recipe)))
    } else {
        None
    };
    if let Some(map) = map.as_deref() {
        apply_colours(rgb, w, h, &finish.colours, map);
        if let Some(background) = finish.background.filter(|b| b.amount > 0.0) {
            apply_background(rgb, w, h, background, map);
        }
    }
    let mut field = Field::new(w, h);
    if let Some(map) = map.as_deref() {
        let scale = Scale::of(map, w, h);
        if !finish.face.is_identity() {
            for face in &map.faces {
                face_warps(&mut field, &scale, face, &finish.face);
            }
        }
        if !finish.body.is_identity() {
            body_warps(&mut field, &scale, map, &finish.body);
        }
    }
    for stroke in &finish.liquify {
        liquify(&mut field, stroke);
    }
    if field.touched {
        let source = rgb.to_vec();
        field.resample(&source, rgb);
    }
}

// ---- colour -----------------------------------------------------------------------------

/// sRGB `0..=1` as linear BT.709.
fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// A picked sRGB colour as linear Rec.2020 *display* light: what the output transform encodes
/// back to the same sRGB value, so a chosen white exports as that white.
#[must_use]
pub fn picked_colour(srgb: [f32; 3]) -> [f32; 3] {
    let l = srgb.map(|v| srgb_to_linear(v.clamp(0.0, 1.0)));
    // BT.709 to BT.2020 primaries (ITU-R BT.2087).
    let m = [
        [0.627_4, 0.329_3, 0.043_3],
        [0.069_1, 0.919_5, 0.011_4],
        [0.016_4, 0.088_0, 0.895_6],
    ];
    let d = [0, 1, 2].map(|r| m[r][0] * l[0] + m[r][1] * l[1] + m[r][2] * l[2]);
    // Undo the output roll-off, so the value lands where the picker said; the inverse is
    // unbounded at one, so stop just short of it.
    d.map(|v| filmic_lite_inverse(v.clamp(0.0, 0.995)))
}

/// The same colour as a linear ratio with its brightest channel at one, for a multiply.
fn tint_ratio(srgb: [f32; 3]) -> [f32; 3] {
    let l = picked_colour(srgb);
    let top = l.iter().copied().fold(1e-6, f32::max);
    l.map(|v| (v / top).max(0.02))
}

/// A region resolved onto the buffer.
fn region(map: &PortraitMap, region: Region, w: usize, h: usize) -> Vec<f32> {
    crate::portrait::resolve(map, region, w as u32, h as u32)
}

/// Lay `tint` over a region, keeping each pixel's light and shade. `lift` moves the brightness
/// that share of the way (in log terms) toward the colour's own, which a hair dye needs and an
/// iris does not.
fn colour_blend(rgb: &mut [f32], weight: &[f32], tint: Tint, lift: f32) {
    let target = picked_colour(tint.colour);
    let target_luma = luma(target).max(1e-4);
    let amount = (tint.amount / 100.0).clamp(0.0, 1.0);
    rgb.par_chunks_mut(3)
        .zip(weight.par_iter())
        .for_each(|(p, k)| {
            let k = k.clamp(0.0, 1.0) * amount;
            if k <= 1e-4 {
                return;
            }
            let here = [p[0], p[1], p[2]];
            let l = luma(here).max(1e-5);
            let goal = (l.ln() + (target_luma.ln() - l.ln()) * lift * amount).exp();
            let coloured = set_luma(target, goal);
            for c in 0..3 {
                p[c] = here[c] + (coloured[c] - here[c]) * k;
            }
        });
}

/// Multiply a region by a colour, the way makeup sits on skin.
fn colour_multiply(rgb: &mut [f32], weight: &[f32], tint: Tint, depth: f32) {
    let ratio = tint_ratio(tint.colour);
    let amount = (tint.amount / 100.0).clamp(0.0, 1.0) * depth;
    rgb.par_chunks_mut(3)
        .zip(weight.par_iter())
        .for_each(|(p, k)| {
            let k = k.clamp(0.0, 1.0) * amount;
            if k <= 1e-4 {
                return;
            }
            for c in 0..3 {
                p[c] *= 1.0 + (ratio[c] - 1.0) * k;
            }
        });
}

/// An elliptical soft spot in buffer pixels: one at the centre, zero at the rim.
fn spot(plane: &mut [f32], w: usize, h: usize, centre: [f32; 2], radii: [f32; 2], angle: f32) {
    let (rx, ry) = (radii[0].max(1.0), radii[1].max(1.0));
    let reach = rx.max(ry);
    let x0 = (centre[0] - reach).floor().max(0.0) as usize;
    let x1 = ((centre[0] + reach).ceil().max(0.0) as usize).min(w);
    let y0 = (centre[1] - reach).floor().max(0.0) as usize;
    let y1 = ((centre[1] + reach).ceil().max(0.0) as usize).min(h);
    let (s, c) = angle.sin_cos();
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - centre[0];
            let dy = y as f32 + 0.5 - centre[1];
            let u = (dx * c + dy * s) / rx;
            let v = (-dx * s + dy * c) / ry;
            let r2 = u * u + v * v;
            if r2 < 1.0 {
                let f = (1.0 - r2) * (1.0 - r2);
                let slot = &mut plane[y * w + x];
                *slot = slot.max(f);
            }
        }
    }
}

fn apply_colours(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    colours: &studio_finish::Colours,
    map: &PortraitMap,
) {
    let live = |t: Option<Tint>| t.filter(|t| t.amount > 0.0);
    if let Some(t) = live(colours.hair) {
        colour_blend(rgb, &region(map, Region::Hair, w, h), t, 0.35);
    }
    if let Some(t) = live(colours.eyes) {
        colour_blend(rgb, &region(map, Region::Iris, w, h), t, 0.1);
    }
    if let Some(t) = live(colours.lips) {
        colour_blend(rgb, &region(map, Region::Lips, w, h), t, 0.15);
    }
    if let Some(t) = live(colours.eyebrows) {
        colour_blend(rgb, &region(map, Region::Eyebrows, w, h), t, 0.2);
    }
    if map.faces.is_empty() {
        return;
    }
    let scale = Scale::of(map, w, h);
    if let Some(t) = live(colours.blush) {
        let mut plane = vec![0.0_f32; w * h];
        for face in &map.faces {
            let f = scale.face(face);
            for side in [-1.0, 1.0] {
                spot(
                    &mut plane,
                    w,
                    h,
                    f.at(side * 0.8, f.mouth * 0.55),
                    [0.55 * f.io, 0.4 * f.io],
                    f.roll,
                );
            }
        }
        // On skin only, so a cheek's blush never lands on hair or a collar.
        let skin = region(map, Region::Skin, w, h);
        plane
            .par_iter_mut()
            .zip(skin.par_iter())
            .for_each(|(p, s)| *p *= s.clamp(0.0, 1.0));
        colour_multiply(rgb, &plane, t, 0.45);
    }
    if let Some(t) = live(colours.eyeshadow) {
        let mut plane = vec![0.0_f32; w * h];
        for face in &map.faces {
            let f = scale.face(face);
            for eye in [f.left_eye, f.right_eye] {
                let lid = [eye[0] + f.v[0] * -0.2 * f.io, eye[1] + f.v[1] * -0.2 * f.io];
                spot(&mut plane, w, h, lid, [0.38 * f.io, 0.2 * f.io], f.roll);
            }
        }
        // Never over the eye itself.
        let eyes = region(map, Region::Eyes, w, h);
        plane
            .par_iter_mut()
            .zip(eyes.par_iter())
            .for_each(|(p, e)| *p *= 1.0 - e.clamp(0.0, 1.0));
        colour_multiply(rgb, &plane, t, 0.6);
    }
}

// ---- background -------------------------------------------------------------------------

fn short_edge(w: usize, h: usize) -> f32 {
    w.min(h) as f32
}

fn apply_background(rgb: &mut [f32], w: usize, h: usize, b: Background, map: &PortraitMap) {
    let which = if b.mode == BackgroundMode::Sky {
        Region::Sky
    } else {
        Region::Background
    };
    let mut weight = region(map, which, w, h);
    if weight.iter().all(|v| *v <= 1e-3) {
        return;
    }
    let feather = ((b.feather / 100.0) * 0.012 * short_edge(w, h)).round() as usize;
    if feather > 0 {
        weight = crate::spatial::blur_plane(&weight, w, h, feather);
    }
    let amount = (b.amount / 100.0).clamp(0.0, 1.0);
    match b.mode {
        BackgroundMode::Colour => {
            let c = picked_colour(b.colour);
            fill(rgb, &weight, amount, w, |_| c);
        }
        BackgroundMode::Gradient => {
            let (top, bottom) = (picked_colour(b.colour), picked_colour(b.colour2));
            fill(rgb, &weight, amount, w, |y| {
                lerp3(top, bottom, y as f32 / h.max(2) as f32)
            });
        }
        BackgroundMode::Sky => {
            // The gradient runs from the top of the frame to the lowest row that is still sky.
            let horizon = (0..h)
                .rev()
                .find(|y| weight[y * w..(y + 1) * w].iter().any(|v| *v > 0.5))
                .unwrap_or(h / 2)
                .max(1);
            let (top, low) = (picked_colour(b.colour), picked_colour(b.colour2));
            // Keep the sky's own clouds as a faint texture so the replacement is not flat.
            let base = crate::spatial::luma_plane(rgb, w, h);
            let mean = {
                let (mut s, mut n) = (0.0_f64, 0.0_f64);
                for (l, k) in base.iter().zip(weight.iter()) {
                    s += f64::from(l * k);
                    n += f64::from(*k);
                }
                if n > 0.0 {
                    (s / n) as f32
                } else {
                    0.18
                }
            }
            .max(1e-4);
            rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
                let c = lerp3(top, low, (y as f32 / horizon as f32).min(1.0));
                for x in 0..w {
                    let k = weight[y * w + x].clamp(0.0, 1.0) * amount;
                    if k <= 1e-4 {
                        continue;
                    }
                    let texture = (base[y * w + x] / mean).clamp(0.6, 1.6).powf(0.35);
                    for ch in 0..3 {
                        let p = &mut row[x * 3 + ch];
                        *p += (c[ch] * texture - *p) * k;
                    }
                }
            });
        }
        BackgroundMode::Blur => {
            let radius = ((b.amount / 100.0) * 0.035 * short_edge(w, h))
                .round()
                .max(1.0) as usize;
            // A normalised convolution: the background blurred over the background only, so the
            // person does not bleed into it as a dark halo.
            let mut planes = [
                vec![0.0_f32; w * h],
                vec![0.0_f32; w * h],
                vec![0.0_f32; w * h],
            ];
            for (i, k) in weight.iter().enumerate() {
                for (c, plane) in planes.iter_mut().enumerate() {
                    plane[i] = rgb[i * 3 + c] * k;
                }
            }
            let norm = crate::spatial::blur_plane(&weight, w, h, radius);
            let blurred: Vec<Vec<f32>> = planes
                .iter()
                .map(|p| crate::spatial::blur_plane(p, w, h, radius))
                .collect();
            rgb.par_chunks_mut(3).enumerate().for_each(|(i, p)| {
                let k = weight[i].clamp(0.0, 1.0);
                if k <= 1e-4 || norm[i] <= 1e-4 {
                    return;
                }
                for c in 0..3 {
                    let soft = blurred[c][i] / norm[i];
                    p[c] += (soft - p[c]) * k;
                }
            });
        }
    }
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * t)
}

fn fill(
    rgb: &mut [f32],
    weight: &[f32],
    amount: f32,
    w: usize,
    colour: impl Fn(usize) -> [f32; 3] + Sync,
) {
    rgb.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        let c = colour(y);
        for x in 0..w {
            let k = weight[y * w + x].clamp(0.0, 1.0) * amount;
            if k <= 1e-4 {
                continue;
            }
            for ch in 0..3 {
                let p = &mut row[x * 3 + ch];
                *p += (c[ch] - *p) * k;
            }
        }
    });
}

// ---- the displacement field -------------------------------------------------------------

/// A backward displacement: the output pixel at `p` shows the input at `p + d(p)`.
struct Field {
    w: usize,
    h: usize,
    dx: Vec<f32>,
    dy: Vec<f32>,
    touched: bool,
}

/// One bounded deformation, in buffer pixels.
#[derive(Debug, Clone, Copy)]
enum Warp {
    /// Moves what is inside the ellipse by `v`, fully at the centre and not at all at the rim.
    Move {
        c: [f32; 2],
        r: [f32; 2],
        angle: f32,
        v: [f32; 2],
    },
    /// Enlarges (`a > 0`) or shrinks what is inside, separately along the ellipse's two axes.
    Scale {
        c: [f32; 2],
        r: [f32; 2],
        angle: f32,
        a: [f32; 2],
    },
    /// Takes the displacement already in the field back toward none.
    Restore { c: [f32; 2], r: f32, k: f32 },
}

/// The falloff every primitive shares: smooth, one at the centre, zero with zero slope at the rim.
fn falloff(r2: f32) -> f32 {
    if r2 >= 1.0 {
        0.0
    } else {
        (1.0 - r2) * (1.0 - r2)
    }
}

impl Field {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            dx: Vec::new(),
            dy: Vec::new(),
            touched: false,
        }
    }

    fn ensure(&mut self) {
        if self.dx.is_empty() {
            self.dx = vec![0.0; self.w * self.h];
            self.dy = vec![0.0; self.w * self.h];
        }
    }

    fn add(&mut self, warp: Warp) {
        let (c, reach) = match warp {
            Warp::Move { c, r, .. } | Warp::Scale { c, r, .. } => (c, r[0].max(r[1])),
            Warp::Restore { c, r, .. } => (c, r),
        };
        if !reach.is_finite() || reach < 0.5 || !c[0].is_finite() || !c[1].is_finite() {
            return;
        }
        let x0 = (c[0] - reach).floor().max(0.0) as usize;
        let x1 = ((c[0] + reach).ceil().max(0.0) as usize).min(self.w);
        let y0 = (c[1] - reach).floor().max(0.0) as usize;
        let y1 = ((c[1] + reach).ceil().max(0.0) as usize).min(self.h);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        self.ensure();
        self.touched = true;
        let w = self.w;
        let rows = self.dx[y0 * w..y1 * w]
            .par_chunks_mut(w)
            .zip(self.dy[y0 * w..y1 * w].par_chunks_mut(w));
        rows.enumerate().for_each(|(j, (rx_row, ry_row))| {
            let y = (y0 + j) as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5 - c[0];
                let py = y - c[1];
                match warp {
                    Warp::Move { r, angle, v, .. } => {
                        let (s, co) = angle.sin_cos();
                        let u = (px * co + py * s) / r[0];
                        let t = (-px * s + py * co) / r[1];
                        let f = falloff(u * u + t * t);
                        rx_row[x] -= v[0] * f;
                        ry_row[x] -= v[1] * f;
                    }
                    Warp::Scale { r, angle, a, .. } => {
                        let (s, co) = angle.sin_cos();
                        let lu = px * co + py * s;
                        let lt = -px * s + py * co;
                        let (u, t) = (lu / r[0], lt / r[1]);
                        let f = falloff(u * u + t * t);
                        if f > 0.0 {
                            // Sampling nearer the centre enlarges; further away shrinks.
                            let du = -lu * a[0] * f;
                            let dt = -lt * a[1] * f;
                            rx_row[x] += du * co - dt * s;
                            ry_row[x] += du * s + dt * co;
                        }
                    }
                    Warp::Restore { r, k, .. } => {
                        let f = falloff((px * px + py * py) / (r * r));
                        rx_row[x] *= 1.0 - k * f;
                        ry_row[x] *= 1.0 - k * f;
                    }
                }
            }
        });
    }

    /// A vertical stretch of everything below `from`, ramped in over `ramp` rows above it, so
    /// legs lengthen without a crease at the hip. `s > 0` lengthens.
    fn stretch_below(&mut self, from: f32, ramp: f32, s: f32, x_range: (f32, f32, f32)) {
        if s == 0.0 || !from.is_finite() {
            return;
        }
        self.ensure();
        self.touched = true;
        let w = self.w;
        let (left, right, soft) = x_range;
        let factor = 1.0 / (1.0 + s) - 1.0;
        self.dy.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            let y = y as f32 + 0.5;
            // Below `from` the stretch grows with distance; inside the ramp it eases in.
            let depth = if y >= from {
                y - from + ramp * 0.5
            } else if y > from - ramp {
                let t = (y - (from - ramp)) / ramp;
                t * t * ramp * 0.5
            } else {
                return;
            };
            for (x, slot) in row.iter_mut().enumerate() {
                let x = x as f32 + 0.5;
                let edge = ((x - left).min(right - x) / soft).clamp(0.0, 1.0);
                let k = edge * edge * (3.0 - 2.0 * edge);
                *slot += depth * factor * k;
            }
        });
    }

    fn resample(&self, src: &[f32], out: &mut [f32]) {
        let (w, h) = (self.w, self.h);
        out.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                let i = y * w + x;
                let sx = (x as f32 + self.dx[i]).clamp(0.0, (w - 1) as f32);
                let sy = (y as f32 + self.dy[i]).clamp(0.0, (h - 1) as f32);
                let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
                let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
                let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
                for c in 0..3 {
                    let a = src[(y0 * w + x0) * 3 + c];
                    let b = src[(y0 * w + x1) * 3 + c];
                    let d = src[(y1 * w + x0) * 3 + c];
                    let e = src[(y1 * w + x1) * 3 + c];
                    let top = a + (b - a) * fx;
                    let bottom = d + (e - d) * fx;
                    row[x * 3 + c] = top + (bottom - top) * fy;
                }
            }
        });
    }
}

/// A translation, bounded so it cannot fold: at most two fifths of the shorter radius.
fn moving(c: [f32; 2], r: [f32; 2], angle: f32, v: [f32; 2]) -> Warp {
    let cap = 0.4 * r[0].min(r[1]);
    let len = v[0].hypot(v[1]);
    let v = if len > cap && len > 0.0 {
        [v[0] * cap / len, v[1] * cap / len]
    } else {
        v
    };
    Warp::Move { c, r, angle, v }
}

/// A scale, bounded to `-0.5..=0.5` on each axis.
fn scaling(c: [f32; 2], r: [f32; 2], angle: f32, a: [f32; 2]) -> Warp {
    Warp::Scale {
        c,
        r,
        angle,
        a: a.map(|v| v.clamp(-0.5, 0.5)),
    }
}

// ---- faces ------------------------------------------------------------------------------

/// The analysis grid onto the buffer.
struct Scale {
    sx: f32,
    sy: f32,
}

/// One face in buffer pixels, in its own upright frame: `u` across, `v` down, in units of
/// the distance between the eyes.
struct Face {
    mid: [f32; 2],
    u: [f32; 2],
    v: [f32; 2],
    io: f32,
    roll: f32,
    left_eye: [f32; 2],
    right_eye: [f32; 2],
    /// Eye line to nose and to mouth, in interocular units.
    nose: f32,
    mouth: f32,
    /// Mouth width in interocular units.
    mouth_width: f32,
}

impl Scale {
    fn of(map: &PortraitMap, w: usize, h: usize) -> Self {
        Self {
            sx: w as f32 / map.width.max(1) as f32,
            sy: h as f32 / map.height.max(1) as f32,
        }
    }

    fn point(&self, p: [f32; 2]) -> [f32; 2] {
        [p[0] * self.sx, p[1] * self.sy]
    }

    fn face(&self, g: &FaceGeometry) -> Face {
        let k = (self.sx + self.sy) * 0.5;
        let left_eye = self.point(g.left_eye);
        let right_eye = self.point(g.right_eye);
        let mid = [
            (left_eye[0] + right_eye[0]) * 0.5,
            (left_eye[1] + right_eye[1]) * 0.5,
        ];
        let io = (g.interocular() * k).max(2.0);
        let (s, c) = g.roll.sin_cos();
        let u = [c, s];
        let v = [-s, c];
        let along = |p: [f32; 2]| ((p[0] - mid[0]) * v[0] + (p[1] - mid[1]) * v[1]) / io;
        // The landmarks are measured or placed by a prior; keep them where a face can be.
        let nose = along(self.point(g.nose)).clamp(0.4, 1.0);
        let mouth = along(self.point(g.mouth)).clamp(nose + 0.2, 1.6);
        Face {
            mid,
            u,
            v,
            io,
            roll: g.roll,
            left_eye,
            right_eye,
            nose,
            mouth,
            mouth_width: (g.mouth_width * k / io).clamp(0.4, 1.4),
        }
    }
}

impl Face {
    /// A point `x` across and `y` down from the eye line, in interocular units.
    fn at(&self, x: f32, y: f32) -> [f32; 2] {
        [
            self.mid[0] + (self.u[0] * x + self.v[0] * y) * self.io,
            self.mid[1] + (self.u[1] * x + self.v[1] * y) * self.io,
        ]
    }

    fn across(&self, k: f32) -> [f32; 2] {
        [self.u[0] * k * self.io, self.u[1] * k * self.io]
    }

    fn down(&self, k: f32) -> [f32; 2] {
        [self.v[0] * k * self.io, self.v[1] * k * self.io]
    }

    fn r(&self, x: f32, y: f32) -> [f32; 2] {
        [x * self.io, y * self.io]
    }
}

// One block per slider, in the order the panel lists them; splitting it would scatter the list.
#[allow(clippy::too_many_lines)]
fn face_warps(field: &mut Field, scale: &Scale, g: &FaceGeometry, s: &FaceShape) {
    let f = scale.face(g);
    let t = |v: f32| (v / 100.0).clamp(-1.0, 1.0);
    let m = f.mouth;
    // Whole head first, so the features move within the resized head.
    if s.head_size != 0.0 {
        field.add(scaling(
            f.at(0.0, m * 0.35),
            f.r(1.7, 2.1),
            f.roll,
            [t(s.head_size) * 0.16; 2],
        ));
    }
    for side in [-1.0_f32, 1.0] {
        // Inward is toward the midline: the opposite sign of the side.
        let inward = |k: f32| f.across(-side * k);
        if s.slim != 0.0 {
            field.add(moving(
                f.at(side * 1.05, m * 0.8),
                f.r(0.8, 1.05),
                f.roll,
                inward(t(s.slim) * 0.16),
            ));
        }
        if s.jaw != 0.0 {
            field.add(moving(
                f.at(side * 0.85, m + 0.4),
                f.r(0.6, 0.55),
                f.roll,
                inward(t(s.jaw) * 0.14),
            ));
        }
        if s.cheekbones != 0.0 {
            field.add(moving(
                f.at(side * 1.05, 0.45),
                f.r(0.55, 0.5),
                f.roll,
                inward(t(s.cheekbones) * 0.11),
            ));
        }
        let eye = if side < 0.0 { f.left_eye } else { f.right_eye };
        if s.eye_size != 0.0 {
            field.add(scaling(
                eye,
                f.r(0.55, 0.42),
                f.roll,
                [t(s.eye_size) * 0.32; 2],
            ));
        }
        if s.eye_distance != 0.0 {
            field.add(moving(
                eye,
                f.r(0.5, 0.45),
                f.roll,
                f.across(side * t(s.eye_distance) * 0.09),
            ));
        }
        let corner = f.at(side * f.mouth_width * 0.5, m);
        if s.mouth_width != 0.0 {
            field.add(moving(
                corner,
                f.r(0.38, 0.32),
                f.roll,
                f.across(side * t(s.mouth_width) * 0.1),
            ));
        }
        if s.smile != 0.0 {
            field.add(moving(
                corner,
                f.r(0.3, 0.3),
                f.roll,
                f.down(-t(s.smile) * 0.07),
            ));
        }
    }
    if s.chin != 0.0 {
        field.add(moving(
            f.at(0.0, m + 0.8),
            f.r(0.65, 0.6),
            f.roll,
            f.down(t(s.chin) * 0.17),
        ));
    }
    if s.forehead != 0.0 {
        field.add(moving(
            f.at(0.0, -1.25),
            f.r(1.25, 0.75),
            f.roll,
            f.down(-t(s.forehead) * 0.17),
        ));
    }
    if s.nose_width != 0.0 {
        field.add(scaling(
            f.at(0.0, f.nose),
            f.r(0.42, 0.45),
            f.roll,
            [-t(s.nose_width) * 0.3, 0.0],
        ));
    }
    if s.nose_length != 0.0 {
        field.add(moving(
            f.at(0.0, f.nose),
            f.r(0.4, 0.42),
            f.roll,
            f.down(t(s.nose_length) * 0.09),
        ));
    }
    if s.lips != 0.0 {
        field.add(scaling(
            f.at(0.0, m),
            f.r(f.mouth_width * 0.65, 0.34),
            f.roll,
            [t(s.lips) * 0.08, t(s.lips) * 0.3],
        ));
    }
}

// ---- the body ---------------------------------------------------------------------------

/// The person's bounds in buffer pixels, and their width at a given height.
struct Person {
    plane: Vec<f32>,
    w: usize,
    top: usize,
    bottom: usize,
    left: usize,
    right: usize,
}

impl Person {
    fn find(map: &PortraitMap, w: usize, h: usize) -> Option<Self> {
        let plane = region(map, Region::Body, w, h);
        let (mut top, mut bottom, mut left, mut right) = (h, 0, w, 0);
        for y in 0..h {
            for x in 0..w {
                if plane[y * w + x] > 0.5 {
                    top = top.min(y);
                    bottom = bottom.max(y);
                    left = left.min(x);
                    right = right.max(x);
                }
            }
        }
        (top < bottom && left < right).then_some(Self {
            plane,
            w,
            top,
            bottom,
            left,
            right,
        })
    }

    fn height(&self) -> f32 {
        (self.bottom - self.top) as f32
    }

    fn y_at(&self, share: f32) -> f32 {
        self.top as f32 + self.height() * share
    }

    /// The left and right edge of the person on one row.
    fn extent(&self, y: f32) -> Option<(f32, f32)> {
        let y = (y.max(0.0) as usize).min(self.bottom);
        let row = &self.plane[y * self.w..(y + 1) * self.w];
        let left = row.iter().position(|v| *v > 0.5)?;
        let right = row.iter().rposition(|v| *v > 0.5)?;
        (right > left).then_some((left as f32, right as f32))
    }
}

fn body_warps(field: &mut Field, scale: &Scale, map: &PortraitMap, s: &BodyShape) {
    let Some(person) = Person::find(map, field.w, field.h) else {
        return;
    };
    let t = |v: f32| (v / 100.0).clamp(-1.0, 1.0);
    let height = person.height();
    // Where the shoulders are: below the largest face when there is one.
    let face = map.faces.first().map(|g| scale.face(g));
    let shoulders_y = face
        .as_ref()
        .map_or_else(|| person.y_at(0.2), |f| f.at(0.0, f.mouth + 1.6)[1])
        .clamp(person.top as f32, person.bottom as f32);
    let below = |share: f32| shoulders_y + (person.bottom as f32 - shoulders_y) * share;
    let mut sides = |y: f32, slider: f32, reach: f32, depth: f32, outward: bool| {
        if slider == 0.0 {
            return;
        }
        let Some((left, right)) = person.extent(y) else {
            return;
        };
        let width = right - left;
        let r = [width * reach, height * 0.09];
        let push = width * depth * t(slider);
        let sign = if outward { -1.0 } else { 1.0 };
        field.add(moving([left, y], r, 0.0, [push * sign, 0.0]));
        field.add(moving([right, y], r, 0.0, [-push * sign, 0.0]));
    };
    sides(shoulders_y, s.shoulders, 0.22, 0.05, true);
    sides(below(0.22), s.arms, 0.18, 0.05, false);
    sides(below(0.38), s.waist, 0.3, 0.07, false);
    sides(below(0.52), s.hips, 0.3, 0.06, false);
    if s.slim != 0.0 {
        let cx = (person.left + person.right) as f32 * 0.5;
        let cy = below(0.4);
        let rx = (person.right - person.left) as f32 * 0.75;
        field.add(scaling(
            [cx, cy],
            [rx, height * 0.75],
            0.0,
            [-t(s.slim) * 0.12, 0.0],
        ));
    }
    if s.neck != 0.0 {
        if let Some(f) = &face {
            // The head rises; the falloff's lower edge sits on the shoulders, so the neck stretches.
            field.add(moving(
                f.at(0.0, f.mouth * 0.4),
                f.r(1.8, 2.2),
                f.roll,
                f.down(-t(s.neck) * 0.22),
            ));
        }
    }
    if s.legs != 0.0 {
        let hip = below(0.55);
        let margin = (person.right - person.left) as f32 * 0.6;
        field.stretch_below(
            hip,
            height * 0.08,
            t(s.legs) * 0.14,
            (
                person.left as f32 - margin,
                person.right as f32 + margin,
                margin.max(1.0),
            ),
        );
    }
}

// ---- liquify ----------------------------------------------------------------------------

fn liquify(field: &mut Field, stroke: &LiquifyStroke) {
    if stroke.strength <= 0.0 || stroke.points.is_empty() {
        return;
    }
    let (w, h) = (field.w as f32, field.h as f32);
    let radius = stroke.radius * w.min(h);
    let to_px = |p: [f32; 2]| [p[0] * w, p[1] * h];
    let k = stroke.strength.clamp(0.0, 1.0);
    match stroke.mode {
        LiquifyMode::Push => {
            // Each segment pushes the pixels under the brush along itself.
            for pair in stroke.points.windows(2) {
                let (a, b) = (to_px(pair[0]), to_px(pair[1]));
                let v = [(b[0] - a[0]) * k, (b[1] - a[1]) * k];
                if v[0].hypot(v[1]) > 0.01 {
                    field.add(moving(a, [radius; 2], 0.0, v));
                }
            }
        }
        LiquifyMode::Bloat | LiquifyMode::Pinch => {
            let sign = if stroke.mode == LiquifyMode::Bloat {
                1.0
            } else {
                -1.0
            };
            // Dabs at most a third of a radius apart, so a drag is one continuous change.
            let mut last: Option<[f32; 2]> = None;
            for p in &stroke.points {
                let p = to_px(*p);
                if last.is_some_and(|q| (p[0] - q[0]).hypot(p[1] - q[1]) < radius / 3.0) {
                    continue;
                }
                last = Some(p);
                field.add(scaling(p, [radius; 2], 0.0, [sign * k * 0.25; 2]));
            }
        }
        LiquifyMode::Restore => {
            for p in &stroke.points {
                field.add(Warp::Restore {
                    c: to_px(*p),
                    r: radius,
                    k: k * 0.5,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_recipe::studio_finish::StudioFinish;

    #[test]
    fn a_picked_white_lands_on_white_through_the_output_transform() {
        let c = picked_colour([1.0, 1.0, 1.0]);
        let out = crate::output::map_one(
            c,
            crate::output::working_to_output(crate::contract::render::OutputColour::Srgb),
            crate::contract::render::OutputColour::Srgb,
        );
        for v in out {
            assert!(v > 0.99, "{out:?}");
        }
        let grey = picked_colour([0.5, 0.5, 0.5]);
        let out = crate::output::map_one(
            grey,
            crate::output::working_to_output(crate::contract::render::OutputColour::Srgb),
            crate::contract::render::OutputColour::Srgb,
        );
        for v in out {
            assert!((v - 0.5).abs() < 0.02, "{out:?}");
        }
    }

    fn ramp(w: usize, h: usize) -> Vec<f32> {
        (0..w * h)
            .flat_map(|i| {
                let v = (i % w) as f32 / w as f32;
                [v, v, v]
            })
            .collect()
    }

    #[test]
    fn an_empty_field_changes_nothing_and_a_restore_undoes_a_push() {
        let (w, h) = (64, 48);
        let mut field = Field::new(w, h);
        assert!(!field.touched);
        let stroke = LiquifyStroke {
            mode: LiquifyMode::Push,
            radius: 0.2,
            strength: 1.0,
            points: vec![[0.4, 0.5], [0.5, 0.5]],
        };
        liquify(&mut field, &stroke);
        assert!(field.touched);
        let moved = field.dx.iter().map(|v| v.abs()).fold(0.0, f32::max);
        assert!(moved > 1.0);
        for _ in 0..30 {
            liquify(
                &mut field,
                &LiquifyStroke {
                    mode: LiquifyMode::Restore,
                    radius: 0.4,
                    strength: 1.0,
                    points: vec![[0.4, 0.5]],
                },
            );
        }
        let left = field.dx.iter().map(|v| v.abs()).fold(0.0, f32::max);
        assert!(left < moved * 0.05, "{left} of {moved}");
    }

    #[test]
    fn a_push_moves_content_along_the_stroke() {
        let (w, h) = (100, 60);
        let src = ramp(w, h);
        let mut field = Field::new(w, h);
        liquify(
            &mut field,
            &LiquifyStroke {
                mode: LiquifyMode::Push,
                radius: 0.3,
                strength: 1.0,
                points: vec![[0.5, 0.5], [0.55, 0.5]],
            },
        );
        let mut out = src.clone();
        field.resample(&src, &mut out);
        // Pushed right: the pixel at the centre now shows something from its left.
        let i = (30 * w + 50) * 3;
        assert!(out[i] < src[i] - 0.02, "{} vs {}", out[i], src[i]);
        // Far from the brush nothing moves.
        let j = (2 * w + 2) * 3;
        assert!((out[j] - src[j]).abs() < 1e-6);
    }

    #[test]
    fn the_field_never_folds_at_full_strength() {
        let (w, h) = (120, 120);
        let mut field = Field::new(w, h);
        for _ in 0..3 {
            field.add(moving([60.0, 60.0], [30.0, 30.0], 0.0, [500.0, 0.0]));
        }
        // The cap holds per primitive: the source x is monotone along a row through the centre.
        let row = 60 * w;
        let xs: Vec<f32> = (0..w).map(|x| x as f32 + field.dx[row + x]).collect();
        // Three stacked moves at the cap can at most flatten, the single-move guarantee is strict.
        let mut single = Field::new(w, h);
        single.add(moving([60.0, 60.0], [30.0, 30.0], 0.0, [500.0, 0.0]));
        let s: Vec<f32> = (0..w).map(|x| x as f32 + single.dx[row + x]).collect();
        assert!(s.windows(2).all(|p| p[1] > p[0]), "single move folded");
        assert!(xs.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn a_body_stretch_lengthens_below_the_hip_only() {
        let (w, h) = (40, 100);
        let mut field = Field::new(w, h);
        field.stretch_below(50.0, 8.0, 0.1, (-100.0, 200.0, 1.0));
        assert!(field.dy[10 * w + 20].abs() < 1e-6);
        assert!(field.dy[90 * w + 20] < -1.0);
    }

    #[test]
    fn a_recipe_without_a_finish_is_left_alone() {
        let recipe = aura_recipe::fixtures::reference();
        let mut rgb = ramp(16, 8);
        let before = rgb.clone();
        apply(&mut rgb, 16, 8, &recipe, None);
        assert_eq!(rgb, before);
        assert!(!wants(&recipe));
    }

    #[test]
    fn a_liquify_only_finish_renders_without_a_parse() {
        let mut recipe = aura_recipe::fixtures::reference();
        let finish = StudioFinish {
            liquify: vec![LiquifyStroke {
                mode: LiquifyMode::Bloat,
                radius: 0.3,
                strength: 1.0,
                points: vec![[0.5, 0.5]],
            }],
            ..StudioFinish::default()
        };
        studio_finish::write(&mut recipe, &finish).unwrap();
        assert!(wants(&recipe));
        let mut rgb = ramp(64, 64);
        let before = rgb.clone();
        apply(&mut rgb, 64, 64, &recipe, None);
        assert_ne!(rgb, before);
    }
}
