//! FROZEN CONTRACT. What a photograph should be exposed to, and what colour the light
//! was: the exposure offset, the white balance, the illuminants behind them and the skin
//! constraint that bounds both.
//!
//! PHASE-15 section 5 freezes `ToneEstimate` before any solver exists. The file is in
//! `aura-core` rather than in `aura-brain-photo` for the reason
//! [`crate::contract::integrity`], [`crate::contract::composition`] and
//! [`crate::contract::emotion`] are: the phases that consume a tone decision are 16 (tone
//! curves), 17 (style), 18 (local masks and local white balance), 25 (gallery
//! normalisation), 26 (multi-camera matching) and 27 (QC), and none of them needs the
//! chromaticity solver, the hypothesis generators or the histogram reader.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **Exposure here is subject-referred, not frame-referred.** Section 1: "the bride's face
//! is the anchor, not the mean luminance of a dark reception hall". Every field that looks
//! like it is about the whole photograph - [`ToneEstimate::subject_luma_before`],
//! [`ToneEstimate::subject_luma_target`] - is about the faces in it, weighted by phase 06's
//! prominence. When there are no faces the frame falls back to a scene model and
//! [`ToneEstimate::face_anchored`] says so, because a reception exposed on its mean
//! luminance and a reception exposed on the bride's cheek are two different photographs and
//! only one of them is what a photographer asked for.
//!
//! ## The second thing: a skin target is measured, never assumed
//!
//! Section 6.3. The plausible chromaticity region for a person's skin - [`SkinLocus`] - is
//! accumulated **from that wedding's own best-lit frames of that person**, and there is no
//! field anywhere in this contract for an "ideal" skin value. That is not a courtesy. A
//! fixed target is how an editor lightens dark skin toward a Eurocentric reference while
//! believing it is correcting a colour cast, and a shape with nowhere to put such a
//! constant cannot do it. The constraint is a *hard* one in the solve rather than a
//! post-hoc adjustment, which is why [`SkinLocus`] is in the frozen contract and not an
//! implementation detail of the solver.
//!
//! ## Eight spellings differ from the phase document
//!
//! All eight are recorded in
//! `docs/adr/ADR-0031-exposure-white-balance-and-skin.md` section 2.
//!
//! * **`ImageId` is `PhotoId`**, aliased rather than duplicated, exactly as the scene,
//!   people, moment, integrity, emotion, composition and cull contracts already do it.
//! * **`alternatives: Vec<(f32, f32, f32)>` is `Vec<ToneAlternative>`.** A bare triple has
//!   no room for *why* a runner-up lost, and section 13's "every value overridable" is only
//!   useful in a panel that can say what the second-best answer was and what it cost.
//! * **`illuminants[].cct` is [`Illuminant::cct_k`], and every illuminant carries a
//!   chromaticity as well.** A correlated colour temperature is a projection of a
//!   chromaticity onto the Planckian locus, and a fluorescent tube or a cheap LED is
//!   nowhere near that locus - so a CCT alone cannot describe the light most receptions are
//!   lit by. [`Illuminant::uv`] is the estimate; the CCT and tint are what a slider shows.
//! * **`reasons: Vec<Reason>` carries a typed [`ToneCode`]** rather than a free string, the
//!   same choice phases 09 to 13 made, so `docs/mixed-lighting.md` is a finishable job and
//!   a stored row costs a code rather than a sentence.
//! * **[`SkinLocus`] is not in section 5.** Section 6.2 and 6.3 both require it -
//!   "accumulated across the wedding, so a single bad frame cannot mislead" - and a
//!   per-identity constraint that lives only inside one solver is a constraint the next
//!   phase re-derives differently.
//! * **[`ReferenceFrame`] is not in section 5 either.** Section 6.4 hands 3-5 anchors per
//!   segment forward to phase 25, and a handoff with no type is a handoff nobody can test.
//! * **[`ToneOutline`] is the coverage carrier**, inherited from phase 05 for the ninth
//!   time, with this phase's refinement written at [`ToneOutline::face_anchored`].
//! * **[`ToneService`] is not in section 5.** Six later phases consume a tone decision and
//!   a contract with no entry point makes each of them find its own way in.
//!
//! ## Three version fields, because they invalidate three different things
//!
//! [`ToneEstimate::model_ver`] invalidates the learned white-balance prediction and the
//! faceless scene exposure, because both come from a head. [`ToneEstimate::analysis_ver`]
//! invalidates the statistics, the neutral detection, the hypothesis scoring and the solve,
//! because those come from this build's arithmetic. [`ToneEstimate::targets_ver`]
//! invalidates every number that was compared against a **band** - the subject luminance
//! target, the clipping tolerance, the mood policy - because a scene's bands are a product
//! decision a release can move without changing a line of solver code.
//!
//! Comparing two rows across any of them returns a plausible number that means nothing.
//! `AURA-ML-5060` exists so that comparison never happens silently. Ninth phase, ninth
//! version-drift code.
//!
//! Changing anything in this file requires an ADR.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{IdentityId, ProjectId, SegmentId};
use crate::contract::integrity::CropRect;
use crate::contract::scene::SceneId;

pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// Ranges, tolerances and gates
// ---------------------------------------------------------------------------

/// The exposure agreement gate, in stops.
///
/// Section 10.1: "exposure within +/-0.15 EV of expert on >= 85 % of frames". It lives in
/// the contract rather than in the eval harness because the review queue uses the same
/// number to decide whether an estimate is worth a photographer's attention, and two copies
/// of 0.15 is one chance to write one of them as 0.2.
pub const EXPOSURE_TOLERANCE_EV: f32 = 0.15;

/// The white-balance agreement gate, in kelvin.
///
/// Section 10.1: "WB within 200 K and 4 tint units on >= 85 %".
pub const WB_TOLERANCE_K: f32 = 200.0;

/// The white-balance agreement gate, in tint units.
pub const WB_TOLERANCE_TINT: f32 = 4.0;

/// The mean skin error a shipped solver may have, in dE00.
///
/// Section 10.1's fairness gate, first half.
pub const SKIN_DE00_CEILING: f32 = 3.0;

/// The most the *worst* skin-tone bucket may exceed the best, in dE00.
///
/// Section 10.1's fairness gate, second half, and section 6.3's shipping rule: "the model
/// cannot ship if any group is more than 1.0 dE00 worse than the best group". The two are
/// deliberately separate numbers - a solver can be uniformly mediocre or selectively good,
/// and only the second is a fairness failure.
pub const SKIN_DE00_SPREAD: f32 = 1.0;

/// Below this white-balance confidence a frame goes to the review queue.
///
/// In the contract because the panel, the store, the outline and the eval harness all ask
/// the same question. Chosen at the point where the winning hypothesis stopped being
/// clearly better than the runner-up rather than at a round number; ADR-0031 section 5 has
/// the derivation.
pub const REVIEW_WB_BELOW: f32 = 0.55;

/// The fewest reference frames a segment must have for phase 25 to normalise against it.
///
/// Section 6.4: "select 3-5 reference frames", and section 10.1 makes three a gate.
pub const MIN_REFERENCE_FRAMES: usize = 3;

