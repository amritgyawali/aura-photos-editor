//! The Studio's measured beauty, attire, background and subject adjustments, rendered.
//! ADR-0150.
//!
//! Called from [`crate::studio_finish`] with the portrait parse already made. Two passes:
//!
//! 1. [`before_shape`] - skin, makeup, hair and body skin, attire, and the people's own light
//!    and colour, on the photograph as it was parsed, so every landmark is where the parse
//!    found it. Runs before the background is replaced, so a subject's measurements are never
//!    taken against a background somebody chose.
//! 2. [`after_background`] - the background's own adjustments and the light wrap, which need
//!    the finished background; and [`last`], the vignette that avoids faces, after any reshape.
//!
//! Every adjustment is measured against the photograph it is applied to: a person's own cheeks,
//! the frame's own gold, the group's median face. None of them compares a person with a
//! constant, and none of them runs unless a photographer moved its slider.
// Every plane and buffer index is clamped into its frame before it is read.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::similar_names
)]

mod attire;
mod hair;
mod makeup;
mod scene;
mod skin;
mod subject;

use aura_portrait::{FaceGeometry, Plane, PortraitMap, Region};
use aura_recipe::studio_finish::StudioFinish;
use rayon::prelude::*;

use crate::colour::luma;
use crate::studio_finish::{Face, Scale};

/// The first pass: everything measured on the people. See the module documentation.
pub(crate) fn before_shape(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    finish: &StudioFinish,
    map: &PortraitMap,
) {
    let ctx = Ctx::new(w, h, map, &finish.faces);
    if !finish.skin.is_identity() {
        skin::apply(rgb, &ctx, &finish.skin);
    }
    if !finish.makeup.is_identity() {
        makeup::apply(rgb, &ctx, &finish.makeup);
    }
    if !finish.hair.is_identity() {
        hair::apply(rgb, &ctx, &finish.hair);
    }
    if !finish.attire.is_identity() {
        attire::apply(rgb, &ctx, &finish.attire);
    }
    if finish.body.double_chin > 0.0 {
        let measured = Ctx::new(w, h, map, &finish.faces).measure_chins(rgb);
        subject::double_chin_shade(rgb, &measured, finish.body.double_chin / 100.0);
    }
    if !finish.subject.is_identity() {
        subject::apply(rgb, &ctx, &finish.subject);
    }
}

/// The second pass: the background, once it is final.
pub(crate) fn after_background(
    rgb: &mut [f32],
    w: usize,
    h: usize,
    finish: &StudioFinish,
    map: &PortraitMap,
) {
    if finish.scene.is_identity() {
        return;
    }
    let ctx = Ctx::new(w, h, map, &finish.faces);
    scene::apply(rgb, &ctx, &finish.scene);
}

/// The last pass, after any reshape: a vignette that keeps off every face.
pub(crate) fn last(rgb: &mut [f32], w: usize, h: usize, finish: &StudioFinish, map: &PortraitMap) {
    if finish.subject.face_vignette <= 0.0 {
        return;
    }
    let ctx = Ctx::new(w, h, map, &finish.faces);
    subject::face_vignette(rgb, &ctx, finish.subject.face_vignette / 100.0);
}

// ---- the context ------------------------------------------------------------------------

