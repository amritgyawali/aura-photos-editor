//! The luminosity mask: why a lifted face does not glow.
//!
//! PHASE-19 section 6.1, first bullet, and the single most important idea in the phase:
//!
//! > Work in linear light and modulate by a luminosity mask derived from the face's own
//! > tonal distribution, so shadows lift more than mid-tones and highlights barely move -
//! > this is what prevents the flat 'glowing face' look.
//!
//! A flat exposure lift inside a face mask moves every tone by the same number of stops. The
//! shadow side of the nose and the lit side of the forehead rise together, the ratio between
//! them is preserved, and the result is a face with the same contrast and more brightness -
//! which is exactly what "glowing" means. What a retoucher does instead is lift the *shadow*
//! side and leave the lit side where it is, which reduces the ratio and reads as better
//! lighting rather than as more light.
//!
//! ## How that becomes three recipe fields
//!
//! Schema v1's [`aura_recipe::MaskParams`] carries `exposure`, `shadows` and `highlights`
//! inside a mask, and `aura-render` applies them at three different stages. So the lift is
//! *split*: the flat part becomes exposure, the tone-dependent part becomes shadows, and a
//! small negative highlight term holds the bright end down. [`split`] is the only place that
//! division happens.
//!
//! Doing it any other way was tried first and is worth recording: folding the whole lift into
//! `exposure` and then adding a negative `highlights` to compensate produces the right mean
//! and the wrong *distribution* - the highlights recovery stage is a shoulder, not an inverse
//! exposure, so a face lifted 0.8 EV and pulled back with -40 highlights has a compressed lit
//! side rather than an unmoved one, and compression on skin is visible at 100 %.

use aura_core::contract::local::FaceLightDelta;

/// How many `shadows` units one stop of shadow-region lift is worth.
///
/// `aura-render`'s tone stage applies `shadows` as a gain concentrated below the mid-point;
/// at the region's own mean luminance, sixty units moves it by about a stop. Measured against
/// the reference path rather than assumed, and `tests/eval/local_eval.rs` re-measures it: a
/// renderer change that moved this constant would otherwise silently change how far every
/// face in the product gets lifted.
pub const SHADOWS_PER_EV: f32 = 60.0;

/// How many `highlights` units of restraint one stop of lift buys.
///
/// Negative in use. Eighteen per stop is enough to keep the lit side of a face from moving
/// visibly and small enough not to flatten it - the failure mode at forty is a forehead that
/// has stopped having a specular fall-off at all.
pub const HIGHLIGHTS_PER_EV: f32 = 18.0;

/// The luminance the split treats as the middle of a face.
///
/// Faces are not mid-grey. A well-exposed face sits above it - phase 15's bands are between
/// 0.30 and 0.60 - so a split pivoted at 0.5 would treat a correctly lit ceremony face as a
/// shadow and put the whole lift into the shadows slider.
pub const FACE_PIVOT: f32 = 0.48;

/// How much of a lift goes into the shadows rather than into the exposure.
///
/// One at black, zero at and above the pivot, smooth in between. Smooth rather than linear
/// because the derivative is what a photographer sees: two faces a hundredth of a unit apart
/// in luminance that get noticeably different treatments look like a bug even when both are
/// defensible.
#[must_use]
pub fn shadow_share(mean_luma: f32) -> f32 {
    let x = ((FACE_PIVOT - mean_luma) / FACE_PIVOT).clamp(0.0, 1.0);
    // Smoothstep. At the pivot the share is zero and its slope is zero, so the transition
    // from "this face is fine" to "this face is dark" has no corner in it.
    x * x * (3.0 - 2.0 * x)
}

