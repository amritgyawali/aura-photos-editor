//! FROZEN CONTRACT. What is technically wrong with a photograph, where it is wrong,
//! and how sure we are.
//!
//! PHASE-09 section 5 freezes these shapes before any analyser exists. The file is in
//! `aura-core` rather than in `aura-brain-photo` for the reason
//! [`crate::contract::moment`] and [`crate::contract::scene`] are: the phases that
//! consume a technical verdict are 12 (culling and coverage), 13 (the explainability
//! ledger), 22 (restoration), 25 (gallery consistency) and 27 (the QC agent), and none
//! of them needs the Laplacian, the structure tensor or the eye-state head.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **This phase describes a photograph; it never rejects one.** Section 2.2 puts every
//! keep-or-reject decision in phase 12, and the boundary is kept structurally: nothing
//! in this contract can cull, rank or delete a frame, and there is no field that would
//! let it. [`IntegrityResult::technical_score`] is evidence in the same sense that
//! phase 05's distances, phase 07's scene tolerances and phase 08's groupings are
//! evidence.
//!
//! That matters more here than it did in any of those phases, because this is the
//! phase whose output looks most like a verdict. Section 12's first failure mode is
//! "false rejections destroy trust", and a `technical_score` of 0.31 read as a
//! rejection is exactly how that happens.
//!
//! ## Seven spellings differ from the phase document
//!
//! All seven are recorded in `docs/adr/ADR-0019-frame-integrity-and-eye-intent.md`
//! section 2.
//!
//! * **`ImageId` is `PhotoId`**, aliased rather than duplicated, exactly as the scene,
//!   people and moment contracts already do it.
//! * **`bitflags!` is hand-rolled.** [`IntegrityFlags`] is a `u32` newtype with the
//!   same fourteen bits section 5 names and the same API surface the rest of this
//!   crate's bitfields have. The `bitflags` crate is not a workspace dependency and
//!   [`crate::contract::scene::AttrFlags`] already set the precedent.
//! * **[`ExposureVerdict`] has four variants, not three.** Section 6.3 names
//!   `recoverable`, `marginal` and `lost`; a correctly exposed frame is none of those.
//!   Folding it into `Recoverable` would tell phase 15 that every photograph in a
//!   wedding needs a correction, so [`ExposureVerdict::Good`] exists and is the
//!   default.
//! * **[`Reason`] carries a typed [`ReasonCode`]** rather than a free string. Section 5
//!   spells the shape `{ code, text, weight, evidence_crop }` and section 9 gives DOC
//!   "document every reason code in user language" - a deliverable that is only
//!   possible if the set of codes is closed. `docs/frame-integrity.md` is generated
//!   against [`ReasonCode::ALL`].
//! * **A [`Reason`] may be an exoneration.** [`Reason::weight`] is signed, and the
//!   reasons that matter most in this phase are the ones that say *why something is
//!   not a defect*: [`ReasonCode::ShallowDepthOfField`] and
//!   [`ReasonCode::EyesClosedOk`] both carry zero or positive weight. A panel that can
//!   only explain penalties cannot explain the decision a photographer disagrees with.
//! * **[`IntegrityOutline`] is not in section 5 at all.** It is the coverage carrier,
//!   inherited from phase 05 for the fifth time, with this phase's refinement written
//!   at [`IntegrityOutline::subject_aware`].
//! * **[`IntegrityService`] is not in section 5 either.** Five later phases consume
//!   technical verdicts and a contract with no entry point makes each of them find its
//!   own way in.
//!
//! ## Three version fields, because they invalidate three different things
//!
//! [`IntegrityResult::model_ver`] invalidates the subject sharpness and every eye
//! state, because both come from a learned head. [`IntegrityResult::analysis_ver`]
//! invalidates the motion kind, the exposure verdict, the noise figure, the flags and
//! the score, because those come from this build's arithmetic.
//! [`IntegrityResult::calib_ver`] invalidates every *normalised* number, because a
//! camera calibration table is what makes "sharp" mean sharp for this body.
//!
//! Comparing two rows across any of them returns a plausible number that means
//! nothing. `AURA-ML-5033` exists so that comparison never happens silently. Fifth
//! phase, fifth version-drift code.
//!
//! Changing anything in this file requires an ADR.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{FaceId, IdentityId, MomentId, ProjectId};
use crate::contract::scene::SceneId;

pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

/// Everything that can be technically true of one photograph.
///
/// PHASE-09 section 5's fourteen bits, in section 5's order, with their values
/// unchanged - the bit positions are a wire format the `image_integrity.flags` column
/// stores and a later build has to be able to read.
///
/// A hand-rolled `u32` rather than `bitflags!`; see this module's header. Eighteen bits
/// spare, which is not laziness about the type: phases 10, 11 and 22 will each want to
/// say something about a frame that this phase measured but did not name, and a
/// fifteenth flag should not need a migration.
///
/// **Two of the fourteen are not defects.** [`IntegrityFlags::INTENTIONAL_MOTION`] and
/// [`IntegrityFlags::EYES_CLOSED_OK`] are the phase's whole argument in two bits: a
/// panned exit frame and a kiss with closed eyes are the photographs that sell
/// weddings. [`IntegrityFlags::defects`] is the mask that knows the difference, and it
/// exists so that five later phases do not each write the same `!matches!`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(transparent)]
pub struct IntegrityFlags(u32);

impl IntegrityFlags {
    /// Nothing is true of this frame. The verdict on a clean photograph.
    pub const EMPTY: Self = Self(0);

