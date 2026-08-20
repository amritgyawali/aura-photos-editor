//! The plane every other module in this phase works on, and the seven operations later phases
//! and the brush both go through.
//!
//! Section 2.1: "Mask algebra: union, intersect, subtract, feather, expand/contract, invert,
//! and 'mask minus skin' style compositions used by later phases." All seven are here and
//! `mask minus skin` is `Subtract`, which is why there is no eighth.
//!
//! # Why one plane type rather than three
//!
//! A mask is a bitmap when it is stored as a run length, an eight-bit alpha when it is stored
//! as a plane, and a `f32` alpha when the renderer multiplies by it. That is three
//! representations of one thing, and three is exactly how many chances there are for a
//! rounding to disagree. [`Plane`] is the *working* representation - `f32` in `0.0 ..= 1.0` -
//! and it is the only one any arithmetic in this phase happens in. `store` converts at the
//! boundary in one place and `upload_gpu` converts at the other.
//!
//! # The operations are defined on the *stronger* side, deliberately
//!
//! Union is a maximum and intersection is a minimum, rather than the probabilistic
//! `a + b - ab` and `ab`. The probabilistic pair is what a Bayesian would write and it is
//! wrong for this job: intersecting a face mask at 0.9 with a skin mask at 0.9 would give
//! 0.81, so "her facial skin" would be *less* opaque than either of the two regions it is
//! made of - and a local exposure lift applied through it would visibly under-apply exactly
//! where the photographer was most specific. Min and max are idempotent, which is the
//! property a photographer's mental model actually has.

use rayon::prelude::*;

/// A working alpha plane.
///
/// `f32` in `0.0 ..= 1.0`, row major, `w * h` long. Nothing in this phase does arithmetic on
/// any other representation.
#[derive(Debug, Clone, PartialEq)]
pub struct Plane {
    /// Width in pixels.
    pub w: u32,
    /// Height in pixels.
    pub h: u32,
    /// `w * h` alpha values.
    pub a: Vec<f32>,
}

