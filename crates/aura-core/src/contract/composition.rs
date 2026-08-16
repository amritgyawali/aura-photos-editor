//! FROZEN CONTRACT. How a photograph is framed: where the horizon is, what the frame
//! cut, where the subject sits in it, what is behind them, and how much of that a
//! picture editor would mind.
//!
//! PHASE-11 section 5 freezes these shapes before any analyser exists. The file is in
//! `aura-core` rather than in `aura-brain-photo` for the reason
//! [`crate::contract::integrity`], [`crate::contract::emotion`] and
//! [`crate::contract::moment`] are: the phases that consume a framing judgement are 12
//! (culling), 13 (the explainability ledger), 23 (the geometry suite, which is the one
//! that actually crops) and 29 (curation), and none of them needs the Hough accumulator,
//! the keypoint decoder or the saliency plane.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **This phase describes a frame; it never re-frames one.** Section 2.2 puts the
//! cropping and the straightening in phase 23 and the distraction removal in phase 24.
//! Nothing in this contract can move a pixel, and [`CropHint`] is spelled *hint* in the
//! type, in the field, in the column and on the wire for the same reason
//! `DuplicateSet::keep_hint` is.
//!
//! It is also the phase where that restraint is hardest to keep, because a composition
//! score *feels* like a verdict about taste rather than a measurement about geometry.
//! Section 6.3 caps its influence for exactly that reason and section 12's first failure
//! mode - "rigid rules punish creative work" - is what a product that forgot the cap
//! ships.
//!
//! ## Eight spellings differ from the phase document
//!
//! All eight are recorded in `docs/adr/ADR-0023-composition-rules-and-aesthetics.md`
//! section 2.
//!
//! * **`ImageId` is `PhotoId`**, aliased rather than duplicated, exactly as the scene,
//!   people, moment, integrity and emotion contracts already do it.
//! * **`Box2` is [`CropRect`]**, aliased for the same reason. Section 5 spells the
//!   evidence rectangles `Box2`; phase 09 already froze a normalised rectangle with a
//!   `clamped` and an `expanded` on it, and a second rectangle type is two conventions
//!   about whether `w` is a width or a right edge.
//! * **`JointCut::box` is [`JointCut::area`]**, because `box` is a reserved word in
//!   Rust.
//! * **[`CompositionFlags`] is not in section 5 at all.** Section 2.1 asks for
//!   "structured `composition_flags`" and section 13 for "violation flags with evidence
//!   boxes"; this is the same hand-rolled `u32` newtype phase 09's `IntegrityFlags` is,
//!   with sixteen bits named.
//! * **[`CompositionReason`] carries a typed [`CompositionCode`]** rather than a free
//!   string, so that section 9's DOC deliverable - "document composition reason codes
//!   with example images" - is a finishable job. `docs/composition-and-framing.md` is
//!   generated against [`CompositionCode::ALL`].
//! * **A reason may be an exoneration.** Twelve of the twenty-six codes carry no
//!   penalty, and they are the ones this phase most needs: a dutch angle on a dance
//!   floor, a deliberately tight crop in a couple portrait and a centred flat-lay are
//!   all *correct* framing that a rule engine reads as three violations.
//! * **[`CompositionOutline`] is not in section 5 either.** It is the coverage carrier,
//!   inherited from phase 05 for the seventh time, with this phase's refinement written
//!   at [`CompositionOutline::keypoint_aware`].
//! * **[`CompositionService`] is not in section 5 either.** Four later phases consume a
//!   framing judgement and a contract with no entry point makes each of them find its
//!   own way in.
//!
//! ## Three version fields, because they invalidate three different things
//!
//! [`CompositionResult::model_ver`] invalidates the keypoints and the learned aesthetic,
//! because both come from a head. [`CompositionResult::analysis_ver`] invalidates the
//! tilt, the crop audit, the placement, the background measures and the composite,
//! because those come from this build's arithmetic. [`CompositionResult::rules_ver`]
//! invalidates every number that was compared against a **band** - the headroom verdict,
//! the tilt tolerance, the clutter threshold - because a scene's bands are a product
//! decision that a release can move without changing a line of analysis code.
//!
//! Comparing two rows across any of them returns a plausible number that means nothing.
//! `AURA-ML-5043` exists so that comparison never happens silently. Seventh phase,
//! seventh version-drift code.
//!
//! Changing anything in this file requires an ADR.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{MomentId, ProjectId};
use crate::contract::integrity::CropRect;
use crate::contract::scene::SceneId;

pub use crate::contract::scene::ImageId;

/// A rectangle in a photograph, normalised to the frame.
///
/// Section 5's `Box2`, aliased onto phase 09's [`CropRect`] rather than duplicated; see
/// this module's header. Everything phase 09 gave that type - clamping, context
/// expansion, the emptiness test - is what an evidence box needs, and a second rectangle
/// type would be a second convention about whether the last two numbers are a size or a
/// corner.
pub type Box2 = CropRect;

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

/// Everything that can be true of how one photograph is framed.
///
/// Sixteen bits, in the order `docs/composition-and-framing.md` documents them, with
/// their values fixed - the bit positions are a wire format the `image_composition.flags`
/// column stores and a later build has to be able to read.
///
/// A hand-rolled `u32` rather than `bitflags!`, for the reason
/// [`crate::contract::integrity::IntegrityFlags`] gives. Sixteen bits spare.
///
/// **Three of the sixteen are not defects.** [`CompositionFlags::INTENTIONAL_TILT`],
/// [`CompositionFlags::CENTRED`] and [`CompositionFlags::NO_GEOMETRY`] are this phase's
/// whole argument in three bits: a dutch angle is a decision, a centred flat-lay is
/// correct, and a frame with no body in it has not been judged against a rule about
/// bodies. [`CompositionFlags::DEFECTS`] is the mask that knows the difference, and it
/// exists so that four later phases do not each write the same `!matches!`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(transparent)]
pub struct CompositionFlags(u32);

impl CompositionFlags {
    /// Nothing is true of this frame. The verdict on a cleanly framed photograph.
    pub const EMPTY: Self = Self(0);

