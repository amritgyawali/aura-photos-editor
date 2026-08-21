//! How much redness is in the sclera, and how much definition the iris has lost.
//!
//! PHASE-21 section 6.2:
//!
//! > Eyes: reduce sclera redness (chroma only), add small iris local contrast, and explicitly
//! > protect catchlights by excluding specular pixels; no enlargement, no colour change, no
//! > whitening of the sclera beyond a cap.
//!
//! ## Four things this module structurally cannot do
//!
//! **It cannot enlarge an eye.** [`EyeDecision`] carries two scalars and no geometry, and so does
//! `MicroOp::Eyes`. There is nowhere to put a displacement.
//!
//! **It cannot change eye colour.** The iris half is a *local contrast* gain - it multiplies the
//! iris's departure from its own local mean and leaves the mean alone - so the hue of every iris
//! pixel is arithmetically unchanged. The renderer implements it that way and
//! `shader_parity.rs` holds the device path to the same thing.
//!
//! **It cannot whiten the sclera.** The sclera half is chroma-only: it reduces the *saturation*
//! of the redness by a bounded share and does not touch luminance at all. A sclera that reads as
//! grey stays grey; only the pink comes down.
//!
//! **It cannot dull a catchlight.** Specular samples are excluded from every statistic here, the
//! renderer's operator excludes them again, and `guard::enforce` measures the peak iris
//! luminance after the fact and withdraws the whole eye family if it dropped. Three layers,
//! because a catchlight is the single thing whose loss makes an edited eye read as dead.
//!
//! ## Redness is measured against the eye's own sclera, never against a constant
//!
//! There is no target white here. The measurement is the sclera's own chroma in `u'v'` relative
//! to the frame's neutral, and the operation removes a bounded share of the *excess* outside the
//! locus. A sclera already inside the locus is at excess zero and nothing happens.
//!
//! ## Everything here is linear
//!
//! Invariant 8.

use aura_core::contract::micro::{ColourLocus, MAX_IRIS_CLARITY, MAX_SCLERA};
use aura_raw::colour::illuminant::linear_srgb_to_uv;

use crate::texture_guard::Frame;

/// Luminance at or above this is a catchlight and is excluded from every statistic.
///
/// Deliberately the same number as `aura_render::micro::SPECULAR_FLOOR`, which is what the
/// operator excludes at. Two different specular thresholds would mean the decision measured one
/// set of pixels and the renderer protected another.
pub const SPECULAR_FLOOR: f32 = 0.90;

/// The fewest sclera samples a redness measurement needs.
pub const MIN_SCLERA_SAMPLES: u32 = 24;

/// The fewest iris samples a clarity measurement needs.
pub const MIN_IRIS_SAMPLES: u32 = 24;

/// The local contrast an iris is expected to carry, as a fraction of its own mean luminance.
///
/// A healthy iris at proxy scale carries about this much structure - the fibres, the pupil edge
/// and the limbal ring. Below it the iris has been flattened by defocus, by noise reduction or by
/// a small face, and *adding* contrast there amplifies whatever is left rather than recovering
/// what is gone.
pub const EXPECTED_IRIS_DETAIL: f32 = 0.085;

/// The smallest detail deficit worth acting on.
pub const MIN_IRIS_DEFICIT: f32 = 0.10;

/// The radius the iris's local mean is taken over, as a fraction of the iris region's own side.
pub const IRIS_DETAIL_FRAC: f32 = 1.0 / 6.0;

/// What one pair of eyes needs, and what they may have.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeDecision {
    /// Share of the sclera's measured redness excess to remove, `0..1`.
    pub sclera: f32,
    /// Iris local contrast gain, `0..1`.
    pub iris_clarity: f32,
    /// True when a ceiling bound the answer.
    pub capped: bool,
    /// How far outside the locus the sclera's mean chromaticity sat.
    pub redness: f32,
    /// How much local contrast the iris carries, as a fraction of its own mean.
    pub iris_detail: f32,
    /// How many samples the two measurements were taken over, summed.
    pub samples: u32,
}

