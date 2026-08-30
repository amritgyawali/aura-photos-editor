//! FROZEN CONTRACT. What a whole wedding looks like as one body of work: the tree of scene
//! nodes, the anchors each node is judged against, the target they imply, the bounded
//! movement every other frame makes toward it, and the frames that would not come.
//!
//! PHASE-25 section 5 freezes [`SceneNode`], [`NodeTarget`] and [`NormalisationDelta`] before
//! any solver exists. The file is in `aura-core` rather than in `aura-brain-gallery` for the
//! reason [`crate::contract::tone`] and [`crate::contract::colour`] are: the phases that
//! consume a gallery decision are 26 (cross-camera matching), 27 (QC), 28 (autopilot) and 29
//! (curation), and none of them needs the anchor ranker, the change-point detector or the
//! skin solver.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **Every number in this file is a residual, and the thing it is a residual *from* is phase
//! 15's and phase 16's stored per-frame answer.** [`NormalisationDelta::d_cct`] is not a
//! temperature; it is how much this frame's temperature moves after phase 15 has already
//! decided what it should be. Three layers - the camera, the per-frame estimate, the gallery
//! delta - applied in that order at merge time, and phase 26 adds a fourth.
//!
//! A caller that adds `d_cct` to something other than [`NormalisationDelta::from_cct_k`] has
//! misunderstood the shape. ADR-0051 section 2 has the argument; phase 17 wrote the same rule
//! for style profiles and this is its second application.
//!
//! ## The second thing: the delta is measured from the un-normalised world, which is what
//! makes it idempotent
//!
//! Section 6.2 requires that running the consistency pass twice changes nothing, and section
//! 12 names solver drift as a failure mode. That is not achieved by detecting a second run
//! and it is not achieved by convergence. It is achieved because the solver's input -
//! phase 15's `ToneEstimate` and phase 16's `ColourDecision` - is **immutable with respect to
//! the solver's own output**. The second run computes the same delta as the first and writing
//! it again is a no-op.
//!
//! [`NormalisationDelta::from_cct_k`], [`NormalisationDelta::from_tint`] and
//! [`NormalisationDelta::from_exposure_ev`] are on the row so a panel can draw the movement
//! and an audit can check it. **They are a record, not a mechanism**: nothing reads them back
//! into the solver.
//!
//! ## The third thing: a node that could not be anchored is not a node that needed nothing
//!
//! [`GalleryCode::NodeUnanchored`] and an empty delta list are different answers from
//! "every frame here was already consistent", and they are separate codes, separate rows and
//! separate runbooks. Phase 24 wrote this rule as "an absent input is ignorance, not
//! permission" and it is inherited unchanged: a node the product could not judge and a node
//! it judged and left alone must never be the same query.
//!
//! ## Eight spellings differ from the phase document
//!
//! All eight are recorded in `docs/adr/ADR-0051-gallery-consistency-and-normalisation.md`
//! section 10 and in the notes below.
//!
//! * **`ImageId` is `PhotoId`**, aliased rather than duplicated, exactly as the scene,
//!   people, moment, integrity, emotion, composition, cull, tone and colour contracts already
//!   do it.
//! * **`NodeId` is a new typed id** in [`crate::contract::ids`], the fourteenth. Section 5
//!   uses the name without defining it. ADR-0051 section 10 records why it is an id rather
//!   than `(segment_id, ordinal)`: an ordinal renumbers when a node is split or merged, and
//!   an anchor row, a delta row and an outlier row all have to point at a node across a
//!   re-analysis.
//! * **`SkinCorrection` is not in section 5.** [`NormalisationDelta::skin_correction`] is
//!   printed there as `Option<SkinCorrection>` with no definition of the type.
//! * **`Bound` is not in section 5 either**, and [`NormalisationDelta::bounded_by`] is
//!   printed as `Option<Bound>`. It is an enum naming which of five bounds bit, because
//!   "this frame was clamped" and "this frame was clamped *in temperature*" are different
//!   facts and only the second can be acted on.
//! * **`reasons: Vec<Reason>` carries a typed [`GalleryCode`]** rather than
//!   [`crate::contract::integrity::Reason`]'s free-text pair, the same choice phases 09 to
//!   24 made: a stored row costs a code rather than a sentence, and a sentence in a catalog
//!   cannot be translated.
//! * **[`Outlier`] is not in section 5.** Section 6.4 makes outliers "a first-class output"
//!   and section 2.1 makes them "exactly the Phase 27 QC input"; a handoff with no type is a
//!   handoff nobody can test.
//! * **[`GalleryOutline`] is the coverage carrier**, inherited from phase 05 for the
//!   nineteenth time, with this phase's two refinements written at
//!   [`GalleryOutline::anchored_nodes`] and [`GalleryOutline::skin_targeted`].
//! * **[`GalleryService`] is not in section 5.** Four later phases consume a gallery decision
//!   and a contract with no entry point makes each of them find its own way in.
//!
//! ## Two version fields, and why there is no third
//!
//! [`NormalisationDelta::analysis_ver`] invalidates the tree, the anchors, the target and
//! every delta, because all four come from this build's arithmetic.
//! [`NormalisationDelta::policy_ver`] invalidates every number that was compared against a
//! **bound or a damping factor**, because those are a product decision a release can move
//! without changing a line of solver code.
//!
//! There is no `model_ver`, because this phase ships no model. A column that can never change
//! is a column that will eventually be compared against and mean nothing, which is the
//! opposite of what a version column is for.
//!
//! Comparing two rows across either of them returns a plausible number that means nothing.
//! `AURA-ML-5127` exists so that comparison never happens silently. Tenth phase, tenth
//! version-drift code.
//!
//! Changing anything in this file requires an ADR.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{IdentityId, NodeId, ProjectId, SegmentId};
use crate::contract::scene::SceneId;

pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// Bounds, damping and gates
// ---------------------------------------------------------------------------

/// The fewest anchors a node needs before it may be normalised toward anything.
///
/// Section 2.1: "select 3-5 anchors". Three, and the floor is the interesting half: a node
/// with two anchors has a target that one bad frame can move by half, and phase 15 already
/// refuses a segment with fewer than three reference candidates for the same reason.
pub const MIN_ANCHORS: usize = 3;

/// The most anchors a node carries.
pub const MAX_ANCHORS: usize = 5;

/// The fewest frames a node may contain and still be a node.
///
/// Below this a change-point split is refused and the frames stay with their neighbours: a
/// two-frame node cannot be anchored, so splitting one out converts a mild inconsistency into
/// an unanchored gap.
pub const MIN_NODE_FRAMES: usize = 4;

/// The most a frame's correlated colour temperature may move, in kelvin.
///
/// Section 6.2's own number. About the largest shift that still reads as "the same room"
/// rather than "a different white balance"; past it the correction is more visible than the
/// drift it corrects.
pub const MAX_D_CCT_K: f32 = 450.0;

/// The most a frame's tint may move, in the recipe's tint units.
///
/// Scaled from [`MAX_D_CCT_K`] at the sensitivity phase 15's own tolerances imply - it calls
/// 200 K and 4 tint units equivalent - which puts 450 K at 9. Twelve rather than nine,
/// because a green cast under fluorescent light is the one axis where the *drift* is
/// routinely larger than the temperature drift beside it.
pub const MAX_D_TINT: f32 = 12.0;

/// The most a frame's exposure may move, in stops.
///
/// Section 6.2's own number. A third of a stop is at the edge of what reads as a difference
/// between two adjacent frames, which is the comparison this phase exists to fix.
pub const MAX_D_EXPOSURE_EV: f32 = 0.35;

/// The most a frame's contrast may move, in the recipe's `-100..100` units.
///
/// An eighth of the subtlety ceiling phase 16 already enforces on a whole grade. A
/// consistency pass is the third thing to touch contrast on a frame and it gets the smallest
/// share.
pub const MAX_D_CONTRAST: f32 = 8.0;

/// The most a frame's saturation may move, in the recipe's `-100..100` units.
///
/// Lower than [`MAX_D_CONTRAST`], because a saturation move is the one most visible on skin
/// and skin is what phase 16's guard is measuring while this runs.
pub const MAX_D_SATURATION: f32 = 6.0;