    /// The horizon is off level by more than this scene tolerates.
    pub const HORIZON_TILTED: Self = Self(1 << 0);
    /// A large tilt that reads as a decision rather than as a mistake.
    ///
    /// **Not a defect.** Section 6.1: a tilt above six degrees with a centred subject, no
    /// strong horizon and a candid or dance-floor scene is style. Section 12's first
    /// failure mode is a rule engine that punishes creative work, and this is the bit
    /// that stops it.
    pub const INTENTIONAL_TILT: Self = Self(1 << 1);
    /// Vertical lines converge: the camera was tilted up or down at architecture.
    pub const VERTICALS_CONVERGING: Self = Self(1 << 2);
    /// Less space above the subject's head than this scene's band allows.
    pub const HEADROOM_TIGHT: Self = Self(1 << 3);
    /// More space above the subject's head than this scene's band allows.
    ///
    /// A separate bit from the tight one because they are opposite mistakes with opposite
    /// remedies, and a single `HEADROOM_WRONG` would make phase 23's crop hint guess
    /// which way to move.
    pub const HEADROOM_EXCESSIVE: Self = Self(1 << 4);
    /// The top of a head is cut by the frame edge where it should not be.
    pub const HEAD_CROPPED: Self = Self(1 << 5);
    /// The frame cuts a limb *at* a joint - a wrist, an elbow, a knee, an ankle.
    pub const JOINT_CUT: Self = Self(1 << 6);
    /// The frame cuts a limb between joints. The milder of the two.
    pub const LIMB_CUT: Self = Self(1 << 7);
    /// Something - usually a guest - enters the frame at an edge.
    pub const EDGE_INTRUSION: Self = Self(1 << 8);
    /// Visual weight sits heavily on one side with nothing to answer it.
    pub const OFF_BALANCE: Self = Self(1 << 9);
    /// The main subject is at the centre rather than near a power point.
    ///
    /// **Not a defect.** A `details` flat-lay, a ring shot and a symmetrical altar frame
    /// are all centred on purpose, and section 2.1 requires a flat-lay to be judged
    /// differently from a couple portrait. The bit is here so a photographer can filter
    /// *for* it.
    pub const CENTRED: Self = Self(1 << 10);
    /// The background carries more edge energy than this scene tolerates.
    pub const CLUTTERED_BACKGROUND: Self = Self(1 << 11);
    /// A bright blob brighter than the subject pulls the eye.
    pub const BRIGHT_BLOB: Self = Self(1 << 12);
    /// A vertical structure grows out of somebody's head.
    pub const HEAD_MERGE: Self = Self(1 << 13);
    /// A saturated background colour competes with the subject.
    pub const COLOUR_COMPETITION: Self = Self(1 << 14);
    /// No body keypoints and no faces: nothing in this frame is a subject to be framed
    /// badly.
    ///
    /// Not a defect either, and its presence changes how every other bit should be read.
    /// A frame with this set has a horizon, a balance and a background reading, and no
    /// headroom, crop-audit or head-merge reading at all.
    pub const NO_GEOMETRY: Self = Self(1 << 15);

    /// Every flag, in documentation order. The order the histogram is reported in.
    pub const ALL: [Self; 16] = [
        Self::HORIZON_TILTED,
        Self::INTENTIONAL_TILT,
        Self::VERTICALS_CONVERGING,
        Self::HEADROOM_TIGHT,
        Self::HEADROOM_EXCESSIVE,
        Self::HEAD_CROPPED,
        Self::JOINT_CUT,
        Self::LIMB_CUT,
        Self::EDGE_INTRUSION,
        Self::OFF_BALANCE,
        Self::CENTRED,
        Self::CLUTTERED_BACKGROUND,
        Self::BRIGHT_BLOB,
        Self::HEAD_MERGE,
        Self::COLOUR_COMPETITION,
        Self::NO_GEOMETRY,
    ];

    /// How many flags the bitfield names.
    pub const COUNT: usize = 16;

    /// The bits that describe something wrong with the framing.
    ///
    /// Everything except the three that describe something *right* with it, or something
    /// merely true of it: a deliberate tilt, a deliberate centre, and the absence of a
    /// subject to have framed.
    pub const DEFECTS: Self = Self(
        Self::HORIZON_TILTED.0
            | Self::VERTICALS_CONVERGING.0
            | Self::HEADROOM_TIGHT.0
            | Self::HEADROOM_EXCESSIVE.0
            | Self::HEAD_CROPPED.0
            | Self::JOINT_CUT.0
            | Self::LIMB_CUT.0
            | Self::EDGE_INTRUSION.0
            | Self::OFF_BALANCE.0
            | Self::CLUTTERED_BACKGROUND.0
            | Self::BRIGHT_BLOB.0
            | Self::HEAD_MERGE.0
            | Self::COLOUR_COMPETITION.0,
    );

    /// The bits a filter chip in the grid offers.
    ///
    /// The four a photographer actually hunts for before a delivery, and the choice is
    /// deliberate: three of them are *geometric facts* a viewer can verify in a second,
    /// and none of them is the aesthetic score. A chip labelled "badly composed" is a
    /// chip that starts an argument the product cannot win.
    pub const CHIPS: [Self; 4] = [
        Self::HORIZON_TILTED,
        Self::HEAD_CROPPED,
        Self::JOINT_CUT,
        Self::HEAD_MERGE,
    ];