    /// The subject the frame is about is not sharp.
    ///
    /// Subject, not frame: a blurred background is craft and a blurred bride is a
    /// defect, and the distinction is the reason this phase exists.
    pub const SUBJECT_SOFT: Self = Self(1 << 0);
    /// Global directional smear with a long shutter: the photographer moved.
    pub const CAMERA_SHAKE: Self = Self(1 << 1);
    /// Local smear against a sharp background: the subject moved.
    pub const SUBJECT_MOTION: Self = Self(1 << 2);
    /// Motion that reads as a decision - a pan, a dance-floor drag, a spin.
    ///
    /// **Not a defect.** Section 6.2: "never flag intentional motion as a defect;
    /// instead expose `INTENTIONAL_MOTION` so Phase 12 can treat it as a stylistic
    /// keeper."
    pub const INTENTIONAL_MOTION: Self = Self(1 << 3);
    /// The sharpest plane is behind the subject.
    pub const BACK_FOCUS: Self = Self(1 << 4);
    /// The sharpest plane is in front of the subject.
    pub const FRONT_FOCUS: Self = Self(1 << 5);
    /// Highlights are clipped past what the RAW headroom can bring back.
    pub const HIGHLIGHT_LOST: Self = Self(1 << 6);
    /// Shadows are crushed into the noise floor.
    pub const SHADOW_LOST: Self = Self(1 << 7);
    /// Noise beyond what this scene tolerates on this body at this ISO.
    pub const HEAVY_NOISE: Self = Self(1 << 8);
    /// An important subject's eyes are closed and nothing justifies it.
    pub const EYES_CLOSED: Self = Self(1 << 9);
    /// Closed eyes the scene, the expression or the partner justifies.
    ///
    /// **Not a defect.** The single most trust-critical bit in the product: a kiss, a
    /// prayer, a first-look reaction and a tearful hug all have closed eyes in them,
    /// and a naive eye-open detector destroys exactly those frames.
    pub const EYES_CLOSED_OK: Self = Self(1 << 10);
    /// An important subject is squinting.
    pub const SQUINT: Self = Self(1 << 11);
    /// No face and no body: the sharpness verdict is about the frame, not a subject.
    ///
    /// Not a defect either, and its presence changes how every other flag should be
    /// read. A detail shot of rings has no subject to be soft.
    pub const NO_SUBJECT_DETECTED: Self = Self(1 << 12);
    /// Two illuminants of different colour temperature on the same skin.
    ///
    /// Carried here rather than in phase 15 because it is measured from the same pass
    /// and because it changes what "recoverable" means for exposure.
    pub const MIXED_LIGHT_RISK: Self = Self(1 << 13);

    /// Every flag, in section 5's order. The order the histogram is reported in.
    pub const ALL: [Self; 14] = [
        Self::SUBJECT_SOFT,
        Self::CAMERA_SHAKE,
        Self::SUBJECT_MOTION,
        Self::INTENTIONAL_MOTION,
        Self::BACK_FOCUS,
        Self::FRONT_FOCUS,
        Self::HIGHLIGHT_LOST,
        Self::SHADOW_LOST,
        Self::HEAVY_NOISE,
        Self::EYES_CLOSED,
        Self::EYES_CLOSED_OK,
        Self::SQUINT,
        Self::NO_SUBJECT_DETECTED,
        Self::MIXED_LIGHT_RISK,
    ];

    /// How many flags the bitfield names.
    pub const COUNT: usize = 14;

    /// The bits that describe something wrong with a photograph.
    ///
    /// Everything except the three that describe something *right* with it, or
    /// something merely true of it: intentional motion, justified closed eyes, and the
    /// absence of a subject to judge.
    pub const DEFECTS: Self = Self(
        Self::SUBJECT_SOFT.0
            | Self::CAMERA_SHAKE.0
            | Self::SUBJECT_MOTION.0
            | Self::BACK_FOCUS.0
            | Self::FRONT_FOCUS.0
            | Self::HIGHLIGHT_LOST.0
            | Self::SHADOW_LOST.0
            | Self::HEAVY_NOISE.0
            | Self::EYES_CLOSED.0
            | Self::SQUINT.0
            | Self::MIXED_LIGHT_RISK.0,
    );

    /// The bits a filter chip in the grid offers.
    ///
    /// Section 9's `MFE` deliverable: "filter chips (soft, blinked, clipped, noisy)".
    /// The four a photographer actually hunts for before a delivery.
    pub const CHIPS: [Self; 4] = [
        Self::SUBJECT_SOFT,
        Self::EYES_CLOSED,
        Self::HIGHLIGHT_LOST,
        Self::HEAVY_NOISE,
    ];

