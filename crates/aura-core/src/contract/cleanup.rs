//! FROZEN CONTRACT. Removing the exit sign, the gaffer tape and the caterer's crate - and
//! refusing, loudly and on the record, everything else.
//!
//! PHASE-24 section 5 freezes [`CleanupProposal`] and [`SafetyVerdict`] before any detector,
//! any fill and any inpaint exists. The file is in `aura-core` for the reason
//! [`crate::contract::restore`], [`crate::contract::micro`] and [`crate::contract::geometry`]
//! are: the phases that consume a cleanup decision are 27 (QC, which has to be able to say why
//! a background looks smeared) and 28 (autopilot, which must know what ran unattended), and
//! neither of them needs the detector, the patch search or the self-check's internals.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **This is the first phase in the product that removes something the camera got right.**
//!
//! Phase 22 removed noise, which is not information. Phase 23 removed framing, and called itself
//! the most dangerous phase in the product because a cropped frame does not look cropped. This
//! removes an *object that was there* and replaces it with pixels that were not, and when it is
//! wrong the result is not a photograph edited differently from the one somebody wanted - it is a
//! photograph containing something that never existed, delivered to a couple who will keep it for
//! fifty years.
//!
//! Refusing is nearly free. A distraction left in place costs a photographer two minutes of manual
//! work they were not going to spend. A wrongly removed heirloom costs them the client. That
//! asymmetry is why almost everything in this file resolves toward doing nothing, and why the
//! shapes below make "did not remove" a *row* rather than an absence.
//!
//! ## The second thing: the safety filter runs before the score, and the score cannot see it
//!
//! [`SafetyVerdict`] is not a term in a ranking. It is a gate in front of one. A
//! [`CleanupProposal`] cannot be constructed without a verdict, a verdict that is not `allowed`
//! cannot become a proposal, and the removability score has **no field for faces, hands, dresses,
//! rings or cake** - so there is no weight anybody could tune to trade a bride's hands against a
//! cleaner background.
//!
//! This is phase 23's rule about crop safety and phase 12's about guarantees outranking
//! preferences, restated in the phase where the cost of getting it wrong is highest.
//! `docs/adr/ADR-0049-generative-cleanup-and-the-safety-engine.md` section 2 has the argument.
//!
//! ## The third thing: real pixels before invented ones, and the order is in the type
//!
//! [`CleanupMethod`] is ordered - borrow, then fill, then inpaint - and
//! [`CleanupMethod::preference`] is what a source selector sorts by. There is no configuration
//! that reorders them, because a studio that could would eventually reorder for speed, and
//! diffusion is faster than searching a moment for an aligned sibling.
//!
//! ## What this contract cannot express
//!
//! There is no text field anywhere in this file. Not on the proposal, not on the method, not on
//! the override. `docs/generative-policy.md` promises that AURA never generates from a
//! description, and the way that promise is kept is that no type here could carry one.
//!
//! There is also no field that raises a bound. [`AREA_CAP_DEFAULT`], [`DENYLIST_OVERLAP_MAX`] and
//! [`ZERO_TOUCH_CONFIDENCE`] are owned by this file; `cleanup_policy.toml` may only make them
//! stricter. That is what makes the policy document a promise about the product rather than a
//! description of its defaults.

use serde::{Deserialize, Serialize};

use crate::contract::error::AuraResult;
use crate::contract::ids::{ProjectId, ProposalId};
use crate::contract::integrity::CropRect;
use crate::contract::ledger::Autonomy;
use crate::contract::scene::SceneId;

/// One photograph, spelled the way section 5 spells it. Phase 07's alias onto [`PhotoId`],
/// re-exported here for the reason [`crate::contract::restore`] re-exports it.
pub use crate::contract::scene::ImageId;

/// Section 5's `Box2`, aliased onto phase 09's [`CropRect`] for the reason
/// [`crate::contract::composition::Box2`] is: one normalised rectangle in the product, not four.
pub type Box2 = CropRect;

// ---------------------------------------------------------------------------------------------
// Bounds this file owns.
// ---------------------------------------------------------------------------------------------

/// The largest share of a frame an *automated* removal may cover. Section 6.2.
///
/// A studio may lower this and may not raise it. A region above the cap is not refused outright -
/// it is refused *automation*, and becomes a manual action somebody takes while looking at the
/// photograph, which is section 2.1's distinction and not the same thing as a ban.
pub const AREA_CAP_DEFAULT: f32 = 0.04;

/// The most a candidate may overlap a denylisted region before it is blocked, as a share of the
/// candidate's own area. Section 6.2.
///
/// One per cent rather than zero because a mask boundary is a matte with a soft edge, and a
/// rectangle that touches one alpha-0.02 pixel of somebody's sleeve has not overlapped her.
pub const DENYLIST_OVERLAP_MAX: f32 = 0.01;

