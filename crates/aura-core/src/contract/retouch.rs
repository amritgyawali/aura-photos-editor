//! FROZEN CONTRACT. What was done to somebody's skin: which marks were removed, which were
//! protected because they are that person rather than a defect, how much of the original
//! texture survived it, and what the product refused to do.
//!
//! PHASE-20 section 5 freezes [`RetouchPlan`] before any solver exists. The file is in
//! `aura-core` for the reason [`crate::contract::integrity`], [`crate::contract::composition`],
//! [`crate::contract::tone`], [`crate::contract::colour`] and [`crate::contract::local`] are:
//! the phases that consume a retouch decision are 21 (micro retouch, which must not re-smooth
//! what this phase smoothed), 25 (gallery consistency, which normalises across frames whose
//! skin work differs) and 27 (QC, which has to be able to say *why* a face looks worked on),
//! and none of them needs the detector, the patch synthesis or the band arithmetic.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **A protected feature is not a threshold, it is a veto.** Section 10.1 gates false removal
//! of permanent features at two per cent and of tattoos at *zero*, and those are different
//! kinds of number: the first is a measurement and the second is a promise. So
//! [`ProtectedFeature::vetoes`] removes a candidate from the list rather than scaling it down,
//! and [`ProtectedKind::is_absolute`] marks the one kind a photographer cannot switch off
//! either. There is no value of any strength anywhere in this module at which a mole is
//! partially inpainted, because a partially inpainted mole is a smudged one.
//!
//! ## The second thing: the guarantee is a measurement
//!
//! [`TextureReport::band_ratio`] is the high-frequency band energy of the skin *after* the
//! retouch divided by the same energy before it, measured by running the plan through the real
//! renderer. Section 6.3 turns "we don't produce plastic skin" from a claim into a tested
//! invariant, and the number lives on the row so the claim is `SELECT MIN(band_ratio)` rather
//! than a sentence in a document. When it cannot be met the retouch is withdrawn - see
//! [`TextureReport::withdrawn`] - because a frame that ships unretouched is a much smaller
//! failure than a frame that ships plastic.
//!
//! ## The third thing: strength belongs to a person, not to a frame
//!
//! [`RetouchPlan::per_identity_strength`] is a **gallery constant**. Section 6.4 lists four
//! inputs - face size, scene, role and preset - and section 10.1 asks that one identity's
//! strength vary by no more than five per cent across a wedding; those two are only
//! simultaneously satisfiable if the four inputs are read as *gallery* statistics, which is
//! what `aura_retouch::strength` does. What the individual frame decides is which operations
//! run at all. `docs/adr/ADR-0043-portrait-retouch-and-texture-protection.md` section 6 has
//! the argument.
//!
//! ## What this contract cannot express
//!
//! There is no field here for body geometry, for a skin-tone target, for a face swap or for a
//! reshaping envelope, and that is structural rather than remembered: section 11 of
//! `docs/plan/CLAUDE.md` forbids all four permanently, and a schema with nowhere to put them
//! is a schema in which adding them is a visible contract change rather than a quiet field.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::composition::Box2;
use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{IdentityId, MaskId, ProjectId};
use crate::contract::scene::SceneId;

pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// Floors, caps and triggers
// ---------------------------------------------------------------------------

/// The high-band energy a retouched skin region must keep, as a fraction of what it had.
///
/// Section 6.3's hard floor and section 0's headline KPI. Ninety per cent is the point at
/// which a retoucher looking at a 100 % crop stops being able to say that pores were touched;
/// below it the skin starts reading as *rendered* rather than photographed, and the effect is
/// cumulative across a gallery in a way one frame never shows.
pub const TEXTURE_FLOOR: f32 = 0.90;

/// The floor [`RetouchPreset::Polished`] may lower to, and never past.
///
/// Section 6.3: "configurable per preset, never below 0.80 even in Polished". A bound on the
/// *config file* rather than a default inside it - `aura_retouch::presets` refuses a table that
/// sets a floor below this with `AURA-ML-5099`, because a claim the product makes in
/// `docs/retouch.md` must not be retractable by editing a text file.
pub const POLISHED_FLOOR: f32 = 0.80;

/// The probability that an anomaly is temporary, below which nothing is done to it.
///
/// Section 6.1: "uncertain anomalies are left alone, because removing a client's mole is a far
/// worse error than leaving a pimple". Deliberately not one half. Everything between this and
/// [`PERMANENT_FLOOR`] is the band in which the product says [`RetouchCode::AnomalyUncertain`]
/// and moves on.
pub const TEMPORARY_FLOOR: f32 = 0.75;

/// The probability that an anomaly is permanent, above which it joins the protect set.
///
/// Lower than [`TEMPORARY_FLOOR`]'s mirror would be, and that asymmetry is the ethical core of
/// the phase written as a number: it is easier to become protected than to become removable.
pub const PERMANENT_FLOOR: f32 = 0.55;

/// How many frames a mark must appear on, at the same place on the same face, to be permanent.
///
/// Section 6.1's cross-frame evidence. Four rather than two, because a two-frame agreement is
/// most often the two halves of one burst.
pub const PERMANENCE_MIN_FRAMES: u32 = 4;

/// How many minutes those frames must span.
///
/// Both this and [`PERMANENCE_MIN_FRAMES`] must hold. Forty-five minutes is longer than any
/// burst, longer than a formals set-up and long enough that a lighting artefact has moved.
pub const PERMANENCE_MIN_SPAN_MIN: f32 = 45.0;

/// The most an under-eye region may be lifted, in stops.
///
/// Section 6.4: "typical luma lift <= 0.25 EV inside the periorbital mask ... because
/// over-correction is the classic tell of automated retouching". The cap is hard rather than
/// typical here, because a cap that the solver may exceed on an unusual frame is a cap that is
/// exceeded on exactly the frames a person looks at.
pub const MAX_UNDEREYE_LUMA_EV: f32 = 0.25;

/// The most an under-eye region's chroma may be moved, `0..1`.
///
/// Dark circles are blue-magenta against skin, and the correction is a *reduction* of that
/// separation rather than a move toward a target colour. Twelve per cent of the measured
/// separation is where the shadow stops reading as a bruise and before the eye socket starts
/// reading as flat.
pub const MAX_UNDEREYE_CHROMA: f32 = 0.12;

/// The most tone evening may move the mid band, as a fraction of its own energy.
///
/// The mid band is where blotches, flush and makeup mismatch live. Reducing it by more than a
/// third takes the modelling out of the face along with the blotches - the difference between
/// a face that is evenly lit and a face that is a mask.
pub const MAX_EVENING_MID: f32 = 0.33;