/// The most reference frames a segment carries.
pub const MAX_REFERENCE_FRAMES: usize = 5;

/// The fewest skin patches an identity needs before its locus constrains anything.
///
/// Section 6.2: "accumulated across the wedding, so a single bad frame cannot mislead".
/// Five, because a locus fitted to fewer is a locus fitted to one lighting condition, and a
/// constraint derived from one tungsten frame would reject every daylight solution for that
/// person.
pub const MIN_LOCUS_SAMPLES: u32 = 5;

/// The kelvin range a solution may take. The recipe's own range, restated.
pub const KELVIN_RANGE: (f32, f32) = (2000.0, 50_000.0);

/// The tint range a solution may take. The recipe's own range, restated.
pub const TINT_RANGE: (f32, f32) = (-150.0, 150.0);

/// The exposure range a solution may take, in stops. The recipe's own range, restated.
pub const EXPOSURE_RANGE: (f32, f32) = (-5.0, 5.0);

// ---------------------------------------------------------------------------
// Illuminants
// ---------------------------------------------------------------------------

/// What kind of light this is.
///
/// A closed set, because an illuminant kind nothing recognises is an illuminant no policy
/// can be written about - and this phase's two hardest cases, [`IlluminantKind::Stage`] and
/// [`IlluminantKind::Fluorescent`], are exactly the ones a CCT alone cannot tell apart from
/// a badly white-balanced daylight frame.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum IlluminantKind {
    /// Nothing identified it. The default, and the answer that claims the least.
    #[default]
    Unknown,
    /// Direct sun or open daylight, roughly 5000-6500 K on the locus.
    Daylight,
    /// Open shade or a blue hour sky, above 7000 K on the locus.
    Shade,
    /// Overcast, between daylight and shade.
    Cloudy,
    /// Incandescent or halogen, below 3500 K and close to the locus.
    ///
    /// The reception light this phase exists for. Section 2.1 names "tungsten receptions"
    /// first among the hard cases.
    Tungsten,
    /// A gas-discharge tube: near a CCT but visibly off the locus in green.
    Fluorescent,
    /// A cheap white LED: near a CCT, off the locus, and with a spectrum that makes skin
    /// read differently from a black body at the same temperature.
    Led,
    /// The camera's own flash, or a strobe. Close to daylight and *local to the subject*,
    /// which is what makes it the illuminant that usually governs.
    Flash,
    /// A candle or a diya. Below 2200 K, small, and almost never something to neutralise.
    Candle,
    /// A saturated coloured source: an uplighter, a gel, a dance-floor wash.
    ///
    /// **The one kind this phase deliberately does not correct.** Section 2.1: "a purple
    /// dance floor should stay purple".
    Stage,
}

impl IlluminantKind {
    /// Every kind, in the order `docs/mixed-lighting.md` documents them.
    pub const ALL: [Self; 10] = [
        Self::Unknown,
        Self::Daylight,
        Self::Shade,
        Self::Cloudy,
        Self::Tungsten,
        Self::Fluorescent,
        Self::Led,
        Self::Flash,
        Self::Candle,
        Self::Stage,
    ];

    /// How many kinds the vocabulary names.
    pub const COUNT: usize = 10;

    /// The stable slug, stored and sent on the wire. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Daylight => "daylight",
            Self::Shade => "shade",
            Self::Cloudy => "cloudy",
            Self::Tungsten => "tungsten",
            Self::Fluorescent => "fluorescent",
            Self::Led => "led",
            Self::Flash => "flash",
            Self::Candle => "candle",
            Self::Stage => "stage",
        }
    }

    /// The words a photographer reads.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Unknown => "an unidentified light",
            Self::Daylight => "daylight",
            Self::Shade => "open shade",
            Self::Cloudy => "overcast daylight",
            Self::Tungsten => "tungsten light",
            Self::Fluorescent => "fluorescent light",
            Self::Led => "LED light",
            Self::Flash => "flash",
            Self::Candle => "candlelight",
            Self::Stage => "coloured stage lighting",
        }
    }

    /// Parse the stored slug. Unknown text is [`IlluminantKind::Unknown`].
    #[must_use]
    pub fn from_str_or_unknown(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == text)
            .unwrap_or(Self::Unknown)
    }

    /// True when neutralising this light would destroy what the photograph is of.
    ///
    /// Section 2.1's preserve-mood policy, as a property of the *kind* rather than as a
    /// threshold somebody remembers. Candlelight and stage light are both intentional and
    /// both are what the frame is about; correcting either to a neutral grey is the failure
    /// section 12 names second.
    #[must_use]
    pub const fn is_intentional(self) -> bool {
        matches!(self, Self::Stage | Self::Candle)
    }

    /// True when this light is local to the subject rather than to the room.
    ///
    /// A flash governs the face it is pointed at whatever the room is doing, which is the
    /// whole of section 6.2's "choose the illuminant governing the subject" in the one case
    /// where the answer is not a judgement call.
    #[must_use]
    pub const fn is_subject_local(self) -> bool {
        matches!(self, Self::Flash)
    }
}

impl fmt::Display for IlluminantKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where an illuminant hypothesis came from.
///
/// Carried for the reason [`crate::contract::composition::HorizonSource`] is: a stored
/// estimate that does not say how it was made cannot be audited when it turns out to be
/// wrong, and the four generators section 6.2 names fail in four different ways.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum HypothesisSource {
    /// The camera's own as-shot white balance. The floor, present on every frame.
    #[default]
    CameraAsShot,
    /// The frame averages to grey. Wrong whenever the frame is mostly one colour, which at
    /// a wedding it very often is.
    GreyWorld,
    /// The brightest non-specular pixels are white. Wrong whenever the brightest thing is a
    /// coloured light.
    WhitePatch,
    /// A learned prediction from a thumbnail.
    Learned,
    /// A region identified as a known neutral: a dress, a tablecloth, printed paper.
    KnownNeutral,
    /// The chromaticity that puts known skin closest to its own measured locus.
    Skin,
    /// A flash signature: a bright subject-local falloff against a warmer room.
    Flash,
}

impl HypothesisSource {
    /// Every source, in the order the generators run.
    pub const ALL: [Self; 7] = [
        Self::CameraAsShot,
        Self::GreyWorld,
        Self::WhitePatch,
        Self::Learned,
        Self::KnownNeutral,
        Self::Skin,
        Self::Flash,
    ];

    /// How many generators there are.
    pub const COUNT: usize = 7;

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CameraAsShot => "camera_as_shot",
            Self::GreyWorld => "grey_world",
            Self::WhitePatch => "white_patch",
            Self::Learned => "learned",
            Self::KnownNeutral => "known_neutral",
            Self::Skin => "skin",
            Self::Flash => "flash",
        }
    }

    /// Parse the stored text. Unknown values read as [`HypothesisSource::CameraAsShot`].
    #[must_use]
    pub fn from_str_or_as_shot(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|source| source.as_str() == text)
            .unwrap_or(Self::CameraAsShot)
    }

    /// True when this source read a learned head.
    ///
    /// What decides whether `model_ver` invalidates a stored estimate, asked in one place so
    /// the store and the outline cannot disagree about it.
    #[must_use]
    pub const fn is_learned(self) -> bool {
        matches!(self, Self::Learned)
    }
}

