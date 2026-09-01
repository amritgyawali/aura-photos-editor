//! What may be done about a finding, and the one gate every proposal passes through.
//! PHASE-27 sections 6.2 and 6.3.
//!
//! ## The choke point
//!
//! There are two ways a remedy can be proposed: [`propose`] derives one mechanically from a
//! finding, and [`crate::planner`] returns a model's suggestion. **Both go through
//! [`validate`], and nothing else in this crate constructs a `Remedy` that reaches the loop.**
//!
//! That is phase 24's mechanism copied rather than its promise. `aura_generative::source::select`
//! takes a `SafeCandidate` with no public constructor; here the equivalent is that
//! `planner::ProposedStep` is not a `Remedy` and the only function that turns one into the other is
//! this module's. A step naming an operation the policy does not permit, a magnitude outside the
//! contract's bounds, or a frame that is not this ticket's frame, produces `None`.
//!
//! So an unreachable provider, a spent budget, a malformed answer, a hallucinated parameter name
//! and a plan proposing something forbidden all leave the image with its mechanical triage. ADR-0055
//! section 7.
//!
//! ## Why a remedy narrows rather than sets
//!
//! [`Remedy::ResolveParam`] carries a *constraint*, never a value. A remedy that could set a
//! parameter would make this phase a place a photograph can be edited from, and phase 14's rule is
//! that a delivered file is re-creatable from four values one of which is a recipe written by
//! exactly one function. A remedy that can only add a constraint is a request to the deciding phase,
//! which still solves its own problem under its own bounds and its own guards.
//!
//! The same argument is why [`Remedy::ReduceStrength`] is bounded strictly below one. A QC agent
//! that could raise a strength would be a QC agent that edits, and the phase that decides how strong
//! an operation should be is the phase that owns the operation.

use aura_core::contract::qc::{
    ImageId, QcCategory, QcCode, QcTicket, Remedy, SolveTarget, MAX_STRENGTH_FACTOR,
    MIN_STRENGTH_FACTOR,
};

use crate::checks::{Finding, Frame};
use crate::policy::LoopPolicy;

/// The strength multiplier a first reduction proposes.
///
/// Three quarters, matching phase 20's texture guard and phase 21's naturalness guard, which both
/// re-solve at 0.75 before withdrawing. Copying their step is not cosmetic: a QC remedy that reduced
/// by a different amount would produce a frame neither guard had ever evaluated.
const FIRST_REDUCTION: f32 = 0.75;

/// The multiplier a second round proposes, when the first was kept and the finding stands.
///
/// Half. `MIN_STRENGTH_FACTOR` is 0.25 and anything at or below it is switching the operation off,
/// which is [`Remedy::RevertOp`] - a different row, a different reason code and a different sentence
/// in the report.
const SECOND_REDUCTION: f32 = 0.50;

