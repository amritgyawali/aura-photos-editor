//! PHASE-17. Shifting phase 15's answer toward one photographer's own, and re-running both of
//! phase 15's guarantees on the result.
//!
//! `aura-brain-photo` does not depend on `aura-style`. What arrives here is a
//! [`StyleAdvice`] - a frozen shape from `aura-core` - which the caller resolved through
//! `StyleService`, and everything in this module is arithmetic on it. The dependency runs
//! through the trait and not through a crate, which is phase 10's pattern for the same reason:
//! `aura-brain-photo` and `aura-style` depend on each other in **neither** direction.
//!
//! ## The rule this module exists to keep
//!
//! **The shift happens before the guarantees, not after.** ADR-0035 decision 1. A style applied
//! after phase 15's clipping bound and skin-locus constraint is a set of values nobody checked,
//! delivered under a sentence that says somebody did - and that sentence is section 6.3's, the
//! hardest promise this product makes.
//!
//! So this module does not simply add a number to an exposure. It shifts the *target*, and then
//! re-runs the same two bounds `exposure::solve` ran:
//!
//! * **Highlights.** `FrameStats::clipping_added` on the styled exposure, against the scene's
//!   own `clip_tolerance`. A photographer whose taste is a stop brighter does not get a stop
//!   brighter on the frame with a sparkler in it.
//! * **Skin.** `Constraints::rejects` on the styled illuminant. A photographer whose taste is
//!   eight hundred kelvin warmer does not get eight hundred kelvin on the frame where that
//!   would make somebody's skin implausible - the correction walks back until everybody in the
//!   frame is plausible again, which is the same linear scan phase 15's own solve performs and
//!   for the same reason: with two people whose loci differ, the satisfying set is not an
//!   interval.
//!
//! Both re-runs can withdraw the whole shift, and both say so with a code.

use aura_core::contract::style::{StyleAdvice, StyleCode};
use aura_core::contract::tone::{ToneCode, EXPOSURE_RANGE, KELVIN_RANGE, TINT_RANGE};
use aura_raw::colour::illuminant::{cct_to_uv, duv};

use crate::tone::stats::FrameStats;
use crate::tone::store::codec::{self, Explained};
use crate::tone::targets::SceneTarget;
use crate::tone::wb::Constraints;

/// How many steps the white-balance walk-back takes.
///
/// Twenty, matching `solve::CORRECTION_STEPS`. A linear scan rather than a bisection, for
/// phase 15's own reason: with two people whose loci differ the satisfying set is not an
/// interval, so a bisection can land in a gap and report it as a solution.
pub const WALK_BACK_STEPS: usize = 20;

/// The recipe tint units one unit of `duv` is worth.
///
/// `illuminant::describe` divides `duv` by this to produce a tint, so multiplying by it is the
/// inverse. Written here as a named constant rather than as a literal at the one call site,
/// because a second copy of `0.0045` is a second copy that drifts from the first.
pub const DUV_PER_TINT: f32 = 0.004_5;

/// What phase 15 decided, before any style was applied to it.
///
/// Three numbers rather than three arguments, because they travel together everywhere and
/// because a signature with three consecutive `f32`s is a signature somebody eventually calls
/// with the tint where the temperature goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solved {
    /// The exposure phase 15 settled on, in stops.
    pub exposure_ev: f32,
    /// The colour temperature it settled on, in kelvin.
    pub temperature_k: f32,
    /// The tint it settled on.
    pub tint: f32,
}

/// What the style did to phase 15's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    /// The exposure after the shift and after the clipping bound.
    pub exposure_ev: f32,
    /// The colour temperature after the shift and after the locus constraint.
    pub temperature_k: f32,
    /// The tint after the same.
    pub tint: f32,
    /// How much new highlight clipping the styled exposure adds.
    pub clipping_added: f32,
    /// How much of the requested white-balance shift survived, `0..1`.
    pub wb_applied: f32,
    /// Why, as phase 15's own explained reasons plus this phase's codes.
    pub reasons: Vec<Explained>,
}