    /// Wrap a raw bit pattern, dropping the sixteen spare bits.
    ///
    /// Dropping rather than refusing, for the reason `SceneId::from_str_or_unknown`
    /// gives: a catalog written by a newer build must still open, and an unknown flag
    /// read as nothing is safer than an unknown flag read as a violation.
    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits & 0xFFFF)
    }

    /// The raw bit pattern, as stored in `image_composition.flags`.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// True when every bit of `other` is set here.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// True when any bit of `other` is set here.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// Both sets of bits.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Set or clear one flag.
    #[must_use]
    pub const fn with(self, other: Self, on: bool) -> Self {
        if on {
            Self(self.0 | other.0)
        } else {
            Self(self.0 & !other.0)
        }
    }

    /// Clear one flag. What [`CompositionService::dismiss`] does.
    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// True when nothing is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// True when at least one bit describes a violation.
    ///
    /// **Still not a rejection.** A frame with `JOINT_CUT` set may be the only
    /// photograph of the ring exchange, and phase 12 knows that and this crate does not.
    #[must_use]
    pub const fn has_violation(self) -> bool {
        self.0 & Self::DEFECTS.0 != 0
    }

    /// The violation bits alone.
    #[must_use]
    pub const fn violations(self) -> Self {
        Self(self.0 & Self::DEFECTS.0)
    }

    /// How many flags are set.
    #[must_use]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// The stable slug for one flag, for the wire, the chips and the logs.
    ///
    /// `None` for a combination rather than a single bit, which is what makes this safe
    /// to call in a loop over [`CompositionFlags::ALL`].
    #[must_use]
    pub const fn as_str(self) -> Option<&'static str> {
        Some(match self {
            Self::HORIZON_TILTED => "horizon_tilted",
            Self::INTENTIONAL_TILT => "intentional_tilt",
            Self::VERTICALS_CONVERGING => "verticals_converging",
            Self::HEADROOM_TIGHT => "headroom_tight",
            Self::HEADROOM_EXCESSIVE => "headroom_excessive",
            Self::HEAD_CROPPED => "head_cropped",
            Self::JOINT_CUT => "joint_cut",
            Self::LIMB_CUT => "limb_cut",
            Self::EDGE_INTRUSION => "edge_intrusion",
            Self::OFF_BALANCE => "off_balance",
            Self::CENTRED => "centred",
            Self::CLUTTERED_BACKGROUND => "cluttered_background",
            Self::BRIGHT_BLOB => "bright_blob",
            Self::HEAD_MERGE => "head_merge",
            Self::COLOUR_COMPETITION => "colour_competition",
            Self::NO_GEOMETRY => "no_geometry",
            _ => return None,
        })
    }

    /// Parse one slug back. Unknown text is [`CompositionFlags::EMPTY`].
    #[must_use]
    pub fn from_str_or_empty(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|flag| flag.as_str() == Some(text))
            .unwrap_or(Self::EMPTY)
    }

    /// Every set flag, in [`CompositionFlags::ALL`] order.
    #[must_use]
    pub fn set_flags(self) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|flag| self.contains(*flag))
            .collect()
    }
}

impl fmt::Display for CompositionFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("clean");
        }
        let names: Vec<&str> = self
            .set_flags()
            .into_iter()
            .filter_map(CompositionFlags::as_str)
            .collect();
        f.write_str(&names.join("+"))
    }
}

// ---------------------------------------------------------------------------
// The crop audit
// ---------------------------------------------------------------------------

/// Where a body was cut by the frame edge.
///
/// Section 2.1 names the wrist, the elbow, the knee and the ankle. Three more are here
/// because a keypoint model produces them anyway and a photographer names them when they
/// complain: the neck, the shoulder and the hip. The seven are ordered **down the body**,
/// which is also roughly the order of how much a cut at each one hurts.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Joint {
    /// The base of the neck. The worst cut on this list and the least often deliberate.
    #[default]
    Neck,
    /// The shoulder.
    Shoulder,
    /// The elbow.
    Elbow,
    /// The wrist. The cut that makes a hand look severed rather than out of frame.
    Wrist,
    /// The hip. The classic "cut at the waist" that a half-length portrait avoids.
    Hip,
    /// The knee.
    Knee,
    /// The ankle. The mildest, and the one a full-length portrait most often makes.
    Ankle,
}

impl Joint {
    /// Every joint, down the body.
    pub const ALL: [Self; 7] = [
        Self::Neck,
        Self::Shoulder,
        Self::Elbow,
        Self::Wrist,
        Self::Hip,
        Self::Knee,
        Self::Ankle,
    ];

    /// How many joints the audit names.
    pub const COUNT: usize = 7;

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Neck => "neck",
            Self::Shoulder => "shoulder",
            Self::Elbow => "elbow",
            Self::Wrist => "wrist",
            Self::Hip => "hip",
            Self::Knee => "knee",
            Self::Ankle => "ankle",
        }
    }

    /// The word a photographer reads.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Neck => "the neck",
            Self::Shoulder => "a shoulder",
            Self::Elbow => "an elbow",
            Self::Wrist => "a wrist",
            Self::Hip => "the waist",
            Self::Knee => "a knee",
            Self::Ankle => "an ankle",
        }
    }

    /// Parse the stored text. Unknown values read as [`Joint::Neck`].
    #[must_use]
    pub fn from_str_or_neck(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|joint| joint.as_str() == text)
            .unwrap_or(Self::Neck)
    }
}

impl fmt::Display for Joint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which edge of the frame did the cutting.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum FrameEdge {
    /// The bottom edge. The default because it is where most cuts happen.
    #[default]
    Bottom,
    /// The left edge.
    Left,
    /// The right edge.
    Right,
    /// The top edge.
    Top,
}

impl FrameEdge {
    /// Every edge.
    pub const ALL: [Self; 4] = [Self::Bottom, Self::Left, Self::Right, Self::Top];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
        }
    }

    /// Parse the stored text. Unknown values read as [`FrameEdge::Bottom`].
    #[must_use]
    pub fn from_str_or_bottom(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|edge| edge.as_str() == text)
            .unwrap_or(Self::Bottom)
    }
}

impl fmt::Display for FrameEdge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One place the frame cut a person.
///
/// Section 5's `{ joint, severity, box }`, with `box` spelled [`JointCut::area`] because
/// `box` is reserved, and with [`JointCut::at_joint`] carried beside the severity rather
/// than folded into it.
///
/// The two are separate on purpose. Section 6.1 says "cutting at a joint scores worse
/// than cutting mid-limb", which is a statement about *severity*; but the panel has to
/// say **which** of the two happened, because the remedies differ - a mid-limb cut is
/// usually fixed by cropping further in and a joint cut by cropping further out. A single
/// number cannot carry that and a caller reconstructing it from a threshold would be
/// re-deriving a decision this phase already made.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct JointCut {
    /// Which joint the cut is at, or nearest to.
    pub joint: Joint,
    /// Which edge did the cutting.
    pub edge: FrameEdge,
    /// True when the cut lands *on* the joint rather than between two of them.
    pub at_joint: bool,
    /// How much this costs, `0..1`. Scene-conditioned: the same ankle cut is a violation
    /// in a `family_portrait` and unremarkable on a `dance_floor`.
    pub severity: f32,
    /// The rectangle to show, section 5's `box`.
    pub area: Box2,
}