/// The default damping factor: how much of the distance to the target a frame actually moves.
///
/// Section 6.2 gives the range 0.6-0.8 and this is its middle. Damping below one is the first
/// of the four defences against flattening intentional variation, and it is the softest -
/// bounds, change-point splitting and the intentional-illuminant exclusion are the other
/// three, and unlike damping each of them can refuse entirely.
pub const DEFAULT_DAMPING: f32 = 0.7;

/// The range a damping factor may take.
///
/// The floor is not zero: a damping of zero is a pass that is switched off, and switching the
/// pass off is a feature flag rather than a config value somebody sets by accident. The
/// ceiling is not one: a damping of one moves every frame onto its target exactly, which is
/// the flattening of section 12's first failure mode.
pub const DAMPING_RANGE: (f32, f32) = (0.30, 0.90);

/// How much a frame may move on a second run before idempotence is considered broken.
///
/// Section 10.1: "a second normalisation run moves every frame by less than a small epsilon".
/// Expressed as a fraction of each bound rather than in absolute units, so one number covers
/// five axes with five different scales.
pub const IDEMPOTENCE_EPSILON: f32 = 0.01;

/// The dE00 spread a single identity's skin may show across a whole gallery.
///
/// Section 6.3's measurable claim, and the headline half of section 10.1. It is in the
/// contract rather than in the eval harness because the panel, the outline and the gate all
/// ask the same question, and two copies of 2.0 is one chance to write one of them as 3.0.
pub const SKIN_DE00_SPREAD_CEILING: f32 = 2.0;

/// The furthest a skin correction may move a frame's skin chromaticity, in CIE 1976 `u'v'`.
///
/// **Reduced toward zero as the frame's light becomes more intentional**, which is section
/// 6.3's "capped so lighting mood is preserved (a candle-lit face may stay warm, but not
/// magenta)" as an arithmetic rule rather than a sentence. See
/// [`SkinCorrection::cap_for_mood`].
pub const SKIN_CHROMA_CAP: f32 = 0.018;

/// The furthest a skin correction may move a frame's skin luminance, `0..1`.
pub const SKIN_LUMA_CAP: f32 = 0.06;

/// The fewest frames an identity needs before their skin target constrains anything.
///
/// Phase 15's argument for `MIN_LOCUS_SAMPLES`, unchanged: a target fitted to fewer is a
/// target fitted to one lighting condition, and a weak target is worse than none because it
/// looks like evidence.
pub const MIN_SKIN_FRAMES: u32 = 5;

/// The residual, as a fraction of each bound, past which a frame is an outlier.
///
/// Measured on what is **left after** the delta was applied, never on the raw deviation. A
/// frame 900 K from its node that the bound could only move 450 K is an outlier; a frame
/// 300 K away that was corrected in full is not. ADR-0051 section 7.
pub const OUTLIER_RESIDUAL: f32 = 0.75;

/// The within-run spreads a step must exceed before it is called a change point.
///
/// A two-sample statistic rather than a fixed threshold, because the spread of a candle-lit
/// vow and the spread of an outdoor ceremony differ by an order of magnitude and a fixed
/// number is right for exactly one of them. Phase 22's rule: a threshold is a statement about
/// the instrument as well as about the world.
pub const SPLIT_SIGMA: f32 = 3.0;

/// The fewest consecutive frames either side of a change point.
///
/// Below this the "transition" is a handful of frames rather than a new lighting condition,
/// and splitting on it produces two nodes neither of which can be anchored.
pub const MIN_RUN: usize = 6;

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

/// Which bound stopped a frame moving as far as its target.
///
/// Not in section 5, which prints `bounded_by: Option<Bound>` without defining `Bound`.
/// An enum rather than a boolean because "this frame was clamped" and "this frame was clamped
/// *in temperature*" are different facts, and only the second tells a photographer whether
/// the node is wrong or the frame is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Bound {
    /// [`MAX_D_CCT_K`].
    #[default]
    Cct,
    /// [`MAX_D_TINT`].
    Tint,
    /// [`MAX_D_EXPOSURE_EV`].
    Exposure,
    /// [`MAX_D_CONTRAST`].
    Contrast,
    /// [`MAX_D_SATURATION`].
    Saturation,
}

impl Bound {
    /// Every bound, in the order a delta is solved.
    pub const ALL: [Self; 5] = [
        Self::Cct,
        Self::Tint,
        Self::Exposure,
        Self::Contrast,
        Self::Saturation,
    ];

    /// How many bounds there are.
    pub const COUNT: usize = 5;

    /// The stored slug, sent on the wire. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cct => "cct",
            Self::Tint => "tint",
            Self::Exposure => "exposure",
            Self::Contrast => "contrast",
            Self::Saturation => "saturation",
        }
    }

    /// The words a photographer reads.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Cct => "colour temperature",
            Self::Tint => "tint",
            Self::Exposure => "exposure",
            Self::Contrast => "contrast",
            Self::Saturation => "saturation",
        }
    }

    /// The contract's own ceiling for this axis, in the axis's own units.
    ///
    /// Asked in one place so the solver, the store, the loader, the panel and the gate cannot
    /// disagree about where the edge is. `consistency.toml` may lower one of these and may
    /// never raise one; the loader refuses a file that tries.
    #[must_use]
    pub const fn ceiling(self) -> f32 {
        match self {
            Self::Cct => MAX_D_CCT_K,
            Self::Tint => MAX_D_TINT,
            Self::Exposure => MAX_D_EXPOSURE_EV,
            Self::Contrast => MAX_D_CONTRAST,
            Self::Saturation => MAX_D_SATURATION,
        }
    }

    /// Parse the stored slug. Unknown text reads as [`Bound::Cct`].
    #[must_use]
    pub fn from_str_or_cct(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|bound| bound.as_str() == text)
            .unwrap_or(Self::Cct)
    }
}

impl fmt::Display for Bound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why a frame moved as far as it did, or did not move at all, as a closed set.
///
/// Section 9 gives DOC "explain gallery consistency, anchors and how to pin them", which is
/// only a finishable job if the codes are enumerable. `docs/gallery-consistency.md` is written
/// against [`GalleryCode::ALL`] and a test asserts every variant appears there.
///
/// **Twelve of these twenty-six withdraw a claim rather than making one.** They are the codes
/// that say the product declined to normalise, or normalised on less evidence than it wanted,
/// and this is the phase where that distinction is most expensive: a gallery that has been
/// flattened and a gallery that was already consistent look identical in a summary and
/// completely different on a screen.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum GalleryCode {
    // -- the node and its anchors ------------------------------------------------------
    /// The node was anchored on frames the product was most confident about.
    #[default]
    NodeAnchored,
    /// The node had fewer than [`MIN_ANCHORS`] usable candidates, so nothing was normalised.
    ///
    /// **Not the same as "nothing needed normalising."** ADR-0051 section 3.
    NodeUnanchored,
    /// A photographer pinned at least one of this node's anchors.
    AnchorPinned,
    /// A photographer rejected an anchor the product had chosen.
    AnchorRejected,
    /// The node's target came from a median rather than a mean, because an anchor disagreed.
    RobustTarget,
    /// The node was split because the light genuinely changed part-way through.
    NodeSplitByChangePoint,
    /// A split was declined because one side would have had fewer than [`MIN_NODE_FRAMES`].
    SplitTooSmall,
    /// The node is a sub-cluster of a long segment rather than the whole of one.
    NodeSubClustered,

    // -- what moved ---------------------------------------------------------------------
    /// The frame's white balance moved toward the node's anchors.
    WarmthNormalised,
    /// The frame's exposure moved toward the node's subject-luminance band.
    ExposureNormalised,
    /// The frame's contrast and saturation moved toward the node's grade character.
    GradeHarmonised,
    /// The frame's skin was corrected toward the identity's own gallery target.
    SkinNormalised,

    // -- what did not move, and why ----------------------------------------------------
    /// The frame was already inside its node's tolerances.
    AlreadyConsistent,
    /// The movement hit a bound and was clamped. [`NormalisationDelta::bounded_by`] says which.
    BoundedByPolicy,
    /// The frame's light is intentional - stage or candle - so the product left it alone.
    ///
    /// Section 2.1's preserve-mood rule, read straight off phase 15's
    /// `IlluminantKind::is_intentional` rather than decided a second time here.
    MoodPreserved,
    /// The frame is lit by more than one source, so a single delta would be wrong somewhere.
    MixedLightSkipped,
    /// The photographer set this frame's values by hand and automation never overwrites them.
    UserEdited,
    /// The pass was switched off for this frame.
    Disabled,

    // -- absent inputs -----------------------------------------------------------------
    /// The frame has no phase 15 estimate, so there is nothing to move.
    ToneEstimateAbsent,
    /// The frame has no phase 16 decision, so contrast and saturation were left alone.
    ColourDecisionAbsent,
    /// No identity-scoped skin mask was available, so no skin correction was attempted.
    ///
    /// **Ignorance, not permission.** Phase 24's rule. This is a different row from
    /// [`GalleryCode::SkinTargetAbsent`], which says the masks were there and the *person* was
    /// not measured often enough.
    SkinMaskAbsent,
    /// The identity has fewer than [`MIN_SKIN_FRAMES`] well-lit frames, so it has no target.
    SkinTargetAbsent,
    /// The frame belongs to no segment, so it belongs to no node.
    SegmentAbsent,

    // -- outliers ----------------------------------------------------------------------
    /// After normalising, the frame is still further from its node than [`OUTLIER_RESIDUAL`].
    OutlierAfterNormalisation,
    /// The frame's skin is still outside the identity's gallery spread.
    SkinOutlier,
    /// The node's own anchors disagree with each other more than the node's frames do.
    ///
    /// The anchor selection has gone wrong rather than the frames having drifted, and the
    /// honest thing is to say so instead of normalising a coherent node toward an incoherent
    /// target. Section 12's second failure mode, made visible.
    AnchorsDisagree,
}

