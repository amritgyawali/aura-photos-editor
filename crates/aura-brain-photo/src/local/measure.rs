//! What the pixels say, measured once.
//!
//! PHASE-19 section 11 budgets **80 ms** for the decisions and the map generation of one
//! photograph, on top of a decode phases 06, 09, 11 and 15 have already paid for. That is
//! only reachable if the frame is read once, so this module is the single pass: a luminance
//! plane, a chroma plane, the frame's mean, and the region statistics every operation asks
//! for afterwards.
//!
//! ## Perceptual, not linear, and why that is the right choice *here*
//!
//! Invariant 8 says colour maths happens in linear light, and every value this phase writes
//! into a recipe is applied by `aura-render` in linear Rec.2020. But the measurements here
//! are not colour maths - they are *perceptual* judgements: how bright a face looks, whether
//! a background is pulling the eye, how much of the allowance an edit has spent. Section 6.4
//! asks for the budget "measured as mean absolute change in a perceptual space", in those
//! words, and a mean absolute change measured in linear light would call a shadow lift free
//! and a highlight nudge enormous.
//!
//! So: the plane is gamma-encoded `0..1`, the same space phase 15's bands and phase 09's
//! thresholds live in, and the conversion to the stops a recipe carries happens once, in
//! [`ev_between`], with the encoding exponent written down.

use aura_core::contract::integrity::CropRect;
use aura_core::contract::local::MaskField;
use aura_vision::embed::descriptors::{luma_plane, LumaPlane};

/// The encoding exponent the perceptual/linear conversions assume.
///
/// 2.2 rather than the exact sRGB piecewise curve. The difference between them is below one
/// part in a hundred everywhere above the toe, every number in this phase is a *mean* over
/// thousands of pixels, and the alternative is a transcendental per pixel in the one pass
/// section 11's budget is tightest on.
pub const ENCODING_GAMMA: f32 = 2.2;

/// The exposure change, in stops, that moves a perceptual luminance from one value to
/// another.
///
/// Multiplying linear luminance by `2^ev` multiplies the encoded value by `2^(ev / gamma)`,
/// so the inverse is this. Asked in exactly one place, because a phase with two of these has
/// a face lighting solver and a governor that disagree about what a stop is.
#[must_use]
pub fn ev_between(from: f32, to: f32) -> f32 {
    let from = from.max(1e-4);
    let to = to.max(1e-4);
    ENCODING_GAMMA * (to / from).log2()
}

/// Where a perceptual luminance lands after an exposure change, in the same space.
#[must_use]
pub fn apply_ev(value: f32, ev: f32) -> f32 {
    (value.max(0.0) * (ev / ENCODING_GAMMA).exp2()).clamp(0.0, 1.0)
}

/// One frame, measured.
#[derive(Debug, Clone)]
pub struct FrameMeasure {
    /// Perceptual luminance, row-major, `0..1`.
    pub luma: LumaPlane,
    /// Chroma, row-major, `0..1`: the distance of the pixel from its own grey.
    ///
    /// A cheap saturation rather than a colorimetric chroma, and deliberately: what section
    /// 6.2 wants to know is whether a background is *colourful enough to compete*, which is a
    /// question about how far the pixel is from neutral and not about where it sits on a
    /// chromaticity diagram. Phase 15 owns the colorimetry.
    pub chroma: Vec<f32>,
    /// The frame's mean perceptual luminance.
    pub mean_luma: f32,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
}

/// One region, measured.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RegionStats {
    /// Mean perceptual luminance over the region, weighted by mask coverage.
    pub mean_luma: f32,
    /// Mean chroma over the region.
    pub mean_chroma: f32,
    /// The fraction of the frame the region covers, `0..1`.
    pub area: f32,
    /// The 95th-percentile luminance, for the operations that care about the bright end.
    pub p95_luma: f32,
}

impl RegionStats {
    /// True when nothing was measured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.area <= f32::EPSILON
    }
}

impl FrameMeasure {
    /// Measure one decoded 8-bit sRGB frame.
    #[must_use]
    pub fn of(rgb: &[u8], width: usize, height: usize) -> Self {
        let luma = luma_plane(rgb, width, height);
        let mut chroma = Vec::with_capacity(width * height);
        for pixel in rgb.chunks_exact(3).take(width * height) {
            let (Some(r), Some(g), Some(b)) = (pixel.first(), pixel.get(1), pixel.get(2)) else {
                continue;
            };
            let r = f32::from(*r) / 255.0;
            let g = f32::from(*g) / 255.0;
            let b = f32::from(*b) / 255.0;
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            // Saturation rather than (max - min): a bright doorway and a dim one with the
            // same absolute spread are not equally colourful, and it is the colourfulness
            // that competes.
            chroma.push(if max <= f32::EPSILON {
                0.0
            } else {
                (max - min) / max
            });
        }
        chroma.resize(width * height, 0.0);
        let mean_luma = if luma.values.is_empty() {
            0.0
        } else {
            luma.values.iter().sum::<f32>() / luma.values.len() as f32
        };
        Self {
            luma,
            chroma,
            mean_luma,
            width,
            height,
        }
    }