/// The calibrated confidence a tier-1 removal needs before Zero-Touch may apply it unattended.
/// Section 6.4.
pub const ZERO_TOUCH_CONFIDENCE: f32 = 0.97;

/// The most proposals one photograph may carry. Section 6.1: cleanup stays a light touch.
///
/// A frame with fifteen distractions is a frame whose background is the problem, and removing
/// three of them makes it look edited rather than better.
pub const MAX_PROPOSALS_PER_IMAGE: usize = 3;

/// The lowest removability confidence that may reach the cloud editorial judgement, and the
/// highest. Section 7: outside this band the mechanical answer stands.
pub const JUDGEMENT_BAND: (f32, f32) = (0.60, 0.90);

// ---------------------------------------------------------------------------------------------
// What was found.
// ---------------------------------------------------------------------------------------------

/// What a candidate is, from a closed vocabulary.
///
/// Frozen now rather than when a detector exists, because phase 13's ledger, phase 27's QC and
/// the delivery report all name a class, and a vocabulary that arrives with a model is a
/// vocabulary three phases have already stored strings from.
///
/// **[`DistractionClass::Unclassified`] is not a null.** It is what makes the cautious path
/// correct: a candidate whose class is unknown cannot be shown to be story-irrelevant, so
/// [`DistractionClass::story_safe`] is false for it, so it never reaches unattended application.
/// In this build the measurement returns it for everything, which is why this build proposes
/// nothing unattended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistractionClass {
    /// A fire exit or similar building sign. Never one that names the couple.
    ExitSign,
    /// A bin, a crate, a catering tub.
    Bin,
    /// Cabling, an extension lead, a taped-down run.
    Cable,
    /// Gaffer tape on a floor or a wall.
    GafferTape,
    /// A water bottle, a discarded glass, a can.
    Bottle,
    /// A stacked or stray chair, usually at the frame edge.
    Chair,
    /// A lit phone or tablet screen in a dark room.
    PhoneScreen,
    /// A hand or forearm entering the frame from outside, belonging to nobody in it.
    StrayHand,
    /// A person in the background who is not a guest of this wedding.
    ///
    /// Removable only under section 6.2's extra conditions - fully separated from every primary
    /// subject, small, and near the frame edge - and never in bulk. Removing a person is a human
    /// decision about a human being.
    BackgroundPerson,
    /// Something the measurement found and cannot name.
    ///
    /// The default answer of every detector in this build.
    Unclassified,
}

impl DistractionClass {
    /// Every variant, in a fixed order, so a panel legend and a stored histogram agree.
    pub const ALL: [Self; 10] = [
        Self::ExitSign,
        Self::Bin,
        Self::Cable,
        Self::GafferTape,
        Self::Bottle,
        Self::Chair,
        Self::PhoneScreen,
        Self::StrayHand,
        Self::BackgroundPerson,
        Self::Unclassified,
    ];

    /// The stored slug. Stable: it is a column value and a wire field.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExitSign => "exit_sign",
            Self::Bin => "bin",
            Self::Cable => "cable",
            Self::GafferTape => "gaffer_tape",
            Self::Bottle => "bottle",
            Self::Chair => "chair",
            Self::PhoneScreen => "phone_screen",
            Self::StrayHand => "stray_hand",
            Self::BackgroundPerson => "background_person",
            Self::Unclassified => "unclassified",
        }
    }

    /// The words a photographer reads.
    ///
    /// One sentence per class, in the product's own voice, and every one of them says what the
    /// thing *is* rather than what AURA will do about it: a panel that read "removable clutter"
    /// would be telling a photographer the answer before showing them the question.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::ExitSign => "a building sign, like a fire exit",
            Self::Bin => "a bin, a crate or a catering tub",
            Self::Cable => "cabling or a taped-down run",
            Self::GafferTape => "tape on the floor",
            Self::Bottle => "a bottle or a cup",
            Self::Chair => "a chair or a stand at the frame edge",
            Self::PhoneScreen => "a lit phone screen",
            Self::StrayHand => "a stray hand or arm entering the frame",
            Self::BackgroundPerson => {
                "somebody in the background. AURA never removes a person on its own"
            }
            Self::Unclassified => {
                "something that draws the eye and does not belong to anything AURA recognises"
            }
        }
    }

    /// Read a stored slug back. `None` rather than a default, so a row written by a newer build
    /// is a refusal rather than a silent `Unclassified`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == text)
    }

    /// True when removing this class can be shown not to remove part of the wedding's story.
    ///
    /// False for [`Self::Unclassified`] - an unknown object cannot be shown to be irrelevant -
    /// and false for [`Self::BackgroundPerson`], which is a person and therefore never automatic.
    #[must_use]
    pub const fn story_safe(self) -> bool {
        matches!(
            self,
            Self::ExitSign
                | Self::Bin
                | Self::Cable
                | Self::GafferTape
                | Self::Bottle
                | Self::Chair
                | Self::PhoneScreen
                | Self::StrayHand
        )
    }
}