/// The frame, the parse, and the faces a finish applies to, in buffer pixels.
pub(crate) struct Ctx<'a> {
    pub(crate) w: usize,
    pub(crate) h: usize,
    pub(crate) map: &'a PortraitMap,
    /// The chosen faces (every face when nobody chose), each in its own upright frame.
    pub(crate) faces: Vec<Face>,
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(w: usize, h: usize, map: &'a PortraitMap, chosen: &[[f32; 2]]) -> Self {
        let scale = Scale::of(map, w, h);
        let faces = chosen_faces(map, chosen)
            .iter()
            .map(|g| scale.face(g))
            .collect();
        Self { w, h, map, faces }
    }

    /// The same faces with each chin measured on the photograph (`Face::measure_chin`).
    pub(crate) fn measure_chins(mut self, rgb: &[f32]) -> Self {
        let (w, h) = (self.w, self.h);
        self.faces = self
            .faces
            .into_iter()
            .map(|f| f.measure_chin(rgb, w, h))
            .collect();
        self
    }

    /// A region read straight off the analysis grid.
    pub(crate) fn view(&self, region: Region) -> View<'a> {
        View::new(self.map.plane(region), self.w, self.h)
    }

    /// A region softened on the analysis grid by `radius` analysis pixels, for a feathered edge
    /// that costs nothing at full resolution.
    pub(crate) fn soft(&self, region: Region, radius: usize) -> OwnedView {
        let plane = self.map.plane(region).map(|p| {
            let values = crate::bands::blur(&p.values, p.width as usize, p.height as usize, radius);
            Plane::from_values(p.width, p.height, values)
        });
        OwnedView::new(plane, self.w, self.h)
    }

    /// The median face's interocular distance in buffer pixels, for sizing frame-wide radii.
    pub(crate) fn face_unit(&self) -> f32 {
        let mut units: Vec<f32> = self.faces.iter().map(|f| f.io).collect();
        units.sort_by(f32::total_cmp);
        units
            .get(units.len() / 2)
            .copied()
            .unwrap_or(self.w.min(self.h) as f32 * 0.06)
            .max(3.0)
    }

    /// The buffer rectangle `left..right` across and `up..down` along a face, in interocular
    /// units from the eye line, grown to cover the face's roll.
    pub(crate) fn around(&self, f: &Face, left: f32, right: f32, up: f32, down: f32) -> Area {
        let corners = [
            f.at(left, up),
            f.at(right, up),
            f.at(left, down),
            f.at(right, down),
        ];
        let x0 = corners.iter().map(|c| c[0]).fold(f32::INFINITY, f32::min);
        let x1 = corners
            .iter()
            .map(|c| c[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let y0 = corners.iter().map(|c| c[1]).fold(f32::INFINITY, f32::min);
        let y1 = corners
            .iter()
            .map(|c| c[1])
            .fold(f32::NEG_INFINITY, f32::max);
        Area::clipped(x0, y0, x1, y1, self.w, self.h)
    }

    /// Where a region is on the buffer, grown by `margin` of the short edge; `None` when the
    /// parse has none of it.
    pub(crate) fn bounds(&self, region: Region, margin: f32) -> Option<Area> {
        let plane = self.map.plane(region)?;
        let (pw, ph) = (plane.width as usize, plane.height as usize);
        let (mut x0, mut y0, mut x1, mut y1) = (pw, ph, 0, 0);
        for y in 0..ph {
            for x in 0..pw {
                if plane.values[y * pw + x] > 0.05 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let (sx, sy) = (self.w as f32 / pw as f32, self.h as f32 / ph as f32);
        let m = margin * self.w.min(self.h) as f32;
        let area = Area::clipped(
            x0 as f32 * sx - m,
            y0 as f32 * sy - m,
            x1 as f32 * sx + m,
            y1 as f32 * sy + m,
            self.w,
            self.h,
        );
        (!area.is_empty()).then_some(area)
    }

    /// The whole frame.
    pub(crate) fn whole(&self) -> Area {
        Area {
            x0: 0,
            y0: 0,
            x1: self.w,
            y1: self.h,
        }
    }
}

/// The faces a finish applies to: those nearest the points a photographer chose, or every face.
pub(crate) fn chosen_faces<'a>(map: &'a PortraitMap, chosen: &[[f32; 2]]) -> Vec<&'a FaceGeometry> {
    if chosen.is_empty() {
        return map.faces.iter().collect();
    }
    let (mw, mh) = (map.width.max(1) as f32, map.height.max(1) as f32);
    map.faces
        .iter()
        .filter(|g| {
            chosen.iter().any(|p| {
                let dx = p[0] * mw - g.center[0];
                let dy = p[1] * mh - g.center[1];
                dx.hypot(dy) <= g.size.max(4.0) * 0.75
            })
        })
        .collect()
}

/// A region on the analysis grid, read bilinearly at buffer pixels.
pub(crate) struct View<'a> {
    plane: Option<&'a Plane>,
    sx: f32,
    sy: f32,
}

