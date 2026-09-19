//! One decode, eleven readings.
//!
//! Phase 05 wrote the rule this module follows: **descriptors are computed once**, and a phase
//! that re-opens a file to measure a second thing about it is a phase opening a file that did
//! not need opening. A reference photograph is decoded here exactly once and everything the
//! rest of the crate knows about it comes out of that one pass.
//!
//! ## Two colour spaces, deliberately
//!
//! Every *statistic* in a [`ReferenceReading`] - the tone landmarks, the zone tints, the
//! chroma - is measured in CIELAB, because the whole phase is a comparison of two galleries and
//! a difference in a device space is not a perceptual quantity. Phase 02's rule.
//!
//! The *band assignment* is not. [`aura_core::contract::colour::HslBand::of_hue`] takes an HSV
//! hue, because that is the wheel the renderer's own HSL stage operates on, and a band solved
//! from a Lab hue angle would be a shift applied to a different set of pixels than the ones it
//! was measured over. Red sits at 0 degrees on one wheel and near 40 on the other; the gap is
//! not a rounding error, it is most of a band.
//!
//! So: Lab for how much, HSV for which. Both come from the same three samples.

use aura_core::contract::colour::HslBand;
use aura_core::contract::look::{BandReading, ReferenceReading, ToneLandmarks, ZoneTint};
use aura_raw::codec::Rgb8;
use aura_raw::colour::de2000::{xyz_d65_to_lab, Lab};

/// Below this chroma a pixel is treated as neutral and is left out of the band readings.
///
/// Two Lab chroma units. A band reading is a statement about *coloured* content, and a frame is
/// mostly wall, dress and shadow - so including near-neutral pixels makes every band's mean
/// chroma a measurement of how much grey the photograph has, which is the one thing the eight
/// bands are not for. It also keeps the hue angle meaningful: the hue of a grey pixel is noise,
/// and `atan2` returns it with complete confidence.
pub const BAND_CHROMA_FLOOR: f32 = 2.0;

/// At or below this chroma a pixel is neutral enough to estimate the rendered white point from.
///
/// Six Lab chroma units, three times [`BAND_CHROMA_FLOOR`]. The two numbers answer different
/// questions and it would be a mistake to share one: a band reading wants pixels that are
/// definitely coloured, and a white-point estimate wants pixels that are *plausibly* meant to be
/// neutral, which is a much larger set. A wedding dress, a white wall and a grey suit are all in
/// the second and none of them is in the first.
pub const NEUTRAL_CHROMA_CEILING: f32 = 6.0;

/// The fewest neutral pixels an estimate of the rendered colour temperature needs.
///
/// One in two hundred. Below it the estimate is withheld rather than made from eleven pixels of
/// a specular highlight, and [`ReferenceReading::rendered_cct_k`] carries the D65 value that
/// claims nothing.
pub const MIN_NEUTRAL_SHARE: f32 = 0.005;

/// The colour temperature a frame with nothing neutral in it is reported at.
///
/// D65, which is what sRGB encodes against, so "no evidence" and "exactly as neutral as the
/// container assumes" are the same number. That is honest rather than convenient: the reading is
/// about how a *finished JPEG* renders, and a finished JPEG with no neutral content in it
/// renders at its container's white point by definition.
pub const NO_EVIDENCE_CCT_K: f32 = 6504.0;

/// One reference photograph, decoded and reduced.
///
/// Held separately from [`ReferenceReading`] because the reading is a frozen contract shape and
/// this is the working set the measurement is taken over: the Lab samples, kept only as long as
/// the function that made them.
#[derive(Debug)]
struct Samples {
    lab: Vec<Lab>,
    hue_deg: Vec<f32>,
}