impl EyeDecision {
    /// True when this decision changes nothing.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.sclera <= f32::EPSILON && self.iris_clarity <= f32::EPSILON
    }
}

/// What one frame's eyes measure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeReading {
    /// Mean chromaticity of the non-specular sclera, in `u'v'`.
    pub sclera_uv: [f32; 2],
    /// How many sclera samples that was.
    pub sclera_samples: u32,
    /// Mean absolute local residual inside the iris, over the iris's own mean luminance.
    pub iris_detail: f32,
    /// How many iris samples that was.
    pub iris_samples: u32,
    /// Peak luminance found inside the iris, before anything ran.
    ///
    /// Carried so a caller can tell a frame with a catchlight from one without: an eye with no
    /// specular sample at all is an eye whose catchlight the guard cannot protect, and the
    /// honest reading is to leave it alone.
    pub iris_peak: f32,
}

/// Measure one frame's eyes.
///
/// `sclera` and `iris` are the per-pixel coverages from phase 18. Either may be empty, and the
/// corresponding half of the reading is then absent rather than guessed.
#[must_use]
pub fn measure(frame: &Frame, sclera: &[f32], iris: &[f32]) -> EyeReading {
    let pixels = frame.width * frame.height;

    // --- the sclera --------------------------------------------------------------------------
    let mut sum = [0.0f64; 3];
    let mut sclera_samples = 0u32;
    for index in 0..pixels {
        if sclera.get(index).copied().unwrap_or(0.0) < 0.5 {
            continue;
        }
        let Some(rgb) = triple(frame, index) else {
            continue;
        };
        if luma(rgb) >= SPECULAR_FLOOR {
            continue;
        }
        sclera_samples += 1;
        for channel in 0..3 {
            if let (Some(target), Some(source)) = (sum.get_mut(channel), rgb.get(channel)) {
                *target += f64::from(*source);
            }
        }
    }
    let sclera_uv = if sclera_samples == 0 {
        [0.0, 0.0]
    } else {
        linear_srgb_to_uv([
            (sum[0] / f64::from(sclera_samples)) as f32,
            (sum[1] / f64::from(sclera_samples)) as f32,
            (sum[2] / f64::from(sclera_samples)) as f32,
        ])
    };

    // --- the iris ----------------------------------------------------------------------------
    let mut luminance = vec![0.0f32; pixels];
    for index in 0..pixels {
        if let (Some(slot), Some(rgb)) = (luminance.get_mut(index), triple(frame, index)) {
            *slot = luma(rgb);
        }
    }

    // The catchlight has to be excluded from the *blur* as well as from the sum, and this is the
    // whole of the difficulty. Skipping specular samples when accumulating the residual is not
    // enough: a blur spreads a blown pixel across its whole radius, so every neighbour of the
    // catchlight acquires an enormous residual and a flat iris reads as full of detail. So the
    // specular samples are replaced by the region's own non-specular median *before* the blur,
    // and the residual is taken against that. The unit test at the bottom of this file is what
    // found it.
    let mut plain: Vec<f32> = Vec::new();
    for index in 0..pixels {
        if iris.get(index).copied().unwrap_or(0.0) < 0.5 {
            continue;
        }
        let value = luminance.get(index).copied().unwrap_or(0.0);
        if value < SPECULAR_FLOOR {
            plain.push(value);
        }
    }
    plain.sort_by(f32::total_cmp);
    let median = plain.get(plain.len() / 2).copied().unwrap_or(0.0);

    // Everything that is not plain iris is replaced by the iris's own median before the blur:
    // the catchlight, and **also every sample outside the iris**. The second half matters as much
    // as the first. An iris is a dozen pixels across and the sclera beside it is three times its
    // luminance, so a blur that reaches outside the region reports an enormous residual at every
    // iris edge, and a completely flat iris reads as full of detail. Same rule as
    // `hair::background_estimate` and `clothing::robust_local`: a local estimate must be computed
    // from the region it describes.
    let mut masked = vec![median; pixels];
    for index in 0..pixels {
        if iris.get(index).copied().unwrap_or(0.0) < 0.5 {
            continue;
        }
        let value = luminance.get(index).copied().unwrap_or(0.0);
        if let Some(slot) = masked.get_mut(index) {
            *slot = if value >= SPECULAR_FLOOR {
                median
            } else {
                value
            };
        }
    }

    let radius =
        ((frame.width.min(frame.height) as f32 * IRIS_DETAIL_FRAC / 8.0).round() as usize).max(1);
    let smoothed = aura_render::bands::blur(&masked, frame.width, frame.height, radius);

    let mut residual = 0.0f64;
    let mut mean = 0.0f64;
    let mut iris_samples = 0u32;
    let mut iris_peak = 0.0f32;
    for index in 0..pixels {
        if iris.get(index).copied().unwrap_or(0.0) < 0.5 {
            continue;
        }
        let value = luminance.get(index).copied().unwrap_or(0.0);
        iris_peak = iris_peak.max(value);
        if value >= SPECULAR_FLOOR {
            continue;
        }
        let base = smoothed.get(index).copied().unwrap_or(value);
        residual += f64::from((value - base).abs());
        mean += f64::from(value);
        iris_samples += 1;
    }
    let iris_detail = if iris_samples == 0 || mean <= f64::EPSILON {
        0.0
    } else {
        (residual / mean) as f32
    };

    EyeReading {
        sclera_uv,
        sclera_samples,
        iris_detail,
        iris_samples,
        iris_peak,
    }
}

