//! FROZEN CONTRACT. Quality control, remediation and the bounded re-edit loop. PHASE-27 section 5.
//!
//! Twenty-six phases decided things about a photograph, a wedding or a camera body. This one
//! decides about **the product's own decisions**, and that changes what every shape here has to
//! carry.
//!
//! ## The one property that separates a QC agent from a tenth analyser
//!
//! Every phase from 09 onward measures something and writes a verdict. This one measures a
//! *verdict* - phase 16's grade against phase 25's node target, phase 20's texture report against
//! its own floor, phase 23's crop against its safety report - and is then permitted to **change
//! it**. [`Remedy`] is the first shape in the product that can undo another phase's work, and
//! [`Remedy::ReplaceFrame`] is the first that can change which photograph is delivered at all.
//!
//! That authority is what the rest of this module is built to bound.
//!
//! ## The six properties this contract exists to make structural
//!
//! **A finding is a number and a threshold, never an opinion.** [`QcTicket::deviation`] and
//! [`QcTicket::threshold`] are not optional, and there is no variant of [`QcCode`] that means "this
//! looks wrong". Section 6.1: a ticket is actionable, testable and explainable exactly when it says
//! *how far* something is from *what*. `skin 4.2 dE00 against a 2.5 threshold` is a query a gate can
//! run; "the skin looks magenta" is a sentence nobody can regress-test.
//!
//! **Improvement is measured against what the ticket opened with.** [`QcRound::deviation_before`]
//! and [`QcRound::deviation_after`] are both on the row, and [`QcRound::realised_share`] is the
//! number the loop decides on. Comparing a remediated frame against the *threshold* instead would
//! discard every partial repair on the hardest frames, which are the frames a photographer most
//! wants helped. ADR-0055 section 4, and it is phase 19's lesson - a converged value cannot be used
//! to detect its own constraints - in a new place.
//!
//! **A remedy is checked for collateral damage on the checks it can reach, and that list is in
//! code.** [`Remedy::collateral_checks`] is a `const fn` on the enum rather than a row in a TOML
//! file, because which categories an operation can move is a fact about the operation. A studio
//! that shortened the list would produce a pass that still ran, still reported, and silently
//! stopped noticing one class of damage. ADR-0055 section 5.
//!
//! **A replacement is refused by a filter, never scored against one.** Coverage is re-validated
//! before a candidate's metrics are compared, so a swap that would leave a must-have uncovered is
//! not a worse candidate - it is not a candidate. Phase 12 wrote this rule, phase 23 applied it to
//! crop safety and phase 24 made it a property of the type system; this is its fourth application.
//!
//! **An absent input is a skipped check and not a passed one.** [`QcOutline::checked`] counts what
//! ran, [`QcReport::skipped`] is on the report a photographer reads, and every category has an
//! `*Unavailable` code separate from every finding code. A wedding whose masks are missing must not
//! report zero mask artefacts and read as a clean bill of health - which in this build, with
//! several heads untrained, is the common case rather than the exotic one. Phase 24's rule.
//!
//! **Nothing here moves a pixel and nothing here writes a recipe.** [`QcService`] has no `apply`.
//! A remedy is a decision that phase 14's `aura_recipe::schema::merge` executes, and
//! `crates/aura-qc/tests/no_pixel_ops.rs` fails the build if this phase's crate grows an operator
//! of its own. Phase 14's rule, twelfth application.
//!
//! ## The one thing a later phase can get wrong
//!
//! **QC runs after every editing phase and before export, and it reads their stored answers rather
//! than recomputing them.** Phase 28 calls this as a stage; phase 30 consumes
//! [`QcTicket::status`] and [`QcTicket::outcome_code`] as its learning signal. A caller that ran QC
//! before phase 25's normalisation would be inspecting frames that are about to move, and every
//! consistency ticket it wrote would be a ticket about work that had not happened yet.

use std::cmp::Ordering;
use std::fmt;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::contract::composition::Box2;
use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{IdentityId, ProjectId, TicketId};
use crate::contract::ledger::Autonomy;
use crate::contract::scene::{SceneId, Timestamp};

pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The most remediation rounds one photograph may receive.
///
/// Section 6.3's own number. Two rather than three because the second round exists to fix what the
/// first round's *side effects* introduced, and a third round is a product negotiating with itself:
/// by then either the remedy family is wrong for this frame or the ticket is not mechanically
/// fixable, and both of those want a person rather than another attempt.
pub const MAX_ROUNDS: u8 = 2;

/// The share of a predicted gain a remedy must actually realise to be kept.
///
/// Half. A remedy that promised 2.0 dE00 of improvement and delivered 1.0 helped; one that
/// delivered 0.1 did not, whatever the absolute number says.
///
/// Not 1.0, for the reason phase 25 lowered its own reduction gate: a build that demanded the whole
/// of an honest prediction would revert almost every remedy, because the predictor is
/// `expected_gain` on a ticket written by arithmetic sitting on placeholder heads. Requiring
/// perfection from an imperfect predictor is a gate a correct implementation cannot meet, which is
/// the same defect phases 19, 21, 22 and 25 each found once. ADR-0055 section 4.
pub const MIN_GAIN_SHARE: f32 = 0.50;

/// How much a remedy may worsen another check, as a share of *that* check's own threshold.
///
/// A share rather than an absolute, because the ten checks are measured in five different units -
/// dE00, EV, a normalised sharpness ratio, a fraction of frame area and a plain count - and one
/// number stated in dE00 does not stop at the same place in EV.
///
/// Ten per cent. Section 6.3 asks for "a small tolerance" and this is the smallest one that does
/// not turn ordinary measurement noise into a revert.
pub const MAX_COLLATERAL: f32 = 0.10;

/// The lowest confidence at which a parameter-level remedy may be applied without a person.
///
/// Section 6.4 asks that replacements require "higher confidence than parameter fixes" without
/// naming either number. This is the low one, and it is low deliberately: a parameter fix that is
/// wrong produces a photograph that is slightly worse and is reverted by re-inspection inside the
/// same pass.
pub const FIX_CONFIDENCE_FLOOR: f32 = 0.60;

/// The lowest confidence at which a frame may be replaced by its runner-up without a person.
///
/// Section 6.4's "higher confidence than parameter fixes", as a number. The gap is large rather
/// than nominal because the two mistakes are not comparable: a bad parameter fix is a slightly
/// worse photograph that the loop puts back, and a bad replacement delivers a **different
/// photograph**, which a photographer scrolling a gallery has no way to notice - the frame they
/// would have chosen is simply not there.
pub const REPLACE_CONFIDENCE_FLOOR: f32 = 0.85;

/// How much better a runner-up's post-edit metrics must be before a swap is considered.
///
/// Section 6.4's "clearly better", as a share of the ticket's own threshold. A runner-up that is
/// 2 % sharper is not clearly better; it is the same photograph twice, and swapping on that margin
/// makes the gallery's contents a function of measurement noise.
pub const REPLACE_MARGIN: f32 = 0.35;

/// The most tickets one photograph may carry.
///
/// Ten checks run, and a frame that failed eight of them does not need the ninth reported: it needs
/// a person. Above this the image escalates whole with [`QcCode::MultiSymptom`], which is also the
/// trigger section 7 uses to reach the planner.
pub const MAX_TICKETS_PER_IMAGE: usize = 8;

/// The number of open tickets on one image that sends it to the planner.
///
/// Section 7's trigger verbatim: "images with >= 3 open tickets, or contradictory tickets, or a
/// failed first remediation round".
pub const PLANNER_TICKET_FLOOR: usize = 3;

/// The most planner calls one wedding may make. Section 7's cost control.
pub const MAX_PLANNER_CALLS: u32 = 40;

/// The most steps one plan may contain. Section 7's schema, as a constant.
pub const MAX_PLAN_STEPS: usize = 4;

/// The most read-only tool calls one planner invocation may make. Section 7.
pub const MAX_TOOL_STEPS: u8 = 6;

/// The most reasons one ticket carries.
///
/// Four, matching phase 12's keepers rather than phases 09 to 11's six. A finding with six reasons
/// is a finding nobody believes, and this is the phase whose whole product is a sentence somebody
/// reads.
pub const MAX_REASONS: usize = 4;

/// The smallest strength multiplier [`Remedy::ReduceStrength`] may propose.
///
/// A quarter. Below this the operation is being switched off rather than reduced, and switching an
/// operation off is [`Remedy::RevertOp`] - which is a different row, a different reason code and a
/// different sentence in the report.
pub const MIN_STRENGTH_FACTOR: f32 = 0.25;

/// The largest strength multiplier [`Remedy::ReduceStrength`] may propose.
///
/// Strictly below one. This remedy *reduces*; a QC agent that could raise a strength would be a
/// QC agent that edits, and the phase that decides how strong an operation should be is the phase
/// that owns the operation.
pub const MAX_STRENGTH_FACTOR: f32 = 0.90;

/// The per-wedding wall-clock ceiling for one QC pass, in milliseconds.
///
/// Section 11's budget of 90 s per 1,000 images, expressed for the pass rather than per image so
/// the scheduler has something to check against. `aura_qc::api` stops opening new images when this
/// is spent and reports what it did not reach, which is a different outcome from a clean run.
pub const PASS_BUDGET_MS_PER_1K: u64 = 90_000;

/// The ceiling on one remediation round, in milliseconds. Section 11.
pub const ROUND_BUDGET_MS: u64 = 1_200;

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

/// The ten inspections, and the vocabulary a ticket is filed under.
///
/// Section 2.1's list in section 2.1's order, which is also the order
/// [`QcCategory::triage_rank`] uses: section 7's offline fallback asks for "consistency ->
/// exposure -> skin -> retouch -> sharpness" and the principle behind that ordering generalises to
/// all ten - **fix root causes before symptoms**. A frame whose white balance is wrong is a frame
/// whose skin reads magenta and whose retouch looks heavy, and remediating the retouch first is
/// treating a symptom while leaving the cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QcCategory {
    /// Colour and tone against this frame's scene-node anchors. Phase 25.
    Consistency,
    /// A person's skin against their own gallery target. Phases 15, 16 and 25.
    Skin,
    /// Exposure and clipping after the edit. Phases 09, 14, 15 and 16.
    Exposure,
    /// Subject sharpness after restoration. Phases 09 and 22.
    Sharpness,
    /// Texture loss and retouch artefacts. Phases 20 and 21.
    Retouch,
    /// Mask edge artefacts and regions edited through a boundary nobody could determine.
    /// Phases 18 and 19.
    Mask,
    /// Crop safety after geometry. Phase 23.
    Crop,
    /// Generative cleanup artefacts and undisclosed removals. Phase 24.
    Cleanup,
    /// Near-duplicate frames that both reached the gallery. Phase 08.
    Duplicate,
    /// Must-have rules and identity minimums after everything else. Phase 12.
    Coverage,
}