impl fmt::Display for HypothesisSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One light, as section 5's `{ kind, cct, weight, region }` plus the two additions this
/// module's header argues for.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Illuminant {
    /// What kind of light this is.
    pub kind: IlluminantKind,
    /// Correlated colour temperature in kelvin, within [`KELVIN_RANGE`].
    ///
    /// What a slider shows. **A projection, not the estimate** - see [`Illuminant::uv`].
    pub cct_k: f32,
    /// Distance off the Planckian locus, in the recipe's tint units, within [`TINT_RANGE`].
    ///
    /// Positive is magenta, Adobe's convention and `aura_render::colour::white_balance`'s.
    pub tint: f32,
    /// The actual estimate: CIE 1976 `u'v'` chromaticity.
    ///
    /// The solve happens here rather than in kelvin, because a distance in kelvin is not a
    /// distance in colour - 200 K at 2800 K is a visible shift and 200 K at 9000 K is
    /// nothing - and because a fluorescent tube has no honest CCT at all.
    pub uv: [f32; 2],
    /// How much of the frame this light accounts for, `0..1`.
    ///
    /// The weights of a frame's illuminants sum to at most one; the remainder is frame that
    /// no hypothesis claimed.
    pub weight: f32,
    /// Where in the frame this light dominates, when that is localisable.
    ///
    /// `None` for a light that fills the frame. A `Some` on two illuminants at once is what
    /// [`ToneEstimate::mixed_light`] is about, and is what phase 18 reads to place a local
    /// correction.
    #[serde(default)]
    pub region: Option<CropRect>,
    /// Which generator proposed it.
    pub source: HypothesisSource,
    /// How far off neutral the light itself is, `0..1`.
    ///
    /// Section 6.2's "high chroma" test for coloured lighting, stored rather than
    /// recomputed: a purple uplighter and a warm tungsten bulb can share a CCT and this is
    /// the number that separates them.
    pub chroma: f32,
}

impl Illuminant {
    /// Above this chroma a light is saturated enough to be a creative choice.
    ///
    /// Section 6.2's coloured-lighting detector, first of three conditions. The other two -
    /// a stage scene and a low-CRI signature - are checked in
    /// `aura_brain_photo::tone::illuminant`, because they need the scene and the spectrum
    /// and this is a shape rather than a solver.
    pub const SATURATED_ABOVE: f32 = 0.055;

    /// True when this light is saturated enough that neutralising it would be a decision
    /// about the photograph rather than a correction to it.
    #[must_use]
    pub fn is_saturated(&self) -> bool {
        self.chroma >= Self::SATURATED_ABOVE
    }

    /// True when this light should be preserved rather than corrected.
    ///
    /// The kind's own intent, or a saturated light of any kind. Section 2.1's policy in one
    /// place, so that the solver, the panel and the eval harness agree about which frames
    /// the preserve-mood rule applies to.
    #[must_use]
    pub fn should_preserve(&self) -> bool {
        self.kind.is_intentional() || self.is_saturated()
    }
}

// ---------------------------------------------------------------------------
// The skin locus
// ---------------------------------------------------------------------------

/// Where one person's skin actually sits, measured from this wedding.
///
/// Section 6.3, and **the most consequential type in this phase**. Every field is a
/// measurement taken from that person's own best-lit frames; there is no field for a target
/// and no constructor that takes one. A white-balance solution that would push this
/// person's skin outside this region is rejected by the solve - a hard constraint, section
/// 6.3's own word, rather than something applied afterwards and therefore skippable.
///
/// ## Why this is per identity rather than per skin-tone group
///
/// Because a group is a guess about a person and a person is not a guess. Phase 06's rule -
/// never infer anything about a person - is not weakened here: the locus is a pair of
/// chromaticity coordinates and a radius, it carries no label, no bucket and no category,
/// and nothing in this contract can express one. The Monk-scale buckets section 6.3 asks
/// for exist in the **evaluation harness**, where a fairness measurement needs groups, and
/// they never reach the catalog or a decision.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkinLocus {
    /// Whose skin this describes.
    pub identity: IdentityId,
    /// The centre of the region, in CIE 1976 `u'v'`.
    pub uv: [f32; 2],
    /// How far from the centre a sample may sit and still be plausible.
    ///
    /// Grown from the observed spread rather than fixed, with a floor: a person
    /// photographed under one light all day has a tiny measured spread, and a constraint
    /// that tight would reject every correct answer at the reception.
    pub radius: f32,
    /// The reference luminance of the skin, `0..1` in the working space.
    ///
    /// Used by the exposure solve as the anchor when this person is the prominent subject.
    /// Measured, never assumed - this is the number a fixed "ideal skin brightness" would
    /// have been, and it is the single place where a lightening bias would enter.
    pub luma: f32,
    /// How many frames contributed.
    ///
    /// Below [`MIN_LOCUS_SAMPLES`] the locus does not constrain anything and
    /// [`ToneCode::SkinLocusUnavailable`] is emitted instead. A weak locus is worse than
    /// none: it looks like evidence.
    pub samples: u32,
    /// How tightly the samples agree, `0..1`. One is a perfect agreement.
    pub cohesion: f32,
    /// Which build's arithmetic measured it.
    pub analysis_ver: u16,
}

impl SkinLocus {
    /// The smallest radius a locus may have, in `u'v'`.
    ///
    /// A floor rather than a fitted minimum, and the reason is in the type's header: a
    /// person shot under one light has a measured spread of nearly zero, and a hard
    /// constraint of nearly zero width rejects every honest solution. Roughly 0.012 in
    /// `u'v'` is about what a competent human calls "the same skin under different light".
    pub const MIN_RADIUS: f32 = 0.012;

    /// The largest radius a locus may have.
    ///
    /// Past this the constraint has stopped constraining, and saying so is better than
    /// carrying a circle that contains the whole chromaticity diagram.
    pub const MAX_RADIUS: f32 = 0.070;

    /// True when this locus has enough evidence behind it to reject a solution.
    #[must_use]
    pub const fn is_usable(&self) -> bool {
        self.samples >= MIN_LOCUS_SAMPLES
    }

    /// Distance from a chromaticity to the centre of this locus, in `u'v'`.
    #[must_use]
    pub fn distance(&self, uv: [f32; 2]) -> f32 {
        let du = uv[0] - self.uv[0];
        let dv = uv[1] - self.uv[1];
        du.hypot(dv)
    }

