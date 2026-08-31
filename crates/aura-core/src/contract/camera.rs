//! FROZEN CONTRACT. Multi-camera and second-shooter matching. PHASE-26 section 5.
//!
//! Twenty-five phases decided things about a photograph or about a set of photographs. This one
//! decides about a **body**: the thing that was between the light and the sensor, and the person
//! who was holding it.
//!
//! ## Why matching appearance is a different problem from matching parameters
//!
//! Two cameras set to 5,200 K and +0.0 EV do not produce the same photograph, and no amount of
//! agreeing about the numbers makes them. Brand colour science differs in the demosaic, in the
//! forward matrix, in the tone curve baked into the manufacturer's own rendering and - most
//! visibly at a wedding - in what each body does to a highlight as it rolls off. So the objective
//! here is stated over **what the frames look like** rather than over what they were set to:
//! [`AppearanceDistance`] is a weighted sum of a skin difference, a white-point difference, a
//! grade-character difference and a contrast difference, and a transform is good exactly when it
//! makes that number smaller on evidence the solver has not seen.
//!
//! That framing is what makes the phase testable. "The files match" is an opinion; `skin_de00 <=
//! 2.0 on held-out pairs` is a query.
//!
//! ## The five properties this contract exists to make structural
//!
//! **A transform is per body *and* per flash state.** [`FlashState`] is half of the key of every
//! fingerprint and every transform in this module, and there is no shape here that can hold one
//! transform for a whole body. Section 6.1: brand differences are amplified under flash, and a
//! single transform fitted across both populations is a transform that is wrong for both. A
//! photographer with a Canon on ambient and a Sony on flash is the common case rather than the
//! exotic one.
//!
//! **Evidence is graded and the grade is on the row.** [`TransformSource`] has three variants and
//! not the two section 5's comment lists, because a transform blended between the wedding's own
//! matched pairs and a bundled brand baseline is neither of them. Reporting a blend as
//! [`TransformSource::MatchedPairs`] overstates the evidence and reporting it as
//! [`TransformSource::BrandBaseline`] understates it, and a per-camera report whose whole job is
//! to say what was corrected and on what evidence cannot round either way.
//! [`CameraTransform::blend`] carries the share. ADR-0053 section 3.
//!
//! **A transform that does not improve held-out evidence is not used.** [`CameraTransform`] carries
//! four numbers - the appearance distance before and after on the fitting pairs, and the same two
//! on the held-out pairs - so section 6.2's "verify on held-out pairs: if the transform does not
//! improve appearance distance on held-out evidence, fall back to the brand baseline and say so"
//! is a stored fact rather than a promise about the solver. [`CameraCode::HeldOutFailed`] is what a
//! photographer reads when it happened.
//!
//! **A shooter's habit is capped, never erased.** [`ShooterBias`] stores the systematic offset that
//! was *measured* beside the correction that was *applied*, and [`MAX_SHOOTER_SHARE`] means the
//! second is never the whole of the first. A second shooter who works a third of a stop darker than
//! the lead is harmonised toward them and does not become them. Section 6.3, and the report is what
//! makes the difference visible rather than silent.
//!
//! **Nothing here moves a pixel and nothing here writes a recipe.** [`CameraMatchService`] has no
//! `apply`. The transforms are stored, phase 25 reads them before it builds its tree, and
//! `aura_recipe::schema::merge` is still the one function in the workspace permitted to write a
//! recipe. Phase 14's rule, eleventh application.
//!
//! ## Order of operations, which is the one thing a later phase can get wrong
//!
//! Section 6.4: **camera transforms are applied before phase 25's within-scene normalisation.**
//! Not as a convention - as a data dependency. `aura_brain_gallery::api::collect_frames` folds a
//! camera transform into the `Frame` it builds, so the consistency pass's tree, its change points,
//! its anchors and its targets are all computed over already-comparable numbers. Reversing the two
//! produces a gallery in which every node's target is the average of two brands' colour science
//! and every frame is normalised toward a look neither camera can produce.

use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::error::{AuraError, AuraResult};
use super::gallery::ImageId;
use super::ids::{NodeId, PairId};
use super::moment::CameraId;
use crate::{ProjectId, SceneId};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The fewest verified matched pairs before a solved transform is trusted on its own.
///
/// Section 6.1's own default. Twelve rather than a smaller number because the transform vector has
/// nine free parameters and a fit with fewer observations than twice its own dimension is a fit
/// that describes the pairs rather than the cameras. Below this the answer is blended with the
/// brand baseline in proportion to the evidence, which is [`CameraCode::BlendedWithBaseline`].
pub const MIN_MATCHED_PAIRS: u32 = 12;

/// The fewest verified pairs before a solve is attempted at all.
///
/// Below this there is nothing to fit and the brand baseline is used whole.
/// [`CameraCode::PairsInsufficient`] when there were some, [`CameraCode::PairsAbsent`] when there
/// were none, and they are separate codes because "we looked and found four" and "the second camera
/// never photographed the same room" send a photographer to two different places.
pub const MIN_SOLVE_PAIRS: u32 = 4;

/// The widest gap, in milliseconds, between two frames that may be called a matched pair.
///
/// Ninety seconds. Section 6.1 asks for "the same ceremony minutes apart under the same light", and
/// the binding constraint is not the ceremony but the light: a room lit by the late-afternoon sun
/// through one window measurably changes inside three minutes, and a pair straddling that change
/// teaches the solver that a Sony is warmer than a Canon when what happened is that the sun moved.
pub const MAX_PAIR_GAP_MS: i64 = 90_000;

/// The least embedding similarity two frames must have before they are candidates.
///
/// Phase 05's cosine similarity on the frozen wedding embedding. A floor rather than a ceiling,
/// and it is deliberately permissive: the *subjects* being similar is a cheap pre-filter, and
/// [`MIN_BACKGROUND_AGREEMENT`] is the check that actually decides.
pub const MIN_PAIR_SIMILARITY: f32 = 0.55;

/// How closely two frames' background statistics must agree before the pair is verified.
///
/// Section 6.1: "verify by comparing background statistics rather than subjects." The reason is the
/// whole mechanism. Two frames of the same bride's face from two bodies differ in *exactly the way
/// this phase is trying to measure*, so scoring the pair on the subject scores the thing under
/// test. The background - a wall, a marquee ceiling, a row of chairs - was lit by the same light and
/// is not what either camera was metering for, so a disagreement there means the pair is not a pair.
pub const MIN_BACKGROUND_AGREEMENT: f32 = 0.70;

/// The share of verified pairs held back from the fit and used only to check it.
///
/// A quarter, taken deterministically by pair id order rather than at random, because invariant 4
/// says the same inputs produce the same output and a random split makes a transform a function of
/// a seed nobody stored.
pub const HELDOUT_SHARE: f32 = 0.25;

/// The fewest held-out pairs before held-out verification is considered to have happened.
///
/// Below this the check is not run and [`CameraTransform::heldout_after`] equals
/// [`CameraTransform::heldout_before`]; the transform is still blended toward the baseline, because
/// a fit nobody could check is a fit on thin evidence by definition.
pub const MIN_HELDOUT_PAIRS: u32 = 3;

/// How much smaller the held-out appearance distance must become before a solve is kept.
///
/// A share of the distance the transform started at rather than an absolute, because the distance
/// between two Canon bodies and the distance between a Canon and a Fujifilm differ by an order of
/// magnitude and one absolute number is right for exactly one of them. Phase 22's rule: a threshold
/// on a measurement is a statement about the instrument as well as about the world.
pub const MIN_HELDOUT_IMPROVEMENT: f32 = 0.05;

/// The most a camera transform may move correlated colour temperature, in kelvin.
///
/// Twice phase 25's [`super::gallery::MAX_D_CCT_K`], and the difference is the point: a gallery
/// delta corrects *drift within one room*, where 450 K is already more than the eye forgives, and
/// this corrects *a brand's colour science*, where the honest measured difference between two
/// popular bodies under tungsten reaches 700 K. A bound below the effect makes the phase a
/// no-operation dressed as a feature.
pub const MAX_T_CCT_K: f32 = 900.0;

/// The most a camera transform may move tint.
///
/// Scaled from [`MAX_T_CCT_K`] at phase 15's own equivalence - it calls 200 K and 4 tint units
/// comparable - which puts 900 K at 18. Twenty rather than eighteen, because the green-magenta axis
/// is where two brands' matrices differ most under fluorescent and LED light, which is most of a
/// modern reception.
pub const MAX_T_TINT: f32 = 20.0;

/// The most a camera transform may move exposure, in stops.
///
/// Two thirds of a stop. Large enough to absorb a metering difference between two bodies and a
/// habit difference between two people; small enough that a transform cannot rescue a frame that is
/// simply wrong, which is phase 15's job and phase 15's row.
pub const MAX_T_EXPOSURE_EV: f32 = 0.60;

/// The most a per-channel gain may depart from unity.
///
/// Ten per cent. This is the axis with the least headroom in the whole contract, because a channel
/// gain is the one parameter here that can make a photograph look *broken* rather than merely
/// different: pushing red by a quarter to match a skin measurement takes every red in the frame -
/// the roses, the sari, the exit sign - with it. Section 6.2's "bounds prevent the solver from
/// making a Canon file look broken to satisfy a metric", and this is the bound it means.
pub const MAX_CHANNEL_GAIN: f32 = 0.10;

/// The most a camera transform may move saturation, in the recipe's `-100..100` units.
pub const MAX_T_SATURATION: f32 = 12.0;

/// The most a camera transform may reshape contrast, per shadow/mid/highlight term.
///
/// A fraction rather than recipe units: [`CameraTransform::contrast_shape`] is a multiplier on the
/// three tonal thirds and fifteen per cent is about where a roll-off correction stops reading as a
/// correction and starts reading as a different photograph.
pub const MAX_CONTRAST_SHAPE: f32 = 0.15;

/// The cross-camera skin dE00 this phase promises inside a matched scene.
///
/// Section 0's headline KPI and section 10.1's first gate. It is in the contract rather than in the
/// eval harness because the panel, the outline and the gate all ask the same question, and two
/// copies of 2.0 is one chance to write one of them as 3.0. Phase 25 made the same call about its
/// own ceiling and for the same reason.
pub const CROSS_CAMERA_DE00_CEILING: f32 = 2.0;

/// The share of the grade-signature distance between two cameras this phase promises to remove.
///
/// Section 0's second KPI: "cross-shooter grade signature distance reduced >= 65 %".
pub const SIGNATURE_REDUCTION_TARGET: f32 = 0.65;

/// The furthest a camera-level skin correction may move skin chromaticity, in CIE 1976 `u'v'`.
///
/// Below phase 25's per-frame [`super::gallery::SKIN_CHROMA_CAP`] on purpose, and that is not a
/// typo. A camera correction applies to **every frame that body shot**, so an error here is an
/// error four thousand times over, and the two corrections compose: this one runs first and phase
/// 25's runs on what is left. The conservative half is the one that cannot be inspected frame by
/// frame.
pub const SKIN_UV_CAP: f32 = 0.012;

/// The furthest a camera-level skin correction may move skin luminance, `0..1`.
pub const SKIN_LUMA_CAP: f32 = 0.04;

