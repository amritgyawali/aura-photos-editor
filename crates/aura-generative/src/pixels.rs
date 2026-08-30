//! The one pixel view the removal modules share, and the arithmetic all three of them need.
//!
//! [`borrow`](crate::borrow), [`fill`](crate::fill) and [`selfcheck`](crate::selfcheck) each read
//! and write a rectangle of a photograph. They agree about what a photograph is here rather than
//! three times, for the reason `aura_raw::colour::lens` exists: two implementations of a bilinear
//! sample is two answers to where a pixel is, and the disagreement shows up as a half-pixel seam
//! at the edge of a patch, which is exactly the artefact this phase is supposed to catch.
//!
//! ## Everything here is linear and nothing encodes
//!
//! Invariant 8, and the fourth crate to inherit it after `aura-render`, `aura-retouch` and
//! `aura-restore`. An [`Image`] is scene-referred linear Rec.2020, which is what phase 14's
//! pipeline carries between `Stage::CameraMatrix` and `Stage::OutputTransform`. A patch computed
//! against an encoded buffer would match its surroundings on the screen and not in the file.
//!
//! ## This module opens no files and owns no decode
//!
//! An [`Image`] is handed in. Phase 02's `PreviewService` is the only thing in the product that
//! turns a RAW into pixels, and phase 14's `RenderService` is the only thing that turns a recipe
//! into them; this crate is downstream of both and reaches neither. `crates/aura-generative/
//! tests/one_choke_point.rs` fails the build if that changes.

use aura_core::contract::cleanup::Box2;

/// A scene-referred linear image, three channels, interleaved, row major.
///
/// Interleaved rather than planar because every operation here is a neighbourhood read of all
/// three channels at one position - a patch match, a bilinear sample, a gradient - and a planar
/// layout costs three cache lines for what interleaved does in one.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    /// Width in pixels.
    pub w: usize,
    /// Height in pixels.
    pub h: usize,
    /// `w * h * 3` linear samples, red first.
    pub rgb: Vec<f32>,
}

impl Image {
    /// A black image of a given size.
    #[must_use]
    pub fn black(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            rgb: vec![0.0; w * h * 3],
        }
    }

    /// True when the buffer is the length the dimensions claim.
    ///
    /// Checked at every entry point rather than assumed, because a short buffer is the one input
    /// that would make an indexing read silently return the wrong row.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.w > 0 && self.h > 0 && self.rgb.len() == self.w * self.h * 3
    }

    /// The sample at an integer position, clamped to the edge.
    ///
    /// Clamped rather than zero outside, which is phase 18's defect verbatim: `Plane::
    /// resize_bilinear` read zero past the edge and manufactured a one-pixel dark rim around
    /// every upsampled mask. A patch that reads zero off the edge of the frame darkens its own
    /// outermost row, which is a halo produced by the code that is supposed to remove one.
    #[must_use]
    pub fn at(&self, x: isize, y: isize) -> [f32; 3] {
        let cx = x.clamp(0, self.w as isize - 1).max(0) as usize;
        let cy = y.clamp(0, self.h as isize - 1).max(0) as usize;
        let base = (cy * self.w + cx) * 3;
        [
            self.rgb.get(base).copied().unwrap_or(0.0),
            self.rgb.get(base + 1).copied().unwrap_or(0.0),
            self.rgb.get(base + 2).copied().unwrap_or(0.0),
        ]
    }

    /// Write one sample, ignoring a position outside the frame.
    pub fn put(&mut self, x: usize, y: usize, value: [f32; 3]) {
        if x >= self.w || y >= self.h {
            return;
        }
        let base = (y * self.w + x) * 3;
        for (offset, channel) in value.into_iter().enumerate() {
            if let Some(slot) = self.rgb.get_mut(base + offset) {
                *slot = channel;
            }
        }
    }

    /// A bilinear sample at a fractional position, clamped at the edge.
    #[must_use]
    pub fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        let fx = x.floor();
        let fy = y.floor();
        let tx = x - fx;
        let ty = y - fy;
        let ix = fx as isize;
        let iy = fy as isize;
        let p00 = self.at(ix, iy);
        let p10 = self.at(ix + 1, iy);
        let p01 = self.at(ix, iy + 1);
        let p11 = self.at(ix + 1, iy + 1);
        let mut out = [0.0f32; 3];
        for (channel, slot) in out.iter_mut().enumerate() {
            let at = |corner: &[f32; 3]| corner.get(channel).copied().unwrap_or(0.0);
            let top = at(&p00) * (1.0 - tx) + at(&p10) * tx;
            let bottom = at(&p01) * (1.0 - tx) + at(&p11) * tx;
            *slot = top * (1.0 - ty) + bottom * ty;
        }
        out
    }

    /// Rec.2020 relative luminance at an integer position.
    #[must_use]
    pub fn luma(&self, x: isize, y: isize) -> f32 {
        luminance(self.at(x, y))
    }
}

