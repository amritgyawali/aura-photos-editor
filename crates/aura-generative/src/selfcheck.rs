//! The artefact self-check. Three measurements over the *result*, and an automatic revert.
//! Section 6.4, and ADR-0049 section 8.
//!
//! ## The trap this module exists inside, stated before the code
//!
//! Phase 19 tried to detect a halo by comparing the gradient at a boundary before and after an
//! edit, and the ratio scored **the size of the edit**, because every local brightening increases
//! the step at its own boundary. Phase 22 hit the same wall measuring ringing and wrote down the
//! general form: what a defect is, is a pixel pushed *beyond the range its own neighbourhood had*.
//!
//! This is the third instance and the most tempting one, because an inpaint necessarily changes
//! every pixel inside its own region. Anything that measures how much they changed is measuring the
//! removal, and would score a perfect fill of a large object worse than a botched fill of a small
//! one.
//!
//! So each check compares the patch against **the rest of the same frame**:
//!
//! * A repeated texture is only evidence if that spatial period occurs nowhere else. Grass repeats.
//!   A tiled floor repeats. A patch that repeats at a period the photograph does not use is a
//!   synthesis artefact.
//! * A warped line is only evidence if a line was straight *where it enters* the patch. A frame
//!   with no straight lines cannot have a warped one.
//! * A terminated gradient is only evidence if the step at the seam exceeds what steps in this
//!   photograph look like. A frame of high-contrast confetti has large steps everywhere.
//!
//! ## [`inspect`] cannot see the before-state, and that is the point
//!
//! It takes the *result* and the region. There is no parameter it could compare against, so the
//! trap above is closed by the signature rather than by a comment asking the next author not to
//! fall into it. The caller holds the original because it has to revert; this function does not
//! get it.
//!
//! ## The revert is automatic and happens before anybody sees the proposal
//!
//! That is a rule about where the check sits rather than about what it measures. A self-check that
//! ran after review would be asking a photographer to catch what the product already knew.
//! [`crate::queue`] runs it inside the same call that produces the pixels, and a failure becomes
//! `CleanupCode::RevertedOnSelfCheck` with `AURA-ML-5121` - a **warning**, because it is the
//! mechanism working.

use aura_core::contract::cleanup::{Box2, CleanupCode};

use crate::pixels::{self, Image, Rect};

/// How strongly a patch may out-repeat the rest of the frame before it is called synthetic.
///
/// The score is the patch's best periodic autocorrelation *minus* the frame's own, so zero means
/// "repeats no more than this photograph does". A quarter is a patch with a clearly visible
/// repetition that nothing else in the frame shares.
pub const REPEAT_MAX: f32 = 0.25;

/// How far a line entering a patch may bend inside it, as a share of a right angle.
pub const WARP_MAX: f32 = 0.22;

/// What share of the seam may carry a step larger than anything in the rest of the frame.
pub const GHOST_MAX: f32 = 0.18;

/// The gradient coherence a ring needs before "there is a line here" is a claim at all.
///
/// Below this the ring has no dominant direction, so an orientation difference between it and the
/// patch is the difference between two arbitrary numbers. Phase 22's rule: a threshold on a
/// measurement is a statement about the instrument, and this is the point at which the instrument
/// says anything.
pub const LINE_COHERENCE_FLOOR: f32 = 0.55;

/// What the three checks measured.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ArtefactReport {
    /// How much more the patch repeats than the rest of the frame, `0..1`.
    pub repeated_texture: f32,
    /// How far a line entering the patch bends inside it, `0..1` where one is a right angle.
    pub warped_line: f32,
    /// What share of the seam carries a step the rest of the frame does not, `0..1`.
    pub ghost_edge: f32,
    /// How many samples the seam figure was taken over.
    ///
    /// Phase 21's rule: a ratio over eleven samples is arithmetic rather than evidence, and a
    /// stored figure that does not say how many samples it came from cannot be audited.
    pub seam_samples: u32,
}

impl ArtefactReport {
    /// The report for a region nothing was done to.
    pub const CLEAN: Self = Self {
        repeated_texture: 0.0,
        warped_line: 0.0,
        ghost_edge: 0.0,
        seam_samples: 0,
    };

