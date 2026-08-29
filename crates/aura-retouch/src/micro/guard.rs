//! What the plan did to the catchlights, the hairline and the teeth, measured.
//!
//! PHASE-21 section 6.4, and the rule phase 16 wrote for skin colour and phase 20 repeated for
//! skin texture, inherited a third time:
//!
//! > **A guarantee about a pixel is enforced on the pixel.**
//!
//! [`enforce`] applies the plan through `aura_render::micro::apply` - the same code the delivered
//! JPEG goes through - and measures three quantities on the result. A parameter bound and a
//! promise about a photograph are different things, and everything that has ever shipped a
//! glowing eye bounded the parameter.
//!
//! ## Three measurements, three families, three independent withdrawals
//!
//! | Measurement | Held to | Family it holds |
//! |---|---|---|
//! | peak iris luminance, after over before | [`CATCHLIGHT_FLOOR`] | [`OpFamily::Eyes`] |
//! | hair-region edge energy, after over before | [`HAIR_ENERGY_FLOOR`] | [`OpFamily::Hair`] |
//! | *increase* in teeth excursion | [`TEETH_EXCURSION_CEILING`] | [`OpFamily::Teeth`] |
//!
//! All three are **differences against the unedited frame**, and the third one is the one that
//! had to be argued about. An absolute teeth excursion would hold the guard to a result the
//! operator is deliberately not allowed to reach - `MAX_TEETH_YELLOW` removes a third of the
//! excess, so strongly yellow teeth are still outside the locus afterwards and are meant to be -
//! and the family would be withdrawn on exactly the photographs it exists for. What must never
//! happen is a correction that takes a tooth *further* from natural or overshoots past the locus,
//! and both of those are increases.
//!
//! A family that misses its bound gives up a quarter of its strength -
//! [`NATURALNESS_RESOLVE_STEP`] - and is measured again, up to [`NATURALNESS_MAX_RESOLVES`]
//! times. If it still misses, **that family is withdrawn** and the rest of the plan ships.
//!
//! **This is where phase 21 departs from phase 20.** Phase 20's texture guard withdraws the whole
//! plan, because its measurement is one number over one region and there is no way to attribute a
//! failure to one operation. Here the three regions are disjoint - iris, hair, teeth - and each
//! measurement is moved by exactly one family, so withdrawing all three because the hairline lost
//! energy would be throwing away a lint removal for a reason that has nothing to do with it.
//! ADR-0045 section 5 has the argument.
//!
//! ## The re-solve weakens, it does not re-detect
//!
//! A re-solve scales the surviving operations of one family. It does not run the detectors again,
//! because the detectors did not get it wrong - the *magnitude* did - and re-detecting on
//! partially edited pixels is phase 19's own trap: a measurement taken on a value the pass has
//! already moved is not linear in its own strength.
//!
//! ## Everything here is linear
//!
//! Invariant 8. Nothing here encodes.

use aura_core::contract::error::AuraError;
use aura_core::contract::micro::{
    ColourLocus, MicroOp, NaturalnessReport, OpFamily, CATCHLIGHT_FLOOR, HAIR_ENERGY_FLOOR,
    MAX_FLYAWAY_STRENGTH, MAX_GLARE_REDUCE, MAX_IRIS_CLARITY, MAX_SCLERA, MAX_TEETH_LUMA_EV,
    MAX_TEETH_YELLOW, NATURALNESS_MAX_RESOLVES, NATURALNESS_RESOLVE_STEP, TEETH_EXCURSION_CEILING,
};
use aura_core::contract::micro::{MicroOverride, MicroPlan, MicroRegion};
use aura_render::micro::{self, MicroContext};

use crate::errors;
use crate::texture_guard::Frame;

/// What the guard decided, and the pixels it decided it on.
#[derive(Debug, Clone)]
pub struct Guarded {
    /// The operations that survived, at the strengths they survived at.
    ///
    /// A withdrawn family contributes nothing here, which is what
    /// [`MicroPlan::broken_guarantee`] insists on.
    pub ops: Vec<MicroOp>,
    /// The measurement.
    pub report: NaturalnessReport,
    /// The edited pixels, for a caller that wants to show or export them.
    pub rendered: Vec<f32>,
}