impl QcCategory {
    /// Every category, in section 2.1's order.
    pub const ALL: [Self; 10] = [
        Self::Consistency,
        Self::Skin,
        Self::Exposure,
        Self::Sharpness,
        Self::Retouch,
        Self::Mask,
        Self::Crop,
        Self::Cleanup,
        Self::Duplicate,
        Self::Coverage,
    ];

    /// How many there are.
    pub const COUNT: usize = 10;

    /// The stored form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Consistency => "consistency",
            Self::Skin => "skin",
            Self::Exposure => "exposure",
            Self::Sharpness => "sharpness",
            Self::Retouch => "retouch",
            Self::Mask => "mask",
            Self::Crop => "crop",
            Self::Cleanup => "cleanup",
            Self::Duplicate => "duplicate",
            Self::Coverage => "coverage",
        }
    }

    /// Parse the stored form.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    /// Where this category sits in the root-cause ordering. Lower is nearer the root.
    ///
    /// Section 7's offline fallback names five of the ten; the other five are placed by the same
    /// argument. Coverage and duplicates sit at the end because they are facts about the *set*
    /// rather than about the frame, so remediating one of them cannot change what any other check
    /// measures - which makes them safe to leave until last and pointless to do first.
    #[must_use]
    pub const fn triage_rank(self) -> u8 {
        match self {
            // The light was wrong. Everything downstream inherits it.
            Self::Consistency => 0,
            Self::Exposure => 1,
            Self::Skin => 2,
            // The frame's geometry, before anything that measures inside it.
            Self::Crop => 3,
            // Operations that changed pixels, root-most first.
            Self::Cleanup => 4,
            Self::Mask => 5,
            Self::Retouch => 6,
            Self::Sharpness => 7,
            // Facts about the gallery rather than about the frame.
            Self::Duplicate => 8,
            Self::Coverage => 9,
        }
    }

    /// The unit this category's deviation is measured in.
    ///
    /// On the wire and in the report, because "4.2" means nothing without it and a panel that
    /// hard-coded the units would go stale the first time a check's formulation changed.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub const fn unit(self) -> &'static str {
        // Ten arms rather than five, deliberately. Consistency and Skin are both stated in dE00
        // and are not the same answer: one is a distance from a lighting group and the other a
        // distance from a person's own appearance across the gallery. Merging the arms would let
        // a later change to one silently change the other.
        match self {
            Self::Consistency => "dE00",
            Self::Skin => "dE00",
            Self::Exposure => "EV",
            Self::Sharpness | Self::Retouch => "ratio",
            Self::Mask | Self::Crop | Self::Cleanup => "fraction",
            Self::Duplicate => "hamming",
            Self::Coverage => "frames",
        }
    }

    /// True when a finding in this category is about the gallery rather than about one frame.
    ///
    /// Coverage and duplicates. It matters because these two are the only categories whose ticket
    /// can be resolved by changing a *different* photograph, so the loop's "re-inspect this
    /// ticket's metric only" rule has to re-run them over the set.
    #[must_use]
    pub const fn is_gallery_scoped(self) -> bool {
        matches!(self, Self::Coverage | Self::Duplicate)
    }
}

impl fmt::Display for QcCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Reason codes
// ---------------------------------------------------------------------------

/// Every reason this phase can give, as a code rather than a sentence.
///
/// Phase 09's rule and the one this phase most needed to inherit: **a reason stores its code, never
/// its copy.** A QC ticket is the most user-facing sentence the product produces, which makes it
/// the sentence most likely to be rewritten between releases - and a studio archiving QC reports
/// would otherwise end up with two weddings whose identical findings read differently because
/// somebody improved the wording. ADR-0055 section 3.
///
/// Forty codes in four groups: what was found, what was done, what was refused, and what could not
/// be checked. The fourth group is the one that makes this phase honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QcCode {
    // -- Findings: consistency and colour -------------------------------------------------
    /// Colour or tone sits outside this frame's scene node by more than the node's tolerance.
    ConsistencyDrift,
    /// The frame's grade character disagrees with its node's anchors.
    SignatureDrift,
    /// A person's skin moved away from their own gallery target.
    SkinDrift,
    /// Skin hue or chroma moved further than phase 16's guard permits.
    SkinGuardExceeded,
    // -- Findings: exposure ---------------------------------------------------------------
    /// The edited frame's subject luminance is outside its scene band.
    ExposureRegression,
    /// The edit introduced clipping that the original did not have.
    ClippingIntroduced,
    /// The edit crushed shadow detail past phase 09's budget.
    ShadowsCrushed,
    // -- Findings: sharpness and restoration ----------------------------------------------
    /// Subject sharpness after restoration is below this scene's floor.
    SharpnessBelowFloor,
    /// Sharpening left an excursion beyond the neighbourhood's own range.
    RingingDetected,
    /// Denoising removed more texture than the scene ceiling allows.
    TextureLost,
    /// A recovered face moved further from its own identity than the ceiling allows.
    IdentityDrift,
    // -- Findings: retouch -----------------------------------------------------------------
    /// Phase 20's texture floor was not met on the delivered frame.
    TextureFloorMissed,
    /// Phase 21's naturalness guard withdrew an operation family and the frame still reads worked.
    NaturalnessMissed,
    /// The per-image perceptual allowance was spent past its cap.
    AllowanceExceeded,
    // -- Findings: masks -------------------------------------------------------------------
    /// An edit was applied through a boundary whose quality does not support it.
    MaskEdgeArtefact,
    /// A region's confidence is below the floor its operation needed.
    MaskQualityLow,
    /// A local operation ran with no region covering it.
    MaskUncovered,
    // -- Findings: crop and geometry ---------------------------------------------------------
    /// The delivered crop fails one of phase 23's safety checks.
    CropUnsafe,
    /// The delivered crop is below the resolution floor for its purpose.
    CropResolutionLow,
    /// The delivered crop dropped more of the frame's content than the rules allow.
    CropContentLost,
    // -- Findings: cleanup -------------------------------------------------------------------
    /// A generative removal left a measurable artefact.
    CleanupArtefact,
    /// A removal reached the gallery without a disclosure row.
    CleanupUndisclosed,
    // -- Findings: the set --------------------------------------------------------------------
    /// Two frames close enough to be duplicates are both in the gallery.
    DuplicateLeak,
    /// A must-have rule is not covered in the delivered gallery.
    CoverageMissing,
    /// A must-have rule is covered only by a frame that lost a veto.
    CoverageWeak,
    /// An identity appears fewer times than the minimum.
    IdentityUnderCovered,
    // -- Findings: the image as a whole --------------------------------------------------------
    /// More open tickets than any single remedy can address.
    MultiSymptom,
    /// Two findings that cannot both be remediated without one undoing the other.
    ContradictoryFindings,
    // -- Outcomes: what was done ----------------------------------------------------------------
    /// A remedy was applied and re-inspection confirmed the gain.
    RemedyApplied,
    /// A remedy was applied, did not realise its predicted gain, and was put back.
    RemedyReverted,
    /// A remedy improved its own metric and worsened another check past tolerance.
    CollateralDamage,
    /// The frame was replaced by its runner-up.
    ReplacedWithRunnerUp,
    /// Two rounds were spent and the finding stands.
    RoundsExhausted,
    /// The finding was handed to a person.
    EscalatedToHuman,
    // -- Outcomes: what was refused ---------------------------------------------------------------
    /// A replacement was refused because it would have left a must-have uncovered.
    ReplacementBreaksCoverage,
    /// There was no runner-up: the moment held one frame.
    RunnerUpAbsent,
    /// The runner-up's own post-edit metrics were not clearly better.
    RunnerUpNotBetter,
    /// The proposed remedy is not permitted by policy for this category or scene.
    RemedyRefusedByPolicy,
    /// The photographer edited this field, so nothing here may change it.
    UserEdited,
    // -- Outcomes: what could not be checked -------------------------------------------------------
    /// The inspection's input was not present, so the check did not run.
    CheckSkipped,
    /// The planner was asked and could not answer, so mechanical triage stands.
    PlannerUnavailable,
    /// The planner answered and its plan did not survive policy validation.
    PlannerRefused,
    /// The pass ran out of its wall-clock budget before reaching this image.
    BudgetSpent,
}

impl QcCode {
    /// Every code, in declaration order.
    pub const ALL: [Self; 43] = [
        Self::ConsistencyDrift,
        Self::SignatureDrift,
        Self::SkinDrift,
        Self::SkinGuardExceeded,
        Self::ExposureRegression,
        Self::ClippingIntroduced,
        Self::ShadowsCrushed,
        Self::SharpnessBelowFloor,
        Self::RingingDetected,
        Self::TextureLost,
        Self::IdentityDrift,
        Self::TextureFloorMissed,
        Self::NaturalnessMissed,
        Self::AllowanceExceeded,
        Self::MaskEdgeArtefact,
        Self::MaskQualityLow,
        Self::MaskUncovered,
        Self::CropUnsafe,
        Self::CropResolutionLow,
        Self::CropContentLost,
        Self::CleanupArtefact,
        Self::CleanupUndisclosed,
        Self::DuplicateLeak,
        Self::CoverageMissing,
        Self::CoverageWeak,
        Self::IdentityUnderCovered,
        Self::MultiSymptom,
        Self::ContradictoryFindings,
        Self::RemedyApplied,
        Self::RemedyReverted,
        Self::CollateralDamage,
        Self::ReplacedWithRunnerUp,
        Self::RoundsExhausted,
        Self::EscalatedToHuman,
        Self::ReplacementBreaksCoverage,
        Self::RunnerUpAbsent,
        Self::RunnerUpNotBetter,
        Self::RemedyRefusedByPolicy,
        Self::UserEdited,
        Self::CheckSkipped,
        Self::PlannerUnavailable,
        Self::PlannerRefused,
        Self::BudgetSpent,
    ];