impl GalleryCode {
    /// Every code, in the order `docs/gallery-consistency.md` documents them.
    pub const ALL: [Self; 26] = [
        Self::NodeAnchored,
        Self::NodeUnanchored,
        Self::AnchorPinned,
        Self::AnchorRejected,
        Self::RobustTarget,
        Self::NodeSplitByChangePoint,
        Self::SplitTooSmall,
        Self::NodeSubClustered,
        Self::WarmthNormalised,
        Self::ExposureNormalised,
        Self::GradeHarmonised,
        Self::SkinNormalised,
        Self::AlreadyConsistent,
        Self::BoundedByPolicy,
        Self::MoodPreserved,
        Self::MixedLightSkipped,
        Self::UserEdited,
        Self::Disabled,
        Self::ToneEstimateAbsent,
        Self::ColourDecisionAbsent,
        Self::SkinMaskAbsent,
        Self::SkinTargetAbsent,
        Self::SegmentAbsent,
        Self::OutlierAfterNormalisation,
        Self::SkinOutlier,
        Self::AnchorsDisagree,
    ];

    /// How many codes there are.
    pub const COUNT: usize = 26;

    /// The stored slug, sent on the wire. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NodeAnchored => "node_anchored",
            Self::NodeUnanchored => "node_unanchored",
            Self::AnchorPinned => "anchor_pinned",
            Self::AnchorRejected => "anchor_rejected",
            Self::RobustTarget => "robust_target",
            Self::NodeSplitByChangePoint => "node_split_by_change_point",
            Self::SplitTooSmall => "split_too_small",
            Self::NodeSubClustered => "node_sub_clustered",
            Self::WarmthNormalised => "warmth_normalised",
            Self::ExposureNormalised => "exposure_normalised",
            Self::GradeHarmonised => "grade_harmonised",
            Self::SkinNormalised => "skin_normalised",
            Self::AlreadyConsistent => "already_consistent",
            Self::BoundedByPolicy => "bounded_by_policy",
            Self::MoodPreserved => "mood_preserved",
            Self::MixedLightSkipped => "mixed_light_skipped",
            Self::UserEdited => "user_edited",
            Self::Disabled => "disabled",
            Self::ToneEstimateAbsent => "tone_estimate_absent",
            Self::ColourDecisionAbsent => "colour_decision_absent",
            Self::SkinMaskAbsent => "skin_mask_absent",
            Self::SkinTargetAbsent => "skin_target_absent",
            Self::SegmentAbsent => "segment_absent",
            Self::OutlierAfterNormalisation => "outlier_after_normalisation",
            Self::SkinOutlier => "skin_outlier",
            Self::AnchorsDisagree => "anchors_disagree",
        }
    }

    /// The sentence a photographer reads. Rendered from the code, never stored.
    ///
    /// Phase 09's rule, inherited for the seventeenth time: a stored sentence is copy a
    /// release can change, and a catalog full of English cannot be translated.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::NodeAnchored => "This part of the wedding is anchored to its best frames.",
            Self::NodeUnanchored => {
                "AURA could not find three frames it was confident enough about to anchor this \
                 part of the wedding, so it left every frame here exactly as it was."
            }
            Self::AnchorPinned => "You pinned one of the anchors for this part of the wedding.",
            Self::AnchorRejected => "You rejected an anchor AURA had chosen here.",
            Self::RobustTarget => {
                "One anchor disagreed with the others, so the target came from the middle of \
                 them rather than the average."
            }
            Self::NodeSplitByChangePoint => {
                "The light genuinely changed part-way through, so the two halves are matched \
                 separately."
            }
            Self::SplitTooSmall => {
                "The light seemed to change here, but there were too few frames on one side to \
                 treat it as a separate look."
            }
            Self::NodeSubClustered => {
                "This chapter was long enough to be matched in parts rather than as one block."
            }
            Self::WarmthNormalised => {
                "The warmth was brought into line with the rest of this part."
            }
            Self::ExposureNormalised => {
                "The brightness was brought into line with the rest of this part."
            }
            Self::GradeHarmonised => {
                "The contrast and colour character were brought into line with the rest of this \
                 part."
            }
            Self::SkinNormalised => {
                "This person's skin was brought into line with how they look across the rest of \
                 the wedding."
            }
            Self::AlreadyConsistent => "This frame already matched the rest of this part.",
            Self::BoundedByPolicy => {
                "This frame was a long way from the rest, so AURA moved it as far as it is \
                 allowed to and no further."
            }
            Self::MoodPreserved => {
                "The light here is meant to be this colour, so AURA left it alone."
            }
            Self::MixedLightSkipped => {
                "There is more than one kind of light in this frame, so a single correction \
                 would have been wrong somewhere in it."
            }
            Self::UserEdited => "You set this frame yourself, so AURA has not touched it.",
            Self::Disabled => "Gallery matching is switched off for this frame.",
            Self::ToneEstimateAbsent => {
                "AURA has not worked out the light in this frame yet, so there is nothing to \
                 match."
            }
            Self::ColourDecisionAbsent => {
                "AURA has not graded this frame yet, so its contrast and colour were left alone."
            }
            Self::SkinMaskAbsent => {
                "AURA cannot yet tell which pixels are this person's skin, so it has not \
                 adjusted any of them."
            }
            Self::SkinTargetAbsent => {
                "AURA has not seen this person in enough well-lit frames to know how they \
                 should look, so it has left their skin alone."
            }
            Self::SegmentAbsent => {
                "This frame is not part of any chapter yet, so there is nothing to match it to."
            }
            Self::OutlierAfterNormalisation => {
                "This frame is still noticeably different from the rest of this part."
            }
            Self::SkinOutlier => {
                "This person's skin still looks different here from how it looks elsewhere."
            }
            Self::AnchorsDisagree => {
                "The frames AURA picked as anchors disagree with each other, so it has not \
                 matched anything to them."
            }
        }
    }

    /// Parse the stored slug. Unknown text reads as [`GalleryCode::NodeAnchored`].
    #[must_use]
    pub fn from_str_or_anchored(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|code| code.as_str() == text)
            .unwrap_or(Self::NodeAnchored)
    }

    /// True when this code says the product declined to act or acted on less than it wanted.
    ///
    /// The twelve the module header counts. Asked in one place because the outline, the panel
    /// and `docs/gallery-consistency.md` all need the same answer, and a product that could
    /// not tell a refusal from an action would report a wedding it left alone as a wedding it
    /// made consistent.
    #[must_use]
    pub const fn withdraws(self) -> bool {
        matches!(
            self,
            Self::NodeUnanchored
                | Self::SplitTooSmall
                | Self::AlreadyConsistent
                | Self::BoundedByPolicy
                | Self::MoodPreserved
                | Self::MixedLightSkipped
                | Self::UserEdited
                | Self::Disabled
                | Self::ToneEstimateAbsent
                | Self::ColourDecisionAbsent
                | Self::SkinMaskAbsent
                | Self::SkinTargetAbsent
        )
    }

    /// The bit this code occupies in a stored reason set.
    ///
    /// Twenty-six codes fit in a `u32`, so a frame's reasons are one integer column rather than
    /// a list of slugs. That is 8 bytes against about sixty, on the one table in this phase that
    /// carries a row per photograph, and section 11 budgets 500 B per image for the whole of it.
    #[must_use]
    pub const fn bit(self) -> u32 {
        1u32 << (self as u32)
    }

    /// Every code in a stored reason set, in [`GalleryCode::ALL`] order.
    #[must_use]
    pub fn from_bits(bits: u32) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|code| bits & code.bit() != 0)
            .collect()
    }

    /// Pack a set of codes into one integer.
    #[must_use]
    pub fn to_bits(codes: &[Self]) -> u32 {
        codes.iter().fold(0u32, |acc, code| acc | code.bit())
    }

    /// How much this code contributes to a panel's ordering, `0..1`.
    ///
    /// **A property of the code rather than of the frame**, which is why it is rendered rather
    /// than stored - the same argument phase 09 made for the sentence, taken one step further.
    /// A weight that varied per frame would be a number a panel sorts by and nothing measures,
    /// and storing twenty-six of them per photograph would cost more than every other column on
    /// the row put together.
    ///
    /// The ordering it produces: what the product refused to do comes first, then what it did,
    /// then how the node was built. A photographer scanning a frame wants to know why it was
    /// left alone before they want to know that its node was sub-clustered.
    #[must_use]
    pub const fn default_weight(self) -> f32 {
        match self {
            // Refusals and caps. What a photographer needs to see first.
            Self::NodeUnanchored | Self::AnchorsDisagree | Self::MoodPreserved => 1.0,
            Self::UserEdited | Self::Disabled => 0.95,
            Self::BoundedByPolicy | Self::MixedLightSkipped => 0.9,
            Self::ToneEstimateAbsent
            | Self::ColourDecisionAbsent
            | Self::SkinMaskAbsent
            | Self::SkinTargetAbsent
            | Self::SegmentAbsent => 0.85,
            // Outliers.
            Self::OutlierAfterNormalisation | Self::SkinOutlier => 0.8,
            // What actually moved.
            Self::WarmthNormalised | Self::ExposureNormalised => 0.7,
            Self::SkinNormalised => 0.65,
            Self::GradeHarmonised => 0.6,
            Self::AlreadyConsistent => 0.5,
            // How the node was built. Context rather than a decision about this frame.
            Self::NodeSplitByChangePoint | Self::SplitTooSmall => 0.4,
            Self::AnchorPinned | Self::AnchorRejected => 0.35,
            Self::RobustTarget | Self::NodeSubClustered => 0.3,
            Self::NodeAnchored => 0.2,
        }
    }

    /// True when this code describes a frame phase 27 should look at.
    ///
    /// Section 2.1: outliers are "exactly the Phase 27 QC input". Asked here so the QC agent
    /// does not re-derive the set from thresholds this phase owns.
    #[must_use]
    pub const fn is_outlier(self) -> bool {
        matches!(
            self,
            Self::OutlierAfterNormalisation | Self::SkinOutlier | Self::AnchorsDisagree
        )
    }
}

