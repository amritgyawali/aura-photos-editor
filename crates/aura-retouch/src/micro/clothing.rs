//! What is on the garment that was never part of it.
//!
//! PHASE-21 section 6.3:
//!
//! > Lint/thread/stain detection as small anomaly detection restricted to the clothing mask, with
//! > inpainting reused from Phase 20; creases and wrinkles are opt-in only, since removing them
//! > can look artificial.
//!
//! ## Restricted to the garment, and to *small*
//!
//! Two restrictions and both are structural. The detector only looks where phase 18's clothing or
//! dress region is above half, so a lint detector cannot find a mark on somebody's face. And a
//! candidate above [`aura_core::contract::micro::MAX_CLOTHING_AREA`] is reported and never acted
//! on: a stain a tenth of a percent of the frame across is an object, phase 24 owns objects, and
//! the boundary between the two phases is a number rather than a judgement.
//!
//! ## Three kinds, measured apart
//!
//! | Kind | What it looks like |
//! |---|---|
//! | [`ClothingIssue::Lint`] | small, compact, **lighter** than the fabric, low aspect ratio |
//! | [`ClothingIssue::Thread`] | thin and long: a high aspect ratio at a small area |
//! | [`ClothingIssue::Stain`] | compact and **darker** than the fabric, or off its hue |
//!
//! The two the contract marks opt-in - [`ClothingIssue::Strap`] and [`ClothingIssue::Crease`] -
//! are deliberately **not detected here**. A crease is a fold of the garment and a strap is a
//! garment; neither is an anomaly against the fabric, so neither can be found by an anomaly
//! detector, and a module that pretended otherwise would be finding shadows. They exist in the
//! vocabulary so a studio can express a policy about them and so the schema can refuse one that
//! arrives without that policy; producing them needs a segmentation head this phase does not
//! have, and `MicroCode::OptedOut` is what a frame gets in the meantime.
//!
//! ## The fabric texture test
//!
//! Section 10.1 asks for "no fabric-texture damage at 100 % zoom". Lace, tweed and sequins are
//! *made of* small high-contrast anomalies, and an anomaly detector on one finds hundreds. So
//! [`fabric_texture`] measures the local structure of the garment around each candidate, and a
//! value above [`MAX_FABRIC_TEXTURE`] refuses it: on a textured fabric there is no such thing as
//! a piece of lint that is distinguishable from the weave.
//!
//! ## Everything here is linear
//!
//! Invariant 8.

use aura_core::contract::composition::Box2;
use aura_core::contract::micro::{ClothingIssue, MAX_CLOTHING_AREA};

use crate::texture_guard::Frame;

/// The neighbourhood a candidate is measured against, in pixels at proxy scale.
pub const NEIGHBOURHOOD: usize = 9;

/// The smallest departure from the local fabric that counts as a mark.
pub const MIN_DEPARTURE: f32 = 0.045;

/// Above this local structure the fabric is patterned and nothing may be cleaned off it.
///
/// The lace, tweed and sequin refusal. Measured on the ring around the candidate rather than on
/// the candidate, for the reason `hair::background_detail` gives: measuring inside would score
/// the mark.
pub const MAX_FABRIC_TEXTURE: f32 = 0.14;

/// Aspect ratio at or above which a small mark is a thread rather than a piece of lint.
pub const THREAD_ASPECT: f32 = 3.0;

/// The most candidates one frame may produce.
///
/// A frame that finds more than this on a garment has found the weave, and the honest response is
/// to stop rather than to clean forty of them. The cap is here as well as in the plan because a
/// runaway component search is also a performance problem.
pub const MAX_CANDIDATES: usize = 48;

/// One thing on a garment.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// Where it is, normalised to the frame.
    pub region: Box2,
    /// What it looks like.
    pub kind: ClothingIssue,
    /// How far it departs from the fabric around it, `0..1`.
    pub departure: f32,
    /// How structured the fabric around it is, `0..1`.
    pub fabric_texture: f32,
    /// True when it is too large to be a small distraction.
    pub too_large: bool,
    /// True when the fabric around it is too patterned to inpaint into.
    pub fabric_busy: bool,
}