/// Decide what may be done to one pair of eyes.
///
/// `neutral` is phase 15's illuminant in `u'v'`; `None` skips the sclera half, because the
/// redness excess is measured against it. `strength` is the frame's own scaling.
///
/// `None` when neither half has enough to measure.
#[must_use]
pub fn solve(
    reading: &EyeReading,
    neutral: Option<[f32; 2]>,
    locus: ColourLocus,
    strength: f32,
) -> Option<EyeDecision> {
    let strength = strength.clamp(0.0, 1.0);
    if strength <= 0.0 {
        return None;
    }
    if reading.sclera_samples < MIN_SCLERA_SAMPLES && reading.iris_samples < MIN_IRIS_SAMPLES {
        return None;
    }

    let mut capped = false;

    // --- redness -----------------------------------------------------------------------------
    let (redness, sclera) = match neutral {
        Some(white) if reading.sclera_samples >= MIN_SCLERA_SAMPLES => {
            let du = reading.sclera_uv[0] - white[0];
            let dv = reading.sclera_uv[1] - white[1];
            let excess = locus.excess(du, dv);
            if excess <= f32::EPSILON {
                (0.0, 0.0)
            } else {
                let share = MAX_SCLERA * strength;
                if share >= MAX_SCLERA - 1e-6 {
                    capped = true;
                }
                (excess, share.clamp(0.0, MAX_SCLERA))
            }
        }
        _ => (0.0, 0.0),
    };

    // --- iris clarity ------------------------------------------------------------------------
    let clarity = if reading.iris_samples < MIN_IRIS_SAMPLES {
        0.0
    } else {
        let deficit =
            ((EXPECTED_IRIS_DETAIL - reading.iris_detail) / EXPECTED_IRIS_DETAIL).clamp(0.0, 1.0);
        if deficit < MIN_IRIS_DEFICIT {
            0.0
        } else {
            let wanted = deficit * MAX_IRIS_CLARITY * strength;
            if wanted >= MAX_IRIS_CLARITY - 1e-6 {
                capped = true;
            }
            wanted.clamp(0.0, MAX_IRIS_CLARITY)
        }
    };

    Some(EyeDecision {
        sclera,
        iris_clarity: clarity,
        capped,
        redness,
        iris_detail: reading.iris_detail,
        samples: reading.sclera_samples + reading.iris_samples,
    })
}