impl JointCut {
    /// True when this cut is severe enough to raise a flag.
    ///
    /// The threshold is in this contract rather than in the analyser because the panel,
    /// the store and the eval harness all ask the same question and three copies of `>=
    /// 0.30` is three chances to write one of them as `>`.
    pub const FLAG_AT: f32 = 0.30;

    /// True when this cut raises a flag.
    #[must_use]
    pub fn is_flagged(&self) -> bool {
        self.severity >= Self::FLAG_AT
    }
}

// ---------------------------------------------------------------------------
// The horizon
// ---------------------------------------------------------------------------

/// What the tilt estimate was made from.
///
/// Carried because section 6.1 asks for a horizon "cross-checked against gravity metadata
/// when the camera records it", and a stored angle that does not say where it came from
/// cannot be audited when it turns out to be wrong. It is also what makes
/// [`CompositionCode::HorizonUncertain`] honest: a 0.4 degree gate measured against a
/// gravity tag and one measured against a gradient histogram are two different claims.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum HorizonSource {
    /// Nothing in the frame said which way was level. The default, and the answer that
    /// claims the least.
    #[default]
    None,
    /// The orientation histogram of long edges - the weighted Hough of section 6.1.
    Gradient,
    /// Converging verticals gave a vanishing direction.
    VanishingLines,
    /// The camera recorded a gravity vector and it agreed with the pixels.
    Gravity,
}

impl HorizonSource {
    /// Every source, in the order they are trusted.
    pub const ALL: [Self; 4] = [
        Self::None,
        Self::Gradient,
        Self::VanishingLines,
        Self::Gravity,
    ];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gradient => "gradient",
            Self::VanishingLines => "vanishing_lines",
            Self::Gravity => "gravity",
        }
    }

    /// Parse the stored text. Unknown values read as [`HorizonSource::None`].
    #[must_use]
    pub fn from_str_or_none(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|source| source.as_str() == text)
            .unwrap_or(Self::None)
    }

    /// True when there was a horizon to measure at all.
    #[must_use]
    pub const fn is_measured(self) -> bool {
        !matches!(self, Self::None)
    }
}

impl fmt::Display for HorizonSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Below this confidence nothing is straightened and nothing is flagged.
///
/// Section 6.1: "report confidence and never straighten below 0.6 confidence". It lives
/// in the contract rather than in the analyser because phase 23 is the phase that would
/// do the straightening, and a threshold that two crates each keep a copy of is a
/// threshold that moves in one of them.
pub const HORIZON_ACT_AT: f32 = 0.60;

/// Above this angle a tilt is a candidate for being deliberate.
///
/// Section 6.1's "large tilt (> 6 deg)". Being a candidate is not being deliberate: the
/// other three conditions - a centred subject, no strong horizon and a scene that expects
/// it - are checked in `aura_brain_photo::composition::horizon`.
pub const DUTCH_ANGLE_DEG: f32 = 6.0;

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why the composition score moved, as a closed set.
///
/// Section 9 gives DOC "document composition reason codes with example images", which is
/// only a finishable job if the codes are enumerable. `docs/composition-and-framing.md`
/// is written against [`CompositionCode::ALL`] and a test asserts every variant appears
/// there.
///
/// **Twelve of these twenty-six carry no penalty.** They are the exonerations - the
/// reasons a photograph is *not* badly framed despite tripping a rule - and this is the
/// phase where they matter most. Every one of the four named in section 12's first
/// failure mode is here: a dutch angle, a deliberate tight crop, a centred subject and a
/// frame with nothing in it that a body rule applies to.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum CompositionCode {
    /// Nothing was wrong. The reason a cleanly framed photograph carries, so that every
    /// photograph has at least one.
    #[default]
    Clean,
    /// The horizon is off level by more than this scene tolerates.
    HorizonTilted,
    /// There was no reliable horizon to measure. **An exoneration**, in the sense that it
    /// withdraws a claim rather than making one.
    HorizonUncertain,
    /// The tilt reads as a decision. **An exoneration.**
    IntentionalTilt,
    /// Vertical lines converge: the camera was pointed up or down at architecture.
    VerticalsConverging,
    /// Less space above the head than this scene wants.
    HeadroomTight,
    /// More space above the head than this scene wants.
    HeadroomExcessive,
    /// The top of a head is cut off.
    HeadCropped,
    /// A tight crop this scene treats as deliberate. **An exoneration.**
    DeliberateCrop,
    /// The frame cuts a limb at a joint.
    JointCut,
    /// The frame cuts a limb between joints.
    LimbCut,
    /// Something enters the frame at an edge.
    EdgeIntrusion,
    /// The subject sits away from both the centre and the power points.
    OffThirds,
    /// The subject sits on a power point. **An exoneration.**
    ThirdsAligned,
    /// The subject is centred, in a scene where that is right. **An exoneration.**
    Centred,
    /// The visual weight sits on one side with nothing to answer it.
    OffBalance,
    /// The frame is balanced. **An exoneration.**
    Balanced,
    /// More going on behind the subject than this scene tolerates.
    ClutteredBackground,
    /// The background is quiet behind the subject. **An exoneration.**
    CleanBackground,
    /// Something brighter than the subject pulls the eye.
    BrightBlob,
    /// A vertical structure grows out of somebody's head.
    HeadMerge,
    /// A saturated background colour competes with the subject.
    ColourCompetition,
    /// There were no bodies and no faces to judge the framing of. **An exoneration.**
    NoGeometry,
    /// Faces were available but the body-keypoint audit was not. **An exoneration**: it
    /// withdraws limb and joint crop claims without falsely claiming that no subject was
    /// found.
    KeypointsUnavailable,
    /// This scene has no rule row, so the neutral bands were used. **An exoneration**: it
    /// says the verdict is less certain, which is why it also lowers
    /// [`CompositionResult::confidence`].
    NoRule,
    /// The learned aesthetic head did not run, so the score is geometry alone. **An
    /// exoneration**, for the same reason.
    AestheticUnavailable,
}