/// The fewest usable frames a body needs before it is fingerprinted at all.
///
/// Below this [`CameraCode::FingerprintAbsent`] is recorded and the body is matched from its brand
/// baseline alone. Phase 15's argument for `MIN_LOCUS_SAMPLES`, unchanged: a measurement fitted to
/// fewer is a measurement of one lighting condition, and a weak fingerprint is worse than none
/// because it looks like evidence.
pub const MIN_FINGERPRINT_SAMPLES: u32 = 8;

/// The frames below which a fingerprint is measured but its confidence is reduced.
///
/// Between [`MIN_FINGERPRINT_SAMPLES`] and this, [`CameraCode::FingerprintThin`] is recorded and
/// the confidence falls off linearly. A body that shot fourteen frames of a wedding has a
/// fingerprint; it does not have the same fingerprint as one that shot fourteen hundred.
pub const FULL_FINGERPRINT_SAMPLES: u32 = 60;

/// The fewest frames one shooter needs in one scene class before their habit is measured.
///
/// Section 6.3 measures a *median* offset per scene class, and a median over fewer than twenty
/// samples of something as variable as subject luminance at a wedding is a number that moves when
/// one frame is added.
pub const MIN_SHOOTER_FRAMES: u32 = 20;

/// The most of a measured shooter bias that may be corrected, `0..1`.
///
/// Section 6.3: "cap the correction so a deliberately moodier second shooter is harmonised, not
/// erased." Sixty per cent, which is the PM decision this phase exists to record: two thirds of the
/// way is where a gallery stops looking like two people shot it and a second shooter can still be
/// picked out of a contact sheet by somebody who knows their work. ADR-0053 section 6.
pub const MAX_SHOOTER_SHARE: f32 = 0.60;

/// The most a shooter-habit correction may contribute, in stops, whatever the share works out at.
///
/// A second cap on top of [`MAX_SHOOTER_SHARE`], and the two are not redundant: a share bounds the
/// correction relative to the habit and this bounds it absolutely, so a second shooter who works a
/// stop and a half darker is moved by a third of a stop rather than by nine tenths.
pub const MAX_SHOOTER_EV: f32 = 0.30;

/// The exposure offset below which a shooter's habit is left entirely alone, in stops.
///
/// An eighth of a stop is inside the frame-to-frame variation of one person shooting one wedding,
/// so a "habit" that small is noise in the median rather than a difference in how somebody works.
/// [`CameraCode::ShooterStylePreserved`].
pub const SHOOTER_DEADBAND_EV: f32 = 0.125;

/// The appearance distance below which two cameras are called already matched.
///
/// [`CameraCode::AlreadyMatched`], and it is a claim rather than an absence: two bodies from the
/// same manufacturer, on the same profile, under the same light genuinely do agree, and a product
/// that solved a transform anyway would be moving one of them for no reason.
pub const MATCHED_DISTANCE: f32 = 0.35;

/// The weight of the skin term in [`AppearanceDistance::total`].
///
/// Section 6.2's own number, and the ordering of the four is the argument rather than the values:
/// skin is what a client looks at, the white point is what makes two frames read as one room, the
/// grade signature is the look, and contrast is the least of the four because phase 25 harmonises
/// it again afterwards inside every scene node.
pub const W_SKIN: f32 = 3.0;
/// The weight of the white-point term in [`AppearanceDistance::total`]. Section 6.2.
pub const W_WHITE_POINT: f32 = 1.5;
/// The weight of the grade-signature term in [`AppearanceDistance::total`]. Section 6.2.
pub const W_SIGNATURE: f32 = 1.0;
/// The weight of the contrast term in [`AppearanceDistance::total`]. Section 6.2.
pub const W_CONTRAST: f32 = 0.5;

// ---------------------------------------------------------------------------
// Flash
// ---------------------------------------------------------------------------

/// Which of a body's two colour behaviours a fingerprint or a transform describes.
///
/// Not in section 5's struct listing as a type, only as a field. It is an enum with exactly two
/// variants and no `Unknown`, and the absence is deliberate: a photograph whose EXIF does not say
/// whether the flash fired is [`FlashState::Ambient`], because "we could not tell" and "no flash"
/// produce the same correction and a third population would be a population of two frames that
/// could never be fingerprinted.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum FlashState {
    /// The frame was lit by the room.
    #[default]
    Ambient,
    /// The camera's own flash or a strobe fired.
    Flash,
}

impl FlashState {
    /// Both states, in the order every table in this phase iterates them.
    pub const ALL: [Self; 2] = [Self::Ambient, Self::Flash];

    /// How many there are.
    pub const COUNT: usize = 2;

    /// The stored slug, sent on the wire. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ambient => "ambient",
            Self::Flash => "flash",
        }
    }

    /// Parse a stored slug, defaulting to [`FlashState::Ambient`].
    #[must_use]
    pub fn from_str_or_ambient(s: &str) -> Self {
        match s {
            "flash" => Self::Flash,
            _ => Self::Ambient,
        }
    }

    /// Which state a photograph belongs to, given what EXIF said.
    ///
    /// `None` - the field was absent or unreadable - is [`FlashState::Ambient`]. See the type's
    /// own note: the alternative is a third population nobody can measure.
    #[must_use]
    pub fn of(flash_fired: Option<bool>) -> Self {
        if flash_fired == Some(true) {
            Self::Flash
        } else {
            Self::Ambient
        }
    }
}

impl fmt::Display for FlashState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Brands
// ---------------------------------------------------------------------------

/// The manufacturer a body belongs to, for the bundled baseline lookup only.
///
/// **It is a fallback key and never an input to the solver.** A transform fitted on the wedding's
/// own matched pairs does not consult this; a body whose brand this cannot name still gets a full
/// solved transform, and only the baseline half of a blend is affected. That is why
/// [`Brand::Other`] is a member of the set rather than an error.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Brand {
    /// Sony.
    Sony,
    /// Canon.
    Canon,
    /// Nikon.
    Nikon,
    /// Fujifilm.
    Fujifilm,
    /// Panasonic.
    Panasonic,
    /// OM System, formerly Olympus.
    Olympus,
    /// Leica.
    Leica,
    /// A manufacturer this build has no measured baseline for.
    ///
    /// [`CameraCode::BaselineUnknownBrand`], and the baseline used is the neutral one - an
    /// identity transform - rather than the nearest brand's. Guessing that an unknown body behaves
    /// like a Canon is how a product ships a correction it cannot defend.
    #[default]
    Other,
}

impl Brand {
    /// Every brand, in the order the baselines directory lists them.
    pub const ALL: [Self; 8] = [
        Self::Sony,
        Self::Canon,
        Self::Nikon,
        Self::Fujifilm,
        Self::Panasonic,
        Self::Olympus,
        Self::Leica,
        Self::Other,
    ];

    /// How many there are.
    pub const COUNT: usize = 8;

    /// The stored slug, which is also the baseline file's stem.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sony => "sony",
            Self::Canon => "canon",
            Self::Nikon => "nikon",
            Self::Fujifilm => "fujifilm",
            Self::Panasonic => "panasonic",
            Self::Olympus => "olympus",
            Self::Leica => "leica",
            Self::Other => "neutral",
        }
    }

    /// Parse a stored slug, defaulting to [`Brand::Other`].
    #[must_use]
    pub fn from_str_or_other(s: &str) -> Self {
        match s {
            "sony" => Self::Sony,
            "canon" => Self::Canon,
            "nikon" => Self::Nikon,
            "fujifilm" => Self::Fujifilm,
            "panasonic" => Self::Panasonic,
            "olympus" => Self::Olympus,
            "leica" => Self::Leica,
            _ => Self::Other,
        }
    }

    /// Which brand an EXIF maker string names.
    ///
    /// Case-insensitive and prefix-matched, because the field is written as `NIKON CORPORATION`,
    /// `Canon`, `SONY`, `FUJIFILM` and `OLYMPUS IMAGING CORP.` by the five bodies that write it,
    /// and a table of exact strings is a table that goes stale with every firmware release.
    #[must_use]
    pub fn from_make(make: &str) -> Self {
        let lower = make.trim().to_ascii_lowercase();
        for brand in [
            Self::Sony,
            Self::Canon,
            Self::Nikon,
            Self::Fujifilm,
            Self::Panasonic,
            Self::Leica,
        ] {
            if lower.starts_with(brand.as_str()) {
                return brand;
            }
        }
        if lower.starts_with("olympus") || lower.starts_with("om digital") {
            return Self::Olympus;
        }
        Self::Other
    }
}

impl fmt::Display for Brand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why a camera was matched the way it was, or was not, as a closed set.
///
/// Section 9 gives DOC "document camera matching, when it needs matched pairs, and how to disable
/// it", which is only a finishable job if the codes are enumerable. `docs/camera-matching.md` is
/// written against [`CameraCode::ALL`] and a test asserts every variant appears there.
///
/// **Fifteen of these thirty-two withdraw a claim rather than making one**, and this is the phase
/// where that distinction is worth the most: a body matched from twelve verified pairs of the
/// wedding's own ceremony and a body matched from a bundled baseline measured on somebody else's
/// unit produce transforms of the same shape, and only one of them is evidence about this wedding.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum CameraCode {
    // -- fingerprinting -----------------------------------------------------------------
    /// The body's colour response was measured from this wedding's own frames.
    #[default]
    Fingerprinted,
    /// It was measured from fewer than [`FULL_FINGERPRINT_SAMPLES`] frames, so confidence fell.
    FingerprintThin,
    /// It could not be measured at all: fewer than [`MIN_FINGERPRINT_SAMPLES`] usable frames.
    FingerprintAbsent,
    /// The body's flash and ambient frames were fingerprinted as separate populations.
    FlashSeparated,
    /// One of the two populations was too small to fingerprint, so it uses the brand baseline.
    FlashPopulationThin,

    // -- the reference ------------------------------------------------------------------
    /// This body is the reference; its transform is the identity and nothing about it moves.
    IsReference,
    /// The reference was the body labelled as the primary shooter's.
    ReferenceByShooter,
    /// The reference was the body with the most frames in the gallery.
    ReferenceByFrameCount,
    /// The photographer chose the reference.
    ReferenceByUser,

    // -- evidence -----------------------------------------------------------------------
    /// Matched pairs were found: the two bodies photographed the same conditions.
    PairsFound,
    /// A pair's backgrounds agreed, so the pair is evidence about the cameras.
    PairBackgroundVerified,
    /// A candidate pair was rejected because its backgrounds disagreed.
    ///
    /// The subjects looked alike and the light did not, which means the two frames were shot in
    /// different conditions and their difference is not a camera difference. Section 6.1.
    PairRejectedBackground,
    /// Fewer than [`MIN_MATCHED_PAIRS`] verified pairs, so the answer is blended toward a baseline.
    PairsInsufficient,
    /// No verified pairs at all: this body never photographed the same conditions as the reference.
    PairsAbsent,

    // -- solving ------------------------------------------------------------------------
    /// The transform was solved on this wedding's own matched pairs.
    SolvedFromPairs,
    /// The solved transform was blended with the brand baseline in proportion to the evidence.
    BlendedWithBaseline,
    /// The transform is the brand baseline alone.
    BaselineOnly,
    /// This build has no measured baseline for the body's manufacturer, so the neutral one is used.
    BaselineUnknownBrand,
    /// The transform improved appearance distance on pairs the solver had not seen.
    HeldOutImproved,
    /// It did not, so the brand baseline is used instead and the report says so.
    HeldOutFailed,
    /// A parameter hit its bound and was clamped. [`CameraTransform::bounded`] says which.
    BoundedByPolicy,
    /// A candidate step was refused because it would have put skin outside its plausible locus.
    ///
    /// Phase 15's hard constraint, reused rather than re-derived. Section 6.2.
    SkinLocusRefused,

    // -- what was matched ---------------------------------------------------------------
    /// Skin chromaticity was brought into line with the reference body's.
    SkinMatched,
    /// The white point was brought into line.
    WhitePointMatched,
    /// Saturation, contrast character and roll-off were brought into line.
    GradeMatched,
    /// The two bodies already agreed, so nothing was corrected.
    ///
    /// **A claim, not an absence.** Two bodies of the same brand under the same light genuinely
    /// agree, and correcting one of them anyway would be movement with no evidence behind it.
    AlreadyMatched,

    // -- the shooter --------------------------------------------------------------------
    /// A systematic per-shooter exposure habit was measured and partly corrected.
    ShooterBiasCorrected,
    /// The correction was capped, so the habit is reduced rather than removed. Section 6.3.
    ShooterBiasCapped,
    /// Too few frames in this scene class to measure a habit, so none was corrected.
    ShooterBiasAbsent,
    /// The measured habit was inside [`SHOOTER_DEADBAND_EV`], so it was left entirely alone.
    ShooterStylePreserved,

    // -- the photographer ---------------------------------------------------------------
    /// Matching is switched off for this body.
    Disabled,
    /// The photographer set this transform's values by hand and automation never overwrites them.
    UserEdited,
}