    /// Wrap a raw bit pattern, dropping the eighteen spare bits.
    ///
    /// Dropping rather than refusing, for the reason `SceneId::from_str_or_unknown`
    /// gives: a catalog written by a newer build must still open, and an unknown flag
    /// read as nothing is safer than an unknown flag read as a defect.
    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits & 0x3FFF)
    }

    /// The raw bit pattern, as stored in `image_integrity.flags`.
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
    ///
    /// The question a filter chip asks: "show me the frames that are soft *or*
    /// blinked".
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

    /// Clear one flag.
    ///
    /// What [`IntegrityService::dismiss`] does when the photographer overrules a
    /// penalty.
    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// True when nothing is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// True when at least one bit describes a defect.
    ///
    /// **Still not a rejection.** A frame with `SUBJECT_SOFT` set may be the only
    /// photograph of the ring exchange, and phase 12 knows that and this crate does
    /// not.
    #[must_use]
    pub const fn has_defect(self) -> bool {
        self.0 & Self::DEFECTS.0 != 0
    }

    /// The defect bits alone.
    #[must_use]
    pub const fn defects(self) -> Self {
        Self(self.0 & Self::DEFECTS.0)
    }

    /// How many flags are set.
    #[must_use]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// The stable slug for one flag, for the wire, the filter chips and the logs.
    ///
    /// `None` for a combination rather than a single bit, which is what makes this
    /// safe to call in a loop over [`IntegrityFlags::ALL`].
    #[must_use]
    pub const fn as_str(self) -> Option<&'static str> {
        Some(match self {
            Self::SUBJECT_SOFT => "subject_soft",
            Self::CAMERA_SHAKE => "camera_shake",
            Self::SUBJECT_MOTION => "subject_motion",
            Self::INTENTIONAL_MOTION => "intentional_motion",
            Self::BACK_FOCUS => "back_focus",
            Self::FRONT_FOCUS => "front_focus",
            Self::HIGHLIGHT_LOST => "highlight_lost",
            Self::SHADOW_LOST => "shadow_lost",
            Self::HEAVY_NOISE => "heavy_noise",
            Self::EYES_CLOSED => "eyes_closed",
            Self::EYES_CLOSED_OK => "eyes_closed_ok",
            Self::SQUINT => "squint",
            Self::NO_SUBJECT_DETECTED => "no_subject_detected",
            Self::MIXED_LIGHT_RISK => "mixed_light_risk",
            _ => return None,
        })
    }

    /// Parse one slug back. Unknown text is [`IntegrityFlags::EMPTY`].
    #[must_use]
    pub fn from_str_or_empty(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|flag| flag.as_str() == Some(text))
            .unwrap_or(Self::EMPTY)
    }

    /// Every set flag, in [`IntegrityFlags::ALL`] order.
    ///
    /// A `Vec` rather than an iterator, for the reason `AttrFlags::set_names` returns
    /// one: the caller is a panel or a log line that is about to collect anyway, and a
    /// borrowing iterator over a `Copy` bitfield would need a lifetime for nothing.
    #[must_use]
    pub fn set_flags(self) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|flag| self.contains(*flag))
            .collect()
    }
}

impl fmt::Display for IntegrityFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("clean");
        }
        let names: Vec<&str> = self
            .set_flags()
            .into_iter()
            .filter_map(IntegrityFlags::as_str)
            .collect();
        f.write_str(&names.join("+"))
    }
}

// ---------------------------------------------------------------------------
// Motion
// ---------------------------------------------------------------------------

/// What kind of motion is in a frame, if any.
///
/// PHASE-09 section 6.2's four outcomes. The distinction between the last two is the
/// one that decides whether a photographer trusts this product: a dance-floor drag and
/// a missed focus both produce a smeared subject, and only one of them is a mistake.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum MotionKind {
    /// Nothing moved enough to say anything about. The default, because it is the
    /// answer that claims the least.
    #[default]
    None,
    /// Global anisotropic smear with a shutter below the focal-length rule: the
    /// camera moved.
    CameraShake,
    /// Local smear with a sharp background: the subject moved.
    SubjectMotion,
    /// A panning signature, or motion in a scene that expects it. **Not a defect.**
    Intentional,
}

impl MotionKind {
    /// Every kind, in the order section 6.2 names them.
    pub const ALL: [Self; 4] = [
        Self::None,
        Self::CameraShake,
        Self::SubjectMotion,
        Self::Intentional,
    ];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::CameraShake => "camera_shake",
            Self::SubjectMotion => "subject_motion",
            Self::Intentional => "intentional",
        }
    }

    /// Parse the stored text. Unknown values read as [`MotionKind::None`], which is the
    /// direction that claims least.
    #[must_use]
    pub fn from_str_or_none(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == text)
            .unwrap_or(Self::None)
    }

    /// True when this motion counts against a frame.
    #[must_use]
    pub const fn is_defect(self) -> bool {
        matches!(self, Self::CameraShake | Self::SubjectMotion)
    }

    /// The flag this kind raises, if any.
    #[must_use]
    pub const fn flag(self) -> IntegrityFlags {
        match self {
            Self::None => IntegrityFlags::EMPTY,
            Self::CameraShake => IntegrityFlags::CAMERA_SHAKE,
            Self::SubjectMotion => IntegrityFlags::SUBJECT_MOTION,
            Self::Intentional => IntegrityFlags::INTENTIONAL_MOTION,
        }
    }
}

impl fmt::Display for MotionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Exposure
// ---------------------------------------------------------------------------

/// How much of the exposure can be brought back.
///
/// Section 6.3's three labels plus [`ExposureVerdict::Good`]; see this module's header
/// for why the fourth exists.
///
/// **Recovery-aware, not histogram-aware.** A two-stop-under dance frame from a modern
/// sensor is `Recoverable` and the same frame from a 2012 body is `Marginal`, because
/// the question is not how dark the photograph is but what lifting it would cost. The
/// per-body headroom and noise floor that decide it live in
/// `aura_brain_photo::integrity::calibration`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ExposureVerdict {
    /// Nothing needs doing. Phase 15 may still grade it; it does not need to rescue it.
    #[default]
    Good,
    /// A correction stays inside the noise budget for this ISO on this body.
    Recoverable,
    /// A correction is possible and will cost something visible.
    Marginal,
    /// The information is not in the file. Highlights past headroom, shadows below the
    /// noise floor, or both.
    Lost,
}

