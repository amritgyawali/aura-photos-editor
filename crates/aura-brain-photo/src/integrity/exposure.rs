//! How much of the exposure can be brought back.
//!
//! PHASE-09 section 6.3. The verdict is **recovery-aware rather than histogram-aware**,
//! and section 1 gives the example: "a 2-stop-under dance frame from a modern sensor is
//! fine, the same from an older body is not". Two frames with identical histograms get
//! different verdicts because the question is not how dark the photograph is but what
//! lifting it would cost - and that is a property of the body, which is why
//! [`crate::integrity::calibration`] exists.
//!
//! ## The clipped candle
//!
//! Section 6.3: "compute clipping from the RAW histogram before tone mapping, per
//! channel, with the specular-highlight exclusion mask (a clipped candle flame is not a
//! defect)". Section 10.1 makes it a gate: candle and sparkler frames must not be
//! flagged.
//!
//! [`specular_fraction`] is that mask. A specular highlight is a *light source* or a
//! reflection of one: small, isolated, and surrounded by darkness. A blown sky or a
//! blown wedding dress is large and surrounded by more bright pixels. The test is
//! therefore local rather than global - a clipped pixel whose neighbourhood is dark is a
//! light, and a clipped pixel whose neighbourhood is bright is a loss - and it is the
//! one measurement in this module that a photographer would recognise as obviously
//! right.
//!
//! ## What this build measures, honestly
//!
//! Section 6.3 says the clipping figures come from the RAW histogram *before* tone
//! mapping. This build computes them from the 2048 px proxy, which is after phase 02's
//! documented render. That is a real limitation, it is recorded as condition C3 in the
//! phase 09 exit report, and it is bounded rather than open-ended: `PixelBuffer` carries
//! its `PixelSource`, phase 02's render is documented and reversible in the highlights,
//! and the proxy is built from the RAW's full range - so the *count* of clipped pixels
//! is right and the estimate of what lies above the clip point is the part that a RAW
//! histogram would improve.

use aura_core::contract::integrity::ExposureVerdict;
use aura_vision::embed::descriptors::LumaPlane;

use crate::integrity::calibration::Calibration;

/// Above this, a proxy pixel counts as clipped.
///
/// 253/255, the same white point `aura_vision::embed::descriptors` uses for its
/// luminance statistics. Two modules measuring "blown" differently would be a support
/// ticket nobody could answer, so the constant is copied deliberately and a test asserts
/// the two agree.
pub const WHITE_POINT: f32 = 253.0 / 255.0;

/// Below this, a proxy pixel counts as crushed.
pub const BLACK_POINT: f32 = 2.0 / 255.0;

/// Mid-grey in the proxy's gamma-encoded space.
///
/// 0.18 in linear light is about 0.46 encoded. The target a well-exposed frame's median
/// sits near, and the reference [`ev_offset`] is measured against.
pub const MID_GREY: f32 = 0.46;

/// Half the side of the neighbourhood the specular test looks at, on a 2048 px proxy.
///
/// Sixteen, so a 33×33 window - about 1.6 % of the frame width. Wide enough to reach past
/// a candle flame and its glow, narrow enough that a face beside a window does not reach
/// across into the window.
pub const SPECULAR_RADIUS: usize = 16;

/// How many times the specular window may double before it gives up.
///
/// Three, so a window may reach eight times [`SPECULAR_RADIUS`] - a quarter of the frame
/// width - before the region it is sitting in is declared too large to be a light source.
pub const SPECULAR_GROWTH: u32 = 3;

/// The window radius for a plane of a given width.
///
/// Scaled from [`SPECULAR_RADIUS`] at 2048 px, with a floor of four. The analysers run on
/// the proxy in the product and on 512 px frames in the fixtures, and a radius fixed in
/// pixels would be four times as wide, relative to the content, in the second case.
#[must_use]
pub fn specular_radius(width: usize) -> usize {
    (width * SPECULAR_RADIUS / 2048).max(4)
}

/// How dark a clipped pixel's neighbourhood must be for it to be a light source.
///
/// The window's median must be below this. A candle flame sits in a dark room; a blown
/// dress sits in a bright frame.
pub const SPECULAR_SURROUND: f32 = 0.35;