impl CameraCode {
    /// Every code, in the order `docs/camera-matching.md` documents them.
    pub const ALL: [Self; 32] = [
        Self::Fingerprinted,
        Self::FingerprintThin,
        Self::FingerprintAbsent,
        Self::FlashSeparated,
        Self::FlashPopulationThin,
        Self::IsReference,
        Self::ReferenceByShooter,
        Self::ReferenceByFrameCount,
        Self::ReferenceByUser,
        Self::PairsFound,
        Self::PairBackgroundVerified,
        Self::PairRejectedBackground,
        Self::PairsInsufficient,
        Self::PairsAbsent,
        Self::SolvedFromPairs,
        Self::BlendedWithBaseline,
        Self::BaselineOnly,
        Self::BaselineUnknownBrand,
        Self::HeldOutImproved,
        Self::HeldOutFailed,
        Self::BoundedByPolicy,
        Self::SkinLocusRefused,
        Self::SkinMatched,
        Self::WhitePointMatched,
        Self::GradeMatched,
        Self::AlreadyMatched,
        Self::ShooterBiasCorrected,
        Self::ShooterBiasCapped,
        Self::ShooterBiasAbsent,
        Self::ShooterStylePreserved,
        Self::Disabled,
        Self::UserEdited,
    ];

    /// How many codes there are. Exactly the width of the integer a row stores them in.
    pub const COUNT: usize = 32;

    /// The stored slug, sent on the wire. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fingerprinted => "fingerprinted",
            Self::FingerprintThin => "fingerprint_thin",
            Self::FingerprintAbsent => "fingerprint_absent",
            Self::FlashSeparated => "flash_separated",
            Self::FlashPopulationThin => "flash_population_thin",
            Self::IsReference => "is_reference",
            Self::ReferenceByShooter => "reference_by_shooter",
            Self::ReferenceByFrameCount => "reference_by_frame_count",
            Self::ReferenceByUser => "reference_by_user",
            Self::PairsFound => "pairs_found",
            Self::PairBackgroundVerified => "pair_background_verified",
            Self::PairRejectedBackground => "pair_rejected_background",
            Self::PairsInsufficient => "pairs_insufficient",
            Self::PairsAbsent => "pairs_absent",
            Self::SolvedFromPairs => "solved_from_pairs",
            Self::BlendedWithBaseline => "blended_with_baseline",
            Self::BaselineOnly => "baseline_only",
            Self::BaselineUnknownBrand => "baseline_unknown_brand",
            Self::HeldOutImproved => "held_out_improved",
            Self::HeldOutFailed => "held_out_failed",
            Self::BoundedByPolicy => "bounded_by_policy",
            Self::SkinLocusRefused => "skin_locus_refused",
            Self::SkinMatched => "skin_matched",
            Self::WhitePointMatched => "white_point_matched",
            Self::GradeMatched => "grade_matched",
            Self::AlreadyMatched => "already_matched",
            Self::ShooterBiasCorrected => "shooter_bias_corrected",
            Self::ShooterBiasCapped => "shooter_bias_capped",
            Self::ShooterBiasAbsent => "shooter_bias_absent",
            Self::ShooterStylePreserved => "shooter_style_preserved",
            Self::Disabled => "disabled",
            Self::UserEdited => "user_edited",
        }
    }

    /// Parse a stored slug, defaulting to [`CameraCode::Fingerprinted`].
    #[must_use]
    pub fn from_str_or_fingerprinted(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|code| code.as_str() == s)
            .unwrap_or(Self::Fingerprinted)
    }

    /// The sentence a photographer reads. Rendered from the code, never stored.
    ///
    /// Phase 09's rule, inherited for the eighteenth time: a stored sentence is copy a release can
    /// change, and a catalog full of English cannot be translated.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::Fingerprinted => {
                "AURA measured how this camera renders colour, using this wedding's own photographs"
            }
            Self::FingerprintThin => {
                "this camera shot only a few photographs here, so AURA is less certain about how \
                 it renders colour"
            }
            Self::FingerprintAbsent => {
                "this camera shot too few photographs to measure, so AURA has matched it from what \
                 it knows about the brand instead"
            }
            Self::FlashSeparated => {
                "flash and available-light photographs from this camera were matched separately, \
                 because a camera behaves differently under each"
            }
            Self::FlashPopulationThin => {
                "there were too few photographs of one kind - flash or available light - from this \
                 camera, so that half was matched from the brand instead"
            }
            Self::IsReference => {
                "this is the camera everything else is matched to, so nothing about it was changed"
            }
            Self::ReferenceByShooter => "this is the main photographer's camera",
            Self::ReferenceByFrameCount => "this camera shot most of the wedding",
            Self::ReferenceByUser => "you chose this camera as the one to match everything else to",
            Self::PairsFound => {
                "AURA found photographs where both cameras were shooting the same thing under the \
                 same light, and matched them from those"
            }
            Self::PairBackgroundVerified => {
                "the surroundings in both photographs agree, so the two cameras really were in the \
                 same light"
            }
            Self::PairRejectedBackground => {
                "two photographs looked like a pair but their surroundings did not agree, so AURA \
                 did not use them - the light had changed between them"
            }
            Self::PairsInsufficient => {
                "there were not many photographs where both cameras shot the same thing, so AURA \
                 leaned partly on what it knows about the brand"
            }
            Self::PairsAbsent => {
                "these two cameras never photographed the same thing under the same light, so AURA \
                 matched this one from what it knows about the brand"
            }
            Self::SolvedFromPairs => {
                "the correction comes from this wedding's own photographs rather than from a \
                 general setting"
            }
            Self::BlendedWithBaseline => {
                "the correction is part what AURA measured here and part what it knows about the \
                 brand, weighted by how much evidence there was"
            }
            Self::BaselineOnly => {
                "the correction is what AURA knows about this brand, because there was nothing in \
                 this wedding to measure from"
            }
            Self::BaselineUnknownBrand => {
                "AURA has no measurements for this camera's manufacturer, so it has changed \
                 nothing rather than guess"
            }
            Self::HeldOutImproved => {
                "AURA checked the correction against photographs it had not used to work it out, \
                 and they matched better afterwards"
            }
            Self::HeldOutFailed => {
                "the correction did not hold up when AURA checked it against photographs it had \
                 not used, so it fell back on what it knows about the brand"
            }
            Self::BoundedByPolicy => {
                "the correction reached the furthest AURA will move a camera, so it went that far \
                 and no further"
            }
            Self::SkinLocusRefused => {
                "going further would have pushed skin to a colour skin does not come in, so AURA \
                 stopped"
            }
            Self::SkinMatched => "skin from this camera now matches skin from the main camera",
            Self::WhitePointMatched => "whites and greys from the two cameras now agree",
            Self::GradeMatched => {
                "colour richness and contrast from this camera now match the main camera's"
            }
            Self::AlreadyMatched => "these two cameras already agreed, so AURA changed nothing",
            Self::ShooterBiasCorrected => {
                "this photographer consistently exposes differently from the main photographer, \
                 and AURA has brought them partly into line"
            }
            Self::ShooterBiasCapped => {
                "AURA brought this photographer's exposure partly toward the main photographer's \
                 and deliberately stopped short, so their own way of working is still visible"
            }
            Self::ShooterBiasAbsent => {
                "there were not enough photographs of this kind from this photographer to tell \
                 whether they expose differently"
            }
            Self::ShooterStylePreserved => {
                "this photographer's exposure is close enough to the main photographer's that AURA \
                 left it exactly as it was"
            }
            Self::Disabled => "you switched matching off for this camera",
            Self::UserEdited => "you set this camera's correction yourself",
        }
    }

    /// True when this code withdraws a claim rather than making one.
    ///
    /// What the panel greys out and what the report puts under "on what evidence". Fifteen of the
    /// thirty-two; see the type's own note.
    #[must_use]
    pub const fn withdraws(self) -> bool {
        matches!(
            self,
            Self::FingerprintThin
                | Self::FingerprintAbsent
                | Self::FlashPopulationThin
                | Self::PairRejectedBackground
                | Self::PairsInsufficient
                | Self::PairsAbsent
                | Self::BlendedWithBaseline
                | Self::BaselineOnly
                | Self::BaselineUnknownBrand
                | Self::HeldOutFailed
                | Self::BoundedByPolicy
                | Self::SkinLocusRefused
                | Self::ShooterBiasCapped
                | Self::ShooterBiasAbsent
                | Self::Disabled
        )
    }

    /// True when this code says the transform rests on this wedding's own evidence.
    ///
    /// The one question the per-camera report leads with, and it is not the negation of
    /// [`CameraCode::withdraws`]: a bounded transform still rests on measured pairs.
    #[must_use]
    pub const fn is_measured_here(self) -> bool {
        matches!(
            self,
            Self::Fingerprinted | Self::PairsFound | Self::SolvedFromPairs | Self::HeldOutImproved
        )
    }

    /// How much this reason contributes to an ordering, `0..1`.
    ///
    /// Refusals outrank actions, exactly as phases 09 to 25 order theirs: a photographer scanning a
    /// per-camera report needs the sentence about missing evidence above the sentence about what
    /// was corrected, because the second is only meaningful in the light of the first.
    #[must_use]
    pub const fn default_weight(self) -> f32 {
        match self {
            Self::PairsAbsent
            | Self::FingerprintAbsent
            | Self::HeldOutFailed
            | Self::BaselineUnknownBrand => 0.95,
            Self::PairsInsufficient
            | Self::BaselineOnly
            | Self::SkinLocusRefused
            | Self::Disabled
            | Self::UserEdited => 0.85,
            Self::FingerprintThin
            | Self::FlashPopulationThin
            | Self::BlendedWithBaseline
            | Self::BoundedByPolicy
            | Self::ShooterBiasCapped
            | Self::ShooterBiasAbsent
            | Self::PairRejectedBackground => 0.70,
            Self::IsReference | Self::AlreadyMatched | Self::ShooterStylePreserved => 0.55,
            Self::SolvedFromPairs | Self::HeldOutImproved | Self::PairsFound => 0.45,
            Self::SkinMatched | Self::WhitePointMatched | Self::GradeMatched => 0.35,
            Self::ShooterBiasCorrected | Self::FlashSeparated | Self::PairBackgroundVerified => {
                0.25
            }
            Self::Fingerprinted
            | Self::ReferenceByShooter
            | Self::ReferenceByFrameCount
            | Self::ReferenceByUser => 0.15,
        }
    }

    /// This code's bit in the integer a row stores its reason set as.
    #[must_use]
    pub fn bit(self) -> u32 {
        let index = Self::ALL.iter().position(|code| *code == self).unwrap_or(0);
        1u32 << index
    }

    /// Pack a list of codes into one integer.
    #[must_use]
    pub fn to_bits(codes: &[Self]) -> u32 {
        codes.iter().fold(0u32, |acc, code| acc | code.bit())
    }

    /// Read a stored integer back, in [`CameraCode::ALL`] order.
    #[must_use]
    pub fn from_bits(bits: u32) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|code| bits & code.bit() != 0)
            .collect()
    }
}