impl CompositionCode {
    /// Every code, in the order `docs/composition-and-framing.md` documents them.
    pub const ALL: [Self; 26] = [
        Self::Clean,
        Self::HorizonTilted,
        Self::HorizonUncertain,
        Self::IntentionalTilt,
        Self::VerticalsConverging,
        Self::HeadroomTight,
        Self::HeadroomExcessive,
        Self::HeadCropped,
        Self::DeliberateCrop,
        Self::JointCut,
        Self::LimbCut,
        Self::EdgeIntrusion,
        Self::OffThirds,
        Self::ThirdsAligned,
        Self::Centred,
        Self::OffBalance,
        Self::Balanced,
        Self::ClutteredBackground,
        Self::CleanBackground,
        Self::BrightBlob,
        Self::HeadMerge,
        Self::ColourCompetition,
        Self::NoGeometry,
        Self::KeypointsUnavailable,
        Self::NoRule,
        Self::AestheticUnavailable,
    ];

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::HorizonTilted => "horizon_tilted",
            Self::HorizonUncertain => "horizon_uncertain",
            Self::IntentionalTilt => "intentional_tilt",
            Self::VerticalsConverging => "verticals_converging",
            Self::HeadroomTight => "headroom_tight",
            Self::HeadroomExcessive => "headroom_excessive",
            Self::HeadCropped => "head_cropped",
            Self::DeliberateCrop => "deliberate_crop",
            Self::JointCut => "joint_cut",
            Self::LimbCut => "limb_cut",
            Self::EdgeIntrusion => "edge_intrusion",
            Self::OffThirds => "off_thirds",
            Self::ThirdsAligned => "thirds_aligned",
            Self::Centred => "centred",
            Self::OffBalance => "off_balance",
            Self::Balanced => "balanced",
            Self::ClutteredBackground => "cluttered_background",
            Self::CleanBackground => "clean_background",
            Self::BrightBlob => "bright_blob",
            Self::HeadMerge => "head_merge",
            Self::ColourCompetition => "colour_competition",
            Self::NoGeometry => "no_geometry",
            Self::KeypointsUnavailable => "keypoints_unavailable",
            Self::NoRule => "no_rule",
            Self::AestheticUnavailable => "aesthetic_unavailable",
        }
    }

    /// Parse the stored slug. Unknown text is [`CompositionCode::Clean`].
    #[must_use]
    pub fn from_str_or_clean(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|code| code.as_str() == text)
            .unwrap_or(Self::Clean)
    }

    /// The sentence this code says when nothing more specific is known.
    ///
    /// **Stored rows carry the code, not the sentence**, for the three reasons phase 09's
    /// `ReasonCode::user_text` gives - size, copy that changes without behaviour
    /// changing, and translation. A reason whose text differs from this, because it names
    /// a joint or an angle, stores its text as well and only then.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::Clean => "the framing reads normally here: level, well placed and uncluttered",
            Self::HorizonTilted => {
                "the horizon is off level by more than this kind of \
                 photograph usually carries"
            }
            Self::HorizonUncertain => {
                "there is no clear horizon in this frame, so AURA has not \
                 made a claim about whether it is level"
            }
            Self::IntentionalTilt => {
                "the tilt here is large, the subject is centred and the \
                 scene is one where a tilted frame is a choice: this reads as deliberate"
            }
            Self::VerticalsConverging => {
                "the vertical lines lean towards each other, which \
                 happens when the camera is pointed up or down at a building"
            }
            Self::HeadroomTight => {
                "there is less space above their head than this kind of \
                 photograph usually leaves"
            }
            Self::HeadroomExcessive => {
                "there is more empty space above their head than this \
                 kind of photograph usually leaves"
            }
            Self::HeadCropped => "the top of a head is cut off by the edge of the frame",
            Self::DeliberateCrop => {
                "this is a close portrait, where cropping into the head is \
                 a normal choice rather than a mistake"
            }
            Self::JointCut => {
                "the frame cuts through a joint, which reads as a severed limb \
                 rather than as a limb continuing out of shot"
            }
            Self::LimbCut => "the frame cuts through a limb between joints",
            Self::EdgeIntrusion => "something is entering the frame at the edge",
            Self::OffThirds => {
                "the subject sits away from both the centre and the natural \
                 placement points"
            }
            Self::ThirdsAligned => {
                "the subject sits close to one of the natural placement \
                 points"
            }
            Self::Centred => {
                "the subject is centred, which is how this kind of photograph is \
                 usually framed"
            }
            Self::OffBalance => {
                "the visual weight is heavily on one side with nothing on the \
                 other to answer it"
            }
            Self::Balanced => "the weight in this frame is spread evenly",
            Self::ClutteredBackground => {
                "there is more going on behind the subject than this \
                 kind of photograph carries well"
            }
            Self::CleanBackground => "the background behind the subject is quiet",
            Self::BrightBlob => {
                "something brighter than the subject is pulling the eye away \
                 from them"
            }
            Self::HeadMerge => {
                "a vertical line in the background runs straight out of \
                 somebody's head"
            }
            Self::ColourCompetition => {
                "a strong colour behind the subject is competing with \
                 them"
            }
            Self::NoGeometry => {
                "no people were found here, so the rules about headroom, \
                 cropping and head merges were not applied"
            }
            Self::KeypointsUnavailable => {
                "body keypoints were unavailable, so AURA did not \
                 make limb or joint crop claims for this photograph"
            }
            Self::NoRule => {
                "AURA has no framing rules recorded for this kind of photograph \
                 yet, so it has judged this one cautiously against neutral ones"
            }
            Self::AestheticUnavailable => {
                "the learned part of the composition judgement did \
                 not run here, so this score is geometry alone"
            }
        }
    }

    /// True when this code withdraws a claim rather than making one.
    ///
    /// Every exoneration is documented as such in its own doc comment; this is the same
    /// list, machine-readable, so the panel can render them differently and the eval
    /// harness can assert that an exoneration never carries a penalty.
    #[must_use]
    pub const fn is_exoneration(self) -> bool {
        matches!(
            self,
            Self::Clean
                | Self::HorizonUncertain
                | Self::IntentionalTilt
                | Self::DeliberateCrop
                | Self::ThirdsAligned
                | Self::Centred
                | Self::Balanced
                | Self::CleanBackground
                | Self::NoGeometry
                | Self::KeypointsUnavailable
                | Self::NoRule
                | Self::AestheticUnavailable
        )
    }
}