impl<'a> View<'a> {
    fn new(plane: Option<&'a Plane>, w: usize, h: usize) -> Self {
        let (sx, sy) = plane.map_or((1.0, 1.0), |p| {
            (
                p.width as f32 / w.max(1) as f32,
                p.height as f32 / h.max(1) as f32,
            )
        });
        Self { plane, sx, sy }
    }

    /// The weight at buffer pixel `(x, y)`, `0..=1`.
    pub(crate) fn at(&self, x: usize, y: usize) -> f32 {
        self.plane
            .map_or(0.0, |p| bilinear(p, x, y, self.sx, self.sy))
    }

    /// True when the parse has this region at all.
    pub(crate) fn present(&self) -> bool {
        self.plane
            .is_some_and(|p| p.values.iter().any(|v| *v > 0.05))
    }
}

/// [`View`] over a plane this pass made.
pub(crate) struct OwnedView {
    plane: Option<Plane>,
    sx: f32,
    sy: f32,
}

impl OwnedView {
    fn new(plane: Option<Plane>, w: usize, h: usize) -> Self {
        let (sx, sy) = plane.as_ref().map_or((1.0, 1.0), |p| {
            (
                p.width as f32 / w.max(1) as f32,
                p.height as f32 / h.max(1) as f32,
            )
        });
        Self { plane, sx, sy }
    }

    pub(crate) fn at(&self, x: usize, y: usize) -> f32 {
        self.plane
            .as_ref()
            .map_or(0.0, |p| bilinear(p, x, y, self.sx, self.sy))
    }
}

fn bilinear(p: &Plane, x: usize, y: usize, sx: f32, sy: f32) -> f32 {
    let fx = (x as f32 + 0.5) * sx - 0.5;
    let fy = (y as f32 + 0.5) * sy - 0.5;
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let a = p.at(x0, y0);
    let b = p.at(x0 + 1, y0);
    let c = p.at(x0, y0 + 1);
    let d = p.at(x0 + 1, y0 + 1);
    let top = a + (b - a) * tx;
    let bottom = c + (d - c) * tx;
    (top + (bottom - top) * ty).clamp(0.0, 1.0)
}

// ---- areas ------------------------------------------------------------------------------

/// A rectangle of the buffer, `x0..x1` by `y0..y1`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Area {
    pub(crate) x0: usize,
    pub(crate) y0: usize,
    pub(crate) x1: usize,
    pub(crate) y1: usize,
}

impl Area {
    fn clipped(x0: f32, y0: f32, x1: f32, y1: f32, w: usize, h: usize) -> Self {
        let clamp = |v: f32, hi: usize| (v.max(0.0) as usize).min(hi);
        let (x0, x1) = (clamp(x0.floor(), w), clamp(x1.ceil(), w));
        let (y0, y1) = (clamp(y0.floor(), h), clamp(y1.ceil(), h));
        Self { x0, y0, x1, y1 }
    }

    pub(crate) fn width(self) -> usize {
        self.x1.saturating_sub(self.x0)
    }

    pub(crate) fn height(self) -> usize {
        self.y1.saturating_sub(self.y0)
    }

    pub(crate) fn is_empty(self) -> bool {
        self.width() < 2 || self.height() < 2
    }

    /// Index of absolute `(x, y)` in a plane the size of this area.
    pub(crate) fn index(self, x: usize, y: usize) -> usize {
        (y - self.y0) * self.width() + (x - self.x0)
    }

    /// Luminance over the area.
    pub(crate) fn luma(self, rgb: &[f32], w: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.width() * self.height());
        for y in self.y0..self.y1 {
            for x in self.x0..self.x1 {
                let i = (y * w + x) * 3;
                out.push(luma([rgb[i], rgb[i + 1], rgb[i + 2]]));
            }
        }
        out
    }

    /// One channel over the area.
    pub(crate) fn channel(self, rgb: &[f32], w: usize, c: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.width() * self.height());
        for y in self.y0..self.y1 {
            for x in self.x0..self.x1 {
                out.push(rgb[(y * w + x) * 3 + c]);
            }
        }
        out
    }

    /// A blur over the area's own plane; `radius` at least one pixel.
    pub(crate) fn blur(self, plane: &[f32], radius: f32) -> Vec<f32> {
        let r = radius.round().max(1.0) as usize;
        crate::bands::blur(plane, self.width(), self.height(), r)
    }
}