    /// Statistics over the region one mask field covers.
    ///
    /// The mask's grid is almost always coarser than the frame, so the frame is walked and
    /// the mask is sampled - rather than the reverse. Walking the mask would give every
    /// sample equal weight regardless of how many pixels it stands for, which is wrong the
    /// moment a mask is not square.
    #[must_use]
    pub fn region(&self, mask: &MaskField) -> RegionStats {
        if self.width == 0 || self.height == 0 || mask.width == 0 || mask.height == 0 {
            return RegionStats::default();
        }
        let mut weight = 0.0f32;
        let mut luma_sum = 0.0f32;
        let mut chroma_sum = 0.0f32;
        let mut samples: Vec<f32> = Vec::new();
        for y in 0..self.height {
            let my = (y * usize::from(mask.height) / self.height).min(usize::from(mask.height) - 1);
            for x in 0..self.width {
                let mx =
                    (x * usize::from(mask.width) / self.width).min(usize::from(mask.width) - 1);
                let alpha = mask.sample(mx as u16, my as u16);
                if alpha <= 0.0 {
                    continue;
                }
                let index = y * self.width + x;
                let l = self.luma.values.get(index).copied().unwrap_or(0.0);
                let c = self.chroma.get(index).copied().unwrap_or(0.0);
                weight += alpha;
                luma_sum += l * alpha;
                chroma_sum += c * alpha;
                if alpha > 0.5 {
                    samples.push(l);
                }
            }
        }
        if weight <= f32::EPSILON {
            return RegionStats::default();
        }
        samples.sort_by(f32::total_cmp);
        let p95 = samples
            .get((samples.len() * 95 / 100).min(samples.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0.0);
        RegionStats {
            mean_luma: luma_sum / weight,
            mean_chroma: chroma_sum / weight,
            area: weight / (self.width * self.height) as f32,
            p95_luma: p95,
        }
    }

    /// Statistics over a rectangle, for the measurements that have a box rather than a mask.
    #[must_use]
    pub fn rect(&self, rect: CropRect) -> RegionStats {
        let rect = rect.clamped();
        if self.width == 0 || self.height == 0 || rect.is_empty() {
            return RegionStats::default();
        }
        let x0 = ((rect.x * self.width as f32) as usize).min(self.width.saturating_sub(1));
        let y0 = ((rect.y * self.height as f32) as usize).min(self.height.saturating_sub(1));
        let x1 = (((rect.x + rect.w) * self.width as f32) as usize).min(self.width);
        let y1 = (((rect.y + rect.h) * self.height as f32) as usize).min(self.height);
        let mut count = 0usize;
        let mut luma_sum = 0.0f32;
        let mut chroma_sum = 0.0f32;
        let mut samples: Vec<f32> = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                let index = y * self.width + x;
                let l = self.luma.values.get(index).copied().unwrap_or(0.0);
                luma_sum += l;
                chroma_sum += self.chroma.get(index).copied().unwrap_or(0.0);
                samples.push(l);
                count += 1;
            }
        }
        if count == 0 {
            return RegionStats::default();
        }
        samples.sort_by(f32::total_cmp);
        let p95 = samples
            .get((samples.len() * 95 / 100).min(samples.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0.0);
        RegionStats {
            mean_luma: luma_sum / count as f32,
            mean_chroma: chroma_sum / count as f32,
            area: count as f32 / (self.width * self.height) as f32,
            p95_luma: p95,
        }
    }

    /// Crop the luminance plane to a rectangle, for the operations that work on a face.
    #[must_use]
    pub fn crop_luma(&self, rect: CropRect) -> LumaPlane {
        let rect = rect.clamped();
        let x0 = ((rect.x * self.width as f32) as usize).min(self.width.saturating_sub(1));
        let y0 = ((rect.y * self.height as f32) as usize).min(self.height.saturating_sub(1));
        let x1 = (((rect.x + rect.w) * self.width as f32) as usize).min(self.width);
        let y1 = (((rect.y + rect.h) * self.height as f32) as usize).min(self.height);
        let w = x1.saturating_sub(x0);
        let h = y1.saturating_sub(y0);
        let mut values = Vec::with_capacity(w * h);
        for y in y0..y1 {
            for x in x0..x1 {
                values.push(
                    self.luma
                        .values
                        .get(y * self.width + x)
                        .copied()
                        .unwrap_or(0.0),
                );
            }
        }
        LumaPlane {
            values,
            width: w,
            height: h,
        }
    }
}