// ---------------------------------------------------------------------------------------------
// The safety engine's answer.
// ---------------------------------------------------------------------------------------------

/// One of the five checks section 6.2 requires, in the order they run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyCheck {
    /// The region is at most [`AREA_CAP_DEFAULT`] of the frame, or whatever a studio lowered it to.
    SizeCap,
    /// The region does not overlap a face, skin, hands, a dress, rings or cake by more than
    /// [`DENYLIST_OVERLAP_MAX`].
    ///
    /// **An absent mask fails this check rather than passing it.** Phase 18's segmenter is a
    /// placeholder and no generator is wired into this pass, so on a real photograph there is
    /// nothing to intersect - and "nothing to intersect" is not "no overlap". ADR-0049 section 3.
    Denylist,
    /// The region is clear of every primary identity's body, not merely of their face.
    IdentityProtect,
    /// The region does not cross a long straight architectural line or a repeating pattern
    /// boundary, both of which inpainting warps predictably.
    StructureSpan,
    /// The removability confidence is high enough for the method being proposed.
    Confidence,
}

impl SafetyCheck {
    /// Every check, in the order [`SafetyVerdict`] records them.
    pub const ALL: [Self; 5] = [
        Self::SizeCap,
        Self::Denylist,
        Self::IdentityProtect,
        Self::StructureSpan,
        Self::Confidence,
    ];

    /// How many there are, for a fixed-width stored histogram.
    pub const COUNT: usize = Self::ALL.len();

    /// The stored slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SizeCap => "size_cap",
            Self::Denylist => "denylist",
            Self::IdentityProtect => "identity_protect",
            Self::StructureSpan => "structure_span",
            Self::Confidence => "confidence",
        }
    }

    /// Read a stored slug back.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == text)
    }
}

/// What the safety engine checked and what it found. Section 5.
///
/// Every check is recorded whether it passed or failed, so a stored verdict says what was
/// *examined* rather than only what went wrong. That distinction is the whole audit: a build in
/// which the denylist never ran and a build in which it ran and found nothing produce identical
/// removals and completely different rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafetyVerdict {
    /// True only when every check in [`Self::checks`] passed.
    pub allowed: bool,
    /// One entry per [`SafetyCheck`], in [`SafetyCheck::ALL`] order.
    pub checks: Vec<(SafetyCheck, bool)>,
    /// The first check that failed, named for a person rather than for a log.
    ///
    /// A code rather than a sentence would be better and is not what section 5 froze; the reason
    /// codes in [`CleanupCode`] carry the machine-readable half.
    pub blocked_reason: Option<String>,
}

impl SafetyVerdict {
    /// A verdict in which every check passed.
    #[must_use]
    pub fn allow() -> Self {
        Self {
            allowed: true,
            checks: SafetyCheck::ALL.into_iter().map(|c| (c, true)).collect(),

            blocked_reason: None,
        }
    }

    /// A verdict blocked by one named check, with every other check recorded as it was found.
    #[must_use]
    pub fn block(failed: SafetyCheck, reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            checks: SafetyCheck::ALL
                .into_iter()
                .map(|c| (c, c != failed))
                .collect(),

            blocked_reason: Some(reason.into()),
        }
    }

    /// Which check stopped this candidate, when one did.
    #[must_use]
    pub fn failed_check(&self) -> Option<SafetyCheck> {
        self.checks
            .iter()
            .find(|(_, passed)| !passed)
            .map(|(check, _)| *check)
    }

    /// True when the verdict is self-consistent: `allowed` agrees with the checks, and every
    /// check is present exactly once.
    ///
    /// A store reads this before it writes, because a verdict that says `allowed` while carrying
    /// a failed check is the one row that would make the audit meaningless.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        if self.checks.len() != SafetyCheck::COUNT {
            return false;
        }
        for check in SafetyCheck::ALL {
            if !self.checks.iter().any(|(c, _)| *c == check) {
                return false;
            }
        }
        let all_passed = self.checks.iter().all(|(_, passed)| *passed);
        self.allowed == all_passed && self.allowed == self.blocked_reason.is_none()
    }
}

// ---------------------------------------------------------------------------------------------
// How the pixels are replaced.
// ---------------------------------------------------------------------------------------------