/// Shift one frame's exposure and white balance toward a photographer's own look.
///
/// `solved_ev`, `solved_k` and `solved_tint` are what phase 15 decided; the return value is
/// what phase 15 would have decided if this photographer's taste had been one of its inputs.
///
/// The advice may be neutral, in which case every field comes back unchanged and the only
/// reason is the advice's own - which is a complete answer rather than an absence, and is what
/// `StyleAdvice::none` produces for a project with no profile.
#[must_use]
pub fn apply(
    advice: &StyleAdvice,
    solved: Solved,
    stats: &FrameStats,
    target: SceneTarget,
    constraints: &Constraints,
    backlit: bool,
) -> Applied {
    let (solved_ev, solved_k, solved_tint) =
        (solved.exposure_ev, solved.temperature_k, solved.tint);
    let delta = &advice.delta;
    let mut reasons: Vec<Explained> = Vec::new();

    // -- exposure ------------------------------------------------------
    let wanted = (solved_ev + delta.exposure).clamp(EXPOSURE_RANGE.0, EXPOSURE_RANGE.1);
    let allowance = if backlit {
        target.backlight_allowance.max(target.clip_tolerance)
    } else {
        target.clip_tolerance
    };
    let mut exposure_ev = wanted;
    if wanted > solved_ev && stats.clipping_added(wanted) > allowance {
        // Walk back to the largest styled exposure the scene's own clipping tolerance accepts,
        // never below what phase 15 already decided - the baseline is a floor here, because
        // phase 15's answer already met this bound and a style must not be able to make a frame
        // worse than the answer it is shifting.
        let mut best = solved_ev;
        let mut step = wanted - solved_ev;
        for _ in 0..16 {
            step /= 2.0;
            let candidate = best + step;
            if stats.clipping_added(candidate) <= allowance {
                best = candidate;
            }
        }
        exposure_ev = best;
        reasons.push(codec::plain(ToneCode::ExposureHeldForHighlights, -0.03));
    }

    // -- white balance -------------------------------------------------
    let wanted_k = (solved_k + delta.temperature_k).clamp(KELVIN_RANGE.0, KELVIN_RANGE.1);
    let wanted_tint = (solved_tint + delta.tint).clamp(TINT_RANGE.0, TINT_RANGE.1);
    let mut applied = 1.0_f32;
    let (mut temperature_k, mut tint) = (wanted_k, wanted_tint);

    if !constraints.is_empty() && (delta.temperature_k.abs() > 1.0 || delta.tint.abs() > 0.1) {
        applied = 0.0;
        for step in (0..=WALK_BACK_STEPS).rev() {
            let t = step as f32 / WALK_BACK_STEPS as f32;
            let candidate_k = solved_k + (wanted_k - solved_k) * t;
            let candidate_tint = solved_tint + (wanted_tint - solved_tint) * t;
            if !constraints.rejects(uv_of(candidate_k, candidate_tint)) {
                temperature_k = candidate_k;
                tint = candidate_tint;
                applied = t;
                break;
            }
        }
        if applied < 1.0 {
            // At `t = 0` the candidate is phase 15's own answer, which the solve already
            // checked - so the loop always terminates with something plausible, and a zero here
            // means the whole shift was refused rather than that no answer was found.
            temperature_k = solved_k + (wanted_k - solved_k) * applied;
            tint = solved_tint + (wanted_tint - solved_tint) * applied;
            // Phase 15's own code for "a person's skin vetoed a light", re-used rather than
            // duplicated: the constraint that fired is the same constraint, and a second code
            // saying the same thing would be a second thing for a runbook to keep in step.
            reasons.push(codec::plain(ToneCode::SkinLocusConstrained, -0.05));
        }
    }

    Applied {
        exposure_ev,
        temperature_k,
        tint,
        clipping_added: stats.clipping_added(exposure_ev),
        wb_applied: applied,
        reasons,
    }
}

/// Which style code describes what happened to the white balance.
///
/// Split out so the colour half and the panel can both name the same thing, and so the
/// mapping from "how much survived" to "what to say" is in one place rather than at every
/// call site.
#[must_use]
pub fn wb_code(applied: f32) -> Option<StyleCode> {
    if applied >= 0.999 {
        None
    } else {
        Some(StyleCode::LocusConstraintOverrode)
    }
}

