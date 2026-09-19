//! Reference minus baseline, shrunk toward the global lean, then bounded.
//!
//! ## The two halves, and why there are two
//!
//! [`initial`] maps a difference between two [`LookAggregate`]s onto the recipe's parameters
//! with a table of scale constants. Those constants are **authored** - nobody fitted them, there
//! is no data in this repository to fit them on, and they are documented as the argument that
//! produced each one rather than as a measurement. On their own they would be exactly the kind
//! of number this product has refused to ship nine times.
//!
//! [`refine`] is why they are acceptable anyway. It takes the initial guess, renders the
//! photographer's own frames with it through the **real renderer**, measures how far the result
//! actually landed from the reference, and walks each parameter until the distance stops
//! falling. The scale constants therefore decide only where the search *starts*; what ships is
//! measured. Phase 16's rule - a guarantee is measured, not asserted - and phase 17's coordinate
//! descent, pointed at a different target.
//!
//! When there is nothing to render - no analysed frames, a project with no baseline - [`refine`]
//! is not run and [`initial`]'s answer ships with
//! [`aura_core::contract::look::LookCode::BaselineAbsent`] on the row. That is the honest
//! failure: a guess, labelled as one.
//!
//! ## What is deliberately not solved
//!
//! **No hue rotation.** Every `HslShift::h` this module produces is zero, and
//! [`aura_core::contract::look::LookCode::HueRotationWithheld`] is on the wire. A hue rotation
//! solved from a whole-frame band statistic is a rotation applied to every pixel in that band,
//! and most of a face at every skin tone is in the orange band - so the one parameter that would
//! most obviously move somebody's skin is the one with the least evidence behind it. Saturation
//! and luminance per band are kept, because both are magnitudes: getting them slightly wrong
//! makes a colour slightly too strong, and getting a hue wrong makes a person a different
//! colour.
//!
//! **No skin bias.** There is no field for one on [`LookProfile`] and nothing here could fill it.
//! `aura_core::contract::look`'s header, third thing.

use aura_core::contract::colour::{HslBand, HslShift};
use aura_core::contract::look::{
    LookAggregate, MAX_EXPOSURE_DELTA_EV, MAX_TEMPERATURE_DELTA_K,
};
use aura_core::contract::style::{CurveShift, StyleDelta, MAX_PARAM_DELTA};
use aura_raw::colour::de2000::{ciede2000, Lab};

/// How many of the photographer's own frames a bucket needs before its own answer outweighs the
/// global lean.
///
/// Sixteen. The shrinkage is `n / (n + k)`, phase 17's `tree.rs` form and the same argument: at
/// sixteen frames a bucket is trusted half way, and the curve is smooth so there is no sample
/// count at which a bucket's answer jumps. The number is lower than phase 17's `PRIOR_STRENGTH`
/// of twelve is high, because a look has fewer buckets to spread its evidence over - there is no
/// scene axis here - so a bucket that exists at all tends to have more in it.
pub const PRIOR_STRENGTH: f32 = 16.0;

/// Points of recipe contrast per unit of proportional midtone-spread difference.
///
/// A hundred, which makes the mapping "a reference whose midtone spread is a tenth wider than
/// yours asks for ten points of contrast". It is the one scale constant in this file that is
/// close to a definition rather than a judgement: the recipe's contrast parameter is
/// approximately linear in midtone slope over its working range.
pub const CONTRAST_PER_RATIO: f32 = 100.0;

/// Points of recipe blacks per unit of `p01` difference in L\* fraction.
///
/// Four hundred. A photographer lifting their blacks by twenty points moves the darkest real
/// detail of a frame by roughly five points of L\*, which is 0.05 in this module's units.
pub const BLACKS_PER_LUMA: f32 = 400.0;

/// Points of recipe whites per unit of `p99` difference.
///
/// Four hundred, by the argument above read at the other end. It is the same number rather than
/// a separately chosen one because nothing distinguishes the two ends here, and two constants
/// that happen to be equal invite a later change to one of them for a reason that applies to
/// both.
pub const WHITES_PER_LUMA: f32 = 400.0;