/// Clipping, per channel and in total, with speculars excluded.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Clipping {
    /// Fraction of pixels clipped at the top, speculars already removed.
    pub high: f32,
    /// Fraction of pixels crushed at the bottom.
    pub low: f32,
    /// Fraction clipped in the red channel alone, before exclusion.
    ///
    /// Per-channel figures are kept because a frame that is blown only in red is a
    /// saturated colour rather than a blown highlight - the tungsten-lit skin of a
    /// mehndi ceremony reaches it constantly - and phase 15 needs to know which.
    pub red: f32,
    /// Green channel.
    pub green: f32,
    /// Blue channel.
    pub blue: f32,
    /// Fraction of clipped pixels that were judged to be light sources.
    pub specular: f32,
}

/// Fraction of pixels clipped in each channel, before any exclusion.
///
/// Per channel because a channel that clips alone is a saturation problem and three
/// channels that clip together are a highlight problem, and only the second is a defect.
#[must_use]
pub fn channel_clipping(rgb: &[u8], width: usize, height: usize) -> (f32, f32, f32) {
    let total = width * height;
    if total == 0 {
        return (0.0, 0.0, 0.0);
    }
    let limit = (WHITE_POINT * 255.0) as u8;
    let mut counts = [0u32; 3];
    for pixel in rgb.chunks_exact(3).take(total) {
        for (channel, slot) in pixel.iter().zip(counts.iter_mut()) {
            if *channel >= limit {
                *slot += 1;
            }
        }
    }
    let scale = |count: u32| -> f32 { f64::from(count) as f32 / total as f32 };
    (scale(counts[0]), scale(counts[1]), scale(counts[2]))
}

/// What fraction of a frame's clipped pixels are light sources rather than losses.
///
/// A clipped pixel is a light source when the median of its neighbourhood is dark. The
/// median rather than the mean, because a candle flame's own bright halo drags a mean up
/// and leaves a median where it belongs.
///
/// Measured on a decimated grid - every fourth pixel in each direction - because the
/// answer is a fraction and sampling one pixel in sixteen changes it by less than the
/// number is ever used to a precision of. On a 2048 px proxy that is 130,000 windows
/// instead of two million, which is the difference between this test costing 4 ms and
/// costing 60.
#[must_use]
pub fn specular_fraction(plane: &LumaPlane) -> f32 {
    let mut clipped = 0u32;
    let mut specular = 0u32;
    let at = |x: usize, y: usize| -> f32 {
        plane.values.get(y * plane.width + x).copied().unwrap_or(0.0)
    };

    let mut y = 0;
    while y < plane.height {
        let mut x = 0;
        while x < plane.width {
            if at(x, y) >= WHITE_POINT {
                clipped += 1;
                if surround_median(plane, x, y) < SPECULAR_SURROUND {
                    specular += 1;
                }
            }
            x += 4;
        }
        y += 4;
    }
    if clipped == 0 {
        return 0.0;
    }
    f64::from(specular) as f32 / f64::from(clipped) as f32
}

/// Median luminance of the *unclipped* pixels around one pixel.
///
/// **Clipped pixels are excluded from the window**, and that exclusion is the whole test
/// rather than a detail. The first version of this function took the median of every
/// pixel in the window, and the phase 09 eval harness caught it: a candle flame thirty
/// pixels across fills a seventeen-pixel window completely, so the median was the flame
/// itself and the flame was not a light source. What a specular highlight actually is, is
/// a blown region *surrounded by darkness* - so the question is about the surroundings,
/// and the surroundings are by definition the pixels that are not blown.
///
/// A window that is entirely clipped returns 1.0 and is therefore not specular, which is
/// the right answer for the middle of a blown sky.
///
/// Thirty-two bins rather than 256: the answer is compared against one threshold, and a
/// coarser histogram costs a quarter of the memory traffic in the innermost loop of the
/// most expensive test in the module.
fn surround_median(plane: &LumaPlane, cx: usize, cy: usize) -> f32 {
    const BINS: usize = 32;
    let base = specular_radius(plane.width);

    // Grow the window until it reaches past the highlight, up to `SPECULAR_GROWTH`
    // doublings. **This is the "small" half of the definition of a specular**: a candle
    // flame is escaped in one or two steps, and a region that is still all-clipped at
    // eight times the base radius is not a light in a dark room - it is a blown sky, a
    // blown dress or a blown wall, and the loop gives up and says so.
    let mut radius = base;
    for _ in 0..=SPECULAR_GROWTH {
        let mut histogram = [0u16; BINS];
        let mut count = 0u32;
        let x0 = cx.saturating_sub(radius);
        let y0 = cy.saturating_sub(radius);
        let x1 = (cx + radius + 1).min(plane.width);
        let y1 = (cy + radius + 1).min(plane.height);
        for y in y0..y1 {
            for x in x0..x1 {
                let value = plane
                    .values
                    .get(y * plane.width + x)
                    .copied()
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                if value >= WHITE_POINT {
                    continue;
                }
                let bin = ((value * (BINS - 1) as f32) as usize).min(BINS - 1);
                if let Some(slot) = histogram.get_mut(bin) {
                    *slot += 1;
                }
                count += 1;
            }
        }
        if count > 0 {
            let target = count / 2;
            let mut seen = 0u32;
            for (bin, hits) in histogram.iter().enumerate() {
                seen += u32::from(*hits);
                if seen > target {
                    return bin as f32 / (BINS - 1) as f32;
                }
            }
            return 1.0;
        }
        radius *= 2;
    }
    // Nothing but blown pixels within eight windows. Not a light source.
    1.0
}