    /// How many there are.
    pub const COUNT: usize = 43;

    /// The stored form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConsistencyDrift => "consistency_drift",
            Self::SignatureDrift => "signature_drift",
            Self::SkinDrift => "skin_drift",
            Self::SkinGuardExceeded => "skin_guard_exceeded",
            Self::ExposureRegression => "exposure_regression",
            Self::ClippingIntroduced => "clipping_introduced",
            Self::ShadowsCrushed => "shadows_crushed",
            Self::SharpnessBelowFloor => "sharpness_below_floor",
            Self::RingingDetected => "ringing_detected",
            Self::TextureLost => "texture_lost",
            Self::IdentityDrift => "identity_drift",
            Self::TextureFloorMissed => "texture_floor_missed",
            Self::NaturalnessMissed => "naturalness_missed",
            Self::AllowanceExceeded => "allowance_exceeded",
            Self::MaskEdgeArtefact => "mask_edge_artefact",
            Self::MaskQualityLow => "mask_quality_low",
            Self::MaskUncovered => "mask_uncovered",
            Self::CropUnsafe => "crop_unsafe",
            Self::CropResolutionLow => "crop_resolution_low",
            Self::CropContentLost => "crop_content_lost",
            Self::CleanupArtefact => "cleanup_artefact",
            Self::CleanupUndisclosed => "cleanup_undisclosed",
            Self::DuplicateLeak => "duplicate_leak",
            Self::CoverageMissing => "coverage_missing",
            Self::CoverageWeak => "coverage_weak",
            Self::IdentityUnderCovered => "identity_under_covered",
            Self::MultiSymptom => "multi_symptom",
            Self::ContradictoryFindings => "contradictory_findings",
            Self::RemedyApplied => "remedy_applied",
            Self::RemedyReverted => "remedy_reverted",
            Self::CollateralDamage => "collateral_damage",
            Self::ReplacedWithRunnerUp => "replaced_with_runner_up",
            Self::RoundsExhausted => "rounds_exhausted",
            Self::EscalatedToHuman => "escalated_to_human",
            Self::ReplacementBreaksCoverage => "replacement_breaks_coverage",
            Self::RunnerUpAbsent => "runner_up_absent",
            Self::RunnerUpNotBetter => "runner_up_not_better",
            Self::RemedyRefusedByPolicy => "remedy_refused_by_policy",
            Self::UserEdited => "user_edited",
            Self::CheckSkipped => "check_skipped",
            Self::PlannerUnavailable => "planner_unavailable",
            Self::PlannerRefused => "planner_refused",
            Self::BudgetSpent => "budget_spent",
        }
    }

    /// Parse the stored form.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|code| code.as_str() == text)
    }

    /// Which category this code belongs to, when it belongs to exactly one.
    ///
    /// `None` for the outcome and refusal codes, which can appear under any category. A code that
    /// pretended to belong to one would make `QcReport::by_category` count a revert as a finding.
    #[must_use]
    pub const fn category(self) -> Option<QcCategory> {
        match self {
            Self::ConsistencyDrift | Self::SignatureDrift => Some(QcCategory::Consistency),
            Self::SkinDrift | Self::SkinGuardExceeded => Some(QcCategory::Skin),
            Self::ExposureRegression | Self::ClippingIntroduced | Self::ShadowsCrushed => {
                Some(QcCategory::Exposure)
            }
            Self::SharpnessBelowFloor
            | Self::RingingDetected
            | Self::TextureLost
            | Self::IdentityDrift => Some(QcCategory::Sharpness),
            Self::TextureFloorMissed | Self::NaturalnessMissed | Self::AllowanceExceeded => {
                Some(QcCategory::Retouch)
            }
            Self::MaskEdgeArtefact | Self::MaskQualityLow | Self::MaskUncovered => {
                Some(QcCategory::Mask)
            }
            Self::CropUnsafe | Self::CropResolutionLow | Self::CropContentLost => {
                Some(QcCategory::Crop)
            }
            Self::CleanupArtefact | Self::CleanupUndisclosed => Some(QcCategory::Cleanup),
            Self::DuplicateLeak => Some(QcCategory::Duplicate),
            Self::CoverageMissing | Self::CoverageWeak | Self::IdentityUnderCovered => {
                Some(QcCategory::Coverage)
            }
            _ => None,
        }
    }

    /// True when this code opens a ticket rather than describing what happened to one.
    ///
    /// The gate counts findings with this and nothing else, so a build that started filing
    /// `RemedyReverted` as a defect would fail its own detection gate rather than inflate it.
    #[must_use]
    pub const fn is_finding(self) -> bool {
        self.category().is_some()
            || matches!(self, Self::MultiSymptom | Self::ContradictoryFindings)
    }

    /// True when this code records that an inspection could not run.
    ///
    /// ADR-0055 section 8: an absent input is a skipped check and not a passed one, and the two
    /// must never be one query.
    #[must_use]
    pub const fn is_unavailable(self) -> bool {
        matches!(
            self,
            Self::CheckSkipped
                | Self::PlannerUnavailable
                | Self::PlannerRefused
                | Self::BudgetSpent
                | Self::MaskUncovered
        )
    }

    /// True when this code means nothing was changed and nothing will be.
    #[must_use]
    pub const fn is_refusal(self) -> bool {
        matches!(
            self,
            Self::ReplacementBreaksCoverage
                | Self::RunnerUpAbsent
                | Self::RunnerUpNotBetter
                | Self::RemedyRefusedByPolicy
                | Self::UserEdited
        )
    }

    /// The sentence a photographer reads, with the numbers left to the caller.
    ///
    /// Present tense, no jargon, and it never says "error". A QC report is read by somebody
    /// deciding whether to trust a delivery, and a page of failures reads as a broken product even
    /// when every line is a small correction the product made on purpose.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::ConsistencyDrift => {
                "this frame's colour sits outside the rest of its lighting group"
            }
            Self::SignatureDrift => "this frame is graded differently from its reference frames",
            Self::SkinDrift => {
                "somebody's skin here does not match how they look elsewhere in the gallery"
            }
            Self::SkinGuardExceeded => "the grade moved skin further than AURA allows",
            Self::ExposureRegression => {
                "the finished frame is brighter or darker than this kind of scene should be"
            }
            Self::ClippingIntroduced => {
                "the edit clipped highlights or shadows the original still had"
            }
            Self::ShadowsCrushed => "the edit lost shadow detail the original held",
            Self::SharpnessBelowFloor => "the subject is softer than this kind of photograph needs",
            Self::RingingDetected => "sharpening left a bright halo along an edge",
            Self::TextureLost => "noise reduction took more texture than it should have",
            Self::IdentityDrift => {
                "a recovered face moved further from the person than AURA permits"
            }
            Self::TextureFloorMissed => "the skin here has less texture than AURA's floor allows",
            Self::NaturalnessMissed => "the retouching on this frame reads as worked on",
            Self::AllowanceExceeded => {
                "more local adjustments were spent on this frame than its budget"
            }
            Self::MaskEdgeArtefact => {
                "an adjustment shows along the edge of the region it was applied to"
            }
            Self::MaskQualityLow => {
                "a region was not determined well enough for what was done inside it"
            }
            Self::MaskUncovered => "a local adjustment ran with no region behind it",
            Self::CropUnsafe => "the delivered crop cuts something it should not",
            Self::CropResolutionLow => "the delivered crop is smaller than this use needs",
            Self::CropContentLost => {
                "the delivered crop drops more of the frame than the rules allow"
            }
            Self::CleanupArtefact => "a removal left a visible mark",
            Self::CleanupUndisclosed => "a removal reached the gallery without a record of it",
            Self::DuplicateLeak => "two nearly identical frames are both in the gallery",
            Self::CoverageMissing => "a moment the gallery has to include is not covered",
            Self::CoverageWeak => "a moment is covered only by a photograph that did not work",
            Self::IdentityUnderCovered => {
                "somebody appears fewer times than the gallery guarantees"
            }
            Self::MultiSymptom => "this frame has more problems than one correction can fix",
            Self::ContradictoryFindings => "two findings on this frame cannot both be fixed",
            Self::RemedyApplied => "AURA corrected this and checked the correction worked",
            Self::RemedyReverted => "AURA tried a correction, it did not help, and it was put back",
            Self::CollateralDamage => {
                "the correction fixed this and made something else worse, so it was put back"
            }
            Self::ReplacedWithRunnerUp => {
                "AURA delivered a better frame from the same moment instead"
            }
            Self::RoundsExhausted => "AURA tried twice and this still needs your eyes",
            Self::EscalatedToHuman => "this one needs your eyes",
            Self::ReplacementBreaksCoverage => {
                "a better frame exists but swapping to it would leave a moment uncovered"
            }
            Self::RunnerUpAbsent => {
                "there is no alternative frame: this moment was photographed once"
            }
            Self::RunnerUpNotBetter => "the alternative frame is not clearly better than this one",
            Self::RemedyRefusedByPolicy => {
                "the correction this needs is not one AURA is allowed to make here"
            }
            Self::UserEdited => "you set this yourself, so AURA has left it alone",
            Self::CheckSkipped => {
                "AURA could not check this, so it has not made a claim either way"
            }
            Self::PlannerUnavailable => {
                "the second opinion was unavailable, so AURA used its own ordering"
            }
            Self::PlannerRefused => {
                "the second opinion proposed something AURA is not allowed to do, so it was ignored"
            }
            Self::BudgetSpent => "the quality pass ran out of time before reaching this photograph",
        }
    }
}

