//! Composition flag bitset and its stable wire spellings.

use std::fmt;

use serde::{Deserialize, Serialize};

Exit code: 0
Wall time: 1.3 seconds
Output:
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