impl fmt::Display for CameraCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One reason a camera was matched the way it was.
///
/// Section 5 prints `reasons: Vec<Reason>`. The type is named [`CameraReason`] here for the reason
/// every phase since 09 has named its own: `Reason` is already taken by phase 09's integrity
/// vocabulary, and one `Reason` type carrying twenty-six phases' codes would be a type nobody can
/// exhaustively match on. ADR-0053 section 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CameraReason {
    /// What happened.
    pub code: CameraCode,
    /// The sentence, rendered from the code at read time. Never persisted.
    pub text: String,
    /// How much this reason contributed, `0..1`. Ordering in a panel, nothing else.
    pub weight: f32,
}

impl CameraReason {
    /// Build a reason from a code, rendering its sentence.
    #[must_use]
    pub fn new(code: CameraCode, weight: f32) -> Self {
        Self {
            code,
            text: code.user_text().to_string(),
            weight: weight.clamp(0.0, 1.0),
        }
    }

    /// Build a reason from a code at the code's own weight.
    #[must_use]
    pub fn of(code: CameraCode) -> Self {
        Self::new(code, code.default_weight())
    }

    /// Read a stored reason set back, strongest first.
    ///
    /// Ties break on [`CameraCode::ALL`] order, so two runs produce the same list in the same
    /// order. Invariant 4.
    #[must_use]
    pub fn from_bits(bits: u32) -> Vec<Self> {
        let mut out: Vec<Self> = CameraCode::from_bits(bits)
            .into_iter()
            .map(Self::of)
            .collect();
        out.sort_by(|a, b| {
            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.code.cmp(&b.code))
        });
        out
    }

    /// Pack a list of reasons into the integer a row stores.
    #[must_use]
    pub fn to_bits(reasons: &[Self]) -> u32 {
        reasons.iter().fold(0u32, |acc, r| acc | r.code.bit())
    }
}

// ---------------------------------------------------------------------------
// The appearance metric
// ---------------------------------------------------------------------------

/// How far apart two cameras look, decomposed into the four things that make them look apart.
///
/// Section 6.2's objective, as a type rather than as a float, and that is the decision worth
/// stating: a solver that returned one number would leave a per-camera report unable to say *what*
/// was wrong, and "your Sony's skin was two dE00 warm" and "your Sony's shadows were greener" call
/// for completely different responses from a photographer who disagrees with the correction.
///
/// Every component is measured on the **background-verified matched pairs**, never on a single
/// frame, and every component is scaled so that one is a large difference - so
/// [`AppearanceDistance::total`] is comparable across weddings and across brands.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppearanceDistance {
    /// The mean dE00 between the two cameras' skin readings on the pairs.
    ///
    /// In dE00 units rather than normalised, because [`CROSS_CAMERA_DE00_CEILING`] is the KPI and
    /// a normalised number would have to be un-normalised to check it.
    pub skin_de00: f32,
    /// The mean `u'v'` distance between the two cameras' white points, scaled by `0.02`.
    pub white_point: f32,
    /// The mean grade-signature distance, `0..1`. See `NodeTarget::signature_distance`.
    pub grade_signature: f32,
    /// The mean absolute contrast difference, in the recipe's units, scaled by `100`.
    pub contrast: f32,
}

impl AppearanceDistance {
    /// The `u'v'` distance one unit of [`AppearanceDistance::white_point`] represents.
    ///
    /// Two hundredths of `u'v'` is about a 400 K disagreement near daylight, which is where a
    /// photographer starts calling two frames different rather than similar.
    pub const UV_SCALE: f32 = 0.02;

    /// The contrast difference one unit of [`AppearanceDistance::contrast`] represents.
    pub const CONTRAST_SCALE: f32 = 100.0;

    /// The one weighted sum, section 6.2's own formula.
    ///
    /// `3 * skin + 1.5 * white_point + 1.0 * signature + 0.5 * contrast`. It is here rather than in
    /// the solver so the solver, the held-out check, the gate, the report and the panel cannot
    /// disagree about what "closer" means - which is the same argument phase 25 made for putting
    /// `NodeTarget::contains` in the contract.
    #[must_use]
    pub fn total(&self) -> f32 {
        W_SKIN * self.skin_de00
            + W_WHITE_POINT * self.white_point
            + W_SIGNATURE * self.grade_signature
            + W_CONTRAST * self.contrast
    }

    /// True when every component is finite. A distance with a NaN in it is a solver bug.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.skin_de00.is_finite()
            && self.white_point.is_finite()
            && self.grade_signature.is_finite()
            && self.contrast.is_finite()
    }

    /// The share of `self` that `other` removes, `0..1`.
    ///
    /// Zero when `self` is already zero, which is what stops "we measured no difference" reading as
    /// "we removed all of the difference". Phase 25's `cct_spread_reduction` made the same call.
    #[must_use]
    pub fn reduction_to(&self, other: &Self) -> f32 {
        let before = self.total();
        if before <= f32::EPSILON {
            return 0.0;
        }
        (1.0 - other.total() / before).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Fingerprints
// ---------------------------------------------------------------------------

/// How one body renders colour, measured from the wedding's own frames. Section 5, verbatim.
///
/// Five additions sit below section 5's nine fields and each is recorded in ADR-0053 section 2:
/// [`CameraFingerprint::brand`] because the baseline lookup needs a key and re-deriving it from a
/// make string at every read is two answers to the same question;
/// [`CameraFingerprint::grade_signature`] because the objective's third term needs it and phase
/// 16 already stores what it is built from; [`CameraFingerprint::subject_luma`] because the
/// exposure half of the transform is solved against it; and `reasons` plus `analysis_ver` because
/// invariant 2 and phase 06's version rule apply to every decision surface in the product.
///
/// ## What is measured and what is not
///
/// Everything here comes from readings other phases already stored - phase 15's illuminant and
/// subject luminance, phase 16's grade, phase 25's per-identity skin work - and **this phase opens
/// no pixels of its own**. That is invariant 3 read at its strongest: a fingerprint over a
/// four-thousand-frame wedding that decoded four thousand RAWs would spend twenty minutes to
/// produce nine numbers that were already in the catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CameraFingerprint {
    /// The body.
    pub camera_id: CameraId,
    /// Which of its two colour behaviours this describes.
    pub flash: FlashState,
    /// Where this body puts skin, in CIE 1976 `u'v'`.
    ///
    /// The robust central chromaticity of every identity-scoped skin reading from this body, which
    /// is why it is a property of the *camera* and not of any person: a body's skin rendering is
    /// what it does to all of them, and the per-person question is phase 25's.
    pub skin_chroma: [f32; 2],
    /// Where this body puts a neutral, in CIE 1976 `u'v'`.
    pub white_point: [f32; 2],
    /// How saturation responds across four tonal quarters, as multipliers on the reference.
    pub sat_response: [f32; 4],
    /// How contrast responds across four tonal quarters, as multipliers on the reference.
    pub contrast_response: [f32; 4],
    /// How gently the highlights roll off, `0..1`. One is a hard clip.
    pub highlight_rolloff: f32,
    /// How many frames it was measured from.
    pub samples: u32,
    /// How much this fingerprint is worth, `0..1`.
    ///
    /// A product of three terms - the sample count against [`FULL_FINGERPRINT_SAMPLES`], the mean
    /// of the underlying phase 15 white-balance confidences, and how much the samples agree with
    /// each other - because a fingerprint measured from two hundred frames the product was unsure
    /// about is not worth more than one measured from twenty it was sure about.
    pub confidence: f32,
    /// The manufacturer, for the baseline lookup only. Never an input to the solver.
    pub brand: Brand,
    /// The body's colour character, as the eight numbers a distance is measured over.
    pub grade_signature: [f32; 8],
    /// The robust subject luminance this body's frames sit at, `0..1`.
    pub subject_luma: f32,
    /// Why this fingerprint is what it is.
    pub reasons: Vec<CameraReason>,
    /// Which build's arithmetic measured it.
    pub analysis_ver: u16,
}

impl CameraFingerprint {
    /// True when this fingerprint rests on enough frames to be used.
    #[must_use]
    pub const fn is_usable(&self) -> bool {
        self.samples >= MIN_FINGERPRINT_SAMPLES
    }

    /// The confidence a sample count alone justifies, `0..1`.
    ///
    /// Linear from [`MIN_FINGERPRINT_SAMPLES`] to [`FULL_FINGERPRINT_SAMPLES`] and flat after. One
    /// of the three terms [`CameraFingerprint::confidence`] is a product of, exposed so the
    /// fingerprint module and the panel agree about where "thin" ends.
    #[must_use]
    pub fn sample_weight(samples: u32) -> f32 {
        if samples < MIN_FINGERPRINT_SAMPLES {
            return 0.0;
        }
        if samples >= FULL_FINGERPRINT_SAMPLES {
            return 1.0;
        }
        let span = f64::from(FULL_FINGERPRINT_SAMPLES - MIN_FINGERPRINT_SAMPLES);
        let over = f64::from(samples - MIN_FINGERPRINT_SAMPLES);
        #[allow(clippy::cast_possible_truncation)]
        {
            (over / span) as f32
        }
    }
}

// ---------------------------------------------------------------------------
// The skin half of a transform
// ---------------------------------------------------------------------------