    /// True when a chromaticity sits inside the plausible region.
    ///
    /// The hard constraint of section 6.3, asked in exactly one place so that the solver,
    /// the store, the panel and the eval harness cannot disagree about where the edge is.
    #[must_use]
    pub fn contains(&self, uv: [f32; 2]) -> bool {
        self.distance(uv) <= self.radius.clamp(Self::MIN_RADIUS, Self::MAX_RADIUS)
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why the exposure and the white balance came out where they did, as a closed set.
///
/// Section 9 gives DOC "write the mixed-lighting explainer and the skin-fairness
/// statement", which is only a finishable job if the codes are enumerable.
/// `docs/mixed-lighting.md` is written against [`ToneCode::ALL`] and a test asserts every
/// variant appears there.
///
/// **Eleven of these twenty-five withdraw a claim rather than making one.** They are the
/// codes that say the product declined to act, or acted on less evidence than it wanted,
/// and this is the phase where that distinction is most expensive: an editor that
/// confidently neutralises a purple dance floor and an editor that says "this light is
/// meant to be purple and I left it alone" produce the same pixels only in the second case.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ToneCode {
    // -- exposure -------------------------------------------------------
    /// The subject was already inside this scene's luminance band. **A withdrawal**: the
    /// exposure was not moved, and this is the reason a frame that needed nothing carries.
    #[default]
    SubjectExposed,
    /// The subject was below the band and the exposure was lifted.
    SubjectUnderexposed,
    /// The subject was above the band and the exposure was pulled down.
    SubjectOverexposed,
    /// The move stopped short of the band so that highlights did not clip further.
    ///
    /// Section 6.1's first constraint, made visible: a frame whose face is still a third of
    /// a stop dark because the window behind it would have blown is a decision, not a miss.
    ExposureHeldForHighlights,
    /// The move stopped short of the band so that shadow noise stayed inside phase 09's
    /// budget for this scene.
    ///
    /// Section 6.1's second constraint. A lift amplifies whatever the sensor recorded in the
    /// shadows, so the *same* move is bounded from two directions at once - the highlights it
    /// would blow and the noise it would raise - and the two have separate codes because the
    /// remedies differ: one is fixed by a different frame and the other by a different body.
    ExposureHeldForShadows,
    /// The subject was exposed for and the background was allowed to blow.
    ///
    /// Section 6.1's backlit rule. Not a defect: it is what a photographer does by hand.
    BacklitSubject,
    /// The scene's intent kept the frame dark. **A withdrawal.**
    ///
    /// Section 2.1: "moody dance floors stay moody". The band was reachable and the solver
    /// deliberately did not reach for it.
    MoodPreserved,
    /// No face was found, so the scene-level exposure model decided.
    ///
    /// Section 6.1's third bullet. A weaker claim than a face-anchored one, which is why
    /// it lowers the confidence and why [`ToneEstimate::face_anchored`] exists.
    NoFaceSceneModel,
    /// Nothing measurable was found and the exposure was left alone. **A withdrawal.**
    ExposureUnavailable,

    // -- white balance --------------------------------------------------
    /// A known-neutral region anchored the solve. **A withdrawal** in the sense that it is
    /// the strongest evidence this phase has and claims nothing beyond it.
    NeutralFound,
    /// No neutral and no usable skin: the frame's own average decided.
    ///
    /// The weakest answer this phase gives, and the one most likely to be wrong at a
    /// wedding, where a frame is routinely almost entirely one colour.
    GreyWorldFallback,
    /// The winning hypothesis is the one that made known skin most plausible. **A
    /// withdrawal**: it says what the answer was anchored to.
    SkinAnchored,
    /// A hypothesis was rejected because it pushed a known person's skin off their locus.
    ///
    /// The hard constraint firing. Section 6.3.
    SkinLocusConstrained,
    /// No identity in this frame had enough samples for a locus. **A withdrawal**: the
    /// skin constraint was not applied and the answer is weaker for it.
    SkinLocusUnavailable,
    /// Two illuminants light this frame and they disagree spatially.
    ///
    /// Section 6.2: the global answer is the one that governs the subject, both are
    /// recorded, and the local correction is phase 18's.
    MixedLight,
    /// The illuminant governing the subject was chosen over a stronger one elsewhere.
    DominantIlluminantChosen,
    /// The light is saturated and deliberately non-neutral, and it was preserved. **A
    /// withdrawal**, and the one section 2.1 cares most about.
    ColouredLightPreserved,
    /// A saturated light was corrected only far enough to keep skin plausible.
    ///
    /// Section 6.2's compromise: the mood survives and the faces do not go green.
    ColouredLightPartial,
    /// A warm interior below the tungsten floor; the correction was bounded so the room did
    /// not turn blue.
    TungstenReception,
    /// A flash signature was detected and the flash governs the subject.
    FlashDetected,
    /// The hypotheses disagreed and the answer is weak. Goes to the review queue.
    WbLowConfidence,
    /// Nothing could be estimated and the camera's own as-shot values were kept. **A
    /// withdrawal.**
    CameraAsShotUsed,

    // -- provenance -----------------------------------------------------
    /// This scene has no exposure-target row, so the neutral band was used. **A
    /// withdrawal**, and it lowers the confidence for the same reason phase 11's `NoRule`
    /// does.
    NoTargetRow,
    /// This frame was chosen as a per-segment anchor for gallery consistency. **A
    /// withdrawal**: it describes what the frame is *for*, and costs nothing.
    ReferenceAnchor,
    /// The photographer set this by hand and nothing was changed. **A withdrawal**, and the
    /// only one that is unbeatable.
    UserOverride,
}

impl ToneCode {
    /// Every code, in the order `docs/mixed-lighting.md` documents them.
    pub const ALL: [Self; 25] = [
        Self::SubjectExposed,
        Self::SubjectUnderexposed,
        Self::SubjectOverexposed,
        Self::ExposureHeldForHighlights,
        Self::ExposureHeldForShadows,
        Self::BacklitSubject,
        Self::MoodPreserved,
        Self::NoFaceSceneModel,
        Self::ExposureUnavailable,
        Self::NeutralFound,
        Self::GreyWorldFallback,
        Self::SkinAnchored,
        Self::SkinLocusConstrained,
        Self::SkinLocusUnavailable,
        Self::MixedLight,
        Self::DominantIlluminantChosen,
        Self::ColouredLightPreserved,
        Self::ColouredLightPartial,
        Self::TungstenReception,
        Self::FlashDetected,
        Self::WbLowConfidence,
        Self::CameraAsShotUsed,
        Self::NoTargetRow,
        Self::ReferenceAnchor,
        Self::UserOverride,
    ];