impl fmt::Display for GalleryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One reason a frame moved, or did not.
///
/// The shape phases 09 to 24 all settled on: a typed code, a rendered sentence that is never
/// stored, and a weight so a panel can order them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GalleryReason {
    /// What happened.
    pub code: GalleryCode,
    /// The sentence, rendered from the code at read time. Never persisted.
    pub text: String,
    /// How much this reason contributed, `0..1`. Ordering in a panel, nothing else.
    pub weight: f32,
}

impl GalleryReason {
    /// Build a reason from a code, rendering its sentence.
    #[must_use]
    pub fn new(code: GalleryCode, weight: f32) -> Self {
        Self {
            code,
            text: code.user_text().to_string(),
            weight: weight.clamp(0.0, 1.0),
        }
    }

    /// Build a reason from a code at the code's own weight.
    ///
    /// What a stored reason set reads back as. See [`GalleryCode::default_weight`].
    #[must_use]
    pub fn of(code: GalleryCode) -> Self {
        Self::new(code, code.default_weight())
    }

    /// Read a stored reason set back, strongest first.
    ///
    /// Ties break on [`GalleryCode::ALL`] order, so two runs of the same frame produce the same
    /// list in the same order. Invariant 4.
    #[must_use]
    pub fn from_bits(bits: u32) -> Vec<Self> {
        let mut out: Vec<Self> = GalleryCode::from_bits(bits)
            .into_iter()
            .map(Self::of)
            .collect();
        out.sort_by(|a, b| {
            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.code.cmp(&b.code))
        });
        out
    }

    /// Pack a list of reasons into the integer a row stores.
    #[must_use]
    pub fn to_bits(reasons: &[Self]) -> u32 {
        reasons
            .iter()
            .fold(0u32, |acc, reason| acc | reason.code.bit())
    }
}

// ---------------------------------------------------------------------------
// The tree
// ---------------------------------------------------------------------------

/// One node of the scene tree: a run of photographs that should look like each other.
///
/// Section 3's tree - `Ceremony > Entrance/Ritual/Couple/Reactions` - built from phase 07's
/// segments plus sub-clustering inside long ones, then split further wherever the light
/// genuinely changed.
///
/// **A node is not a segment.** A segment is a chapter of the story and is what a photographer
/// renames; a node is a *lighting group* and is what a target is computed over. One segment
/// becomes several nodes when a flash goes on, when the sun sets, or when it is simply long
/// enough that its first hour and its last do not describe the same room.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SceneNode {
    /// This node.
    pub id: NodeId,
    /// The node it was split or sub-clustered out of, when it was.
    pub parent: Option<NodeId>,
    /// The chapter it belongs to.
    pub segment_id: SegmentId,
    /// What to call it. Derived from the segment's chapter plus an ordinal when it was split.
    pub label: String,
    /// Its frames, in capture order.
    ///
    /// Capture order rather than any other, because the change-point detector, the timeline
    /// strips and the idempotence test all read this as a sequence and a set-derived order
    /// would make two runs produce two answers.
    pub image_ids: Vec<ImageId>,
    /// The frames the target was computed from, best first.
    ///
    /// Between [`MIN_ANCHORS`] and [`MAX_ANCHORS`] when the node is anchored, and **empty**
    /// when it is not - which is [`GalleryCode::NodeUnanchored`] and not a failure.
    pub anchors: Vec<ImageId>,
    /// What the anchors say this node should look like, or `None` when there are none.
    ///
    /// `Option` rather than a neutral default, because a neutral target is one every frame
    /// would be normalised toward and the whole point of an unanchored node is that nothing
    /// is.
    pub target: Option<NodeTarget>,
    /// The dominant scene of the segment this node came from.
    pub scene: SceneId,
    /// Why the node is shaped the way it is.
    pub reasons: Vec<GalleryReason>,
    /// Which build's arithmetic built it.
    pub analysis_ver: u16,
    /// Which policy table its bounds came from.
    pub policy_ver: u16,
}

impl SceneNode {
    /// True when this node has enough anchors to normalise anything toward.
    #[must_use]
    pub fn is_anchored(&self) -> bool {
        self.target.is_some() && self.anchors.len() >= MIN_ANCHORS
    }

    /// How many frames it contains.
    #[must_use]
    pub fn len(&self) -> usize {
        self.image_ids.len()
    }

    /// True when it contains nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.image_ids.is_empty()
    }
}