/// Points of recipe shadows per unit of `p05` difference, net of what blacks already moved.
pub const SHADOWS_PER_LUMA: f32 = 300.0;

/// Points of recipe highlights per unit of `p95` difference, net of what whites already moved.
pub const HIGHLIGHTS_PER_LUMA: f32 = 300.0;

/// Kelvin of temperature per unit of midtone `b*` difference.
///
/// Eighty. Over the range this phase works in, a hundred kelvin of white-balance movement shifts
/// a midtone's `b*` by something close to 1.25 units, and this is that relationship inverted.
/// It is an approximation of a curve by a line and it is only the starting point for
/// [`refine`].
pub const KELVIN_PER_B: f32 = 80.0;

/// Recipe tint units per unit of midtone `a*` difference.
///
/// Two. The recipe's tint parameter runs green-magenta on roughly the same axis `a*` does.
pub const TINT_PER_A: f32 = 2.0;

/// Points of recipe vibrance per unit of proportional median-chroma difference.
///
/// Sixty, and it is vibrance rather than saturation that carries most of a chroma difference
/// for a reason worth stating: vibrance moves the less saturated pixels more than the already
/// saturated ones, which is what a photographer means when they say a page looks "rich" rather
/// than "oversaturated". Flat saturation gets the remainder that vibrance cannot reach, which is
/// the difference measured at the 90th percentile.
pub const VIBRANCE_PER_CHROMA: f32 = 60.0;

/// Points of recipe saturation per unit of proportional 90th-percentile chroma difference.
pub const SATURATION_PER_CHROMA: f32 = 30.0;

/// Points of per-band saturation per unit of proportional band-chroma difference.
pub const BAND_SAT_PER_CHROMA: f32 = 40.0;

/// Points of per-band luminance per unit of band-luma difference.
pub const BAND_LUMA_PER_LUMA: f32 = 60.0;

/// Below this share of a frame's coloured pixels, a band's difference is not acted on.
///
/// Three per cent. A band holding less than that is a tie, a bouquet stem or a single uplighter,
/// and its median chroma across a page is a measurement of which photographs happened to contain
/// one. Phase 18's rule - a region says how much may be done with it - read as a weight rather
/// than as a gate.
pub const BAND_MIN_SHARE: f32 = 0.03;

/// The recipe's 0-255 curve units per unit of L\* fraction difference at an anchor.
///
/// Two hundred and fifty-five: the curve's output axis *is* the tone axis, so this is a unit
/// conversion rather than a judgement. The curve is what carries the part of a tone difference
/// that the five scalar parameters could not, which is why it is solved last and from the
/// residual.
pub const CURVE_PER_LUMA: f32 = 255.0;

/// The CIE lightness function's linear-segment breakpoint, `6/29`.
const LAB_DELTA: f64 = 6.0 / 29.0;

/// L\* to relative luminance, the CIE inverse.
fn luma_of(l_fraction: f32) -> f32 {
    let l = f64::from(l_fraction.clamp(0.0, 1.0) * 100.0);
    let fy = (l + 16.0) / 116.0;
    let y = if fy > LAB_DELTA {
        fy * fy * fy
    } else {
        3.0 * LAB_DELTA * LAB_DELTA * (fy - 4.0 / 29.0)
    };
    y.max(1e-6) as f32
}

/// A proportional difference, guarded against a zero denominator.
fn ratio(reference: f32, baseline: f32) -> f32 {
    if baseline.abs() < 1e-5 {
        return 0.0;
    }
    reference / baseline - 1.0
}