    /// How many reason codes there are.
    pub const COUNT: usize = 25;

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SubjectExposed => "subject_exposed",
            Self::SubjectUnderexposed => "subject_underexposed",
            Self::SubjectOverexposed => "subject_overexposed",
            Self::ExposureHeldForHighlights => "exposure_held_for_highlights",
            Self::ExposureHeldForShadows => "exposure_held_for_shadows",
            Self::BacklitSubject => "backlit_subject",
            Self::MoodPreserved => "mood_preserved",
            Self::NoFaceSceneModel => "no_face_scene_model",
            Self::ExposureUnavailable => "exposure_unavailable",
            Self::NeutralFound => "neutral_found",
            Self::GreyWorldFallback => "grey_world_fallback",
            Self::SkinAnchored => "skin_anchored",
            Self::SkinLocusConstrained => "skin_locus_constrained",
            Self::SkinLocusUnavailable => "skin_locus_unavailable",
            Self::MixedLight => "mixed_light",
            Self::DominantIlluminantChosen => "dominant_illuminant_chosen",
            Self::ColouredLightPreserved => "coloured_light_preserved",
            Self::ColouredLightPartial => "coloured_light_partial",
            Self::TungstenReception => "tungsten_reception",
            Self::FlashDetected => "flash_detected",
            Self::WbLowConfidence => "wb_low_confidence",
            Self::CameraAsShotUsed => "camera_as_shot_used",
            Self::NoTargetRow => "no_target_row",
            Self::ReferenceAnchor => "reference_anchor",
            Self::UserOverride => "user_override",
        }
    }

    /// Parse the stored slug. Unknown text is [`ToneCode::SubjectExposed`].
    #[must_use]
    pub fn from_str_or_exposed(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|code| code.as_str() == text)
            .unwrap_or(Self::SubjectExposed)
    }

    /// The sentence this code says when nothing more specific is known.
    ///
    /// **Stored rows carry the code, not the sentence**, for phase 09's three reasons -
    /// size, copy that changes without behaviour changing, and translation. A reason whose
    /// text differs from this, because it names a temperature or a person, stores its text
    /// as well and only then.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::SubjectExposed => {
                "the faces here were already at the brightness this kind of photograph wants"
            }
            Self::SubjectUnderexposed => {
                "the faces here were darker than this kind of photograph wants, so the exposure \
                 was lifted"
            }
            Self::SubjectOverexposed => {
                "the faces here were brighter than this kind of photograph wants, so the \
                 exposure was brought down"
            }
            Self::ExposureHeldForHighlights => {
                "the faces are still a little dark, because lifting them further would have \
                 blown out the bright areas behind them"
            }
            Self::ExposureHeldForShadows => {
                "the exposure was not brought down as far as usual, because the shadows would \
                 have become noisier than this kind of photograph carries"
            }
            Self::BacklitSubject => {
                "the light is behind them, so AURA exposed for their faces and let the \
                 background go bright on purpose"
            }
            Self::MoodPreserved => {
                "this is meant to be a dark photograph, so AURA left it dark rather than \
                 brightening it to an average"
            }
            Self::NoFaceSceneModel => {
                "there are no faces in this photograph, so the exposure was set from what this \
                 kind of scene usually wants"
            }
            Self::ExposureUnavailable => {
                "AURA could not measure anything useful here, so it left the exposure alone"
            }
            Self::NeutralFound => {
                "something in this photograph is known to be white or grey, and the colour was \
                 set from it"
            }
            Self::GreyWorldFallback => {
                "there was nothing white and no recognised faces here, so the colour was set \
                 from the photograph's own average - treat it as a starting point"
            }
            Self::SkinAnchored => {
                "the colour was chosen to make the skin here match how it looks in the rest of \
                 this wedding"
            }
            Self::SkinLocusConstrained => {
                "a warmer or cooler setting would have made somebody's skin an implausible \
                 colour, so it was ruled out"
            }
            Self::SkinLocusUnavailable => {
                "AURA has not yet seen enough well-lit photographs of the people here to know \
                 what their skin should look like, so it has not used skin as a guide"
            }
            Self::MixedLight => {
                "there are two different-coloured lights in this photograph. AURA has set the \
                 colour for the light on the people; the rest can be corrected separately"
            }
            Self::DominantIlluminantChosen => {
                "more of this photograph is lit by one light, but the people are lit by \
                 another, and AURA set the colour for the people"
            }
            Self::ColouredLightPreserved => {
                "the lighting here is deliberately coloured, so AURA left it coloured rather \
                 than correcting it to white"
            }
            Self::ColouredLightPartial => {
                "the lighting here is deliberately coloured, so AURA corrected it only far \
                 enough to keep skin looking like skin"
            }
            Self::TungstenReception => {
                "this room is lit by warm bulbs. AURA has kept some of that warmth rather than \
                 making the whole room look blue"
            }
            Self::FlashDetected => {
                "flash was used, so the colour was set for the flash on the people rather than \
                 for the room behind them"
            }
            Self::WbLowConfidence => {
                "AURA is not confident about the colour here. It is worth a look"
            }
            Self::CameraAsShotUsed => {
                "AURA could not work out the colour of the light, so it kept the setting your \
                 camera recorded"
            }
            Self::NoTargetRow => {
                "AURA has no exposure guidance recorded for this kind of photograph yet, so it \
                 has used a cautious neutral setting"
            }
            Self::ReferenceAnchor => {
                "this photograph is one of the reference frames for this part of the day; the \
                 rest of the section will be matched to it"
            }
            Self::UserOverride => "you set this by hand, and AURA has not changed it",
        }
    }

    /// True when this code withdraws a claim rather than making one.
    ///
    /// The same machine-readable list phase 11 introduced, and the eval harness asserts
    /// that a withdrawal never carries a penalty against the confidence beyond the one its
    /// own doc comment names.
    #[must_use]
    pub const fn is_withdrawal(self) -> bool {
        matches!(
            self,
            Self::SubjectExposed
                | Self::MoodPreserved
                | Self::ExposureUnavailable
                | Self::NeutralFound
                | Self::SkinAnchored
                | Self::SkinLocusUnavailable
                | Self::ColouredLightPreserved
                | Self::CameraAsShotUsed
                | Self::NoTargetRow
                | Self::ReferenceAnchor
                | Self::UserOverride
        )
    }

    /// True when this code is about the exposure rather than about the colour.
    ///
    /// The panel groups reasons under the two sliders they belong to, and a caller that
    /// derived this from the slug's prefix would get [`ToneCode::NoTargetRow`] wrong.
    #[must_use]
    pub const fn is_exposure(self) -> bool {
        matches!(
            self,
            Self::SubjectExposed
                | Self::SubjectUnderexposed
                | Self::SubjectOverexposed
                | Self::ExposureHeldForHighlights
                | Self::ExposureHeldForShadows
                | Self::BacklitSubject
                | Self::MoodPreserved
                | Self::NoFaceSceneModel
                | Self::ExposureUnavailable
        )
    }
}

impl fmt::Display for ToneCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One thing that decided the exposure or the white balance, with the pixels behind it.
///
/// Invariant 2 makes the reason mandatory. The evidence rectangle is optional here and not
/// in phase 11, because most of what decides a white balance is a statistic over the whole
/// frame - but the two cases where it is *not* are the two the panel most needs to show: a
/// known-neutral region and a mixed-light boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToneReason {
    /// Which reason this is.
    pub code: ToneCode,
    /// The sentence a photographer reads, in the product's voice.
    pub text: String,
    /// How much this moved the confidence.
    ///
    /// Negative for a doubt, zero or positive for a withdrawal that costs nothing. Signed
    /// for phase 11's reason: the panel sorts by it, and a sort that has to consult two
    /// fields gets written wrong once.
    pub weight: f32,
    /// The pixels to show, when there are any more specific than the whole frame.
    #[serde(default)]
    pub evidence: Option<CropRect>,
}