/// What a node's anchors say the node should look like.
///
/// Every field is a **robust** statistic over the anchors - a trimmed mean for the scalars, a
/// component-wise median for the chromaticity - because one anchor that is wrong should not
/// move a target computed over three of them. Section 6.1.
///
/// The tolerances beside each value are what "already consistent" means for this node, and
/// they are per-node rather than global: a candle-lit vow varies more frame to frame than an
/// outdoor portrait session does, and one tolerance is right for exactly one of them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NodeTarget {
    /// The correlated colour temperature the node should sit at, in kelvin.
    pub cct_k: f32,
    /// How far from it a frame may sit and still be called consistent, in kelvin.
    pub cct_tol: f32,
    /// The tint the node should sit at, in the recipe's units.
    pub tint: f32,
    /// How far from it a frame may sit, in the recipe's units.
    pub tint_tol: f32,
    /// The subject luminance the node should sit at, `0..1`.
    ///
    /// Subject-referred, like phase 15's: the faces in the frame weighted by prominence, not
    /// the mean luminance of the room. A node exposed on its mean and a node exposed on the
    /// bride's cheek are two different galleries.
    pub subject_luma: f32,
    /// How far from it a frame may sit, `0..1`.
    pub luma_tol: f32,
    /// The contrast character, in the recipe's `-100..100` units.
    pub contrast: f32,
    /// The saturation character, in the recipe's `-100..100` units.
    pub saturation: f32,
    /// A compact descriptor of the node's colour character.
    ///
    /// Section 5's `grade_signature: [f32; 8]`. Eight numbers: the mean and spread of the
    /// shadow hue, the mean and spread of the highlight hue, the shadow and highlight chroma,
    /// the mid-tone slope and the black point. Enough to tell "warm shadows, clean highlights"
    /// from "cool shadows, warm highlights", which is what makes two frames of the same room
    /// read as one look or as two.
    ///
    /// It is compared, never applied. Nothing in this contract turns a signature back into
    /// parameters - phase 16 owns the grade and this phase owns the distance between two of
    /// them.
    pub grade_signature: [f32; 8],
    /// How many anchors it was computed from.
    pub anchor_count: u16,
    /// How much the anchors agree with each other, `0..1`. One is a perfect agreement.
    ///
    /// Below [`NodeTarget::MIN_COHESION`] the node emits [`GalleryCode::AnchorsDisagree`] and
    /// normalises nothing: anchors that disagree more than the node's own frames do is an
    /// anchor-selection failure, and normalising toward the middle of it would make a coherent
    /// node incoherent. Section 12's second failure mode.
    pub cohesion: f32,
}

impl NodeTarget {
    /// The agreement below which a target is not used.
    pub const MIN_COHESION: f32 = 0.35;

    /// How many numbers a grade signature carries.
    pub const SIGNATURE_LEN: usize = 8;

    /// True when this target has enough agreement behind it to move anything.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        self.cohesion >= Self::MIN_COHESION && self.anchor_count as usize >= MIN_ANCHORS
    }

    /// True when a frame at these values is already inside the node's tolerances.
    ///
    /// The one place "already consistent" is decided, so the solver, the outline, the outlier
    /// detector and the panel cannot disagree about where the edge is.
    #[must_use]
    pub fn contains(&self, cct_k: f32, tint: f32, subject_luma: f32) -> bool {
        (cct_k - self.cct_k).abs() <= self.cct_tol
            && (tint - self.tint).abs() <= self.tint_tol
            && (subject_luma - self.subject_luma).abs() <= self.luma_tol
    }

    /// The distance between two grade signatures, `0..1`.
    ///
    /// Euclidean over the eight components, each already normalised to `0..1` by the code that
    /// measures it, then scaled by the length so the answer does not depend on
    /// [`NodeTarget::SIGNATURE_LEN`].
    #[must_use]
    pub fn signature_distance(a: &[f32; 8], b: &[f32; 8]) -> f32 {
        let sum: f32 = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| {
                let d = x - y;
                d * d
            })
            .sum();
        #[allow(clippy::cast_precision_loss)]
        let scaled = sum / Self::SIGNATURE_LEN as f32;
        scaled.sqrt().clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Skin
// ---------------------------------------------------------------------------

/// Where one identity's skin sits across a whole gallery.
///
/// Not in section 5. Section 6.3 requires it - "for each identity, collect skin chromaticity
/// and luminance from frames with high mask confidence and good exposure; take the robust
/// central tendency as that person's target appearance" - and a per-identity target that lives
/// only inside one solver is a target the next phase re-derives differently.
///
/// ## Why this is per identity rather than per skin-tone group
///
/// Phase 15's answer, unchanged and for the same reason: because a group is a guess about a
/// person and a person is not a guess. This carries a pair of chromaticity coordinates, a
/// luminance and a spread. It carries no label, no bucket and no category, and **nothing in
/// this contract can express one**. There is no ideal-skin constant here, in the config file,
/// in migration 25 or anywhere in the code path, and the phase gate scans for one on every
/// run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkinTarget {
    /// Whose skin this describes.
    pub identity: IdentityId,
    /// The gallery-wide central chromaticity, in CIE 1976 `u'v'`.
    pub uv: [f32; 2],
    /// The gallery-wide central luminance, `0..1`.
    pub luma: f32,
    /// How many frames contributed.
    ///
    /// Below [`MIN_SKIN_FRAMES`] the target constrains nothing and
    /// [`GalleryCode::SkinTargetAbsent`] is emitted instead.
    pub frames: u32,
    /// The dE00 spread across those frames **before** any correction.
    ///
    /// The number section 10.1's gate is measured against, kept on the row so the improvement
    /// is a subtraction rather than a re-measurement of a moving target.
    pub spread_before: f32,
    /// The dE00 spread **after** the corrections this pass planned.
    pub spread_after: f32,
    /// Which build's arithmetic measured it.
    pub analysis_ver: u16,
}

impl SkinTarget {
    /// True when this target has enough evidence behind it to move anything.
    #[must_use]
    pub const fn is_usable(&self) -> bool {
        self.frames >= MIN_SKIN_FRAMES
    }

    /// True when this identity meets section 6.3's promise.
    #[must_use]
    pub fn meets_promise(&self) -> bool {
        self.spread_after <= SKIN_DE00_SPREAD_CEILING
    }
}

/// What was done to one person's skin in one frame, to bring it into line with the rest.
///
/// Section 5 prints `Option<SkinCorrection>` without defining the type. It applies **inside
/// phase 18's identity-scoped skin mask only** - there is no field here for a region, because
/// the region is not this phase's to choose - and it is a residual on top of phase 16's grade,
/// which re-runs its skin guard afterwards. Phase 17's rule, third application.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkinCorrection {
    /// Whose skin.
    pub identity: IdentityId,
    /// How far the chromaticity moves, in CIE 1976 `u'v'`.
    pub d_uv: [f32; 2],
    /// How far the luminance moves, `0..1`.
    pub d_luma: f32,
    /// The dE00 between this frame's skin and the identity's target, before the correction.
    pub de00_before: f32,
    /// The dE00 after it.
    pub de00_after: f32,
    /// The cap that applied to this frame, in `u'v'`.
    ///
    /// [`SKIN_CHROMA_CAP`] scaled down by the frame's mood weight; see
    /// [`SkinCorrection::cap_for_mood`]. Stored rather than recomputed because the mood weight
    /// comes from phase 15's illuminant and a panel showing "capped" needs to say by how much.
    pub cap: f32,
    /// True when the cap bit.
    pub capped: bool,
}

impl SkinCorrection {
    /// The chroma cap that applies to a frame, given how intentional its light is.
    ///
    /// Section 6.3: "capped so lighting mood is preserved (a candle-lit face may stay warm,
    /// but not magenta)". `mood` is `0..1`, zero for ordinary light and one for light phase 15
    /// called intentional; the cap falls to a fifth of its value at one rather than to zero,
    /// because a magenta cast on a candle-lit face is still a magenta cast and the promise
    /// this phase makes about skin is not switched off by the room.
    #[must_use]
    pub fn cap_for_mood(mood: f32) -> f32 {
        let mood = mood.clamp(0.0, 1.0);
        SKIN_CHROMA_CAP * (1.0 - 0.8 * mood)
    }