/// Where the replacement pixels come from. Section 5.
///
/// The variants are ordered by how much of the room they are a record of, and
/// [`Self::preference`] is what a source selector sorts by. Nothing configurable reorders them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupMethod {
    /// Real pixels, homography-aligned from another frame of the same moment.
    ///
    /// Always preferred, always disclosed, and it carries its source in the type so a stored row
    /// cannot say a borrow happened without saying where from - phase 21's rule for its glare
    /// borrow, inherited.
    BorrowFrom(ImageId),
    /// Texture already in this photograph, copied into the region.
    ///
    /// Preferred to [`Self::Inpaint`] because it **cannot invent structure**. Its failure mode is
    /// a visible seam or a repeated tuft of grass: ugly, findable, and not a fabrication.
    ClassicalFill,
    /// Pixels a diffusion model made up.
    ///
    /// The last resort, bounded by every check above, and disclosed differently from the other two
    /// because it is different: those move real pixels and this makes new ones.
    ///
    /// **`solve` returns an error on every call in this build.** There is no diffusion model in
    /// `models.lock` and no fallback, because the thing that would stand in for it is the
    /// classical fill, which was already tried first. ADR-0049 section 5.
    Inpaint {
        /// Which model, so a disclosure names it.
        model: String,
    },
}

impl CleanupMethod {
    /// Sort key: lower is preferred. Borrow 0, fill 1, inpaint 2.
    #[must_use]
    pub const fn preference(&self) -> u8 {
        match self {
            Self::BorrowFrom(_) => 0,
            Self::ClassicalFill => 1,
            Self::Inpaint { .. } => 2,
        }
    }

    /// True when this method moved pixels that a camera recorded.
    #[must_use]
    pub const fn is_real_pixels(&self) -> bool {
        matches!(self, Self::BorrowFrom(_) | Self::ClassicalFill)
    }

    /// True when this method may apply unattended in Zero-Touch, before confidence is consulted.
    ///
    /// Section 6.4: tier-1 only. [`Self::Inpaint`] is false here and a studio opt-in is a
    /// separate switch that this contract does not carry, because a field that could turn it on
    /// is a field somebody defaults to true.
    #[must_use]
    pub const fn tier_one(&self) -> bool {
        self.is_real_pixels()
    }

    /// The stored slug, without the payload.
    #[must_use]
    pub const fn kind_str(&self) -> &'static str {
        match self {
            Self::BorrowFrom(_) => "borrow",
            Self::ClassicalFill => "fill",
            Self::Inpaint { .. } => "inpaint",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Reasons.
// ---------------------------------------------------------------------------------------------

/// Why a proposal exists, or why one does not. A closed set, in the shape phases 09 to 23 use.
///
/// **More than half of these are refusals**, which is the highest proportion in the product and is
/// the phase working rather than failing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupCode {
    // Found something.
    /// A region drew attention and belongs to nothing in the story.
    UnexplainedSalience,
    /// The region sits in the background plane, away from every subject.
    BackgroundPlane,
    /// The region is near a frame edge, where a distraction costs least to remove.
    NearFrameEdge,
    /// A sibling frame of the same moment shows this background without the object.
    SiblingAvailable,
    /// The surrounding texture is uniform enough to fill from.
    TextureUniform,

    // Refused by the safety engine.
    /// The region is larger than the automated cap.
    TooLarge,
    /// The region overlaps a face, skin, hands, a dress, rings or cake.
    OverlapsProtected,
    /// The masks that would prove it does not overlap are absent.
    ///
    /// Distinct from [`Self::OverlapsProtected`] on purpose: one says the product checked and
    /// found a person, the other says it could not check. Only the first is a claim.
    ProtectionUnknown,
    /// The region touches a primary identity's body.
    OverlapsIdentity,
    /// The region crosses a long straight line or a repeating pattern.
    StructureSpanned,
    /// The removability confidence is too low for any method.
    ConfidenceLow,
    /// The class is unknown, so story relevance cannot be established.
    ClassUnknown,
    /// The object is part of what the wedding was about.
    StoryRelevant,
    /// The object is a person, so removal is manual and confirmed rather than proposed.
    PersonPresent,
    /// This photograph already carries the maximum number of proposals.
    ProposalCapReached,

    // Source selection.
    /// No sibling frame could be aligned well enough to borrow from.
    NoAlignedSibling,
    /// The surrounding texture is too structured to fill from.
    TextureStructured,
    /// A diffusion model would have been needed and none is installed.
    InpaintUnavailable,

    // The self-check.
    /// The result repeated a texture at a period that occurs nowhere else in the frame.
    ArtefactRepeatedTexture,
    /// A straight line changed direction inside the patched region.
    ArtefactWarpedLine,
    /// A gradient terminated at the patch boundary.
    ArtefactGhostEdge,
    /// The self-check failed and the removal was undone before anybody saw it.
    RevertedOnSelfCheck,

    // Autonomy and disclosure.
    /// Held for review because the method is not tier one.
    ReviewRequiredMethod,
    /// Held for review because the calibrated confidence is below the unattended threshold.
    ReviewRequiredConfidence,
    /// Applied unattended under Zero-Touch.
    AppliedUnattended,
    /// A person accepted this proposal.
    AcceptedByUser,
    /// A person rejected this proposal.
    RejectedByUser,
    /// A person removed this region themselves, outside the automated path.
    ManualRemoval,

    // Housekeeping.
    /// The cloud editorial judgement declined the removal.
    JudgementDeclined,
    /// The cloud editorial judgement was not reachable, so the mechanical answer stood.
    JudgementUnavailable,
    /// Stored proposals came from different detectors, policies or arithmetic.
    VersionDrift,
}

impl CleanupCode {
    /// Every code, in a fixed order.
    pub const ALL: [Self; 31] = [
        Self::UnexplainedSalience,
        Self::BackgroundPlane,
        Self::NearFrameEdge,
        Self::SiblingAvailable,
        Self::TextureUniform,
        Self::TooLarge,
        Self::OverlapsProtected,
        Self::ProtectionUnknown,
        Self::OverlapsIdentity,
        Self::StructureSpanned,
        Self::ConfidenceLow,
        Self::ClassUnknown,
        Self::StoryRelevant,
        Self::PersonPresent,
        Self::ProposalCapReached,
        Self::NoAlignedSibling,
        Self::TextureStructured,
        Self::InpaintUnavailable,
        Self::ArtefactRepeatedTexture,
        Self::ArtefactWarpedLine,
        Self::ArtefactGhostEdge,
        Self::RevertedOnSelfCheck,
        Self::ReviewRequiredMethod,
        Self::ReviewRequiredConfidence,
        Self::AppliedUnattended,
        Self::AcceptedByUser,
        Self::RejectedByUser,
        Self::ManualRemoval,
        Self::JudgementDeclined,
        Self::JudgementUnavailable,
        Self::VersionDrift,
    ];