impl ToneReason {
    /// A reason about the whole frame.
    #[must_use]
    pub fn frame(code: ToneCode, text: impl Into<String>, weight: f32) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: None,
        }
    }

    /// A reason about one region.
    #[must_use]
    pub fn at(code: ToneCode, text: impl Into<String>, weight: f32, evidence: CropRect) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: Some(evidence),
        }
    }

    /// A reason carrying only the code's own sentence.
    #[must_use]
    pub fn plain(code: ToneCode, weight: f32) -> Self {
        Self::frame(code, code.user_text(), weight)
    }

    /// True when this reason cost the estimate confidence.
    #[must_use]
    pub fn is_doubt(&self) -> bool {
        self.weight < 0.0
    }
}

// ---------------------------------------------------------------------------
// Alternatives
// ---------------------------------------------------------------------------

/// A runner-up solution, and what it would have cost.
///
/// Section 5's `alternatives: Vec<(f32, f32, f32)>`, given names and a fourth number. The
/// fourth is why the shape changed: a panel that offers "try this instead" without saying
/// what it gives up is a panel that turns a considered solve into a lucky dip, and phase
/// 27's QC agent needs to know how close the second answer was before it swaps one in.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToneAlternative {
    /// Exposure offset in stops, within [`EXPOSURE_RANGE`].
    pub exposure_ev: f32,
    /// Colour temperature in kelvin, within [`KELVIN_RANGE`].
    pub temperature_k: f32,
    /// Green-magenta tint, within [`TINT_RANGE`].
    pub tint: f32,
    /// What the solve scored it at. Lower is better; the winner's cost is the floor.
    pub cost: f32,
    /// Why it lost, or what it would have been anchored to.
    pub why: ToneCode,
}

impl ToneAlternative {
    /// The most alternatives one estimate carries.
    ///
    /// Three. A panel that offers eight is a panel that has stopped making a decision, and
    /// the storage budget in section 11 is 600 bytes for the whole row.
    pub const MAX: usize = 3;
}

// ---------------------------------------------------------------------------
// The estimate
// ---------------------------------------------------------------------------

/// Everything phase 15 decided about one photograph's exposure and colour.
///
/// PHASE-15 section 5's frozen shape, plus the additions this module's header lists. Every
/// field is a measurement, a decision derived from one, or provenance about how the
/// decision was reached.
///
/// **It is not an edit.** The values here reach the pixels only through
/// `aura_recipe::schema::merge`, which is phase 14's rule and the only place
/// [`ToneEstimate::user_edited`] can be honoured. A `ToneEstimate` with
/// `user_edited = true` is a record of what the product *would* have said, kept beside what
/// the photographer said instead, so that the review queue can show the disagreement and
/// phase 30's learning loop can read it.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct ToneEstimate {
    /// The photograph.
    pub image_id: ImageId,
    /// Exposure offset in stops, within [`EXPOSURE_RANGE`]. Positive brightens.
    pub exposure_ev: f32,
    /// How sure the exposure is, `0..1`. Invariant 2.
    pub exposure_conf: f32,
    /// Colour temperature in kelvin, within [`KELVIN_RANGE`].
    pub temperature_k: f32,
    /// Green-magenta tint, within [`TINT_RANGE`]. Positive is magenta.
    pub tint: f32,
    /// How sure the white balance is, `0..1`. Invariant 2.
    ///
    /// Compared against [`REVIEW_WB_BELOW`] to decide whether the frame reaches the review
    /// queue. Separate from [`ToneEstimate::exposure_conf`] because the two fail
    /// independently: a backlit frame can have an obvious white balance and a hard-won
    /// exposure, and a tungsten close-up the reverse.
    pub wb_conf: f32,
    /// Every light found in this frame, strongest weight first.
    pub illuminants: Vec<Illuminant>,
    /// True when two illuminants disagree spatially.
    ///
    /// Section 2.1: a `MIXED_LIGHT` note "so Phase 18 can apply local WB corrections
    /// later". This is that note, and phase 18 reads
    /// [`Illuminant::region`] on the entries to place them.
    pub mixed_light: bool,
    /// Index into [`ToneEstimate::illuminants`] of the light that governs the subject.
    ///
    /// `None` when there was no subject to govern, which is a different statement from
    /// index zero and is why this is an option rather than a default.
    pub dominant_on_subject: Option<usize>,
    /// The prominence-weighted mean luminance of the faces, before the exposure moved,
    /// `0..1`.
    ///
    /// Zero with [`ToneEstimate::face_anchored`] false means "not measured", not "black".
    pub subject_luma_before: f32,
    /// Where this scene wants that luminance to be, `0..1`.
    pub subject_luma_target: f32,
    /// The estimated skin error this solution leaves, in dE00.
    ///
    /// Measured against the identities' own loci, so it is an estimate of *consistency*
    /// across the wedding rather than of accuracy against a chart. Section 10.1's gate is
    /// on the eval harness's figure, which is measured against expert references; this is
    /// the number the product can compute without one.
    pub skin_de00_estimate: f32,
    /// Runner-up solutions, best first, at most [`ToneAlternative::MAX`].
    pub alternatives: Vec<ToneAlternative>,
    /// Why, at most [`ToneEstimate::MAX_REASONS`], strongest doubt first.
    pub reasons: Vec<ToneReason>,
    /// Which scene the bands were conditioned on.
    ///
    /// Invariant 7: a stored decision that does not say which scene it assumed is not
    /// reproducible. [`SceneId::Unknown`] means the neutral target row was used.
    pub scene: SceneId,
    /// True when a face anchored the exposure.
    ///
    /// **This phase's most important single bit.** Section 1's whole argument is that
    /// subject-referred exposure is what fixes auto-exposure, and a wedding at 100 %
    /// coverage and 3 % face-anchored has been exposed on frame averages nearly everywhere.
    pub face_anchored: bool,
    /// True when the frame was read as backlit and exposed for the subject anyway.
    pub backlit: bool,
    /// True when a saturated non-neutral light was preserved rather than corrected.
    pub coloured_light: bool,
    /// How much highlight clipping this exposure adds, as a fraction of the frame.
    ///
    /// Section 6.1's constraint, stored so that section 10.1's "never increases clipping
    /// beyond scene tolerance" is a query rather than a re-render. Negative is impossible;
    /// zero is the usual answer.
    pub clipping_added: f32,
    /// How many identities in this frame had a usable skin locus.
    ///
    /// The denominator behind [`ToneOutline::skin_constrained`], per frame.
    pub constrained_identities: u32,
    /// True when the photographer set these values by hand.
    ///
    /// Checked inside the statement a re-analysis would overwrite the row with. A
    /// photographer's decision is unbeatable, for the seventh phase running - and here it
    /// is doubled: `Provenance::user_edited_fields` in the recipe protects the pixels and
    /// this protects the record of what the product thinks.
    pub user_edited: bool,
    /// True when the photographer has looked at this frame in the review queue.
    ///
    /// Separate from [`ToneEstimate::user_edited`]: accepting an estimate and changing it
    /// are different answers, and a queue that cannot tell them apart shows the same frame
    /// every time it is opened.
    pub reviewed: bool,
    /// Which learned heads produced the prediction and the faceless exposure.
    pub model_ver: u16,
    /// Which build's arithmetic produced the statistics and the solve.
    pub analysis_ver: u16,
    /// Which target table the bands came from.
    pub targets_ver: u16,
}