/// Rec.2020 relative luminance of one linear triple.
///
/// The same coefficients `aura_render::output` uses. Written once here rather than inlined at
/// four call sites, because a luminance that disagrees between the fill and the self-check would
/// let a patch pass a check it should have failed.
#[must_use]
pub fn luminance(rgb: [f32; 3]) -> f32 {
    0.2627 * rgb[0] + 0.6780 * rgb[1] + 0.0593 * rgb[2]
}

/// A rectangle in pixels, half-open on the right and bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left edge, inclusive.
    pub x: usize,
    /// Top edge, inclusive.
    pub y: usize,
    /// Width in pixels, at least one.
    pub w: usize,
    /// Height in pixels, at least one.
    pub h: usize,
}

impl Rect {
    /// One past the right edge.
    #[must_use]
    pub const fn right(&self) -> usize {
        self.x + self.w
    }

    /// One past the bottom edge.
    #[must_use]
    pub const fn bottom(&self) -> usize {
        self.y + self.h
    }

    /// Area in pixels.
    #[must_use]
    pub const fn area(&self) -> usize {
        self.w * self.h
    }

    /// True when a position is inside.
    #[must_use]
    pub const fn contains(&self, x: usize, y: usize) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// This rectangle grown by `pad` on every side, clipped to a frame.
    #[must_use]
    pub fn grown(&self, pad: usize, w: usize, h: usize) -> Self {
        let x = self.x.saturating_sub(pad);
        let y = self.y.saturating_sub(pad);
        let right = self.right().saturating_add(pad).min(w);
        let bottom = self.bottom().saturating_add(pad).min(h);
        Self {
            x,
            y,
            w: right.saturating_sub(x).max(1),
            h: bottom.saturating_sub(y).max(1),
        }
    }
}

/// A normalised region resolved onto one image's pixel grid.
///
/// Rounds outward, so a region that covers 3.2 pixels covers four of them. A patch that stops one
/// pixel short of the object it is replacing leaves a rim of the object behind, and a rim of an
/// exit sign is more visible than the sign was.
///
/// Returns `None` for a region that is degenerate or entirely outside the frame, which is the
/// caller's cue to refuse rather than to clamp: a candidate whose rectangle does not land on the
/// photograph is a candidate about a different photograph.
#[must_use]
pub fn resolve(region: &Box2, w: usize, h: usize) -> Option<Rect> {
    if w == 0 || h == 0 || region.w <= 0.0 || region.h <= 0.0 {
        return None;
    }
    let left = (region.x * w as f32).floor().max(0.0) as usize;
    let top = (region.y * h as f32).floor().max(0.0) as usize;
    let right = (((region.x + region.w) * w as f32).ceil() as usize).min(w);
    let bottom = (((region.y + region.h) * h as f32).ceil() as usize).min(h);
    if right <= left || bottom <= top {
        return None;
    }
    Some(Rect {
        x: left,
        y: top,
        w: right - left,
        h: bottom - top,
    })
}