/// Apply a plan, measure it, weaken what missed, and withdraw what could not be reached.
///
/// `before` is the frame as phases 14 to 20 left it. The returned pixels are `before` with the
/// surviving operations applied.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn enforce(before: &Frame, ops: &[MicroOp], context: &MicroContext) -> Guarded {
    // The baselines, measured on the unedited frame. Taken once: re-measuring the baseline after
    // a re-solve would compare each attempt against the previous attempt rather than against the
    // photograph, and a sequence of individually small losses would then all pass.
    let iris = context
        .regions
        .get(&MicroRegion::Iris)
        .cloned()
        .unwrap_or_default();
    let hair = context
        .regions
        .get(&MicroRegion::Hair)
        .cloned()
        .unwrap_or_default();
    let teeth = context
        .regions
        .get(&MicroRegion::Teeth)
        .cloned()
        .unwrap_or_default();

    let (iris_before, iris_samples) =
        micro::catchlight_peak(&before.rgb, before.width, before.height, &iris);
    let (hair_before, hair_samples) =
        micro::edge_energy(&before.rgb, before.width, before.height, &hair);
    let (teeth_before, _) = micro::teeth_excursion(
        &before.rgb,
        before.width,
        before.height,
        &teeth,
        context.neutral,
        context.teeth_locus,
    );

    if ops.is_empty() {
        return Guarded {
            ops: Vec::new(),
            report: NaturalnessReport::UNTOUCHED,
            rendered: before.rgb.clone(),
        };
    }

    let mut scales = [1.0f32; OpFamily::COUNT];
    let mut withdrawn = [false; OpFamily::COUNT];
    let mut resolves = 0u8;
    let mut attempts = [0u8; OpFamily::COUNT];

    // Bounded by construction: each family may weaken at most `NATURALNESS_MAX_RESOLVES` times,
    // so the loop runs at most one more time than the sum of those.
    let mut rendered;
    let mut report;
    loop {
        let surviving = surviving_ops(ops, &scales, withdrawn);
        rendered = before.rgb.clone();
        micro::apply(
            &mut rendered,
            before.width,
            before.height,
            &surviving,
            context,
        );

        let (iris_after, _) = micro::catchlight_peak(&rendered, before.width, before.height, &iris);
        let (hair_after, _) = micro::edge_energy(&rendered, before.width, before.height, &hair);
        let (excursion, teeth_samples) = micro::teeth_excursion(
            &rendered,
            before.width,
            before.height,
            &teeth,
            context.neutral,
            context.teeth_locus,
        );

        report = NaturalnessReport {
            catchlight_ratio: ratio(iris_after, iris_before),
            hair_energy_ratio: ratio(hair_after, hair_before),
            teeth_excursion: (excursion - teeth_before).max(0.0),
            measured_on: iris_samples + hair_samples + teeth_samples,
            resolves,
            withdrawn,
        };

        // Which families missed, and did any of them still have room to weaken?
        let missed = [
            report.hair_energy_ratio < HAIR_ENERGY_FLOOR - 1e-4
                && has_family(&surviving, OpFamily::Hair),
            report.teeth_excursion > TEETH_EXCURSION_CEILING + 1e-6
                && has_family(&surviving, OpFamily::Teeth),
            report.catchlight_ratio < CATCHLIGHT_FLOOR - 1e-4
                && has_family(&surviving, OpFamily::Eyes),
        ];
        if !missed.iter().any(|flag| *flag) {
            break;
        }

        let mut acted = false;
        for (index, missing) in missed.iter().enumerate() {
            if !*missing {
                continue;
            }
            let attempt = attempts.get(index).copied().unwrap_or(0);
            if attempt >= NATURALNESS_MAX_RESOLVES {
                if let Some(slot) = withdrawn.get_mut(index) {
                    if !*slot {
                        *slot = true;
                        acted = true;
                    }
                }
                continue;
            }
            if let Some(slot) = attempts.get_mut(index) {
                *slot = attempt + 1;
            }
            if let Some(slot) = scales.get_mut(index) {
                *slot *= NATURALNESS_RESOLVE_STEP;
            }
            resolves = resolves.saturating_add(1);
            acted = true;
        }
        if !acted {
            // Nothing left to weaken and nothing left to withdraw. Every remaining miss belongs
            // to a family with no surviving operations, which the loop cannot fix and which the
            // report already describes.
            break;
        }
    }

    report.withdrawn = withdrawn;
    report.resolves = resolves;

    let ops = surviving_ops(ops, &scales, withdrawn);
    // The pixels have to match the operations that shipped. The last render inside the loop was
    // made with the same scales and the same withdrawals, except on the iteration that set the
    // final withdrawal - so it is re-rendered here rather than reasoned about.
    let mut rendered = before.rgb.clone();
    micro::apply(&mut rendered, before.width, before.height, &ops, context);

    // Re-measure once, so the stored numbers describe the pixels that shipped rather than the
    // last attempt. Phase 20's `TextureReport` does the same, and for the same reason: a report
    // that describes a render nobody kept is a report that cannot be audited.
    let (iris_after, _) = micro::catchlight_peak(&rendered, before.width, before.height, &iris);
    let (hair_after, _) = micro::edge_energy(&rendered, before.width, before.height, &hair);
    let (excursion, teeth_samples) = micro::teeth_excursion(
        &rendered,
        before.width,
        before.height,
        &teeth,
        context.neutral,
        context.teeth_locus,
    );
    report.catchlight_ratio = ratio(iris_after, iris_before);
    report.hair_energy_ratio = ratio(hair_after, hair_before);
    report.teeth_excursion = (excursion - teeth_before).max(0.0);
    report.measured_on = iris_samples + hair_samples + teeth_samples;

    Guarded {
        ops,
        report,
        rendered,
    }
}

