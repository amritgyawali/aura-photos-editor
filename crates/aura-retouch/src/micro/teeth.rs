//! How far these teeth sit outside the locus, and how much of that may go.
//!
//! PHASE-21 section 6.2:
//!
//! > Teeth: even the luminance across the teeth mask and reduce yellow toward a *natural* locus
//! > derived from real teeth measurements, with a ceiling far below cosmetic whitening; skip
//! > entirely if the mask confidence is low or the mouth is small in frame.
//!
//! ## The locus is an offset, never an absolute
//!
//! This is the single most important line in the module. The locus in
//! `crates/aura-retouch/config/micro_retouch.toml` is a small region in CIE `u'v'` expressed
//! **relative to the frame's own neutral**, which phase 15 measured. An absolute locus would do
//! two wrong things at once: under tungsten it would fight the white balance and turn a warm
//! room's teeth blue, and across a gallery it would drive everybody's teeth to one colour, which
//! is the cosmetic whitening this phase exists not to do.
//!
//! With no illuminant estimate there is no origin, so **no colour move is made at all** and the
//! plan records `MicroCode::NoIlluminant`. Phase 15's rule inherited: a measurement with no
//! reference is not a small measurement, it is not a measurement.
//!
//! ## Two ceilings, and the second is the interesting one
//!
//! The first is [`aura_core::contract::micro::MAX_TEETH_LUMA_EV`], a hard stop on the lift.
//!
//! The second has no constant, because it is not a number: **teeth may never end up brighter
//! than the brightest non-specular skin on the same face.** That is what stops the fluorescent
//! result even at a legal lift, it is measured on this frame rather than assumed, and it scales
//! correctly with exposure, with skin tone and with the light. [`solve`] clamps against it and
//! reports `capped` when it binds.
//!
//! ## Evening is separate from lifting
//!
//! Section 6.2 asks for both. The lift moves the whole region and the evening reduces the spread
//! *within* it; the operation carries one luminance magnitude, so what [`solve`] returns is the
//! lift, and the renderer's teeth operator applies it with a per-pixel weight proportional to how
//! far below the region's own upper quartile each sample sits. A tooth already at the top of the
//! region does not move; one in shadow at the back of the mouth moves most. That is evening and
//! lifting in one pass, and it is why a single magnitude is enough.
//!
//! ## Everything here is linear
//!
//! Invariant 8.

use aura_core::contract::composition::Box2;
use aura_core::contract::micro::{ColourLocus, MAX_TEETH_LUMA_EV, MAX_TEETH_YELLOW};
use aura_core::contract::people::FaceRef;
use aura_raw::colour::illuminant::linear_srgb_to_uv;

use crate::texture_guard::Frame;

/// The smallest share of the frame a mouth may be and still be corrected.
///
/// The teeth region's own coverage, not the face's. Below this the teeth are a dozen pixels, the
/// measurement is noise and the correction would be invisible - so the honest answer is
/// `MicroCode::MouthTooSmall` rather than a small number.
pub const MIN_TEETH_COVERAGE: f32 = 0.000_15;

/// The fewest teeth samples a measurement needs.
///
/// Sixty-four. Below this the median and the quartile are the same three pixels.
pub const MIN_TEETH_SAMPLES: u32 = 64;

/// Luminance at or above this is a specular highlight on enamel and is excluded.
///
/// Wet enamel carries a hard specular, and including it would raise the region's own upper
/// quartile - which is the reference the lift is measured against - so every frame would look
/// like the teeth needed nothing.
pub const ENAMEL_SPECULAR: f32 = 0.88;

/// How much of the measured unevenness one correction may take out.
///
/// Two thirds. The remaining third is what keeps teeth reading as separate teeth: a region
/// levelled completely is a white rectangle where a mouth was.
pub const EVENING_SHARE: f32 = 0.667;

/// The share of the gap to the local skin's brightest sample the lift may close.
///
/// Half. The clamp described in the module header, expressed as a fraction rather than as a hard
/// equality, because landing exactly on the skin's own peak is itself a tell.
pub const SKIN_HEADROOM_SHARE: f32 = 0.50;