/// What a camera transform does to skin, and why it did not do more.
///
/// Section 5 prints `skin_correction: SkinCorrection` without defining the type. It is
/// **camera-scoped**, which makes it a different type from `gallery::SkinCorrection` even though
/// they share a name across two modules: this one carries no identity, because a camera does not
/// render one person's skin differently from another's, and phase 25's carries one because a
/// *gallery* does drift per person. Conflating them would produce a per-camera correction that
/// varies by who is in the frame, which is a correction to the wrong thing.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkinCorrection {
    /// How far skin chromaticity moves, in CIE 1976 `u'v'`. Bounded by [`SKIN_UV_CAP`].
    pub d_uv: [f32; 2],
    /// How far skin luminance moves, `0..1`. Bounded by [`SKIN_LUMA_CAP`].
    pub d_luma: f32,
    /// The cross-camera skin dE00 on the fitting pairs before this correction.
    pub de00_before: f32,
    /// The cross-camera skin dE00 after it.
    ///
    /// Section 10.1's first gate is `SELECT MAX(de00_after)` over a project's transforms rather
    /// than a sentence in a document. Phase 16's rule, sixth application: a guarantee is measured,
    /// not asserted.
    pub de00_after: f32,
    /// True when phase 15's skin locus admitted the corrected chromaticity.
    ///
    /// False means the solver stopped short of what the metric wanted, which is
    /// [`CameraCode::SkinLocusRefused`]. It is stored rather than derived because a photographer
    /// looking at a residual difference needs to know it was a refusal and not a failure.
    pub locus_valid: bool,
    /// True when a cap bit.
    pub capped: bool,
}

impl SkinCorrection {
    /// True when this correction meets the phase's headline promise.
    #[must_use]
    pub fn meets_promise(&self) -> bool {
        self.de00_after <= CROSS_CAMERA_DE00_CEILING
    }

    /// How much of the skin difference this correction closed, `0..1`.
    #[must_use]
    pub fn closed(&self) -> f32 {
        if self.de00_before <= f32::EPSILON {
            return 1.0;
        }
        (1.0 - self.de00_after / self.de00_before).clamp(0.0, 1.0)
    }

    /// True when both movements are inside their contract caps.
    #[must_use]
    pub fn within_caps(&self) -> bool {
        let uv = (self.d_uv[0] * self.d_uv[0] + self.d_uv[1] * self.d_uv[1]).sqrt();
        uv <= SKIN_UV_CAP + f32::EPSILON && self.d_luma.abs() <= SKIN_LUMA_CAP + f32::EPSILON
    }
}

// ---------------------------------------------------------------------------
// Transforms
// ---------------------------------------------------------------------------

/// Where a transform's numbers came from.
///
/// Section 5's comment lists two. Three ship, and the third is the honest one: a transform fitted
/// on eight pairs and pulled two thirds of the way toward a bundled baseline is neither "solved
/// from this wedding" nor "the brand's baseline", and a per-camera report whose job is to say what
/// was corrected and on what evidence cannot round either way. ADR-0053 section 3.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum TransformSource {
    /// Solved on at least [`MIN_MATCHED_PAIRS`] verified pairs from this wedding.
    MatchedPairs,
    /// Part solved, part bundled baseline. [`CameraTransform::blend`] carries the share.
    Blended,
    /// The bundled brand baseline alone.
    #[default]
    BrandBaseline,
}

impl TransformSource {
    /// Every source, in increasing order of how much this wedding contributed.
    pub const ALL: [Self; 3] = [Self::BrandBaseline, Self::Blended, Self::MatchedPairs];

    /// How many there are.
    pub const COUNT: usize = 3;

    /// The stored slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MatchedPairs => "matched_pairs",
            Self::Blended => "blended",
            Self::BrandBaseline => "brand_baseline",
        }
    }

    /// Parse a stored slug, defaulting to the most cautious answer.
    #[must_use]
    pub fn from_str_or_baseline(s: &str) -> Self {
        match s {
            "matched_pairs" => Self::MatchedPairs,
            "blended" => Self::Blended,
            _ => Self::BrandBaseline,
        }
    }

    /// True when this wedding's own frames contributed anything at all.
    #[must_use]
    pub const fn measured_here(self) -> bool {
        matches!(self, Self::MatchedPairs | Self::Blended)
    }
}

impl fmt::Display for TransformSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which bound stopped a transform going further.
///
/// Not in section 5. An enum rather than a boolean, for phase 25's reason: "this camera was
/// clamped" and "this camera was clamped *in temperature*" are different facts, and only the
/// second tells a photographer whether the bound is wrong or the pairing is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum TransformBound {
    /// [`MAX_T_CCT_K`].
    #[default]
    Cct,
    /// [`MAX_T_TINT`].
    Tint,
    /// [`MAX_T_EXPOSURE_EV`].
    Exposure,
    /// [`MAX_CHANNEL_GAIN`].
    ChannelGain,
    /// [`MAX_T_SATURATION`].
    Saturation,
    /// [`MAX_CONTRAST_SHAPE`].
    ContrastShape,
    /// [`SKIN_UV_CAP`] or [`SKIN_LUMA_CAP`].
    Skin,
}

impl TransformBound {
    /// Every bound, in the order a transform's fields are declared.
    pub const ALL: [Self; 7] = [
        Self::Cct,
        Self::Tint,
        Self::Exposure,
        Self::ChannelGain,
        Self::Saturation,
        Self::ContrastShape,
        Self::Skin,
    ];

    /// How many there are.
    pub const COUNT: usize = 7;

    /// The stored slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cct => "cct",
            Self::Tint => "tint",
            Self::Exposure => "exposure",
            Self::ChannelGain => "channel_gain",
            Self::Saturation => "saturation",
            Self::ContrastShape => "contrast_shape",
            Self::Skin => "skin",
        }
    }

    /// Parse a stored slug, defaulting to [`TransformBound::Cct`].
    #[must_use]
    pub fn from_str_or_cct(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|bound| bound.as_str() == s)
            .unwrap_or(Self::Cct)
    }

    /// The contract's ceiling on this axis.
    ///
    /// The one place the seven numbers are looked up, so the solver, the config loader, the SQL
    /// CHECK generator and the panel cannot disagree about where a ceiling is. Phase 25's `Bound`
    /// established the shape.
    #[must_use]
    pub const fn ceiling(self) -> f32 {
        match self {
            Self::Cct => MAX_T_CCT_K,
            Self::Tint => MAX_T_TINT,
            Self::Exposure => MAX_T_EXPOSURE_EV,
            Self::ChannelGain => MAX_CHANNEL_GAIN,
            Self::Saturation => MAX_T_SATURATION,
            Self::ContrastShape => MAX_CONTRAST_SHAPE,
            Self::Skin => SKIN_UV_CAP,
        }
    }
}

impl fmt::Display for TransformBound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The correction one body needs to look like the reference body. Section 5, verbatim.
///
/// Eleven additions sit below section 5's fourteen fields and every one is recorded in ADR-0053
/// section 2. The four distance numbers are the largest of them and they are the phase's evidence:
/// without `heldout_before` and `heldout_after` on the row, section 6.2's "verify on held-out pairs"
/// is a claim about a function nobody can check afterwards.
///
/// ## A transform is a residual and the schema cannot express an absolute
///
/// Every `d_` field is a movement on top of what phases 15 and 16 already decided for a frame, and
/// there is no field here that says what temperature a frame *is*. A caller that adds
/// [`CameraTransform::d_cct`] to anything but a frame's own solved temperature has misunderstood
/// the shape. Phase 25 wrote this rule for its own deltas and this is its second application; the
/// two compose, in that order, and [`CameraTransform`] is the one that runs first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CameraTransform {
    /// The body being corrected.
    pub camera_id: CameraId,
    /// Which of its two colour behaviours this corrects.
    pub flash: FlashState,
    /// The body it is being corrected toward.
    pub reference: CameraId,
    /// How far its temperature moves, in kelvin. Bounded by [`MAX_T_CCT_K`].
    pub d_cct: f32,
    /// How far its tint moves. Bounded by [`MAX_T_TINT`].
    pub d_tint: f32,
    /// How far its exposure moves, in stops. Bounded by [`MAX_T_EXPOSURE_EV`].
    ///
    /// **This carries the shooter-habit correction as well as the metering difference**, because
    /// section 6.3 says gear and habit are entangled in practice and two exposure offsets on one
    /// frame is two things that have to be added up somewhere. [`ShooterBias`] records the habit
    /// half separately so the report can say which part of the number is which.
    pub d_exposure: f32,
    /// Per-channel linear gain in the working space, as multipliers around one.
    ///
    /// Each within `1 ± ` [`MAX_CHANNEL_GAIN`]. This is the axis that corrects what a white balance
    /// cannot: two bodies can agree about a neutral and still disagree about a saturated red,
    /// because their forward matrices differ off the neutral axis.
    pub channel_gain: [f32; 3],
    /// How far its saturation moves. Bounded by [`MAX_T_SATURATION`].
    pub d_saturation: f32,
    /// Multipliers on the shadow, mid and highlight thirds. Each within `1 ± ` [`MAX_CONTRAST_SHAPE`].
    pub contrast_shape: [f32; 3],
    /// What it does to skin, and why it did not do more.
    pub skin_correction: SkinCorrection,
    /// How many verified pairs it was fitted on. Zero for a pure baseline.
    pub evidence_pairs: u32,
    /// Where the numbers came from.
    pub source: TransformSource,
    /// How much this transform is worth, `0..1`.
    pub confidence: f32,
    /// Why it is what it is.
    pub reasons: Vec<CameraReason>,
    /// The share of the solved answer in the blend, `0..1`. One is fully solved, zero fully baseline.
    pub blend: f32,
    /// The appearance distance on the fitting pairs before this transform.
    pub distance_before: AppearanceDistance,
    /// The appearance distance on the fitting pairs after it.
    pub distance_after: AppearanceDistance,
    /// The appearance distance on the held-out pairs before it.
    pub heldout_before: AppearanceDistance,
    /// The appearance distance on the held-out pairs after it.
    ///
    /// Equal to [`CameraTransform::heldout_before`] when there were fewer than
    /// [`MIN_HELDOUT_PAIRS`] to check against, which is a stated absence rather than a passing
    /// grade - and [`CameraTransform::heldout_checked`] is how a caller tells the two apart.
    pub heldout_after: AppearanceDistance,
    /// How many pairs were held out of the fit.
    pub heldout_pairs: u32,
    /// Which bound stopped it going further, when one did.
    pub bounded: Option<TransformBound>,
    /// True when matching is switched on for this body.
    pub enabled: bool,
    /// True when the photographer set these values by hand.
    pub user_edited: bool,
    /// Which build's arithmetic solved it.
    pub analysis_ver: u16,
    /// Which policy table bounded it.
    pub policy_ver: u16,
}

impl CameraTransform {
    /// The transform that changes nothing, for the reference body and for a refusal.
    ///
    /// The same value in both cases and that is deliberate: what separates them is the reason set,
    /// not the numbers, and a caller that had to tell them apart by looking at the arithmetic would
    /// be inferring a decision from a coincidence.
    #[must_use]
    pub fn identity(
        camera_id: CameraId,
        flash: FlashState,
        reference: CameraId,
        analysis_ver: u16,
        policy_ver: u16,
    ) -> Self {
        Self {
            camera_id,
            flash,
            reference,
            d_cct: 0.0,
            d_tint: 0.0,
            d_exposure: 0.0,
            channel_gain: [1.0; 3],
            d_saturation: 0.0,
            contrast_shape: [1.0; 3],
            skin_correction: SkinCorrection {
                locus_valid: true,
                ..SkinCorrection::default()
            },
            evidence_pairs: 0,
            source: TransformSource::BrandBaseline,
            confidence: 0.0,
            reasons: Vec::new(),
            blend: 0.0,
            distance_before: AppearanceDistance::default(),
            distance_after: AppearanceDistance::default(),
            heldout_before: AppearanceDistance::default(),
            heldout_after: AppearanceDistance::default(),
            heldout_pairs: 0,
            bounded: None,
            enabled: true,
            user_edited: false,
            analysis_ver,
            policy_ver,
        }
    }