    /// How many of them are refusals - a code that says the product declined to act.
    ///
    /// Asserted by a test rather than counted by hand, because the ratio is the phase's own
    /// argument about itself and a drift in it is a drift in what the product does.
    pub const REFUSAL_COUNT: usize = 16;

    /// The stored slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnexplainedSalience => "unexplained_salience",
            Self::BackgroundPlane => "background_plane",
            Self::NearFrameEdge => "near_frame_edge",
            Self::SiblingAvailable => "sibling_available",
            Self::TextureUniform => "texture_uniform",
            Self::TooLarge => "too_large",
            Self::OverlapsProtected => "overlaps_protected",
            Self::ProtectionUnknown => "protection_unknown",
            Self::OverlapsIdentity => "overlaps_identity",
            Self::StructureSpanned => "structure_spanned",
            Self::ConfidenceLow => "confidence_low",
            Self::ClassUnknown => "class_unknown",
            Self::StoryRelevant => "story_relevant",
            Self::PersonPresent => "person_present",
            Self::ProposalCapReached => "proposal_cap_reached",
            Self::NoAlignedSibling => "no_aligned_sibling",
            Self::TextureStructured => "texture_structured",
            Self::InpaintUnavailable => "inpaint_unavailable",
            Self::ArtefactRepeatedTexture => "artefact_repeated_texture",
            Self::ArtefactWarpedLine => "artefact_warped_line",
            Self::ArtefactGhostEdge => "artefact_ghost_edge",
            Self::RevertedOnSelfCheck => "reverted_on_self_check",
            Self::ReviewRequiredMethod => "review_required_method",
            Self::ReviewRequiredConfidence => "review_required_confidence",
            Self::AppliedUnattended => "applied_unattended",
            Self::AcceptedByUser => "accepted_by_user",
            Self::RejectedByUser => "rejected_by_user",
            Self::ManualRemoval => "manual_removal",
            Self::JudgementDeclined => "judgement_declined",
            Self::JudgementUnavailable => "judgement_unavailable",
            Self::VersionDrift => "version_drift",
        }
    }

    /// Read a stored slug back.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == text)
    }

    /// The sentence a photographer reads.
    ///
    /// Thirty-one of them, and **more than half say what AURA declined to do**. That is the phase
    /// rather than an accident, and the wording follows from it: a refusal is written as a
    /// decision the product made on purpose, not as an apology for a missing feature. "AURA could
    /// not tell where people are in this photograph, so it has left it alone" is a different
    /// sentence from "AURA failed to segment this photograph", and only the first is true.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::UnexplainedSalience => {
                "this draws the eye and does not seem to belong to anything in the wedding"
            }
            Self::BackgroundPlane => "it sits well behind everybody in the photograph",
            Self::NearFrameEdge => "it is near the edge of the frame",
            Self::SiblingAvailable => {
                "another frame of the same moment shows this background without it, so the \
                 replacement is real pixels rather than invented ones"
            }
            Self::TextureUniform => {
                "the surroundings are even enough to copy from, so nothing had to be invented"
            }
            Self::TooLarge => {
                "it covers more of the frame than AURA will ever tidy on its own"
            }
            Self::OverlapsProtected => {
                "it overlaps a face, skin, hands, a dress, rings or the cake, so AURA left it alone"
            }
            Self::ProtectionUnknown => {
                "AURA cannot yet tell where people, dresses and rings are in this photograph, so \
                 it will not tidy anything out of it"
            }
            Self::OverlapsIdentity => "it touches somebody this wedding is about",
            Self::StructureSpanned => {
                "it crosses a straight line or a repeating pattern, which tidying would bend"
            }
            Self::ConfidenceLow => "AURA is not sure enough about this to suggest it",
            Self::ClassUnknown => {
                "AURA cannot tell what this is, so it cannot show that it is not part of your \
                 wedding"
            }
            Self::StoryRelevant => "this looks like part of what the wedding was about",
            Self::PersonPresent => {
                "this is a person. Removing somebody is your decision and never AURA's"
            }
            Self::ProposalCapReached => {
                "this photograph already has as many suggestions as AURA will make for one frame"
            }
            Self::NoAlignedSibling => {
                "no other frame of this moment could be lined up well enough to borrow from"
            }
            Self::TextureStructured => {
                "the surroundings are too patterned to copy from without inventing something"
            }
            Self::InpaintUnavailable => {
                "this would need AURA to make up new pixels, which this installation cannot do"
            }
            Self::ArtefactRepeatedTexture => {
                "the result repeated a pattern that appears nowhere else in the photograph"
            }
            Self::ArtefactWarpedLine => "a straight line bent inside the tidied area",
            Self::ArtefactGhostEdge => "an edge appeared where the tidied area meets the rest",
            Self::RevertedOnSelfCheck => {
                "AURA did not like its own result and put the photograph back exactly as it was"
            }
            Self::ReviewRequiredMethod => {
                "this would need invented pixels, so it always waits for you"
            }
            Self::ReviewRequiredConfidence => "this is waiting for you to look at it",
            Self::AppliedUnattended => "AURA was confident enough to do this without asking",
            Self::AcceptedByUser => "you accepted this",
            Self::RejectedByUser => "you turned this down",
            Self::ManualRemoval => "you asked for this one yourself",
            Self::JudgementDeclined => {
                "a second, more cautious review decided this belongs in the photograph"
            }
            Self::JudgementUnavailable => {
                "the second review was not reachable, so AURA kept its own cautious answer"
            }
            Self::VersionDrift => {
                "AURA has improved how it spots distractions and is re-checking this wedding"
            }
        }
    }

    /// True when this code records something the product declined to do.
    #[must_use]
    pub const fn is_refusal(self) -> bool {
        matches!(
            self,
            Self::TooLarge
                | Self::OverlapsProtected
                | Self::ProtectionUnknown
                | Self::OverlapsIdentity
                | Self::StructureSpanned
                | Self::ConfidenceLow
                | Self::ClassUnknown
                | Self::StoryRelevant
                | Self::PersonPresent
                | Self::ProposalCapReached
                | Self::NoAlignedSibling
                | Self::TextureStructured
                | Self::InpaintUnavailable
                | Self::RevertedOnSelfCheck
                | Self::JudgementDeclined
                | Self::RejectedByUser
        )
    }
}