/// One rectangle of an image, copied out onto its own grid.
///
/// The removal modules produce a whole frame and the queue keeps only this, because a plan with
/// three proposals on a 2048 px proxy would otherwise carry a hundred megabytes of pixels around
/// for the sake of three postage stamps. It is also what makes a proposal's preview cheap: the
/// patch is what a before-and-after actually differs by.
#[must_use]
pub fn extract(image: &Image, rect: &Rect) -> Image {
    let mut out = Image::black(rect.w, rect.h);
    for y in 0..rect.h {
        for x in 0..rect.w {
            out.put(x, y, image.at((rect.x + x) as isize, (rect.y + y) as isize));
        }
    }
    out
}

/// Write a patch back onto a frame at a rectangle.
///
/// The inverse of [`extract`] and the only way a stored patch reaches a photograph. A patch whose
/// dimensions do not match the rectangle is refused rather than stretched: a resampled patch is a
/// different set of pixels from the one the self-check passed, and it would carry that check's
/// verdict onto content the check never saw.
pub fn paste(image: &mut Image, patch: &Image, rect: &Rect) -> bool {
    if patch.w != rect.w || patch.h != rect.h || !patch.is_well_formed() {
        return false;
    }
    for y in 0..rect.h {
        for x in 0..rect.w {
            image.put(rect.x + x, rect.y + y, patch.at(x as isize, y as isize));
        }
    }
    true
}

/// The mean and standard deviation of luminance over a rectangle.
///
/// One pass with the shifted-data trick rather than the naive sum of squares, because a linear
/// scene-referred value at the top of a highlight is large and squaring four thousand of them
/// loses the variance in the mean. The shift is the first sample, which costs nothing and is
/// deterministic.
#[must_use]
pub fn luma_stats(image: &Image, rect: &Rect) -> (f32, f32) {
    let mut count = 0.0f32;
    let mut sum = 0.0f32;
    let mut sum_sq = 0.0f32;
    let shift = image.luma(rect.x as isize, rect.y as isize);
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let value = image.luma(x as isize, y as isize) - shift;
            sum += value;
            sum_sq += value * value;
            count += 1.0;
        }
    }
    if count <= 0.0 {
        return (0.0, 0.0);
    }
    let mean = sum / count;
    let variance = (sum_sq / count - mean * mean).max(0.0);
    (mean + shift, variance.sqrt())
}

/// Normalised cross-correlation between two equally sized windows, `-1..1`.
///
/// Normalised rather than a plain sum of absolute differences, because the two frames of a moment
/// are the same room under the same light a third of a second apart, and the thing that changed
/// between them is usually *exposure*: a flash recycled, an aperture ramped, a cloud moved. An SAD
/// score reads that as a mismatch everywhere and refuses every borrow in the wedding; NCC is
/// invariant to gain and offset, which is exactly the difference that does not matter here.
///
/// Returns `0.0` when either window is flat, because a correlation with a constant is undefined
/// and returning `1.0` would make a blank wall match anything.
#[must_use]
pub fn ncc(a: &Image, ax: isize, ay: isize, b: &Image, bx: isize, by: isize, radius: isize) -> f32 {
    let mut mean_a = 0.0f32;
    let mut mean_b = 0.0f32;
    let mut count = 0.0f32;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            mean_a += a.luma(ax + dx, ay + dy);
            mean_b += b.luma(bx + dx, by + dy);
            count += 1.0;
        }
    }
    if count <= 0.0 {
        return 0.0;
    }
    mean_a /= count;
    mean_b /= count;

    let mut cov = 0.0f32;
    let mut var_a = 0.0f32;
    let mut var_b = 0.0f32;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let va = a.luma(ax + dx, ay + dy) - mean_a;
            let vb = b.luma(bx + dx, by + dy) - mean_b;
            cov += va * vb;
            var_a += va * va;
            var_b += vb * vb;
        }
    }
    let denominator = (var_a * var_b).sqrt();
    if denominator <= 1e-9 {
        return 0.0;
    }
    (cov / denominator).clamp(-1.0, 1.0)
}