/// The first guess: a difference between two aggregates, mapped onto the recipe.
///
/// Every number here is a **difference**, which is what makes a look a residual by construction.
/// Two identical aggregates produce [`StyleDelta::neutral`], and so does a reference this
/// photographer's baseline already matches.
#[must_use]
pub fn initial(reference: &LookAggregate, baseline: &LookAggregate) -> StyleDelta {
    if !reference.is_usable() || !baseline.is_usable() {
        return StyleDelta::neutral();
    }

    let mut delta = StyleDelta::neutral();

    // --- exposure -------------------------------------------------------
    //
    // In stops, from relative luminance rather than from L*: a stop is a doubling of light and
    // L* is not linear in light, so a log2 of two L* values is not a number of stops.
    let ref_y = luma_of(reference.tone.p50);
    let base_y = luma_of(baseline.tone.p50);
    delta.exposure = (ref_y / base_y).log2().clamp(-MAX_EXPOSURE_DELTA_EV, MAX_EXPOSURE_DELTA_EV);

    // --- contrast and the four end-point parameters ---------------------
    delta.contrast = ratio(
        reference.tone.midtone_spread(),
        baseline.tone.midtone_spread(),
    ) * CONTRAST_PER_RATIO;

    delta.blacks = (reference.tone.p01 - baseline.tone.p01) * BLACKS_PER_LUMA;
    delta.whites = (reference.tone.p99 - baseline.tone.p99) * WHITES_PER_LUMA;
    // Net of what the end points already moved, so a page whose whole bottom end is lifted does
    // not get the lift counted twice - once in `blacks` and once in `shadows`.
    delta.shadows = ((reference.tone.p05 - baseline.tone.p05)
        - (reference.tone.p01 - baseline.tone.p01))
        * SHADOWS_PER_LUMA;
    delta.highlights = ((reference.tone.p95 - baseline.tone.p95)
        - (reference.tone.p99 - baseline.tone.p99))
        * HIGHLIGHTS_PER_LUMA;

    // --- white balance --------------------------------------------------
    //
    // From the *midtones* rather than from the whole frame. The shadow and highlight tints are
    // what split toning is, and folding them into the white balance would turn a page that cools
    // its shadows into a page that cools everything.
    delta.temperature_k = ((reference.mid.b - baseline.mid.b) * KELVIN_PER_B)
        .clamp(-MAX_TEMPERATURE_DELTA_K, MAX_TEMPERATURE_DELTA_K);
    delta.tint = (reference.mid.a - baseline.mid.a) * TINT_PER_A;

    // --- colour strength -------------------------------------------------
    delta.vibrance = ratio(reference.chroma_p50, baseline.chroma_p50) * VIBRANCE_PER_CHROMA;
    delta.saturation = ratio(reference.chroma_p90, baseline.chroma_p90) * SATURATION_PER_CHROMA;

    // --- per band ---------------------------------------------------------
    for band in HslBand::ALL {
        let index = band as usize;
        let (Some(ref_band), Some(base_band)) =
            (reference.bands.get(index), baseline.bands.get(index))
        else {
            continue;
        };
        // Both sides have to hold enough of the frame for the comparison to be about the band
        // rather than about which photographs happened to contain one.
        if ref_band.share < BAND_MIN_SHARE || base_band.share < BAND_MIN_SHARE {
            continue;
        }
        delta.hsl.set(
            band,
            HslShift {
                // Zero, always. See this module's header.
                h: 0.0,
                s: ratio(ref_band.chroma, base_band.chroma) * BAND_SAT_PER_CHROMA,
                l: (ref_band.luma - base_band.luma) * BAND_LUMA_PER_LUMA,
            },
        );
    }

    // --- the curve, from what is left -------------------------------------
    //
    // Solved last and from the residual the scalars could not carry. The five anchors are
    // `CurveShift::ANCHORS`, and the outer two are handled by `CurveShift::clamped` - white is
    // white, and a curve cannot lift it.
    let residual = |reference_value: f32, baseline_value: f32, carried: f32| {
        ((reference_value - baseline_value) - carried) * CURVE_PER_LUMA
    };
    let exposure_carry = reference.tone.p50 - baseline.tone.p50;
    delta.curve_shift = CurveShift::from_array([
        residual(reference.tone.p01, baseline.tone.p01, delta.blacks / BLACKS_PER_LUMA),
        residual(reference.tone.p25, baseline.tone.p25, exposure_carry),
        0.0,
        residual(reference.tone.p75, baseline.tone.p75, exposure_carry),
        residual(reference.tone.p99, baseline.tone.p99, delta.whites / WHITES_PER_LUMA),
    ]);

    delta.samples = reference.samples;
    delta.confidence = confidence_of(reference, baseline);
    delta.clamped()
}