/// Rewrite every pixel of `area` that `f` returns a value for, rows in parallel. `f` gets the
/// absolute position and the pixel as it is now.
pub(crate) fn each(
    rgb: &mut [f32],
    w: usize,
    area: Area,
    f: impl Fn(usize, usize, [f32; 3]) -> Option<[f32; 3]> + Sync,
) {
    if area.is_empty() {
        return;
    }
    rgb.par_chunks_mut(w * 3)
        .enumerate()
        .skip(area.y0)
        .take(area.height())
        .for_each(|(y, row)| {
            for x in area.x0..area.x1 {
                let i = x * 3;
                let old = [row[i], row[i + 1], row[i + 2]];
                if let Some(new) = f(x, y, old) {
                    row[i] = new[0].max(0.0);
                    row[i + 1] = new[1].max(0.0);
                    row[i + 2] = new[2].max(0.0);
                }
            }
        });
}

/// A weight per pixel of an area.
pub(crate) fn weights(area: Area, f: impl Fn(usize, usize) -> f32 + Sync) -> Vec<f32> {
    let w = area.width();
    let mut out = vec![0.0_f32; w * area.height()];
    out.par_chunks_mut(w.max(1))
        .enumerate()
        .for_each(|(j, row)| {
            for (i, slot) in row.iter_mut().enumerate() {
                *slot = f(area.x0 + i, area.y0 + j).clamp(0.0, 1.0);
            }
        });
    out
}

/// A blur of `plane` over the pixels `weight` admits only, so a region's local level is never
/// pulled toward what surrounds it. Where nothing nearby is admitted, the pixel keeps its value.
pub(crate) fn masked_blur(area: Area, plane: &[f32], weight: &[f32], radius: f32) -> Vec<f32> {
    let product: Vec<f32> = plane.iter().zip(weight).map(|(v, k)| v * k).collect();
    let top = area.blur(&product, radius);
    let bottom = area.blur(weight, radius);
    top.iter()
        .zip(&bottom)
        .zip(plane)
        .map(|((t, b), v)| if *b > 1e-3 { t / b } else { *v })
        .collect()
}

/// A blur along one direction only: a triangle-weighted average of samples taken at whole-pixel
/// steps along `dir` (a unit vector), `radius` pixels each way, read bilinearly.
pub(crate) fn along(plane: &[f32], w: usize, h: usize, dir: [f32; 2], radius: f32) -> Vec<f32> {
    let n = radius.round().max(1.0) as i32;
    let read = |x: f32, y: f32| -> f32 {
        let x = x.clamp(0.0, (w - 1) as f32);
        let y = y.clamp(0.0, (h - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (x - x0 as f32, y - y0 as f32);
        let top = plane[y0 * w + x0] + (plane[y0 * w + x1] - plane[y0 * w + x0]) * tx;
        let bottom = plane[y1 * w + x0] + (plane[y1 * w + x1] - plane[y1 * w + x0]) * tx;
        top + (bottom - top) * ty
    };
    let mut out = vec![0.0_f32; w * h];
    if w == 0 || h == 0 {
        return out;
    }
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, slot) in row.iter_mut().enumerate() {
            let (mut sum, mut total) = (0.0_f32, 0.0_f32);
            for t in -n..=n {
                let k = (n + 1 - t.abs()) as f32;
                let s = t as f32;
                sum += read(x as f32 + dir[0] * s, y as f32 + dir[1] * s) * k;
                total += k;
            }
            *slot = sum / total;
        }
    });
    out
}

// ---- small arithmetic -------------------------------------------------------------------

/// A pixel's position in a face's own frame, in interocular units from the eye line.
pub(crate) fn local(f: &Face, x: usize, y: usize) -> [f32; 2] {
    let dx = x as f32 + 0.5 - f.mid[0];
    let dy = y as f32 + 0.5 - f.mid[1];
    [
        (dx * f.u[0] + dy * f.u[1]) / f.io,
        (dx * f.v[0] + dy * f.v[1]) / f.io,
    ]
}