impl ExposureVerdict {
    /// Every verdict, best first.
    pub const ALL: [Self; 4] = [Self::Good, Self::Recoverable, Self::Marginal, Self::Lost];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Recoverable => "recoverable",
            Self::Marginal => "marginal",
            Self::Lost => "lost",
        }
    }

    /// Parse the stored text.
    ///
    /// Unknown values read as [`ExposureVerdict::Good`]. That is the *optimistic*
    /// direction and it is deliberate: this phase's documented failure mode is false
    /// rejection, so a value a build cannot interpret must not become a penalty.
    #[must_use]
    pub fn from_str_or_good(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|verdict| verdict.as_str() == text)
            .unwrap_or(Self::Good)
    }

    /// True when phase 15 has real work to do here.
    #[must_use]
    pub const fn needs_recovery(self) -> bool {
        matches!(self, Self::Recoverable | Self::Marginal)
    }
}

impl fmt::Display for ExposureVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Eyes
// ---------------------------------------------------------------------------

/// What one pair of eyes is doing.
///
/// Section 2.1's five classes. `LookingDown` is separate from `Closed` because they
/// look alike to a classifier and mean opposite things to a photographer: a bride
/// looking down at her bouquet is a photograph, and a bride mid-blink is a reshoot.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum EyeOpenness {
    /// Open. The default, and the class the head must be conservative about leaving.
    #[default]
    Open,
    /// Narrowed - sun, laughter, a flash about to fire.
    Squint,
    /// Closed.
    Closed,
    /// Open, but directed downwards. Not a blink.
    LookingDown,
    /// Hidden by a veil, a hand, a glass, another head, or the frame edge.
    Occluded,
}

impl EyeOpenness {
    /// Every class, in the head's output order. **This order is the model's output
    /// order**, for the same reason `SceneId::ALL` is the scene head's: renumbering it
    /// relabels a trained model.
    pub const ALL: [Self; 5] = [
        Self::Open,
        Self::Squint,
        Self::Closed,
        Self::LookingDown,
        Self::Occluded,
    ];

    /// How many classes the head emits.
    pub const COUNT: usize = 5;

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Squint => "squint",
            Self::Closed => "closed",
            Self::LookingDown => "looking_down",
            Self::Occluded => "occluded",
        }
    }

    /// Parse the stored text. Unknown values read as [`EyeOpenness::Open`], which is
    /// the direction that does not invent a defect.
    #[must_use]
    pub fn from_str_or_open(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|state| state.as_str() == text)
            .unwrap_or(Self::Open)
    }

    /// True when the eyes are shut, whatever the reason.
    ///
    /// `Occluded` is not shut - it is unknown, and treating unknown as shut is how a
    /// veil becomes a defect.
    #[must_use]
    pub const fn is_closed(self) -> bool {
        matches!(self, Self::Closed)
    }
}

impl fmt::Display for EyeOpenness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One face's eyes, with who it is and whether the closure is intentional.
///
/// Section 5's `Vec<EyeState>`, "per face, with identity + intent".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EyeState {
    /// The face phase 06 found.
    pub face_id: FaceId,
    /// Who that face is, when the identity pass has run and assigned one.
    ///
    /// `None` means unassigned, which is not the same as "a guest": an unclustered
    /// face has no importance yet and therefore [`EyeState::gates`] is false.
    pub identity: Option<IdentityId>,
    /// What the eyes are doing.
    pub state: EyeOpenness,
    /// How sure the head is, `0..1`.
    pub confidence: f32,
    /// True when the closure is justified by the scene, the expression or the partner.
    ///
    /// Section 6.4's intent rules. Only meaningful when [`EyeState::state`] is
    /// [`EyeOpenness::Closed`]; false otherwise and never inspected in that case.
    pub intentional: bool,
    /// True when this face's eyes decide anything about the frame.
    ///
    /// Section 6.4: "only the *important* identities' eyes gate a frame: a guest
    /// blinking in row four is not a defect; the bride blinking in a portrait is."
    /// The importance comes from phase 06's prominence and role; this crate reads it
    /// and does not compute it.
    pub gates: bool,
    /// How much of the frame this face covers, `0..1`. Carried so the review panel can
    /// order faces the way a viewer's eye does.
    pub area_frac: f32,
    /// Where the face is, for the evidence crop the panel zooms to.
    pub crop: CropRect,
}