/// The largest blemish, as a fraction of the face's shorter side, this phase will inpaint.
///
/// Above this the region is not a blemish. A twelfth of a face is a birthmark, a bruise, a
/// shadow or a piece of confetti, and patch synthesis over an area that size invents skin
/// rather than borrowing it.
pub const MAX_BLEMISH_FRACTION: f32 = 1.0 / 12.0;

/// The smallest face, as a fraction of the frame's shorter side, this phase will retouch.
///
/// Below this a face is about forty pixels on a 2048 px proxy: the periorbital region is four
/// pixels tall, the mid band is two samples wide, and every operation here would be measuring
/// its own resampling. The face is still *counted* - see [`RetouchOutline::faces_seen`] - and
/// [`RetouchCode::FaceTooSmall`] says which of the two happened.
pub const MIN_RETOUCHABLE_FACE: f32 = 0.045;

/// How much a re-solve gives up when the texture guard refuses a plan.
///
/// A quarter of the current strength. Smaller steps take more passes through the renderer for
/// no benefit; larger ones overshoot to a retouch nobody can see and report a pass.
pub const TEXTURE_RESOLVE_STEP: f32 = 0.25;

/// How many times the texture guard will re-solve before withdrawing the retouch.
pub const TEXTURE_MAX_RESOLVES: u8 = 3;

/// Below this plan confidence the frame is worth a photographer's attention.
///
/// The same shape as [`crate::contract::tone::REVIEW_WB_BELOW`] and
/// [`crate::contract::local::REVIEW_BELOW`], and used the same way: it feeds
/// [`RetouchService::needs_review`] and gates nothing.
pub const REVIEW_BELOW: f32 = 0.50;

/// The most operations one plan may carry.
///
/// Two hundred. A frame with sixty faces and three marks each is the realistic worst case; past
/// that the plan is a stored image in disguise and section 11's budget could not hold it.
pub const MAX_OPS: usize = 200;

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

/// How much care a photograph is given, as a product decision a person picks.
///
/// Section 2.1's "global Light/Natural/Polished presets", plus the switch that turns the phase
/// off. `Off` is a variant rather than a `bool` beside the enum, because a plan that ran under
/// "the photographer switched retouching off" and a plan that ran at Light and found nothing
/// are two different answers and a coverage report has to tell them apart.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum RetouchPreset {
    /// Nothing is retouched. The kill switch hard rule 8 requires.
    Off,
    /// Blemishes only, and only the confident ones.
    Light,
    /// The default. PM owns it and section 9 says so.
    #[default]
    Natural,
    /// The most this product will do, and still inside [`POLISHED_FLOOR`].
    Polished,
}

impl RetouchPreset {
    /// Every preset, gentlest first.
    pub const ALL: [Self; 4] = [Self::Off, Self::Light, Self::Natural, Self::Polished];

    /// How many presets there are.
    pub const COUNT: usize = 4;

    /// Stable text for the catalog, the wire and the config file. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Light => "light",
            Self::Natural => "natural",
            Self::Polished => "polished",
        }
    }

    /// Parse the stable text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == text)
    }

    /// The texture floor this preset may not go below.
    ///
    /// Polished is the only preset allowed to lower the floor at all, and only to
    /// [`POLISHED_FLOOR`]. Light and Natural are held at [`TEXTURE_FLOOR`], which is what makes
    /// "the default cannot produce plastic skin" a property of the enum.
    #[must_use]
    pub const fn floor(self) -> f32 {
        match self {
            Self::Polished => POLISHED_FLOOR,
            _ => TEXTURE_FLOOR,
        }
    }

    /// True when this preset does nothing at all.
    #[must_use]
    pub const fn is_off(self) -> bool {
        matches!(self, Self::Off)
    }
}

impl fmt::Display for RetouchPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// The operations
// ---------------------------------------------------------------------------

/// Which band an operation is allowed to touch.
///
/// Section 5's `FreqBand`. **There is no `High` variant**, and that absence is the whole
/// texture guarantee expressed in the type system: the high band is pores, fine lines and the
/// grain of skin, and every operation in this phase either leaves it alone or transplants the
/// original back over its own output. An operator that could name the high band as its target
/// is one refactor away from smoothing it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum FreqBand {
    /// Form: the shape of the light on a face. Phase 19 shapes it; this phase reads it.
    Low,
    /// Blotches, flush, makeup mismatch and the neck/face step. The only band this phase moves.
    #[default]
    Mid,
}

impl FreqBand {
    /// Every band this phase can name.
    pub const ALL: [Self; 2] = [Self::Low, Self::Mid];

    /// Stable text for the catalog and the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Mid => "mid",
        }
    }

    /// Parse the stable text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.as_str() == text)
    }
}

impl fmt::Display for FreqBand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a blemish is removed.
///
/// Section 6.2 gives two methods and the boundary between them is size: a small mark is healed
/// from a nearby patch of the same person's skin under the same light, and a larger one needs
/// a generator. `Learned` is in the enum and **this build never emits it**, because the
/// inpainting network section 6.2 mentions does not exist here and an operator that cannot run
/// must not be recorded as though it did.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum InpaintMethod {
    /// Offset patch plus gradient blend: the healing-brush equivalent of section 6.2.
    #[default]
    Patch,
    /// A learned inpainting network constrained to the skin mask. Not shipped in this build.
    Learned,
}

impl InpaintMethod {
    /// Every method.
    pub const ALL: [Self; 2] = [Self::Patch, Self::Learned];

    /// Stable text for the catalog and the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Patch => "patch",
            Self::Learned => "learned",
        }
    }

    /// Parse the stable text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_str() == text)
    }
}

impl fmt::Display for InpaintMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The four things a retouch plan can ask for.
///
/// Section 5's own enum, with `box` spelled `area` because `box` is a reserved word and with
/// `ShineReduce` present and never emitted here - see
/// `docs/adr/ADR-0043-portrait-retouch-and-texture-protection.md` section 2.
///
/// A closed set, because these operations spend against a budget shared with phase 19 and a
/// budget with an open-ended list of spenders is not a budget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RetouchOp {
    /// One temporary mark, removed.
    Blemish {
        /// Where it is, normalised to the frame.
        area: Box2,
        /// How it was removed.
        method: InpaintMethod,
        /// How strongly, `0..1`.
        strength: f32,
    },
    /// One person's periorbital region, lifted and de-tinted.
    UnderEye {
        /// Whose.
        identity: IdentityId,
        /// Luminance lift in stops, bounded by [`MAX_UNDEREYE_LUMA_EV`].
        luma: f32,
        /// Chroma separation reduction, `0..1`, bounded by [`MAX_UNDEREYE_CHROMA`].
        chroma: f32,
    },
    /// Mid-frequency unevenness, calmed inside one mask.
    ToneEvening {
        /// The phase 18 region this runs inside.
        mask: MaskId,
        /// How strongly, `0..1`.
        strength: f32,
        /// Which band. Always [`FreqBand::Mid`]; the field is here because section 5 freezes it
        /// and because a future operator that named [`FreqBand::Low`] would be a *different*
        /// operation that a reader must be able to see.
        band: FreqBand,
    },
    /// Specular sheen, reduced. **Phase 19's operation.** This phase never emits one.
    ShineReduce {
        /// Where it is.
        area: Box2,
        /// How strongly, `0..1`.
        strength: f32,
    },
}