impl ToneEstimate {
    /// The most reasons one estimate carries.
    ///
    /// Six, matching every phase since 09. A panel that lists nine reasons is a panel
    /// nobody reads, and the ranking that picks the six is part of the explanation.
    pub const MAX_REASONS: usize = 6;

    /// The one confidence a caller outside this phase should use.
    ///
    /// The **geometric** mean of the two, so a confident exposure cannot rescue a hopeless
    /// white balance. The same fusion phase 09's technical score and phase 12's keep score
    /// use, and for the same reason: these are two things that both have to be right.
    #[must_use]
    pub fn confidence(&self) -> f32 {
        (self.exposure_conf.clamp(0.0, 1.0) * self.wb_conf.clamp(0.0, 1.0)).sqrt()
    }

    /// True when this frame belongs in the low-confidence review queue.
    ///
    /// A frame the photographer has already reviewed never returns, whatever its
    /// confidence: a queue that re-offers a decision somebody has made is a queue somebody
    /// stops opening.
    #[must_use]
    pub fn needs_review(&self) -> bool {
        !self.reviewed && !self.user_edited && self.wb_conf < REVIEW_WB_BELOW
    }

    /// The light that governs the subject, when one was identified.
    #[must_use]
    pub fn subject_illuminant(&self) -> Option<&Illuminant> {
        self.dominant_on_subject
            .and_then(|index| self.illuminants.get(index))
    }

    /// The reasons that cost confidence, strongest first.
    #[must_use]
    pub fn doubts(&self) -> Vec<&ToneReason> {
        let mut out: Vec<&ToneReason> = self.reasons.iter().filter(|r| r.is_doubt()).collect();
        out.sort_by(|left, right| left.weight.total_cmp(&right.weight));
        out
    }

    /// The reasons that withdrew a claim, in the order they were made.
    #[must_use]
    pub fn withdrawals(&self) -> Vec<&ToneReason> {
        self.reasons
            .iter()
            .filter(|reason| reason.code.is_withdrawal())
            .collect()
    }

    /// True when the three version fields match this build's.
    ///
    /// The check `AURA-ML-5060` is raised on, written here so every consumer asks the
    /// question the same way.
    #[must_use]
    pub const fn matches_versions(&self, model: u16, analysis: u16, targets: u16) -> bool {
        self.model_ver == model && self.analysis_ver == analysis && self.targets_ver == targets
    }

    /// True when this estimate is inside the section 10.1 tolerances of a reference.
    ///
    /// In the contract rather than in the eval harness because phase 25 compares an
    /// estimate against a segment's reference frame with exactly these tolerances, and a
    /// second copy of the comparison is a second answer to "did this frame match".
    #[must_use]
    pub fn agrees_with(&self, exposure_ev: f32, temperature_k: f32, tint: f32) -> bool {
        (self.exposure_ev - exposure_ev).abs() <= EXPOSURE_TOLERANCE_EV
            && (self.temperature_k - temperature_k).abs() <= WB_TOLERANCE_K
            && (self.tint - tint).abs() <= WB_TOLERANCE_TINT
    }
}

// ---------------------------------------------------------------------------
// Reference frames
// ---------------------------------------------------------------------------

/// One of a segment's anchors for gallery consistency.
///
/// Section 6.4: "for each Phase 07 segment, select 3-5 reference frames: high WB
/// confidence, good subject exposure, primary identity present, no mixed light. Store them
/// with the segment; Phase 25 normalises the rest of the segment toward these anchors."
///
/// **It is an anchor, not a correction.** Nothing in this phase changes another frame to
/// match one of these, and there is no column, field or method here that could. Phase 25
/// owns the normalisation and phase 26 owns the cross-camera version of it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReferenceFrame {
    /// The photograph.
    pub image_id: ImageId,
    /// The chapter it anchors.
    pub segment: SegmentId,
    /// Its place in the segment's ordering, zero first.
    pub rank: u16,
    /// The white-balance confidence that got it chosen.
    pub wb_conf: f32,
    /// Its solved temperature, which is what phase 25 normalises toward.
    pub temperature_k: f32,
    /// Its solved tint.
    pub tint: f32,
    /// Its subject luminance after the exposure moved, `0..1`.
    pub subject_luma: f32,
    /// How good an anchor it is, `0..1`. One is the best frame in the segment.
    pub quality: f32,
}

// ---------------------------------------------------------------------------
// Coverage
// ---------------------------------------------------------------------------

/// What a project's tone pass covered and what it found.
///
/// Not in section 5. Phase 05's rule, inherited for the ninth time: report coverage when
/// you report a result, and say what the denominator is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToneOutline {
    /// Photographs with an estimate.
    pub estimated: u32,
    /// Photographs in the project.
    pub photos: u32,
    /// Fraction of the project with an estimate, `0..1`.
    ///
    /// The denominator is **every photograph**, as phases 09, 10, 11 and 14's are. An
    /// exposure and a white balance need only pixels, so a frame with no estimate is this
    /// phase's gap and reporting it any other way would hide it.
    pub coverage: f32,
    /// Fraction of *estimated* frames whose exposure was anchored on a face, `0..1`.
    ///
    /// **This phase's refinement of the coverage rule, and the number that matters when it
    /// is low.** Section 1's argument is that subject-referred exposure is the whole
    /// improvement; a wedding at 100 % coverage and 3 % face-anchored has been exposed the
    /// way every other editor exposes it, and a photographer about to trust these numbers
    /// needs to know that before they do.
    pub face_anchored: f32,
    /// Fraction of estimated frames whose white balance was bounded by a skin locus, `0..1`.
    ///
    /// The second number, and the one section 6.3's fairness argument depends on. A wedding
    /// where no locus was ever usable has been white-balanced by grey-world and neutrals
    /// alone.
    pub skin_constrained: f32,
    /// Frames carrying [`ToneEstimate::mixed_light`].
    pub mixed_light: u32,
    /// Frames where a saturated light was preserved.
    pub coloured_light: u32,
    /// Frames below [`REVIEW_WB_BELOW`] that nobody has reviewed.
    pub needs_review: u32,
    /// Frames the photographer has set by hand.
    pub user_edited: u32,
    /// Mean exposure offset over estimated frames, in stops.
    ///
    /// Section 11's `tone.estimated {mean_ev}`.
    pub mean_ev: f32,
    /// Mean colour temperature over estimated frames, in kelvin.
    ///
    /// Section 11's `tone.estimated {mean_cct}`.
    pub mean_cct: f32,
    /// How many frames carry each illuminant kind, in [`IlluminantKind::ALL`] order.
    ///
    /// Counted on the *dominant* illuminant, so the histogram sums to at most `estimated`.
    /// The number that tells a support engineer, in one line, that a wedding was shot
    /// entirely under fluorescent tubes.
    pub illuminant_histogram: [u32; IlluminantKind::COUNT],
    /// Segments with at least [`MIN_REFERENCE_FRAMES`] anchors.
    pub segments_anchored: u32,
    /// Segments in the project's story.
    pub segments: u32,
    /// Identities with a usable skin locus.
    pub loci: u32,
    /// Scenes with no target row, by slug.
    ///
    /// Phase 11's `unruled_scenes` under a new name, and for the same reason: a wedding
    /// whose scenes are all missing from the target table has been exposed entirely against
    /// neutral bands.
    pub untargeted_scenes: Vec<String>,
    /// Which learned heads produced the numbers.
    pub model_ver: u16,
    /// Which build's arithmetic produced them.
    pub analysis_ver: u16,
    /// Which target table the bands came from.
    pub targets_ver: u16,
}