/// One reason, with the pixels behind it where there are any.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CleanupReason {
    /// The code. Stored rather than its sentence - phase 09's rule.
    pub code: CleanupCode,
    /// The sentence a photographer reads.
    pub text: String,
    /// How much this moved the confidence. Negative for a doubt.
    pub weight: f32,
    /// The region this reason is about, when it is about one.
    pub evidence: Option<Box2>,
}

impl CleanupReason {
    /// A reason about the whole frame.
    #[must_use]
    pub fn plain(code: CleanupCode, weight: f32) -> Self {
        Self {
            code,
            text: String::new(),
            weight,
            evidence: None,
        }
    }

    /// A reason about one region.
    #[must_use]
    pub fn at(code: CleanupCode, weight: f32, evidence: Box2) -> Self {
        Self {
            code,
            text: String::new(),
            weight,
            evidence: Some(evidence),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The proposal.
// ---------------------------------------------------------------------------------------------

/// A reference to a rendered before/after pair the panel can show.
///
/// A handle rather than pixels, for the reason phase 13's `Evidence` carries no image bytes: a
/// shape that could hold a buffer is a shape a support bundle eventually holds one in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewRef {
    /// The cache key the preview service can resolve.
    pub key: String,
    /// The render level it was produced at.
    pub level: u16,
}

/// One proposed removal. Section 5.
///
/// **Constructed only through [`CleanupProposal::new`]**, which refuses a verdict that is not
/// `allowed` and refuses one that is not well formed. There is no way to obtain a proposal whose
/// safety was not established, which is what makes section 2's ordering a property of the type
/// rather than a convention in a function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CleanupProposal {
    /// This proposal.
    pub id: ProposalId,
    /// The photograph.
    pub image_id: ImageId,
    /// Where, normalised to the frame.
    pub region: Box2,
    /// What it is, as far as anything can tell.
    pub class: DistractionClass,
    /// The share of the frame the region covers, `0..1`.
    pub area_frac: f32,
    /// How much attention the region draws, `0..1`.
    pub salience: f32,
    /// Where the replacement pixels would come from.
    pub method: CleanupMethod,
    /// What the safety engine checked. Always `allowed` on a constructed proposal.
    pub safety: SafetyVerdict,
    /// How sure the whole proposal is, `0..1`.
    pub confidence: f32,
    /// Why, worst first.
    pub reasons: Vec<CleanupReason>,
    /// A before/after the panel can render, when one has been produced.
    pub preview: Option<PreviewRef>,
    /// What may happen without asking. Phase 13's bands, raised one for this phase.
    pub autonomy: Autonomy,
    /// Which scene the thresholds were conditioned on. Invariant 7.
    pub scene: SceneId,
    /// Which detector produced the candidate.
    pub detector_ver: u16,
    /// Which safety arithmetic judged it.
    pub analysis_ver: u16,
    /// Which `cleanup_policy.toml` the caps and denylist came from.
    pub policy_ver: u16,
}

impl CleanupProposal {
    /// The only constructor.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5116` when the verdict is not `allowed`, when it is not well formed, when the
    /// region is degenerate or leaves the frame, or when the reasons are empty. Invariant 2: a
    /// decision without an explanation is a bug, and here it would be a removal nobody can
    /// account for.
    pub fn new(
        id: ProposalId,
        image_id: ImageId,
        region: Box2,
        class: DistractionClass,
        method: CleanupMethod,
        safety: SafetyVerdict,
        reasons: Vec<CleanupReason>,
    ) -> AuraResult<Self> {
        if !safety.is_well_formed() {
            return Err(crate::errors::ml::cleanup_proposal_refused(
                "the safety verdict is not self-consistent",
            ));
        }
        if !safety.allowed {
            return Err(crate::errors::ml::cleanup_proposal_refused(
                "a blocked candidate cannot become a proposal",
            ));
        }
        if reasons.is_empty() {
            return Err(crate::errors::ml::cleanup_proposal_refused(
                "a proposal with no reasons cannot be shown to anybody",
            ));
        }
        if !region_is_sane(&region) {
            return Err(crate::errors::ml::cleanup_proposal_refused(
                "the region is degenerate or leaves the frame",
            ));
        }
        let area_frac = region.w * region.h;
        Ok(Self {
            id,
            image_id,
            region,
            class,
            area_frac,
            salience: 0.0,
            method,
            safety,
            confidence: 0.0,
            reasons,
            preview: None,
            autonomy: Autonomy::RequireReview,
            scene: SceneId::Unknown,
            detector_ver: 0,
            analysis_ver: 0,
            policy_ver: 0,
        })
    }