impl Mark {
    /// True when this mark may be acted on at all.
    #[must_use]
    pub fn is_actionable(&self) -> bool {
        !self.too_large && !self.fabric_busy && self.departure >= MIN_DEPARTURE
    }
}

/// Find the small distractions on one frame's garments.
///
/// `garment` is the per-pixel clothing-or-dress coverage from phase 18. Ordered by departure,
/// strongest first, then by position - deterministic, invariant 4.
#[must_use]
pub fn detect(frame: &Frame, garment: &[f32]) -> Vec<Mark> {
    let (width, height) = (frame.width, frame.height);
    if width == 0 || height == 0 || garment.len() < width * height {
        return Vec::new();
    }

    let luminance = luma_plane(frame);
    let local = robust_local(&luminance, width, height);
    // The garment, eroded by the blur's own radius. Without this the local background estimate
    // is computed across the garment's boundary, so every edge of every lapel carries a large
    // residual and the detector finds a "stain" the shape of the collar. It is the same class of
    // defect phase 18 found in its resampler - arithmetic that reads outside the region it is
    // describing - and it is fixed the same way, by not asking about samples whose neighbourhood
    // is not all garment.
    let searchable = erode(garment, width, height, NEIGHBOURHOOD / 2);
    let area_limit = ((width * height) as f32 * MAX_CLOTHING_AREA).ceil() as usize + 4;

    let mut seen = vec![false; width * height];
    let mut out: Vec<Mark> = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if searchable.get(index).copied().unwrap_or(0.0) < 0.5 {
                continue;
            }
            if seen.get(index).copied().unwrap_or(true) {
                continue;
            }
            let value = luminance.get(index).copied().unwrap_or(0.0);
            let base = local.get(index).copied().unwrap_or(value);
            if (value - base).abs() < MIN_DEPARTURE {
                if let Some(slot) = seen.get_mut(index) {
                    *slot = true;
                }
                continue;
            }
            let sign = if value > base { 1.0f32 } else { -1.0f32 };
            let Some(component) = grow(
                index,
                sign,
                &luminance,
                &local,
                &searchable,
                &mut seen,
                width,
                height,
                area_limit,
            ) else {
                continue;
            };

            let w = component.x1 - component.x0 + 1;
            let h = component.y1 - component.y0 + 1;
            let region = Box2 {
                x: component.x0 as f32 / width as f32,
                y: component.y0 as f32 / height as f32,
                w: w as f32 / width as f32,
                h: h as f32 / height as f32,
            };
            let aspect = (w.max(h) as f32) / (w.min(h).max(1) as f32);
            let kind = if aspect >= THREAD_ASPECT {
                ClothingIssue::Thread
            } else if sign > 0.0 {
                ClothingIssue::Lint
            } else {
                ClothingIssue::Stain
            };
            let texture =
                fabric_texture(&luminance, &local, &searchable, &component, width, height);

            out.push(Mark {
                region,
                kind,
                departure: component.departure.clamp(0.0, 1.0),
                fabric_texture: texture,
                too_large: component.overflowed || region.w * region.h > MAX_CLOTHING_AREA + 1e-9,
                fabric_busy: texture > MAX_FABRIC_TEXTURE,
            });
            if out.len() >= MAX_CANDIDATES {
                break;
            }
        }
        if out.len() >= MAX_CANDIDATES {
            break;
        }
    }

    out.sort_by(|a, b| {
        b.departure
            .total_cmp(&a.departure)
            .then(a.region.y.total_cmp(&b.region.y))
            .then(a.region.x.total_cmp(&b.region.x))
    });
    out
}

/// One connected run of samples departing from the local fabric in the same direction.
#[derive(Debug, Clone, Copy)]
pub(super) struct Component {
    pub(super) x0: usize,
    pub(super) y0: usize,
    pub(super) x1: usize,
    pub(super) y1: usize,
    pub(super) departure: f32,
    pub(super) overflowed: bool,
}