impl ToneOutline {
    /// True when nothing has been estimated.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.estimated == 0
    }

    /// How many frames carry one illuminant kind as their dominant light.
    #[must_use]
    pub fn count(&self, kind: IlluminantKind) -> u32 {
        IlluminantKind::ALL
            .iter()
            .position(|candidate| *candidate == kind)
            .and_then(|index| self.illuminant_histogram.get(index).copied())
            .unwrap_or(0)
    }

    /// Fraction of estimated frames that are mixed-light, `0..1`.
    ///
    /// Section 11's `tone.estimated {mixed_light_ratio}`.
    ///
    /// `f64` rather than `f32`, unlike every other ratio in the product, because this one is
    /// a *count* over a *count* and both can exceed `f32`'s exact integer range on a wedding
    /// nobody has shot yet. The cost of being right about it is nothing.
    #[must_use]
    pub fn mixed_light_ratio(&self) -> f64 {
        if self.estimated == 0 {
            return 0.0;
        }
        f64::from(self.mixed_light) / f64::from(self.estimated)
    }

    /// True when every segment in the story has enough anchors for phase 25.
    ///
    /// Section 10.1's sixth gate, and section 13's sixth criterion, asked in one place.
    #[must_use]
    pub const fn every_segment_anchored(&self) -> bool {
        self.segments_anchored >= self.segments
    }
}

// ---------------------------------------------------------------------------
// The photographer's own answer
// ---------------------------------------------------------------------------

/// What a photographer set by hand.
///
/// Every field is optional because the three sliders are independent: somebody who corrects
/// only the temperature has not made a claim about the exposure, and an override that
/// carried all three would silently freeze the two they did not touch.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToneOverride {
    /// Exposure offset in stops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure_ev: Option<f32>,
    /// Colour temperature in kelvin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_k: Option<f32>,
    /// Green-magenta tint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<f32>,
}

impl ToneOverride {
    /// True when nothing was set.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.exposure_ev.is_none() && self.temperature_k.is_none() && self.tint.is_none()
    }

    /// The dotted recipe paths this override touches, sorted.
    ///
    /// What `aura_recipe::schema::merge` is handed as `user_edited_fields`, produced here so
    /// the spelling of a protected path lives beside the shape that names it rather than in
    /// whichever command happened to be written first.
    #[must_use]
    pub fn recipe_paths(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.exposure_ev.is_some() {
            out.push("global.exposure".to_string());
        }
        if self.temperature_k.is_some() {
            out.push("global.temperature".to_string());
        }
        if self.tint.is_some() {
            out.push("global.tint".to_string());
        }
        out.sort();
        out
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask what a photograph should be exposed to and what colour its light was.
///
/// Frozen. Implemented by `aura_brain_photo::tone::Tone`, and the only route from any later
/// phase to an exposure offset, a temperature, an illuminant or a skin locus.
///
/// The rule phase 05 wrote for `SimilarityIndex`, phase 06 for `PeopleService`, phase 07
/// for `StoryService`, phase 08 for `MomentService`, phase 09 for `IntegrityService`, phase
/// 10 for `EmotionService`, phase 11 for `CompositionService`, phase 12 for `CullService`,
/// phase 13 for `ExplainService` and phase 14 for `RenderService`, an eleventh time and for
/// the same reason: **no phase may keep its own idea of what colour the light was.** Phase
/// 16 grades on top of these values, phase 17 shifts them, phase 18 corrects locally
/// against them, phase 25 normalises a gallery toward them and phase 26 matches two cameras
/// with them. Two answers to "what temperature was this room" is an album that does not
/// match the gallery.
pub trait ToneService: Send + Sync + fmt::Debug {
    /// What a project's pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the estimates cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<ToneOutline>;

    /// One photograph's estimate, or `None` when it has not been analysed.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the estimate cannot be read.
    fn of_image(&self, image: ImageId) -> AuraResult<Option<ToneEstimate>>;

    /// The frames whose white balance is worth a photographer's attention, weakest first.
    ///
    /// Section 9's MFE deliverable: "per-scene review queue for low-confidence WB, batch
    /// accept/adjust". `limit` bounds the answer because the queue is a page.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the estimates cannot be read.
    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>>;

    /// One segment's anchors, best first.
    ///
    /// What phase 25 reads. Empty when the segment had fewer than
    /// [`MIN_REFERENCE_FRAMES`] candidates, which is a real answer rather than a failure: a
    /// chapter of eleven frames shot under three different lights has no anchor, and
    /// normalising it toward a frame that is not representative is worse than leaving it
    /// alone.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the anchors cannot be read.
    fn reference_frames(&self, segment: SegmentId) -> AuraResult<Vec<ReferenceFrame>>;

    /// Where one person's skin sits, when enough frames have said.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the locus cannot be read.
    fn skin_locus(&self, identity: IdentityId) -> AuraResult<Option<SkinLocus>>;

    /// Every usable locus in a project, by identity.
    ///
    /// Read once per pass rather than per frame: a 4,000-image wedding with forty
    /// identities would otherwise make 160,000 round trips through a service whose answer
    /// changes twice an hour.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the loci cannot be read.
    fn skin_loci(&self, project: ProjectId) -> AuraResult<BTreeMap<IdentityId, SkinLocus>>;

    /// Record that the photographer has looked at this estimate and agrees.
    ///
    /// Sets [`ToneEstimate::reviewed`] and removes the frame from the queue. It does not
    /// set [`ToneEstimate::user_edited`], because accepting a suggestion is not authoring
    /// one and phase 30's learning loop needs to be able to tell those apart.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5061` when the photograph has no estimate.
    fn accept(&self, image: ImageId) -> Result<(), AuraError>;

    /// Record what the photographer set instead.
    ///
    /// Sets [`ToneEstimate::user_edited`] and [`ToneEstimate::reviewed`], and is not undone
    /// by a re-analysis: the check is inside the statement that would overwrite the row,
    /// exactly as `identities.user_locked`, `segments.user_locked`, `moments.user_locked`,
    /// `image_integrity.user_reviewed` and `image_composition.dismissed` are.
    ///
    /// **This records the disagreement; it does not move a pixel.** The pixels move when
    /// the caller writes the same values through `aura_recipe::schema::merge`, which is the
    /// only function in the workspace permitted to add to `user_edited_fields`. Two writes
    /// rather than one, deliberately: a service that could do both would be a second way to
    /// edit a recipe.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5061` when the photograph has no estimate, or when the override is empty,
    /// or when a value is outside its documented range.
    fn set_override(&self, image: ImageId, values: ToneOverride) -> Result<(), AuraError>;
}