/// Measure a frame's clipping, with speculars excluded from the high figure.
#[must_use]
pub fn measure(plane: &LumaPlane, rgb: &[u8], width: usize, height: usize) -> Clipping {
    let total = plane.values.len();
    if total == 0 {
        return Clipping::default();
    }
    let mut high = 0u32;
    let mut low = 0u32;
    for value in &plane.values {
        if *value >= WHITE_POINT {
            high += 1;
        }
        if *value <= BLACK_POINT {
            low += 1;
        }
    }
    let (red, green, blue) = channel_clipping(rgb, width, height);
    let specular = specular_fraction(plane);
    let raw_high = f64::from(high) as f32 / total as f32;
    Clipping {
        high: (raw_high * (1.0 - specular)).max(0.0),
        low: f64::from(low) as f32 / total as f32,
        red,
        green,
        blue,
        specular,
    }
}

/// How far the frame is from a correct exposure, in stops.
///
/// From the median rather than the mean, because a wedding frame is routinely half
/// black suit and the mean says so. Negative is under-exposed.
///
/// Bounded at six stops either way. Past that the median has reached the noise floor or
/// the clip point and the number stops describing an exposure error - it describes a
/// file with no information left in it, which is what the verdict is for.
#[must_use]
pub fn ev_offset(median: f32) -> f32 {
    if median <= f32::EPSILON {
        return -6.0;
    }
    (median / MID_GREY).log2().clamp(-6.0, 6.0)
}

/// Everything the exposure analysis produced for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExposureResult {
    /// The verdict.
    pub verdict: ExposureVerdict,
    /// Clipping, speculars excluded.
    pub clipping: Clipping,
    /// Stops from correct. Negative is under.
    pub ev_offset: f32,
    /// True when the highlights are past this body's measured headroom.
    pub highlight_lost: bool,
    /// True when the shadows are below the noise floor at this ISO.
    pub shadow_lost: bool,
    /// True when two illuminants of different colour temperature were detected.
    pub mixed_light: bool,
}

/// How much clipping is nothing to worry about.
///
/// Half a per cent of the frame. Every photograph with a light source in it clips
/// something, and a threshold of zero would flag every reception frame in the wedding.
pub const CLIP_IGNORE: f32 = 0.005;

/// How much clipped highlight is a real loss.
///
/// Two per cent. Between this and [`CLIP_IGNORE`] the verdict depends on the body's
/// headroom, which is the band the calibration table earns its place in.
pub const CLIP_SERIOUS: f32 = 0.02;

/// How much crushed shadow is a real loss.
///
/// Six per cent, three times the highlight figure, and the asymmetry is deliberate: a
/// wedding photograph routinely has a genuinely black background - a night ceremony, a
/// dark reception room - and crushing it is not a defect. A blown highlight of the same
/// size almost always is.
pub const SHADOW_SERIOUS: f32 = 0.06;