#[allow(clippy::too_many_arguments)]
fn grow(
    seed: usize,
    sign: f32,
    luminance: &[f32],
    local: &[f32],
    garment: &[f32],
    seen: &mut [bool],
    width: usize,
    height: usize,
    limit: usize,
) -> Option<Component> {
    let mut stack = vec![seed];
    if let Some(slot) = seen.get_mut(seed) {
        *slot = true;
    }
    let (mut x0, mut y0) = (seed % width, seed / width);
    let (mut x1, mut y1) = (x0, y0);
    let mut count = 0usize;
    let mut total = 0.0f64;
    let mut weight = 0.0f64;
    let mut overflowed = false;

    while let Some(index) = stack.pop() {
        let value = luminance.get(index).copied().unwrap_or(0.0);
        let base = local.get(index).copied().unwrap_or(value);
        let departure = (value - base) * sign;
        if departure < MIN_DEPARTURE {
            continue;
        }
        count += 1;
        // Weighted by how far each sample departs from the fabric it sits on. Phase 20's
        // correction, inherited: a plain mean over a component reads a strong mark as a weak one,
        // because a component includes its own falloff and a falloff sample is half fabric.
        total += f64::from(departure) * f64::from(departure);
        weight += f64::from(departure);
        let (x, y) = (index % width, index / width);
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
        if count > limit {
            overflowed = true;
            break;
        }

        // The eight neighbours, in unsigned coordinates. The frame edge is handled by clamping
        // the window rather than by casting to a signed type and testing for negatives: a mark on
        // the first row is an ordinary mark, and a cast is a second way to get its bounds wrong.
        let left = x.saturating_sub(1);
        let right = (x + 1).min(width.saturating_sub(1));
        let top = y.saturating_sub(1);
        let bottom = (y + 1).min(height.saturating_sub(1));
        for ny in top..=bottom {
            for nx in left..=right {
                if nx == x && ny == y {
                    continue;
                }
                let neighbour = ny * width + nx;
                if garment.get(neighbour).copied().unwrap_or(0.0) < 0.5 {
                    continue;
                }
                if seen.get(neighbour).copied().unwrap_or(true) {
                    continue;
                }
                if let Some(slot) = seen.get_mut(neighbour) {
                    *slot = true;
                }
                stack.push(neighbour);
            }
        }
    }

    if count == 0 || weight <= f64::EPSILON {
        return None;
    }
    Some(Component {
        x0,
        y0,
        x1,
        y1,
        departure: (total / weight) as f32,
        overflowed,
    })
}

/// How patterned the fabric around a candidate is, `0..1`.
///
/// The lace-and-sequins refusal. Measured on the garment samples in the ring around the
/// component, excluding the component itself.
pub(super) fn fabric_texture(
    luminance: &[f32],
    local: &[f32],
    garment: &[f32],
    component: &Component,
    width: usize,
    height: usize,
) -> f32 {
    // The excluded window is the component **dilated by the blur radius**, not the component
    // itself. A mark perturbs the local estimate out to that radius, so a ring measured any
    // closer scores the mark's own falloff and calls plain fabric textured.
    let skirt = NEIGHBOURHOOD / 2 + 1;
    let reach = NEIGHBOURHOOD * 2;
    let inner_x0 = component.x0.saturating_sub(skirt);
    let inner_y0 = component.y0.saturating_sub(skirt);
    let inner_x1 = component.x1 + skirt;
    let inner_y1 = component.y1 + skirt;
    let x0 = component.x0.saturating_sub(reach);
    let y0 = component.y0.saturating_sub(reach);
    let x1 = (component.x1 + reach).min(width.saturating_sub(1));
    let y1 = (component.y1 + reach).min(height.saturating_sub(1));

    let mut total = 0.0f64;
    let mut count = 0u32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if x >= inner_x0 && x <= inner_x1 && y >= inner_y0 && y <= inner_y1 {
                continue;
            }
            let index = y * width + x;
            if garment.get(index).copied().unwrap_or(0.0) < 0.5 {
                continue;
            }
            let value = luminance.get(index).copied().unwrap_or(0.0);
            let base = local.get(index).copied().unwrap_or(value);
            total += f64::from((value - base).abs());
            count += 1;
        }
    }
    if count == 0 {
        // Nothing measurable around it: refused rather than permitted, as the hair module does.
        return 1.0;
    }
    ((total / f64::from(count)) as f32 * 6.0).clamp(0.0, 1.0)
}