    /// How much of the gap to the target this correction closed, `0..1`.
    #[must_use]
    pub fn closed(&self) -> f32 {
        if self.de00_before <= f32::EPSILON {
            return 1.0;
        }
        (1.0 - self.de00_after / self.de00_before).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// The delta
// ---------------------------------------------------------------------------

/// How far one frame moves toward its node, and why it did not move further.
///
/// **Every `d_` field is a residual on top of phase 15's and phase 16's stored answer**, not
/// an absolute. See the module header; a caller that adds `d_cct` to anything but
/// [`NormalisationDelta::from_cct_k`] has misunderstood the shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NormalisationDelta {
    /// The photograph.
    pub image_id: ImageId,
    /// The node it was normalised inside.
    pub node_id: NodeId,
    /// How far the exposure moves, in stops. Bounded by [`MAX_D_EXPOSURE_EV`].
    pub d_exposure: f32,
    /// How far the correlated colour temperature moves, in kelvin. Bounded by [`MAX_D_CCT_K`].
    pub d_cct: f32,
    /// How far the tint moves, in the recipe's units. Bounded by [`MAX_D_TINT`].
    pub d_tint: f32,
    /// How far the contrast moves, in the recipe's units. Bounded by [`MAX_D_CONTRAST`].
    pub d_contrast: f32,
    /// How far the saturation moves, in the recipe's units. Bounded by [`MAX_D_SATURATION`].
    pub d_saturation: f32,
    /// What was done to the skin, when anything was.
    pub skin_correction: Option<SkinCorrection>,
    /// The exposure this delta is a residual from, in stops.
    ///
    /// A record rather than a mechanism. Nothing reads these three back into the solver;
    /// they are here so a panel can draw the movement and an audit can check it.
    pub from_exposure_ev: f32,
    /// The temperature this delta is a residual from, in kelvin.
    pub from_cct_k: f32,
    /// The tint this delta is a residual from.
    pub from_tint: f32,
    /// The damping that was applied, within [`DAMPING_RANGE`].
    pub damping: f32,
    /// Which bound clamped the movement, when one did.
    pub bounded_by: Option<Bound>,
    /// Why it moved, or did not.
    pub reasons: Vec<GalleryReason>,
    /// How much the product believes this delta, `0..1`.
    ///
    /// Invariant 2. Built from the node's cohesion, the frame's own phase 15 confidence and
    /// how much of the movement survived the bounds - a frame clamped hard is a frame the
    /// product is *less* sure about, because the clamp says the node and the frame disagree
    /// about what room they are in.
    pub confidence: f32,
    /// The photographer set these values by hand and automation never overwrites them.
    pub user_edited: bool,
    /// Which build's arithmetic solved it.
    pub analysis_ver: u16,
    /// Which policy table its bounds and damping came from.
    pub policy_ver: u16,
}

impl NormalisationDelta {
    /// True when nothing moved on any axis.
    ///
    /// Not the same as "this frame was fine": a frame with no tone estimate, a frame the
    /// photographer edited and a frame already inside its tolerances all produce a zero delta
    /// and three different codes.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.d_exposure == 0.0
            && self.d_cct == 0.0
            && self.d_tint == 0.0
            && self.d_contrast == 0.0
            && self.d_saturation == 0.0
            && self.skin_correction.is_none()
    }

    /// How far this delta moves the frame, as a fraction of the bounds, `0..1`.
    ///
    /// The largest of the five ratios rather than their mean: a frame moved to the limit on
    /// one axis has been moved to the limit, and averaging that with four zeroes reports it as
    /// a fifth of a movement.
    #[must_use]
    pub fn magnitude(&self) -> f32 {
        [
            (self.d_exposure / MAX_D_EXPOSURE_EV).abs(),
            (self.d_cct / MAX_D_CCT_K).abs(),
            (self.d_tint / MAX_D_TINT).abs(),
            (self.d_contrast / MAX_D_CONTRAST).abs(),
            (self.d_saturation / MAX_D_SATURATION).abs(),
        ]
        .into_iter()
        .fold(0.0_f32, f32::max)
        .clamp(0.0, 1.0)
    }

    /// True when every axis is inside its contract ceiling.
    ///
    /// Section 10.1's bounds gate, asked on the shape rather than in the harness so a store
    /// round trip and a solver are held to the same rule.
    #[must_use]
    pub fn within_bounds(&self) -> bool {
        self.d_exposure.abs() <= MAX_D_EXPOSURE_EV + f32::EPSILON
            && self.d_cct.abs() <= MAX_D_CCT_K + f32::EPSILON
            && self.d_tint.abs() <= MAX_D_TINT + f32::EPSILON
            && self.d_contrast.abs() <= MAX_D_CONTRAST + f32::EPSILON
            && self.d_saturation.abs() <= MAX_D_SATURATION + f32::EPSILON
    }

    /// True when two solves of the same frame agree to within [`IDEMPOTENCE_EPSILON`].
    ///
    /// Expressed as a fraction of each bound so one epsilon covers five axes at five scales.
    /// Section 10.1's idempotence gate, and the one place it is decided.
    #[must_use]
    pub fn agrees_with(&self, other: &Self) -> bool {
        let axes = [
            (self.d_exposure - other.d_exposure, MAX_D_EXPOSURE_EV),
            (self.d_cct - other.d_cct, MAX_D_CCT_K),
            (self.d_tint - other.d_tint, MAX_D_TINT),
            (self.d_contrast - other.d_contrast, MAX_D_CONTRAST),
            (self.d_saturation - other.d_saturation, MAX_D_SATURATION),
        ];
        axes.into_iter()
            .all(|(diff, scale)| (diff / scale).abs() <= IDEMPOTENCE_EPSILON)
    }
}

// ---------------------------------------------------------------------------
// Outliers
// ---------------------------------------------------------------------------

/// A frame that is still noticeably different from its node after normalising.
///
/// Not in section 5. Section 6.4 makes outliers a first-class output and section 2.1 makes
/// them "exactly the Phase 27 QC input"; a handoff with no type is a handoff nobody can test.
///
/// **Measured on the residual, not on the raw deviation.** A frame 900 K from its node that
/// the bound could only move 450 K is an outlier with a 450 K residual; a frame 300 K away
/// that was corrected in full is not an outlier at all. ADR-0051 section 7.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Outlier {
    /// The photograph.
    pub image_id: ImageId,
    /// The node it should have matched.
    pub node_id: NodeId,
    /// What is left in kelvin after the delta was applied. Signed: positive is warmer.
    pub residual_cct: f32,
    /// What is left in tint units. Signed: positive is magenta.
    pub residual_tint: f32,
    /// What is left in stops. Signed: positive is brighter.
    pub residual_exposure: f32,
    /// What is left on the worst identity's skin, in dE00. Zero when no skin was measured.
    pub residual_skin_de00: f32,
    /// The identity whose skin is furthest out, when one is.
    pub worst_identity: Option<IdentityId>,
    /// How far out this frame is overall, `0..1`, as a fraction of the bounds.
    pub deviation: f32,
    /// Why it is out.
    pub reasons: Vec<GalleryReason>,
    /// Which build's arithmetic measured it.
    pub analysis_ver: u16,
}

impl Outlier {
    /// The sentence section 6.4 asks for: "+310 K warmer than node anchors, magenta skin cast
    /// 4.2 dE00".
    ///
    /// Assembled from the residuals rather than stored, for the reason every reason sentence
    /// in this product is: a stored sentence is copy a release can change.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.residual_cct.abs() >= 1.0 {
            let word = if self.residual_cct > 0.0 {
                "warmer"
            } else {
                "cooler"
            };
            parts.push(format!(
                "{:+.0} K {word} than the anchors",
                self.residual_cct
            ));
        }
        if self.residual_tint.abs() >= 0.5 {
            let word = if self.residual_tint > 0.0 {
                "magenta"
            } else {
                "green"
            };
            parts.push(format!("{:+.1} {word}", self.residual_tint));
        }
        if self.residual_exposure.abs() >= 0.02 {
            let word = if self.residual_exposure > 0.0 {
                "brighter"
            } else {
                "darker"
            };
            parts.push(format!("{:+.2} EV {word}", self.residual_exposure));
        }
        if self.residual_skin_de00 >= 0.1 {
            parts.push(format!("skin cast {:.1} dE00", self.residual_skin_de00));
        }
        if parts.is_empty() {
            "within tolerance on every axis".to_string()
        } else {
            parts.join(", ")
        }
    }
}

// ---------------------------------------------------------------------------
// Coverage
// ---------------------------------------------------------------------------