/// Decide the verdict.
///
/// `iso` and `calibration` are what make it recovery-aware; `scene_tolerance` is
/// invariant 7, and comes from `SceneProfile::max_acceptable_noise` because the cost of
/// lifting a shadow is paid in noise.
#[must_use]
pub fn verdict(
    clipping: Clipping,
    ev_offset: f32,
    iso: u32,
    calibration: &Calibration,
    scene_tolerance: f32,
    mixed_light: bool,
) -> ExposureResult {
    // The highlight question: is what was clipped inside this body's headroom? A frame
    // two stops over with a body that has 1.3 stops of headroom has lost something; the
    // same frame from a body with 1.5 stops has not.
    let over = ev_offset.max(0.0);
    let highlight_lost = clipping.high > CLIP_SERIOUS
        || (clipping.high > CLIP_IGNORE && over > calibration.highlight_headroom_ev);

    // The shadow question, and the one section 1's example is about. `shadow_headroom_at`
    // already reduces the base-ISO figure as ISO rises; the scene tolerance widens it,
    // because a dance floor is allowed to be noisier than a family portrait and lifting
    // its shadows is therefore allowed to cost more.
    let lift_needed = (-ev_offset).max(0.0);
    let headroom = calibration.shadow_headroom_at(iso) * (0.75 + scene_tolerance * 0.5);
    let shadow_lost = clipping.low > SHADOW_SERIOUS && lift_needed > headroom;

    let verdict = if highlight_lost || shadow_lost {
        ExposureVerdict::Lost
    } else if lift_needed <= 0.35 && over <= 0.35 && clipping.high <= CLIP_IGNORE {
        ExposureVerdict::Good
    } else if lift_needed <= headroom && over <= calibration.highlight_headroom_ev {
        // An uncalibrated body may not claim `Recoverable` on the strength of headroom
        // it has never been measured for. `AURA-ML-5037`'s runbook states this as one of
        // the three costs of the fallback, and it is the only one that changes a stored
        // label rather than a confidence.
        if calibration.measured {
            ExposureVerdict::Recoverable
        } else {
            ExposureVerdict::Marginal
        }
    } else {
        ExposureVerdict::Marginal
    };

    ExposureResult {
        verdict,
        clipping,
        ev_offset,
        highlight_lost,
        shadow_lost,
        mixed_light,
    }
}

/// Whether two illuminants of different colour temperature light the same frame.
///
/// The cheap version of a hard problem, and it is documented as such. Two peaks in the
/// distribution of per-pixel blue-to-red ratio, separated by more than a third, with
/// both carrying at least a tenth of the frame. That catches the case the flag exists
/// for - a tungsten-lit room with daylight through a window, or a reception under LED
/// uplighters with a flash - and it will say nothing about a frame lit by two sources of
/// the *same* temperature from different directions, which is the case a real
/// illuminant-estimation pass would catch.
///
/// It is here rather than in phase 15 because it is free from a pass that is already
/// reading every pixel, and because it changes what "recoverable" means: a mixed-light
/// frame cannot be corrected with one white balance, so the exposure verdict is not the
/// whole story about whether it can be rescued.
#[must_use]
pub fn mixed_light_risk(rgb: &[u8], width: usize, height: usize) -> bool {
    const BINS: usize = 24;
    let total = width * height;
    if total < 1_000 {
        return false;
    }
    let mut histogram = [0u32; BINS];
    let mut counted = 0u32;
    for pixel in rgb.chunks_exact(3).take(total).step_by(4) {
        let (Some(red), Some(blue)) = (pixel.first(), pixel.get(2)) else {
            continue;
        };
        // Skip near-black and near-white pixels: a ratio of two small numbers is noise
        // and a ratio of two clipped ones is 1.0 whatever lit them.
        let luma = f32::from(*red).midpoint(f32::from(*blue)) / 255.0;
        if !(0.10..=0.92).contains(&luma) {
            continue;
        }
        let ratio = (f32::from(*blue) + 1.0) / (f32::from(*red) + 1.0);
        // log2 of the ratio, so warm and cool are symmetric around zero, binned over
        // ±2 stops which covers tungsten to shade.
        let position = ((ratio.log2() + 2.0) / 4.0).clamp(0.0, 1.0);
        let bin = ((position * (BINS - 1) as f32) as usize).min(BINS - 1);
        if let Some(slot) = histogram.get_mut(bin) {
            *slot += 1;
        }
        counted += 1;
    }
    if counted < 500 {
        return false;
    }
    let floor = counted / 10;
    let mut peaks: Vec<usize> = Vec::new();
    for (index, hits) in histogram.iter().enumerate() {
        if *hits < floor {
            continue;
        }
        let left = index
            .checked_sub(1)
            .and_then(|before| histogram.get(before))
            .copied()
            .unwrap_or(0);
        let right = histogram.get(index + 1).copied().unwrap_or(0);
        if *hits >= left && *hits >= right {
            peaks.push(index);
        }
    }
    match (peaks.first(), peaks.last()) {
        (Some(first), Some(last)) => last.saturating_sub(*first) >= BINS / 3,
        _ => false,
    }
}