/// What one mouth needs, and what it may have.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeethDecision {
    /// Luminance lift in stops, at or below [`MAX_TEETH_LUMA_EV`].
    pub luma_ev: f32,
    /// Share of the chromaticity's own excess outside the locus to remove, `0..1`.
    pub yellow_reduce: f32,
    /// True when a ceiling bound the answer.
    pub capped: bool,
    /// True when the lift was bound by the face's own brightest skin rather than by the ceiling.
    ///
    /// Reported separately because it means something different to a photographer: the first is
    /// "this is as far as AURA goes", the second is "any further and the teeth would outshine
    /// her face".
    pub skin_bound: bool,
    /// True when the teeth were already inside the locus and needed no colour move.
    pub already_natural: bool,
    /// How far outside the locus the region's mean chromaticity sat, before.
    pub excess: f32,
    /// How many teeth samples the measurement was taken over.
    pub samples: u32,
}

impl TeethDecision {
    /// True when this decision changes nothing.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.luma_ev.abs() <= f32::EPSILON && self.yellow_reduce <= f32::EPSILON
    }
}

/// What one frame's teeth measure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeethReading {
    /// Median luminance of the non-specular teeth.
    pub median: f32,
    /// Upper quartile of the same.
    pub upper: f32,
    /// Mean chromaticity of the non-specular teeth, in `u'v'`.
    pub uv: [f32; 2],
    /// How many samples it was taken over.
    pub samples: u32,
    /// Fraction of the frame the teeth cover.
    pub coverage: f32,
}

/// Measure one frame's teeth.
///
/// `teeth` is the per-pixel teeth coverage from phase 18, `frame.width * frame.height` long.
/// `None` when there is not enough of it to measure.
#[must_use]
pub fn measure(frame: &Frame, teeth: &[f32]) -> Option<TeethReading> {
    let pixels = frame.width * frame.height;
    if pixels == 0 || teeth.len() < pixels {
        return None;
    }

    let mut luminances: Vec<f32> = Vec::new();
    let mut sum = [0.0f64; 3];
    let mut covered = 0.0f64;
    for index in 0..pixels {
        let coverage = teeth.get(index).copied().unwrap_or(0.0);
        covered += f64::from(coverage);
        // Half rather than "any": a sample the matte is unsure about is a sample on the lip
        // boundary, and a lip in the teeth statistic is what makes a lip get whitened.
        if coverage < 0.5 {
            continue;
        }
        let slot = index * 3;
        let Some(rgb) = frame.rgb.get(slot..slot + 3) else {
            continue;
        };
        let triple = [
            rgb.first().copied().unwrap_or(0.0),
            rgb.get(1).copied().unwrap_or(0.0),
            rgb.get(2).copied().unwrap_or(0.0),
        ];
        let value = luma(triple);
        if value >= ENAMEL_SPECULAR {
            continue;
        }
        luminances.push(value);
        for channel in 0..3 {
            if let (Some(target), Some(source)) = (sum.get_mut(channel), triple.get(channel)) {
                *target += f64::from(*source);
            }
        }
    }

    let samples = luminances.len() as u32;
    if samples < MIN_TEETH_SAMPLES {
        return None;
    }
    luminances.sort_by(f32::total_cmp);
    let median = luminances.get(luminances.len() / 2).copied().unwrap_or(0.0);
    let upper = luminances
        .get(luminances.len() * 3 / 4)
        .copied()
        .unwrap_or(median);

    let mean = [
        (sum[0] / f64::from(samples)) as f32,
        (sum[1] / f64::from(samples)) as f32,
        (sum[2] / f64::from(samples)) as f32,
    ];

    Some(TeethReading {
        median,
        upper,
        uv: linear_srgb_to_uv(mean),
        samples,
        coverage: (covered / pixels as f64) as f32,
    })
}