    /// True when this transform moves nothing.
    ///
    /// Note what it does **not** consult: the reason set. A body that is the reference, a body that
    /// was already matched and a body whose brand has no baseline all produce zeroes here, and they
    /// are three completely different statements. Phase 24's rule, and the codes are how they are
    /// told apart.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        const EPS: f32 = 1e-4;
        self.d_cct.abs() < EPS
            && self.d_tint.abs() < EPS
            && self.d_exposure.abs() < EPS
            && self.d_saturation.abs() < EPS
            && self.channel_gain.iter().all(|g| (g - 1.0).abs() < EPS)
            && self.contrast_shape.iter().all(|c| (c - 1.0).abs() < EPS)
            && self.skin_correction.d_luma.abs() < EPS
            && self.skin_correction.d_uv.iter().all(|v| v.abs() < EPS)
    }

    /// True when every axis is inside its contract ceiling.
    ///
    /// Checked before a row is written, and checked again by migration 26's CHECK constraints. Two
    /// layers, for phase 25's reason: the first lives in Rust a future caller could route around
    /// with a raw INSERT, and section 10.1 makes "no camera exceeds documented maximum movement" a
    /// gate that ought to be unviolatable rather than merely measurable.
    #[must_use]
    pub fn within_bounds(&self) -> bool {
        let finite = self.d_cct.is_finite()
            && self.d_tint.is_finite()
            && self.d_exposure.is_finite()
            && self.d_saturation.is_finite()
            && self.channel_gain.iter().all(|g| g.is_finite())
            && self.contrast_shape.iter().all(|c| c.is_finite());
        finite
            && self.d_cct.abs() <= MAX_T_CCT_K + f32::EPSILON
            && self.d_tint.abs() <= MAX_T_TINT + f32::EPSILON
            && self.d_exposure.abs() <= MAX_T_EXPOSURE_EV + f32::EPSILON
            && self.d_saturation.abs() <= MAX_T_SATURATION + f32::EPSILON
            && self
                .channel_gain
                .iter()
                .all(|g| (g - 1.0).abs() <= MAX_CHANNEL_GAIN + f32::EPSILON)
            && self
                .contrast_shape
                .iter()
                .all(|c| (c - 1.0).abs() <= MAX_CONTRAST_SHAPE + f32::EPSILON)
            && self.skin_correction.within_caps()
    }

    /// True when a held-out check actually ran.
    #[must_use]
    pub const fn heldout_checked(&self) -> bool {
        self.heldout_pairs >= MIN_HELDOUT_PAIRS
    }

    /// True when the held-out check passed, or `None` when it did not run.
    ///
    /// `Option<bool>` rather than `bool`, because "we checked and it improved", "we checked and it
    /// did not" and "we could not check" are three states and collapsing the third into either of
    /// the first two is how a product claims verification it did not do.
    #[must_use]
    pub fn heldout_improved(&self) -> Option<bool> {
        if !self.heldout_checked() {
            return None;
        }
        let before = self.heldout_before.total();
        if before <= f32::EPSILON {
            return Some(true);
        }
        Some(self.heldout_before.reduction_to(&self.heldout_after) >= MIN_HELDOUT_IMPROVEMENT)
    }

    /// How far this transform moves the body, `0..1`, as the worst of its axes.
    ///
    /// The **worst** rather than the mean, for phase 25's reason: a transform at its temperature
    /// ceiling and nowhere else has moved a body as far as this product will move one, and
    /// averaging that against five zeroes reports it as a sixth of a correction.
    #[must_use]
    pub fn magnitude(&self) -> f32 {
        let gain = self
            .channel_gain
            .iter()
            .map(|g| (g - 1.0).abs() / MAX_CHANNEL_GAIN)
            .fold(0.0_f32, f32::max);
        let shape = self
            .contrast_shape
            .iter()
            .map(|c| (c - 1.0).abs() / MAX_CONTRAST_SHAPE)
            .fold(0.0_f32, f32::max);
        [
            self.d_cct.abs() / MAX_T_CCT_K,
            self.d_tint.abs() / MAX_T_TINT,
            self.d_exposure.abs() / MAX_T_EXPOSURE_EV,
            self.d_saturation.abs() / MAX_T_SATURATION,
            gain,
            shape,
        ]
        .into_iter()
        .fold(0.0_f32, f32::max)
        .clamp(0.0, 1.0)
    }

    /// True when this transform meets the phase's headline skin promise.
    #[must_use]
    pub fn meets_skin_promise(&self) -> bool {
        self.skin_correction.meets_promise()
    }

    /// The share of the appearance distance this transform removed on the fitting pairs, `0..1`.
    #[must_use]
    pub fn distance_reduction(&self) -> f32 {
        self.distance_before.reduction_to(&self.distance_after)
    }

    /// The share of the grade-signature distance it removed, `0..1`.
    ///
    /// Section 10.1's second gate, computed here rather than in the harness so the panel, the gate
    /// and the exit report cannot disagree.
    #[must_use]
    pub fn signature_reduction(&self) -> f32 {
        let before = self.distance_before.grade_signature;
        if before <= f32::EPSILON {
            return 0.0;
        }
        (1.0 - self.distance_after.grade_signature / before).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Matched pairs
// ---------------------------------------------------------------------------

/// Two frames, from two bodies, of the same conditions - the evidence a transform is solved on.
///
/// Not in section 5. Section 6.1 makes them "the gold standard" and section 4 gives them a table,
/// so they are a first-class shape rather than a solver-internal tuple: the matched-pair viewer in
/// section 9's SFE row shows them, a photographer who disagrees with a correction needs to see what
/// it was fitted on, and a pair that was rejected is as informative as one that was kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MatchedPair {
    /// This pair.
    pub id: PairId,
    /// The scene node both frames belong to. Phase 25's tree is the pairing's outer key.
    pub node: NodeId,
    /// The reference body's frame.
    pub left: ImageId,
    /// The other body's frame.
    pub right: ImageId,
    /// The reference body.
    pub left_camera: CameraId,
    /// The body being matched.
    pub right_camera: CameraId,
    /// The flash state both frames share. A pair across states is never formed.
    pub flash: FlashState,
    /// How far apart in time they were taken, in milliseconds. Within [`MAX_PAIR_GAP_MS`].
    pub gap_ms: i64,
    /// How alike their subjects are, `0..1`. Phase 05's cosine similarity.
    pub subject_similarity: f32,
    /// How much their **backgrounds** agree, `0..1`.
    ///
    /// The number that decides. See [`MIN_BACKGROUND_AGREEMENT`] - scoring the pair on the subject
    /// would be scoring the thing under test.
    pub background_agreement: f32,
    /// True when the pair passed background verification.
    pub verified: bool,
    /// True when this pair was held out of the fit and used only to check it.
    pub held_out: bool,
    /// Which build's arithmetic formed it.
    pub analysis_ver: u16,
}

impl MatchedPair {
    /// True when this pair is evidence a solver may fit on.
    #[must_use]
    pub const fn is_fitting(&self) -> bool {
        self.verified && !self.held_out
    }

    /// True when this pair is evidence a solver may only be checked against.
    #[must_use]
    pub const fn is_heldout(&self) -> bool {
        self.verified && self.held_out
    }
}

// ---------------------------------------------------------------------------
// Shooters
// ---------------------------------------------------------------------------

/// How one photographer's exposure habit differs from the reference photographer's, per scene class.
///
/// Not in section 5; section 6.3 requires it. A habit is measured as a **median subject-luminance
/// offset within one scene class**, and both halves of that matter: a median because a wedding
/// contains a handful of deliberately dark frames that would drag a mean, and per scene class
/// because a second shooter who works darker during a ceremony may not during a reception, and one
/// number for both is a number that is wrong twice.
///
/// **The measured offset and the applied correction are separate fields.** That is the whole
/// mechanism of section 6.3's cap: a report that only stored what was applied could not tell a
/// photographer that their second shooter is two thirds of a stop darker and has been moved by a
/// third of one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ShooterBias {
    /// The shooter's label, from the catalog's `camera.shooter_label`.
    pub shooter: String,
    /// The body the label belongs to.
    ///
    /// One shooter may carry two bodies and this is per body, because a habit measured across two
    /// bodies is a habit confounded with whichever body they reached for in a dark room.
    pub camera_id: CameraId,
    /// The scene class the habit was measured in.
    pub scene: SceneId,
    /// The systematic subject-luminance offset that was measured, in stops.
    ///
    /// Positive means this shooter exposes brighter than the reference.
    pub measured_ev: f32,
    /// The exposure correction that is applied, in stops.
    ///
    /// **It opposes [`ShooterBias::measured_ev`] in sign**, because it is the correction rather
    /// than the habit: a shooter who works a third of a stop darker has `measured_ev = -0.33` and
    /// receives `applied_ev = +0.20`. Storing them with the same sign would read fine and produce a
    /// gallery in which the second shooter's frames are pushed *further* from the lead's, which is
    /// the one arithmetic mistake in this phase that would look like the feature working.
    ///
    /// Never more than [`MAX_SHOOTER_SHARE`] of the measured offset in magnitude, and never more
    /// than [`MAX_SHOOTER_EV`] in absolute terms.
    pub applied_ev: f32,
    /// How many frames the median was taken over.
    pub frames: u32,
    /// True when a cap bit, so the habit is reduced rather than removed.
    pub capped: bool,
    /// Why this is what it is.
    pub reasons: Vec<CameraReason>,
    /// Which build's arithmetic measured it.
    pub analysis_ver: u16,
}

impl ShooterBias {
    /// The correction a measured offset justifies, in stops, after both caps.
    ///
    /// The one place section 6.3's cap is applied, so the solver, the report, the gate and the
    /// panel cannot disagree about how much of a habit survives. Returns zero inside
    /// [`SHOOTER_DEADBAND_EV`], which is [`CameraCode::ShooterStylePreserved`] rather than a
    /// correction of zero - the codes are what tell the two apart.
    ///
    /// **The sign is flipped**: a habit is a description of where somebody's exposures sit and a
    /// correction moves them the other way. See [`ShooterBias::applied_ev`].
    #[must_use]
    pub fn correction_for(measured_ev: f32) -> f32 {
        if !measured_ev.is_finite() || measured_ev.abs() < SHOOTER_DEADBAND_EV {
            return 0.0;
        }
        let shared = -measured_ev * MAX_SHOOTER_SHARE;
        shared.clamp(-MAX_SHOOTER_EV, MAX_SHOOTER_EV)
    }

    /// True when the correction is smaller than the measured habit, whatever the reason.
    #[must_use]
    pub fn is_capped(&self) -> bool {
        self.applied_ev.abs() + f32::EPSILON < self.measured_ev.abs()
    }