impl RetouchOp {
    /// The stable operator name, which is also the recipe's `retouch[].op` spelling.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Blemish { .. } => "blemish",
            Self::UnderEye { .. } => "under_eye",
            Self::ToneEvening { .. } => "tone_evening",
            Self::ShineReduce { .. } => "shine_reduce",
        }
    }

    /// Every operator name, in the order `docs/retouch.md` documents them.
    pub const NAMES: [&'static str; 4] = ["blemish", "under_eye", "tone_evening", "shine_reduce"];

    /// How strongly this operation runs, `0..1`.
    ///
    /// [`RetouchOp::UnderEye`] has two magnitudes rather than a strength, so it reports the
    /// larger of the two as fractions of their own caps - which is the number the budget
    /// spends against and the number the panel shows.
    #[must_use]
    pub fn strength(&self) -> f32 {
        match self {
            Self::Blemish { strength, .. }
            | Self::ToneEvening { strength, .. }
            | Self::ShineReduce { strength, .. } => *strength,
            Self::UnderEye { luma, chroma, .. } => ((luma.abs() / MAX_UNDEREYE_LUMA_EV)
                .max(chroma.abs() / MAX_UNDEREYE_CHROMA))
            .clamp(0.0, 1.0),
        }
    }

    /// Where on the frame this operation acts, when it names a rectangle.
    ///
    /// `None` for the two that act through a mask or a landmark region rather than a box, which
    /// is a real answer: the panel draws an evidence rectangle for a blemish and highlights a
    /// region for the others.
    #[must_use]
    pub fn area(&self) -> Option<Box2> {
        match self {
            Self::Blemish { area, .. } | Self::ShineReduce { area, .. } => Some(*area),
            _ => None,
        }
    }

    /// What is wrong with this operation, if anything.
    ///
    /// A sentence rather than an [`AuraError`], the split every phase since 09 has kept:
    /// `aura-core` owns the shape and `aura-retouch` owns the error registry.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        match self {
            Self::Blemish { area, strength, .. } => {
                if !(0.0..=1.0).contains(strength) {
                    return Some(format!(
                        "a blemish strength of {strength:.3} is outside 0..1"
                    ));
                }
                if area.w <= 0.0 || area.h <= 0.0 {
                    return Some("a blemish with no area".into());
                }
                None
            }
            Self::UnderEye { luma, chroma, .. } => {
                if luma.abs() > MAX_UNDEREYE_LUMA_EV + 1e-4 {
                    return Some(format!(
                        "an under-eye lift of {luma:.3} EV is above {MAX_UNDEREYE_LUMA_EV:.2}"
                    ));
                }
                if chroma.abs() > MAX_UNDEREYE_CHROMA + 1e-4 {
                    return Some(format!(
                        "an under-eye chroma move of {chroma:.3} is above \
                         {MAX_UNDEREYE_CHROMA:.2}"
                    ));
                }
                None
            }
            Self::ToneEvening { strength, band, .. } => {
                if !(0.0..=1.0).contains(strength) {
                    return Some(format!(
                        "an evening strength of {strength:.3} is outside 0..1"
                    ));
                }
                if *band != FreqBand::Mid {
                    return Some("tone evening may only move the mid band".into());
                }
                None
            }
            Self::ShineReduce { strength, .. } => {
                if !(0.0..=1.0).contains(strength) {
                    return Some(format!("a shine strength of {strength:.3} is outside 0..1"));
                }
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Protected features
// ---------------------------------------------------------------------------

/// What kind of permanent feature this is.
///
/// Section 2.1: "freckles, moles, birthmarks, scars, tattoos and dimples explicitly detected
/// and protected". Six kinds and one of them is absolute.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ProtectedKind {
    /// A mole or a beauty mark.
    #[default]
    Mole,
    /// Freckles, which arrive as a field rather than as one mark.
    Freckle,
    /// A birthmark.
    Birthmark,
    /// A scar.
    Scar,
    /// A tattoo. **Absolute**: see [`ProtectedKind::is_absolute`].
    Tattoo,
    /// A dimple, which is geometry rather than pigment and is protected from evening.
    Dimple,
}

impl ProtectedKind {
    /// Every kind, in the order `docs/retouch.md` documents them.
    pub const ALL: [Self; 6] = [
        Self::Mole,
        Self::Freckle,
        Self::Birthmark,
        Self::Scar,
        Self::Tattoo,
        Self::Dimple,
    ];

    /// How many kinds there are.
    pub const COUNT: usize = 6;

    /// Stable text for the catalog and the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mole => "mole",
            Self::Freckle => "freckle",
            Self::Birthmark => "birthmark",
            Self::Scar => "scar",
            Self::Tattoo => "tattoo",
            Self::Dimple => "dimple",
        }
    }

    /// Parse the stable text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == text)
    }

    /// True when no setting anywhere may unprotect this kind.
    ///
    /// Section 10.1 gates tattoo removal at **zero** per cent, and a zero implemented as a very
    /// small threshold is a promise that expires the next time somebody retrains a detector. So
    /// it is a property of the kind: [`RetouchService::set_protection`] refuses to clear one
    /// with `AURA-ML-5097`, and no strength multiplies it down.
    #[must_use]
    pub const fn is_absolute(self) -> bool {
        matches!(self, Self::Tattoo)
    }
}

impl fmt::Display for ProtectedKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a feature came to be protected.
///
/// Three sources with a strict order of authority: a person outranks a measurement outranks a
/// single-frame guess. Stored rather than derived, because "why is this protected" is the one
/// question a photographer asks of this list and "because you said so" is a different answer
/// from "because it was on her cheek for six hours".
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ProtectedSource {
    /// Seen at the same place on the same face across frames and hours. Section 6.1.
    #[default]
    CrossFrame,
    /// The permanent-feature classifier said so from one frame.
    Classifier,
    /// A photographer said so. Outranks both, and is never overwritten by a re-analysis.
    User,
}

impl ProtectedSource {
    /// Every source, weakest authority first.
    pub const ALL: [Self; 3] = [Self::Classifier, Self::CrossFrame, Self::User];

