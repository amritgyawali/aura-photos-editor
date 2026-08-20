//! PHASE-17. Shifting phase 16's grade toward one photographer's own, before the guards.
//!
//! This module is four additions and one curve shift, and its whole importance is **where it is
//! called from**: `analyse` applies it to the solved grade and then runs the clipping guard and
//! the skin guard on the result. ADR-0035 decision 1, and phase 16's own exit report wrote the
//! rule down before this phase existed:
//!
//! > The skin guard runs last, and it runs after phase 17's shift too. A personal style that
//! > moved somebody's skin would be a personal style the guard withdraws, and that ordering is
//! > not negotiable by any later phase.
//!
//! ## Why there is no attenuation here
//!
//! There is no factor in this module that scales a style down before the guard sees it, and
//! that is deliberate. Phase 16's guard is a **post-condition measured on pixels**; a factor
//! applied here would be a guess about what the guard is going to say, and a guess that is
//! usually right is worse than no guess at all, because it removes the cases the guard would
//! have caught while leaving the promise unchanged.
//!
//! The one thing that does happen before the guard is the bound `StyleDelta::clamped` already
//! applied when the delta was constructed: the skin band is held to `SKIN_BAND_FRACTION` of the
//! rest. That is a bound on what a style may *ask for* and not a promise about a pixel, and the
//! two are kept separate on purpose.
//!
//! ## The curve
//!
//! `CurveShift::applied_to` lives in `aura-core` rather than here, because phase 16 solves the
//! curve and phase 17 shifts it and one addition written in two crates is one addition that
//! eventually disagrees with itself. It also cannot produce a non-monotone curve: the shifted
//! points go through `ToneCurve::new`, and a refusal returns the original.

use aura_core::contract::style::{StyleAdvice, StyleCode};

use crate::colour::hsl::Solved as HslSolved;
use crate::colour::skin_guard::Grade;
use crate::colour::tone::ToneParams;

/// One grade with a photographer's own lean added to it.
///
/// Every field is a sum, and every sum is bounded on both sides: the delta was clamped when it
/// was built and the parameters are clamped to the recipe's range here. Nothing is scaled, and
/// nothing pretends to know what the guards will decide.
#[must_use]
pub fn apply(grade: &Grade, advice: &StyleAdvice) -> Grade {
    let delta = &advice.delta;
    if delta.is_neutral() {
        return grade.clone();
    }
    let param = |value: f32| value.clamp(-100.0, 100.0);
    Grade {
        tone: ToneParams {
            contrast: param(grade.tone.contrast + delta.contrast),
            highlights: param(grade.tone.highlights + delta.highlights),
            shadows: param(grade.tone.shadows + delta.shadows),
            whites: param(grade.tone.whites + delta.whites),
            blacks: param(grade.tone.blacks + delta.blacks),
        },
        curve: delta.curve_shift.applied_to(&grade.curve),
        colour: HslSolved {
            hsl: delta.applied_to_hsl(&grade.colour.hsl),
            vibrance: param(grade.colour.vibrance + delta.vibrance),
            saturation: param(grade.colour.saturation + delta.saturation),
            reasons: grade.colour.reasons.clone(),
        },
    }
}

/// Which style codes describe what happened to one frame's grade.
///
/// Returned rather than pushed, because the reasons this phase adds to a *colour* decision go
/// through phase 16's own `ColourCode` vocabulary on the way to the catalog - migration 16's
/// `reason_count` CHECK and its reason column both speak that vocabulary - while the panel and
/// the ledger want the style codes. Both are true and they belong in different places.
#[must_use]
pub fn codes(advice: &StyleAdvice, guard_intervened: bool, clip_resolved: bool) -> Vec<StyleCode> {
    let mut out: Vec<StyleCode> = advice.reasons.iter().map(|reason| reason.code).collect();
    if advice.delta.is_neutral() {
        return out;
    }
    if guard_intervened {
        out.push(StyleCode::SkinGuardOverrode);
    }
    if clip_resolved {
        out.push(StyleCode::ClipGuardOverrode);
    }
    out
}