/// How sure a delta solved from these two aggregates is.
///
/// Three independent things, multiplied rather than averaged, for the reason every geometric
/// fusion in this product is geometric: a look measured from four photographs is not rescued by
/// having been compared against six hundred, and an average would let it be.
#[must_use]
pub fn confidence_of(reference: &LookAggregate, baseline: &LookAggregate) -> f32 {
    let usable = aura_core::contract::look::USABLE_REFERENCES as f32;
    let reference_weight = (reference.samples as f32 / usable).clamp(0.0, 1.0);
    let baseline_weight = (baseline.samples as f32 / usable).clamp(0.0, 1.0);
    // A page whose frames scatter is a page with more than one look on it, and a delta solved
    // from the middle of two looks is a third look nobody has.
    let coherence = (1.0 - (reference.tone_spread / crate::aggregate::INCOHERENT_SPREAD))
        .clamp(0.0, 1.0)
        .max(0.2);
    (reference_weight * baseline_weight * coherence).clamp(0.0, 1.0)
}

/// Pull one bucket's answer toward the global one in proportion to how little evidence it has.
///
/// `global + (bucket - global) * n / (n + k)`, phase 17's James-Stein form. A bucket with one
/// photograph in it returns very nearly the global lean; a bucket with sixty returns very nearly
/// its own.
#[must_use]
pub fn shrink(bucket: &StyleDelta, global: &StyleDelta, samples: u32) -> StyleDelta {
    let weight = samples as f32 / (samples as f32 + PRIOR_STRENGTH);
    blend(global, bucket, weight)
}

/// A linear blend of two deltas: `from * (1 - weight) + to * weight`.
///
/// Also what a photographer's strength override multiplies by, which is why it is public: there
/// is one implementation of "less of this look", and it cannot be handed a weight above one.
#[must_use]
pub fn blend(from: &StyleDelta, to: &StyleDelta, weight: f32) -> StyleDelta {
    let w = weight.clamp(0.0, 1.0);
    let mix = |a: f32, b: f32| a + (b - a) * w;
    let mut out = StyleDelta::neutral();
    out.exposure = mix(from.exposure, to.exposure);
    out.temperature_k = mix(from.temperature_k, to.temperature_k);
    out.tint = mix(from.tint, to.tint);
    out.contrast = mix(from.contrast, to.contrast);
    out.highlights = mix(from.highlights, to.highlights);
    out.shadows = mix(from.shadows, to.shadows);
    out.whites = mix(from.whites, to.whites);
    out.blacks = mix(from.blacks, to.blacks);
    out.vibrance = mix(from.vibrance, to.vibrance);
    out.saturation = mix(from.saturation, to.saturation);

    let from_curve = from.curve_shift.as_array();
    let to_curve = to.curve_shift.as_array();
    let mut curve = [0.0_f32; 5];
    for (slot, (a, b)) in curve.iter_mut().zip(from_curve.iter().zip(to_curve.iter())) {
        *slot = mix(*a, *b);
    }
    out.curve_shift = CurveShift::from_array(curve);

    for band in HslBand::ALL {
        let a = from.hsl.get(band);
        let b = to.hsl.get(band);
        out.hsl.set(
            band,
            HslShift {
                h: 0.0,
                s: mix(a.s, b.s),
                l: mix(a.l, b.l),
            },
        );
    }

    out.skin_bias = aura_core::contract::look::look_skin_bias();
    out.confidence = mix(from.confidence, to.confidence);
    out.samples = from.samples.max(to.samples);
    out.clamped()
}