    /// The worst of the three, which is what a stored `artefact_score` carries.
    ///
    /// The maximum rather than a mean, because the three failures are independent and any one of
    /// them alone ruins a photograph. Averaging would let a patch with a badly warped line pass on
    /// the strength of two clean checks it was never going to fail.
    #[must_use]
    pub fn worst(&self) -> f32 {
        self.repeated_texture
            .max(self.warped_line)
            .max(self.ghost_edge)
            .clamp(0.0, 1.0)
    }

    /// The code for the check that failed, if one did.
    #[must_use]
    pub fn failure(&self) -> Option<CleanupCode> {
        // Checked in a fixed order so a patch failing two of them reports the same one every time.
        if self.repeated_texture > REPEAT_MAX {
            return Some(CleanupCode::ArtefactRepeatedTexture);
        }
        if self.warped_line > WARP_MAX {
            return Some(CleanupCode::ArtefactWarpedLine);
        }
        if self.ghost_edge > GHOST_MAX {
            return Some(CleanupCode::ArtefactGhostEdge);
        }
        None
    }

    /// True when the result may be shown to anybody.
    #[must_use]
    pub fn passes(&self) -> bool {
        self.failure().is_none()
    }
}

/// Measure one patched region against the rest of its own frame.
///
/// Returns [`ArtefactReport::CLEAN`] for a region that does not resolve onto the frame, which is
/// unreachable through [`crate::source::select`] - it refuses such a region long before this - and
/// is the safe answer for any caller that reaches here another way, because a clean report on a
/// region nobody patched changes nothing.
#[must_use]
pub fn inspect(result: &Image, region: &Box2) -> ArtefactReport {
    if !result.is_well_formed() {
        return ArtefactReport::CLEAN;
    }
    let Some(rect) = pixels::resolve(region, result.w, result.h) else {
        return ArtefactReport::CLEAN;
    };

    let (seam, samples) = seam_excursion(result, &rect);
    ArtefactReport {
        repeated_texture: repetition(result, &rect),
        warped_line: bend(result, &rect),
        ghost_edge: seam,
        seam_samples: samples,
    }
}

/// The finest luminance step the percentile histogram can resolve.
///
/// One bucket of two hundred and fifty six over `0..1`. A frame with no steps at all - a studio
/// backdrop, a clear sky, a painted wall - has a 99th percentile of zero, and dividing the seam
/// against zero would either bail out (which is what the first implementation did, scoring a hard
/// rectangle edge in a perfectly smooth frame as **no artefact at all**) or make every step
/// infinite.
///
/// Phase 22's rule for the third time in this repository: a threshold on a measurement is a
/// statement about the instrument as well as about the world, and this is the floor at which this
/// instrument says anything.
pub const MIN_STEP_REFERENCE: f32 = 1.0 / 255.0;

/// The largest lag a period profile carries.
///
/// Thirty-two is generous for a region bounded at 4 % of a proxy, whose typical side is a few dozen
/// pixels. A longer profile would spend time on lags no candidate region is wide enough to measure.
pub const MAX_PERIOD_LAG: usize = 32;