/// The local fabric, estimated without letting a mark into its own estimate.
///
/// Two box blurs. The first is an ordinary local mean; the second is taken over a plane in which
/// every sample departing from that mean by more than [`MIN_DEPARTURE`] has been replaced by it.
///
/// One blur is not enough, and the failure is specific: a bright piece of lint raises the mean
/// across its whole blur window, so the fabric immediately around it sits *below* its own local
/// mean and is detected as a dark stain the shape of a ring. The ring is then large, so it is
/// refused as too large - and the frame reports a distraction it did not have, having failed to
/// clean the one it did. A median filter is the textbook answer and is far too slow over a
/// garment at proxy resolution; this is two passes of arithmetic that is already separable.
///
/// The same idea as `hair::background_estimate` and the masked blur in `eyes::measure`: a local
/// estimate must be computed from the region it describes.
fn robust_local(luminance: &[f32], width: usize, height: usize) -> Vec<f32> {
    let first = super::hair::box_blur(luminance, width, height, NEIGHBOURHOOD);
    let mut cleaned = luminance.to_vec();
    for index in 0..width * height {
        let value = luminance.get(index).copied().unwrap_or(0.0);
        let base = first.get(index).copied().unwrap_or(value);
        if (value - base).abs() > MIN_DEPARTURE {
            if let Some(slot) = cleaned.get_mut(index) {
                *slot = base;
            }
        }
    }
    super::hair::box_blur(&cleaned, width, height, NEIGHBOURHOOD)
}

/// A coverage plane shrunk by `radius`, so every surviving sample's neighbourhood is inside it.
///
/// A minimum filter over a square window, separable in the same way a box blur is.
fn erode(plane: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    if radius == 0 {
        return plane.to_vec();
    }
    let mut horizontal = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut lowest = f32::INFINITY;
            for offset in 0..=radius * 2 {
                // `x + offset - radius`, kept unsigned. Off the left edge and off the right edge
                // are the same answer - zero, which is what makes this an erosion - and both are
                // reached without a signed intermediate.
                let value = if x + offset < radius {
                    0.0
                } else {
                    let sx = x + offset - radius;
                    if sx >= width {
                        0.0
                    } else {
                        plane.get(y * width + sx).copied().unwrap_or(0.0)
                    }
                };
                lowest = lowest.min(value);
            }
            if let Some(slot) = horizontal.get_mut(y * width + x) {
                *slot = lowest;
            }
        }
    }
    let mut out = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut lowest = f32::INFINITY;
            for offset in 0..=radius * 2 {
                // The vertical half, unsigned for the reason the horizontal half is.
                let value = if y + offset < radius {
                    0.0
                } else {
                    let sy = y + offset - radius;
                    if sy >= height {
                        0.0
                    } else {
                        horizontal.get(sy * width + x).copied().unwrap_or(0.0)
                    }
                };
                lowest = lowest.min(value);
            }
            if let Some(slot) = out.get_mut(y * width + x) {
                *slot = lowest;
            }
        }
    }
    out
}