/// The operations of one family, scaled, with withdrawn families dropped.
fn surviving_ops(
    ops: &[MicroOp],
    scales: &[f32; OpFamily::COUNT],
    withdrawn: [bool; OpFamily::COUNT],
) -> Vec<MicroOp> {
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        let Some(family) = op.family() else {
            // Clothing. No naturalness floor covers fabric - see `MicroOp::family` - so a garment
            // clean is never scaled or withdrawn by this guard. Its bound is the area cap and the
            // fabric-texture refusal, both applied before the plan reaches here.
            out.push(op.clone());
            continue;
        };
        let index = OpFamily::ALL
            .iter()
            .position(|candidate| *candidate == family)
            .unwrap_or(0);
        if withdrawn.get(index).copied().unwrap_or(false) {
            continue;
        }
        let scale = scales.get(index).copied().unwrap_or(1.0);
        out.push(scale_op(op, scale));
    }
    out
}

/// Whether any surviving operation belongs to a family.
fn has_family(ops: &[MicroOp], family: OpFamily) -> bool {
    ops.iter().any(|op| op.family() == Some(family))
}

/// One operation at a fraction of its strength.
///
/// Every magnitude is clamped to its own contract ceiling on the way out, so a scale can only
/// ever make an operation gentler and a rounding error can never lift one past a bound.
fn scale_op(op: &MicroOp, scale: f32) -> MicroOp {
    let scale = scale.clamp(0.0, 1.0);
    match op {
        MicroOp::Flyaway { region, strength } => MicroOp::Flyaway {
            region: *region,
            strength: (strength * scale).clamp(0.0, MAX_FLYAWAY_STRENGTH),
        },
        MicroOp::Teeth {
            identity,
            luma,
            yellow_reduce,
        } => MicroOp::Teeth {
            identity: *identity,
            luma: (luma * scale).clamp(0.0, MAX_TEETH_LUMA_EV),
            yellow_reduce: (yellow_reduce * scale).clamp(0.0, MAX_TEETH_YELLOW),
        },
        MicroOp::Eyes {
            identity,
            sclera,
            iris_clarity,
        } => MicroOp::Eyes {
            identity: *identity,
            sclera: (sclera * scale).clamp(0.0, MAX_SCLERA),
            iris_clarity: (iris_clarity * scale).clamp(0.0, MAX_IRIS_CLARITY),
        },
        MicroOp::Clothing {
            region,
            kind,
            strength,
        } => MicroOp::Clothing {
            region: *region,
            kind: *kind,
            strength: *strength,
        },
        MicroOp::Glare { region, method } => MicroOp::Glare {
            region: *region,
            method: match method {
                // A borrow has no strength to give up: it either replaces a destroyed region or
                // it does not happen. When the eye family is withdrawn it is dropped entirely,
                // which is the only outcome available and the right one.
                aura_core::contract::micro::GlareMethod::BorrowFrom { source, alignment } => {
                    aura_core::contract::micro::GlareMethod::BorrowFrom {
                        source: *source,
                        alignment: *alignment,
                    }
                }
                aura_core::contract::micro::GlareMethod::Reduce { strength } => {
                    aura_core::contract::micro::GlareMethod::Reduce {
                        strength: (strength * scale).clamp(0.0, MAX_GLARE_REDUCE),
                    }
                }
            },
        },
    }
}