/// How much more the patch repeats than the rest of the frame, **at the same period**.
///
/// Comparing the patch's strongest period against the frame's strongest period is wrong and was the
/// first implementation: a photograph whose background carries a slow twenty-pixel undulation
/// scores as high as a patch that repeats hard at four, so the difference comes out at nothing and
/// a synthesis artefact passes. The two maxima are usually at different lags, and comparing them
/// compares two unrelated facts.
///
/// What section 6.4 asks for is "a texture repeated at a period that occurs nowhere else in the
/// frame", so the comparison is per lag: the patch's autocorrelation profile minus the frame's,
/// maximised over the lags. A tiled floor repeats at twelve in both and cancels; a fill that
/// repeats at four in a frame that does not, does not.
fn repetition(image: &Image, rect: &Rect) -> f32 {
    let inside = period_profile(image, rect);
    // Four reference windows of the same size, at the frame's quarter points, skipping any that
    // overlaps the patch. Four rather than one because a single reference window can land on a
    // genuinely periodic thing - a row of chairs - and report the whole frame as repetitive.
    let mut elsewhere: Option<Vec<f32>> = None;
    for (fx, fy) in [(0.2, 0.2), (0.8, 0.2), (0.2, 0.8), (0.8, 0.8)] {
        let cx = (image.w as f32 * fx) as usize;
        let cy = (image.h as f32 * fy) as usize;
        let x = cx
            .saturating_sub(rect.w / 2)
            .min(image.w.saturating_sub(rect.w));
        let y = cy
            .saturating_sub(rect.h / 2)
            .min(image.h.saturating_sub(rect.h));
        let window = Rect {
            x,
            y,
            w: rect.w.min(image.w),
            h: rect.h.min(image.h),
        };
        if overlaps(&window, rect) {
            continue;
        }
        let profile = period_profile(image, &window);
        // The strongest reference at each lag, so a period occurring anywhere else in the frame
        // excuses the same period in the patch. Four reference windows rather than one, because a
        // single one can land on something genuinely periodic - a row of chairs - and report the
        // whole frame as repetitive.
        elsewhere = Some(match elsewhere {
            None => profile,
            Some(best) => best
                .iter()
                .zip(profile.iter())
                .map(|(a, b)| a.max(*b))
                .collect(),
        });
    }
    let Some(elsewhere) = elsewhere else {
        // No reference window fits, which means the patch is most of the frame. That cannot happen
        // under the area cap, and the honest answer for a caller that got here anyway is that
        // nothing was measured.
        return 0.0;
    };
    inside
        .iter()
        .zip(elsewhere.iter())
        .map(|(patch, frame)| (patch - frame).clamp(0.0, 1.0))
        .fold(0.0f32, f32::max)
}

/// The normalised autocorrelation of a window at every lag, horizontal lags then vertical.
///
/// Lags start at two, because a lag of one is ordinary local smoothness and every photograph has
/// it. The upper bound is [`MAX_PERIOD_LAG`], past which there are too few overlapping samples for
/// the correlation to mean anything.
///
/// The profile is a fixed length whatever the window's size, so two windows' profiles can be
/// subtracted lag for lag. A shorter window carries zeroes past its own half-width, which is the
/// correct reading: a period a window is too small to see is not a period it can excuse.
fn period_profile(image: &Image, rect: &Rect) -> Vec<f32> {
    let mut profile = vec![0.0f32; MAX_PERIOD_LAG * 2];
    let max_lag_x = (rect.w / 2).min(MAX_PERIOD_LAG + 1);
    let max_lag_y = (rect.h / 2).min(MAX_PERIOD_LAG + 1);
    for lag in 2..=max_lag_x {
        if let Some(slot) = profile.get_mut(lag - 2) {
            *slot = shifted_correlation(image, rect, lag as isize, 0);
        }
    }
    for lag in 2..=max_lag_y {
        if let Some(slot) = profile.get_mut(MAX_PERIOD_LAG + lag - 2) {
            *slot = shifted_correlation(image, rect, 0, lag as isize);
        }
    }
    profile
}

/// Normalised correlation of a window with itself, shifted.
fn shifted_correlation(image: &Image, rect: &Rect, dx: isize, dy: isize) -> f32 {
    let mut n = 0.0f32;
    let mut sum_a = 0.0f32;
    let mut sum_b = 0.0f32;
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let bx = x as isize + dx;
            let by = y as isize + dy;
            if bx < rect.x as isize
                || by < rect.y as isize
                || bx >= rect.right() as isize
                || by >= rect.bottom() as isize
            {
                continue;
            }
            sum_a += image.luma(x as isize, y as isize);
            sum_b += image.luma(bx, by);
            n += 1.0;
        }
    }
    if n < 8.0 {
        return 0.0;
    }
    let mean_a = sum_a / n;
    let mean_b = sum_b / n;
    let mut cov = 0.0f32;
    let mut var_a = 0.0f32;
    let mut var_b = 0.0f32;
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let bx = x as isize + dx;
            let by = y as isize + dy;
            if bx < rect.x as isize
                || by < rect.y as isize
                || bx >= rect.right() as isize
                || by >= rect.bottom() as isize
            {
                continue;
            }
            let va = image.luma(x as isize, y as isize) - mean_a;
            let vb = image.luma(bx, by) - mean_b;
            cov += va * vb;
            var_a += va * va;
            var_b += vb * vb;
        }
    }
    let denominator = (var_a * var_b).sqrt();
    if denominator <= 1e-9 {
        return 0.0;
    }
    (cov / denominator).clamp(0.0, 1.0)
}