/// The mechanical remedy for one finding.
///
/// Section 6.2: "single-symptom tickets with an obvious remedy are handled by deterministic rules -
/// cheap, fast, reproducible." This is those rules, and there is exactly one per category plus the
/// two special cases the finding code names.
///
/// A finding whose `expected_gain` is zero has already said that nothing mechanical will help, and
/// gets an escalation. That is how `IdentityDrift`, `CropUnsafe`, `CleanupUndisclosed`,
/// `CoverageMissing` and every duplicate reach a person without this function needing to know why.
#[must_use]
pub fn propose(finding: &Finding, frame: &Frame, round: u8) -> Remedy {
    if finding.expected_gain <= 0.0 || !finding.expected_gain.is_finite() {
        return Remedy::Escalate {
            note: finding.code.user_text().to_string(),
        };
    }
    if frame.user_edited {
        // The photographer set this frame's parameters. Nothing here may change them, and the
        // ticket says so through its reason set rather than by not existing.
        return Remedy::Escalate {
            note: QcCode::UserEdited.user_text().to_string(),
        };
    }

    match finding.category {
        // Root causes first. Section 7's offline fallback ordering is `QcCategory::triage_rank`,
        // and these three are the top of it.
        QcCategory::Consistency => Remedy::ResolveParam {
            target: SolveTarget::Normalisation,
            constraint: "move this frame toward its node target and hold its own exposure".into(),
        },
        QcCategory::Exposure => Remedy::ResolveParam {
            target: SolveTarget::Exposure,
            constraint: "land on the scene band without adding clipping".into(),
        },
        QcCategory::Skin => {
            if finding.code == QcCode::SkinGuardExceeded {
                // The grade moved skin. Reduce the grade, do not re-solve the white balance: the
                // illuminant is not what went wrong.
                strength(round, "colour.grade")
            } else {
                Remedy::ResolveParam {
                    target: SolveTarget::WhiteBalance,
                    constraint: "hold the exposure and correct toward this person's own skin \
                                 target"
                        .into(),
                }
            }
        }
        QcCategory::Crop => Remedy::ResolveParam {
            target: SolveTarget::Crop,
            constraint: "choose a rectangle that passes every safety check".into(),
        },
        QcCategory::Cleanup => Remedy::RevertOp {
            op: "cleanup".into(),
        },
        QcCategory::Mask => match finding.code {
            // Nothing supported the edit at all: switch it off rather than do less of it.
            QcCode::MaskUncovered => Remedy::RevertOp { op: "local".into() },
            _ => strength(round, "local"),
        },
        QcCategory::Retouch => match finding.code {
            QcCode::AllowanceExceeded => strength(round, "local"),
            QcCode::NaturalnessMissed => strength(round, "micro"),
            _ => strength(round, "retouch"),
        },
        QcCategory::Sharpness => match finding.code {
            QcCode::TextureLost => Remedy::ResolveParam {
                target: SolveTarget::Restoration,
                constraint: "step the denoise tier down one".into(),
            },
            QcCode::RingingDetected => strength(round, "restore.sharpen"),
            QcCode::SharpnessBelowFloor => Remedy::ResolveParam {
                target: SolveTarget::Restoration,
                constraint: "sharpen the subject region if the preconditions hold".into(),
            },
            _ => Remedy::Escalate {
                note: finding.code.user_text().to_string(),
            },
        },
        // Both gallery-scoped categories reach a person: their remedy is a change to what is in the
        // gallery, and that is `replace::consider` rather than a parameter.
        QcCategory::Duplicate | QcCategory::Coverage => Remedy::Escalate {
            note: finding.code.user_text().to_string(),
        },
    }
}

/// A strength reduction for the round in progress.
///
/// Round 1 proposes 0.75, matching phases 20 and 21's own guards; round 2 proposes 0.50. Anything
/// lower is switching the operation off, which is a different remedy.
fn strength(round: u8, op: &str) -> Remedy {
    let factor = if round >= 1 {
        SECOND_REDUCTION
    } else {
        FIRST_REDUCTION
    };
    Remedy::ReduceStrength {
        op: op.to_string(),
        factor,
    }
}

/// Why a remedy was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// A magnitude outside the contract's own bounds, or an empty operation name.
    OutOfBounds,
    /// A replacement naming a frame that is not this ticket's runner-up.
    NotTheRunnerUp,
    /// A replacement on a frame with no runner-up at all.
    NoRunnerUp,
    /// A mutating remedy on a frame the photographer set by hand.
    UserEdited,
    /// An operation this category may not touch.
    WrongOperation,
    /// A replacement below the policy's confidence floor.
    TooUncertain,
}

impl Refusal {
    /// The reason code this refusal is recorded as.
    #[must_use]
    pub const fn code(self) -> QcCode {
        match self {
            Self::NotTheRunnerUp | Self::NoRunnerUp => QcCode::RunnerUpAbsent,
            Self::UserEdited => QcCode::UserEdited,
            Self::OutOfBounds | Self::WrongOperation | Self::TooUncertain => {
                QcCode::RemedyRefusedByPolicy
            }
        }
    }
}