    /// Stable text for the catalog and the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CrossFrame => "cross_frame",
            Self::Classifier => "classifier",
            Self::User => "user",
        }
    }

    /// Parse the stable text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == text)
    }

    /// True when a re-analysis must leave this row alone.
    #[must_use]
    pub const fn is_user(self) -> bool {
        matches!(self, Self::User)
    }
}

impl fmt::Display for ProtectedSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One thing about a person that this product will not remove.
///
/// Section 5's `{ box, kind, identity }` plus the three fields that make it explicable - see
/// `docs/adr/ADR-0043-portrait-retouch-and-texture-protection.md` section 2. The rectangle is
/// in **face-normalised coordinates** rather than frame coordinates, which is what lets one row
/// protect the same mole in four hundred photographs: the origin is the midpoint between the
/// eyes, the x axis is the eye-to-eye line and the unit is the inter-ocular distance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProtectedFeature {
    /// Whose face it is on.
    pub identity: IdentityId,
    /// What kind of feature it is.
    pub kind: ProtectedKind,
    /// Where it sits on that face, in face-normalised coordinates.
    ///
    /// `x` and `y` may be negative: the origin is between the eyes, so a mark on the left cheek
    /// has a negative `x` under this convention.
    pub area: Box2,
    /// How sure the product is, `0..1`. One for [`ProtectedSource::User`].
    pub confidence: f32,
    /// How it came to be protected.
    pub source: ProtectedSource,
    /// How many frames it was measured on.
    pub frames: u32,
    /// The span those frames covered, in minutes.
    pub span_minutes: f32,
    /// The first photograph it was seen on, for the panel's evidence crop.
    pub first_seen: ImageId,
}

impl ProtectedFeature {
    /// How much of a candidate must overlap for this feature to veto it.
    ///
    /// A quarter. A blemish detector's box and a protect box are both approximate, and
    /// requiring a majority overlap would let a candidate box that is slightly offset from a
    /// mole remove half of it - which is the worst available outcome, being both a damaged
    /// permanent feature and an invisible one.
    pub const VETO_OVERLAP: f32 = 0.25;

    /// True when this feature forbids touching that region.
    ///
    /// **A veto, not a discount.** The candidate is dropped; nothing scales.
    #[must_use]
    pub fn vetoes(&self, candidate: Box2) -> bool {
        let x0 = self.area.x.max(candidate.x);
        let y0 = self.area.y.max(candidate.y);
        let x1 = (self.area.x + self.area.w).min(candidate.x + candidate.w);
        let y1 = (self.area.y + self.area.h).min(candidate.y + candidate.h);
        let overlap = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
        let area = (candidate.w * candidate.h).max(f32::EPSILON);
        overlap / area >= Self::VETO_OVERLAP
    }

    /// True when the cross-frame evidence meets both of section 6.1's thresholds.
    #[must_use]
    pub fn is_well_evidenced(&self) -> bool {
        self.frames >= PERMANENCE_MIN_FRAMES && self.span_minutes >= PERMANENCE_MIN_SPAN_MIN
    }

    /// True when nothing may clear this row.
    #[must_use]
    pub const fn is_absolute(&self) -> bool {
        self.kind.is_absolute()
    }
}

// ---------------------------------------------------------------------------
// The texture report
// ---------------------------------------------------------------------------

/// What the retouch did to the skin's own texture, measured rather than promised.
///
/// Section 5's `{ band_ratio, floor, passed }` plus the three fields that say *how* it passed -
/// see `docs/adr/ADR-0043-portrait-retouch-and-texture-protection.md` section 2. The
/// measurement is taken by running the plan through the real renderer, which is phase 16's rule
/// applied to texture instead of to colour: a guarantee about a pixel that is enforced on a
/// parameter is not a guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TextureReport {
    /// High-band skin energy after the retouch, over the same energy before it.
    ///
    /// One is a retouch that changed no texture at all. Above one is possible and is not a
    /// pass: it means the operation *added* high-frequency content, which is a patch seam.
    pub band_ratio: f32,
    /// The floor this frame was held to, from the preset.
    pub floor: f32,
    /// True when the stored plan is inside the floor.
    pub passed: bool,
    /// How many samples the ratio was measured over.
    ///
    /// A ratio measured over eleven skin samples is arithmetic rather than evidence, and the
    /// panel says so rather than showing a number to three decimal places.
    pub measured_on: u32,
    /// How many times the solver had to give up strength to get here.
    pub resolves: u8,
    /// True when the retouch was withdrawn entirely because the floor could not be met.
    ///
    /// When this is set the plan carries no operations at all: the frame ships unretouched, and
    /// [`RetouchCode::TextureFloorUnreachable`] says so.
    pub withdrawn: bool,
}

impl TextureReport {
    /// The report for a frame nothing was done to.
    pub const UNTOUCHED: Self = Self {
        band_ratio: 1.0,
        floor: TEXTURE_FLOOR,
        passed: true,
        measured_on: 0,
        resolves: 0,
        withdrawn: false,
    };

    /// The smallest number of samples a ratio may be believed over.
    ///
    /// Two hundred and fifty-six samples of skin. Below this the measurement is reported with
    /// `measured_on` and the plan's confidence is reduced rather than the guard being skipped -
    /// because a guard that switches itself off on hard frames is a guard that protects the
    /// easy ones.
    pub const MIN_SAMPLES: u32 = 256;

    /// True when the ratio was measured over enough skin to mean anything.
    #[must_use]
    pub const fn is_well_measured(&self) -> bool {
        self.measured_on >= Self::MIN_SAMPLES
    }

