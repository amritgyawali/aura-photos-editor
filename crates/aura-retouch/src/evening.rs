//! Calming blotchy skin without smoothing any of it.
//!
//! PHASE-20 section 2.1: "mid-frequency unevenness (blotches, flush, makeup mismatch, neck/face
//! mismatch) corrected by frequency-selective operations". Section 6.3 is the constraint:
//! whatever this does, the high band comes back untouched.
//!
//! ## Why this can be safe at all
//!
//! Because `aura_render::bands` reconstruction is exact: `low + mid + high` is the input,
//! sample for sample. Scaling `mid` and adding the other two back therefore cannot touch a pore
//! by any value of any parameter - which is a stronger statement than "we chose a small radius",
//! and it is why the texture ratio of an evening-only plan is exactly one rather than close to
//! one.
//!
//! ## What is measured, and against what
//!
//! Unevenness is the mid-band energy over the skin **as a fraction of the luminance of that
//! skin**. Relative rather than absolute, because absolute mid-band energy is a property of how
//! brightly the face is lit and how large it is in the frame - a threshold on it would even out
//! every close-up and no wide, and it would read differently on two people under the same light.
//! The pores never enter the measurement at all: they are high-frequency, and this reads the mid
//! band alone.

use aura_core::contract::retouch::MAX_EVENING_MID;
use aura_render::bands;

use crate::blemish::FaceCrop;

/// The mid-band energy, as a fraction of the luminance of the skin, below which skin is even.
///
/// Four thousandths. Skin with ordinary pores and no blotches sits well below it, because pore
/// texture is high-frequency and lands in a band this measurement does not read; flush across
/// the cheeks, a makeup line at the jaw or a neck two shades from the chin all run above it.
///
/// **Relative to the luminance of the skin, not absolute**, which is the same normalisation
/// phase 15 uses and the reason a threshold tuned on one skin tone does not quietly stop working
/// on another. It is also what makes the number scale-free: a face that fills the frame and one
/// that fills a quarter of it produce the same reading of the same skin.
pub const EVEN_RATIO: f32 = 0.004;

/// The reading at which the operation runs at full strength.
///
/// Twelve thousandths - three times the floor. Past this more evening does not help: what is
/// left is either lighting, which phase 19 owns, or a mark, which [`crate::blemish`] owns.
pub const FULL_RATIO: f32 = 0.012;

/// The smallest number of skin samples worth measuring a ratio over.
pub const MIN_SAMPLES: usize = 256;

/// What the evening decided.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EveningDecision {
    /// How strongly to run, `0..1`.
    pub strength: f32,
    /// The measured unevenness, as a fraction of the luminance of the skin.
    pub ratio: f32,
    /// The share of the mid band this will remove, for the panel.
    ///
    /// Always at most [`MAX_EVENING_MID`], which is a third: past that the modelling of the face
    /// leaves along with the blotches.
    pub mid_removed: f32,
}

/// Decide how much evening one face needs.
///
/// `None` when the skin is already even, when there is not enough of it to measure, or when the
/// caller asked for no strength. Each of those is a withdrawal the plan names.
#[must_use]
pub fn solve(crop: &FaceCrop, strength: f32) -> Option<EveningDecision> {
    if strength <= 0.0 || crop.width == 0 || crop.height == 0 {
        return None;
    }

    let mut luma = Vec::with_capacity(crop.width * crop.height);
    let mut weights = Vec::with_capacity(crop.width * crop.height);
    for index in 0..crop.width * crop.height {
        luma.push(crop.luma_at(index));
        weights.push(crop.skin_at(index));
    }
    if weights.iter().filter(|w| **w > 0.5).count() < MIN_SAMPLES {
        return None;
    }

    let decomposed = bands::separate(&luma, crop.width, crop.height);
    let (_, counted) = decomposed.high_energy_masked(&weights);
    if counted == 0 {
        return None;
    }
    let level = masked_mean(&luma, &weights).max(1e-4);
    let mid = masked_energy(&decomposed.mid, &weights);
    let ratio = mid / level;
    if ratio <= EVEN_RATIO {
        return None;
    }

    let ramp = ((ratio - EVEN_RATIO) / (FULL_RATIO - EVEN_RATIO)).clamp(0.0, 1.0);
    let applied = (ramp * strength).clamp(0.0, 1.0);
    if applied <= 0.0 {
        return None;
    }

    Some(EveningDecision {
        strength: applied,
        ratio,
        mid_removed: applied * MAX_EVENING_MID,
    })
}

/// Mean value over the samples a mask covers.
fn masked_mean(values: &[f32], weights: &[f32]) -> f32 {
    let mut total = 0.0f64;
    let mut weight = 0.0f64;
    for (value, w) in values.iter().zip(weights.iter()) {
        if *w <= 0.0 {
            continue;
        }
        total += f64::from(*value) * f64::from(*w);
        weight += f64::from(*w);
    }
    if weight <= f64::EPSILON {
        return 0.0;
    }
    (total / weight) as f32
}

/// Mean absolute value over the samples a mask covers.
fn masked_energy(values: &[f32], weights: &[f32]) -> f32 {
    let mut total = 0.0f64;
    let mut weight = 0.0f64;
    for (value, w) in values.iter().zip(weights.iter()) {
        if *w <= 0.0 {
            continue;
        }
        total += f64::from(value.abs()) * f64::from(*w);
        weight += f64::from(*w);
    }
    if weight <= f64::EPSILON {
        return 0.0;
    }
    (total / weight) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn a_blotchy_face_is_evened_and_an_even_one_is_not() {
        let blotchy = fixtures::blotchy_face();
        let decision = solve(&blotchy, 1.0).expect("an evening");
        assert!(decision.ratio > EVEN_RATIO);
        assert!(decision.strength > 0.0);
        assert!(decision.mid_removed <= MAX_EVENING_MID + 1e-6);

        let even = fixtures::even_face();
        assert!(solve(&even, 1.0).is_none());
    }

    #[test]
    fn the_share_of_the_mid_band_removed_is_bounded_at_a_third() {
        let blotchy = fixtures::blotchy_face();
        let decision = solve(&blotchy, 1.0).expect("an evening");
        assert!(decision.mid_removed <= MAX_EVENING_MID + 1e-6);
    }

    #[test]
    fn a_crop_with_almost_no_skin_is_not_measured() {
        let mut crop = fixtures::blotchy_face();
        for (index, weight) in crop.skin.iter_mut().enumerate() {
            *weight = if index < 32 { 1.0 } else { 0.0 };
        }
        assert!(solve(&crop, 1.0).is_none());
    }

    #[test]
    fn evening_is_deterministic() {
        let crop = fixtures::blotchy_face();
        assert_eq!(solve(&crop, 0.7), solve(&crop, 0.7));
    }
}