    /// True when there is enough evidence for this row to constrain anything.
    #[must_use]
    pub const fn is_usable(&self) -> bool {
        self.frames >= MIN_SHOOTER_FRAMES
    }
}

// ---------------------------------------------------------------------------
// The reference camera
// ---------------------------------------------------------------------------

/// How the reference body was chosen.
///
/// Section 2.1's three policies, and the order here is the order they are tried: a photographer's
/// choice beats a shooter label, which beats a frame count. Section 9 gives PM "decide default
/// reference-camera policy", and this is the decision.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceSource {
    /// The photographer chose it.
    User,
    /// It is the body labelled as the primary shooter's.
    #[default]
    PrimaryShooter,
    /// It shot the most frames in the gallery.
    FrameCount,
}

impl ReferenceSource {
    /// Every source, in the order they are tried.
    pub const ALL: [Self; 3] = [Self::User, Self::PrimaryShooter, Self::FrameCount];

    /// How many there are.
    pub const COUNT: usize = 3;

    /// The stored slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::PrimaryShooter => "primary_shooter",
            Self::FrameCount => "frame_count",
        }
    }

    /// Parse a stored slug, defaulting to [`ReferenceSource::PrimaryShooter`].
    #[must_use]
    pub fn from_str_or_shooter(s: &str) -> Self {
        match s {
            "user" => Self::User,
            "frame_count" => Self::FrameCount,
            _ => Self::PrimaryShooter,
        }
    }

    /// The reason code that says how a reference was chosen.
    #[must_use]
    pub const fn code(self) -> CameraCode {
        match self {
            Self::User => CameraCode::ReferenceByUser,
            Self::PrimaryShooter => CameraCode::ReferenceByShooter,
            Self::FrameCount => CameraCode::ReferenceByFrameCount,
        }
    }
}

impl fmt::Display for ReferenceSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which body a project is matched to, and how that was decided.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Reference {
    /// The project.
    pub project: ProjectId,
    /// The body everything else is matched toward.
    pub camera_id: CameraId,
    /// How it was chosen.
    pub source: ReferenceSource,
    /// How many frames it shot.
    pub frames: u32,
    /// Its shooter label, when it has one.
    pub shooter: Option<String>,
}

// ---------------------------------------------------------------------------
// The outline
// ---------------------------------------------------------------------------

/// What a project's matching pass covered and what it found.
///
/// Not in section 5. Phase 05's rule, inherited for the twentieth time: report coverage when you
/// report a result, and say what the denominator is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CameraOutline {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs whose body carries a transform - including the reference body's own frames.
    pub matched: u32,
    /// [`CameraOutline::matched`] over [`CameraOutline::photos`], `0..1`.
    ///
    /// The denominator is **every photograph**, as phases 09, 10, 15 and 25 count it. A photograph
    /// whose body could not be identified is a gap in this pass whatever caused it.
    pub coverage: f32,
    /// Bodies seen in the project.
    pub cameras: u32,
    /// Bodies with a fingerprint measured from this wedding's own frames.
    pub fingerprinted: u32,
    /// Transforms solved on this wedding's own matched pairs.
    ///
    /// The second number, and the one that matters when it is low: a project with four cameras and
    /// zero solved transforms has been matched entirely from bundled baselines, which is a
    /// completely different claim from a project matched on its own ceremony.
    pub solved_from_pairs: u32,
    /// Transforms blended between the two.
    pub blended: u32,
    /// Transforms that are a bundled baseline alone.
    pub baseline_only: u32,
    /// Verified matched pairs found.
    pub pairs: u32,
    /// Candidate pairs rejected because their backgrounds disagreed.
    pub pairs_rejected: u32,
    /// Pairs held out of every fit.
    pub heldout_pairs: u32,
    /// Bodies whose flash and ambient populations were both fingerprinted.
    pub flash_separated: u32,
    /// Shooters whose exposure habit was measured.
    pub shooters_measured: u32,
    /// Shooter-habit corrections that a cap reduced.
    pub shooters_capped: u32,
    /// Bodies a photographer switched matching off for.
    pub disabled: u32,
    /// Bodies a photographer set by hand.
    pub user_edited: u32,
    /// The mean cross-camera skin dE00 across the project's transforms, before matching.
    ///
    /// Section 10.1's headline gate is the fall from this to
    /// [`CameraOutline::skin_de00_after`], and both are on the outline so the claim is a
    /// subtraction a panel can show rather than a number only the harness knows. Phase 25's shape.
    pub skin_de00_before: f32,
    /// The same after matching.
    pub skin_de00_after: f32,
    /// The worst per-camera skin dE00 after matching.
    ///
    /// The promise as a query: at or below [`CROSS_CAMERA_DE00_CEILING`] it holds for every body in
    /// the project. A mean can meet a ceiling that one body misses badly.
    pub worst_skin_de00: f32,
    /// The mean grade-signature distance between the bodies and the reference, before matching.
    pub signature_before: f32,
    /// The same after matching.
    pub signature_after: f32,
    /// The reference body, when one was chosen.
    pub reference: Option<CameraId>,
    /// How the reference was chosen.
    pub reference_source: ReferenceSource,
    /// Bodies whose manufacturer this build has no measured baseline for.
    pub unknown_brands: Vec<String>,
    /// Which build's arithmetic produced these numbers.
    pub analysis_ver: u16,
    /// Which policy table bounded them.
    pub policy_ver: u16,
}

impl CameraOutline {
    /// The share of the cross-camera skin difference that matching removed, `0..1`.
    #[must_use]
    pub fn skin_reduction(&self) -> f32 {
        if self.skin_de00_before <= f32::EPSILON {
            return 0.0;
        }
        (1.0 - self.skin_de00_after / self.skin_de00_before).clamp(0.0, 1.0)
    }

    /// The share of the grade-signature distance that matching removed, `0..1`.
    ///
    /// Section 10.1's second gate: "grade-signature distance between cameras reduced >= 65 %".
    #[must_use]
    pub fn signature_reduction(&self) -> f32 {
        if self.signature_before <= f32::EPSILON {
            return 0.0;
        }
        (1.0 - self.signature_after / self.signature_before).clamp(0.0, 1.0)
    }

    /// True when every body in the project meets the headline skin promise.
    #[must_use]
    pub fn meets_skin_promise(&self) -> bool {
        self.worst_skin_de00 <= CROSS_CAMERA_DE00_CEILING
    }

    /// The share of bodies whose transform rests on this wedding's own evidence, `0..1`.
    #[must_use]
    pub fn measured_share(&self) -> f32 {
        if self.cameras == 0 {
            return 0.0;
        }
        f64::from(self.solved_from_pairs + self.blended) as f32 / f64::from(self.cameras) as f32
    }
}

// ---------------------------------------------------------------------------
// Overrides
// ---------------------------------------------------------------------------

/// What a photographer set instead, for one body in one flash state.
///
/// Every field is optional and at least one must be present; an empty override is refused rather
/// than silently accepted, because an empty override that set `user_edited` would take a body out
/// of automation without changing anything about it. Phase 25's shape.
///
/// **There is no way to raise a bound and no strength field.** Phase 21's rule - a ceiling can be
/// lowered by a studio and raised by nobody - applied to the surface a photographer touches. A body
/// that needs to move further than [`MAX_T_CCT_K`] is a body whose per-frame estimates are wrong,
/// and phase 15's own override is where that is fixed.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CameraOverride {
    /// The temperature movement to use instead, in kelvin, within [`MAX_T_CCT_K`].
    pub d_cct: Option<f32>,
    /// The tint movement to use instead, within [`MAX_T_TINT`].
    pub d_tint: Option<f32>,
    /// The exposure movement to use instead, in stops, within [`MAX_T_EXPOSURE_EV`].
    pub d_exposure: Option<f32>,
    /// The saturation movement to use instead, within [`MAX_T_SATURATION`].
    pub d_saturation: Option<f32>,
}