impl fmt::Display for QcCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One reason, with its weight and what it points at.
///
/// The same shape phases 09 to 26 use, for the same reason: a code, a weight and evidence, and the
/// sentence assembled on read.
#[derive(Debug, Clone, PartialEq)]
pub struct QcReason {
    /// Which reason.
    pub code: QcCode,
    /// How much it contributed, `-1..1`. Negative argues against the finding.
    pub weight: f32,
    /// What to look at.
    pub evidence: Evidence,
}

impl QcReason {
    /// A reason with no evidence.
    #[must_use]
    pub const fn new(code: QcCode, weight: f32) -> Self {
        Self {
            code,
            weight,
            evidence: Evidence::None,
        }
    }

    /// A reason pointing at a region of the frame.
    #[must_use]
    pub const fn at(code: QcCode, weight: f32, crop: Box2) -> Self {
        Self {
            code,
            weight,
            evidence: Evidence::Crop(crop),
        }
    }

    /// A reason pointing at other photographs - the anchors, the runner-up, the duplicate.
    #[must_use]
    pub fn against(code: QcCode, weight: f32, frames: Vec<ImageId>) -> Self {
        Self {
            code,
            weight,
            evidence: Evidence::Frames(frames),
        }
    }

    /// The sentence.
    #[must_use]
    pub const fn text(&self) -> &'static str {
        self.code.user_text()
    }
}

/// What a reason points at.
///
/// Deliberately identical in shape to [`crate::contract::ledger::Evidence`] and deliberately a
/// separate type: this one adds [`Evidence::Anchors`], which names the scene node's reference
/// frames beside the frames themselves, and the ledger's variant list is frozen under phase 13.
///
/// **There is no variant that could hold image bytes.** Phase 13's rule, inherited without change:
/// what makes "a QC report contains no pixels" a property of the shape rather than a promise about
/// an exporter.
#[derive(Debug, Clone, PartialEq)]
pub enum Evidence {
    /// Nothing to point at.
    None,
    /// A region of this frame, in normalised coordinates.
    Crop(Box2),
    /// Other photographs.
    Frames(Vec<ImageId>),
    /// This frame's scene-node anchors - the frames the deviation was measured against.
    ///
    /// Separate from [`Evidence::Frames`] because a panel renders them differently: an anchor is
    /// what the finding is *relative to*, and showing it in the same strip as a duplicate or a
    /// runner-up would put two opposite meanings behind the same thumbnail.
    Anchors(Vec<ImageId>),
    /// Named numbers, as `(name, value)`.
    Params(Vec<(String, f32)>),
}

impl Evidence {
    /// The most items one evidence list carries.
    pub const MAX_ITEMS: usize = 8;

    /// The stored discriminant.
    #[must_use]
    pub const fn kind_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Crop(_) => "crop",
            Self::Frames(_) => "frames",
            Self::Anchors(_) => "anchors",
            Self::Params(_) => "params",
        }
    }

    /// True when there is nothing to look at.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::None => true,
            Self::Crop(_) => false,
            Self::Frames(list) | Self::Anchors(list) => list.is_empty(),
            Self::Params(list) => list.is_empty(),
        }
    }

    /// The same evidence with its lists truncated to [`Evidence::MAX_ITEMS`].
    #[must_use]
    pub fn bounded(mut self) -> Self {
        match &mut self {
            Self::Frames(list) | Self::Anchors(list) => list.truncate(Self::MAX_ITEMS),
            Self::Params(list) => list.truncate(Self::MAX_ITEMS),
            Self::None | Self::Crop(_) => {}
        }
        self
    }
}

// ---------------------------------------------------------------------------
// Remedies
// ---------------------------------------------------------------------------

/// Which phase's decision a [`Remedy::ResolveParam`] re-runs.
///
/// Section 5 types this field as phase 13's `DecisionKind`, which has six variants covering the
/// whole product - `Cull | Edit | Retouch | Qc | Curate | Export`. That vocabulary cannot say
/// *which* edit to re-solve, and "re-run the edit" over a frame that needs its white balance
/// re-solved would re-run the grade, the local light and the geometry with it.
///
/// This is the narrower vocabulary the remedy actually needs. The ledger still records the remedy
/// under `DecisionKind::Qc`, so nothing about phase 13's model changes. ADR-0055 section 5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveTarget {
    /// Phase 15's illuminant and exposure.
    WhiteBalance,
    /// Phase 15's exposure alone.
    Exposure,
    /// Phase 16's tone and colour grade.
    Grade,
    /// Phase 25's normalisation toward the scene node.
    Normalisation,
    /// Phase 22's denoise and sharpen amounts.
    Restoration,
    /// Phase 23's crop selection.
    Crop,
}

impl SolveTarget {
    /// Every target.
    pub const ALL: [Self; 6] = [
        Self::WhiteBalance,
        Self::Exposure,
        Self::Grade,
        Self::Normalisation,
        Self::Restoration,
        Self::Crop,
    ];

    /// How many there are.
    pub const COUNT: usize = 6;

    /// The stored form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WhiteBalance => "white_balance",
            Self::Exposure => "exposure",
            Self::Grade => "grade",
            Self::Normalisation => "normalisation",
            Self::Restoration => "restoration",
            Self::Crop => "crop",
        }
    }

    /// Parse the stored form.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }
}

impl fmt::Display for SolveTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What may be done about a finding. Section 5's frozen enum.
///
/// Five variants and no sixth. There is no `Adjust { param, value }`, no `SetStrength` above one
/// and no free-form operation, because every one of those would make this phase a place a
/// photograph could be edited from - and phase 14's rule is that a delivered file is re-creatable
/// from four values, one of which is a recipe written by exactly one function.
#[derive(Debug, Clone, PartialEq)]
pub enum Remedy {
    /// Re-run one phase's decision for this frame, under an extra constraint.
    ///
    /// The constraint is a sentence the deciding phase's own solver understands - "hold the
    /// illuminant, move only the exposure" - and it narrows rather than overrides. A remedy that
    /// could *set* a parameter would be an edit; one that can only add a constraint is a request.
    ResolveParam {
        /// Which decision to re-run.
        target: SolveTarget,
        /// What to hold fixed while re-running it.
        constraint: String,
    },
    /// Multiply one operation's strength by a factor strictly inside
    /// `MIN_STRENGTH_FACTOR..MAX_STRENGTH_FACTOR`.
    ReduceStrength {
        /// The operation's stable name, as its owning phase spells it.
        op: String,
        /// The multiplier. Always below one: this remedy reduces.
        factor: f32,
    },
    /// Switch one operation off for this frame.
    RevertOp {
        /// The operation's stable name.
        op: String,
    },
    /// Deliver a different frame from the same moment.
    ReplaceFrame {
        /// The runner-up phase 12 already chose.
        with: ImageId,
    },
    /// Hand it to a person.
    Escalate {
        /// One sentence about what to look at. Rendered, never stored - see
        /// [`QcTicket::render_diagnosis`].
        note: String,
    },
}

impl Remedy {
    /// The stored discriminant.
    #[must_use]
    pub const fn kind_str(&self) -> &'static str {
        match self {
            Self::ResolveParam { .. } => "resolve_param",
            Self::ReduceStrength { .. } => "reduce_strength",
            Self::RevertOp { .. } => "revert_op",
            Self::ReplaceFrame { .. } => "replace_frame",
            Self::Escalate { .. } => "escalate",
        }
    }

    /// Every discriminant the planner's schema may name. Section 7's enum, verbatim.
    pub const KINDS: [&'static str; 5] = [
        "resolve_param",
        "reduce_strength",
        "revert_op",
        "replace_frame",
        "escalate",
    ];

    /// True when applying this changes what a delivered photograph looks like.
    ///
    /// [`Remedy::Escalate`] is the only one that does not, which is what makes it the safe answer
    /// whenever anything is uncertain.
    #[must_use]
    pub const fn mutates(&self) -> bool {
        !matches!(self, Self::Escalate { .. })
    }

    /// The lowest confidence at which this remedy may be applied without a person.
    ///
    /// Section 6.4: replacements require more than parameter fixes.
    #[must_use]
    pub const fn confidence_floor(&self) -> f32 {
        match self {
            Self::ReplaceFrame { .. } => REPLACE_CONFIDENCE_FLOOR,
            Self::Escalate { .. } => 0.0,
            _ => FIX_CONFIDENCE_FLOOR,
        }
    }

    /// Which other checks this remedy can move, and therefore which ones must be re-run.
    ///
    /// A `const fn` on the enum rather than a row in a configuration file, because it is a fact
    /// about what the operation touches. ADR-0055 section 5: a studio that shortened the list would
    /// produce a pass that still ran, still reported and silently stopped noticing one class of
    /// damage.
    ///
    /// [`Remedy::ReplaceFrame`] returns every category, because a different photograph is a
    /// different answer to all ten questions.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub fn collateral_checks(&self) -> Vec<QcCategory> {
        // Some remedies happen to disturb the same set of checks today. They are separate arms
        // because they disturb them for different reasons, and the next person to add an
        // operation should be reading one rule per remedy rather than a merged list.
        match self {
            Self::ResolveParam { target, .. } => match target {
                SolveTarget::WhiteBalance => vec![
                    QcCategory::Consistency,
                    QcCategory::Skin,
                    QcCategory::Exposure,
                ],
                SolveTarget::Exposure => vec![
                    QcCategory::Consistency,
                    QcCategory::Exposure,
                    QcCategory::Skin,
                ],
                SolveTarget::Grade => vec![
                    QcCategory::Consistency,
                    QcCategory::Skin,
                    QcCategory::Exposure,
                ],
                SolveTarget::Normalisation => {
                    vec![QcCategory::Consistency, QcCategory::Skin]
                }
                SolveTarget::Restoration => {
                    vec![QcCategory::Sharpness, QcCategory::Retouch]
                }
                SolveTarget::Crop => vec![QcCategory::Crop, QcCategory::Cleanup],
            },
            Self::ReduceStrength { .. } | Self::RevertOp { .. } => vec![
                QcCategory::Retouch,
                QcCategory::Sharpness,
                QcCategory::Mask,
                QcCategory::Cleanup,
            ],
            // A different photograph is a different answer to every question.
            Self::ReplaceFrame { .. } => QcCategory::ALL.to_vec(),
            // Nothing changed, so nothing can have been made worse.
            Self::Escalate { .. } => Vec::new(),
        }
    }

    /// True when this remedy's own fields are inside the contract's bounds.
    ///
    /// The one place a magnitude is checked. `aura_qc::remedy::validate` calls it on every remedy
    /// including the ones the planner proposed, which is what stops a model inventing a strength
    /// of 3.0 and a caller applying it.
    #[must_use]
    pub fn is_within_bounds(&self) -> bool {
        match self {
            Self::ReduceStrength { factor, op } => {
                !op.trim().is_empty()
                    && factor.is_finite()
                    && *factor >= MIN_STRENGTH_FACTOR
                    && *factor <= MAX_STRENGTH_FACTOR
            }
            Self::RevertOp { op } => !op.trim().is_empty(),
            Self::ResolveParam { constraint, .. } => !constraint.trim().is_empty(),
            Self::ReplaceFrame { .. } | Self::Escalate { .. } => true,
        }
    }
}