impl Plane {
    /// An empty plane of a given size.
    #[must_use]
    pub fn zeros(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            a: vec![0.0; (w as usize).saturating_mul(h as usize)],
        }
    }

    /// A full plane of a given size.
    #[must_use]
    pub fn ones(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            a: vec![1.0; (w as usize).saturating_mul(h as usize)],
        }
    }

    /// Wrap an existing buffer, truncating or zero-filling it to `w * h`.
    #[must_use]
    pub fn from_vec(w: u32, h: u32, mut a: Vec<f32>) -> Self {
        let want = (w as usize).saturating_mul(h as usize);
        a.resize(want, 0.0);
        for value in &mut a {
            *value = value.clamp(0.0, 1.0);
        }
        Self { w, h, a }
    }

    /// Wrap a buffer without clamping it.
    ///
    /// The guided filter in [`crate::mask::matting`] holds three planes whose values are not
    /// alphas - the guide, which is a scene-referred luminance that can exceed one, and the
    /// two affine coefficients, whose slope is negative whenever a dark subject sits against a
    /// bright background. Clamping those would turn a legitimate negative slope into a flat
    /// matte, which is a hair boundary that quietly reverts to the coarse mask.
    #[must_use]
    pub fn from_vec_unclamped(w: u32, h: u32, mut a: Vec<f32>) -> Self {
        a.resize((w as usize).saturating_mul(h as usize), 0.0);
        Self { w, h, a }
    }

    /// Write an alpha without clamping. Only the guided filter needs this; see
    /// [`Plane::from_vec_unclamped`].
    pub fn set_raw(&mut self, x: i64, y: i64, value: f32) {
        if x < 0 || y < 0 || x >= i64::from(self.w) || y >= i64::from(self.h) {
            return;
        }
        let index = (y as usize) * (self.w as usize) + (x as usize);
        if let Some(slot) = self.a.get_mut(index) {
            *slot = value;
        }
    }

    /// True when the plane has no pixels at all.
    #[must_use]
    pub const fn is_degenerate(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// The alpha at a pixel, with the edge extended outwards.
    ///
    /// Clamp-to-edge rather than zero-outside, and it is not a detail: a bilinear upsample that
    /// read zero past the border would darken the outermost half-pixel of every mask it
    /// resized, which is a one-pixel dark rim around every region at every render level. That
    /// is a halo - the exact artefact section 10.1 audits for at 100 % zoom - manufactured by
    /// the resampler rather than by the segmenter.
    #[must_use]
    pub fn at_clamped(&self, x: i64, y: i64) -> f32 {
        if self.is_degenerate() {
            return 0.0;
        }
        let cx = x.clamp(0, i64::from(self.w) - 1);
        let cy = y.clamp(0, i64::from(self.h) - 1);
        self.at(cx, cy)
    }

    /// The alpha at a pixel, or `0.0` outside.
    #[must_use]
    pub fn at(&self, x: i64, y: i64) -> f32 {
        if x < 0 || y < 0 || x >= i64::from(self.w) || y >= i64::from(self.h) {
            return 0.0;
        }
        let index = (y as usize) * (self.w as usize) + (x as usize);
        self.a.get(index).copied().unwrap_or(0.0)
    }

    /// Write an alpha, ignoring writes outside the plane.
    pub fn set(&mut self, x: i64, y: i64, value: f32) {
        if x < 0 || y < 0 || x >= i64::from(self.w) || y >= i64::from(self.h) {
            return;
        }
        let index = (y as usize) * (self.w as usize) + (x as usize);
        if let Some(slot) = self.a.get_mut(index) {
            *slot = value.clamp(0.0, 1.0);
        }
    }

    /// The sum of every alpha. The area of the region, in fractional pixels.
    #[must_use]
    pub fn area(&self) -> f64 {
        self.a.iter().map(|v| f64::from(*v)).sum()
    }

    /// The fraction of the plane the region covers.
    #[must_use]
    pub fn coverage(&self) -> f32 {
        let pixels = f64::from(self.w) * f64::from(self.h);
        if pixels <= 0.0 {
            return 0.0;
        }
        (self.area() / pixels) as f32
    }

    /// True when no pixel is above `1e-4`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.a.iter().all(|v| *v <= 1e-4)
    }

    /// Nearest-neighbour resize. Deterministic and separable, and the only resize a *mask*
    /// wants: a bilinear resize of a binary region invents alpha values along the boundary
    /// that no matting produced, which is a soft edge nobody measured.
    #[must_use]
    pub fn resize_nearest(&self, w: u32, h: u32) -> Self {
        if w == self.w && h == self.h {
            return self.clone();
        }
        if self.is_degenerate() || w == 0 || h == 0 {
            return Self::zeros(w, h);
        }
        let mut out = Self::zeros(w, h);
        let sx = f64::from(self.w) / f64::from(w);
        let sy = f64::from(self.h) / f64::from(h);
        for y in 0..h {
            for x in 0..w {
                let src_x = ((f64::from(x) + 0.5) * sx).floor() as i64;
                let src_y = ((f64::from(y) + 0.5) * sy).floor() as i64;
                out.set(i64::from(x), i64::from(y), self.at_clamped(src_x, src_y));
            }
        }
        out
    }

    /// Bilinear resize. What [`crate::mask::upload_plane`] uses on the way *out*, where the
    /// alpha values have already been decided and interpolating between them is exactly
    /// right.
    #[must_use]
    pub fn resize_bilinear(&self, w: u32, h: u32) -> Self {
        if w == self.w && h == self.h {
            return self.clone();
        }
        if self.is_degenerate() || w == 0 || h == 0 {
            return Self::zeros(w, h);
        }
        let mut out = Self::zeros(w, h);
        let sx = f64::from(self.w) / f64::from(w);
        let sy = f64::from(self.h) / f64::from(h);
        for y in 0..h {
            let fy = ((f64::from(y) + 0.5) * sy - 0.5).max(0.0);
            let y0 = fy.floor() as i64;
            let ty = (fy - fy.floor()) as f32;
            for x in 0..w {
                let fx = ((f64::from(x) + 0.5) * sx - 0.5).max(0.0);
                let x0 = fx.floor() as i64;
                let tx = (fx - fx.floor()) as f32;
                let top = self.at_clamped(x0, y0) * (1.0 - tx) + self.at_clamped(x0 + 1, y0) * tx;
                let bottom =
                    self.at_clamped(x0, y0 + 1) * (1.0 - tx) + self.at_clamped(x0 + 1, y0 + 1) * tx;
                out.set(i64::from(x), i64::from(y), top * (1.0 - ty) + bottom * ty);
            }
        }
        out
    }
}