impl fmt::Display for CompositionCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One thing that moved the composition score, with the pixels that prove it.
///
/// Section 2.1's "per-flag evidence boxes". Invariant 2 makes the reason mandatory;
/// section 13's second criterion - "the overlay shows exactly why a frame was marked
/// badly composed" - makes the box mandatory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompositionReason {
    /// Which reason this is.
    pub code: CompositionCode,
    /// The sentence a photographer reads. Written in the product's voice, never in the
    /// analyser's: "the frame cuts through her wrist", rather than a dump of the keypoint
    /// index and the edge distance that decided it.
    pub text: String,
    /// How much of the score this moved.
    ///
    /// Negative for a penalty, zero or positive for an exoneration. Signed rather than a
    /// magnitude plus a sign bit, because the panel sorts by it and a sort that has to
    /// consult two fields gets written wrong once.
    pub weight: f32,
    /// The pixels to show. `None` only when the reason is about the whole frame and there
    /// is nothing more specific to point at - a tilt is about the whole frame, a cut
    /// wrist is not.
    pub evidence: Option<Box2>,
}

impl CompositionReason {
    /// A reason about the whole frame.
    #[must_use]
    pub fn frame(code: CompositionCode, text: impl Into<String>, weight: f32) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: None,
        }
    }

    /// A reason about one rectangle.
    #[must_use]
    pub fn at(code: CompositionCode, text: impl Into<String>, weight: f32, evidence: Box2) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: Some(evidence),
        }
    }

    /// True when this reason cost the frame something.
    #[must_use]
    pub fn is_penalty(&self) -> bool {
        self.weight < 0.0
    }
}

// ---------------------------------------------------------------------------
// The crop hint
// ---------------------------------------------------------------------------

/// What phase 23 is handed, and the word *hint* is the contract.
///
/// Section 2.1: "`crop_suggestion_hint` (region of interest and safe margins) handed to
/// Phase 23". Section 2.2 puts the cropping itself in phase 23, so nothing here is
/// applied to anything: this is a rectangle a later phase may optimise toward, together
/// with how much room it has to move.
///
/// **It is not a crop.** No aspect ratio is forced, no rotation is carried, and
/// [`CropHint::region`] is deliberately larger than the subject rather than tight to it.
/// A phase that received a finished crop would have nothing left to decide, and the
/// decision belongs to the phase that can see the delivery's aspect ratio and the
/// photographer's own crop history.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CropHint {
    /// What must stay in frame, normalised.
    pub region: Box2,
    /// How far the region may be tightened on each side, as a fraction of the frame.
    ///
    /// The safe margin section 2.1 asks for. Zero means "this is already as tight as it
    /// goes", which is what a frame that already cuts a limb gets: cropping further would
    /// turn a mid-limb cut into a joint cut.
    pub safe_margin: f32,
    /// The rotation phase 23 would have to apply to level the frame, in degrees.
    ///
    /// `None` when the horizon confidence is below [`HORIZON_ACT_AT`], which is section
    /// 6.1's "never straighten below 0.6 confidence" expressed as an absent value rather
    /// than as a zero. Zero degrees is a claim that the frame is level; `None` is the
    /// admission that nobody knows.
    pub straighten_deg: Option<f32>,
    /// How sure this hint is, `0..1`.
    pub confidence: f32,
}

impl CropHint {
    /// The smallest margin worth reporting.
    ///
    /// Below one per cent of the frame there is nothing for phase 23 to do with the room,
    /// and a hint that says "you may crop in by four pixels" is noise in a panel.
    pub const MIN_MARGIN: f32 = 0.01;

    /// True when this hint gives phase 23 anything to work with.
    #[must_use]
    pub fn is_actionable(&self) -> bool {
        !self.region.is_empty()
            && (self.safe_margin >= Self::MIN_MARGIN || self.straighten_deg.is_some())
    }
}

// ---------------------------------------------------------------------------
// The result
// ---------------------------------------------------------------------------