// ---------------------------------------------------------------------------
// Ticket status
// ---------------------------------------------------------------------------

/// Where a ticket stands.
///
/// Section 5 lists five: `Open | Fixed | Reverted | Escalated | Accepted`. This is six, and the
/// sixth is a contract amendment recorded in ADR-0055 section 9 - the fifth in the product's
/// history after phase 09's `FaceRef`, phase 16's re-lock, phase 23's `Lens::coefficients` and
/// phase 24's `Recipe.cleanup[]`.
///
/// Section 11's telemetry names `qc.user_disagree` and there is nowhere in the five to record it.
/// [`TicketStatus::Accepted`] is a photographer agreeing with a finding; a photographer who thinks
/// the finding is wrong is a different row, and folding the two together would make the false-ticket
/// rate section 10.1 gates unmeasurable from the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    /// Found, not yet acted on.
    Open,
    /// A remedy was applied and re-inspection confirmed it.
    Fixed,
    /// A remedy was applied and put back.
    Reverted,
    /// Handed to a person.
    Escalated,
    /// A person looked and agreed with the finding.
    Accepted,
    /// A person looked and disagreed with the finding.
    ///
    /// The one status automation may never write, and the one it may never clear. A ticket a
    /// photographer has rejected that reappears on the next pass is a product arguing with its
    /// user - which is `user_edited`'s rule everywhere else in this schema, in the one place the
    /// disagreement is about a *judgement* rather than about a value.
    Dismissed,
}

impl TicketStatus {
    /// Every status.
    pub const ALL: [Self; 6] = [
        Self::Open,
        Self::Fixed,
        Self::Reverted,
        Self::Escalated,
        Self::Accepted,
        Self::Dismissed,
    ];

    /// How many there are.
    pub const COUNT: usize = 6;

    /// The stored form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Fixed => "fixed",
            Self::Reverted => "reverted",
            Self::Escalated => "escalated",
            Self::Accepted => "accepted",
            Self::Dismissed => "dismissed",
        }
    }

    /// Parse the stored form.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    /// True when a person set this.
    #[must_use]
    pub const fn is_user_set(self) -> bool {
        matches!(self, Self::Accepted | Self::Dismissed)
    }

    /// True when this ticket still wants somebody's attention.
    #[must_use]
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Open | Self::Escalated | Self::Reverted)
    }

    /// True when automation may still change this ticket.
    ///
    /// What `QcStore::sweep` checks before re-analysing. A photographer's own verdict survives a
    /// re-run of the pass, exactly as `user_edited` does in migrations 06 to 26.
    #[must_use]
    pub const fn is_automatable(self) -> bool {
        !self.is_user_set()
    }
}

impl fmt::Display for TicketStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// The ticket
// ---------------------------------------------------------------------------

/// One quality-control finding, and everything needed to argue with it.
///
/// Section 5's frozen shape, with [`QcTicket::project`], [`QcTicket::scene`],
/// [`QcTicket::reasons`], [`QcTicket::code`], [`QcTicket::outcome_code`] and
/// [`QcTicket::created_at`] added; see this module's header and ADR-0055.
///
/// ## `diagnosis` is a method rather than a column
///
/// Section 5 types `diagnosis` as a `String` and shows an example -
/// `"bride face 4.2 dE00 magenta vs node anchors #817/#819/#825"`. It is on this struct and it is
/// **not stored**. [`QcTicket::render_diagnosis`] builds it from the code, the deviation, the
/// threshold and the evidence on every read.
///
/// Phase 09's rule, and it matters more here than it did there: a QC ticket is the most user-facing
/// sentence the product produces, so it is the sentence most likely to be rewritten between
/// releases - and a studio archiving QC reports would otherwise hold two weddings whose identical
/// findings read differently because somebody improved the wording. ADR-0055 section 3.
#[derive(Debug, Clone, PartialEq)]
pub struct QcTicket {
    /// This ticket's identity. Assigned once, never reused.
    pub id: TicketId,
    /// Which wedding.
    pub project: ProjectId,
    /// Which photograph.
    pub image_id: ImageId,
    /// Which inspection found it.
    pub category: QcCategory,
    /// What exactly was found. Always a code with [`QcCode::is_finding`] true.
    pub code: QcCode,
    /// How far from acceptable, in [`QcCategory::unit`].
    pub deviation: f32,
    /// What acceptable was, in the same unit.
    pub threshold: f32,
    /// What to look at.
    pub evidence: Evidence,
    /// The person this finding is about, when it is about one.
    ///
    /// Set by the skin, retouch and sharpness checks. `None` for everything measured over the whole
    /// frame or over the set.
    pub identity: Option<IdentityId>,
    /// What should be done about it.
    pub remedy: Remedy,
    /// How much the deviation is predicted to fall if the remedy is applied, in the same unit.
    ///
    /// The number [`QcRound::realised_share`] is measured against. A prediction rather than a
    /// promise, which is exactly why the loop checks it.
    pub expected_gain: f32,
    /// How sure this finding is, `0..1`. Invariant 2.
    pub confidence: f32,
    /// What the product was allowed to do about it. Phase 13's bands.
    pub autonomy: Autonomy,
    /// Why, strongest first. Never empty, at most [`MAX_REASONS`]. Invariant 2.
    pub reasons: Vec<QcReason>,
    /// Which remediation round this ticket is on. Never above [`MAX_ROUNDS`].
    pub round: u8,
    /// Where it stands.
    pub status: TicketStatus,
    /// What happened to it, when something has.
    ///
    /// One of the outcome or refusal codes. `None` while the ticket is open, which is what makes
    /// "this was never acted on" a different query from "this was acted on and nothing changed".
    pub outcome_code: Option<QcCode>,
    /// The scene the frame was in, for the per-scene thresholds.
    pub scene: SceneId,
    /// When it was opened.
    pub created_at: Timestamp,
    /// Which thresholds table produced it.
    pub thresholds_ver: u16,
    /// Which arithmetic produced it.
    pub analysis_ver: u16,
}

impl QcTicket {
    /// The most reasons one ticket carries. See [`MAX_REASONS`].
    pub const MAX_REASONS: usize = MAX_REASONS;

    /// True when this ticket can be recorded at all.
    ///
    /// Invariant 2, as one call: a finding with a code, a number, a threshold and at least one
    /// reason. Migration 27 checks the same thing again with CHECK constraints, because a promise
    /// enforced in one layer lasts until somebody writes a second caller.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.code.is_finding()
            && self.deviation.is_finite()
            && self.threshold.is_finite()
            && self.threshold > 0.0
            && self.expected_gain.is_finite()
            && self.expected_gain >= 0.0
            && (0.0..=1.0).contains(&self.confidence)
            && !self.reasons.is_empty()
            && self.reasons.len() <= Self::MAX_REASONS
            && self.round <= MAX_ROUNDS
            && self.remedy.is_within_bounds()
    }

    /// How far past the threshold this finding sits, as a multiple of the threshold.
    ///
    /// The number the queue sorts on. A ratio rather than a difference, because the ten categories
    /// are measured in five units and a queue ordered by raw deviation would put every dE00 finding
    /// above every EV one for no reason but the scale.
    #[must_use]
    pub fn severity(&self) -> f32 {
        if self.threshold <= 0.0 || !self.deviation.is_finite() {
            return 0.0;
        }
        (self.deviation / self.threshold).max(0.0)
    }

    /// True when this finding may be acted on without a person.
    ///
    /// Both halves: the confidence clears the remedy's own floor **and** the autonomy band permits
    /// it. Phase 13 owns the band and this phase owns the floor, and neither alone is enough - a
    /// high-confidence replacement in a band that requires review is still a replacement that waits.
    #[must_use]
    pub fn may_act_unattended(&self) -> bool {
        !self.autonomy.needs_review() && self.confidence >= self.remedy.confidence_floor()
    }

    /// True when another round is permitted.
    #[must_use]
    pub const fn may_retry(&self) -> bool {
        self.round < MAX_ROUNDS
    }

    /// The strongest reasons by absolute weight, up to `limit`.
    ///
    /// Ties break on the code so two equally weighted reasons come out in the same order on every
    /// machine - the rule `aura_cull::explain::rank` and phase 13's ledger both use.
    #[must_use]
    pub fn top_reasons(&self, limit: usize) -> Vec<&QcReason> {
        let mut ordered: Vec<&QcReason> = self.reasons.iter().collect();
        ordered.sort_by(|left, right| {
            right
                .weight
                .abs()
                .total_cmp(&left.weight.abs())
                .then_with(|| left.code.cmp(&right.code))
        });
        ordered.truncate(limit);
        ordered
    }

    /// The senior retoucher's note, assembled from the stored numbers.
    ///
    /// Section 5's `diagnosis`, rendered rather than stored. The shape is always
    /// `<what> <deviation><unit> against a <threshold><unit> threshold`, followed by what it was
    /// measured against when the evidence names it.
    ///
    /// Deterministic: two builds with the same row produce the same sentence, which is what lets
    /// `tests/eval/qc_eval.rs` assert on it.
    #[must_use]
    pub fn render_diagnosis(&self) -> String {
        let unit = self.category.unit();
        let mut out = format!(
            "{}: {:.2} {unit} against a {:.2} {unit} threshold",
            self.code.user_text(),
            self.deviation,
            self.threshold
        );
        match &self.evidence {
            Evidence::Anchors(frames) if !frames.is_empty() => {
                let _ = write!(out, ", measured against {} anchor", frames.len());
                if frames.len() != 1 {
                    out.push('s');
                }
            }
            Evidence::Frames(frames) if !frames.is_empty() => {
                let _ = write!(out, ", compared with {} frame", frames.len());
                if frames.len() != 1 {
                    out.push('s');
                }
            }
            Evidence::Crop(_) => out.push_str(", in the marked region"),
            Evidence::Params(_) | Evidence::None | Evidence::Anchors(_) | Evidence::Frames(_) => {}
        }
        out
    }

    /// Order two tickets for the queue: worst first, then by root cause, then by id.
    ///
    /// Severity leads because a photographer clearing a queue wants the worst frames first.
    /// [`QcCategory::triage_rank`] breaks the tie because two equally severe findings on one frame
    /// should be shown cause before symptom. The id breaks the last tie so the order is identical
    /// on every machine.
    #[must_use]
    pub fn queue_order(&self, other: &Self) -> Ordering {
        other
            .severity()
            .total_cmp(&self.severity())
            .then_with(|| {
                self.category
                    .triage_rank()
                    .cmp(&other.category.triage_rank())
            })
            .then_with(|| self.id.cmp(&other.id))
    }
}