/// Resample `other` onto `base`'s grid when the two differ.
///
/// Every binary operation calls this first. Two planes at different resolutions is the normal
/// case rather than the exception - a brush stroke arrives at the panel's overlay resolution
/// and a stored subject mask is at a quarter of the analysis grid - and an operation that
/// refused to combine them would push the resample to every call site.
#[must_use]
fn aligned(base: &Plane, other: &Plane) -> Plane {
    if base.w == other.w && base.h == other.h {
        other.clone()
    } else {
        other.resize_bilinear(base.w, base.h)
    }
}

/// The stronger of the two, everywhere. See the module note on why this is a maximum.
#[must_use]
pub fn union(a: &Plane, b: &Plane) -> Plane {
    let b = aligned(a, b);
    let mut out = a.clone();
    out.a
        .par_iter_mut()
        .zip(b.a.par_iter())
        .for_each(|(slot, other)| *slot = slot.max(*other));
    out
}

/// The weaker of the two, everywhere.
#[must_use]
pub fn intersect(a: &Plane, b: &Plane) -> Plane {
    let b = aligned(a, b);
    let mut out = a.clone();
    out.a
        .par_iter_mut()
        .zip(b.a.par_iter())
        .for_each(|(slot, other)| *slot = slot.min(*other));
    out
}

/// `a` with `b` taken out of it. "Mask minus skin", in section 2.1's own words.
#[must_use]
pub fn subtract(a: &Plane, b: &Plane) -> Plane {
    let b = aligned(a, b);
    let mut out = a.clone();
    out.a
        .par_iter_mut()
        .zip(b.a.par_iter())
        .for_each(|(slot, other)| *slot = (*slot - *other).clamp(0.0, 1.0));
    out
}

/// Everything the plane is not.
#[must_use]
pub fn invert(a: &Plane) -> Plane {
    let mut out = a.clone();
    out.a.par_iter_mut().for_each(|slot| *slot = 1.0 - *slot);
    out
}

/// Grow the region by a radius, in plane pixels.
///
/// A separable maximum filter, which is a dilation by a square rather than by a disc. The
/// difference at the corners is a fraction of a pixel at the radii this phase uses, and a
/// disc costs a quadratic pass per pixel where a square costs two linear ones - which is the
/// difference between 120 ms and not.
#[must_use]
pub fn grow(a: &Plane, radius: u32) -> Plane {
    if radius == 0 || a.is_degenerate() {
        return a.clone();
    }
    morph(a, radius, true)
}

/// Shrink the region by a radius, in plane pixels.
#[must_use]
pub fn shrink(a: &Plane, radius: u32) -> Plane {
    if radius == 0 || a.is_degenerate() {
        return a.clone();
    }
    morph(a, radius, false)
}

/// The separable pass both morphological operations share.
fn morph(a: &Plane, radius: u32, dilate: bool) -> Plane {
    let r = i64::from(radius);
    let mut mid = Plane::zeros(a.w, a.h);
    for y in 0..i64::from(a.h) {
        for x in 0..i64::from(a.w) {
            let mut best = if dilate { 0.0_f32 } else { 1.0_f32 };
            for dx in -r..=r {
                // Outside the plane reads as 0 for a dilation and as 1 for an erosion, so a
                // region that touches the frame edge is not eroded away from the outside by
                // pixels that are not in the photograph. `ClippedByFrame` is the reason code
                // that says the region reached the edge at all.
                let sample = if x + dx < 0 || x + dx >= i64::from(a.w) {
                    if dilate {
                        0.0
                    } else {
                        1.0
                    }
                } else {
                    a.at(x + dx, y)
                };
                best = if dilate {
                    best.max(sample)
                } else {
                    best.min(sample)
                };
            }
            mid.set(x, y, best);
        }
    }
    let mut out = Plane::zeros(a.w, a.h);
    for y in 0..i64::from(a.h) {
        for x in 0..i64::from(a.w) {
            let mut best = if dilate { 0.0_f32 } else { 1.0_f32 };
            for dy in -r..=r {
                let sample = if y + dy < 0 || y + dy >= i64::from(a.h) {
                    if dilate {
                        0.0
                    } else {
                        1.0
                    }
                } else {
                    mid.at(x, y + dy)
                };
                best = if dilate {
                    best.max(sample)
                } else {
                    best.min(sample)
                };
            }
            out.set(x, y, best);
        }
    }
    out
}