/// Everything phase 11 knows about how one photograph is framed.
///
/// PHASE-11 section 5's frozen shape, plus the additions this module's header lists.
/// Every field is a measurement, a label derived from one, or a hint; none of them is a
/// decision about whether the photograph is delivered, cropped or straightened.
///
/// Four `bool`s, which is one more than `clippy::struct_excessive_bools` likes, and the
/// lint is allowed here rather than obeyed. Three of the four - `tilt_intentional`,
/// `head_crop` and `head_merge` - are spelled as booleans in section 5's frozen shape,
/// and the fourth is the photographer's override that every phase since 06 carries. The
/// lint's remedy is to gather them into a sub-struct, which would rename three frozen
/// fields to satisfy a style rule.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct CompositionResult {
    /// The photograph.
    pub image_id: ImageId,
    /// How far off level the frame is, in degrees. Positive is clockwise.
    ///
    /// Bounded to `-45..45`: past forty-five degrees the "horizon" found is the other
    /// axis, and reporting an 89 degree tilt on a portrait-orientation frame is how a
    /// straightening tool rotates a photograph onto its side.
    pub tilt_deg: f32,
    /// True when the tilt reads as a decision rather than as a mistake.
    pub tilt_intentional: bool,
    /// How sure the tilt estimate is, `0..1`. Compared against [`HORIZON_ACT_AT`].
    pub horizon_conf: f32,
    /// What the tilt estimate was made from.
    pub horizon_source: HorizonSource,
    /// Space above the subject's head, as a fraction of frame height.
    ///
    /// Negative when the head is cut off - the depth of the cut, so that phase 23 knows
    /// how far out it would have to crop and the panel can say how much is missing.
    /// `0.0` with [`CompositionFlags::NO_GEOMETRY`] set means "not measured".
    pub headroom: f32,
    /// Distance from the main subject to the nearest power point, `0..1`.
    ///
    /// Normalised by the frame diagonal, so it is comparable between a landscape and a
    /// portrait frame. Zero is exactly on a power point.
    pub thirds_offset: f32,
    /// How balanced the frame is: 0 is lopsided, 1 is balanced.
    pub balance: f32,
    /// How much of the frame is empty, `0..1`.
    ///
    /// Not a defect in either direction, which is why it has no flag: a `details` flat-lay
    /// wants a lot of it and a `dance_floor` frame has none. It is carried because the
    /// aesthetic head reads it and because phase 23's crop needs to know what it is
    /// allowed to remove.
    pub negative_space: f32,
    /// Every place the frame cut a person, worst first.
    pub joint_cuts: Vec<JointCut>,
    /// True when the top of a head is cut where this scene does not allow it.
    pub head_crop: bool,
    /// Things entering the frame at an edge.
    pub edge_intrusions: Vec<Box2>,
    /// Background edge density relative to what this scene tolerates.
    ///
    /// `1.0` is exactly the scene's tolerance, so above one is too busy *for this scene*
    /// and the same background in a `dance_floor` frame and a `couple_portrait` produces
    /// two different numbers. Invariant 7 applied to a measurement rather than to a
    /// threshold, exactly as phase 09's `noise_sigma_rel` is.
    pub clutter: f32,
    /// Bright regions brighter than the subject, largest first.
    pub bright_blobs: Vec<Box2>,
    /// True when a vertical structure grows out of a head.
    pub head_merge: bool,
    /// How much the background colour competes with the subject, `0..1`.
    pub colour_competition: f32,
    /// The learned, scene-conditioned aesthetic reading, `0..1`.
    ///
    /// **Capped in influence by construction.** Section 6.3: "aesthetic score is capped
    /// in influence: Phase 12 weights it below technical integrity and emotion, because
    /// taste should break ties, not override substance." The cap is applied here, at
    /// `score::AESTHETIC_CAP`, and not left to phase 12 to remember.
    pub aesthetic: f32,
    /// The fused, calibrated composite, `0..1`.
    ///
    /// A product of sub-scores with soft penalties, not a sum - the same shape phase 09's
    /// `technical_score` has - so that one catastrophic framing error cannot be averaged
    /// away by four good measures.
    pub composition_score: f32,
    /// What phase 23 is handed, when there is anything to hand it.
    pub crop_suggestion_hint: Option<CropHint>,
    /// Which scene the bands were conditioned on.
    ///
    /// Invariant 7 makes every threshold a function of the scene, so a stored judgement
    /// that does not say which scene it assumed is not reproducible. [`SceneId::Unknown`]
    /// means the frame was judged on the neutral rule row.
    pub scene: SceneId,
    /// Where this frame's composition sits inside its moment, `0..1`.
    ///
    /// The mission sentence, as a number: "among six technically equal frames the
    /// best-composed one wins". One is the best-composed frame of the moment, zero the
    /// worst. `0.5` when the frame is not in a moment, or is alone in one - neither the
    /// best nor the worst of no siblings.
    pub relative_composition: f32,
    /// How many people the crop audit had keypoints for.
    ///
    /// The denominator behind [`CompositionOutline::keypoint_aware`], per frame.
    pub keypoint_subjects: u32,
    /// Everything true of the framing.
    pub flags: CompositionFlags,
    /// Why, at most [`CompositionResult::MAX_REASONS`], strongest penalty first.
    pub reasons: Vec<CompositionReason>,
    /// How sure the whole judgement is, `0..1`. Invariant 2.
    ///
    /// Lowered by a missing scene rule row, an absent keypoint reading, an absent
    /// aesthetic head and an unmeasurable horizon. A judgement drawn with three of its
    /// four inputs missing has to be able to say so.
    pub confidence: f32,
    /// True when the photographer overruled a flag on this frame.
    ///
    /// Set by [`CompositionService::dismiss`], and checked inside the statement a
    /// re-analysis would overwrite the row with. A photographer's decision is unbeatable,
    /// for the sixth phase running.
    pub user_reviewed: bool,
    /// Which learned heads produced the keypoints and the aesthetic.
    pub model_ver: u16,
    /// Which build's arithmetic produced the geometry and the composite.
    pub analysis_ver: u16,
    /// Which rule table the bands came from.
    pub rules_ver: u16,
}

impl CompositionResult {
    /// The most reasons one frame carries.
    ///
    /// Six, matching `IntegrityResult::MAX_REASONS` and `ImageEmotion::MAX_REASONS`. A
    /// panel that lists nine reasons is a panel nobody reads, and the ranking that picks
    /// the six is part of the explanation.
    pub const MAX_REASONS: usize = 6;

    /// True when something is wrong with the framing.
    ///
    /// **Not "reject it".** See this module's header, and section 2.2.
    #[must_use]
    pub const fn has_violation(&self) -> bool {
        self.flags.has_violation()
    }

    /// True when there were people in the frame for the body rules to apply to.
    #[must_use]
    pub const fn has_geometry(&self) -> bool {
        !self.flags.contains(CompositionFlags::NO_GEOMETRY)
    }

    /// The reasons that cost the frame something, strongest first.
    #[must_use]
    pub fn penalties(&self) -> Vec<&CompositionReason> {
        let mut out: Vec<&CompositionReason> =
            self.reasons.iter().filter(|r| r.is_penalty()).collect();
        out.sort_by(|left, right| left.weight.total_cmp(&right.weight));
        out
    }

    /// The reasons that withdrew a claim, in the order they were made.
    #[must_use]
    pub fn exonerations(&self) -> Vec<&CompositionReason> {
        self.reasons
            .iter()
            .filter(|reason| reason.code.is_exoneration())
            .collect()
    }

    /// The cuts severe enough to have raised a flag, worst first.
    #[must_use]
    pub fn flagged_cuts(&self) -> Vec<&JointCut> {
        let mut out: Vec<&JointCut> = self
            .joint_cuts
            .iter()
            .filter(|cut| cut.is_flagged())
            .collect();
        out.sort_by(|left, right| right.severity.total_cmp(&left.severity));
        out
    }

    /// True when the three version fields match this build's.
    ///
    /// The check `AURA-ML-5043` is raised on. Written here rather than in the analyser so
    /// that every consumer asks the question the same way.
    #[must_use]
    pub const fn matches_versions(&self, model: u16, analysis: u16, rules: u16) -> bool {
        self.model_ver == model && self.analysis_ver == analysis && self.rules_ver == rules
    }
}

// ---------------------------------------------------------------------------
// Coverage
// ---------------------------------------------------------------------------