/// The `u'v'` chromaticity one colour temperature and tint name.
///
/// The inverse of `illuminant::describe`, which reports `cct_from_uv` and `duv / DUV_PER_TINT`.
/// The locus normal is found numerically rather than analytically: the Planckian locus has no
/// closed form in `u'v'` that is worth writing twice, and a two-point difference over a one
/// percent temperature step is accurate to far more than the tint slider can express.
///
/// The sign of the normal is **measured rather than assumed**, by evaluating `duv` a short way
/// along it. Getting it backwards would put every styled magenta shift into green, which is a
/// defect that looks exactly like a bad style profile.
#[must_use]
pub fn uv_of(kelvin: f32, tint: f32) -> [f32; 2] {
    let kelvin = kelvin.clamp(KELVIN_RANGE.0, KELVIN_RANGE.1);
    let base = cct_to_uv(kelvin);
    if tint.abs() < f32::EPSILON {
        return base;
    }
    let ahead = cct_to_uv((kelvin * 1.01).min(KELVIN_RANGE.1));
    let tangent = [ahead[0] - base[0], ahead[1] - base[1]];
    let length = (tangent[0] * tangent[0] + tangent[1] * tangent[1]).sqrt();
    if length < 1e-9 {
        return base;
    }
    let mut normal = [-tangent[1] / length, tangent[0] / length];
    let probe = [base[0] + normal[0] * 0.01, base[1] + normal[1] * 0.01];
    if duv(probe) < 0.0 {
        normal = [-normal[0], -normal[1]];
    }
    let offset = tint * DUV_PER_TINT;
    [base[0] + normal[0] * offset, base[1] + normal[1] * offset]
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::style::{StyleBucket, StyleDelta};
    use aura_core::SceneId;

    use crate::tone::fixtures;
    use crate::tone::targets::TargetTable;

    fn advice(delta: StyleDelta) -> StyleAdvice {
        StyleAdvice {
            profile: None,
            bucket: StyleBucket::default(),
            delta,
            level: aura_core::contract::style::FallbackLevel::Bucket,
            confidence: 0.8,
            reasons: Vec::new(),
        }
    }

    fn measured(frame: &fixtures::Frame) -> FrameStats {
        let plane =
            aura_vision::embed::descriptors::luma_plane(&frame.rgb, frame.width, frame.height);
        FrameStats::measure(&frame.rgb, frame.width, frame.height, &plane)
    }

    fn target() -> SceneTarget {
        let Ok(table) = TargetTable::embedded() else {
            unreachable!("the embedded target table ships with the binary")
        };
        table.get(SceneId::Unknown)
    }

    #[test]
    fn the_tint_conversion_round_trips_through_the_describe_it_inverts() {
        // Not exactly, and the reason is geometric rather than a defect. `duv` reports the
        // signed distance to the NEAREST point on the Planckian locus, and for a point pushed
        // off the locus along the local normal, the nearest point is not the temperature it was
        // pushed from - the locus curves. The residual grows with the tint and with the
        // temperature and is under one tint unit across everything a wedding contains, which is
        // finer than the slider's own granularity and far finer than anything visible.
        for (kelvin, tint) in [(5500.0_f32, 0.0_f32), (3200.0, 6.0), (7400.0, -8.0)] {
            let uv = uv_of(kelvin, tint);
            let back = duv(uv) / DUV_PER_TINT;
            assert!(
                (back - tint).abs() < 1.0,
                "{kelvin} K tint {tint} came back as {back}"
            );
        }
    }

    #[test]
    fn a_neutral_advice_changes_nothing() {
        let frame = fixtures::daylight_ceremony(2);
        let stats = measured(&frame);
        let applied = apply(
            &advice(StyleDelta::neutral()),
            Solved {
                exposure_ev: 0.2,
                temperature_k: 5200.0,
                tint: 3.0,
            },
            &stats,
            target(),
            &Constraints::default(),
            false,
        );
        assert!((applied.exposure_ev - 0.2).abs() < 1e-4);
        assert!((applied.temperature_k - 5200.0).abs() < 1e-3);
        assert!((applied.tint - 3.0).abs() < 1e-4);
        assert!(applied.reasons.is_empty());
    }

    #[test]
    fn a_style_that_asks_for_more_exposure_gets_it_when_the_frame_can_carry_it() {
        let frame = fixtures::tungsten_reception(2);
        let stats = measured(&frame);
        let applied = apply(
            &advice(StyleDelta {
                exposure: 0.4,
                ..StyleDelta::neutral()
            }),
            Solved {
                exposure_ev: 0.0,
                temperature_k: 5500.0,
                tint: 0.0,
            },
            &stats,
            target(),
            &Constraints::default(),
            false,
        );
        assert!(applied.exposure_ev > 0.3, "{}", applied.exposure_ev);
    }

    #[test]
    fn the_styled_exposure_never_goes_below_what_phase_fifteen_already_decided() {
        // Whatever the clipping bound does to a style's request, it may never take a frame
        // below the answer phase 15 had already checked: a style shifts an answer and must not
        // be able to make it worse.
        let frame = fixtures::backlit_exit(4);
        let stats = measured(&frame);
        let applied = apply(
            &advice(StyleDelta {
                exposure: 0.6,
                ..StyleDelta::neutral()
            }),
            Solved {
                exposure_ev: 0.15,
                temperature_k: 5500.0,
                tint: 0.0,
            },
            &stats,
            target(),
            &Constraints::default(),
            false,
        );
        assert!(
            applied.exposure_ev >= 0.15 - 1e-4,
            "{}",
            applied.exposure_ev
        );
    }

    #[test]
    fn a_style_with_no_people_in_the_frame_is_not_walked_back() {
        let frame = fixtures::faceless_details();
        let stats = measured(&frame);
        let applied = apply(
            &advice(StyleDelta {
                temperature_k: 500.0,
                ..StyleDelta::neutral()
            }),
            Solved {
                exposure_ev: 0.0,
                temperature_k: 5000.0,
                tint: 0.0,
            },
            &stats,
            target(),
            &Constraints::default(),
            false,
        );
        assert!((applied.temperature_k - 5500.0).abs() < 1.0);
        assert!((applied.wb_applied - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_white_balance_code_only_fires_when_something_was_withdrawn() {
        assert_eq!(wb_code(1.0), None);
        assert_eq!(wb_code(0.4), Some(StyleCode::LocusConstraintOverrode));
    }
}