fn triple(frame: &Frame, index: usize) -> Option<[f32; 3]> {
    let slot = index * 3;
    frame.rgb.get(slot..slot + 3).map(|rgb| {
        [
            rgb.first().copied().unwrap_or(0.0),
            rgb.get(1).copied().unwrap_or(0.0),
            rgb.get(2).copied().unwrap_or(0.0),
        ]
    })
}

fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    const LOCUS: ColourLocus = ColourLocus {
        du: 0.0,
        dv: 0.0,
        radius: 0.012,
    };

    const NEUTRAL: [f32; 2] = [0.1978, 0.4683];

    fn reading(sclera_uv: [f32; 2], iris_detail: f32) -> EyeReading {
        EyeReading {
            sclera_uv,
            sclera_samples: 400,
            iris_detail,
            iris_samples: 400,
            iris_peak: 0.95,
        }
    }

    #[test]
    fn a_red_sclera_is_reduced_inside_the_cap() {
        let uv = [NEUTRAL[0] + 0.040, NEUTRAL[1] + 0.010];
        let decision = solve(&reading(uv, 0.09), Some(NEUTRAL), LOCUS, 1.0).expect("a decision");
        assert!(decision.sclera > 0.0);
        assert!(decision.sclera <= MAX_SCLERA + 1e-6);
        assert!(decision.redness > 0.0);
    }

    #[test]
    fn a_sclera_already_inside_the_locus_is_left_alone() {
        let uv = [NEUTRAL[0] + 0.005, NEUTRAL[1] + 0.004];
        let decision = solve(&reading(uv, 0.09), Some(NEUTRAL), LOCUS, 1.0).expect("a decision");
        assert_eq!(decision.sclera, 0.0);
        assert_eq!(decision.redness, 0.0);
    }

    #[test]
    fn a_flat_iris_gets_clarity_and_a_detailed_one_does_not() {
        let flat = solve(&reading(NEUTRAL, 0.02), Some(NEUTRAL), LOCUS, 1.0).expect("a decision");
        assert!(flat.iris_clarity > 0.0);
        assert!(flat.iris_clarity <= MAX_IRIS_CLARITY + 1e-6);

        let detailed =
            solve(&reading(NEUTRAL, 0.20), Some(NEUTRAL), LOCUS, 1.0).expect("a decision");
        assert_eq!(detailed.iris_clarity, 0.0);
    }

    #[test]
    fn with_no_illuminant_the_sclera_half_does_nothing_and_the_iris_half_still_runs() {
        let decision = solve(&reading([0.24, 0.49], 0.02), None, LOCUS, 1.0).expect("a decision");
        assert_eq!(decision.sclera, 0.0);
        assert!(decision.iris_clarity > 0.0);
    }

    #[test]
    fn too_few_samples_on_both_halves_is_no_decision() {
        let mut sparse = reading([0.24, 0.49], 0.02);
        sparse.sclera_samples = 2;
        sparse.iris_samples = 2;
        assert!(solve(&sparse, Some(NEUTRAL), LOCUS, 1.0).is_none());
    }

    #[test]
    fn a_catchlight_is_excluded_from_the_iris_detail_statistic() {
        // Two frames identical but for one blown pixel inside the iris. The detail statistic
        // must not move, because a catchlight is not iris structure.
        let (width, height) = (16usize, 16usize);
        let mut rgb = vec![0.35f32; width * height * 3];
        let iris = vec![1.0f32; width * height];
        let plain = measure(
            &Frame {
                rgb: rgb.clone(),
                width,
                height,
            },
            &[],
            &iris,
        );
        for channel in 0..3 {
            if let Some(slot) = rgb.get_mut((8 * width + 8) * 3 + channel) {
                *slot = 4.0;
            }
        }
        let with_catchlight = measure(&Frame { rgb, width, height }, &[], &iris);
        assert!(
            with_catchlight.iris_peak > plain.iris_peak,
            "the peak should see the catchlight"
        );
        assert!(
            (with_catchlight.iris_detail - plain.iris_detail).abs() < 0.02,
            "the catchlight moved the detail statistic: {} vs {}",
            with_catchlight.iris_detail,
            plain.iris_detail
        );
    }
}