/// The one gate. Every remedy the loop applies has come through here.
///
/// # Errors
///
/// Returns the [`Refusal`] rather than an `AuraError`, because a refused remedy is not a failure:
/// it is the product declining to do something, and it becomes a reason on the ticket rather than
/// an error anybody has to handle.
pub fn validate(
    remedy: Remedy,
    ticket: &QcTicket,
    frame: &Frame,
    policy: LoopPolicy,
) -> Result<Remedy, Refusal> {
    if !remedy.is_within_bounds() {
        return Err(Refusal::OutOfBounds);
    }
    if remedy.mutates() && frame.user_edited {
        return Err(Refusal::UserEdited);
    }
    match &remedy {
        Remedy::ReplaceFrame { with } => {
            let Some(runner_up) = frame.runner_up else {
                return Err(Refusal::NoRunnerUp);
            };
            // Only phase 12's own runner-up. A swap to any other frame would be this phase
            // re-answering "what is being delivered", which is `CullService`'s question.
            if *with != runner_up {
                return Err(Refusal::NotTheRunnerUp);
            }
            if ticket.confidence < policy.replace_confidence {
                return Err(Refusal::TooUncertain);
            }
        }
        Remedy::ReduceStrength { op, factor } => {
            if !permits(ticket.category, op) {
                return Err(Refusal::WrongOperation);
            }
            // Belt and braces with `is_within_bounds`, because this is the one number a model may
            // propose and the bound is what stops a hallucinated 3.0 reaching a photograph.
            if *factor < MIN_STRENGTH_FACTOR || *factor > MAX_STRENGTH_FACTOR {
                return Err(Refusal::OutOfBounds);
            }
        }
        Remedy::RevertOp { op } => {
            if !permits(ticket.category, op) {
                return Err(Refusal::WrongOperation);
            }
        }
        Remedy::ResolveParam { target, .. } => {
            if !solves(ticket.category, *target) {
                return Err(Refusal::WrongOperation);
            }
        }
        // Always allowed. Escalation changes nothing, which is what makes it the safe answer when
        // anything at all is uncertain.
        Remedy::Escalate { .. } => {}
    }
    Ok(remedy)
}

/// Which operations a category's remedy may name.
///
/// The list is here rather than in the thresholds file for the reason `Remedy::collateral_checks`
/// is a `const fn`: it is a fact about what a category measures, not a preference. A studio that
/// added `cleanup` to the retouch row would produce a build that removes objects to fix skin
/// texture, and nothing about the pass would look wrong while it did so.
fn permits(category: QcCategory, op: &str) -> bool {
    match category {
        QcCategory::Retouch => matches!(op, "retouch" | "micro" | "local"),
        QcCategory::Mask => matches!(op, "local" | "retouch" | "micro"),
        QcCategory::Sharpness => matches!(op, "restore.sharpen" | "restore.denoise"),
        QcCategory::Cleanup => op == "cleanup",
        QcCategory::Skin => matches!(op, "colour.grade" | "retouch"),
        QcCategory::Consistency => op == "colour.grade",
        // Exposure, crop, duplicate and coverage have no operation to reduce: their remedies are a
        // re-solve, a selection change or a person.
        QcCategory::Exposure | QcCategory::Crop | QcCategory::Duplicate | QcCategory::Coverage => {
            false
        }
    }
}

/// Which decisions a category's remedy may re-run.
fn solves(category: QcCategory, target: SolveTarget) -> bool {
    match category {
        QcCategory::Consistency => matches!(
            target,
            SolveTarget::Normalisation | SolveTarget::Grade | SolveTarget::WhiteBalance
        ),
        QcCategory::Skin => matches!(
            target,
            SolveTarget::WhiteBalance | SolveTarget::Normalisation | SolveTarget::Grade
        ),
        QcCategory::Exposure => matches!(target, SolveTarget::Exposure | SolveTarget::Grade),
        QcCategory::Sharpness => target == SolveTarget::Restoration,
        QcCategory::Crop => target == SolveTarget::Crop,
        // Five categories no parameter re-solve can address, in two groups that are separate
        // because they are unaddressable for different reasons. The first three are about an
        // *operation* that ran - the fix is to reduce or revert it, not to re-solve a parameter.
        // The last two are about the *set* - the fix is a different photograph, or none.
        QcCategory::Retouch | QcCategory::Mask | QcCategory::Cleanup => false,
        #[allow(clippy::match_same_arms)]
        QcCategory::Duplicate | QcCategory::Coverage => false,
    }
}