/// What a project's consistency pass covered and what it found.
///
/// Not in section 5. Phase 05's rule, inherited for the nineteenth time: report coverage when
/// you report a result, and say what the denominator is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GalleryOutline {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs that belong to a node.
    pub placed: u32,
    /// Photographs with a solved delta.
    pub normalised: u32,
    /// [`GalleryOutline::normalised`] over [`GalleryOutline::photos`], `0..1`.
    ///
    /// The denominator is **every photograph**, as phases 09, 10 and 15 count. A photograph
    /// with no scene, no tone estimate or no segment is a gap in this pass whatever caused it,
    /// and reporting against the placed count would hide the largest failure the pass has.
    pub coverage: f32,
    /// Nodes in the tree.
    pub nodes: u32,
    /// Nodes with a usable target.
    ///
    /// The second number, and the one that matters when it is low: a project at 100 % coverage
    /// and 20 % anchored has had almost nothing normalised, because an unanchored node
    /// produces a zero delta for every frame in it and a zero delta is still a row.
    pub anchored_nodes: u32,
    /// Nodes that exist because a change point split one.
    pub split_nodes: u32,
    /// Anchors a photographer pinned.
    pub pinned_anchors: u32,
    /// Frames whose movement a bound clamped.
    pub bounded: u32,
    /// Frames left alone because their light is intentional.
    pub mood_preserved: u32,
    /// Frames the photographer had already set by hand.
    pub user_edited: u32,
    /// Frames still out of line after normalising.
    pub outliers: u32,
    /// Identities with a usable skin target.
    pub skin_targeted: u32,
    /// Identities seen at all.
    pub identities: u32,
    /// The mean absolute temperature movement, in kelvin, over frames that moved.
    pub mean_d_cct: f32,
    /// The mean absolute exposure movement, in stops, over frames that moved.
    pub mean_d_ev: f32,
    /// The within-node temperature spread before normalising, in kelvin, averaged over nodes.
    ///
    /// Section 10.1's headline gate is the reduction from this to
    /// [`GalleryOutline::spread_after_cct`], and both are on the outline so the claim is a
    /// subtraction a panel can show rather than a number only the harness knows.
    pub spread_before_cct: f32,
    /// The within-node temperature spread after normalising, in kelvin.
    pub spread_after_cct: f32,
    /// The within-node exposure spread before normalising, in stops.
    pub spread_before_ev: f32,
    /// The within-node exposure spread after normalising, in stops.
    pub spread_after_ev: f32,
    /// The worst per-identity skin dE00 spread after correction.
    ///
    /// Section 6.3's promise as a query rather than a sentence: at or below
    /// [`SKIN_DE00_SPREAD_CEILING`] the promise holds for every identity in the project.
    pub worst_skin_spread: f32,
    /// Scenes with no policy row, so nothing about them could be scene-conditioned.
    pub untargeted_scenes: Vec<String>,
    /// Which build's arithmetic produced these numbers.
    pub analysis_ver: u16,
    /// Which policy table they were bounded by.
    pub policy_ver: u16,
}

impl GalleryOutline {
    /// The share of the temperature spread that normalising removed, `0..1`.
    ///
    /// Section 10.1's first gate: "within-scene WB spread reduced >= 60 %". Computed here
    /// rather than in the harness so the panel, the gate and the exit report cannot disagree.
    #[must_use]
    pub fn cct_spread_reduction(&self) -> f32 {
        if self.spread_before_cct <= f32::EPSILON {
            return 0.0;
        }
        (1.0 - self.spread_after_cct / self.spread_before_cct).clamp(0.0, 1.0)
    }

    /// The share of the exposure spread that normalising removed, `0..1`.
    ///
    /// Section 10.1's second gate: "exposure spread >= 50 %".
    #[must_use]
    pub fn ev_spread_reduction(&self) -> f32 {
        if self.spread_before_ev <= f32::EPSILON {
            return 0.0;
        }
        (1.0 - self.spread_after_ev / self.spread_before_ev).clamp(0.0, 1.0)
    }

    /// The share of nodes that could be anchored, `0..1`.
    #[must_use]
    pub fn anchored_share(&self) -> f32 {
        if self.nodes == 0 {
            return 0.0;
        }
        f32::from(u16::try_from(self.anchored_nodes).unwrap_or(u16::MAX))
            / f32::from(u16::try_from(self.nodes).unwrap_or(u16::MAX))
    }
}

// ---------------------------------------------------------------------------
// Overrides
// ---------------------------------------------------------------------------

/// What a photographer set instead, on one frame.
///
/// Every field is optional and at least one must be present; an empty override is refused
/// rather than silently accepted, because an empty override that set `user_edited` would take
/// a frame out of automation without changing anything about it.
///
/// **There is no strength field, and there is no way to raise a bound.** Phase 21's rule - a
/// ceiling can be lowered by a studio and raised by nobody - applied to a surface a
/// photographer touches. A frame that needs to move further than [`MAX_D_CCT_K`] is a frame
/// whose per-frame estimate is wrong, and phase 15's own override is where that is fixed.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GalleryOverride {
    /// The temperature movement to use instead, in kelvin, within [`MAX_D_CCT_K`].
    pub d_cct: Option<f32>,
    /// The tint movement to use instead, within [`MAX_D_TINT`].
    pub d_tint: Option<f32>,
    /// The exposure movement to use instead, in stops, within [`MAX_D_EXPOSURE_EV`].
    pub d_exposure: Option<f32>,
    /// The contrast movement to use instead, within [`MAX_D_CONTRAST`].
    pub d_contrast: Option<f32>,
    /// The saturation movement to use instead, within [`MAX_D_SATURATION`].
    pub d_saturation: Option<f32>,
}