    /// True when this proposal may be applied without anybody looking.
    ///
    /// Three conditions, and every one of them has to hold: the band says so, the method is tier
    /// one, and the calibrated confidence clears [`ZERO_TOUCH_CONFIDENCE`]. Phase 13's
    /// `uncalibrated_raises` moves every band one toward review while nothing is calibrated, so
    /// this is false everywhere in this build.
    #[must_use]
    pub fn may_apply_unattended(&self) -> bool {
        matches!(self.autonomy, Autonomy::Auto | Autonomy::AutoZeroTouch)
            && self.method.tier_one()
            && self.confidence >= ZERO_TOUCH_CONFIDENCE
            && self.class.story_safe()
    }

    /// What is wrong with this proposal, if anything, as a sentence.
    ///
    /// A store calls it before it writes. `None` means the row is sound.
    #[must_use]
    pub fn broken_guarantee(&self) -> Option<String> {
        if !self.safety.is_well_formed() || !self.safety.allowed {
            return Some("the safety verdict does not permit this proposal".into());
        }
        if self.reasons.is_empty() {
            return Some("a proposal must carry at least one reason".into());
        }
        if !region_is_sane(&self.region) {
            return Some("the region is degenerate or leaves the frame".into());
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Some(format!(
                "the confidence {:.3} is outside 0..1",
                self.confidence
            ));
        }
        if self.class == DistractionClass::BackgroundPerson && self.may_apply_unattended() {
            return Some("a person is never removed unattended".into());
        }
        None
    }
}

/// True when a normalised rectangle is inside the frame and has area.
fn region_is_sane(region: &Box2) -> bool {
    region.w > 0.0
        && region.h > 0.0
        && region.x >= 0.0
        && region.y >= 0.0
        && region.x + region.w <= 1.0 + f32::EPSILON
        && region.y + region.h <= 1.0 + f32::EPSILON
}

// ---------------------------------------------------------------------------------------------
// What was done, and what a project looks like.
// ---------------------------------------------------------------------------------------------

/// A removal that happened, written in the same statement as its disclosure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CleanupDisclosure {
    /// Which proposal.
    pub proposal_id: ProposalId,
    /// The photograph.
    pub image_id: ImageId,
    /// How the pixels were replaced.
    pub method: CleanupMethod,
    /// Where.
    pub region: Box2,
    /// True when a person accepted it rather than a mode applying it.
    pub accepted_by_user: bool,
    /// What the self-check measured on the result, `0..1`, lower is cleaner.
    pub artefact_score: f32,
}