/// Decide what may be done to one mouth.
///
/// `neutral` is phase 15's illuminant in `u'v'`; `None` skips the colour half entirely.
/// `skin_peak` is the brightest non-specular skin luminance on the same face, which is the clamp
/// the module header describes. `strength` is the frame's own scaling - the scene limit and the
/// region's quality multiplied - and it scales what is asked for, never what is permitted.
///
/// `None` when the mouth is too small in frame to correct.
#[must_use]
pub fn solve(
    reading: &TeethReading,
    neutral: Option<[f32; 2]>,
    locus: ColourLocus,
    skin_peak: f32,
    strength: f32,
) -> Option<TeethDecision> {
    if reading.coverage < MIN_TEETH_COVERAGE || reading.samples < MIN_TEETH_SAMPLES {
        return None;
    }
    let strength = strength.clamp(0.0, 1.0);
    if strength <= 0.0 {
        return None;
    }

    // --- the lift ----------------------------------------------------------------------------
    //
    // The unevenness is the gap between the region's own median and its own upper quartile. The
    // lift closes a share of it, which raises the median toward the brighter teeth without
    // moving the brightest ones at all.
    let unevenness = (reading.upper - reading.median).max(0.0);
    let wanted_ratio = if reading.median > f32::EPSILON {
        (reading.median + unevenness * EVENING_SHARE) / reading.median
    } else {
        1.0
    };
    let mut wanted_ev = wanted_ratio.max(1.0).log2() * strength;
    let mut capped = false;
    let mut skin_bound = false;

    if wanted_ev > MAX_TEETH_LUMA_EV {
        wanted_ev = MAX_TEETH_LUMA_EV;
        capped = true;
    }

    // The second ceiling. See the module header: this is measured on the frame rather than
    // assumed, and it is what stops a legal lift producing a fluorescent result on a face that
    // was already bright.
    if skin_peak > f32::EPSILON && reading.upper > f32::EPSILON {
        let headroom = skin_peak / reading.upper;
        if headroom > 1.0 {
            let allowed = headroom.log2() * SKIN_HEADROOM_SHARE;
            if wanted_ev > allowed {
                wanted_ev = allowed.max(0.0);
                capped = true;
                skin_bound = true;
            }
        } else {
            // The teeth are already at or above the brightest skin on this face. Nothing is
            // added; the colour half may still run.
            wanted_ev = 0.0;
            capped = true;
            skin_bound = true;
        }
    }

    // --- the colour --------------------------------------------------------------------------
    let (excess, yellow) = match neutral {
        None => (0.0, 0.0),
        Some(white) => {
            let du = reading.uv[0] - white[0];
            let dv = reading.uv[1] - white[1];
            let excess = locus.excess(du, dv);
            if excess <= f32::EPSILON {
                (0.0, 0.0)
            } else {
                // A *share of the excess*, not a share of the chromaticity. The move can
                // therefore never cross the locus boundary and can never overshoot into blue,
                // whatever the strength is - which is the property that makes
                // `TEETH_EXCURSION_CEILING` reachable at all.
                let share = (MAX_TEETH_YELLOW * strength).clamp(0.0, MAX_TEETH_YELLOW);
                (excess, share)
            }
        }
    };

    let already_natural = excess <= f32::EPSILON && neutral.is_some();

    Some(TeethDecision {
        luma_ev: wanted_ev.clamp(0.0, MAX_TEETH_LUMA_EV),
        yellow_reduce: yellow.clamp(0.0, MAX_TEETH_YELLOW),
        capped,
        skin_bound,
        already_natural,
        excess,
        samples: reading.samples,
    })
}

/// The brightest non-specular skin luminance on one face.
///
/// The clamp reference. Measured over the face box rather than over the whole frame, because a
/// bright dress behind somebody is not their skin, and measured with the specular highlights
/// excluded for the same reason the teeth measurement excludes them.
#[must_use]
pub fn skin_peak(frame: &Frame, skin: &[f32], face: &FaceRef) -> f32 {
    let bounds = Box2 {
        x: face.bbox.x,
        y: face.bbox.y,
        w: face.bbox.w,
        h: face.bbox.h,
    }
    .clamped();
    let x0 = (bounds.x * frame.width as f32).floor().max(0.0) as usize;
    let y0 = (bounds.y * frame.height as f32).floor().max(0.0) as usize;
    let x1 = (((bounds.x + bounds.w) * frame.width as f32).ceil() as usize).min(frame.width);
    let y1 = (((bounds.y + bounds.h) * frame.height as f32).ceil() as usize).min(frame.height);

    let mut values: Vec<f32> = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let index = y * frame.width + x;
            if skin.get(index).copied().unwrap_or(0.0) < 0.5 {
                continue;
            }
            let slot = index * 3;
            let Some(rgb) = frame.rgb.get(slot..slot + 3) else {
                continue;
            };
            let value = luma([
                rgb.first().copied().unwrap_or(0.0),
                rgb.get(1).copied().unwrap_or(0.0),
                rgb.get(2).copied().unwrap_or(0.0),
            ]);
            if value >= ENAMEL_SPECULAR {
                continue;
            }
            values.push(value);
        }
    }
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    // The 98th percentile rather than the maximum: one stray sample on a sequin should not set
    // the ceiling for somebody's teeth.
    values
        .get((values.len() as f32 * 0.98) as usize)
        .or_else(|| values.last())
        .copied()
        .unwrap_or(0.0)
}

fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    const LOCUS: ColourLocus = ColourLocus {
        du: 0.004,
        dv: 0.006,
        radius: 0.010,
    };

    fn reading(median: f32, upper: f32, uv: [f32; 2]) -> TeethReading {
        TeethReading {
            median,
            upper,
            uv,
            samples: 800,
            coverage: 0.002,
        }
    }

    #[test]
    fn a_yellow_uneven_mouth_is_lifted_and_de_yellowed_inside_both_ceilings() {
        let neutral = [0.1978f32, 0.4683f32];
        // Well outside the locus, in the yellow direction.
        let uv = [neutral[0] + 0.030, neutral[1] + 0.028];
        let decision =
            solve(&reading(0.40, 0.52, uv), Some(neutral), LOCUS, 0.75, 1.0).expect("a decision");
        assert!(decision.luma_ev > 0.0, "no lift: {decision:?}");
        assert!(decision.luma_ev <= MAX_TEETH_LUMA_EV + 1e-6);
        assert!(decision.yellow_reduce > 0.0);
        assert!(decision.yellow_reduce <= MAX_TEETH_YELLOW + 1e-6);
        assert!(!decision.already_natural);
    }

    #[test]
    fn teeth_already_inside_the_locus_get_no_colour_move() {
        let neutral = [0.1978f32, 0.4683f32];
        let uv = [neutral[0] + LOCUS.du, neutral[1] + LOCUS.dv];
        let decision =
            solve(&reading(0.40, 0.52, uv), Some(neutral), LOCUS, 0.75, 1.0).expect("a decision");
        assert_eq!(decision.yellow_reduce, 0.0);
        assert!(decision.already_natural);
    }

    #[test]
    fn with_no_illuminant_the_colour_half_does_nothing_and_says_so_by_being_zero() {
        let decision =
            solve(&reading(0.40, 0.52, [0.24, 0.50]), None, LOCUS, 0.75, 1.0).expect("a decision");
        assert_eq!(decision.yellow_reduce, 0.0);
        assert!(
            !decision.already_natural,
            "no illuminant is not the same as already natural"
        );
    }

    #[test]
    fn the_lift_can_never_take_teeth_past_the_faces_own_brightest_skin() {
        let neutral = [0.1978f32, 0.4683f32];
        // Teeth already almost as bright as the skin: the headroom clamp must bind.
        let decision = solve(
            &reading(0.60, 0.70, [neutral[0] + 0.03, neutral[1] + 0.03]),
            Some(neutral),
            LOCUS,
            0.71,
            1.0,
        )
        .expect("a decision");
        assert!(decision.skin_bound, "the skin clamp did not bind");
        let after = 0.70 * decision.luma_ev.exp2();
        assert!(
            after <= 0.71 + 1e-4,
            "the teeth ended at {after:.4}, above the skin peak 0.71"
        );
    }

    #[test]
    fn teeth_already_brighter_than_the_skin_are_never_lifted() {
        let neutral = [0.1978f32, 0.4683f32];
        let decision = solve(
            &reading(0.70, 0.80, [neutral[0] + 0.03, neutral[1] + 0.03]),
            Some(neutral),
            LOCUS,
            0.40,
            1.0,
        )
        .expect("a decision");
        assert_eq!(decision.luma_ev, 0.0);
        assert!(decision.skin_bound);
    }

    #[test]
    fn a_mouth_too_small_in_frame_is_refused_rather_than_corrected_gently() {
        let mut small = reading(0.40, 0.52, [0.24, 0.50]);
        small.coverage = MIN_TEETH_COVERAGE / 2.0;
        assert!(solve(&small, Some([0.1978, 0.4683]), LOCUS, 0.75, 1.0).is_none());
    }

    #[test]
    fn no_strength_is_no_decision() {
        assert!(solve(
            &reading(0.40, 0.52, [0.24, 0.50]),
            Some([0.1978, 0.4683]),
            LOCUS,
            0.75,
            0.0
        )
        .is_none());
    }
}