impl EyeState {
    /// True when this face is a reason to flag the frame.
    ///
    /// Closed, gating, and not justified - all three, and the order they are written in
    /// is the order they are argued in.
    #[must_use]
    pub const fn is_defect(&self) -> bool {
        self.state.is_closed() && self.gates && !self.intentional
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// A rectangle in a photograph, normalised to the frame.
///
/// Section 5's `evidence_crop`, and section 13's last criterion: "the Integrity card
/// shows the exact crop that caused each penalty." Normalised rather than in pixels so
/// that the same rectangle addresses the thumbnail, the proxy and the original.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct CropRect {
    /// Left edge, `0..1`.
    pub x: f32,
    /// Top edge, `0..1`.
    pub y: f32,
    /// Width, `0..1`.
    pub w: f32,
    /// Height, `0..1`.
    pub h: f32,
}

impl CropRect {
    /// The whole frame. What a reason about the entire photograph points at.
    pub const FULL: Self = Self {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    /// Clamp to the frame, so a crop grown for context never addresses off-image
    /// pixels.
    #[must_use]
    pub fn clamped(self) -> Self {
        let x = self.x.clamp(0.0, 1.0);
        let y = self.y.clamp(0.0, 1.0);
        Self {
            x,
            y,
            w: self.w.clamp(0.0, 1.0 - x),
            h: self.h.clamp(0.0, 1.0 - y),
        }
    }

    /// Grow the rectangle by a fraction of its own size, centred, and clamp.
    ///
    /// A face box is the wrong crop to show a photographer: an eye at 100 % zoom with
    /// no jaw in the frame is unreadable. The panel shows the box plus context.
    #[must_use]
    pub fn expanded(self, fraction: f32) -> Self {
        let grow_w = self.w * fraction / 2.0;
        let grow_h = self.h * fraction / 2.0;
        Self {
            x: self.x - grow_w,
            y: self.y - grow_h,
            w: self.w + grow_w * 2.0,
            h: self.h + grow_h * 2.0,
        }
        .clamped()
    }

    /// True when the rectangle covers no area.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
}

/// Why the score moved, as a closed set.
///
/// Section 9 gives DOC "document every reason code in user language", which is only a
/// finishable job if the codes are enumerable. `docs/frame-integrity.md` is written
/// against [`ReasonCode::ALL`] and a test asserts every variant appears there.
///
/// **Eight of these nineteen carry no penalty.** They are the exonerations - the
/// reasons a photograph is *not* defective despite looking like it might be - and they
/// exist because the panel that can only explain penalties is the panel a photographer
/// stops believing.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    /// Nothing was wrong. The reason a clean frame carries, so that every frame has at
    /// least one.
    #[default]
    Clean,
    /// The subject is softer than this body and this scene allow.
    SubjectSoft,
    /// The subject is soft *and* the background is sharp behind it.
    BackFocus,
    /// The subject is soft *and* something in front of it is sharp.
    FrontFocus,
    /// The background is soft and the subject is not. **An exoneration.**
    ShallowDepthOfField,
    /// Directional smear across the whole frame at a shutter below the focal rule.
    CameraShake,
    /// Smear on the subject with a sharp background.
    SubjectMotion,
    /// Motion the scene expects, or a clean panning signature. **An exoneration.**
    IntentionalMotion,
    /// Clipped highlights past this body's measured headroom.
    HighlightLost,
    /// Clipped highlights inside headroom. **An exoneration.**
    HighlightRecoverable,
    /// Clipped highlights that are a light source. **An exoneration.**
    SpecularHighlight,
    /// Shadows below the noise floor for this ISO on this body.
    ShadowLost,
    /// Noise above what this scene tolerates.
    HeavyNoise,
    /// Noise this scene expects at this ISO. **An exoneration.**
    NoiseWithinScene,
    /// An important subject's eyes are closed with nothing to justify it.
    EyesClosed,
    /// Closed eyes the scene, the expression or the partner justifies. **An
    /// exoneration**, and the most important one in the product.
    EyesClosedOk,
    /// An important subject is squinting.
    Squint,
    /// Several people in a group frame have their eyes closed.
    GroupBlink,
    /// No face and no body was found, so the sharpness verdict is about the frame.
    /// **An exoneration**, in the sense that it withdraws a claim rather than making
    /// one.
    NoSubject,
    /// Two illuminants of different colour temperature on the same skin.
    MixedLight,
    /// This camera body has no calibration row, so the normalisation is by sensor
    /// resolution alone. **An exoneration**: it says the verdict is less certain,
    /// which is why it also lowers [`IntegrityResult::confidence`].
    Uncalibrated,
}

impl ReasonCode {
    /// Every code, in the order `docs/frame-integrity.md` documents them.
    pub const ALL: [Self; 21] = [
        Self::Clean,
        Self::SubjectSoft,
        Self::BackFocus,
        Self::FrontFocus,
        Self::ShallowDepthOfField,
        Self::CameraShake,
        Self::SubjectMotion,
        Self::IntentionalMotion,
        Self::HighlightLost,
        Self::HighlightRecoverable,
        Self::SpecularHighlight,
        Self::ShadowLost,
        Self::HeavyNoise,
        Self::NoiseWithinScene,
        Self::EyesClosed,
        Self::EyesClosedOk,
        Self::Squint,
        Self::GroupBlink,
        Self::NoSubject,
        Self::MixedLight,
        Self::Uncalibrated,
    ];

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::SubjectSoft => "subject_soft",
            Self::BackFocus => "back_focus",
            Self::FrontFocus => "front_focus",
            Self::ShallowDepthOfField => "shallow_depth_of_field",
            Self::CameraShake => "camera_shake",
            Self::SubjectMotion => "subject_motion",
            Self::IntentionalMotion => "intentional_motion",
            Self::HighlightLost => "highlight_lost",
            Self::HighlightRecoverable => "highlight_recoverable",
            Self::SpecularHighlight => "specular_highlight",
            Self::ShadowLost => "shadow_lost",
            Self::HeavyNoise => "heavy_noise",
            Self::NoiseWithinScene => "noise_within_scene",
            Self::EyesClosed => "eyes_closed",
            Self::EyesClosedOk => "eyes_closed_ok",
            Self::Squint => "squint",
            Self::GroupBlink => "group_blink",
            Self::NoSubject => "no_subject",
            Self::MixedLight => "mixed_light",
            Self::Uncalibrated => "uncalibrated",
        }
    }

    /// Parse the stored slug. Unknown text is [`ReasonCode::Clean`].
    #[must_use]
    pub fn from_str_or_clean(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|code| code.as_str() == text)
            .unwrap_or(Self::Clean)
    }

    /// The sentence this code says when nothing more specific is known.
    ///
    /// **Stored rows carry the code, not the sentence.** Three reasons, and the third is
    /// the one that matters:
    ///
    /// * a sentence is 60 to 120 bytes and a code is one; section 11 budgets a kilobyte
    ///   for a whole verdict and six stored sentences would be most of it;
    /// * the wording is user-facing copy, which changes in a release that changes no
    ///   behaviour - and stored copy would leave a wedding's old frames explaining
    ///   themselves in last year's words;
    /// * the wording is what a translation replaces, and a catalog full of English
    ///   sentences is a catalog that cannot be translated.
    ///
    /// A reason whose text differs from this - the camera-shake sentence naming a shutter
    /// speed - stores its text as well, and only then.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::Clean => "focus, motion, exposure, noise and eyes all read normally here",
            Self::SubjectSoft => {
                "the subject is softer than this camera should manage in this kind of photograph"
            }
            Self::BackFocus => {
                "what is behind the subject is sharper than the subject is, which usually means                  the focus landed behind them"
            }
            Self::FrontFocus => {
                "something in front of the subject is sharper than the subject is, which usually                  means the focus landed short"
            }
            Self::ShallowDepthOfField => {
                "the background is soft and the subject is sharp: this reads as a deliberate                  shallow depth of field, not as a missed focus"
            }
            Self::CameraShake => {
                "the whole frame is smeared in one direction, which is the camera moving"
            }
            Self::SubjectMotion => {
                "the subject is smeared while the background is not, so they moved during the                  exposure"
            }
            Self::IntentionalMotion => {
                "the blur here runs in one direction and the subject is held: this reads as a                  deliberate pan or drag rather than a mistake"
            }
            Self::HighlightLost => {
                "the highlights are clipped past what this camera can bring back"
            }
            Self::HighlightRecoverable => {
                "some highlights are clipped, but within what this camera can bring back"
            }
            Self::SpecularHighlight => {
                "the brightest parts of this frame are lights rather than blown detail, so they                  are not counted against it"
            }
            Self::ShadowLost => {
                "the shadows are below this camera's noise floor at this ISO, so lifting them                  would show more noise than detail"
            }
            Self::HeavyNoise => "there is more noise here than this kind of photograph carries well",
            Self::NoiseWithinScene => {
                "this frame is noisy, and no noisier than this kind of photograph at this ISO                  usually is"
            }
            Self::EyesClosed => {
                "their eyes are closed here, and nothing about this moment explains it"
            }
            Self::EyesClosedOk => {
                "closed eyes belong to this moment, so this is not marked as a fault"
            }
            Self::Squint => {
                "they are squinting here, which usually means the sun or a flash was in their eyes"
            }
            Self::GroupBlink => {
                "several people in this group have their eyes closed at the same time"
            }
            Self::NoSubject => {
                "no face or person was found here, so the sharpness reading is about the whole                  frame rather than about a subject"
            }
            Self::MixedLight => {
                "two different colours of light are falling on this scene, so one white balance                  will not correct all of it"
            }
            Self::Uncalibrated => {
                "AURA has not measured this camera model yet, so every reading here is a                  cautious one"
            }
        }
    }

    /// True when this code withdraws a claim rather than making one.
    ///
    /// Every exoneration is documented as such in its own doc comment; this is the
    /// same list, machine-readable, so the panel can render them differently and the
    /// eval harness can assert that an exoneration never carries a penalty.
    #[must_use]
    pub const fn is_exoneration(self) -> bool {
        matches!(
            self,
            Self::Clean
                | Self::ShallowDepthOfField
                | Self::IntentionalMotion
                | Self::HighlightRecoverable
                | Self::SpecularHighlight
                | Self::NoiseWithinScene
                | Self::EyesClosedOk
                | Self::NoSubject
                | Self::Uncalibrated
        )
    }
}