/// The appearance distance between two aggregates, in dE00.
///
/// Three CIEDE2000 comparisons - shadow, midtone and highlight - weighted toward the midtones,
/// plus a chroma term. It is phase 26's appearance distance applied to a gallery instead of to a
/// camera body, and it shares that phase's rule: **every term measures a frame, and nothing in
/// it reads a parameter.**
///
/// The midtone weight is double the other two because that is where a face is, where a dress is,
/// and where a photographer looks first. The two ends are not free - split toning is most of
/// what distinguishes two otherwise similar looks - but a page that gets its midtones wrong is
/// wrong in a way nobody has to be told about.
#[must_use]
pub fn distance(one: &LookAggregate, two: &LookAggregate) -> f32 {
    let lab = |l: f32, tint: aura_core::contract::look::ZoneTint| Lab {
        l: f64::from(l.clamp(0.0, 1.0) * 100.0),
        a: f64::from(tint.a),
        b: f64::from(tint.b),
    };

    let shadow = ciede2000(
        lab(one.tone.p05, one.shadow),
        lab(two.tone.p05, two.shadow),
    ) as f32;
    let mid = ciede2000(lab(one.tone.p50, one.mid), lab(two.tone.p50, two.mid)) as f32;
    let high = ciede2000(lab(one.tone.p95, one.high), lab(two.tone.p95, two.high)) as f32;

    // Chroma is already in the three comparisons as `a*b*`, but only as the *tint* of each zone
    // - which is the direction colour leans, not how much of it there is. A monochrome page and
    // a saturated one can have identical zone tints and this term is what separates them. It is
    // scaled to dE00's own range so the four numbers can be summed.
    let chroma = ((one.chroma_p50 - two.chroma_p50).abs() * 128.0
        + (one.chroma_p90 - two.chroma_p90).abs() * 128.0)
        * 0.5;

    (shadow + 2.0 * mid + high + chroma) / 5.0
}

/// Where a candidate delta lands, so [`refine`] can measure it without knowing how.
///
/// A port, deliberately **not** frozen, exactly as `aura_style::api::Baseline` and
/// `aura_render::FrameSource` are. The production implementation renders the photographer's own
/// frames with the delta applied and folds the result; the tests use one that applies a known
/// analytic transform, so a refinement failure is a failure of the search rather than of the
/// renderer.
pub trait Probe: std::fmt::Debug {
    /// What the photographer's own frames look like with this delta applied.
    fn aggregate_with(&self, delta: &StyleDelta) -> LookAggregate;
}

/// How many times [`refine`] sweeps every parameter.
///
/// Three. The parameters interact - lifting the exposure moves the midtone spread - so one sweep
/// leaves the later parameters compensating for the earlier ones. Three is where the measured
/// improvement in the evaluation fixtures stops exceeding a tenth of a dE00.
pub const REFINE_SWEEPS: usize = 3;

/// The fractions of its current value each parameter is tried at.
///
/// A bracketed line search rather than a gradient: the renderer is not differentiable in these
/// parameters in any form this crate can reach, and a finite-difference gradient would cost the
/// same renders while assuming a smoothness the tone curve does not have.
///
/// **Four rather than six, and the two that were dropped are the near ones.** Every step here
/// costs [`crate::verify::MAX_REFINE_FRAMES`] renders, and the whole sweep costs
/// `REFINE_SWEEPS * 11 * REFINE_STEPS.len()` of them - so a step's place has to be earned. A
/// factor of 0.9 or 1.1 moves a parameter by less than the measurement's own noise across six
/// frames, which spends eight renders per sweep to accept a move that is not a finding.
pub const REFINE_STEPS: [f32; 4] = [0.5, 0.75, 1.25, 1.6];