/// What a photographer's own decision changes.
///
/// There is no strength field, no size field and no text field. The only things a person can say
/// are yes, no, and "I will do this one myself".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CleanupOverride {
    /// Which proposal.
    pub proposal_id: Option<ProposalId>,
    /// Accept it, reject it, or leave it alone.
    pub accept: Option<bool>,
    /// Turn cleanup off for this photograph entirely.
    pub disable_for_image: Option<bool>,
}

/// How much of a project has been looked at, and what came of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CleanupOutline {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs the pass has examined.
    pub examined: u32,
    /// Fraction examined; the denominator is every photograph.
    pub coverage: f32,
    /// Photographs carrying at least one proposal.
    pub with_proposals: u32,
    /// Proposals that were applied.
    pub applied: u32,
    /// Candidates the safety engine blocked, by check, in [`SafetyCheck::ALL`] order.
    pub blocked: [u32; SafetyCheck::COUNT],
    /// Applied removals that borrowed real pixels.
    pub borrowed: u32,
    /// Applied removals that were filled from this photograph's own texture.
    pub filled: u32,
    /// Applied removals that a diffusion model produced. Zero in this build.
    pub inpainted: u32,
    /// Removals the self-check reverted before anybody saw them.
    pub reverted: u32,
    /// Fraction of examined frames whose denylist masks arrived.
    ///
    /// **The number to read first.** At zero, every candidate was blocked by
    /// [`SafetyCheck::Denylist`] for want of evidence rather than for want of safety, and the
    /// project's blocked histogram says nothing about what is in the photographs.
    pub mask_covered: f32,
}

/// The one way to ask what was removed from a photograph.
///
/// Twentieth service of its kind. Phase 27 has to be able to say why a background looks smeared,
/// phase 28 must know what ran unattended, and the delivery report lists every disclosure. No
/// phase may keep its own distraction detector, its own denylist or its own idea of what a safe
/// removal is.
pub trait CleanupService: Send + Sync {
    /// Every proposal on one photograph, applied or not.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the store cannot be read.
    fn proposals(&self, image: ImageId) -> AuraResult<Vec<CleanupProposal>>;

    /// Every candidate the safety engine blocked on one photograph, with the check that blocked
    /// it.
    ///
    /// A separate call rather than a field, because the blocked set is usually larger than the
    /// proposed set and a panel that renders three proposals should not carry forty refusals it
    /// will not draw.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the store cannot be read.
    fn blocked(&self, image: ImageId) -> AuraResult<Vec<(Box2, SafetyCheck, CleanupCode)>>;

    /// Everything removed from one project, for the delivery report.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the store cannot be read.
    fn disclosures(&self, project: ProjectId) -> AuraResult<Vec<CleanupDisclosure>>;

    /// Record a photographer's decision.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5117` when the override names no proposal or asks for nothing.
    fn decide(&self, image: ImageId, choice: &CleanupOverride) -> AuraResult<()>;

    /// How much of a project has been examined.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the store cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<CleanupOutline>;
}