/// Split a total lift into the three fields a mask carries.
///
/// `total_ev` is the change the solver wants the face's *mean* to make. Positive lifts.
///
/// A pull-down is not split at all: it is all exposure, with no shadow term and no highlight
/// restraint. Pulling the shadows of an over-bright face down would deepen it as well as
/// darken it, and the reason a face is too bright is almost always that a flash hit it -
/// which is a flat problem with a flat remedy.
#[must_use]
pub fn split(total_ev: f32, mean_luma: f32) -> (f32, i16, i16) {
    if total_ev <= 0.0 {
        return (total_ev, 0, 0);
    }
    let share = shadow_share(mean_luma);
    let shadow_ev = total_ev * share;
    let exposure_ev = total_ev - shadow_ev;
    let shadows = (shadow_ev * SHADOWS_PER_EV).round().clamp(0.0, 100.0) as i16;
    let highlights = (-(total_ev * HIGHLIGHTS_PER_EV)).round().clamp(-100.0, 0.0) as i16;
    (exposure_ev, shadows, highlights)
}

/// The feather a face of this size needs, `0..1`.
///
/// Section 6.1: "feather by face size (a small guest face needs a wider relative feather)".
/// Relative is the operative word - the transition on a guest face forty pixels across has to
/// be a larger *fraction* of the face than the one on a bride's face four hundred pixels
/// across, or it will be two pixels wide and read as a cut-out.
///
/// `side` is the face's shorter side as a fraction of the frame's shorter side.
#[must_use]
pub fn feather_for(side: f32) -> f32 {
    // 0.85 for a tiny face, tapering to 0.30 for a large one. The floor is not zero: a hard
    // edge on a large face against hair is the classic halo, and section 12's first row names
    // it.
    let large = (side / 0.35).clamp(0.0, 1.0);
    0.30f32
        .mul_add(large, 0.85 * (1.0 - large))
        .clamp(0.30, 0.85)
}

/// Fill the three fields of a delta from a solved lift.
///
/// A helper rather than a method on the contract type, because the split is this crate's
/// arithmetic and the contract is a shape.
#[must_use]
pub fn apply_split(mut delta: FaceLightDelta, total_ev: f32) -> FaceLightDelta {
    let (exposure, shadows, highlights) = split(total_ev, delta.luma_before);
    delta.exposure_ev = exposure;
    delta.shadows = shadows;
    delta.highlights = highlights;
    delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dark_face_lifts_mostly_through_the_shadows() {
        let (exposure, shadows, _) = split(0.8, 0.14);
        assert!(
            shadows > 30,
            "a face at 0.14 should lift through its shadows, got {shadows}"
        );
        assert!(
            exposure < 0.2,
            "and should barely move its exposure, got {exposure}"
        );
    }

    #[test]
    fn a_face_already_near_the_band_lifts_flat() {
        let (exposure, shadows, _) = split(0.2, 0.47);
        assert!(shadows <= 2, "no shadow term this close to the pivot");
        assert!((exposure - 0.2).abs() < 0.02);
    }

    #[test]
    fn the_highlight_term_is_never_positive() {
        for luma in [0.05f32, 0.2, 0.4, 0.6, 0.9] {
            let (_, _, highlights) = split(1.0, luma);
            assert!(highlights <= 0, "a lift pushed the highlights up at {luma}");
        }
    }

    #[test]
    fn a_pull_down_is_all_exposure() {
        let (exposure, shadows, highlights) = split(-0.4, 0.72);
        assert!((exposure + 0.4).abs() < 1e-6);
        assert_eq!(shadows, 0, "pulling a bright face down must not deepen it");
        assert_eq!(highlights, 0);
    }

    #[test]
    fn the_share_has_no_corner_at_the_pivot() {
        let below = shadow_share(FACE_PIVOT - 0.001);
        let above = shadow_share(FACE_PIVOT + 0.001);
        assert!(below < 0.001 && above == 0.0, "{below} then {above}");
    }

    #[test]
    fn a_small_face_gets_a_wider_relative_feather() {
        assert!(feather_for(0.03) > feather_for(0.30));
        assert!(feather_for(0.90) >= 0.30, "never a hard edge");
    }
}