impl GalleryOverride {
    /// True when nothing was set.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.d_cct.is_none()
            && self.d_tint.is_none()
            && self.d_exposure.is_none()
            && self.d_contrast.is_none()
            && self.d_saturation.is_none()
    }

    /// True when every value that was set is inside its contract ceiling.
    #[must_use]
    pub fn within_bounds(&self) -> bool {
        let ok = |value: Option<f32>, ceiling: f32| {
            value.is_none_or(|v| v.is_finite() && v.abs() <= ceiling + f32::EPSILON)
        };
        ok(self.d_cct, MAX_D_CCT_K)
            && ok(self.d_tint, MAX_D_TINT)
            && ok(self.d_exposure, MAX_D_EXPOSURE_EV)
            && ok(self.d_contrast, MAX_D_CONTRAST)
            && ok(self.d_saturation, MAX_D_SATURATION)
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask what a whole gallery looks like as one body of work.
///
/// Twenty-first service of its kind and the first whose subject is a **set** of photographs.
/// Phase 26 matches a second camera into these nodes, phase 27 reads these outliers as its QC
/// input, phase 28 acts on them unattended and phase 29 builds albums out of a gallery this
/// phase has already made coherent. No phase may keep its own scene-node tree, its own anchor
/// selection or its own idea of what a consistent gallery is - two answers to "what should
/// this chapter look like" is an album that does not match the gallery.
///
/// **Nothing here writes a recipe or moves a pixel.** The deltas are stored; `aura-app` merges
/// an accepted one through `aura_recipe::schema::merge`, which is the only function in the
/// workspace permitted to write a recipe and the only place `user_edited_fields` is honoured.
/// There is no `apply` on this trait and adding one would need an ADR.
pub trait GalleryService: Send + Sync + fmt::Debug {
    /// What a project's consistency pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<GalleryOutline>;

    /// Every node of a project's tree, in capture order of their first frame.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the nodes cannot be read.
    fn nodes(&self, project: ProjectId) -> AuraResult<Vec<SceneNode>>;

    /// One node, or `None` when it is unknown.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the node cannot be read.
    fn node(&self, node: NodeId) -> AuraResult<Option<SceneNode>>;

    /// Which node a photograph belongs to, or `None` when it belongs to none.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read.
    fn node_of(&self, image: ImageId) -> AuraResult<Option<NodeId>>;

    /// One photograph's delta, or `None` when it has not been solved.
    ///
    /// `None` is not a zero delta, and a caller that renders it as one has turned a gap in
    /// coverage into a decision.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the delta cannot be read.
    fn delta(&self, image: ImageId) -> AuraResult<Option<NormalisationDelta>>;

    /// Every delta inside one node, in capture order.
    ///
    /// What the timeline strips read. A node rather than a project, because a strip is drawn
    /// per node and a 4,000-image wedding would otherwise send the whole gallery to draw one.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the deltas cannot be read.
    fn deltas_in(&self, node: NodeId) -> AuraResult<Vec<NormalisationDelta>>;

    /// Every frame still out of line, worst first.
    ///
    /// What phase 27 reads. `limit` bounds the answer because the queue is a page.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the outliers cannot be read.
    fn outliers(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<Outlier>>;

    /// One identity's gallery skin target, or `None` when they have too few frames.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the target cannot be read.
    fn skin_target(&self, identity: IdentityId) -> AuraResult<Option<SkinTarget>>;

    /// Every usable skin target in a project.
    ///
    /// Read once per pass rather than per frame, for the reason phase 15's `skin_loci` is: a
    /// 4,000-image wedding with forty identities would otherwise make 160,000 round trips
    /// through a service whose answer changes twice an hour.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the targets cannot be read.
    fn skin_targets(&self, project: ProjectId) -> AuraResult<Vec<SkinTarget>>;

    /// Pin a photograph as an anchor of its node, and re-solve that node.
    ///
    /// Section 6.1: "users can pin or reject anchors in the UI; pinned anchors are
    /// authoritative". A pinned anchor survives every re-analysis - the check is inside the
    /// statement that would overwrite it, exactly as `identities.user_locked`,
    /// `segments.user_locked`, `moments.user_locked` and `masks.user_edited` are.
    ///
    /// Only that node is re-solved. A node's target depends on its own anchors and on nothing
    /// outside it, which is what makes section 11's 6-second incremental budget a property of
    /// the structure rather than an optimisation.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5124` when the node is unknown, when the photograph is not in it, or when
    /// pinning would exceed [`MAX_ANCHORS`].
    fn pin_anchor(&self, node: NodeId, image: ImageId) -> Result<(), AuraError>;

    /// Reject a photograph as an anchor of its node, and re-solve that node.
    ///
    /// A rejection is as durable as a pin: automation never re-selects a frame a photographer
    /// has thrown out.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5124` when the node is unknown, when the photograph is not in it, or when
    /// rejecting would leave fewer than [`MIN_ANCHORS`].
    fn reject_anchor(&self, node: NodeId, image: ImageId) -> Result<(), AuraError>;

    /// Record what the photographer set instead, on one frame.
    ///
    /// Sets [`NormalisationDelta::user_edited`], and is not undone by a re-analysis.
    ///
    /// **This records the disagreement; it does not move a pixel.** The pixels move when the
    /// caller writes the same values through `aura_recipe::schema::merge`. Two writes rather
    /// than one, deliberately: a service that could do both would be a second way to edit a
    /// recipe. Phase 15 wrote this rule and this is its fourth application.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5125` when the photograph has no delta, when the override is empty, or when a
    /// value is outside its documented bound.
    fn set_override(&self, image: ImageId, values: GalleryOverride) -> Result<(), AuraError>;

    /// Switch the consistency pass off for one photograph, or back on.
    ///
    /// A kill switch per frame, which is invariant 8's "feature-flag every AI stage" at the
    /// grain a photographer actually wants: one frame in a gallery that should not be matched
    /// to anything.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5125` when the photograph is unknown.
    fn set_enabled(&self, image: ImageId, enabled: bool) -> Result<(), AuraError>;
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::assertions_on_constants,
    clippy::cast_precision_loss
)]
mod tests {
    use super::*;

    #[test]
    fn every_code_has_a_distinct_slug() {
        let mut slugs: Vec<&str> = GalleryCode::ALL.iter().map(|c| c.as_str()).collect();
        slugs.sort_unstable();
        let before = slugs.len();
        slugs.dedup();
        assert_eq!(before, slugs.len(), "two codes share a slug");
        assert_eq!(before, GalleryCode::COUNT);
    }

    #[test]
    fn every_code_round_trips_through_its_slug() {
        for code in GalleryCode::ALL {
            assert_eq!(GalleryCode::from_str_or_anchored(code.as_str()), code);
        }
    }

    #[test]
    fn every_code_has_a_sentence() {
        for code in GalleryCode::ALL {
            assert!(!code.user_text().is_empty(), "{code} has no sentence");
        }
    }

    #[test]
    fn twelve_codes_withdraw_a_claim() {
        let withdrawing = GalleryCode::ALL.iter().filter(|c| c.withdraws()).count();
        assert_eq!(withdrawing, 12, "the module header counts twelve");
    }

    #[test]
    fn three_codes_are_outliers() {
        let outliers = GalleryCode::ALL.iter().filter(|c| c.is_outlier()).count();
        assert_eq!(outliers, 3);
    }

    #[test]
    fn every_bound_round_trips_and_has_a_ceiling() {
        assert_eq!(Bound::ALL.len(), Bound::COUNT);
        for bound in Bound::ALL {
            assert_eq!(Bound::from_str_or_cct(bound.as_str()), bound);
            assert!(bound.ceiling() > 0.0);
        }
    }

    #[test]
    fn the_mood_cap_falls_but_never_to_zero() {
        let ordinary = SkinCorrection::cap_for_mood(0.0);
        let intentional = SkinCorrection::cap_for_mood(1.0);
        assert!((ordinary - SKIN_CHROMA_CAP).abs() < 1e-6);
        assert!(intentional < ordinary);
        assert!(
            intentional > 0.0,
            "a magenta cast on a candle-lit face is still a magenta cast"
        );
    }

    #[test]
    fn magnitude_is_the_worst_axis_not_the_mean() {
        let delta = NormalisationDelta {
            image_id: ImageId::new(),
            node_id: NodeId::new(),
            d_exposure: 0.0,
            d_cct: MAX_D_CCT_K,
            d_tint: 0.0,
            d_contrast: 0.0,
            d_saturation: 0.0,
            skin_correction: None,
            from_exposure_ev: 0.0,
            from_cct_k: 5000.0,
            from_tint: 0.0,
            damping: DEFAULT_DAMPING,
            bounded_by: Some(Bound::Cct),
            reasons: Vec::new(),
            confidence: 0.5,
            user_edited: false,
            analysis_ver: 1,
            policy_ver: 1,
        };
        assert!((delta.magnitude() - 1.0).abs() < 1e-6);
        assert!(delta.within_bounds());
        assert!(!delta.is_zero());
    }

    #[test]
    fn an_outlier_describes_itself_in_the_words_section_6_4_asks_for() {
        let outlier = Outlier {
            image_id: ImageId::new(),
            node_id: NodeId::new(),
            residual_cct: 310.0,
            residual_tint: 0.0,
            residual_exposure: 0.0,
            residual_skin_de00: 4.2,
            worst_identity: None,
            deviation: 0.8,
            reasons: Vec::new(),
            analysis_ver: 1,
        };
        let text = outlier.describe();
        assert!(text.contains("+310 K warmer"), "{text}");
        assert!(text.contains("4.2 dE00"), "{text}");
    }

    #[test]
    fn an_empty_override_is_empty_and_a_wild_one_is_out_of_bounds() {
        assert!(GalleryOverride::default().is_empty());
        let wild = GalleryOverride {
            d_cct: Some(MAX_D_CCT_K * 2.0),
            ..GalleryOverride::default()
        };
        assert!(!wild.is_empty());
        assert!(!wild.within_bounds());
    }

    #[test]
    fn a_reason_set_round_trips_through_one_integer() {
        let codes = [
            GalleryCode::WarmthNormalised,
            GalleryCode::BoundedByPolicy,
            GalleryCode::SkinNormalised,
        ];
        let bits = GalleryCode::to_bits(&codes);
        let back = GalleryCode::from_bits(bits);
        assert_eq!(back.len(), 3);
        for code in codes {
            assert!(back.contains(&code));
        }
        let reasons = GalleryReason::from_bits(bits);
        assert_eq!(reasons.len(), 3);
        assert_eq!(reasons[0].code, GalleryCode::BoundedByPolicy);
        assert_eq!(GalleryReason::to_bits(&reasons), bits);
    }

    #[test]
    fn twenty_six_codes_fit_in_one_u32() {
        assert!(GalleryCode::COUNT <= 32);
        let all = GalleryCode::to_bits(&GalleryCode::ALL);
        assert_eq!(GalleryCode::from_bits(all).len(), GalleryCode::COUNT);
    }

    #[test]
    fn every_code_carries_a_weight_and_refusals_outrank_actions() {
        for code in GalleryCode::ALL {
            let w = code.default_weight();
            assert!((0.0..=1.0).contains(&w), "{code} weight {w}");
        }
        assert!(
            GalleryCode::MoodPreserved.default_weight()
                > GalleryCode::WarmthNormalised.default_weight()
        );
    }

    #[test]
    fn spread_reduction_is_a_share_and_a_zero_baseline_is_not_a_success() {
        let outline = GalleryOutline {
            spread_before_cct: 500.0,
            spread_after_cct: 150.0,
            spread_before_ev: 0.0,
            spread_after_ev: 0.0,
            ..GalleryOutline::default()
        };
        assert!((outline.cct_spread_reduction() - 0.7).abs() < 1e-5);
        assert_eq!(
            outline.ev_spread_reduction(),
            0.0,
            "no measured spread is not a reduction"
        );
    }
}