/// After over before, with the degenerate cases answered honestly.
fn ratio(after: f32, before: f32) -> f32 {
    if before <= f32::EPSILON {
        // Nothing was there to lose. One rather than zero: a region with no measurable content
        // cannot have had content removed from it, and reporting zero would withdraw a family
        // for a measurement that never existed.
        return 1.0;
    }
    (after / before).clamp(0.0, 4.0)
}

// ---------------------------------------------------------------------------
// Turning the contract predicates into this phase's errors
// ---------------------------------------------------------------------------

/// Refuse a plan that breaks one of this phase's guarantees.
///
/// **A refused plan is stored as no plan rather than as a weak one**, for the reason phase 20
/// gives: a stored row that lies to phases 25, 27 and 28 about what happened to somebody's face
/// is worse than an absent one.
///
/// # Errors
///
/// `AURA-ML-5103` naming the photograph and the guarantee.
pub fn check_plan(plan: &MicroPlan) -> Result<(), AuraError> {
    match plan.broken_guarantee() {
        None => Ok(()),
        Some(problem) => Err(errors::micro_failed(&plan.image_id.to_db(), problem)),
    }
}

/// Refuse an override that cannot be applied.
///
/// # Errors
///
/// `AURA-ML-5104` naming the problem.
pub fn check_override(values: &MicroOverride) -> Result<(), AuraError> {
    match values.problem() {
        None => Ok(()),
        Some(problem) => Err(errors::micro_edit_refused(problem)),
    }
}