    /// What is wrong with this report, if anything.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if !(0.0..=4.0).contains(&self.band_ratio) {
            return Some(format!(
                "a band ratio of {:.3} is outside the measurable range",
                self.band_ratio
            ));
        }
        if self.floor < POLISHED_FLOOR - 1e-4 {
            return Some(format!(
                "a texture floor of {:.3} is below {POLISHED_FLOOR:.2}",
                self.floor
            ));
        }
        if self.passed && !self.withdrawn && self.band_ratio + 1e-4 < self.floor {
            return Some(format!(
                "the report passed at a band ratio of {:.3}, below its own floor of {:.3}",
                self.band_ratio, self.floor
            ));
        }
        if self.resolves > TEXTURE_MAX_RESOLVES {
            return Some(format!(
                "{} re-solves is above the {TEXTURE_MAX_RESOLVES} allowed",
                self.resolves
            ));
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why the retouch came out the way it did, as a closed set.
///
/// Twenty-six codes. `docs/retouch.md` is written against [`RetouchCode::ALL`] and a test
/// asserts every variant appears there.
///
/// **Thirteen of the twenty-six withdraw a claim rather than making one** - half, which is the
/// highest proportion of any phase so far and is the shape of the phase. Section 6.1's default
/// is conservative by design, so a product that could not say *I left that alone, and here is
/// why* would be a product whose restraint is indistinguishable from a bug.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum RetouchCode {
    // -- blemishes --------------------------------------------------------
    /// A temporary mark was removed.
    BlemishRemoved,
    /// Nothing temporary was found on this face. **A withdrawal.**
    #[default]
    NoBlemishFound,
    /// An anomaly's temporary probability was between the two floors, so it was left alone.
    /// **A withdrawal.**
    AnomalyUncertain,
    /// An anomaly was larger than [`MAX_BLEMISH_FRACTION`] and was left alone. **A
    /// withdrawal.**
    AnomalyTooLarge,
    /// No donor patch of the same skin under the same light was available. **A withdrawal.**
    NoDonorPatch,

    // -- permanent features ------------------------------------------------
    /// A permanent feature was protected.
    FeatureProtected,
    /// A permanent feature was protected because it was seen across frames and hours.
    ProtectedByCrossFrame,
    /// A permanent feature was protected because a photographer said so.
    ProtectedByUser,
    /// A tattoo was protected and cannot be unprotected. **A withdrawal, permanently.**
    TattooProtected,
    /// A candidate was dropped because it overlapped a protected feature. **A withdrawal.**
    VetoedByProtection,

    // -- under-eye ---------------------------------------------------------
    /// An under-eye region was lifted and de-tinted.
    UnderEyeCorrected,
    /// The under-eye correction stopped at its cap.
    UnderEyeCapped,
    /// There were no eye landmarks to work from. **A withdrawal.**
    NoEyeLandmarks,

    // -- tone evening -------------------------------------------------------
    /// Mid-frequency unevenness was calmed.
    ToneEvened,
    /// The skin was already even. **A withdrawal.**
    SkinAlreadyEven,
    /// Phase 19 had already evened this face, so this phase did not do it again. **A
    /// withdrawal.**
    AlreadyEvenedByLocal,

    // -- the texture guard ---------------------------------------------------
    /// The texture guard measured the retouch and it passed.
    TextureHeld,
    /// The strength was reduced until the texture floor was met.
    TextureResolved,
    /// The floor could not be met, so nothing was applied. **A withdrawal.**
    TextureFloorUnreachable,
    /// There was not enough skin to measure the ratio over. **A withdrawal.**
    TextureUnmeasurable,

    // -- strength and consistency -------------------------------------------
    /// The strength came from this person's own gallery-wide setting.
    IdentityStrength,
    /// This person is not identified, so the frame's own conservative default was used.
    /// **A withdrawal.**
    IdentityUnknown,
    /// The face was too small in the frame to retouch. **A withdrawal.**
    FaceTooSmall,
    /// This scene's preset limits what may be done.
    SceneLimited,

    // -- gating and governance -----------------------------------------------
    /// The skin mask was unavailable or too weak, so the operation was skipped. **A
    /// withdrawal.**
    MaskUnavailable,
    /// Retouching is switched off for this project, or the learned heads are untrained.
    /// **A withdrawal.**
    HeadUntrained,
}

impl RetouchCode {
    /// Every code, in the order `docs/retouch.md` documents them.
    pub const ALL: [Self; 26] = [
        Self::BlemishRemoved,
        Self::NoBlemishFound,
        Self::AnomalyUncertain,
        Self::AnomalyTooLarge,
        Self::NoDonorPatch,
        Self::FeatureProtected,
        Self::ProtectedByCrossFrame,
        Self::ProtectedByUser,
        Self::TattooProtected,
        Self::VetoedByProtection,
        Self::UnderEyeCorrected,
        Self::UnderEyeCapped,
        Self::NoEyeLandmarks,
        Self::ToneEvened,
        Self::SkinAlreadyEven,
        Self::AlreadyEvenedByLocal,
        Self::TextureHeld,
        Self::TextureResolved,
        Self::TextureFloorUnreachable,
        Self::TextureUnmeasurable,
        Self::IdentityStrength,
        Self::IdentityUnknown,
        Self::FaceTooSmall,
        Self::SceneLimited,
        Self::MaskUnavailable,
        Self::HeadUntrained,
    ];