impl fmt::Display for ReasonCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One thing that moved the score, with the pixels that prove it.
///
/// Section 5's `{ code, text, weight, evidence_crop }`. Invariant 2 makes it mandatory;
/// section 13's last criterion makes the crop mandatory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Reason {
    /// Which reason this is.
    pub code: ReasonCode,
    /// The sentence a photographer reads. Written in the product's voice, never in the
    /// analyser's: "her eyes are closed and the scene is a kiss", rather than a dump of
    /// the two fields that decided it.
    pub text: String,
    /// How much of the score this moved.
    ///
    /// Negative for a penalty, zero or positive for an exoneration. Signed rather than
    /// a magnitude plus a sign bit, because the panel sorts by it and a sort that has
    /// to consult two fields gets written wrong once.
    pub weight: f32,
    /// The pixels to show. `None` only when the reason is about the whole frame and
    /// there is nothing more specific to point at.
    pub evidence: Option<CropRect>,
}

impl Reason {
    /// A reason about the whole frame.
    #[must_use]
    pub fn frame(code: ReasonCode, text: impl Into<String>, weight: f32) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: None,
        }
    }

    /// A reason about one rectangle.
    #[must_use]
    pub fn at(
        code: ReasonCode,
        text: impl Into<String>,
        weight: f32,
        evidence: CropRect,
    ) -> Self {
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
// The result
// ---------------------------------------------------------------------------

/// Everything phase 09 knows about one photograph.
///
/// PHASE-09 section 5's frozen shape, plus the six additions this module's header
/// lists. Every field is a measurement or a label derived from one; none of them is a
/// decision about whether the photograph is delivered.
#[derive(Debug, Clone, PartialEq)]
pub struct IntegrityResult {
    /// The photograph.
    pub image_id: ImageId,
    /// How sharp the subject is, `0..1`, weighted by phase 06's prominence.
    ///
    /// Absolute, and normalised by camera calibration so that "sharp" means sharp for
    /// this body. When there is no subject this is the frame's central sharpness and
    /// [`IntegrityFlags::NO_SUBJECT_DETECTED`] is set.
    pub subject_sharpness: f32,
    /// How sharp the background bands are, `0..1`.
    pub bg_sharpness: f32,
    /// Where the sharpest plane is relative to the subject, `-1..1`.
    ///
    /// Negative is front focus - something nearer the camera is sharp. Positive is back
    /// focus. Zero is "on the subject, or not determinable".
    pub focus_offset: f32,
    /// What kind of motion is in the frame.
    pub motion: MotionKind,
    /// How much of it, `0..1`. Meaningless when [`IntegrityResult::motion`] is
    /// [`MotionKind::None`], and zero in that case.
    pub motion_severity: f32,
    /// How much of the exposure can be brought back.
    pub exposure: ExposureVerdict,
    /// Fraction of pixels clipped at the top, `0..1`, specular highlights excluded.
    pub clip_hi: f32,
    /// Fraction of pixels crushed at the bottom, `0..1`.
    pub clip_lo: f32,
    /// How far from a correct exposure, in stops. Negative is under.
    pub ev_offset: f32,
    /// Noise sigma relative to what this scene tolerates at this ISO on this body.
    ///
    /// `1.0` is exactly the scene's tolerance, so above one is too noisy *for this
    /// scene* and the same absolute noise in a dance frame and a family portrait
    /// produces two different numbers. That is section 6.3's requirement and it is why
    /// the field is named `_rel`.
    pub noise_sigma_rel: f32,
    /// One entry per face phase 06 found, in prominence order.
    pub eyes: Vec<EyeState>,
    /// Fraction of the *gating* subjects whose eyes are closed, `0..1`.
    ///
    /// Section 6.4's group path. Zero when nobody gates, which is the same value as
    /// "nobody blinked" and is why phase 12 reads [`IntegrityResult::gating_faces`]
    /// beside it.
    pub closed_eye_ratio: f32,
    /// How many faces gate this frame. The denominator of
    /// [`IntegrityResult::closed_eye_ratio`].
    pub gating_faces: u32,
    /// The scene-weighted composite, `0..1`.
    ///
    /// A product of sub-scores with soft penalties, not a sum - section 6.5 - so that
    /// one catastrophic factor cannot be averaged away by four good ones. Calibrated
    /// per scene, so 0.8 means the same thing in a ceremony and on a dance floor.
    pub technical_score: f32,
    /// Which scene the thresholds were conditioned on.
    ///
    /// Invariant 7 makes every threshold a function of the scene, so a stored verdict
    /// that does not say which scene it assumed is not reproducible.
    /// [`SceneId::Unknown`] means the frame was judged on the neutral profile.
    pub scene: SceneId,
    /// Where this frame's subject sharpness sits inside its moment, `0..1`.
    ///
    /// Section 6.1: "phase 12 mostly needs 'which of these six is sharpest'". One is
    /// the sharpest frame of the moment, zero the softest. `0.5` when the frame is not
    /// in a moment, or is alone in one - the neutral value, because a frame with no
    /// siblings is neither the best nor the worst of them.
    pub relative_sharpness: f32,
    /// Everything true of the frame.
    pub flags: IntegrityFlags,
    /// Why, at most [`IntegrityResult::MAX_REASONS`], strongest penalty first.
    pub reasons: Vec<Reason>,
    /// How sure the whole verdict is, `0..1`. Invariant 2.
    ///
    /// Lowered by a missing calibration row, a missing scene label, a missing face pass
    /// and a low-confidence eye head. A verdict drawn with three of its five inputs
    /// missing has to be able to say so.
    pub confidence: f32,
    /// True when the photographer overruled a flag on this frame.
    ///
    /// Set by [`IntegrityService::dismiss`], and checked inside the statement a
    /// re-analysis would overwrite the row with. A photographer's decision is
    /// unbeatable, for the fourth phase running.
    pub user_reviewed: bool,
    /// Which learned heads produced the sharpness and the eye states.
    pub model_ver: u16,
    /// Which build's arithmetic produced the flags, the verdicts and the score.
    pub analysis_ver: u16,
    /// Which camera calibration table normalised the numbers.
    pub calib_ver: u16,
}

impl IntegrityResult {
    /// The most reasons one frame carries.
    ///
    /// Six, matching `Moment::MAX_REASONS`. A panel that lists nine reasons is a panel
    /// nobody reads, and the ranking that picks the six is part of the explanation.
    pub const MAX_REASONS: usize = 6;

    /// True when something is wrong with the photograph.
    ///
    /// **Not "reject it".** See this module's header, and section 2.2.
    #[must_use]
    pub const fn has_defect(&self) -> bool {
        self.flags.has_defect()
    }

    /// True when the frame has a subject to have been judged against.
    #[must_use]
    pub const fn has_subject(&self) -> bool {
        !self.flags.contains(IntegrityFlags::NO_SUBJECT_DETECTED)
    }

    /// The reasons that cost the frame something, strongest first.
    #[must_use]
    pub fn penalties(&self) -> Vec<&Reason> {
        let mut out: Vec<&Reason> = self.reasons.iter().filter(|r| r.is_penalty()).collect();
        out.sort_by(|left, right| left.weight.total_cmp(&right.weight));
        out
    }

    /// The reasons that withdrew a claim, in the order they were made.
    #[must_use]
    pub fn exonerations(&self) -> Vec<&Reason> {
        self.reasons
            .iter()
            .filter(|reason| reason.code.is_exoneration())
            .collect()
    }

    /// Eye states that count against the frame.
    #[must_use]
    pub fn blinking(&self) -> Vec<&EyeState> {
        self.eyes.iter().filter(|eye| eye.is_defect()).collect()
    }

    /// True when the three version fields match this build's.
    ///
    /// The check `AURA-ML-5033` is raised on. Written here rather than in the analyser
    /// so that every consumer asks the question the same way.
    #[must_use]
    pub const fn matches_versions(&self, model: u16, analysis: u16, calib: u16) -> bool {
        self.model_ver == model && self.analysis_ver == analysis && self.calib_ver == calib
    }
}

// ---------------------------------------------------------------------------
// Coverage
// ---------------------------------------------------------------------------

/// What a project's technical pass covered and what it found.
///
/// Not in section 5. Phase 05's rule, inherited for the fifth time: report coverage
/// when you report a result, and say what the denominator is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IntegrityOutline {
    /// Photographs with a verdict.
    pub scored: u32,
    /// Photographs in the project.
    pub photos: u32,
    /// Fraction of the project with a verdict, `0..1`.
    ///
    /// The denominator is **every photograph**, unlike phase 08's, and the difference
    /// is not an oversight. Grouping needs an embedding, so a frame without one cannot
    /// be grouped by any amount of trying. A technical verdict needs only pixels, and
    /// every photograph in a catalog has pixels - so a frame with no verdict here is
    /// this phase's gap and reporting it any other way would hide it.
    pub coverage: f32,
    /// Fraction of *scored* frames that had a face or a body to be subject-aware about,
    /// `0..1`.
    ///
    /// This phase's refinement of the coverage rule. A wedding scored at 100 % coverage
    /// with `subject_aware` at 0.02 has been judged on frame-wide sharpness almost
    /// everywhere, which is the ordinary global measure this phase exists to replace -
    /// and a caller about to trust `subject_sharpness` needs to know that before it
    /// does.
    pub subject_aware: f32,
    /// How many frames carry each flag, in [`IntegrityFlags::ALL`] order.
    ///
    /// Section 11's `integrity.scored {flag_histogram}` event, computed once.
    pub flag_histogram: [u32; IntegrityFlags::COUNT],
    /// Mean technical score over scored frames.
    pub mean_score: f32,
    /// Frames the photographer has overruled.
    pub reviewed: u32,
    /// Camera bodies with no calibration row, by make and model.
    ///
    /// Section 11's `integrity.camera_uncalibrated {make, model}` event, and section
    /// 12's fourth failure mode. A wedding shot entirely on an uncalibrated body is a
    /// wedding judged by sensor resolution alone, and this is the field that says so.
    pub uncalibrated: Vec<String>,
    /// Which learned heads produced the numbers.
    pub model_ver: u16,
    /// Which build's arithmetic produced them.
    pub analysis_ver: u16,
    /// Which calibration table normalised them.
    pub calib_ver: u16,
}