/// sRGB's 8-bit encoding to linear, the standard piecewise curve.
fn srgb_to_linear(encoded: u8) -> f64 {
    let value = f64::from(encoded) / 255.0;
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear sRGB to CIE XYZ under D65.
///
/// The published sRGB primaries. It is written out here rather than taken from
/// `aura_raw::colour::working_space`, which holds the inverse - XYZ to sRGB - because inverting a
/// 3x3 at every call to save nine constants is arithmetic nobody can check against a standard.
fn linear_srgb_to_xyz(r: f64, g: f64, b: f64) -> [f64; 3] {
    [
        0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b,
        0.212_672_9 * r + 0.715_152_2 * g + 0.072_175_0 * b,
        0.019_333_9 * r + 0.119_192_0 * g + 0.950_304_1 * b,
    ]
}

/// The HSV hue of one 8-bit sRGB triple, in degrees, or `None` when it has no hue.
fn hsv_hue(r: u8, g: u8, b: u8) -> Option<f32> {
    let r = f32::from(r) / 255.0;
    let g = f32::from(g) / 255.0;
    let b = f32::from(b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let span = max - min;
    if span < 1e-6 {
        return None;
    }
    let hue = if (max - r).abs() < 1e-9 {
        60.0 * (((g - b) / span) % 6.0)
    } else if (max - g).abs() < 1e-9 {
        60.0 * ((b - r) / span + 2.0)
    } else {
        60.0 * ((r - g) / span + 4.0)
    };
    Some(hue.rem_euclid(360.0))
}

/// The chroma of a Lab sample.
fn chroma_of(lab: Lab) -> f32 {
    ((lab.a * lab.a + lab.b * lab.b).sqrt()) as f32
}

/// Decode and reduce one image to its Lab and hue samples.
///
/// Sub-samples on a stride rather than box-filtering down to
/// [`aura_core::contract::look::REFERENCE_LONG_EDGE`]. Every reading in this module is a
/// *distribution* statistic, and a distribution is what sub-sampling preserves and averaging
/// destroys: box-filtering a frame of fine detail pulls its chroma toward the mean and would
/// report a busy photograph as a desaturated one. The cost is a little more noise in the
/// estimate, which a median absorbs.
fn samples_of(image: &Rgb8, long_edge: u32) -> Samples {
    let width = image.width.max(1);
    let height = image.height.max(1);
    let longest = width.max(height);
    let stride = usize::try_from(longest.div_ceil(long_edge.max(1))).unwrap_or(1).max(1);

    let capacity = (width as usize / stride + 1) * (height as usize / stride + 1);
    let mut lab = Vec::with_capacity(capacity);
    let mut hue_deg = Vec::with_capacity(capacity);

    for y in (0..height as usize).step_by(stride) {
        for x in (0..width as usize).step_by(stride) {
            let base = (y * width as usize + x) * 3;
            let Some(triple) = image.data.get(base..base + 3) else {
                continue;
            };
            let (r, g, b) = match triple {
                [r, g, b] => (*r, *g, *b),
                _ => continue,
            };
            let xyz = linear_srgb_to_xyz(
                srgb_to_linear(r),
                srgb_to_linear(g),
                srgb_to_linear(b),
            );
            lab.push(xyz_d65_to_lab(xyz));
            hue_deg.push(hsv_hue(r, g, b).unwrap_or(f32::NAN));
        }
    }

    Samples { lab, hue_deg }
}

/// The value at a quantile of an already-sorted slice.
///
/// Nearest-rank rather than interpolated. The samples are tens of thousands of pixels, so the
/// two answers differ in the fourth decimal place, and nearest-rank is the one that cannot
/// invent a value no pixel had.
fn quantile(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let last = sorted.len() - 1;
    let index = ((q.clamp(0.0, 1.0) * last as f32).round() as usize).min(last);
    sorted.get(index).copied().unwrap_or(0.0)
}

/// Sort a slice of floats ascending, with NaN pushed to the end where the quantiles cannot
/// reach it.
fn sort_floats(values: &mut [f32]) {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
}

/// The mean of a slice, or zero.
fn mean(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f32>() / values.len() as f32
}

/// CIE xy chromaticity to a correlated colour temperature, by `McCamy`'s approximation.
///
/// Good to about two kelvin between 2,850 K and 6,500 K, which covers every light this phase
/// sorts into a bucket. It is used for a *reading* rather than for a correction - phase 15 owns
/// what colour the light was, from a wedding's own neutrals - so an approximation is the right
/// instrument and a rigorous Planckian search would be a more precise answer to a question this
/// phase is not asking.
fn mccamy_cct(x: f32, y: f32) -> f32 {
    let denominator = y - 0.1858;
    if denominator.abs() < 1e-6 {
        return NO_EVIDENCE_CCT_K;
    }
    let n = (x - 0.3320) / denominator;
    let cct = 437.0 * n.powi(3) + 3601.0 * n.powi(2) + 6861.0 * n + 5517.0;
    cct.clamp(1_500.0, 20_000.0)
}

/// What one pass over a frame's samples accumulates.
///
/// A struct rather than a tuple of seven because the caller assigns every field of it straight
/// into a [`ReferenceReading`], and a seven-tuple at that boundary is where two of them get
/// swapped.
#[derive(Debug)]
struct Scan {
    shadow: ZoneTint,
    mid: ZoneTint,
    high: ZoneTint,
    chroma_p50: f32,
    chroma_p90: f32,
    bands: [BandReading; HslBand::COUNT],
    rendered_cct_k: f32,
}

/// The zone tints, the chroma quantiles, the eight bands and the rendered white point, in one
/// pass over the samples.
///
/// The zone cuts are this frame's own quartiles rather than fixed lightness values, so "the
/// shadows" means the darkest quarter of *this* photograph. A fixed cut at L\* 25 puts the whole
/// of a high-key frame in the highlight zone and leaves its shadow tint measured over nothing -
/// which reads as a photographer who does not tint their shadows.
fn scan(samples: &Samples, tone: ToneLandmarks) -> Scan {
    let shadow_cut = tone.p25;
    let high_cut = tone.p75;
    let mut shadow_a = Vec::new();
    let mut shadow_b = Vec::new();
    let mut mid_a = Vec::new();
    let mut mid_b = Vec::new();
    let mut high_a = Vec::new();
    let mut high_b = Vec::new();

    let mut chroma: Vec<f32> = Vec::with_capacity(samples.lab.len());
    let mut neutral_xyz = [0.0_f64; 3];
    let mut neutral_count = 0_usize;

    let mut band_count = [0.0_f32; HslBand::COUNT];
    let mut band_chroma = [0.0_f32; HslBand::COUNT];
    let mut band_luma = [0.0_f32; HslBand::COUNT];
    let mut coloured = 0_usize;

    for (index, lab) in samples.lab.iter().enumerate() {
        let l = (lab.l / 100.0) as f32;
        let a = lab.a as f32;
        let b = lab.b as f32;
        let c = chroma_of(*lab);
        chroma.push(c);

        if l <= shadow_cut {
            shadow_a.push(a);
            shadow_b.push(b);
        } else if l >= high_cut {
            high_a.push(a);
            high_b.push(b);
        } else {
            mid_a.push(a);
            mid_b.push(b);
        }

        if c <= NEUTRAL_CHROMA_CEILING && l > 0.05 && l < 0.98 {
            // Accumulate in XYZ rather than in Lab: a white point is a chromaticity, and
            // averaging two Lab samples of different lightness and taking the chromaticity of
            // the result is not the chromaticity of their average light.
            let xyz = lab_to_xyz(*lab);
            for (slot, value) in neutral_xyz.iter_mut().zip(xyz.iter()) {
                *slot += value;
            }
            neutral_count += 1;
        }

        if c < BAND_CHROMA_FLOOR {
            continue;
        }
        let Some(hue) = samples.hue_deg.get(index).copied() else {
            continue;
        };
        if !hue.is_finite() {
            continue;
        }
        let band = HslBand::of_hue(hue) as usize;
        if let (Some(count), Some(sum_c), Some(sum_l)) = (
            band_count.get_mut(band),
            band_chroma.get_mut(band),
            band_luma.get_mut(band),
        ) {
            *count += 1.0;
            *sum_c += c;
            *sum_l += l;
            coloured += 1;
        }
    }

    let bands = fold_bands(&band_count, &band_chroma, &band_luma, coloured);
    sort_floats(&mut chroma);
    let rendered_cct_k = white_point_cct(neutral_xyz, neutral_count, samples.lab.len());

    Scan {
        shadow: ZoneTint {
            a: mean(&shadow_a),
            b: mean(&shadow_b),
        },
        mid: ZoneTint {
            a: mean(&mid_a),
            b: mean(&mid_b),
        },
        high: ZoneTint {
            a: mean(&high_a),
            b: mean(&high_b),
        },
        chroma_p50: quantile(&chroma, 0.50) / 128.0,
        chroma_p90: quantile(&chroma, 0.90) / 128.0,
        bands,
        rendered_cct_k,
    }
}

/// The eight band readings, from one pass's accumulators.
fn fold_bands(
    counts: &[f32; HslBand::COUNT],
    chroma: &[f32; HslBand::COUNT],
    luma: &[f32; HslBand::COUNT],
    coloured: usize,
) -> [BandReading; HslBand::COUNT] {
    let mut bands = [BandReading::default(); HslBand::COUNT];
    for (index, slot) in bands.iter_mut().enumerate() {
        let count = counts.get(index).copied().unwrap_or(0.0);
        if count <= 0.0 {
            continue;
        }
        *slot = BandReading {
            share: if coloured > 0 {
                count / coloured as f32
            } else {
                0.0
            },
            // Normalised to `0..1` against the Lab chroma a saturated sRGB primary reaches, so
            // the reading is a fraction rather than a unit somebody has to look up. The divisor
            // is a scale and not a ceiling: a value above one is possible and is left alone,
            // because clamping a measurement is how a gamut excursion becomes invisible.
            chroma: (chroma.get(index).copied().unwrap_or(0.0) / count) / 128.0,
            luma: luma.get(index).copied().unwrap_or(0.0) / count,
        };
    }
    bands
}

/// The colour temperature a frame's near-neutral content renders at, or
/// [`NO_EVIDENCE_CCT_K`] when there was not enough of it to ask.
fn white_point_cct(accumulated: [f64; 3], counted: usize, total: usize) -> f32 {
    let share = if total == 0 {
        0.0
    } else {
        counted as f32 / total as f32
    };
    if share < MIN_NEUTRAL_SHARE || counted == 0 {
        return NO_EVIDENCE_CCT_K;
    }
    let sum = accumulated.iter().sum::<f64>();
    if sum.abs() < 1e-9 {
        return NO_EVIDENCE_CCT_K;
    }
    mccamy_cct(
        (accumulated.first().copied().unwrap_or(0.0) / sum) as f32,
        (accumulated.get(1).copied().unwrap_or(0.0) / sum) as f32,
    )
}

/// CIE L\*a\*b\* back to XYZ under D50-scaled D65 reference white.
///
/// The inverse of `aura_raw::colour::de2000::xyz_d65_to_lab`, written here because that module
/// only goes one way and a white-point estimate has to come back.
fn lab_to_xyz(lab: Lab) -> [f64; 3] {
    const DELTA: f64 = 6.0 / 29.0;
    let inverse = |t: f64| {
        if t > DELTA {
            t * t * t
        } else {
            3.0 * DELTA * DELTA * (t - 4.0 / 29.0)
        }
    };
    let fy = (lab.l + 16.0) / 116.0;
    let fx = fy + lab.a / 500.0;
    let fz = fy - lab.b / 200.0;
    [
        inverse(fx) * 0.950_47,
        inverse(fy),
        inverse(fz) * 1.088_83,
    ]
}

/// Everything one reference photograph says about a look.
///
/// # Panics
///
/// Never. Every slice access is checked and every divisor is guarded.
#[must_use]
pub fn read(key: impl Into<String>, image: &Rgb8) -> ReferenceReading {
    read_at(key, image, aura_core::contract::look::REFERENCE_LONG_EDGE)
}

/// [`read`] at a stated scale, for the tests that assert the reading does not move with it.
#[must_use]
pub fn read_at(key: impl Into<String>, image: &Rgb8, long_edge: u32) -> ReferenceReading {
    let samples = samples_of(image, long_edge);
    if samples.lab.is_empty() {
        return ReferenceReading {
            key: key.into(),
            rendered_cct_k: NO_EVIDENCE_CCT_K,
            ..ReferenceReading::default()
        };
    }

    // --- tone landmarks -------------------------------------------------
    //
    // L* rather than a linear or gamma-encoded luminance. "My blacks are lifted" is a statement
    // about where a tone *looks* to sit, and the whole comparison downstream is between two
    // galleries somebody is going to look at.
    let mut luma: Vec<f32> = samples.lab.iter().map(|lab| (lab.l / 100.0) as f32).collect();
    sort_floats(&mut luma);
    let tone = ToneLandmarks::from_array([
        quantile(&luma, 0.01),
        quantile(&luma, 0.05),
        quantile(&luma, 0.25),
        quantile(&luma, 0.50),
        quantile(&luma, 0.75),
        quantile(&luma, 0.95),
        quantile(&luma, 0.99),
    ]);

    let scan = scan(&samples, tone);

    let mut reading = ReferenceReading {
        key: key.into(),
        tone,
        shadow: scan.shadow,
        mid: scan.mid,
        high: scan.high,
        chroma_p50: scan.chroma_p50,
        chroma_p90: scan.chroma_p90,
        bands: scan.bands,
        rendered_cct_k: scan.rendered_cct_k,
        lighting: aura_core::contract::style::LightingBucket::Unknown,
        lighting_confidence: 0.0,
    };

    let (bucket, confidence) = crate::light::bucket(&reading);
    reading.lighting = bucket;
    reading.lighting_confidence = confidence;
    reading
}