/// Soften the boundary.
///
/// `amount` is `0.0 ..= 1.0` and maps onto a blur radius that is a fraction of the plane's
/// short edge, so the same slider position produces the same *visual* softness on a 192 px
/// stored plane and on a 4,000 px render. A radius in pixels would make the feather slider
/// mean something different at every resolution, which is the defect this scaling exists to
/// prevent.
#[must_use]
pub fn feather(a: &Plane, amount: f32) -> Plane {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 1e-4 || a.is_degenerate() {
        return a.clone();
    }
    let short = a.w.min(a.h) as f32;
    let radius = ((amount * FEATHER_MAX_FRACTION * short).round() as u32).max(1);
    box_blur(a, radius)
}

/// The largest feather, as a fraction of the plane's short edge.
///
/// Eight per cent. On a 2048 px frame that is a 160 px transition, which is past the point at
/// which a local adjustment reads as a region at all and into the point at which it reads as
/// a gradient - which is what `MaskKind`'s two gradient siblings in the recipe are for.
pub const FEATHER_MAX_FRACTION: f32 = 0.08;

/// A two-pass box blur. Separable, deterministic, and the same on every machine.
#[must_use]
pub fn box_blur(a: &Plane, radius: u32) -> Plane {
    if radius == 0 || a.is_degenerate() {
        return a.clone();
    }
    let r = i64::from(radius);
    let width = i64::from(a.w);
    let height = i64::from(a.h);
    let denom = (2 * r + 1) as f32;

    let mut mid = Plane::zeros(a.w, a.h);
    for y in 0..height {
        let mut acc = 0.0_f32;
        for dx in -r..=r {
            acc += a.at(dx.clamp(0, width - 1), y);
        }
        for x in 0..width {
            // `set_raw` rather than `set`: this blur is also the guided filter's mean pass,
            // and clamping a negative affine slope there is a hair matte that silently
            // reverts to its coarse mask. For an ordinary alpha plane the two are identical,
            // because a mean of values in `0 ..= 1` is in `0 ..= 1`.
            mid.set_raw(x, y, acc / denom);
            let leaving = a.at((x - r).clamp(0, width - 1), y);
            let entering = a.at((x + r + 1).clamp(0, width - 1), y);
            acc += entering - leaving;
        }
    }

    let mut out = Plane::zeros(a.w, a.h);
    for x in 0..width {
        let mut acc = 0.0_f32;
        for dy in -r..=r {
            acc += mid.at(x, dy.clamp(0, height - 1));
        }
        for y in 0..height {
            out.set_raw(x, y, acc / denom);
            let leaving = mid.at(x, (y - r).clamp(0, height - 1));
            let entering = mid.at(x, (y + r + 1).clamp(0, height - 1));
            acc += entering - leaving;
        }
    }
    out
}

/// Threshold a plane into a hard region.
#[must_use]
pub fn threshold(a: &Plane, level: f32) -> Plane {
    let mut out = a.clone();
    out.a.par_iter_mut().for_each(|slot| {
        *slot = if *slot >= level { 1.0 } else { 0.0 };
    });
    out
}

/// Intersection over union between two planes, at `base`'s resolution.
///
/// The metric section 10.1's mIoU gates are measured with, defined on soft planes so a matted
/// boundary is scored as the partial coverage it is rather than being thresholded first.
#[must_use]
pub fn iou(a: &Plane, b: &Plane) -> f32 {
    if a.is_degenerate() {
        return 0.0;
    }
    let b = aligned(a, &b.clone());
    let mut inter = 0.0_f64;
    let mut union_area = 0.0_f64;
    for (x, y) in a.a.iter().zip(b.a.iter()) {
        inter += f64::from(x.min(*y));
        union_area += f64::from(x.max(*y));
    }
    if union_area <= 1e-9 {
        // Two empty planes agree completely. Returning zero would make "there is no sky in
        // this photograph and the mask says so" score as a total failure, which is how a
        // mIoU gate ends up measuring how many skies the fixtures happen to contain.
        return 1.0;
    }
    (inter / union_area) as f32
}