// ---------------------------------------------------------------------------
// Rounds
// ---------------------------------------------------------------------------

/// One remediation attempt, and whether it worked.
///
/// Not in section 5; section 6.3 requires it - "all rounds are recorded in the ledger so the
/// history of an image's edit is fully reconstructable". A round without its own row is a bound
/// nobody can audit, and "the product tried twice and gave up" versus "the product tried once and
/// it worked" is exactly what [`MAX_ROUNDS`] exists to distinguish.
#[derive(Debug, Clone, PartialEq)]
pub struct QcRound {
    /// Which ticket.
    pub ticket: TicketId,
    /// Which round, `1..=MAX_ROUNDS`.
    pub round: u8,
    /// What was tried.
    pub remedy: Remedy,
    /// The ticket's deviation before this remedy, in [`QcCategory::unit`].
    pub deviation_before: f32,
    /// The ticket's deviation after it.
    pub deviation_after: f32,
    /// What the ticket predicted the gain would be.
    pub expected_gain: f32,
    /// The worst collateral movement, as a share of the affected check's own threshold.
    ///
    /// Zero when nothing else moved. Above [`MAX_COLLATERAL`] the round is reverted whatever its
    /// own metric did, which is section 6.3's no-regression rule.
    pub collateral: f32,
    /// Which check took that movement, when one did.
    pub collateral_category: Option<QcCategory>,
    /// Whether the change survived.
    pub kept: bool,
    /// What happened, as a code.
    pub outcome: QcCode,
    /// How long it took, in milliseconds.
    pub ms: u32,
    /// When it ran.
    pub at: Timestamp,
}

impl QcRound {
    /// The share of the predicted gain this round actually realised.
    ///
    /// The number the loop decides on, and the reason both deviations are on the row. Measured
    /// against what the ticket **opened with**, never against the threshold: a ticket at 4.2
    /// against a 2.5 threshold remediated to 3.9 has improved and still fails, and a build that
    /// kept only what passes would throw away every partial repair on the hardest frames.
    /// ADR-0055 section 4.
    ///
    /// Zero when nothing was predicted, which reverts - a remedy that promised nothing has nothing
    /// to have delivered.
    #[must_use]
    pub fn realised_share(&self) -> f32 {
        if !self.expected_gain.is_finite() || self.expected_gain <= 0.0 {
            return 0.0;
        }
        let realised = self.deviation_before - self.deviation_after;
        if !realised.is_finite() {
            return 0.0;
        }
        (realised / self.expected_gain).clamp(-1.0, 4.0)
    }

    /// True when this round earned the right to stay.
    ///
    /// Both conditions, and the second is not redundant: a remedy can realise its whole predicted
    /// gain on its own metric and break another check, which is what
    /// [`QcCode::CollateralDamage`] records.
    #[must_use]
    pub fn improved(&self) -> bool {
        self.realised_share() >= MIN_GAIN_SHARE && self.collateral <= MAX_COLLATERAL
    }
}

// ---------------------------------------------------------------------------
// Replacements
// ---------------------------------------------------------------------------

/// One frame swapped for another, and the comparison that justified it.
///
/// Section 6.4: "always recorded with a before/after", and section 2.1 puts the pair in the report.
/// Both post-edit metrics are stored rather than the difference, because a photographer looking at
/// a swap wants to know what each frame measured and not what the subtraction came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Replacement {
    /// The ticket that caused it.
    pub ticket: TicketId,
    /// The frame that was in the gallery.
    pub replaced: ImageId,
    /// The frame that is in it now.
    pub replacement: ImageId,
    /// Which category's metric decided it.
    pub category: QcCategory,
    /// The replaced frame's post-edit metric, in [`QcCategory::unit`].
    pub metric_before: f32,
    /// The replacement's post-edit metric, same unit.
    pub metric_after: f32,
    /// How sure. Never below [`REPLACE_CONFIDENCE_FLOOR`] for an automatic swap.
    pub confidence: f32,
    /// True when coverage was re-validated and held.
    ///
    /// Always true on a stored replacement: a swap that broke coverage is not a worse candidate,
    /// it is not a candidate, and the refusal is a [`QcCode::ReplacementBreaksCoverage`] reason on
    /// the ticket rather than a row here. The column exists so the property is a query rather than
    /// a claim about a function. ADR-0055 section 6.
    pub coverage_held: bool,
    /// One sentence about why. Section 5's third tuple element.
    pub note: String,
    /// When.
    pub at: Timestamp,
}

impl Replacement {
    /// How much better the replacement is, as a share of the category's threshold.
    #[must_use]
    pub fn margin(&self, threshold: f32) -> f32 {
        if threshold <= 0.0 {
            return 0.0;
        }
        ((self.metric_before - self.metric_after) / threshold).max(0.0)
    }
}

// ---------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------

/// How one category came out. Section 5's `(QcCategory, u32, u32, u32)` tuple, named.
///
/// A struct rather than a tuple because section 5's own comment says `// found, fixed, escalated`
/// and the tuple has four elements. A shape whose field meanings live in a comment is a shape
/// somebody will index wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CategoryTally {
    /// Findings opened.
    pub found: u32,
    /// Findings a remedy fixed and re-inspection confirmed.
    pub fixed: u32,
    /// Findings handed to a person.
    pub escalated: u32,
    /// Frames this check could not be run on.
    ///
    /// The fourth number, and the one that makes the other three honest. A category with zero found
    /// and four hundred skipped is not a clean category. ADR-0055 section 8.
    pub skipped: u32,
}

impl CategoryTally {
    /// Findings neither fixed nor escalated - reverted, dismissed or still open.
    #[must_use]
    pub const fn outstanding(&self) -> u32 {
        self.found
            .saturating_sub(self.fixed)
            .saturating_sub(self.escalated)
    }
}

/// What one QC pass did. Section 5's frozen shape, with the honesty fields added.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct QcReport {
    /// Which wedding.
    pub project: ProjectId,
    /// Inspections that actually ran.
    pub checks_run: u32,
    /// Photographs inspected - the delivered gallery, not the whole project.
    pub images: u32,
    /// Photographs in the gallery that were not reached.
    ///
    /// Non-zero when the wall-clock budget was spent. A pass that inspected 800 of 1,000 frames and
    /// reported no problems has not reported that the gallery is clean.
    pub images_unreached: u32,
    /// One tally per category, in [`QcCategory::ALL`] order.
    pub by_category: [CategoryTally; QcCategory::COUNT],
    /// Every swap, with its before and after.
    pub replacements: Vec<Replacement>,
    /// Checks that could not run because their input was absent.
    pub skipped: u32,
    /// Remedies applied and kept.
    pub fixed: u32,
    /// Remedies applied and put back.
    pub reverted: u32,
    /// Findings handed to a person.
    pub escalated: u32,
    /// Planner calls made.
    pub planner_calls: u32,
    /// How long, in milliseconds.
    pub duration_ms: u64,
    /// True when the planner was reached at all.
    pub cloud_used: bool,
    /// Which thresholds table.
    pub thresholds_ver: u16,
    /// Which arithmetic.
    pub analysis_ver: u16,
}

impl QcReport {
    /// Findings opened, across every category.
    #[must_use]
    pub fn found(&self) -> u32 {
        self.by_category
            .iter()
            .map(|tally| tally.found)
            .fold(0u32, u32::saturating_add)
    }

    /// The tally for one category.
    #[must_use]
    pub fn tally(&self, category: QcCategory) -> CategoryTally {
        let index = QcCategory::ALL
            .iter()
            .position(|kind| *kind == category)
            .unwrap_or(0);
        self.by_category.get(index).copied().unwrap_or_default()
    }

    /// Fraction of findings a remedy resolved, `0..1`.
    ///
    /// Section 10.1's auto-fix gate is stated over *accepted* tickets, which this build cannot know
    /// without a photographer. This is the mechanical half: of everything found, how much the loop
    /// closed.
    #[must_use]
    pub fn fix_rate(&self) -> f32 {
        ratio(self.fixed, self.found())
    }

    /// Fraction of the gallery that was actually inspected, `0..1`.
    ///
    /// Phase 05's rule, inherited for the fourteenth time, and this phase's denominator is the
    /// *delivered gallery* rather than the project. A mask over a rejected frame is not a gap and
    /// a QC check over one is not an inspection. Phase 18 wrote that rule; this follows it.
    #[must_use]
    pub fn coverage(&self) -> f32 {
        let total = self.images.saturating_add(self.images_unreached);
        ratio(self.images, total)
    }

    /// True when this pass reached every frame it was asked to.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.images_unreached == 0
    }
}

// ---------------------------------------------------------------------------
// Coverage of the pass itself
// ---------------------------------------------------------------------------