    /// How many reason codes there are.
    pub const COUNT: usize = 26;

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BlemishRemoved => "blemish_removed",
            Self::NoBlemishFound => "no_blemish_found",
            Self::AnomalyUncertain => "anomaly_uncertain",
            Self::AnomalyTooLarge => "anomaly_too_large",
            Self::NoDonorPatch => "no_donor_patch",
            Self::FeatureProtected => "feature_protected",
            Self::ProtectedByCrossFrame => "protected_by_cross_frame",
            Self::ProtectedByUser => "protected_by_user",
            Self::TattooProtected => "tattoo_protected",
            Self::VetoedByProtection => "vetoed_by_protection",
            Self::UnderEyeCorrected => "under_eye_corrected",
            Self::UnderEyeCapped => "under_eye_capped",
            Self::NoEyeLandmarks => "no_eye_landmarks",
            Self::ToneEvened => "tone_evened",
            Self::SkinAlreadyEven => "skin_already_even",
            Self::AlreadyEvenedByLocal => "already_evened_by_local",
            Self::TextureHeld => "texture_held",
            Self::TextureResolved => "texture_resolved",
            Self::TextureFloorUnreachable => "texture_floor_unreachable",
            Self::TextureUnmeasurable => "texture_unmeasurable",
            Self::IdentityStrength => "identity_strength",
            Self::IdentityUnknown => "identity_unknown",
            Self::FaceTooSmall => "face_too_small",
            Self::SceneLimited => "scene_limited",
            Self::MaskUnavailable => "mask_unavailable",
            Self::HeadUntrained => "head_untrained",
        }
    }

    /// Parse the stable slug.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == text)
    }

    /// True when this code describes something the product declined to do.
    ///
    /// Thirteen of twenty-six. Used by the panel to group the reasons a photographer reads as
    /// "what AURA left alone", which on a conservative retoucher is most of them.
    #[must_use]
    pub const fn is_withdrawal(self) -> bool {
        matches!(
            self,
            Self::NoBlemishFound
                | Self::AnomalyUncertain
                | Self::AnomalyTooLarge
                | Self::NoDonorPatch
                | Self::TattooProtected
                | Self::VetoedByProtection
                | Self::NoEyeLandmarks
                | Self::SkinAlreadyEven
                | Self::AlreadyEvenedByLocal
                | Self::TextureFloorUnreachable
                | Self::TextureUnmeasurable
                | Self::IdentityUnknown
                | Self::FaceTooSmall
        )
    }

    /// The sentence a photographer reads, in the product's voice.
    ///
    /// Not a translation of the slug. Section 9 gives DOC "document presets, protected features
    /// and the texture guarantee", and the hardest of those to explain is why a mark is still
    /// there - so the withdrawal sentences say what was protected and none of them apologise.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::BlemishRemoved => {
                "a temporary mark was removed and the skin around it kept its own texture"
            }
            Self::NoBlemishFound => "there was nothing temporary on this skin to remove",
            Self::AnomalyUncertain => {
                "AURA was not sure whether this mark was temporary or part of how this person \
                 looks, so it left it alone"
            }
            Self::AnomalyTooLarge => {
                "this mark is too large to be a blemish, so AURA left it for you to decide about"
            }
            Self::NoDonorPatch => {
                "there was no nearby skin under the same light to borrow from, so this mark was \
                 left alone"
            }
            Self::FeatureProtected => "this is part of how this person looks, so it is protected",
            Self::ProtectedByCrossFrame => {
                "this mark is in the same place on this person's face through the day, so AURA \
                 treats it as part of them"
            }
            Self::ProtectedByUser => "you asked AURA to keep this",
            Self::TattooProtected => "AURA never alters tattoos",
            Self::VetoedByProtection => {
                "this was next to something AURA protects, so it was left alone"
            }
            Self::UnderEyeCorrected => "the shadow under the eyes was lightened a little",
            Self::UnderEyeCapped => {
                "AURA stopped lightening under the eyes before it would start to look retouched"
            }
            Self::NoEyeLandmarks => {
                "AURA could not find the eyes here, so it did no under-eye work"
            }
            Self::ToneEvened => "uneven patches in the skin tone were calmed, without smoothing",
            Self::SkinAlreadyEven => "this skin was already even, so nothing was changed",
            Self::AlreadyEvenedByLocal => {
                "this face had already been evened out earlier in the edit, so AURA did not do it \
                 twice"
            }
            Self::TextureHeld => "the skin kept its own texture through the retouch",
            Self::TextureResolved => {
                "AURA used a gentler retouch here so that the skin kept its own texture"
            }
            Self::TextureFloorUnreachable => {
                "AURA could not retouch this photograph without losing skin texture, so it left \
                 it alone"
            }
            Self::TextureUnmeasurable => {
                "there was not enough skin visible here to check the texture, so AURA was cautious"
            }
            Self::IdentityStrength => {
                "this person is retouched the same way everywhere in this wedding"
            }
            Self::IdentityUnknown => {
                "AURA does not know who this is yet, so it used its gentlest settings"
            }
            Self::FaceTooSmall => "this face is too small in the frame to retouch",
            Self::SceneLimited => "this kind of photograph is retouched more gently",
            Self::MaskUnavailable => {
                "AURA was not sure enough where the skin is here, so it did not retouch it"
            }
            Self::HeadUntrained => {
                "AURA is using its measured retouching rather than a learned model in this build"
            }
        }
    }
}

impl fmt::Display for RetouchCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One reason, with the region it is about.
///
/// The same shape as [`crate::contract::local::LocalReason`] and
/// [`crate::contract::tone::ToneReason`]: the code is what is stored, the sentence is what is
/// shown, and the weight is the contribution to confidence - negative for doubt. Phase 09's
/// rule, sixth phase running: reasons store their code rather than their sentence, because a
/// stored sentence is copy a release can change and a catalog full of English cannot be
/// translated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RetouchReason {
    /// Which code.
    pub code: RetouchCode,
    /// The sentence, in the product's voice.
    pub text: String,
    /// The contribution to confidence. Negative is doubt.
    pub weight: f32,
    /// The region it is about, in frame coordinates, when it is about one.
    #[serde(default)]
    pub evidence: Option<Box2>,
}

impl RetouchReason {
    /// A reason about the whole frame.
    #[must_use]
    pub fn frame(code: RetouchCode, text: impl Into<String>, weight: f32) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: None,
        }
    }

    /// A reason about one region.
    #[must_use]
    pub fn at(code: RetouchCode, text: impl Into<String>, weight: f32, evidence: Box2) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: Some(evidence),
        }
    }

    /// A reason carrying the code's own sentence.
    #[must_use]
    pub fn plain(code: RetouchCode, weight: f32) -> Self {
        Self::frame(code, code.user_text(), weight)
    }

    /// A reason carrying the code's own sentence, about one region.
    #[must_use]
    pub fn plain_at(code: RetouchCode, weight: f32, evidence: Box2) -> Self {
        Self::at(code, code.user_text(), weight, evidence)
    }

    /// True when this reason cost the plan confidence.
    #[must_use]
    pub fn is_doubt(&self) -> bool {
        self.weight < 0.0
    }
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// Everything phase 20 decided about one photograph's skin.
///
/// PHASE-20 section 5's frozen shape, with the additions this module's header and
/// `docs/adr/ADR-0043-portrait-retouch-and-texture-protection.md` section 2 argue for: the
/// scene it was decided under, the three version columns, and the two flags that record a
/// photographer's involvement.
///
/// **It is not an edit.** The operations here reach the pixels only through
/// `aura_recipe::schema::merge` writing `recipe.retouch[]`, which is phase 14's rule for the
/// fourth phase running. A plan with `user_edited = true` still carries AURA's own numbers, so
/// phase 30's learning loop can read the disagreement.
#[derive(Debug, Clone, PartialEq)]
pub struct RetouchPlan {
    /// The photograph.
    pub image_id: ImageId,
    /// What was done, in a deterministic order.
    pub ops: Vec<RetouchOp>,
    /// The gallery-constant strength for each identity in this frame.
    ///
    /// A `BTreeMap` rather than the hash map section 5 spells: a hash map is refused by
    /// `scripts/check-banned.sh`, because its iteration order is seeded per process and it
    /// decides the bytes of a serialised recipe. See the ADR, section 2.
    pub per_identity_strength: BTreeMap<IdentityId, f32>,
    /// Everything about the people in this frame that must not be removed.
    pub protected: Vec<ProtectedFeature>,
    /// What the retouch did to the skin's own texture.
    pub texture_report: TextureReport,
    /// Which preset this frame was retouched under.
    pub preset: RetouchPreset,
    /// Why. Never empty; invariant 2.
    pub reasons: Vec<RetouchReason>,
    /// How much the plan trusts itself, `0..1`.
    pub confidence: f32,
    /// The scene this was decided under. Invariant 7.
    pub scene: SceneId,
    /// How much of the shared per-image perceptual allowance this plan spent, `0..1`.
    ///
    /// The allowance is phase 19's [`crate::contract::local::PERCEPTUAL_BUDGET`], **shared
    /// rather than duplicated**: six local operations and a retouch that each stay inside their
    /// own budget still add up to a photograph that looks worked on.
    pub budget_used: f32,
    /// True when a photographer changed the strength or the preset by hand.
    pub user_edited: bool,
    /// True when a photographer has looked at this plan and agreed.
    pub reviewed: bool,
    /// Which learned heads produced the detections.
    pub model_ver: u16,
    /// Which build's arithmetic produced the decisions.
    pub analysis_ver: u16,
    /// Which preset file the strengths and floors came from.
    pub preset_ver: u16,
}