/// A raised-cosine feather weight for a position inside a rectangle, `0..1`.
///
/// One at the centre, zero at the boundary, and its derivative is zero at both ends - which is why
/// it is a cosine rather than a linear ramp. A linear feather has a slope discontinuity at the
/// edge of the band, and a slope discontinuity in a smooth background is visible as a faint line
/// exactly where a photographer is already looking for one.
#[must_use]
pub fn feather(rect: &Rect, band: usize, x: usize, y: usize) -> f32 {
    if band == 0 {
        return 1.0;
    }
    if !rect.contains(x, y) {
        return 0.0;
    }
    let left = x.saturating_sub(rect.x);
    let right = rect.right().saturating_sub(x + 1);
    let top = y.saturating_sub(rect.y);
    let bottom = rect.bottom().saturating_sub(y + 1);
    let nearest = left.min(right).min(top).min(bottom);
    if nearest >= band {
        return 1.0;
    }
    let t = (nearest as f32 + 0.5) / band as f32;
    0.5 - 0.5 * (std::f32::consts::PI * t).cos()
}

/// A raised-cosine weight that is one **inside** a rectangle and falls to zero over a band
/// **outside** it.
///
/// The companion to [`feather`], and the one every removal actually wants.
///
/// [`feather`] ramps up from the rectangle's own edge inward, which means a caller compositing
/// `original * (1 - w) + replacement * w` over the rectangle blends the outermost pixels of the
/// replacement **back toward the object it is removing**. Both removal modules shipped that first
/// and the symptom was a rim of the distraction left around every patch - produced by the code that
/// exists to hide a seam, which is the same shape of defect as phase 18's resampler manufacturing a
/// halo in the code that delivers a mask.
///
/// The fix is to move the transition off the object: the replacement covers the whole rectangle at
/// full weight, and the falloff happens on the band of real background outside it.
#[must_use]
pub fn feather_out(rect: &Rect, band: usize, x: usize, y: usize) -> f32 {
    if rect.contains(x, y) {
        return 1.0;
    }
    if band == 0 {
        return 0.0;
    }
    let dx = if x < rect.x {
        rect.x - x
    } else if x >= rect.right() {
        x - rect.right() + 1
    } else {
        0
    };
    let dy = if y < rect.y {
        rect.y - y
    } else if y >= rect.bottom() {
        y - rect.bottom() + 1
    } else {
        0
    };
    let distance = dx.max(dy);
    if distance > band {
        return 0.0;
    }
    let t = 1.0 - (distance as f32 - 0.5) / band as f32;
    (0.5 - 0.5 * (std::f32::consts::PI * t.clamp(0.0, 1.0)).cos()).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(w: usize, h: usize) -> Image {
        let mut image = Image::black(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = (x as f32) / (w as f32);
                image.put(x, y, [v, v * 0.5, v * 0.25]);
            }
        }
        image
    }

    #[test]
    fn a_read_past_the_edge_repeats_the_edge_rather_than_returning_black() {
        // Phase 18's defect, which this clamp exists to avoid: reading zero outside the plane
        // darkens the outermost row of every patch and makes a rim.
        let image = ramp(8, 8);
        assert_eq!(image.at(-4, -4), image.at(0, 0));
        assert_eq!(image.at(99, 99), image.at(7, 7));
    }

    #[test]
    fn a_bilinear_sample_at_an_integer_position_is_the_sample_there() {
        let image = ramp(8, 8);
        let exact = image.at(3, 4);
        let sampled = image.sample(3.0, 4.0);
        for channel in 0..3 {
            assert!((exact[channel] - sampled[channel]).abs() < 1e-6);
        }
    }

    #[test]
    fn a_region_resolves_outward_so_no_rim_of_the_object_is_left_behind() {
        let rect = resolve(
            &Box2 {
                x: 0.101,
                y: 0.101,
                w: 0.101,
                h: 0.101,
            },
            100,
            100,
        )
        .expect("a region inside the frame resolves");
        assert_eq!(rect.x, 10);
        assert_eq!(rect.y, 10);
        assert!(rect.right() >= 21, "rounded outward, got {}", rect.right());
    }

    #[test]
    fn a_degenerate_region_refuses_rather_than_clamping() {
        assert!(resolve(
            &Box2 {
                x: 0.5,
                y: 0.5,
                w: 0.0,
                h: 0.1
            },
            64,
            64
        )
        .is_none());
    }

    #[test]
    fn normalised_correlation_ignores_a_gain_and_offset_between_two_frames() {
        let a = ramp(32, 32);
        let mut b = a.clone();
        for sample in &mut b.rgb {
            *sample = *sample * 1.7 + 0.05;
        }
        let score = ncc(&a, 16, 16, &b, 16, 16, 4);
        assert!(
            score > 0.99,
            "an exposure change is not a mismatch: {score}"
        );
    }

    #[test]
    fn normalised_correlation_of_a_flat_window_is_zero_rather_than_one() {
        let flat = Image {
            w: 16,
            h: 16,
            rgb: vec![0.4; 16 * 16 * 3],
        };
        let other = ramp(16, 16);
        assert!(ncc(&flat, 8, 8, &other, 8, 8, 3).abs() < 1e-6);
    }

    #[test]
    fn the_feather_is_one_in_the_middle_zero_outside_and_smooth_at_the_band() {
        let rect = Rect {
            x: 10,
            y: 10,
            w: 20,
            h: 20,
        };
        assert!((feather(&rect, 4, 20, 20) - 1.0).abs() < 1e-6);
        assert!(feather(&rect, 4, 5, 5) < 1e-6);
        let inner = feather(&rect, 4, 10, 20);
        assert!(inner > 0.0 && inner < 0.3, "edge weight was {inner}");
    }

    #[test]
    fn luma_stats_of_a_flat_patch_has_no_spread() {
        let flat = Image {
            w: 16,
            h: 16,
            rgb: vec![0.3; 16 * 16 * 3],
        };
        let (mean, sd) = luma_stats(
            &flat,
            &Rect {
                x: 2,
                y: 2,
                w: 8,
                h: 8,
            },
        );
        assert!((mean - 0.3).abs() < 1e-5, "mean was {mean}");
        assert!(sd < 1e-5, "sd was {sd}");
    }

    #[test]
    fn a_patch_round_trips_through_extract_and_paste() {
        let image = ramp(32, 32);
        let rect = Rect {
            x: 8,
            y: 8,
            w: 6,
            h: 6,
        };
        let patch = extract(&image, &rect);
        let mut blank = Image::black(32, 32);
        assert!(paste(&mut blank, &patch, &rect));
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                assert_eq!(
                    blank.at(x as isize, y as isize),
                    image.at(x as isize, y as isize)
                );
            }
        }
    }

    #[test]
    fn a_patch_of_the_wrong_size_is_refused_rather_than_stretched() {
        let patch = ramp(4, 4);
        let mut image = Image::black(32, 32);
        let rect = Rect {
            x: 2,
            y: 2,
            w: 6,
            h: 6,
        };
        assert!(!paste(&mut image, &patch, &rect));
        assert_eq!(image, Image::black(32, 32));
    }

    #[test]
    fn the_outward_feather_is_one_over_the_whole_object_and_falls_off_outside_it() {
        // The property that stops a removal blending its own outermost pixels back toward the
        // thing it is removing. Every position inside the rectangle is full weight.
        let rect = Rect {
            x: 10,
            y: 10,
            w: 8,
            h: 8,
        };
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                assert!(
                    (feather_out(&rect, 4, x, y) - 1.0).abs() < 1e-6,
                    "the object itself must be fully replaced at {x},{y}"
                );
            }
        }
        // Outside, it falls monotonically to zero.
        let just_outside = feather_out(&rect, 4, 9, 14);
        let further = feather_out(&rect, 4, 7, 14);
        assert!(just_outside > further, "{just_outside} then {further}");
        assert!(feather_out(&rect, 4, 4, 14) < 1e-6);
    }

    #[test]
    fn growing_a_rectangle_stays_inside_the_frame() {
        let rect = Rect {
            x: 1,
            y: 1,
            w: 4,
            h: 4,
        };
        let grown = rect.grown(8, 20, 20);
        assert_eq!(grown.x, 0);
        assert_eq!(grown.y, 0);
        assert!(grown.right() <= 20 && grown.bottom() <= 20);
    }
}