/// How much a pixel can be this face's lips: an ellipse around the measured mouth. The parse's
/// lip region alone also takes in red fabric beside a face - a bride's veil - so every lip tool
/// multiplies by this.
pub(crate) fn near_mouth(f: &Face, x: usize, y: usize) -> f32 {
    // Flat over the mouth itself, falling away only beyond it.
    let p = local(f, x, y);
    let u = p[0] / (f.mouth_width * 0.7).max(1e-3);
    let v = (p[1] - f.mouth) / 0.3;
    1.0 - smoothstep(0.85, 1.25, u.hypot(v))
}

/// One at `(ca, cb)` falling smoothly to zero at the rim of radii `(ra, rb)`, in face units.
pub(crate) fn ellipse(p: [f32; 2], c: [f32; 2], r: [f32; 2]) -> f32 {
    let u = (p[0] - c[0]) / r[0].max(1e-4);
    let v = (p[1] - c[1]) / r[1].max(1e-4);
    let r2 = u * u + v * v;
    if r2 >= 1.0 {
        0.0
    } else {
        (1.0 - r2) * (1.0 - r2)
    }
}

/// 0 at `low` rising smoothly to 1 at `high`.
pub(crate) fn smoothstep(low: f32, high: f32, v: f32) -> f32 {
    let span = high - low;
    if span.abs() < 1e-9 {
        return if v >= high { 1.0 } else { 0.0 };
    }
    let t = ((v - low) / span).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Luminance and two colour differences; converting back keeps the luminance exactly.
pub(crate) fn opponent(p: [f32; 3]) -> [f32; 3] {
    let y = luma(p);
    [y, p[2] - y, p[0] - y]
}

pub(crate) fn from_opponent(o: [f32; 3]) -> [f32; 3] {
    let [y, cb, cr] = o;
    let r = y + cr;
    let b = y + cb;
    let g = (y - 0.2627 * r - 0.0593 * b) / 0.678;
    [r.max(0.0), g.max(0.0), b.max(0.0)]
}

/// A pixel scaled to a new luminance, keeping its chromaticity.
pub(crate) fn with_luma(p: [f32; 3], target: f32) -> [f32; 3] {
    let l = luma(p);
    if l <= 1e-6 {
        return [target.max(0.0); 3];
    }
    let k = target.max(0.0) / l;
    p.map(|v| v * k)
}

pub(crate) fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
}

/// A percentile of the values whose weight is over one half; `None` with too few.
pub(crate) fn percentile(values: impl Iterator<Item = (f32, f32)>, q: f32) -> Option<f32> {
    let mut kept: Vec<f32> = values
        .filter(|(v, k)| *k > 0.5 && v.is_finite())
        .map(|(v, _)| v)
        .collect();
    if kept.len() < 8 {
        return None;
    }
    kept.sort_by(f32::total_cmp);
    let i = ((kept.len() - 1) as f32 * q.clamp(0.0, 1.0)).round() as usize;
    kept.get(i).copied()
}

/// The weighted mean colour of an area, sampled on a grid of at most about 40 000 points.
pub(crate) fn mean_colour(
    rgb: &[f32],
    w: usize,
    area: Area,
    weight: impl Fn(usize, usize) -> f32,
) -> Option<[f32; 3]> {
    let step = ((area.width() * area.height() / 40_000) as f32)
        .sqrt()
        .ceil()
        .max(1.0) as usize;
    let mut sum = [0.0_f64; 3];
    let mut n = 0.0_f64;
    for y in (area.y0..area.y1).step_by(step) {
        for x in (area.x0..area.x1).step_by(step) {
            let k = weight(x, y);
            if k <= 0.05 {
                continue;
            }
            let i = (y * w + x) * 3;
            for c in 0..3 {
                sum[c] += f64::from(rgb[i + c] * k);
            }
            n += f64::from(k);
        }
    }
    (n > 4.0).then(|| sum.map(|s| (s / n) as f32))
}

#[cfg(test)]
pub(crate) mod testing;