impl RetouchPlan {
    /// A plan that does nothing, for a frame that needed nothing.
    ///
    /// Still carries a reason, because a plan with no reason is a bug rather than an empty plan.
    #[must_use]
    pub fn nothing(image_id: ImageId, scene: SceneId, reason: RetouchReason) -> Self {
        Self {
            image_id,
            ops: Vec::new(),
            per_identity_strength: BTreeMap::new(),
            protected: Vec::new(),
            texture_report: TextureReport::UNTOUCHED,
            preset: RetouchPreset::Natural,
            reasons: vec![reason],
            confidence: 1.0,
            scene,
            budget_used: 0.0,
            user_edited: false,
            reviewed: false,
            model_ver: 0,
            analysis_ver: 0,
            preset_ver: 0,
        }
    }

    /// True when this plan changes no pixel.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.ops.is_empty()
    }

    /// How many operations of one kind this plan carries.
    #[must_use]
    pub fn count_of(&self, name: &str) -> usize {
        self.ops.iter().filter(|op| op.as_str() == name).count()
    }

    /// The strength this plan used for one person.
    #[must_use]
    pub fn strength_for(&self, identity: &IdentityId) -> Option<f32> {
        self.per_identity_strength.get(identity).copied()
    }

    /// True when a region is protected on this frame.
    #[must_use]
    pub fn is_protected(&self, area: Box2) -> bool {
        self.protected.iter().any(|f| f.vetoes(area))
    }

    /// True when the frame is worth a photographer's attention.
    #[must_use]
    pub fn needs_review(&self) -> bool {
        !self.reviewed && !self.user_edited && self.confidence < REVIEW_BELOW
    }

    /// What guarantee this plan breaks, if any.
    ///
    /// Seven checks, and every one is an acceptance criterion rather than a type error: there is
    /// at least one reason, every operation is inside its own bounds, no operation touches a
    /// protected feature, the plan does not carry phase 19's shine operation, the texture report
    /// is coherent, a withdrawn retouch really is empty, and the plan is inside the shared
    /// allowance. They live here so the solver, the store, the IPC layer and the eval harness
    /// all refuse the same frames. `aura_retouch::guard` turns a `Some` here into
    /// `AURA-ML-5098`.
    #[must_use]
    pub fn broken_guarantee(&self) -> Option<String> {
        if self.reasons.is_empty() {
            return Some("a plan with no reason".into());
        }
        if self.ops.len() > MAX_OPS {
            return Some(format!("{} operations is above {MAX_OPS}", self.ops.len()));
        }
        if !(0.0..=1.0).contains(&self.budget_used) {
            return Some(format!(
                "budget used {:.3} is outside 0..1",
                self.budget_used
            ));
        }
        for op in &self.ops {
            if let Some(problem) = op.problem() {
                return Some(problem);
            }
            if matches!(op, RetouchOp::ShineReduce { .. }) {
                // Phase 19's operation. See the ADR, section 2: two phases reducing the same
                // hot spot is a forehead brought down twice, and the boundary is enforced here
                // rather than remembered in a comment.
                return Some("a retouch plan may not carry phase 19's shine reduction".into());
            }
            if let Some(area) = op.area() {
                if self.is_protected(area) {
                    return Some(
                        "an operation overlaps a protected feature, which is never permitted"
                            .into(),
                    );
                }
            }
        }
        if let Some(problem) = self.texture_report.problem() {
            return Some(problem);
        }
        if self.texture_report.withdrawn && !self.ops.is_empty() {
            return Some("a withdrawn retouch still carries operations".into());
        }
        None
    }

    /// True when this plan keeps every guarantee the phase makes.
    #[must_use]
    pub fn is_sound(&self) -> bool {
        self.broken_guarantee().is_none()
    }
}

// ---------------------------------------------------------------------------
// The outline
// ---------------------------------------------------------------------------

/// What a project's retouch pass covered and what it found.
///
/// Phase 05's rule, inherited for the fourteenth time: report coverage when you report a
/// result, and say what the denominator is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RetouchOutline {
    /// Photographs with a plan.
    pub planned: u32,
    /// Photographs in the project.
    pub photos: u32,
    /// Fraction of the project with a plan, `0..1`.
    ///
    /// **The denominator is every photograph**, as phases 09 to 15's are - and unlike phase
    /// 18's, which counts selected frames. A retouch plan over a frame nobody will deliver is
    /// wasted work rather than a missing answer, but the pass is driven from phase 12's keepers
    /// and the caller needs to see the whole project's shape to know that.
    pub coverage: f32,
    /// Fraction of *planned* frames where at least one operation ran, `0..1`.
    ///
    /// The number that matters when it is low. A wedding at 100 % coverage and 2 % acted-on has
    /// been walked past rather than retouched, and because this phase's work is meant to be
    /// invisible that is otherwise very hard to notice.
    pub acted_on: f32,
    /// Fraction of planned frames whose skin mask arrived, `0..1`.
    ///
    /// Zero on a build with no mask generator wired in, which is the honest reading of such a
    /// build.
    pub mask_covered: f32,
    /// Faces seen across the pass, whatever their size.
    pub faces_seen: u32,
    /// Faces large enough to retouch.
    pub faces_retouched: u32,
    /// Blemishes removed.
    pub blemishes_removed: u32,
    /// Anomalies left alone because they were uncertain.
    pub anomalies_left: u32,
    /// Features protected, by kind, in [`ProtectedKind::ALL`] order.
    ///
    /// Section 11's `retouch.protected {kind, count}`.
    pub protected_histogram: [u32; ProtectedKind::COUNT],
    /// Frames where the texture guard reduced a strength.
    pub texture_resolved: u32,
    /// Frames where the retouch was withdrawn because the floor could not be met.
    pub texture_withdrawn: u32,
    /// Mean band ratio over frames where one was measured.
    ///
    /// Section 11's `retouch.texture_guard {mean_band_ratio}`.
    pub mean_band_ratio: f32,
    /// Mean per-identity strength over the project.
    ///
    /// Section 11's `retouch.applied {mean_strength}`.
    pub mean_strength: f32,
    /// The largest spread of one identity's strength across the gallery, `0..1`.
    ///
    /// Section 10.1's cross-frame consistency gate, as a stored number rather than a test
    /// fixture. Zero by construction while strength is a gallery constant, and the outline
    /// reports it anyway so that a future change that made it per-frame would be visible in the
    /// product rather than only in a diff.
    pub max_identity_spread: f32,
    /// How many frames each preset was used on, in [`RetouchPreset::ALL`] order.
    pub preset_histogram: [u32; RetouchPreset::COUNT],
    /// Frames below [`REVIEW_BELOW`] that nobody has reviewed.
    pub needs_review: u32,
    /// Frames a photographer has set by hand.
    pub user_edited: u32,
    /// Scenes with no preset row, by slug.
    pub unpreset_scenes: Vec<String>,
    /// Which learned heads produced the detections.
    pub model_ver: u16,
    /// Which build's arithmetic produced the decisions.
    pub analysis_ver: u16,
    /// Which preset file the strengths came from.
    pub preset_ver: u16,
}