impl IntegrityOutline {
    /// True when nothing has been scored.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.scored == 0
    }

    /// How many frames carry one flag.
    #[must_use]
    pub fn count(&self, flag: IntegrityFlags) -> u32 {
        IntegrityFlags::ALL
            .iter()
            .position(|candidate| *candidate == flag)
            .and_then(|index| self.flag_histogram.get(index).copied())
            .unwrap_or(0)
    }

    /// Frames carrying at least one defect flag.
    ///
    /// An upper bound rather than an exact count - one frame can be soft *and* noisy -
    /// and it is documented as such because the number is shown in the review queue's
    /// header where an exact count would be assumed.
    #[must_use]
    pub fn defective_at_most(&self) -> u32 {
        IntegrityFlags::ALL
            .iter()
            .enumerate()
            .filter(|(_, flag)| IntegrityFlags::DEFECTS.contains(**flag))
            .filter_map(|(index, _)| self.flag_histogram.get(index).copied())
            .sum()
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask what is technically wrong with a photograph.
///
/// Frozen. Implemented by `aura_brain_photo::integrity::Integrity`, and the only route
/// from any later phase to a sharpness, a motion kind, an exposure verdict or an eye
/// state.
///
/// The rule phase 05 wrote for `SimilarityIndex`, phase 06 for `PeopleService`, phase
/// 07 for `StoryService` and phase 08 for `MomentService`, a fifth time and for the
/// same reason: **no phase may keep its own technical scoring.** Two answers to "is
/// this frame sharp" is two culling decisions that disagree, and phase 12's coverage
/// guarantee is written against this one.
pub trait IntegrityService: Send + Sync + fmt::Debug {
    /// What a project's pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the verdicts cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<IntegrityOutline>;

    /// One photograph's verdict, or `None` when it has not been analysed.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the verdict cannot be read.
    fn of_image(&self, image: ImageId) -> AuraResult<Option<IntegrityResult>>;

    /// Photographs carrying any of these flags, worst score first.
    ///
    /// The filter chips and the technical review queue. `limit` bounds the answer
    /// because the queue is a page and a 4,000-frame wedding with a loose chip would
    /// otherwise return every frame in it.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the verdicts cannot be read.
    fn flagged(
        &self,
        project: ProjectId,
        flags: IntegrityFlags,
        limit: usize,
    ) -> AuraResult<Vec<ImageId>>;

    /// One moment's frames ranked by subject sharpness, sharpest first.
    ///
    /// Section 6.1's "within-moment relative" answer, which is the question phase 12
    /// asks most often: not "is this sharp" but "which of these six is sharpest". The
    /// second element is [`IntegrityResult::relative_sharpness`].
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the moment or its verdicts cannot be read.
    fn within_moment(&self, moment: MomentId) -> AuraResult<Vec<(ImageId, f32)>>;

    /// Record that the photographer disagrees with one flag on one frame.
    ///
    /// Clears the flag, sets [`IntegrityResult::user_reviewed`], and is not undone by a
    /// re-analysis: the check is inside the statement that would overwrite the row,
    /// exactly as `identities.user_locked`, `segments.user_locked` and
    /// `moments.user_locked` are.
    ///
    /// It changes what the panel shows and what a filter chip returns. It does not
    /// change whether the photograph is delivered, because nothing in this phase does.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5034` when the photograph has no verdict, or when `flag` is not a
    /// single flag that is currently set on it.
    fn dismiss(&self, image: ImageId, flag: IntegrityFlags) -> Result<(), AuraError>;
}