fn luma_plane(frame: &Frame) -> Vec<f32> {
    let mut out = Vec::with_capacity(frame.width * frame.height);
    for index in 0..frame.width * frame.height {
        let slot = index * 3;
        out.push(frame.rgb.get(slot..slot + 3).map_or(0.0, |rgb| {
            0.2126 * rgb.first().copied().unwrap_or(0.0)
                + 0.7152 * rgb.get(1).copied().unwrap_or(0.0)
                + 0.0722 * rgb.get(2).copied().unwrap_or(0.0)
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain dark suit filling the frame, with one bright piece of lint on it.
    ///
    /// 256 px a side for the reason `micro::fixtures::SIDE` gives: `MAX_CLOTHING_AREA` is a
    /// fraction of the frame, and on a small fixture an ordinary speck of fluff is a large
    /// fraction of one.
    fn lint_on_plain_fabric() -> (Frame, Vec<f32>) {
        let (width, height) = (256usize, 256usize);
        let rgb = vec![0.12f32; width * height * 3];
        let garment = vec![1.0f32; width * height];
        let mut frame = Frame { rgb, width, height };
        for y in 120..124 {
            for x in 120..124 {
                let index = y * width + x;
                for channel in 0..3 {
                    if let Some(slot) = frame.rgb.get_mut(index * 3 + channel) {
                        *slot = 0.62;
                    }
                }
            }
        }
        (frame, garment)
    }

    #[test]
    fn lint_on_plain_fabric_is_found_and_is_actionable() {
        let (frame, garment) = lint_on_plain_fabric();
        let marks = detect(&frame, &garment);
        assert!(!marks.is_empty(), "no mark was found");
        let first = marks.first().expect("a mark");
        assert!(first.is_actionable(), "the mark was refused: {first:?}");
        assert_eq!(first.kind, ClothingIssue::Lint);
    }

    #[test]
    fn a_thin_long_mark_is_a_thread_rather_than_lint() {
        let (mut frame, garment) = lint_on_plain_fabric();
        // Undo the lint and draw a thread instead.
        for value in &mut frame.rgb {
            *value = 0.12;
        }
        for y in 110..134 {
            let index = y * frame.width + 160;
            for channel in 0..3 {
                if let Some(slot) = frame.rgb.get_mut(index * 3 + channel) {
                    *slot = 0.62;
                }
            }
        }
        let marks = detect(&frame, &garment);
        assert!(
            marks.iter().any(|m| m.kind == ClothingIssue::Thread),
            "no thread was classified: {marks:?}"
        );
    }

    #[test]
    fn nothing_is_cleaned_off_a_patterned_fabric() {
        let (mut frame, garment) = lint_on_plain_fabric();
        for y in 0..frame.height {
            for x in 0..frame.width {
                if (x / 2 + y / 2) % 2 != 0 {
                    continue;
                }
                let index = y * frame.width + x;
                for channel in 0..3 {
                    if let Some(slot) = frame.rgb.get_mut(index * 3 + channel) {
                        *slot = 0.55;
                    }
                }
            }
        }
        let marks = detect(&frame, &garment);
        assert!(
            marks.iter().all(|m| !m.is_actionable()),
            "a mark survived a patterned fabric: {marks:?}"
        );
    }

    #[test]
    fn a_large_mark_is_reported_and_never_actioned() {
        let (mut frame, garment) = lint_on_plain_fabric();
        for y in 40..216 {
            for x in 40..216 {
                let index = y * frame.width + x;
                for channel in 0..3 {
                    if let Some(slot) = frame.rgb.get_mut(index * 3 + channel) {
                        *slot = 0.62;
                    }
                }
            }
        }
        let marks = detect(&frame, &garment);
        assert!(marks.iter().any(|m| m.too_large), "nothing was too large");
        assert!(marks
            .iter()
            .filter(|m| m.too_large)
            .all(|m| !m.is_actionable()));
    }

    #[test]
    fn nothing_outside_the_garment_is_ever_a_candidate() {
        let (frame, mut garment) = lint_on_plain_fabric();
        // The lint is at (120..124, 120..124); take the garment away from under it.
        for y in 100..145 {
            for x in 100..145 {
                if let Some(slot) = garment.get_mut(y * frame.width + x) {
                    *slot = 0.0;
                }
            }
        }
        assert!(detect(&frame, &garment).is_empty());
    }

    #[test]
    fn detection_is_deterministic() {
        let (frame, garment) = lint_on_plain_fabric();
        assert_eq!(detect(&frame, &garment), detect(&frame, &garment));
    }
}