/// What a project's QC state holds, and what it does not.
///
/// Phase 05's rule, eleventh consecutive application, with the denominator named in the field docs
/// because this phase's is unusual: **selected frames**, not photographs. Phase 18 established that
/// for masks and the argument is the same here - a QC check over a frame nobody is delivering is
/// not an inspection anybody asked for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct QcOutline {
    /// Frames in the delivered gallery.
    pub selected: u32,
    /// Frames that carry at least one completed inspection.
    pub checked: u32,
    /// Inspections that ran, across every frame.
    pub inspections: u32,
    /// Inspections that could not run because an input was absent.
    pub inspections_skipped: u32,
    /// Tickets open now.
    pub open: u32,
    /// Tickets in each status, in [`TicketStatus::ALL`] order.
    pub by_status: [u32; TicketStatus::COUNT],
    /// Tickets in each category, in [`QcCategory::ALL`] order.
    pub by_category: [u32; QcCategory::COUNT],
    /// Tickets a photographer accepted.
    pub accepted: u32,
    /// Tickets a photographer dismissed.
    ///
    /// The false-ticket numerator section 10.1 gates at 8 %. On the wire and in the outline
    /// rather than derived, because it is the number that says whether this phase is helping.
    pub dismissed: u32,
    /// Frames replaced by a runner-up.
    pub replaced: u32,
    /// Rounds run, across every ticket.
    pub rounds: u32,
    /// Planner calls made, out of [`MAX_PLANNER_CALLS`].
    pub planner_calls: u32,
    /// Bytes this phase occupies for the project.
    pub bytes: u64,
    /// Which thresholds table.
    pub thresholds_ver: u16,
    /// Which arithmetic.
    pub analysis_ver: u16,
    /// True when a defect-detection model is available and trained.
    ///
    /// **False in this build.** No such model ships and none is consulted; every inspection is a
    /// measurement against another phase's stored number. Phase 24 put `detectorTrained` on the
    /// wire and phase 25 `skinFieldAvailable` for the same reason: a panel that had to infer it
    /// would eventually present a measurement as a learned judgement.
    pub detector_trained: bool,
}

impl QcOutline {
    /// True when nothing has been inspected.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.checked == 0
    }

    /// Fraction of the delivered gallery that carries an inspection, `0..1`.
    #[must_use]
    pub fn coverage(&self) -> f32 {
        ratio(self.checked, self.selected)
    }

    /// Fraction of attempted inspections that actually ran, `0..1`.
    ///
    /// The second number, and the one that matters when it is low. A wedding at 100 % coverage and
    /// 20 % inspection completeness has been checked by two of the ten checks.
    #[must_use]
    pub fn inspection_completeness(&self) -> f32 {
        let attempted = self.inspections.saturating_add(self.inspections_skipped);
        ratio(self.inspections, attempted)
    }

    /// Fraction of reviewed tickets a photographer disagreed with, `0..1`.
    ///
    /// Section 10.1's false-ticket rate, gated at 8 %. The denominator is tickets a person *looked
    /// at*, not tickets that exist: a queue nobody has opened has no disagreement rate, and
    /// reporting one as zero would read as unanimous agreement.
    #[must_use]
    pub fn false_ticket_rate(&self) -> f32 {
        ratio(self.dismissed, self.accepted.saturating_add(self.dismissed))
    }

    /// Bytes per thousand selected frames, for the size budget.
    #[must_use]
    pub fn bytes_per_thousand(&self) -> u64 {
        if self.selected == 0 {
            return 0;
        }
        self.bytes
            .saturating_mul(1_000)
            .checked_div(u64::from(self.selected))
            .unwrap_or(u64::MAX)
    }
}

// ---------------------------------------------------------------------------
// The photographer's own decisions
// ---------------------------------------------------------------------------

/// What a photographer decided about a ticket.
///
/// Four things and no fifth. There is no strength field, no threshold field and no way to ask for a
/// remedy the policy does not permit - the same shape phases 20 to 26 use, for the same reason:
/// a ceiling a studio can lower and nobody can raise is what makes a written promise a property of
/// the product rather than a description of its defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct QcOverride {
    /// Which ticket.
    pub ticket: TicketId,
    /// What the photographer decided.
    ///
    /// Only [`TicketStatus::Accepted`] and [`TicketStatus::Dismissed`] may be set here. Anything
    /// else is [`AuraError`] from [`QcService::decide`]: a person may agree or disagree with a
    /// finding, and "mark this fixed" without a remedy having run would be a person writing a
    /// measurement.
    pub status: TicketStatus,
    /// Apply the proposed remedy now, whatever the autonomy band said.
    ///
    /// A photographer overruling a `RequireReview` band upward is the one direction that is safe:
    /// they have looked. There is no field that overrules it downward.
    pub apply_remedy: bool,
    /// One sentence, kept for the studio's own record. Never rendered as a reason.
    pub note: Option<String>,
}

impl QcOverride {
    /// True when this override is one the service will accept.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.status.is_user_set()
            && self
                .note
                .as_ref()
                .is_none_or(|note| note.len() <= Self::MAX_NOTE)
    }

    /// The longest note kept. Long enough for a sentence, short enough that nobody pastes a
    /// client's email into the catalog.
    pub const MAX_NOTE: usize = 280;
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask what quality control found, and the one way to answer it.
///
/// Frozen. Implemented by `aura_qc::Qc`.
///
/// The rule phase 05 wrote for `SimilarityIndex` and every phase since has repeated, a
/// twenty-third time: **no phase may keep its own idea of whether the work is good.** Phase 28
/// calls this as a stage and reads [`QcOutline::open`] before it delivers; phase 29 builds albums
/// out of frames this pass has already checked; phase 30's learning loop reads
/// [`QcTicket::status`] and [`QcTicket::outcome_code`] as its signal about what the product got
/// wrong. Two answers to "was this delivery any good" is a product that ships a gallery its own
/// report says is fine and its own queue says is not.
///
/// **There is no `apply` and no `render`.** A remedy is a decision; `aura_recipe::schema::merge`
/// executes it and `RenderService` draws it. Phase 14's rule, twelfth application, and
/// `crates/aura-qc/tests/no_pixel_ops.rs` is the grep that enforces it.
pub trait QcService: Send + Sync + fmt::Debug {
    /// What this project's QC state holds.
    ///
    /// # Errors
    ///
    /// Returns [`AuraError`] when the stored rows cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<QcOutline>;

    /// The most recent report, when a pass has run.
    ///
    /// # Errors
    ///
    /// Returns [`AuraError`] when the stored rows cannot be read.
    fn report(&self, project: ProjectId) -> AuraResult<Option<QcReport>>;

    /// Every ticket on one photograph, worst first.
    ///
    /// # Errors
    ///
    /// Returns [`AuraError`] when the stored rows cannot be read.
    fn tickets(&self, image: ImageId) -> AuraResult<Vec<QcTicket>>;

    /// The escalation queue: open tickets across the project, worst first.
    ///
    /// `category` narrows it to one inspection, which is section 2.1's "grouped by category so a
    /// photographer can clear 40 tickets in minutes".
    ///
    /// # Errors
    ///
    /// Returns [`AuraError`] when the stored rows cannot be read.
    fn queue(
        &self,
        project: ProjectId,
        category: Option<QcCategory>,
        limit: usize,
    ) -> AuraResult<Vec<QcTicket>>;

    /// Every round run against one ticket, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`AuraError`] when the stored rows cannot be read.
    fn rounds(&self, ticket: TicketId) -> AuraResult<Vec<QcRound>>;

    /// Every replacement this project made.
    ///
    /// # Errors
    ///
    /// Returns [`AuraError`] when the stored rows cannot be read.
    fn replacements(&self, project: ProjectId) -> AuraResult<Vec<Replacement>>;

    /// Record what a photographer decided about a ticket.
    ///
    /// # Errors
    ///
    /// Returns [`AuraError`] when the override names a status automation owns, when the ticket does
    /// not exist, or when the row cannot be written.
    fn decide(&self, over: &QcOverride) -> Result<(), AuraError>;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A part over a whole, quantised to thousandths and clamped to `0..1`.
///
/// Quantised for the reason phase 13's is: a coverage figure that differs in the seventh decimal
/// between two machines is a coverage figure two support cases disagree about.
// The value cast below is `.min(1_000)`, so there is no precision to lose. The lint is about
// `u64` as a type rather than about this value.
#[allow(clippy::cast_precision_loss)]
fn ratio(part: u32, whole: u32) -> f32 {
    if whole == 0 {
        return 0.0;
    }
    let thousandths = u64::from(part)
        .saturating_mul(1_000)
        .checked_div(u64::from(whole))
        .unwrap_or(0)
        .min(1_000);
    thousandths as f32 / 1_000.0
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::assertions_on_constants,
    clippy::cast_precision_loss,
    // Denied crate-wide, and allowed here for one use: a `match` arm that asserts a shape has
    // not changed has nowhere else to go, because the whole point is that reaching it is a bug.
    clippy::panic
)]
mod tests {
    use super::*;

    fn ticket() -> QcTicket {
        QcTicket {
            id: TicketId::new(),
            project: ProjectId::new(),
            image_id: ImageId::new(),
            category: QcCategory::Skin,
            code: QcCode::SkinDrift,
            deviation: 4.2,
            threshold: 2.5,
            evidence: Evidence::Anchors(vec![ImageId::new(), ImageId::new()]),
            identity: None,
            remedy: Remedy::ResolveParam {
                target: SolveTarget::WhiteBalance,
                constraint: "hold the exposure".into(),
            },
            expected_gain: 2.0,
            confidence: 0.72,
            autonomy: Autonomy::Auto,
            reasons: vec![QcReason::new(QcCode::SkinDrift, 1.0)],
            round: 0,
            status: TicketStatus::Open,
            outcome_code: None,
            scene: SceneId::Unknown,
            created_at: 0,
            thresholds_ver: 1,
            analysis_ver: 1,
        }
    }

    #[test]
    fn every_category_has_a_distinct_triage_rank() {
        let mut ranks: Vec<u8> = QcCategory::ALL
            .iter()
            .map(|kind| kind.triage_rank())
            .collect();
        ranks.sort_unstable();
        ranks.dedup();
        assert_eq!(ranks.len(), QcCategory::COUNT);
    }