/// How far the patch's dominant direction departs from the ring's, when the ring has one.
fn bend(image: &Image, rect: &Rect) -> f32 {
    let reach = (rect.w.max(rect.h) / 2).max(3);
    let ring = rect.grown(reach, image.w, image.h);
    let (ring_angle, ring_coherence) = orientation(image, &ring, Some(rect));
    if ring_coherence < LINE_COHERENCE_FLOOR {
        // No line enters the patch, so there is no line to have bent. See the module header.
        return 0.0;
    }
    let (patch_angle, patch_coherence) = orientation(image, rect, None);
    if patch_coherence < LINE_COHERENCE_FLOOR * 0.5 {
        // A line entered and the patch has no direction at all: the structure was erased rather
        // than bent. That is a warp of the most complete kind, and reporting it as zero because
        // there was nothing to measure an angle against would pass the worst possible result.
        return 1.0;
    }
    // Gradient orientation is modulo pi, so the largest possible disagreement is a right angle.
    let mut delta = (patch_angle - ring_angle).abs();
    if delta > std::f32::consts::FRAC_PI_2 {
        delta = std::f32::consts::PI - delta;
    }
    ((delta / std::f32::consts::FRAC_PI_2) * ring_coherence).clamp(0.0, 1.0)
}

/// The dominant gradient orientation of a window and how dominant it is.
///
/// Returns the angle of the *structure* rather than of the gradient - the two differ by a right
/// angle - which is what makes "a line entered at this angle" the sentence the caller wants.
fn orientation(image: &Image, window: &Rect, skip: Option<&Rect>) -> (f32, f32) {
    let mut jxx = 0.0f64;
    let mut jyy = 0.0f64;
    let mut jxy = 0.0f64;
    let mut n = 0.0f64;
    for y in window.y..window.bottom() {
        for x in window.x..window.right() {
            if skip.is_some_and(|r| r.contains(x, y)) {
                continue;
            }
            let gx =
                image.luma(x as isize + 1, y as isize) - image.luma(x as isize - 1, y as isize);
            let gy =
                image.luma(x as isize, y as isize + 1) - image.luma(x as isize, y as isize - 1);
            jxx += f64::from(gx * gx);
            jyy += f64::from(gy * gy);
            jxy += f64::from(gx * gy);
            n += 1.0;
        }
    }
    if n < 8.0 {
        return (0.0, 0.0);
    }
    jxx /= n;
    jyy /= n;
    jxy /= n;
    let trace = jxx + jyy;
    if trace < 1e-12 {
        return (0.0, 0.0);
    }
    let diff = ((jxx - jyy) * (jxx - jyy) + 4.0 * jxy * jxy).sqrt();
    let coherence = (diff / trace).clamp(0.0, 1.0) as f32;
    // The dominant gradient direction, then rotated a right angle to give the structure's.
    let gradient_angle = 0.5 * (2.0 * jxy).atan2(jxx - jyy);
    let structure_angle = gradient_angle as f32 + std::f32::consts::FRAC_PI_2;
    (wrap_pi(structure_angle), coherence)
}

/// Wrap an angle into `-pi/2 ..= pi/2`, which is the range an orientation lives in.
fn wrap_pi(angle: f32) -> f32 {
    let mut a = angle % std::f32::consts::PI;
    if a > std::f32::consts::FRAC_PI_2 {
        a -= std::f32::consts::PI;
    }
    if a < -std::f32::consts::FRAC_PI_2 {
        a += std::f32::consts::PI;
    }
    a
}