/// What a project's composition pass covered and what it found.
///
/// Not in section 5. Phase 05's rule, inherited for the seventh time: report coverage
/// when you report a result, and say what the denominator is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompositionOutline {
    /// Photographs with a judgement.
    pub scored: u32,
    /// Photographs in the project.
    pub photos: u32,
    /// Fraction of the project with a judgement, `0..1`.
    ///
    /// The denominator is **every photograph**, as phase 09's and phase 10's are. A
    /// horizon, a balance and a background reading need only pixels, and every photograph
    /// in a catalog has pixels - so a frame with no judgement here is this phase's gap
    /// and reporting it any other way would hide it.
    pub coverage: f32,
    /// Fraction of *scored* frames that had body keypoints, `0..1`.
    ///
    /// This phase's refinement of the coverage rule, and the number that matters when it
    /// is low. Six of the sixteen flags and the whole crop audit come from keypoints; a
    /// wedding at 100 % coverage and 4 % keypoint-aware has had its horizon measured
    /// everywhere and its limb cropping measured almost nowhere, and a caller about to
    /// trust `joint_cuts` needs to know that before it does.
    pub keypoint_aware: f32,
    /// How many frames carry each flag, in [`CompositionFlags::ALL`] order.
    ///
    /// Section 11's `composition.scored {flag_histogram}` event, computed once.
    pub flag_histogram: [u32; CompositionFlags::COUNT],
    /// Mean composition score over scored frames.
    pub mean_score: f32,
    /// Mean absolute tilt in degrees over frames with a measurable horizon.
    ///
    /// Section 11's `composition.tilt {mean_abs_deg}`. The denominator is frames with a
    /// horizon rather than every frame, because averaging in the zero that an
    /// unmeasurable frame carries would make a wedding shot indoors look level.
    pub mean_abs_tilt: f32,
    /// Fraction of tilted frames whose tilt was read as deliberate, `0..1`.
    ///
    /// Section 11's `composition.tilt {intentional_ratio}`, and the number section 12's
    /// first failure mode is watched with: a build that starts reading nothing as
    /// intentional is a build that has started punishing creative work.
    pub intentional_ratio: f32,
    /// Frames with a crop hint for phase 23.
    pub hinted: u32,
    /// Frames the photographer has overruled.
    pub reviewed: u32,
    /// Scenes with no rule row, by slug.
    ///
    /// Section 12's argument about rigid rules, made measurable. A wedding whose scenes
    /// are all missing from the rule table has been judged entirely on neutral bands.
    pub unruled_scenes: Vec<String>,
    /// Which learned heads produced the numbers.
    pub model_ver: u16,
    /// Which build's arithmetic produced them.
    pub analysis_ver: u16,
    /// Which rule table the bands came from.
    pub rules_ver: u16,
}

impl CompositionOutline {
    /// True when nothing has been scored.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.scored == 0
    }

    /// How many frames carry one flag.
    #[must_use]
    pub fn count(&self, flag: CompositionFlags) -> u32 {
        CompositionFlags::ALL
            .iter()
            .position(|candidate| *candidate == flag)
            .and_then(|index| self.flag_histogram.get(index).copied())
            .unwrap_or(0)
    }

    /// Frames carrying at least one violation flag.
    ///
    /// An upper bound rather than an exact count - one frame can be tilted *and*
    /// cluttered - and it is documented as such because the number is shown in the review
    /// queue's header where an exact count would be assumed.
    #[must_use]
    pub fn violating_at_most(&self) -> u32 {
        CompositionFlags::ALL
            .iter()
            .enumerate()
            .filter(|(_, flag)| CompositionFlags::DEFECTS.contains(**flag))
            .filter_map(|(index, _)| self.flag_histogram.get(index).copied())
            .sum()
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask how a photograph is framed.
///
/// Frozen. Implemented by `aura_brain_photo::composition::Composition`, and the only
/// route from any later phase to a tilt, a headroom, a crop violation or an aesthetic
/// reading.
///
/// The rule phase 05 wrote for `SimilarityIndex`, phase 06 for `PeopleService`, phase 07
/// for `StoryService`, phase 08 for `MomentService`, phase 09 for `IntegrityService` and
/// phase 10 for `EmotionService`, a seventh time and for the same reason: **no phase may
/// keep its own idea of good framing.** Phase 23's smart crop optimises toward this
/// objective and phase 29 ranks albums with it; two answers to "is this well composed" is
/// a crop that fights the gallery it is in.
pub trait CompositionService: Send + Sync + fmt::Debug {
    /// What a project's pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the judgements cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<CompositionOutline>;

    /// One photograph's framing, or `None` when it has not been analysed.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the judgement cannot be read.
    fn of_image(&self, image: ImageId) -> AuraResult<Option<CompositionResult>>;

    /// Photographs carrying any of these flags, worst score first.
    ///
    /// The filter chips and the framing review queue. `limit` bounds the answer because
    /// the queue is a page and a 4,000-frame wedding with a loose chip would otherwise
    /// return every frame in it.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the judgements cannot be read.
    fn flagged(
        &self,
        project: ProjectId,
        flags: CompositionFlags,
        limit: usize,
    ) -> AuraResult<Vec<ImageId>>;

    /// One moment's frames ranked by composition, best first.
    ///
    /// The mission sentence as a method: among six technically equal frames, this is how
    /// a caller finds the best-framed one. The second element is
    /// [`CompositionResult::relative_composition`].
    ///
    /// **An ordering, not a shortlist.** Phase 10 wrote that rule and this inherits it:
    /// nothing here says which frames to keep, and the panel that renders this says so in
    /// its own header.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the moment or its judgements cannot be read.
    fn within_moment(&self, moment: MomentId) -> AuraResult<Vec<(ImageId, f32)>>;

    /// What phase 23 should keep in frame, when there is anything to say.
    ///
    /// Separate from [`CompositionService::of_image`] because phase 23 wants this and
    /// nothing else, and reading a whole judgement to take one field out of it is how a
    /// crop pass ends up decoding reasons it never renders.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the judgement cannot be read.
    fn crop_hint(&self, image: ImageId) -> AuraResult<Option<CropHint>>;

    /// Record that the photographer disagrees with one flag on one frame.
    ///
    /// Clears the flag, sets [`CompositionResult::user_reviewed`], and is not undone by a
    /// re-analysis: the check is inside the statement that would overwrite the row,
    /// exactly as `identities.user_locked`, `segments.user_locked`, `moments.user_locked`
    /// and `image_integrity.user_reviewed` are.
    ///
    /// It changes what the panel shows and what a filter chip returns. It does not change
    /// whether the photograph is delivered or how it is cropped, because nothing in this
    /// phase does.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5044` when the photograph has no judgement, or when `flag` is not a
    /// single flag that is currently set on it.
    fn dismiss(&self, image: ImageId, flag: CompositionFlags) -> Result<(), AuraError>;
}