impl RetouchOutline {
    /// True when nothing has been planned.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.planned == 0
    }

    /// True when every stored plan met its texture floor.
    ///
    /// The product's own version of section 0's headline KPI, asked of the catalog rather than
    /// of a test fixture.
    #[must_use]
    pub const fn texture_guarantee_held(&self) -> bool {
        self.texture_withdrawn == 0
    }
}

// ---------------------------------------------------------------------------
// Overrides
// ---------------------------------------------------------------------------

/// What the photographer set instead.
///
/// Every field optional and independent, the shape phases 15 and 19 both use: somebody who
/// changed the preset has not made a claim about one person's strength, and an override
/// carrying both would silently freeze the one they did not touch.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetouchOverride {
    /// The preset for this photograph.
    #[serde(default)]
    pub preset: Option<RetouchPreset>,
    /// A strength for one person, `0..1`, applied across the gallery.
    ///
    /// **Gallery-wide by design.** Setting one person's strength on one frame and not on the
    /// rest is how a gallery ends up with a bride whose skin changes character between the
    /// ceremony and the reception, which is the failure section 6.4 exists to prevent.
    #[serde(default)]
    pub identity_strength: Option<(IdentityId, f32)>,
}

impl RetouchOverride {
    /// An override that sets only the preset.
    #[must_use]
    pub fn preset(preset: RetouchPreset) -> Self {
        Self {
            preset: Some(preset),
            identity_strength: None,
        }
    }

    /// An override that sets one person's gallery strength.
    #[must_use]
    pub fn strength(identity: IdentityId, strength: f32) -> Self {
        Self {
            preset: None,
            identity_strength: Some((identity, strength)),
        }
    }

    /// True when this override sets nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.preset.is_none() && self.identity_strength.is_none()
    }

    /// What is wrong with this override, if anything.
    ///
    /// `aura_retouch::guard` turns a `Some` here into `AURA-ML-5097`.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if self.is_empty() {
            return Some("the override sets nothing".into());
        }
        if let Some((_, strength)) = &self.identity_strength {
            if !(0.0..=1.0).contains(strength) {
                return Some(format!("a strength of {strength:.3} is outside 0..1"));
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask what was done to somebody's skin.
///
/// Sixteenth service of its kind, and it carries the rule for the sixteenth time: **no phase
/// may keep its own retoucher, its own protect set or its own idea of what a permanent feature
/// is.** Phase 21 retouches hair, teeth, eyes and clothing and must not re-smooth skin this
/// phase already worked on; phase 25 normalises a gallery whose frames were each retouched;
/// phase 27 has to be able to say why a face looks worked on. Two answers to "what did we do to
/// her skin" is a delivery in which the album and the gallery disagree about somebody's face.
pub trait RetouchService: Send + Sync + fmt::Debug {
    /// What a project's pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<RetouchOutline>;

    /// One photograph's plan, or `None` when it has not been planned.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plan cannot be read.
    fn of_image(&self, image: ImageId) -> AuraResult<Option<RetouchPlan>>;

    /// Everything protected on one person, across the whole project.
    ///
    /// Read per person rather than per frame, because that is the shape the panel needs and
    /// because a protect set is a property of a person rather than of a photograph - which is
    /// the point of storing it in face-normalised coordinates.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the features cannot be read.
    fn protected(&self, identity: IdentityId) -> AuraResult<Vec<ProtectedFeature>>;

    /// The frames whose retouch is worth a photographer's attention, weakest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>>;

    /// The gallery-constant strength for every identity in a project.
    ///
    /// Read once per panel rather than per frame, for the reason
    /// [`crate::contract::tone::ToneService::skin_loci`] gives.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the strengths cannot be read.
    fn identity_strengths(&self, project: ProjectId) -> AuraResult<BTreeMap<IdentityId, f32>>;

    /// Record that the photographer has looked at this plan and agrees.
    ///
    /// Sets [`RetouchPlan::reviewed`] and does not set [`RetouchPlan::user_edited`]: accepting a
    /// suggestion is not authoring one, and phase 30's learning loop needs to tell them apart.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5097` when the photograph has no plan.
    fn accept(&self, image: ImageId) -> Result<(), AuraError>;

    /// Record what the photographer set instead.
    ///
    /// Sets [`RetouchPlan::user_edited`] and is not undone by a re-analysis: the check is inside
    /// the statement that would overwrite the row, exactly as `identities.user_locked`,
    /// `segments.user_locked`, `moments.user_locked`, `image_integrity.user_reviewed`,
    /// `image_composition.dismissed`, `image_tone_estimate.user_edited`, `masks.user_edited` and
    /// `local_light_plan.user_edited` are.
    ///
    /// **This records the disagreement; it does not move a pixel.** The pixels move when the
    /// caller writes the same values through `aura_recipe::schema::merge`.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5097` when the photograph has no plan, when the override is empty, or when a
    /// strength is outside `0..1`.
    fn set_override(&self, image: ImageId, values: RetouchOverride) -> Result<(), AuraError>;

    /// Add or clear one protected feature.
    ///
    /// `protect = false` clears a feature a photographer disagrees with - and **cannot clear an
    /// absolute one**. [`ProtectedKind::is_absolute`] is true for a tattoo, section 10.1 gates
    /// tattoo removal at zero per cent, and a promise a setting can retract is not a promise.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5097` when the feature is absolute, when the identity is unknown, or when the
    /// rectangle is empty.
    fn set_protection(&self, feature: ProtectedFeature, protect: bool) -> Result<(), AuraError>;
}