impl CameraOverride {
    /// True when nothing was set.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.d_cct.is_none()
            && self.d_tint.is_none()
            && self.d_exposure.is_none()
            && self.d_saturation.is_none()
    }

    /// True when every value that was set is inside its contract ceiling.
    #[must_use]
    pub fn within_bounds(&self) -> bool {
        let ok = |value: Option<f32>, ceiling: f32| {
            value.is_none_or(|v| v.is_finite() && v.abs() <= ceiling + f32::EPSILON)
        };
        ok(self.d_cct, MAX_T_CCT_K)
            && ok(self.d_tint, MAX_T_TINT)
            && ok(self.d_exposure, MAX_T_EXPOSURE_EV)
            && ok(self.d_saturation, MAX_T_SATURATION)
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask how a body was matched to the rest of a wedding.
///
/// Twenty-second service of its kind. Phase 27 reads these transforms when it asks why two frames
/// of the same room disagree, phase 28 acts on them unattended, phase 29 builds albums from a
/// gallery they have already made comparable and phase 30 exports it. No phase may keep its own
/// camera fingerprint, its own pairing or its own idea of what two bodies matching means - two
/// answers to "what does this Sony need" is a gallery in which the album and the web set disagree
/// about the second shooter.
///
/// **Nothing here writes a recipe or moves a pixel.** There is no `apply` on this trait and adding
/// one would need an ADR. The transforms are stored; `aura_brain_gallery` reads them before it
/// builds its tree, and `aura_recipe::schema::merge` is the one function permitted to write a
/// recipe.
pub trait CameraMatchService: Send + Sync + fmt::Debug {
    /// What a project's matching pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<CameraOutline>;

    /// Every fingerprint in a project, by body and then by flash state.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the fingerprints cannot be read.
    fn fingerprints(&self, project: ProjectId) -> AuraResult<Vec<CameraFingerprint>>;

    /// One body's fingerprint in one flash state, or `None` when it has none.
    ///
    /// `None` is not a neutral fingerprint, and a caller that renders it as one has turned a gap in
    /// evidence into a measurement.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the fingerprint cannot be read.
    fn fingerprint(
        &self,
        project: ProjectId,
        camera: &CameraId,
        flash: FlashState,
    ) -> AuraResult<Option<CameraFingerprint>>;

    /// Every transform in a project, by body and then by flash state.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the transforms cannot be read.
    fn transforms(&self, project: ProjectId) -> AuraResult<Vec<CameraTransform>>;

    /// One body's transform in one flash state, or `None` when it has none.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the transform cannot be read.
    fn transform(
        &self,
        project: ProjectId,
        camera: &CameraId,
        flash: FlashState,
    ) -> AuraResult<Option<CameraTransform>>;

    /// The transform that applies to one photograph, or `None` when none does.
    ///
    /// **This is what phase 25 reads**, once per frame, before it builds its tree. It resolves the
    /// photograph's body and flash state and returns the matching row; a disabled body returns
    /// `None` rather than an identity, so a caller cannot confuse "matching is off here" with
    /// "matching found nothing to do".
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read.
    fn transform_for_image(&self, image: ImageId) -> AuraResult<Option<CameraTransform>>;

    /// The verified matched pairs behind one body's transform, best first.
    ///
    /// What the matched-pair viewer shows. `limit` bounds the answer because the viewer is a page.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the pairs cannot be read.
    fn pairs(
        &self,
        project: ProjectId,
        camera: &CameraId,
        limit: usize,
    ) -> AuraResult<Vec<MatchedPair>>;

    /// Every measured shooter habit in a project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn shooter_bias(&self, project: ProjectId) -> AuraResult<Vec<ShooterBias>>;

    /// Which body a project is matched to, or `None` when no pass has run.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read.
    fn reference(&self, project: ProjectId) -> AuraResult<Option<Reference>>;

    /// Choose the reference body, and re-solve every other body against it.
    ///
    /// A photographer's choice is durable: it survives every re-analysis, exactly as
    /// `identities.user_locked`, `segments.user_locked`, `moments.user_locked`,
    /// `masks.user_edited` and `gallery_anchor.user_pinned` do, and the check is inside the
    /// statement that would overwrite it.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5131` when the body is unknown to the project, or when it shot no photographs.
    fn set_reference(&self, project: ProjectId, camera: &CameraId) -> Result<(), AuraError>;

    /// Switch matching off for one body, or back on.
    ///
    /// Invariant 8's kill switch at the grain a photographer actually wants: one body in a wedding
    /// that should not be matched to anything. Both flash states move together, because a
    /// photographer switching off a camera means the camera.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5131` when the body is unknown.
    fn set_enabled(
        &self,
        project: ProjectId,
        camera: &CameraId,
        enabled: bool,
    ) -> Result<(), AuraError>;

    /// Record what the photographer set instead, for one body in one flash state.
    ///
    /// Sets [`CameraTransform::user_edited`], and is not undone by a re-analysis.
    ///
    /// **This records the disagreement; it does not move a pixel.** Two writes rather than one,
    /// deliberately: a service that could do both would be a second way to edit a recipe. Phase 15
    /// wrote this rule and this is its fifth application.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5131` when the body has no transform, when the override is empty, or when a value
    /// is outside its documented bound.
    fn set_override(
        &self,
        project: ProjectId,
        camera: &CameraId,
        flash: FlashState,
        values: CameraOverride,
    ) -> Result<(), AuraError>;
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
    fn every_code_has_a_distinct_slug_and_a_sentence() {
        let mut slugs: Vec<&str> = CameraCode::ALL.iter().map(|c| c.as_str()).collect();
        slugs.sort_unstable();
        let before = slugs.len();
        slugs.dedup();
        assert_eq!(before, slugs.len(), "two codes share a slug");
        assert_eq!(before, CameraCode::COUNT);
        for code in CameraCode::ALL {
            assert!(!code.user_text().is_empty(), "{code} has no sentence");
            assert_eq!(CameraCode::from_str_or_fingerprinted(code.as_str()), code);
        }
    }

    #[test]
    fn thirty_two_codes_fit_in_one_u32() {
        assert!(CameraCode::COUNT <= 32);
        let all = CameraCode::to_bits(&CameraCode::ALL);
        assert_eq!(CameraCode::from_bits(all).len(), CameraCode::COUNT);
    }

    #[test]
    fn fifteen_codes_withdraw_a_claim() {
        let withdrawing = CameraCode::ALL.iter().filter(|c| c.withdraws()).count();
        assert_eq!(withdrawing, 15, "the module header counts fifteen");
    }

    #[test]
    fn refusals_outrank_actions_in_a_report() {
        assert!(
            CameraCode::PairsAbsent.default_weight() > CameraCode::SolvedFromPairs.default_weight()
        );
        assert!(
            CameraCode::HeldOutFailed.default_weight()
                > CameraCode::HeldOutImproved.default_weight()
        );
        for code in CameraCode::ALL {
            let w = code.default_weight();
            assert!((0.0..=1.0).contains(&w), "{code} weight {w}");
        }
    }

    #[test]
    fn a_reason_set_round_trips_through_one_integer_strongest_first() {
        let codes = [
            CameraCode::SolvedFromPairs,
            CameraCode::BoundedByPolicy,
            CameraCode::SkinMatched,
        ];
        let bits = CameraCode::to_bits(&codes);
        let reasons = CameraReason::from_bits(bits);
        assert_eq!(reasons.len(), 3);
        assert_eq!(reasons[0].code, CameraCode::BoundedByPolicy);
        assert_eq!(CameraReason::to_bits(&reasons), bits);
    }

    #[test]
    fn the_appearance_distance_is_section_6_2s_own_formula() {
        let d = AppearanceDistance {
            skin_de00: 1.0,
            white_point: 1.0,
            grade_signature: 1.0,
            contrast: 1.0,
        };
        assert!((d.total() - 6.0).abs() < 1e-6);
        assert!(d.is_finite());
    }

    #[test]
    fn a_zero_baseline_is_not_a_reduction() {
        let zero = AppearanceDistance::default();
        assert_eq!(
            zero.reduction_to(&zero),
            0.0,
            "nothing measured is not all removed"
        );
    }

    #[test]
    fn a_reference_transform_is_the_identity_and_is_in_bounds() {
        let t = CameraTransform::identity(
            CameraId::new("cam_a"),
            FlashState::Ambient,
            CameraId::new("cam_a"),
            1,
            1,
        );
        assert!(t.is_identity());
        assert!(t.within_bounds());
        assert_eq!(t.magnitude(), 0.0);
        assert_eq!(
            t.heldout_improved(),
            None,
            "an unchecked fit is not a passed one"
        );
    }

    #[test]
    fn magnitude_is_the_worst_axis_not_the_mean() {
        let mut t = CameraTransform::identity(
            CameraId::new("cam_b"),
            FlashState::Flash,
            CameraId::new("cam_a"),
            1,
            1,
        );
        t.d_cct = MAX_T_CCT_K;
        assert!((t.magnitude() - 1.0).abs() < 1e-6);
        assert!(t.within_bounds());
        t.d_cct = MAX_T_CCT_K * 1.5;
        assert!(!t.within_bounds());
    }

    #[test]
    fn every_bound_has_a_ceiling_and_round_trips() {
        assert_eq!(TransformBound::ALL.len(), TransformBound::COUNT);
        for bound in TransformBound::ALL {
            assert!(bound.ceiling() > 0.0);
            assert_eq!(TransformBound::from_str_or_cct(bound.as_str()), bound);
        }
    }

    #[test]
    fn a_shooter_habit_is_reduced_and_never_removed() {
        let measured = 0.8_f32;
        let applied = ShooterBias::correction_for(measured);
        assert!(
            applied.abs() < measured.abs(),
            "a habit is harmonised, not erased"
        );
        assert!(applied.abs() <= MAX_SHOOTER_EV + f32::EPSILON);
        assert_eq!(
            ShooterBias::correction_for(0.05),
            0.0,
            "inside the deadband nothing moves"
        );
        assert!(
            ShooterBias::correction_for(-0.8) > 0.0,
            "a correction opposes the habit it corrects"
        );
    }

    #[test]
    fn a_skin_correction_inside_the_caps_is_accepted_and_outside_is_not() {
        let good = SkinCorrection {
            d_uv: [SKIN_UV_CAP * 0.5, 0.0],
            d_luma: SKIN_LUMA_CAP * 0.5,
            de00_before: 4.0,
            de00_after: 1.2,
            locus_valid: true,
            capped: false,
        };
        assert!(good.within_caps());
        assert!(good.meets_promise());
        assert!(good.closed() > 0.6);
        let bad = SkinCorrection {
            d_uv: [SKIN_UV_CAP * 2.0, 0.0],
            ..good
        };
        assert!(!bad.within_caps());
    }

    #[test]
    fn flash_and_brand_parse_the_strings_real_cameras_write() {
        assert_eq!(FlashState::of(Some(true)), FlashState::Flash);
        assert_eq!(FlashState::of(Some(false)), FlashState::Ambient);
        assert_eq!(
            FlashState::of(None),
            FlashState::Ambient,
            "an unreadable field is not a third population"
        );
        assert_eq!(Brand::from_make("NIKON CORPORATION"), Brand::Nikon);
        assert_eq!(Brand::from_make("OLYMPUS IMAGING CORP."), Brand::Olympus);
        assert_eq!(Brand::from_make("OM Digital Solutions"), Brand::Olympus);
        assert_eq!(Brand::from_make("FUJIFILM"), Brand::Fujifilm);
        assert_eq!(Brand::from_make("Phase One"), Brand::Other);
        for brand in Brand::ALL {
            assert_eq!(Brand::from_str_or_other(brand.as_str()), brand);
        }
    }

    #[test]
    fn a_blend_is_neither_of_the_two_things_it_is_between() {
        assert!(TransformSource::MatchedPairs.measured_here());
        assert!(TransformSource::Blended.measured_here());
        assert!(!TransformSource::BrandBaseline.measured_here());
        for source in TransformSource::ALL {
            assert_eq!(
                TransformSource::from_str_or_baseline(source.as_str()),
                source
            );
        }
    }

    #[test]
    fn an_empty_override_is_empty_and_a_wild_one_is_out_of_bounds() {
        assert!(CameraOverride::default().is_empty());
        let wild = CameraOverride {
            d_cct: Some(MAX_T_CCT_K * 2.0),
            ..CameraOverride::default()
        };
        assert!(!wild.is_empty());
        assert!(!wild.within_bounds());
    }

    #[test]
    fn the_camera_bound_is_wider_than_the_gallery_bound_and_the_skin_cap_is_tighter() {
        // The two halves of ADR-0053 section 4, as an assertion rather than a paragraph: a camera
        // difference is systematic and larger than within-room drift, and a camera skin correction
        // applies to every frame that body shot, so it gets less room than a per-frame one.
        assert!(MAX_T_CCT_K > super::super::gallery::MAX_D_CCT_K);
        assert!(SKIN_UV_CAP < super::super::gallery::SKIN_CHROMA_CAP);
        assert!(SKIN_LUMA_CAP < super::super::gallery::SKIN_LUMA_CAP);
    }

    #[test]
    fn a_held_out_check_that_did_not_run_is_not_a_pass() {
        let mut t = CameraTransform::identity(
            CameraId::new("cam_b"),
            FlashState::Ambient,
            CameraId::new("cam_a"),
            1,
            1,
        );
        assert!(!t.heldout_checked());
        assert_eq!(t.heldout_improved(), None);
        t.heldout_pairs = MIN_HELDOUT_PAIRS;
        t.heldout_before = AppearanceDistance {
            skin_de00: 4.0,
            ..AppearanceDistance::default()
        };
        t.heldout_after = AppearanceDistance {
            skin_de00: 1.0,
            ..AppearanceDistance::default()
        };
        assert_eq!(t.heldout_improved(), Some(true));
        t.heldout_after = t.heldout_before;
        assert_eq!(t.heldout_improved(), Some(false));
    }

    #[test]
    fn the_sample_weight_is_zero_below_the_floor_and_one_above_the_ceiling() {
        assert_eq!(
            CameraFingerprint::sample_weight(MIN_FINGERPRINT_SAMPLES - 1),
            0.0
        );
        assert_eq!(
            CameraFingerprint::sample_weight(FULL_FINGERPRINT_SAMPLES),
            1.0
        );
        let mid = CameraFingerprint::sample_weight(
            (MIN_FINGERPRINT_SAMPLES + FULL_FINGERPRINT_SAMPLES) / 2,
        );
        assert!(mid > 0.4 && mid < 0.6, "{mid}");
    }
}