/// The remedy a frame's runner-up would be, when one exists.
///
/// Not validated here; [`crate::replace::consider`] is what decides whether a swap is justified,
/// and this only builds the shape. Separate because a caller that wants to *ask* about a swap must
/// go through the coverage filter, and a function that both built and approved one would be a place
/// to route around it.
#[must_use]
pub fn replacement_for(frame: &Frame) -> Option<Remedy> {
    frame
        .runner_up
        .map(|with: ImageId| Remedy::ReplaceFrame { with })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::ids::ProjectId;
    use aura_core::contract::qc::Evidence;
    use aura_core::contract::scene::SceneId;

    fn finding(category: QcCategory, code: QcCode, gain: f32) -> Finding {
        Finding::new(category, code, 4.0, 2.0, gain, 0.9)
    }

    fn frame() -> Frame {
        Frame::empty(ImageId::new(), SceneId::Ceremony)
    }

    fn ticket_for(category: QcCategory, confidence: f32) -> QcTicket {
        let f = finding(category, QcCode::SkinDrift, 1.0);
        let mut ticket = crate::ticket::from_finding(
            ProjectId::new(),
            &frame(),
            f,
            Remedy::Escalate { note: "n".into() },
            0,
        );
        ticket.category = category;
        ticket.confidence = confidence;
        ticket.evidence = Evidence::None;
        ticket
    }

    #[test]
    fn a_finding_that_predicts_no_gain_escalates() {
        let remedy = propose(
            &finding(QcCategory::Crop, QcCode::CropUnsafe, 0.0),
            &frame(),
            0,
        );
        assert!(matches!(remedy, Remedy::Escalate { .. }));
        assert!(!remedy.mutates());
    }

    #[test]
    fn a_frame_the_photographer_edited_is_never_mechanically_changed() {
        let mut edited = frame();
        edited.user_edited = true;
        let remedy = propose(
            &finding(QcCategory::Consistency, QcCode::ConsistencyDrift, 2.0),
            &edited,
            0,
        );
        assert!(matches!(remedy, Remedy::Escalate { .. }));
    }

    #[test]
    fn a_colour_drift_re_solves_the_normalisation_rather_than_reducing_a_strength() {
        let remedy = propose(
            &finding(QcCategory::Consistency, QcCode::ConsistencyDrift, 2.0),
            &frame(),
            0,
        );
        assert!(matches!(
            remedy,
            Remedy::ResolveParam {
                target: SolveTarget::Normalisation,
                ..
            }
        ));
    }

    #[test]
    fn a_guard_excursion_reduces_the_grade_and_a_drift_re_solves_the_light() {
        // The two skin codes have opposite root causes and therefore opposite remedies. Fixing a
        // guard excursion by re-solving white balance would treat a grade problem as a light one.
        let guard = propose(
            &finding(QcCategory::Skin, QcCode::SkinGuardExceeded, 2.0),
            &frame(),
            0,
        );
        assert!(matches!(guard, Remedy::ReduceStrength { .. }));
        let drift = propose(
            &finding(QcCategory::Skin, QcCode::SkinDrift, 2.0),
            &frame(),
            0,
        );
        assert!(matches!(
            drift,
            Remedy::ResolveParam {
                target: SolveTarget::WhiteBalance,
                ..
            }
        ));
    }

    #[test]
    fn an_uncovered_region_reverts_rather_than_reduces() {
        let remedy = propose(
            &finding(QcCategory::Mask, QcCode::MaskUncovered, 1.0),
            &frame(),
            0,
        );
        assert!(matches!(remedy, Remedy::RevertOp { .. }));
    }

    #[test]
    fn the_first_reduction_matches_the_step_phases_twenty_and_twenty_one_already_use() {
        let first = propose(
            &finding(QcCategory::Retouch, QcCode::TextureFloorMissed, 1.0),
            &frame(),
            0,
        );
        match first {
            Remedy::ReduceStrength { factor, .. } => assert_eq!(factor, 0.75),
            other => panic!("expected a reduction: {other:?}"),
        }
        let second = propose(
            &finding(QcCategory::Retouch, QcCode::TextureFloorMissed, 1.0),
            &frame(),
            1,
        );
        match second {
            Remedy::ReduceStrength { factor, .. } => assert_eq!(factor, 0.50),
            other => panic!("expected a reduction: {other:?}"),
        }
    }

    #[test]
    fn a_reduction_never_reaches_the_point_where_it_is_a_revert() {
        for round in 0..=2 {
            let remedy = propose(
                &finding(QcCategory::Retouch, QcCode::TextureFloorMissed, 1.0),
                &frame(),
                round,
            );
            if let Remedy::ReduceStrength { factor, .. } = remedy {
                assert!(factor > MIN_STRENGTH_FACTOR);
            }
        }
    }

    #[test]
    fn a_hallucinated_magnitude_is_refused() {
        let bad = Remedy::ReduceStrength {
            op: "retouch".into(),
            factor: 3.0,
        };
        let err = validate(
            bad,
            &ticket_for(QcCategory::Retouch, 0.9),
            &frame(),
            LoopPolicy::reference(),
        )
        .expect_err("a magnitude outside the contract is refused");
        assert_eq!(err, Refusal::OutOfBounds);
        assert_eq!(err.code(), QcCode::RemedyRefusedByPolicy);
    }

    #[test]
    fn a_remedy_naming_an_operation_its_category_does_not_own_is_refused() {
        // The failure this stops: removing an object to fix skin texture. Nothing about the pass
        // would look wrong while it did so.
        let wrong = Remedy::RevertOp {
            op: "cleanup".into(),
        };
        let err = validate(
            wrong,
            &ticket_for(QcCategory::Retouch, 0.9),
            &frame(),
            LoopPolicy::reference(),
        )
        .expect_err("a category may only touch its own operations");
        assert_eq!(err, Refusal::WrongOperation);
    }

    #[test]
    fn a_replacement_to_anything_but_the_runner_up_is_refused() {
        let mut with_runner_up = frame();
        let runner_up = ImageId::new();
        with_runner_up.runner_up = Some(runner_up);
        let stranger = Remedy::ReplaceFrame {
            with: ImageId::new(),
        };
        let err = validate(
            stranger,
            &ticket_for(QcCategory::Sharpness, 0.95),
            &with_runner_up,
            LoopPolicy::reference(),
        )
        .expect_err("only phase 12's own runner-up");
        assert_eq!(err, Refusal::NotTheRunnerUp);

        let correct = Remedy::ReplaceFrame { with: runner_up };
        assert!(validate(
            correct,
            &ticket_for(QcCategory::Sharpness, 0.95),
            &with_runner_up,
            LoopPolicy::reference()
        )
        .is_ok());
    }

    #[test]
    fn a_replacement_below_the_policy_floor_is_refused() {
        let mut with_runner_up = frame();
        let runner_up = ImageId::new();
        with_runner_up.runner_up = Some(runner_up);
        let err = validate(
            Remedy::ReplaceFrame { with: runner_up },
            &ticket_for(QcCategory::Sharpness, 0.70),
            &with_runner_up,
            LoopPolicy::reference(),
        )
        .expect_err("a swap needs more confidence than a parameter fix");
        assert_eq!(err, Refusal::TooUncertain);
    }

    #[test]
    fn a_replacement_on_a_frame_with_no_alternative_is_refused_and_says_which() {
        let err = validate(
            Remedy::ReplaceFrame {
                with: ImageId::new(),
            },
            &ticket_for(QcCategory::Sharpness, 0.95),
            &frame(),
            LoopPolicy::reference(),
        )
        .expect_err("no runner-up");
        assert_eq!(err, Refusal::NoRunnerUp);
        // "there is no alternative" and "the alternative is not better" are separate sentences a
        // photographer reads, so they are separate codes.
        assert_eq!(err.code(), QcCode::RunnerUpAbsent);
    }

    #[test]
    fn every_mutating_remedy_is_refused_on_a_frame_the_photographer_edited() {
        let mut edited = frame();
        edited.user_edited = true;
        edited.runner_up = Some(ImageId::new());
        for remedy in [
            Remedy::ReduceStrength {
                op: "retouch".into(),
                factor: 0.75,
            },
            Remedy::RevertOp {
                op: "retouch".into(),
            },
            Remedy::ResolveParam {
                target: SolveTarget::Grade,
                constraint: "x".into(),
            },
        ] {
            let err = validate(
                remedy,
                &ticket_for(QcCategory::Retouch, 0.99),
                &edited,
                LoopPolicy::reference(),
            )
            .expect_err("a photographer's own frame is left alone");
            assert_eq!(err, Refusal::UserEdited);
        }
    }

    #[test]
    fn an_escalation_is_always_allowed_even_on_a_frame_the_photographer_edited() {
        let mut edited = frame();
        edited.user_edited = true;
        assert!(validate(
            Remedy::Escalate { note: "n".into() },
            &ticket_for(QcCategory::Retouch, 0.1),
            &edited,
            LoopPolicy::reference()
        )
        .is_ok());
    }

    #[test]
    fn a_solve_target_a_category_does_not_own_is_refused() {
        let wrong = Remedy::ResolveParam {
            target: SolveTarget::Crop,
            constraint: "x".into(),
        };
        let err = validate(
            wrong,
            &ticket_for(QcCategory::Skin, 0.9),
            &frame(),
            LoopPolicy::reference(),
        )
        .expect_err("skin does not re-run the crop search");
        assert_eq!(err, Refusal::WrongOperation);
    }
}