/// Refuse a locus a caller supplied that no correction could be measured against.
///
/// # Errors
///
/// `AURA-ML-5105` naming the problem.
pub fn check_locus(locus: ColourLocus, key: &str) -> Result<(), AuraError> {
    match locus.problem() {
        None => Ok(()),
        Some(problem) => Err(errors::micro_matrix_refused(
            crate::micro::matrix::FILE,
            key,
            &problem,
        )),
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use aura_core::contract::composition::Box2;
    use aura_core::contract::micro::{GlareMethod, MicroCode, MicroReason};
    use aura_core::{IdentityId, PhotoId, SceneId};
    use std::collections::BTreeMap;

    fn photo() -> PhotoId {
        PhotoId::from_db("pht_00000000-0000-4000-8000-000000000021").expect("a photo id")
    }

    /// A frame with a hair region carrying real structure and an iris region carrying a
    /// catchlight, so both baselines are non-zero and both floors are meaningful.
    fn frame_with_regions() -> (Frame, MicroContext) {
        let (width, height) = (48usize, 48usize);
        let mut rgb = vec![0.30f32; width * height * 3];
        let mut hair = vec![0.0f32; width * height];
        let mut iris = vec![0.0f32; width * height];

        for y in 0..24 {
            for x in 0..48 {
                let index = y * width + x;
                if let Some(slot) = hair.get_mut(index) {
                    *slot = 1.0;
                }
                // Alternating strands: real edge energy for the hair measurement.
                let value = if x % 2 == 0 { 0.12 } else { 0.48 };
                for channel in 0..3 {
                    if let Some(slot) = rgb.get_mut(index * 3 + channel) {
                        *slot = value;
                    }
                }
            }
        }
        for y in 30..38 {
            for x in 30..38 {
                let index = y * width + x;
                if let Some(slot) = iris.get_mut(index) {
                    *slot = 1.0;
                }
            }
        }
        for y in 33..35 {
            for x in 33..35 {
                let index = y * width + x;
                for channel in 0..3 {
                    if let Some(slot) = rgb.get_mut(index * 3 + channel) {
                        *slot = 1.20;
                    }
                }
            }
        }

        let mut regions = BTreeMap::new();
        regions.insert(MicroRegion::Hair, hair);
        regions.insert(MicroRegion::Iris, iris.clone());
        regions.insert(MicroRegion::Eyes, iris);

        (
            Frame { rgb, width, height },
            MicroContext {
                regions,
                ..MicroContext::empty()
            },
        )
    }

    #[test]
    fn an_empty_plan_is_untouched_and_measures_nothing() {
        let (frame, context) = frame_with_regions();
        let guarded = enforce(&frame, &[], &context);
        assert_eq!(guarded.report, NaturalnessReport::UNTOUCHED);
        assert_eq!(guarded.rendered, frame.rgb);
    }

    #[test]
    fn a_gentle_plan_holds_every_floor() {
        let (frame, context) = frame_with_regions();
        let ops = vec![MicroOp::Flyaway {
            region: Box2 {
                x: 0.02,
                y: 0.02,
                w: 0.04,
                h: 0.04,
            },
            strength: 0.15,
        }];
        let guarded = enforce(&frame, &ops, &context);
        assert!(
            guarded.report.passed(),
            "a gentle plan missed a floor: {:?}",
            guarded.report
        );
        assert!(!guarded.report.any_withdrawn());
        assert_eq!(guarded.ops.len(), 1);
    }

    #[test]
    fn withdrawal_is_per_family_and_clothing_is_never_withdrawn() {
        // Hair withdrawn by hand, so the check is that the *other* families survive it - which is
        // the property that distinguishes this guard from phase 20's.
        let ops = vec![
            MicroOp::Flyaway {
                region: Box2 {
                    x: 0.02,
                    y: 0.02,
                    w: 0.04,
                    h: 0.04,
                },
                strength: 0.5,
            },
            MicroOp::Clothing {
                region: Box2 {
                    x: 0.60,
                    y: 0.60,
                    w: 0.01,
                    h: 0.01,
                },
                kind: aura_core::contract::micro::ClothingIssue::Lint,
                strength: 0.5,
            },
        ];
        let withdrawn = [true, false, false];
        let surviving = surviving_ops(&ops, &[1.0; OpFamily::COUNT], withdrawn);
        assert_eq!(surviving.len(), 1);
        assert_eq!(surviving.first().map(MicroOp::as_str), Some("clothing"));
    }

    #[test]
    fn scaling_can_only_ever_make_an_operation_gentler() {
        let op = MicroOp::Teeth {
            identity: IdentityId::new(),
            luma: MAX_TEETH_LUMA_EV,
            yellow_reduce: MAX_TEETH_YELLOW,
        };
        let scaled = scale_op(&op, NATURALNESS_RESOLVE_STEP);
        assert!(scaled.strength() < op.strength());
        // And a scale above one cannot lift it past the ceiling.
        let clamped = scale_op(&op, 4.0);
        assert!(clamped.problem().is_none());
    }

    #[test]
    fn a_borrow_is_dropped_rather_than_weakened_when_the_eye_family_is_withdrawn() {
        let ops = vec![MicroOp::Glare {
            region: Box2 {
                x: 0.40,
                y: 0.40,
                w: 0.02,
                h: 0.01,
            },
            method: GlareMethod::BorrowFrom {
                source: photo(),
                alignment: 0.95,
            },
        }];
        let kept = surviving_ops(&ops, &[1.0; OpFamily::COUNT], [false, false, true]);
        assert!(kept.is_empty());
    }

    #[test]
    fn a_plan_with_no_reason_is_5103_and_a_sound_one_passes() {
        let good = MicroPlan::nothing(
            photo(),
            SceneId::Ceremony,
            MicroReason::plain(MicroCode::NoFlyawayFound, 0.0),
        );
        assert!(check_plan(&good).is_ok());
        let mut bad = good;
        bad.reasons.clear();
        let err = check_plan(&bad).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5103");
    }

    #[test]
    fn an_empty_override_is_5104() {
        let err = check_override(&MicroOverride::default()).expect_err("refused");
        assert_eq!(err.code.0, "AURA-ML-5104");
        assert!(check_override(&MicroOverride {
            borrowing: Some(false),
            ..MicroOverride::default()
        })
        .is_ok());
    }

    #[test]
    fn a_ratio_over_nothing_is_one_rather_than_zero() {
        assert_eq!(ratio(0.0, 0.0), 1.0);
        assert_eq!(ratio(0.5, 1.0), 0.5);
    }
}