    #[test]
    fn consistency_is_the_root_and_coverage_is_the_leaf() {
        // Section 7's offline fallback: consistency -> exposure -> skin -> retouch -> sharpness.
        assert!(QcCategory::Consistency.triage_rank() < QcCategory::Exposure.triage_rank());
        assert!(QcCategory::Exposure.triage_rank() < QcCategory::Skin.triage_rank());
        assert!(QcCategory::Skin.triage_rank() < QcCategory::Retouch.triage_rank());
        assert!(QcCategory::Retouch.triage_rank() < QcCategory::Sharpness.triage_rank());
        // And the two set-scoped ones come last, because remediating them cannot move any other
        // check's measurement.
        for kind in QcCategory::ALL {
            if !kind.is_gallery_scoped() {
                assert!(kind.triage_rank() < QcCategory::Duplicate.triage_rank());
            }
        }
    }

    #[test]
    fn every_code_round_trips_and_the_all_array_is_complete() {
        assert_eq!(QcCode::ALL.len(), QcCode::COUNT);
        for code in QcCode::ALL {
            assert_eq!(QcCode::parse(code.as_str()), Some(code));
            assert!(!code.user_text().is_empty());
        }
        let mut names: Vec<&str> = QcCode::ALL.iter().map(|code| code.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), QcCode::COUNT);
    }

    #[test]
    fn a_finding_belongs_to_one_category_and_an_outcome_belongs_to_none() {
        for code in QcCode::ALL {
            if code.category().is_some() {
                assert!(
                    code.is_finding(),
                    "{code} has a category but is not a finding"
                );
            }
            if code.is_refusal() || code.is_unavailable() {
                // MaskUncovered is both a finding and an unavailability: a local operation that
                // ran with no region is a real defect *and* a check that could not be made.
                if code != QcCode::MaskUncovered {
                    assert!(
                        !code.is_finding(),
                        "{code} is both an outcome and a finding"
                    );
                }
            }
        }
    }

    #[test]
    fn a_replacement_can_move_every_check_and_an_escalation_can_move_none() {
        let replace = Remedy::ReplaceFrame {
            with: ImageId::new(),
        };
        assert_eq!(replace.collateral_checks().len(), QcCategory::COUNT);
        let escalate = Remedy::Escalate {
            note: "look".into(),
        };
        assert!(escalate.collateral_checks().is_empty());
        assert!(!escalate.mutates());
        assert!(replace.mutates());
    }

    #[test]
    fn a_replacement_needs_more_confidence_than_a_parameter_fix() {
        let replace = Remedy::ReplaceFrame {
            with: ImageId::new(),
        };
        let fix = Remedy::ReduceStrength {
            op: "retouch.evening".into(),
            factor: 0.5,
        };
        assert!(replace.confidence_floor() > fix.confidence_floor());
        assert!(REPLACE_CONFIDENCE_FLOOR > FIX_CONFIDENCE_FLOOR);
    }

    #[test]
    fn a_strength_remedy_cannot_raise_a_strength() {
        assert!(MAX_STRENGTH_FACTOR < 1.0);
        let raise = Remedy::ReduceStrength {
            op: "retouch.evening".into(),
            factor: 1.4,
        };
        assert!(!raise.is_within_bounds());
        let off = Remedy::ReduceStrength {
            op: "retouch.evening".into(),
            factor: 0.05,
        };
        assert!(!off.is_within_bounds(), "switching off is RevertOp");
        let ok = Remedy::ReduceStrength {
            op: "retouch.evening".into(),
            factor: 0.75,
        };
        assert!(ok.is_within_bounds());
    }

    #[test]
    fn a_round_is_kept_on_realised_gain_and_not_on_reaching_the_threshold() {
        // 4.2 -> 3.9 against a 2.5 threshold still fails the check, and the round is kept because
        // it realised more than half of a 0.5 prediction. ADR-0055 section 4.
        let round = QcRound {
            ticket: TicketId::new(),
            round: 1,
            remedy: Remedy::Escalate { note: "n".into() },
            deviation_before: 4.2,
            deviation_after: 3.9,
            expected_gain: 0.5,
            collateral: 0.0,
            collateral_category: None,
            kept: true,
            outcome: QcCode::RemedyApplied,
            ms: 10,
            at: 0,
        };
        assert!(round.realised_share() > MIN_GAIN_SHARE);
        assert!(round.improved());
    }

    #[test]
    fn a_round_that_promised_nothing_is_reverted() {
        let round = QcRound {
            ticket: TicketId::new(),
            round: 1,
            remedy: Remedy::Escalate { note: "n".into() },
            deviation_before: 4.2,
            deviation_after: 4.2,
            expected_gain: 0.0,
            collateral: 0.0,
            collateral_category: None,
            kept: false,
            outcome: QcCode::RemedyReverted,
            ms: 10,
            at: 0,
        };
        assert_eq!(round.realised_share(), 0.0);
        assert!(!round.improved());
    }

    #[test]
    fn collateral_damage_reverts_a_round_that_met_its_own_gain() {
        let round = QcRound {
            ticket: TicketId::new(),
            round: 1,
            remedy: Remedy::Escalate { note: "n".into() },
            deviation_before: 4.0,
            deviation_after: 1.0,
            expected_gain: 3.0,
            collateral: MAX_COLLATERAL + 0.01,
            collateral_category: Some(QcCategory::Exposure),
            kept: false,
            outcome: QcCode::CollateralDamage,
            ms: 10,
            at: 0,
        };
        assert!(round.realised_share() >= MIN_GAIN_SHARE);
        assert!(!round.improved());
    }

    #[test]
    fn severity_is_a_ratio_so_two_units_can_share_a_queue() {
        let mut skin = ticket();
        skin.deviation = 5.0;
        skin.threshold = 2.5;
        let mut exposure = ticket();
        exposure.category = QcCategory::Exposure;
        exposure.code = QcCode::ExposureRegression;
        exposure.deviation = 0.9;
        exposure.threshold = 0.3;
        // The exposure finding is three thresholds out and the skin one is two, so it leads even
        // though its raw number is smaller.
        assert!(exposure.severity() > skin.severity());
        assert_eq!(exposure.queue_order(&skin), Ordering::Less);
    }

    #[test]
    fn a_ticket_is_well_formed_only_with_a_finding_a_number_and_a_reason() {
        let base = ticket();
        assert!(base.is_well_formed());
        let mut no_reasons = ticket();
        no_reasons.reasons.clear();
        assert!(!no_reasons.is_well_formed());
        let mut outcome_as_finding = ticket();
        outcome_as_finding.code = QcCode::RemedyApplied;
        assert!(!outcome_as_finding.is_well_formed());
        let mut zero_threshold = ticket();
        zero_threshold.threshold = 0.0;
        assert!(!zero_threshold.is_well_formed());
    }

    #[test]
    fn the_diagnosis_is_rendered_from_the_numbers_and_is_deterministic() {
        let subject = ticket();
        let first = subject.render_diagnosis();
        let second = subject.render_diagnosis();
        assert_eq!(first, second);
        assert!(first.contains("4.20 dE00"));
        assert!(first.contains("2.50 dE00"));
        assert!(first.contains("2 anchors"));
    }

    #[test]
    fn a_dismissed_ticket_is_never_touched_by_automation() {
        assert!(!TicketStatus::Dismissed.is_automatable());
        assert!(!TicketStatus::Accepted.is_automatable());
        for status in TicketStatus::ALL {
            assert_eq!(status.is_automatable(), !status.is_user_set());
        }
    }

    #[test]
    fn an_override_can_only_set_a_status_a_person_owns() {
        let mut over = QcOverride {
            ticket: TicketId::new(),
            status: TicketStatus::Accepted,
            apply_remedy: false,
            note: None,
        };
        assert!(over.is_valid());
        over.status = TicketStatus::Fixed;
        assert!(!over.is_valid(), "automation owns Fixed");
        over.status = TicketStatus::Dismissed;
        over.note = Some("x".repeat(QcOverride::MAX_NOTE + 1));
        assert!(!over.is_valid());
    }

    #[test]
    fn evidence_can_never_hold_a_pixel_and_is_bounded() {
        let frames: Vec<ImageId> = (0..40).map(|_| ImageId::new()).collect();
        let bounded = Evidence::Frames(frames).bounded();
        match bounded {
            Evidence::Frames(list) => assert_eq!(list.len(), Evidence::MAX_ITEMS),
            _ => panic!("shape changed"),
        }
        // The compiler is the assertion for "no pixels": there is no variant that takes bytes.
        assert_eq!(Evidence::None.kind_str(), "none");
    }

    #[test]
    fn an_unreached_gallery_is_not_a_clean_one() {
        let mut report = QcReport {
            images: 800,
            images_unreached: 200,
            ..QcReport::default()
        };
        assert!(!report.complete());
        assert_eq!(report.coverage(), 0.8);
        report.images_unreached = 0;
        assert!(report.complete());
    }

    #[test]
    fn the_false_ticket_rate_denominator_is_what_somebody_looked_at() {
        let outline = QcOutline {
            open: 500,
            accepted: 18,
            dismissed: 2,
            ..QcOutline::default()
        };
        assert_eq!(outline.false_ticket_rate(), 0.1);
        let untouched = QcOutline {
            open: 500,
            ..QcOutline::default()
        };
        assert_eq!(untouched.false_ticket_rate(), 0.0);
    }

    #[test]
    fn the_shipped_build_ships_no_detector() {
        // Not a property of the type - a property of the build, asserted here so a later change
        // that flipped it has to change this test and explain itself.
        assert!(!QcOutline::default().detector_trained);
    }

    #[test]
    fn the_bounds_are_the_ones_the_phase_document_names() {
        assert_eq!(MAX_ROUNDS, 2);
        assert_eq!(MAX_PLANNER_CALLS, 40);
        assert_eq!(MAX_PLAN_STEPS, 4);
        assert_eq!(MAX_TOOL_STEPS, 6);
        assert_eq!(PLANNER_TICKET_FLOOR, 3);
        assert_eq!(PASS_BUDGET_MS_PER_1K, 90_000);
        assert_eq!(ROUND_BUDGET_MS, 1_200);
    }
}