/// What share of the seam carries a step larger than the rest of the frame has anywhere.
///
/// Phase 22's definition of a defect, applied here: not "the seam changed", but "the seam is
/// **outside the range this photograph's own neighbourhoods occupy**". The reference is the 99th
/// percentile of adjacent-sample luminance steps over the whole frame, so a photograph of confetti
/// under a spotlight has a high bar and a photograph of a grey wall has a low one, and the same
/// seam is an artefact in the second and not in the first.
fn seam_excursion(image: &Image, rect: &Rect) -> (f32, u32) {
    // Floored at the instrument's own resolution, and measured over everything *but* the patch: a
    // reference that included the seam would be partly made of the thing it is judging, and a large
    // patch in a smooth frame would raise its own bar until it cleared it.
    let reference = step_percentile(image, 0.99, rect).max(MIN_STEP_REFERENCE);
    let mut exceeded = 0u32;
    let mut samples = 0u32;

    // Vertical seams: the two columns either side of the patch's left and right edges.
    for y in rect.y..rect.bottom() {
        for x in [rect.x, rect.right()] {
            if x == 0 || x >= image.w {
                continue;
            }
            let step =
                (image.luma(x as isize, y as isize) - image.luma(x as isize - 1, y as isize)).abs();
            samples += 1;
            if step > reference {
                exceeded += 1;
            }
        }
    }
    // Horizontal seams.
    for x in rect.x..rect.right() {
        for y in [rect.y, rect.bottom()] {
            if y == 0 || y >= image.h {
                continue;
            }
            let step =
                (image.luma(x as isize, y as isize) - image.luma(x as isize, y as isize - 1)).abs();
            samples += 1;
            if step > reference {
                exceeded += 1;
            }
        }
    }

    if samples == 0 {
        return (0.0, 0);
    }
    (
        f32::from(u16::try_from(exceeded).unwrap_or(u16::MAX))
            / f32::from(u16::try_from(samples).unwrap_or(u16::MAX)).max(1.0),
        samples,
    )
}

/// The given percentile of adjacent-sample luminance steps across the whole frame.
///
/// Bucketed rather than sorted: a 45 MP frame has ninety million steps and sorting them to find
/// one number would dominate the whole self-check. Two hundred and fifty six buckets over `0..1`
/// resolves the percentile to about four thousandths of a stop, which is far finer than the
/// decision being made from it.
fn step_percentile(image: &Image, percentile: f32, skip: &Rect) -> f32 {
    const BUCKETS: usize = 256;
    let mut histogram = [0u32; BUCKETS];
    let mut total = 0u32;
    // A stride, because the reference is a property of the photograph and does not need every
    // pixel of it. Fixed rather than adaptive so two runs of the same frame sample the same
    // positions. Invariant 4.
    let stride = ((image.w * image.h) / 200_000).max(1);
    let mut index = 0usize;
    for y in 1..image.h {
        for x in 1..image.w {
            index += 1;
            if !index.is_multiple_of(stride) {
                continue;
            }
            // The patch, and the row and column immediately outside it, are the thing being judged.
            if skip.grown(1, image.w, image.h).contains(x, y) {
                continue;
            }
            let dx =
                (image.luma(x as isize, y as isize) - image.luma(x as isize - 1, y as isize)).abs();
            let dy =
                (image.luma(x as isize, y as isize) - image.luma(x as isize, y as isize - 1)).abs();
            for step in [dx, dy] {
                let bucket =
                    ((step.clamp(0.0, 1.0) * (BUCKETS - 1) as f32) as usize).min(BUCKETS - 1);
                if let Some(slot) = histogram.get_mut(bucket) {
                    *slot += 1;
                }
                total += 1;
            }
        }
    }
    if total == 0 {
        return 0.0;
    }
    let want = (f64::from(total) * f64::from(percentile)) as u32;
    let mut seen = 0u32;
    for (bucket, count) in histogram.iter().enumerate() {
        seen += *count;
        if seen >= want {
            return (bucket as f32) / ((BUCKETS - 1) as f32);
        }
    }
    1.0
}