/// Walk a delta until the appearance distance to the reference stops falling.
///
/// Returns the improved delta and the distance it reached. **It can only improve**: the initial
/// delta is scored first and is returned unchanged when no step beats it, so a refinement that
/// finds nothing costs renders and changes nothing.
#[must_use]
pub fn refine(initial: &StyleDelta, target: &LookAggregate, probe: &dyn Probe) -> (StyleDelta, f32) {
    let mut best = initial.clamped();
    let mut best_distance = distance(&probe.aggregate_with(&best), target);

    for _ in 0..REFINE_SWEEPS {
        let mut improved = false;
        for axis in Axis::ALL {
            for step in REFINE_STEPS {
                let candidate = axis.scaled(&best, step);
                let scored = distance(&probe.aggregate_with(&candidate), target);
                if scored < best_distance - 1e-4 {
                    best = candidate;
                    best_distance = scored;
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }

    (best, best_distance)
}

/// One parameter the refinement may move.
///
/// The ten scalars and the curve as a whole. Per-band HSL is **not** on this list: a refinement
/// over sixteen more axes costs sixteen times the renders to chase a term that is already small,
/// and every one of those axes is a colour a photographer can see move. The bands ship as
/// [`initial`] solved them, bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Exposure,
    Temperature,
    Tint,
    Contrast,
    Highlights,
    Shadows,
    Whites,
    Blacks,
    Vibrance,
    Saturation,
    Curve,
}

impl Axis {
    const ALL: [Self; 11] = [
        Self::Exposure,
        Self::Temperature,
        Self::Tint,
        Self::Contrast,
        Self::Highlights,
        Self::Shadows,
        Self::Whites,
        Self::Blacks,
        Self::Vibrance,
        Self::Saturation,
        Self::Curve,
    ];

    /// This delta with one axis scaled. The result is clamped, so a step can never leave a
    /// bound - which is what makes "the search only ever returns something inside the contract"
    /// a property of this function rather than of its caller.
    fn scaled(self, delta: &StyleDelta, factor: f32) -> StyleDelta {
        let mut out = delta.clone();
        match self {
            Self::Exposure => out.exposure *= factor,
            Self::Temperature => out.temperature_k *= factor,
            Self::Tint => out.tint *= factor,
            Self::Contrast => out.contrast *= factor,
            Self::Highlights => out.highlights *= factor,
            Self::Shadows => out.shadows *= factor,
            Self::Whites => out.whites *= factor,
            Self::Blacks => out.blacks *= factor,
            Self::Vibrance => out.vibrance *= factor,
            Self::Saturation => out.saturation *= factor,
            Self::Curve => {
                let scaled: Vec<f32> = out
                    .curve_shift
                    .as_array()
                    .iter()
                    .map(|value| value * factor)
                    .collect();
                let mut array = [0.0_f32; 5];
                for (slot, value) in array.iter_mut().zip(scaled.iter()) {
                    *slot = *value;
                }
                out.curve_shift = CurveShift::from_array(array);
            }
        }
        out.clamped()
    }
}

/// The largest scalar in a delta, as a fraction of what this phase allows.
///
/// What the panel puts on the "how strong is this look" bar, and what
/// [`aura_core::contract::look::LookCode::DeltaClamped`] is raised from: a value at one is a
/// value the bound decided rather than the reference.
#[must_use]
pub fn saturation_of_bounds(delta: &StyleDelta) -> f32 {
    let ratios = [
        (delta.exposure / MAX_EXPOSURE_DELTA_EV).abs(),
        (delta.temperature_k / MAX_TEMPERATURE_DELTA_K).abs(),
        (delta.contrast / MAX_PARAM_DELTA).abs(),
        (delta.shadows / MAX_PARAM_DELTA).abs(),
        (delta.highlights / MAX_PARAM_DELTA).abs(),
        (delta.blacks / MAX_PARAM_DELTA).abs(),
        (delta.whites / MAX_PARAM_DELTA).abs(),
        (delta.vibrance / MAX_PARAM_DELTA).abs(),
        (delta.saturation / MAX_PARAM_DELTA).abs(),
    ];
    ratios
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .clamp(0.0, 1.0)
}