/// How far a style moved one grade, in the same units as phase 16's subtlety metric.
///
/// Reported so a panel can say "your style added this much on top of what AURA decided", which
/// is the number a photographer looks for when a frame does not look like theirs. It is a
/// difference of magnitudes rather than the magnitude of a difference, deliberately: the
/// question is how much *more* processed the frame became, and a style that pulled contrast back
/// toward zero should read as a negative number rather than as a large positive one.
#[must_use]
pub fn added_magnitude(before: &Grade, after: &Grade) -> f32 {
    crate::colour::clip_guard::subtlety(after) - crate::colour::clip_guard::subtlety(before)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::colour::{CurvePoint, HslAdjustments, HslBand, HslShift, ToneCurve};
    use aura_core::contract::style::{
        CurveShift, FallbackLevel, StyleBucket, StyleDelta, MAX_HSL_DELTA, SKIN_BAND_FRACTION,
    };

    fn grade() -> Grade {
        Grade {
            tone: ToneParams {
                contrast: 10.0,
                highlights: -20.0,
                shadows: 15.0,
                whites: 0.0,
                blacks: -5.0,
            },
            curve: ToneCurve::identity(),
            colour: HslSolved {
                hsl: HslAdjustments::default(),
                vibrance: 6.0,
                saturation: 0.0,
                reasons: Vec::new(),
            },
        }
    }

    fn advice(delta: StyleDelta) -> StyleAdvice {
        StyleAdvice {
            profile: None,
            bucket: StyleBucket::default(),
            delta,
            level: FallbackLevel::Bucket,
            confidence: 0.8,
            reasons: Vec::new(),
        }
    }

    #[test]
    fn a_neutral_style_returns_the_grade_untouched() {
        let base = grade();
        assert_eq!(apply(&base, &advice(StyleDelta::neutral())), base);
    }

    #[test]
    fn every_tone_parameter_moves_by_its_own_delta() {
        let out = apply(
            &grade(),
            &advice(StyleDelta {
                contrast: 8.0,
                shadows: -5.0,
                vibrance: 4.0,
                ..StyleDelta::neutral()
            }),
        );
        assert!((out.tone.contrast - 18.0).abs() < 1e-4);
        assert!((out.tone.shadows - 10.0).abs() < 1e-4);
        assert!((out.colour.vibrance - 10.0).abs() < 1e-4);
    }

    #[test]
    fn a_styled_parameter_is_clamped_to_the_recipes_own_range() {
        let mut base = grade();
        base.tone.contrast = 96.0;
        let out = apply(
            &base,
            &advice(StyleDelta {
                contrast: 30.0,
                ..StyleDelta::neutral()
            }),
        );
        assert!((out.tone.contrast - 100.0).abs() < 1e-4);
    }

    #[test]
    fn the_skin_band_arrives_already_held_to_a_fraction_of_the_rest() {
        // The bound is applied by `StyleDelta::clamped` on construction, so a delta that asks
        // for the full HSL range on the orange band cannot even reach this module with it.
        let mut hsl = HslAdjustments::default();
        hsl.set(
            HslBand::Orange,
            HslShift {
                h: MAX_HSL_DELTA,
                s: MAX_HSL_DELTA,
                l: 0.0,
            },
        );
        hsl.set(
            HslBand::Green,
            HslShift {
                h: MAX_HSL_DELTA,
                s: 0.0,
                l: 0.0,
            },
        );
        let delta = StyleDelta {
            hsl,
            ..StyleDelta::neutral()
        }
        .clamped();
        let out = apply(&grade(), &advice(delta));
        let orange = out.colour.hsl.get(HslBand::Orange);
        let green = out.colour.hsl.get(HslBand::Green);
        assert!(
            (orange.h - MAX_HSL_DELTA * SKIN_BAND_FRACTION).abs() < 1e-3,
            "orange {orange:?}"
        );
        assert!((green.h - MAX_HSL_DELTA).abs() < 1e-3, "green {green:?}");
    }

    #[test]
    fn a_curve_shift_produces_a_curve_and_never_a_backwards_one() {
        let Ok(fitted) = ToneCurve::new(vec![
            CurvePoint::new(0, 0),
            CurvePoint::new(64, 60),
            CurvePoint::new(192, 200),
            CurvePoint::new(255, 255),
        ]) else {
            unreachable!("a monotone set")
        };
        let mut base = grade();
        base.curve = fitted;
        let out = apply(
            &base,
            &advice(StyleDelta {
                curve_shift: CurveShift::from_array([0.0, -20.0, 0.0, 20.0, 0.0]),
                ..StyleDelta::neutral()
            }),
        );
        let points = out.curve.points();
        for pair in points.windows(2) {
            let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            assert!(b.y >= a.y, "{points:?}");
        }
    }

    #[test]
    fn the_added_magnitude_is_negative_when_a_style_pulls_a_grade_back() {
        let base = grade();
        let gentler = apply(
            &base,
            &advice(StyleDelta {
                contrast: -8.0,
                shadows: -12.0,
                vibrance: -6.0,
                ..StyleDelta::neutral()
            }),
        );
        assert!(added_magnitude(&base, &gentler) < 0.0);
    }

    #[test]
    fn the_guard_codes_only_appear_when_a_guard_actually_acted() {
        let with_style = advice(StyleDelta {
            contrast: 5.0,
            ..StyleDelta::neutral()
        });
        assert!(codes(&with_style, false, false).is_empty());
        assert_eq!(
            codes(&with_style, true, false),
            vec![StyleCode::SkinGuardOverrode]
        );
        // A neutral style cannot be the reason a guard fired, so it claims neither code.
        assert!(codes(&advice(StyleDelta::neutral()), true, true).is_empty());
    }
}