/// True when two rectangles share any pixel.
fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region_of(rect: Rect, w: usize, h: usize) -> Box2 {
        Box2 {
            x: rect.x as f32 / w as f32,
            y: rect.y as f32 / h as f32,
            w: rect.w as f32 / w as f32,
            h: rect.h as f32 / h as f32,
        }
    }

    fn noisy(w: usize, h: usize) -> Image {
        let mut image = Image::black(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = 0.30 + 0.14 * hash01(x, y);
                image.put(x, y, [v, v, v]);
            }
        }
        image
    }

    /// A deterministic value in `0..1` with no autocorrelation at any lag.
    ///
    /// The first version of this fixture was `(x * 7919 + y * 104_729) % 1000`, which *looks* like
    /// noise and is a linear congruence: it repeats, and the untouched-frame gate failed at 0.252
    /// against a threshold of 0.25 because the reference windows and the patch happened to sit at
    /// different phases of its period. A fixture that sits on its own threshold measures f32
    /// arithmetic rather than the rule - the trap phases 19, 21 and 22 each hit - and here it was
    /// the fixture that was wrong rather than the code.
    fn hash01(x: usize, y: usize) -> f32 {
        let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2545_F491);
        h ^= h >> 13;
        f32::from(u16::try_from(h % 10_000).unwrap_or(0)) / 10_000.0
    }

    const PATCH: Rect = Rect {
        x: 40,
        y: 40,
        w: 24,
        h: 24,
    };

    #[test]
    fn an_untouched_frame_passes_every_check() {
        let frame = noisy(120, 120);
        let report = inspect(&frame, &region_of(PATCH, 120, 120));
        assert!(report.passes(), "{report:?}");
        assert_eq!(report.failure(), None);
    }

    #[test]
    fn a_patch_repeating_at_a_period_the_frame_does_not_use_is_caught() {
        let mut frame = noisy(120, 120);
        // A hard four-pixel stripe inside the patch only. Nothing else in the frame repeats at
        // four, which is what makes this evidence rather than a description of the photograph.
        for y in PATCH.y..PATCH.bottom() {
            for x in PATCH.x..PATCH.right() {
                let v = if (x / 2) % 2 == 0 { 0.62 } else { 0.20 };
                frame.put(x, y, [v, v, v]);
            }
        }
        let report = inspect(&frame, &region_of(PATCH, 120, 120));
        assert!(
            report.repeated_texture > REPEAT_MAX,
            "repetition was {}",
            report.repeated_texture
        );
        assert_eq!(report.failure(), Some(CleanupCode::ArtefactRepeatedTexture));
    }

    #[test]
    fn a_slow_undulation_in_the_frame_does_not_excuse_a_hard_short_period_in_the_patch() {
        // The defect the per-lag comparison fixes. The background carries a strong twenty-pixel
        // wave, so its strongest period is as strong as the patch's - and comparing the two maxima
        // scored this artefact at nothing.
        let mut frame = Image::black(140, 140);
        for y in 0..140 {
            for x in 0..140 {
                let v = 0.35 + 0.14 * ((x as f32) * 0.314).sin();
                frame.put(x, y, [v, v, v]);
            }
        }
        for y in PATCH.y..PATCH.bottom() {
            for x in PATCH.x..PATCH.right() {
                let v = if (x / 2) % 2 == 0 { 0.62 } else { 0.20 };
                frame.put(x, y, [v, v, v]);
            }
        }
        let report = inspect(&frame, &region_of(PATCH, 140, 140));
        assert!(
            report.repeated_texture > REPEAT_MAX,
            "a hard four-pixel repeat was excused by a twenty-pixel wave: {}",
            report.repeated_texture
        );
    }

    #[test]
    fn a_frame_that_repeats_everywhere_is_not_an_artefact() {
        // The trap this check is written around: a tiled floor repeats, and a patch of tiled floor
        // that repeats exactly as much as the floor does is a correct fill.
        let mut frame = Image::black(120, 120);
        for y in 0..120 {
            for x in 0..120 {
                let v = if (x / 6) % 2 == 0 { 0.55 } else { 0.25 };
                frame.put(x, y, [v, v, v]);
            }
        }
        let report = inspect(&frame, &region_of(PATCH, 120, 120));
        assert!(
            report.repeated_texture <= REPEAT_MAX,
            "a tiled floor was called an artefact at {}",
            report.repeated_texture
        );
    }

    #[test]
    fn a_line_that_bends_inside_the_patch_is_caught() {
        let mut frame = Image::black(140, 140);
        // Horizontal bars everywhere, so the ring carries a strong horizontal structure.
        for y in 0..140 {
            for x in 0..140 {
                let v = if (y / 7) % 2 == 0 { 0.62 } else { 0.18 };
                frame.put(x, y, [v, v, v]);
            }
        }
        // Inside the patch, the bars run vertically instead: a right-angle warp.
        for y in PATCH.y..PATCH.bottom() {
            for x in PATCH.x..PATCH.right() {
                let v = if (x / 7) % 2 == 0 { 0.62 } else { 0.18 };
                frame.put(x, y, [v, v, v]);
            }
        }
        let report = inspect(&frame, &region_of(PATCH, 140, 140));
        assert!(
            report.warped_line > WARP_MAX,
            "the bend measured {}",
            report.warped_line
        );
    }

    #[test]
    fn a_frame_with_no_lines_cannot_have_a_warped_one() {
        let frame = noisy(120, 120);
        let report = inspect(&frame, &region_of(PATCH, 120, 120));
        assert!(report.warped_line < 1e-6, "{}", report.warped_line);
    }

    #[test]
    fn a_structure_erased_inside_the_patch_scores_the_worst_possible_bend() {
        // A line entered and the patch has no direction at all. Reporting zero because there was
        // nothing to measure an angle against would let the most complete failure pass.
        let mut frame = Image::black(140, 140);
        for y in 0..140 {
            for x in 0..140 {
                let v = if (y / 7) % 2 == 0 { 0.62 } else { 0.18 };
                frame.put(x, y, [v, v, v]);
            }
        }
        for y in PATCH.y..PATCH.bottom() {
            for x in PATCH.x..PATCH.right() {
                frame.put(x, y, [0.40, 0.40, 0.40]);
            }
        }
        let report = inspect(&frame, &region_of(PATCH, 140, 140));
        assert!(report.warped_line > WARP_MAX, "{}", report.warped_line);
    }

    #[test]
    fn a_seam_outside_the_frames_own_range_is_caught_and_one_inside_it_is_not() {
        // Phase 22's definition applied twice on the same geometry. In a smooth frame a hard
        // rectangle edge is an excursion; in a frame that is full of hard edges it is not.
        let mut smooth = Image::black(120, 120);
        for y in 0..120 {
            for x in 0..120 {
                let v = 0.30 + 0.0005 * x as f32;
                smooth.put(x, y, [v, v, v]);
            }
        }
        let mut ghosted = smooth.clone();
        for y in PATCH.y..PATCH.bottom() {
            for x in PATCH.x..PATCH.right() {
                ghosted.put(x, y, [0.70, 0.70, 0.70]);
            }
        }
        let bad = inspect(&ghosted, &region_of(PATCH, 120, 120));
        assert!(
            bad.ghost_edge > GHOST_MAX,
            "seam excursion was {}",
            bad.ghost_edge
        );
        assert!(bad.seam_samples > 0);

        let clean = inspect(&smooth, &region_of(PATCH, 120, 120));
        assert!(clean.ghost_edge <= GHOST_MAX, "{}", clean.ghost_edge);
    }

    #[test]
    fn the_worst_of_three_is_the_maximum_rather_than_the_mean() {
        let report = ArtefactReport {
            repeated_texture: 0.0,
            warped_line: 0.9,
            ghost_edge: 0.0,
            seam_samples: 96,
        };
        assert!((report.worst() - 0.9).abs() < 1e-6);
        assert_eq!(report.failure(), Some(CleanupCode::ArtefactWarpedLine));
        assert!(!report.passes());
    }

    #[test]
    fn the_check_is_deterministic() {
        let frame = noisy(120, 120);
        let first = inspect(&frame, &region_of(PATCH, 120, 120));
        let second = inspect(&frame, &region_of(PATCH, 120, 120));
        assert_eq!(first, second);
    }
}