#[cfg(test)]
mod tests {
    // The panic family is how a test asserts, and a mask test compares alphas that are exactly
    // zero or exactly one by construction - a painted fixture has no rounding to be tolerant of.
    #![allow(
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::assertions_on_constants,
        clippy::uninlined_format_args
    )]
    use super::*;

    fn disc(w: u32, h: u32, cx: f32, cy: f32, r: f32) -> Plane {
        let mut p = Plane::zeros(w, h);
        for y in 0..h {
            for x in 0..w {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                if dx.hypot(dy) <= r {
                    p.set(i64::from(x), i64::from(y), 1.0);
                }
            }
        }
        p
    }

    #[test]
    fn union_and_intersection_are_idempotent() {
        let a = disc(32, 32, 16.0, 16.0, 8.0);
        assert_eq!(union(&a, &a), a);
        assert_eq!(intersect(&a, &a), a);
    }

    #[test]
    fn intersecting_two_confident_masks_does_not_weaken_the_result() {
        // The module note's own example: a probabilistic intersection would give 0.81.
        let mut a = Plane::zeros(4, 4);
        let mut b = Plane::zeros(4, 4);
        for i in 0..16 {
            a.a[i] = 0.9;
            b.a[i] = 0.9;
        }
        let out = intersect(&a, &b);
        assert!(out.a.iter().all(|v| (*v - 0.9).abs() < 1e-6));
    }

    #[test]
    fn subtract_is_mask_minus_skin() {
        let a = disc(32, 32, 16.0, 16.0, 10.0);
        let b = disc(32, 32, 16.0, 16.0, 4.0);
        let out = subtract(&a, &b);
        assert_eq!(out.at(16, 16), 0.0);
        assert_eq!(out.at(16, 24), 1.0);
    }

    #[test]
    fn grow_then_shrink_recovers_most_of_a_convex_region() {
        let a = disc(64, 64, 32.0, 32.0, 12.0);
        let out = shrink(&grow(&a, 3), 3);
        assert!(iou(&a, &out) > 0.95, "iou {}", iou(&a, &out));
    }

    #[test]
    fn an_erosion_does_not_eat_a_region_that_touches_the_frame_edge() {
        let mut a = Plane::zeros(16, 16);
        for y in 0..16 {
            for x in 0..8 {
                a.set(i64::from(x), i64::from(y), 1.0);
            }
        }
        let out = shrink(&a, 2);
        assert_eq!(out.at(0, 8), 1.0, "the left column was eroded from outside");
        assert_eq!(out.at(7, 8), 0.0);
    }

    #[test]
    fn feather_scales_with_the_plane_rather_than_with_pixels() {
        let small = feather(&disc(64, 64, 32.0, 32.0, 20.0), 1.0);
        let large = feather(&disc(256, 256, 128.0, 128.0, 80.0), 1.0);
        // The transition width as a fraction of the plane is the same, so the coverage of the
        // feathered region is too.
        assert!((small.coverage() - large.coverage()).abs() < 0.02);
    }

    #[test]
    fn two_empty_planes_agree() {
        assert_eq!(iou(&Plane::zeros(8, 8), &Plane::zeros(8, 8)), 1.0);
    }

    #[test]
    fn invert_is_its_own_inverse() {
        let a = disc(32, 32, 16.0, 16.0, 7.0);
        assert_eq!(invert(&invert(&a)), a);
    }

    #[test]
    fn a_binary_operation_resamples_a_mismatched_operand() {
        let a = Plane::ones(32, 32);
        let b = Plane::ones(8, 8);
        assert_eq!(intersect(&a, &b).coverage(), 1.0);
    }

    #[test]
    fn an_upsample_does_not_manufacture_a_dark_rim() {
        // The halo the clamped sampler exists to prevent. A full plane resized up must stay
        // full: any value below one on the border is a one-pixel dark edge around every mask.
        let up = Plane::ones(16, 16).resize_bilinear(64, 64);
        assert!(
            up.a.iter().all(|v| (*v - 1.0).abs() < 1e-6),
            "the border was darkened by the resampler"
        );
    }
}
