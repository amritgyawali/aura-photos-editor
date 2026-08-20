//! FROZEN CONTRACT. How light is shaped inside one photograph: which faces were lifted,
//! how far the subject was separated from what is behind it, where the form was deepened
//! and what the product refused to do because it could not see well enough.
//!
//! PHASE-19 section 5 freezes [`LocalLightPlan`] before any solver exists. The file is in
//! `aura-core` rather than in `aura-brain-photo` for the reason
//! [`crate::contract::integrity`], [`crate::contract::composition`],
//! [`crate::contract::emotion`] and [`crate::contract::tone`] are: the phases that consume
//! a local light decision are 20 (portrait retouch, which must not double-lift a face this
//! phase already lifted), 25 (gallery consistency, which normalises across frames whose
//! local work differs) and 27 (QC, which has to be able to say *why* a frame looks edited),
//! and none of them needs the luminosity solver, the frequency separation or the governor.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **A plan is a set of instructions for masks, not a set of pixels.** Every number in this
//! contract reaches a photograph through `aura_recipe::schema::merge` writing
//! `recipe.masks[]`, which is phase 14's rule and the only place `user_edited_fields` is
//! honoured. Nothing here renders, and there is no field anywhere in this module that could
//! hold image data - which is what makes "all local work is stored as masks and parameters
//! and is fully reversible" (section 13) a property of the shape rather than a promise
//! about an exporter.
//!
//! ## The second thing: this phase does not own a mask
//!
//! Phase 18 owns masks. [`MaskField`] is the **input port** this phase reads them through:
//! a kind, a target, a coarse alpha field, a confidence and an edge quality. Phase 19 has
//! no mask generator, no segmentation model and no fallback that draws one - because a
//! second answer to "where is the subject" is a background reduction that traces a visible
//! outline around a shape nothing else in the product agrees with.
//!
//! When no field arrives, the operations that needed it are **gated rather than guessed**.
//! [`LocalLightPlan::gated_by_mask_quality`] is in section 5's own frozen shape for exactly
//! that, and section 6.4's rule - "all strengths are scaled by mask confidence and edge
//! quality, so a poor mask produces a gentle edit instead of an artefact" - is implemented
//! as a multiplier that reaches zero rather than as a branch that can be forgotten.
//!
//! ## The third thing: subtlety is the deliverable, so there is a budget
//!
//! Section 0 calls the risk "Medium-High - subtlety is the whole point". Six operations that
//! are each individually defensible add up to a photograph that looks processed, so
//! [`LocalLightPlan::total_budget_used`] is a fraction of one shared per-image allowance and
//! [`LocalOp::PRIORITY`] decides what is given up when it runs out. The budget is not
//! advisory: an implementation that exceeds [`PERCEPTUAL_BUDGET`] is a bug, and
//! `tests/eval/local_eval.rs` fails on it.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{IdentityId, ProjectId};
use crate::contract::integrity::CropRect;
use crate::contract::scene::SceneId;

pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// Budgets, caps and triggers
// ---------------------------------------------------------------------------

/// The whole per-image allowance for local work, as a mean absolute change in a perceptual
/// space over the frame.
///
/// Section 6.4's "per-image perceptual budget", and the number the phase's headline KPI
/// depends on. Roughly four and a half per cent mean change is about where a competent
/// retoucher stops being able to say what moved and starts being able to say *that*
/// something moved; every operation in this phase spends against it and none of them may
/// re-check it privately.
pub const PERCEPTUAL_BUDGET: f32 = 0.045;

/// The most any one face may be lifted, in stops, before the noise cap is even consulted.
///
/// Section 6.1's own example is "a face lifted 1.2 EV in a high-ISO reception would reveal
/// noise", so 1.2 is the ceiling rather than a typical value: the dynamic cap in
/// [`FaceLightDelta::noise_cap_ev`] is almost always tighter.
pub const MAX_FACE_LIFT_EV: f32 = 1.20;

/// The most any one face may be pulled down, in stops.
///
/// Tighter than the lift, and deliberately. A face that is too bright is usually too bright
/// because a flash fired, and the recoverable part of that is a highlight problem rather
/// than an exposure one - phase 15 already moved the frame and phase 16 owns the shoulder.
pub const MAX_FACE_PULL_EV: f32 = 0.60;

/// The largest luminance difference allowed between two faces in one frame after lighting.
///
/// Section 10.1: "inter-face luminance spread after lighting <= a documented threshold".
/// This is that threshold, in the same perceptual `0..1` units [`FaceLightDelta::luma_after`]
/// uses. Eight per cent is about a third of a stop at mid-grey, which is the point at which
/// a person in a family formal starts to look pasted in.
pub const MAX_INTER_FACE_SPREAD: f32 = 0.08;

/// How far the frame's mean luminance may drift once the paired operations have run.
///
/// Section 10.1: "subject/background pairing keeps global mean luminance within 3 % of the
/// pre-local value". The pairing exists to make this true; the constant is here so the
/// solver, the governor and the gate cannot disagree about what "3 %" was measured against.
pub const MAX_MEAN_LUMA_DRIFT: f32 = 0.03;

/// Below this mask confidence an operation is skipped entirely.
///
/// Not scaled to nearly nothing - skipped, and named in
/// [`LocalLightPlan::gated_by_mask_quality`]. A background reduction at five per cent
/// strength through a mask that is wrong still traces the wrong outline, and a very quiet
/// artefact is harder to find than a loud one.
pub const MIN_MASK_CONFIDENCE: f32 = 0.35;

/// At or above this mask confidence an operation runs at its full scene strength.
///
/// Between [`MIN_MASK_CONFIDENCE`] and here the strength ramps linearly, which is section
/// 6.4's "scaled by mask confidence and edge quality" made into one documented curve rather
/// than five.
pub const FULL_MASK_CONFIDENCE: f32 = 0.80;

/// The background/subject luminance ratio above which the background is competing.
///
/// Section 6.2 requires the trigger to be *measured* rather than assumed: below this the
/// background balance operation does not run and [`LocalCode::NoCompetitionMeasured`] says
/// so. A background fifteen per cent brighter than the subject is the point at which the
/// eye starts going there first.
pub const COMPETITION_LUMA_RATIO: f32 = 1.15;

/// The background chroma energy above which colour, rather than brightness, is competing.
///
/// Measured as mean chroma over the background region in the working space. A saturated
/// doorway and a bright doorway are different problems with different remedies, and a single
/// trigger would apply the wrong one to half of them.
pub const COMPETITION_CHROMA: f32 = 0.115;

/// A specular pixel is at least this bright, in perceptual `0..1`.
///
/// Section 6.3: "high luma, low chroma, small area, near-highlight". Three of those four
/// conditions are constants and this is the first.
pub const SHINE_LUMA_FLOOR: f32 = 0.86;

/// A specular pixel carries at most this much chroma.
///
/// Sheen is the light's own colour rather than the skin's, so it is markedly less saturated
/// than the face around it. This is what separates a shiny forehead from a warm one.
pub const SHINE_CHROMA_CEILING: f32 = 0.060;

/// The largest fraction of a face a shine region may cover before it stops being shine.
///
/// Above this the bright area is not a hot spot, it is the lighting - a face turned into a
/// window is not a sheen problem, and reducing it as one is how a subject ends up flat.
pub const SHINE_MAX_AREA: f32 = 0.14;

/// The most a shine region's luminance may be reduced, in stops.
pub const MAX_SHINE_REDUCTION_EV: f32 = 0.55;

/// How much mid-frequency band energy the dodge and burn pass may change.
///
/// Section 10.1: "dodge/burn preserves mid-frequency texture (measured band energy unchanged
/// within tolerance)". This is that tolerance, as a fraction of the measured band energy
/// before the operation. Five per cent is below what a 100 % crop shows.
pub const MID_BAND_TOLERANCE: f32 = 0.05;

/// The side of a shaping grid, in samples.
///
/// Section 5 calls the maps "quarter res". They are a quarter of the *face region* rather
/// than of the frame, and thirty-two samples a side is what a quarter of a face box on a
/// 2048 px proxy comes to for the faces this phase is willing to shape at all. A grid over
/// the whole frame would be a stored image, which phase 13's rule forbids and section 11's
/// budget could not hold.
pub const SHAPING_SIDE: u8 = 32;

/// The smallest face, as a fraction of the frame's shorter side, this phase will shape.
///
/// Below this the low-frequency band of the face is three or four samples wide and a
/// shaping map is noise with a name. The face is still *lit* - lighting needs only a mean -
/// and [`LocalCode::FaceTooSmallToShape`] says which of the two happened.
pub const MIN_SHAPEABLE_FACE: f32 = 0.055;

/// The most faces one plan will shape with dodge and burn.
///
/// Lighting is solved for every face in the frame; shaping is not. Four is where the
/// storage budget and the time budget meet, and a forty-person group formal is a frame
/// where per-face form shaping is the wrong idea anyway.
pub const MAX_SHAPED_FACES: usize = 4;

/// Below this plan confidence the frame is worth a photographer's attention.
///
/// The same shape as [`crate::contract::tone::REVIEW_WB_BELOW`] and used the same way: it
/// feeds [`LocalService::needs_review`] rather than gating anything.
pub const REVIEW_BELOW: f32 = 0.50;

/// The luminance gradient below which a reversal is noise rather than a halo.
///
/// Section 10.1 asks for "an automated edge-gradient test finding no artefact". Three
/// readings were tried before this one and it is worth writing down why each is wrong,
/// because somebody will try them again:
///
/// * **The before/after gradient ratio** fails because *every* local brightening increases the
///   step at its own boundary - that is what "local" means. A face lifted half a stop out of a
///   dark reception has a larger step against the room than it did, whether the mask was
///   feathered beautifully or cut out with scissors. It measures the edit's size.
/// * **Peak-over-mean gradient** fails because a hard edge puts its whole transition into one
///   sample, so its peak and its mean are the same number and it scores perfectly.
/// * **The transition width of the difference image** fails because when a mask's edge
///   coincides with a *content* edge - which is exactly what a good subject matte does - the
///   difference image steps there no matter how wide the feather is, because the same lift on
///   skin and on the wall behind it are different sizes.
///
/// What a halo actually is, is a **rim**: a bright or dark ring beside the subject that was
/// not in the photograph. In profile that is a *gradient reversal* - the luminance turns back
/// on itself where it did not before. So the measurement walks the boundary and looks for a
/// sample where the edit made the gradient change sign, and this constant is the magnitude
/// below which such a change is quantisation noise rather than a ring.
///
/// Four thousandths of a luminance unit is about one 8-bit code value, which is the smallest
/// thing that can honestly be called a gradient at all.
pub const HALO_REVERSAL_FLOOR: f32 = 0.004;

/// The smallest luminance change that counts as an edit for the halo measurement.
///
/// Half a per cent. Below this the difference image is quantisation noise and asking whether
/// it has a rim in it is meaningless - which is not a soft answer, it is the reason a frame
/// nothing happened to must be excluded from a test about how things happen.
pub const HALO_MIN_CHANGE: f32 = 0.005;

// ---------------------------------------------------------------------------
// Masks, as an input
// ---------------------------------------------------------------------------

/// What a generated mask is *of*.
///
/// The **analysis** vocabulary, and deliberately not the same set as
/// `aura_recipe::MaskKind`. The recipe's set is what a renderer can evaluate and includes
/// `linear`, `radial` and `brush` - three kinds that are drawn rather than generated and can
/// therefore never be gated by a generator's confidence. This set is what phase 18 produces
/// and what phase 19 can be refused. [`MaskKind::as_recipe_str`] is the total mapping
/// between them and a test asserts it stays total.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum MaskKind {
    /// One person's face, including the jaw and excluding the hair.
    #[default]
    Face,
    /// One person, head to feet, including hair and clothing.
    Subject,
    /// Everything that is not a subject.
    Background,
    /// Visible skin, for the operations that must not touch fabric.
    Skin,
    /// Hair, which is where a face mask's edge is hardest and where a halo shows first.
    Hair,
    /// Sky, which this phase reads but never brightens.
    Sky,
}

impl MaskKind {
    /// Every kind, in the order `docs/local-light.md` documents them.
    pub const ALL: [Self; 6] = [
        Self::Face,
        Self::Subject,
        Self::Background,
        Self::Skin,
        Self::Hair,
        Self::Sky,
    ];

    /// How many kinds there are.
    pub const COUNT: usize = 6;

    /// Stable text for the catalog and the wire. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Face => "face",
            Self::Subject => "subject",
            Self::Background => "background",
            Self::Skin => "skin",
            Self::Hair => "hair",
            Self::Sky => "sky",
        }
    }

    /// The spelling `aura_recipe::MaskKind::as_str` uses for the same region.
    ///
    /// Total by construction: every kind here has a recipe spelling, which is what lets a
    /// plan be written into a recipe without a fallible conversion in the middle. The
    /// reverse is not total and deliberately has no function - `linear`, `radial` and
    /// `brush` are not analysis kinds and a `From` that invented one would be a mask
    /// generator hiding in a conversion.
    #[must_use]
    pub const fn as_recipe_str(self) -> &'static str {
        match self {
            Self::Face => "face",
            Self::Subject => "subject",
            Self::Background => "background",
            // Hair has no recipe kind of its own in schema v1. It is read as evidence about
            // a face mask's hardest edge and never written as a mask, which is why this
            // maps to `face` rather than to something schema v1 cannot evaluate.
            Self::Skin | Self::Hair => "skin",
            Self::Sky => "sky",
        }
    }

    /// Parse the catalog spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == text)
    }

    /// True when this kind may be written into a recipe as a mask of its own.
    ///
    /// [`MaskKind::Hair`] and [`MaskKind::Sky`] are read and never written: hair is an edge
    /// quality measurement and sky is a thing this phase declines to touch. A kind that is
    /// read-only cannot appear in a plan's written masks, and the store refuses one that
    /// does.
    #[must_use]
    pub const fn is_writable(self) -> bool {
        matches!(
            self,
            Self::Face | Self::Subject | Self::Background | Self::Skin
        )
    }
}

impl fmt::Display for MaskKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One mask, as phase 19 reads it.
///
/// **The input port, and the whole of it.** Phase 18 fills these; this phase has no other
/// route to a mask and no way to make one. The alpha is a coarse field rather than a
/// full-resolution matte because every decision in this phase is a statistic over a region -
/// a mean luminance, a chroma energy, a band ratio - and none of them needs per-pixel
/// precision. The renderer gets the real matte from phase 18 at apply time; the decision
/// gets this.
///
/// ## Why `confidence` and `edge_quality` are two numbers
///
/// Because they fail differently and are gated differently. A mask can be confidently the
/// right *region* and have a terrible *boundary* - hair against a bright window is the
/// standard case - and that combination is safe for an exposure lift measured over the
/// region and dangerous for anything with a falloff. Section 6.4 scales by both; one number
/// would have to pick which failure to hide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MaskField {
    /// What the mask is of.
    pub kind: MaskKind,
    /// Who it is of, when it is of somebody.
    ///
    /// `None` for [`MaskKind::Background`] and [`MaskKind::Sky`], and for a subject mask in
    /// a frame where phase 06 identified nobody.
    #[serde(default)]
    pub identity: Option<IdentityId>,
    /// Where the mask's non-zero region sits, as a bounding box in frame coordinates.
    pub bounds: CropRect,
    /// The grid's width in samples.
    pub width: u16,
    /// The grid's height in samples.
    pub height: u16,
    /// Coverage per sample, `0..=255`, row-major, `width * height` long.
    ///
    /// Over the whole frame rather than over [`MaskField::bounds`], so two masks can be
    /// combined without a resample. A field whose length does not match its dimensions is
    /// refused by [`MaskField::validate`].
    pub alpha: Vec<u8>,
    /// How sure the generator is that this is the right region, `0..1`.
    pub confidence: f32,
    /// How good the boundary is, `0..1`. One is a matte that can carry a falloff.
    pub edge_quality: f32,
    /// Which phase 18 model produced it.
    pub model_ver: u16,
}

impl MaskField {
    /// The largest grid this phase will accept, per side.
    ///
    /// A field is a statistic carrier. Past 256 a side it is an image, and an image in a
    /// decision struct is how a 2 GB plan happens.
    pub const MAX_SIDE: u16 = 256;

    /// Fraction of the frame this mask covers, `0..1`.
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn coverage(&self) -> f32 {
        if self.alpha.is_empty() {
            return 0.0;
        }
        let total: u64 = self.alpha.iter().map(|&a| u64::from(a)).sum();
        (total as f64 / (self.alpha.len() as f64 * 255.0)) as f32
    }

    /// The mask's own strength multiplier, `0..1`.
    ///
    /// Section 6.4's rule in one place: the ramp between [`MIN_MASK_CONFIDENCE`] and
    /// [`FULL_MASK_CONFIDENCE`], multiplied by the edge quality. Zero below the floor,
    /// which is what makes "a poor mask produces a gentle edit instead of an artefact"
    /// reach "and a hopeless one produces none".
    #[must_use]
    pub fn strength_scale(&self) -> f32 {
        if self.confidence < MIN_MASK_CONFIDENCE {
            return 0.0;
        }
        let span = FULL_MASK_CONFIDENCE - MIN_MASK_CONFIDENCE;
        let ramp = ((self.confidence - MIN_MASK_CONFIDENCE) / span).clamp(0.0, 1.0);
        (ramp * self.edge_quality.clamp(0.0, 1.0)).clamp(0.0, 1.0)
    }

    /// True when this field is usable at all.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        self.strength_scale() > 0.0 && !self.alpha.is_empty()
    }

    /// Coverage at one sample, `0..1`.
    #[must_use]
    pub fn sample(&self, x: u16, y: u16) -> f32 {
        if x >= self.width || y >= self.height {
            return 0.0;
        }
        let index = usize::from(y) * usize::from(self.width) + usize::from(x);
        self.alpha.get(index).map_or(0.0, |&a| f32::from(a) / 255.0)
    }

    /// What is wrong with this field, if anything.
    ///
    /// A sentence rather than an [`AuraError`], because `aura-core` owns the shape and
    /// `aura-brain-photo` owns the error registry - the same split every phase since 09 has
    /// kept. `aura_brain_photo::local::guard` turns a `Some` here into `AURA-ML-5089`, and
    /// the predicate lives here so the solver, the store and the eval harness cannot
    /// disagree about what a readable field is.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if self.width == 0 || self.height == 0 {
            return Some("a side is zero".into());
        }
        if self.width > Self::MAX_SIDE || self.height > Self::MAX_SIDE {
            return Some(format!("a side is above {}", Self::MAX_SIDE));
        }
        if self.alpha.len() != usize::from(self.width) * usize::from(self.height) {
            return Some("the alpha length does not match the dimensions".into());
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Some("the confidence is outside 0..1".into());
        }
        if !(0.0..=1.0).contains(&self.edge_quality) {
            return Some("the edge quality is outside 0..1".into());
        }
        None
    }

    /// True when this field can be read at all.
    #[must_use]
    pub fn is_readable(&self) -> bool {
        self.problem().is_none()
    }
}

// ---------------------------------------------------------------------------
// The operations
// ---------------------------------------------------------------------------

/// The six things this phase can do to a photograph.
///
/// A closed set, because the governor has to be able to give one up and a per-image budget
/// with an open-ended list of spenders is not a budget. The order of [`LocalOp::PRIORITY`]
/// is the phase's product argument about what matters, and section 6.4's own sentence -
/// "operations are scaled down in priority order (face lighting first, dodge/burn last)" -
/// is read here as *face lighting has the first claim on the budget and dodge and burn the
/// last*. `docs/adr/ADR-0039-local-light-sculpting.md` section 5 records why that reading
/// and not the other: face lighting is the operation section 1 exists for and dodge and burn
/// is both the most decorative and the most artefact-prone, so a budget that protected the
/// shaping and gave up the lift would be spending the allowance on the part a photographer
/// would not miss.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum LocalOp {
    /// Lift or settle a face toward the scene's luminance band.
    #[default]
    FaceLight,
    /// Separate the subject from what is behind it, in contrast and micro-contrast.
    SubjectEnhance,
    /// Calm a background that is competing in brightness or in colour.
    BackgroundBalance,
    /// Reduce specular sheen on a forehead or a nose.
    ShineControl,
    /// Deepen and lift the low-frequency form of a face.
    DodgeBurnLow,
    /// Even out blotchy mid-frequency tonal patches without smoothing.
    DodgeBurnMid,
}

impl LocalOp {
    /// Every operation, highest claim on the budget first.
    ///
    /// This *is* the priority order; there is no second list and no comparator that could
    /// disagree with it.
    pub const PRIORITY: [Self; 6] = [
        Self::FaceLight,
        Self::SubjectEnhance,
        Self::BackgroundBalance,
        Self::ShineControl,
        Self::DodgeBurnLow,
        Self::DodgeBurnMid,
    ];

    /// How many operations there are.
    pub const COUNT: usize = 6;

    /// Stable text for the catalog, the wire and the config file.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FaceLight => "face_light",
            Self::SubjectEnhance => "subject_enhance",
            Self::BackgroundBalance => "background_balance",
            Self::ShineControl => "shine_control",
            Self::DodgeBurnLow => "dodge_burn_low",
            Self::DodgeBurnMid => "dodge_burn_mid",
        }
    }

    /// Parse the stable text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::PRIORITY.into_iter().find(|op| op.as_str() == text)
    }

    /// Where this operation sits in [`LocalOp::PRIORITY`], zero first.
    #[must_use]
    pub fn rank(self) -> usize {
        Self::PRIORITY
            .iter()
            .position(|op| *op == self)
            .unwrap_or(Self::COUNT)
    }

    /// The mask this operation cannot run without.
    ///
    /// `None` for nothing: every operation in this phase is local, and an operation with no
    /// mask requirement would be a global adjustment wearing a local name. The method exists
    /// so the gating table is written once.
    #[must_use]
    pub const fn requires(self) -> MaskKind {
        match self {
            Self::FaceLight | Self::DodgeBurnLow | Self::DodgeBurnMid => MaskKind::Face,
            Self::SubjectEnhance => MaskKind::Subject,
            Self::BackgroundBalance => MaskKind::Background,
            Self::ShineControl => MaskKind::Skin,
        }
    }
}

impl fmt::Display for LocalOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Face lighting
// ---------------------------------------------------------------------------

/// What one person's face was moved by, and what stopped it moving further.
///
/// Section 5's `(IdentityId, FaceLightDelta)`. Six of the nine fields are measurements
/// rather than instructions, and they are there because a lift that stopped short is the
/// thing a photographer most wants explained: "AURA lifted her face 0.4 EV and would have
/// lifted it 0.9" is a sentence, and "+0.4" is a number somebody argues with.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FaceLightDelta {
    /// Exposure inside the face mask, in stops. Positive brightens.
    pub exposure_ev: f32,
    /// Shadow lift inside the face mask, `-100 ..= 100`.
    ///
    /// The luminosity-masked part of section 6.1: shadows move more than mid-tones and
    /// highlights barely move, which is what stops the flat glowing-face look. The exposure
    /// and the shadow are two controls because the renderer applies them at two stages and
    /// folding them into one would put the whole lift where the highlights are.
    pub shadows: i16,
    /// Highlight restraint inside the face mask, `-100 ..= 0`.
    ///
    /// Never positive. A face is never lifted by pushing its highlights up; that is exactly
    /// the operation that makes a forehead glow.
    pub highlights: i16,
    /// Mask feather, `0..1`, chosen from the face's size in the frame.
    ///
    /// Section 6.1: "a small guest face needs a wider relative feather". Relative, so this
    /// is larger for a smaller face.
    pub feather: f32,
    /// The face's mean luminance before, `0..1` perceptual.
    pub luma_before: f32,
    /// Where the scene's band wanted it, `0..1` perceptual.
    pub luma_target: f32,
    /// Where it ended up, `0..1` perceptual.
    ///
    /// Equal to [`FaceLightDelta::luma_target`] only when nothing capped the move, which on
    /// a real wedding is the minority of frames.
    pub luma_after: f32,
    /// The largest lift this frame's noise would tolerate, in stops.
    ///
    /// Section 6.1's dynamic cap, from phase 09's measured noise and the scene's shadow
    /// budget. Stored rather than recomputed so the panel can say what stopped the lift.
    pub noise_cap_ev: f32,
    /// The mask confidence and edge quality this delta was scaled by, `0..1`.
    pub mask_scale: f32,
}

impl FaceLightDelta {
    /// A delta that changes nothing, for a face that was already where it should be.
    #[must_use]
    pub const fn none(luma: f32) -> Self {
        Self {
            exposure_ev: 0.0,
            shadows: 0,
            highlights: 0,
            feather: 0.0,
            luma_before: luma,
            luma_target: luma,
            luma_after: luma,
            noise_cap_ev: 0.0,
            mask_scale: 0.0,
        }
    }

    /// True when this delta moves nothing.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.exposure_ev.abs() < 1e-4 && self.shadows == 0 && self.highlights == 0
    }

    /// True when the lift stopped short of the band.
    #[must_use]
    pub fn was_capped(&self) -> bool {
        (self.luma_target - self.luma_after).abs() > 1e-3
    }
}

// ---------------------------------------------------------------------------
// Subject and background
// ---------------------------------------------------------------------------

/// The subject half of section 6.2's paired operation.
///
/// **Never applied alone.** [`SubjectEnhanceDelta::paired_background_ev`] is the background
/// exposure this delta was solved together with, and it is in this struct rather than only
/// in [`BackgroundBalanceDelta`] so that a caller cannot apply one without carrying the
/// other. Section 6.2: "the eye reads the *relationship*, not absolute values".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SubjectEnhanceDelta {
    /// Clarity inside the subject mask, `0 ..= 100`.
    pub clarity: i16,
    /// Texture inside the subject mask, `0 ..= 100`.
    pub texture: i16,
    /// Contrast inside the subject mask, `-100 ..= 100`.
    pub contrast: i16,
    /// The background exposure, in stops, this was solved against.
    ///
    /// Always zero or negative when the subject was lifted. Duplicated into
    /// [`BackgroundBalanceDelta::exposure_ev`]; the store writes it once and a test asserts
    /// the two agree.
    pub paired_background_ev: f32,
    /// The measured background/subject luminance ratio that triggered it.
    pub competition_ratio: f32,
    /// The mask confidence and edge quality this delta was scaled by, `0..1`.
    pub mask_scale: f32,
}

impl SubjectEnhanceDelta {
    /// A delta that changes nothing.
    pub const NONE: Self = Self {
        clarity: 0,
        texture: 0,
        contrast: 0,
        paired_background_ev: 0.0,
        competition_ratio: 1.0,
        mask_scale: 0.0,
    };

    /// True when this delta moves nothing.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.clarity == 0 && self.texture == 0 && self.contrast == 0
    }
}

/// The background half.
///
/// Section 6.2's three measured triggers are all stored: the luminance ratio, the chroma
/// energy and the count of bright blobs phase 11 already found. An operation that fired
/// without one of them crossing a threshold is a bug, and the gate checks it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BackgroundBalanceDelta {
    /// Exposure inside the background mask, in stops. Zero or negative.
    pub exposure_ev: f32,
    /// Highlight reduction, `-100 ..= 0`.
    pub highlights: i16,
    /// Saturation reduction, `-100 ..= 0`.
    ///
    /// The chroma half of section 6.2. Never positive: this phase calms a background and
    /// never enriches one, because enriching a background is a grade and grades are phase
    /// 16's.
    pub saturation: i16,
    /// Mask feather, `0..1`.
    pub feather: f32,
    /// The measured background/subject luminance ratio.
    pub competition_ratio: f32,
    /// The measured background chroma energy, `0..1`.
    pub chroma_energy: f32,
    /// How many bright blobs phase 11 found behind the subject.
    pub bright_blobs: u8,
    /// The frame's mean luminance before the paired operations, `0..1`.
    pub mean_luma_before: f32,
    /// The frame's mean luminance after them, `0..1`.
    ///
    /// Section 10.1's own measurement, stored rather than recomputed at gate time. The two
    /// must agree within [`MAX_MEAN_LUMA_DRIFT`] and the store refuses a row where they do
    /// not.
    pub mean_luma_after: f32,
    /// The mask confidence and edge quality this delta was scaled by, `0..1`.
    pub mask_scale: f32,
}

impl BackgroundBalanceDelta {
    /// A delta that changes nothing.
    pub const NONE: Self = Self {
        exposure_ev: 0.0,
        highlights: 0,
        saturation: 0,
        feather: 0.0,
        competition_ratio: 1.0,
        chroma_energy: 0.0,
        bright_blobs: 0,
        mean_luma_before: 0.0,
        mean_luma_after: 0.0,
        mask_scale: 0.0,
    };

    /// True when this delta moves nothing.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.exposure_ev.abs() < 1e-4 && self.highlights == 0 && self.saturation == 0
    }

    /// How far the frame's mean luminance drifted.
    #[must_use]
    pub fn luma_drift(&self) -> f32 {
        (self.mean_luma_after - self.mean_luma_before).abs()
    }
}

// ---------------------------------------------------------------------------
// Dodge and burn
// ---------------------------------------------------------------------------

/// A region of a face a retoucher has a name for.
///
/// A closed set, because the shaping map is generated from these zones and a zone nothing
/// can place is a zone that silently shapes nothing. Every one of them is a move a portrait
/// retoucher makes by hand, which is section 6.3's "the classic retoucher moves, applied
/// conservatively" - and naming them is what lets the panel say *what* was shaped rather
/// than showing a grey map nobody can read.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum FaceZone {
    /// Under the eyes. Lifted, always - this is the move that reads as "well lit".
    #[default]
    UnderEye,
    /// The cheekbone. Lifted slightly.
    Cheekbone,
    /// The hollow under the cheekbone. Deepened slightly.
    CheekHollow,
    /// The jawline. Deepened, and the zone that most easily goes too far.
    Jaw,
    /// The bridge of the nose. Lifted.
    NoseBridge,
    /// The side of the nose. Deepened.
    NoseSide,
    /// The centre of the forehead. Lifted or settled depending on the light direction.
    Forehead,
    /// The temple. Deepened, to stop a wide-lit face reading flat.
    Temple,
    /// The chin.
    Chin,
    /// The shadow under the chin, on the neck.
    NeckShadow,
}

impl FaceZone {
    /// Every zone, in the order `docs/local-light.md` documents them.
    pub const ALL: [Self; 10] = [
        Self::UnderEye,
        Self::Cheekbone,
        Self::CheekHollow,
        Self::Jaw,
        Self::NoseBridge,
        Self::NoseSide,
        Self::Forehead,
        Self::Temple,
        Self::Chin,
        Self::NeckShadow,
    ];

    /// How many zones there are.
    pub const COUNT: usize = 10;

    /// Stable text for the catalog and the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnderEye => "under_eye",
            Self::Cheekbone => "cheekbone",
            Self::CheekHollow => "cheek_hollow",
            Self::Jaw => "jaw",
            Self::NoseBridge => "nose_bridge",
            Self::NoseSide => "nose_side",
            Self::Forehead => "forehead",
            Self::Temple => "temple",
            Self::Chin => "chin",
            Self::NeckShadow => "neck_shadow",
        }
    }

    /// Parse the stable text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|z| z.as_str() == text)
    }

    /// True when this zone is only ever lifted, never deepened.
    ///
    /// Three of the ten. A dodge zone that could burn is how a shaping map puts a shadow
    /// under somebody's eyes, and the sign is a property of the zone rather than of the
    /// solver's arithmetic on the day.
    #[must_use]
    pub const fn is_dodge_only(self) -> bool {
        matches!(self, Self::UnderEye | Self::Cheekbone | Self::NoseBridge)
    }
}

impl fmt::Display for FaceZone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One shaping move: where, how much, and how softly.
///
/// **The stored form of a dodge and burn map.** The grid in [`FaceShaping::low_freq`] is
/// *derived* from these zones and is regenerated deterministically rather than persisted -
/// which is phase 13's rule ("evidence can never be a pixel") applied to a decision rather
/// than to evidence, and is what keeps section 11's storage budget reachable. A build that
/// changed the derivation would change every stored plan's pixels, which is what
/// `shaping_ver` on [`LocalLightPlan`] exists to make visible.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ShapingZone {
    /// Which zone this is.
    pub zone: FaceZone,
    /// Centre in frame coordinates, `0..1`.
    pub centre: [f32; 2],
    /// Radius as a fraction of the frame's shorter side.
    pub radius: f32,
    /// Gain in stops. Positive lifts, negative deepens.
    ///
    /// Bounded by [`ShapingZone::MAX_GAIN_EV`], and never positive-then-negative within one
    /// zone: a zone is a move, not a curve.
    pub gain_ev: f32,
}

impl ShapingZone {
    /// The most any one zone may move, in stops.
    ///
    /// A sixth of a stop. Section 6.3's word is "conservatively", and a retoucher's own
    /// dodge and burn on a delivered wedding frame is smaller than most people expect.
    pub const MAX_GAIN_EV: f32 = 0.167;

    /// True when the sign of this zone's gain contradicts its own kind.
    #[must_use]
    pub fn sign_is_wrong(&self) -> bool {
        self.zone.is_dodge_only() && self.gain_ev < 0.0
    }
}

/// One face's shaping, both bands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FaceShaping {
    /// Whose face, when phase 06 knows.
    #[serde(default)]
    pub identity: Option<IdentityId>,
    /// The face region the grid covers, in frame coordinates.
    pub region: CropRect,
    /// The grid side in samples; always [`SHAPING_SIDE`].
    pub side: u8,
    /// The low-frequency shaping map, `side * side`, in units of 1/200 stop.
    ///
    /// Derived from [`FaceShaping::zones`]. Never persisted - see [`ShapingZone`].
    #[serde(skip)]
    pub low_freq: Vec<i8>,
    /// The mid-frequency evening map, same layout and units.
    ///
    /// Derived from the measured mid-band residual, and bounded so the band energy moves by
    /// less than [`MID_BAND_TOLERANCE`]. Never persisted.
    #[serde(skip)]
    pub mid_freq: Vec<i8>,
    /// The moves the low-frequency map was generated from.
    ///
    /// **Derived rather than stored**, and that is one step further than the grid. Every zone's
    /// centre and radius is a fixed proportion of [`FaceShaping::region`], and its gain is a
    /// fixed base scaled by [`FaceShaping::light_direction`] and
    /// [`FaceShaping::low_strength`] - so the whole list is a pure function of four numbers,
    /// and the catalog stores the four. Ten zones written out cost about 450 bytes a face and
    /// four faces is most of a kilobyte; the four numbers cost about forty.
    ///
    /// The panel still shows the zones by name, because they are regenerated on read. What is
    /// versioned is the derivation, which is what `shaping_ver` is for.
    pub zones: Vec<ShapingZone>,
    /// Which side of the face was already darker: `-1` left, `1` right, `0` flatly lit.
    ///
    /// Stored, because it is measured from the pixels and cannot be recovered from anything
    /// else in the row.
    pub light_direction: f32,
    /// The strength the low-frequency shaping ran at, after the scene policy, the mask
    /// scaling and the governor, `0..1`.
    ///
    /// Stored for the same reason. Together with the region and the direction it reproduces
    /// [`FaceShaping::zones`] exactly.
    pub low_strength: f32,
    /// How strongly the mid-frequency evening was applied, `0..1`.
    pub evening: f32,
    /// The measured mid-band energy before, for the texture gate.
    pub band_energy_before: f32,
    /// The measured mid-band energy after.
    pub band_energy_after: f32,
}

impl FaceShaping {
    /// How much the mid-frequency band moved, as a fraction of what it was.
    ///
    /// Section 10.1's texture measurement. Zero band energy before means an already-flat
    /// crop, and the ratio is reported as zero rather than as infinity.
    #[must_use]
    pub fn band_drift(&self) -> f32 {
        if self.band_energy_before <= f32::EPSILON {
            return 0.0;
        }
        ((self.band_energy_after - self.band_energy_before) / self.band_energy_before).abs()
    }

    /// True when the shaping preserved texture within tolerance.
    #[must_use]
    pub fn texture_preserved(&self) -> bool {
        self.band_drift() <= MID_BAND_TOLERANCE
    }
}

/// Every face this plan shaped.
///
/// Section 5's `DodgeBurnMaps`, given a per-face structure. One struct with two flat maps
/// could not express a group formal at all, and section 6.1's group fairness rule means a
/// group formal is exactly the frame this phase must not get wrong.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DodgeBurnMaps {
    /// One entry per shaped face, at most [`MAX_SHAPED_FACES`].
    pub faces: Vec<FaceShaping>,
    /// Which build's derivation turned the zones into grids.
    pub shaping_ver: u16,
}

impl DodgeBurnMaps {
    /// True when nothing was shaped.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// True when every shaped face kept its mid-frequency texture.
    #[must_use]
    pub fn texture_preserved(&self) -> bool {
        self.faces.iter().all(FaceShaping::texture_preserved)
    }
}

// ---------------------------------------------------------------------------
// Shine
// ---------------------------------------------------------------------------

/// A specular hot spot and what was done about it.
///
/// **A luminance operation, and the type cannot express anything else.** Section 6.3:
/// "reduces luminance only, preserving underlying texture". There is no blur radius, no
/// smoothing strength and no texture field anywhere in this struct - which is what stops the
/// obvious wrong fix from being one refactor away, and is the boundary between this phase
/// and phase 20.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ShineReduction {
    /// The regions found, largest first, at most [`ShineReduction::MAX_REGIONS`].
    pub regions: Vec<CropRect>,
    /// Whose face they are on, in the same order.
    ///
    /// `None` where phase 06 identified nobody. Parallel to `regions` rather than paired
    /// with it because a region without an identity is common and an `Option` per element
    /// is cheaper to read than a tuple.
    #[serde(default)]
    pub identities: Vec<Option<IdentityId>>,
    /// Luminance reduction, in stops. Always negative, bounded by
    /// [`MAX_SHINE_REDUCTION_EV`].
    pub reduction_ev: f32,
    /// The fraction of the faces these regions covered, `0..1`.
    pub area_fraction: f32,
    /// The mean luminance of the specular pixels before, `0..1`.
    pub peak_before: f32,
    /// The mean luminance after.
    pub peak_after: f32,
    /// The mask confidence and edge quality this was scaled by, `0..1`.
    pub mask_scale: f32,
}

impl ShineReduction {
    /// The most regions one plan carries.
    ///
    /// Six. A forehead, a nose and a chin on two people is the realistic worst case that is
    /// still a hot-spot problem; past that the frame is lit with a bare flash and the answer
    /// is not local.
    pub const MAX_REGIONS: usize = 6;

    /// True when nothing was found.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why the local work came out the way it did, as a closed set.
///
/// Thirty codes. `docs/local-light.md` is written against [`LocalCode::ALL`] and a test
/// asserts every variant appears there.
///
/// **Fourteen of the thirty withdraw a claim rather than making one**, which is the highest
/// proportion of any phase so far and is the point of the phase. Section 0's risk line is
/// "subtlety is the whole point"; an editor that shapes every frame it can is an editor that
/// shapes frames it should have left alone, and the codes below are how the product says
/// *this one did not need me* out loud rather than by silently doing very little.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum LocalCode {
    // -- face lighting ---------------------------------------------------
    /// A face was lifted or settled toward the scene's band.
    FaceLit,
    /// Every face was already inside the band. **A withdrawal.**
    #[default]
    FaceAlreadyInBand,
    /// The lift stopped at the noise the shadows would have revealed.
    LiftCappedByNoise,
    /// The lift stopped because the highlights on the face would have clipped.
    LiftCappedByHighlights,
    /// The lift stopped because the per-image budget ran out.
    LiftCappedByBudget,
    /// Every face in the frame was solved together toward one band.
    GroupSolvedJointly,
    /// The joint solve pulled a face back to keep the group within
    /// [`MAX_INTER_FACE_SPREAD`].
    GroupSpreadCapped,
    /// A face was too small in the frame to shape, but was still lit. **A withdrawal.**
    FaceTooSmallToShape,

    // -- subject and background ------------------------------------------
    /// The subject was separated from the background.
    SubjectSeparated,
    /// The subject lift and the background reduction were solved as one move.
    SubjectBackgroundPaired,
    /// Nothing behind the subject was competing. **A withdrawal.**
    NoCompetitionMeasured,
    /// A brighter background was brought down.
    BackgroundLumaReduced,
    /// A more saturated background was calmed.
    BackgroundChromaReduced,
    /// A bright blob phase 11 found behind the subject was calmed specifically.
    BrightBlobCalmed,
    /// The pairing was scaled back to keep the frame's mean luminance where it was.
    PairingHeldMeanLuma,

    // -- dodge and burn ---------------------------------------------------
    /// The low-frequency form of a face was shaped.
    FormShaped,
    /// A blotchy mid-frequency patch was evened out.
    MidFrequencyEvened,
    /// The shaping was scaled back to keep the mid-frequency band where it was.
    /// **A withdrawal.**
    TextureProtected,
    /// There were no landmarks to shape against. **A withdrawal.**
    LandmarksUnavailable,
    /// This scene's policy is minimal shaping. **A withdrawal.**
    SceneDeclinesShaping,

    // -- shine -------------------------------------------------------------
    /// Specular sheen was reduced.
    ShineReduced,
    /// Nothing specular was found. **A withdrawal.**
    NoShineFound,
    /// A bright region was too large to be sheen and was left alone. **A withdrawal.**
    ShineTooLargeToBeSheen,

    // -- gating and governance ---------------------------------------------
    /// An operation was skipped because its mask was not available. **A withdrawal.**
    MaskUnavailable,
    /// An operation was scaled down because its mask was weak.
    MaskWeak,
    /// The per-image budget was exhausted and lower-priority work was given up.
    BudgetExhausted,
    /// This scene's policy limits how much may be done at all.
    SceneStrengthLimited,
    /// The learned target head is not available, so the reference targets were used.
    /// **A withdrawal.**
    TargetHeadUnavailable,
    /// Local light sculpting is switched off for this project. **A withdrawal.**
    LocalDisabled,
    /// The photographer set the strengths by hand. **A withdrawal.**
    UserOverride,
}

impl LocalCode {
    /// Every code, in the order `docs/local-light.md` documents them.
    pub const ALL: [Self; 30] = [
        Self::FaceLit,
        Self::FaceAlreadyInBand,
        Self::LiftCappedByNoise,
        Self::LiftCappedByHighlights,
        Self::LiftCappedByBudget,
        Self::GroupSolvedJointly,
        Self::GroupSpreadCapped,
        Self::FaceTooSmallToShape,
        Self::SubjectSeparated,
        Self::SubjectBackgroundPaired,
        Self::NoCompetitionMeasured,
        Self::BackgroundLumaReduced,
        Self::BackgroundChromaReduced,
        Self::BrightBlobCalmed,
        Self::PairingHeldMeanLuma,
        Self::FormShaped,
        Self::MidFrequencyEvened,
        Self::TextureProtected,
        Self::LandmarksUnavailable,
        Self::SceneDeclinesShaping,
        Self::ShineReduced,
        Self::NoShineFound,
        Self::ShineTooLargeToBeSheen,
        Self::MaskUnavailable,
        Self::MaskWeak,
        Self::BudgetExhausted,
        Self::SceneStrengthLimited,
        Self::TargetHeadUnavailable,
        Self::LocalDisabled,
        Self::UserOverride,
    ];

    /// How many reason codes there are.
    pub const COUNT: usize = 30;

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FaceLit => "face_lit",
            Self::FaceAlreadyInBand => "face_already_in_band",
            Self::LiftCappedByNoise => "lift_capped_by_noise",
            Self::LiftCappedByHighlights => "lift_capped_by_highlights",
            Self::LiftCappedByBudget => "lift_capped_by_budget",
            Self::GroupSolvedJointly => "group_solved_jointly",
            Self::GroupSpreadCapped => "group_spread_capped",
            Self::FaceTooSmallToShape => "face_too_small_to_shape",
            Self::SubjectSeparated => "subject_separated",
            Self::SubjectBackgroundPaired => "subject_background_paired",
            Self::NoCompetitionMeasured => "no_competition_measured",
            Self::BackgroundLumaReduced => "background_luma_reduced",
            Self::BackgroundChromaReduced => "background_chroma_reduced",
            Self::BrightBlobCalmed => "bright_blob_calmed",
            Self::PairingHeldMeanLuma => "pairing_held_mean_luma",
            Self::FormShaped => "form_shaped",
            Self::MidFrequencyEvened => "mid_frequency_evened",
            Self::TextureProtected => "texture_protected",
            Self::LandmarksUnavailable => "landmarks_unavailable",
            Self::SceneDeclinesShaping => "scene_declines_shaping",
            Self::ShineReduced => "shine_reduced",
            Self::NoShineFound => "no_shine_found",
            Self::ShineTooLargeToBeSheen => "shine_too_large_to_be_sheen",
            Self::MaskUnavailable => "mask_unavailable",
            Self::MaskWeak => "mask_weak",
            Self::BudgetExhausted => "budget_exhausted",
            Self::SceneStrengthLimited => "scene_strength_limited",
            Self::TargetHeadUnavailable => "target_head_unavailable",
            Self::LocalDisabled => "local_disabled",
            Self::UserOverride => "user_override",
        }
    }

    /// Parse the stable slug.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == text)
    }

    /// The sentence a photographer reads, in the product's voice.
    ///
    /// Not a translation of the slug. Section 9 gives DOC "explain local light shaping and
    /// how to tune strength", and the hardest thing to explain in this phase is why nothing
    /// happened - so eleven of these sentences are about restraint and none of them apologise
    /// for it.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::FaceLit => {
                "the light on this face was lifted to match the rest of this part of the day"
            }
            Self::FaceAlreadyInBand => {
                "the faces here were already lit the way this kind of photograph should be, so \
                 nothing was changed"
            }
            Self::LiftCappedByNoise => {
                "lifting this face further would have brought out grain, so AURA stopped where \
                 it did"
            }
            Self::LiftCappedByHighlights => {
                "lifting this face further would have blown out the bright side of it, so AURA \
                 stopped where it did"
            }
            Self::LiftCappedByBudget => {
                "AURA had already changed enough in this photograph, so it did not lift the face \
                 the whole way"
            }
            Self::GroupSolvedJointly => {
                "everybody in this photograph was lit together, so nobody looks pasted in"
            }
            Self::GroupSpreadCapped => {
                "one person would have ended up noticeably brighter than everybody else, so AURA \
                 held them back"
            }
            Self::FaceTooSmallToShape => {
                "this face is small in the frame, so AURA adjusted its brightness but did not \
                 shape it"
            }
            Self::SubjectSeparated => {
                "the subject was given a little more presence than what is behind them"
            }
            Self::SubjectBackgroundPaired => {
                "the subject was lifted and the background brought down by the same amount, so \
                 the photograph is no brighter overall"
            }
            Self::NoCompetitionMeasured => {
                "nothing behind the subject was pulling the eye, so the background was left alone"
            }
            Self::BackgroundLumaReduced => {
                "the background here was brighter than the subject, so it was brought down"
            }
            Self::BackgroundChromaReduced => {
                "a strong colour behind the subject was competing with them, so it was calmed"
            }
            Self::BrightBlobCalmed => "a bright patch behind the subject was brought down",
            Self::PairingHeldMeanLuma => {
                "AURA kept the overall brightness of the photograph where it was"
            }
            Self::FormShaped => {
                "the shape of the face was deepened very slightly, the way a retoucher would"
            }
            Self::MidFrequencyEvened => {
                "uneven patches of tone on the skin were evened out without touching the texture"
            }
            Self::TextureProtected => {
                "AURA held back the shaping here to keep the skin texture exactly as it was"
            }
            Self::LandmarksUnavailable => {
                "AURA could not find the features of this face reliably enough to shape it, so it \
                 only adjusted the brightness"
            }
            Self::SceneDeclinesShaping => {
                "photographs of this kind are left largely as shot, so AURA did very little here"
            }
            Self::ShineReduced => "shine on the skin was brought down without softening it",
            Self::NoShineFound => "there was no shine to reduce here",
            Self::ShineTooLargeToBeSheen => {
                "the bright area on this face is the lighting rather than shine, so AURA left it \
                 alone"
            }
            Self::MaskUnavailable => {
                "AURA could not work out where the subject ends and the background begins here, \
                 so it did not make any local adjustments"
            }
            Self::MaskWeak => {
                "AURA is not certain where the edges of the subject are here, so it made a \
                 gentler adjustment than usual"
            }
            Self::BudgetExhausted => {
                "AURA had already changed as much as it allows itself in one photograph, so it \
                 stopped"
            }
            Self::SceneStrengthLimited => {
                "photographs of this kind get a lighter touch, and this one was adjusted \
                 accordingly"
            }
            Self::TargetHeadUnavailable => {
                "AURA is using its built-in guidance for how faces should be lit rather than \
                 anything learned from edits"
            }
            Self::LocalDisabled => "local light adjustments are switched off for this wedding",
            Self::UserOverride => "you set these strengths by hand, and AURA has not changed them",
        }
    }

    /// True when this code withdraws a claim rather than making one.
    ///
    /// The machine-readable list phase 11 introduced. `tests/eval/local_eval.rs` asserts a
    /// withdrawal never carries a negative weight it did not earn, and
    /// `docs/local-light.md` groups the reference by it.
    #[must_use]
    pub const fn is_withdrawal(self) -> bool {
        matches!(
            self,
            Self::FaceAlreadyInBand
                | Self::LiftCappedByNoise
                | Self::LiftCappedByHighlights
                | Self::FaceTooSmallToShape
                | Self::NoCompetitionMeasured
                | Self::TextureProtected
                | Self::LandmarksUnavailable
                | Self::SceneDeclinesShaping
                | Self::NoShineFound
                | Self::ShineTooLargeToBeSheen
                | Self::MaskUnavailable
                | Self::TargetHeadUnavailable
                | Self::LocalDisabled
                | Self::UserOverride
        )
    }

    /// Which operation this code is about, when it is about one.
    ///
    /// `None` for the governance codes, which are about the plan. The panel groups reasons
    /// under the operation strength slider they belong to, and a caller that derived this
    /// from the slug's prefix would get [`LocalCode::MaskUnavailable`] wrong.
    #[must_use]
    pub const fn operation(self) -> Option<LocalOp> {
        match self {
            Self::FaceLit
            | Self::FaceAlreadyInBand
            | Self::LiftCappedByNoise
            | Self::LiftCappedByHighlights
            | Self::LiftCappedByBudget
            | Self::GroupSolvedJointly
            | Self::GroupSpreadCapped => Some(LocalOp::FaceLight),
            Self::SubjectSeparated | Self::SubjectBackgroundPaired => Some(LocalOp::SubjectEnhance),
            Self::NoCompetitionMeasured
            | Self::BackgroundLumaReduced
            | Self::BackgroundChromaReduced
            | Self::BrightBlobCalmed
            | Self::PairingHeldMeanLuma => Some(LocalOp::BackgroundBalance),
            Self::FaceTooSmallToShape
            | Self::FormShaped
            | Self::LandmarksUnavailable
            | Self::SceneDeclinesShaping => Some(LocalOp::DodgeBurnLow),
            Self::MidFrequencyEvened | Self::TextureProtected => Some(LocalOp::DodgeBurnMid),
            Self::ShineReduced | Self::NoShineFound | Self::ShineTooLargeToBeSheen => {
                Some(LocalOp::ShineControl)
            }
            Self::MaskUnavailable
            | Self::MaskWeak
            | Self::BudgetExhausted
            | Self::SceneStrengthLimited
            | Self::TargetHeadUnavailable
            | Self::LocalDisabled
            | Self::UserOverride => None,
        }
    }
}

impl fmt::Display for LocalCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One thing that decided the local work, with the pixels behind it.
///
/// Invariant 2 makes the reason mandatory. The evidence rectangle is more often `Some` here
/// than in any previous phase, because almost everything this phase does happens somewhere
/// in particular - and a photographer asking "what did you do to this photograph" about an
/// invisible edit needs to be shown where to look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalReason {
    /// Which reason this is.
    pub code: LocalCode,
    /// The sentence a photographer reads.
    pub text: String,
    /// How much this moved the confidence. Negative for a doubt.
    pub weight: f32,
    /// The pixels to show, when there are any more specific than the whole frame.
    #[serde(default)]
    pub evidence: Option<CropRect>,
}

impl LocalReason {
    /// A reason about the whole frame.
    #[must_use]
    pub fn frame(code: LocalCode, text: impl Into<String>, weight: f32) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: None,
        }
    }

    /// A reason about one region.
    #[must_use]
    pub fn at(code: LocalCode, text: impl Into<String>, weight: f32, evidence: CropRect) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: Some(evidence),
        }
    }

    /// A reason carrying only the code's own sentence.
    #[must_use]
    pub fn plain(code: LocalCode, weight: f32) -> Self {
        Self::frame(code, code.user_text(), weight)
    }

    /// A reason carrying the code's own sentence, about one region.
    #[must_use]
    pub fn plain_at(code: LocalCode, weight: f32, evidence: CropRect) -> Self {
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

/// Everything phase 19 decided about the light inside one photograph.
///
/// PHASE-19 section 5's frozen shape, with the four additions this module's header argues
/// for: the scene it was decided under, the three version columns, the per-operation
/// strengths a photographer may have set, and the flag that says a person set them.
///
/// **It is not an edit.** The values here reach the pixels only through
/// `aura_recipe::schema::merge` writing `recipe.masks[]`, which is phase 14's rule. A plan
/// with `user_edited = true` is a record of what the product *would* have done, kept beside
/// what the photographer chose instead, so phase 30's learning loop can read the
/// disagreement - the same shape phase 15 uses and for the same reason.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalLightPlan {
    /// The photograph.
    pub image_id: ImageId,
    /// What each face was moved by, by identity, in a deterministic order.
    ///
    /// A `Vec` of pairs rather than a map because section 5 freezes it that way and because
    /// a frame can contain a face phase 06 has not identified - which is the majority of
    /// guests at most weddings, and a map keyed by [`IdentityId`] could not hold them. The
    /// identity is `None` for those.
    pub face_light: Vec<(Option<IdentityId>, FaceLightDelta)>,
    /// The subject half of the paired operation.
    pub subject: SubjectEnhanceDelta,
    /// The background half.
    pub background: BackgroundBalanceDelta,
    /// The shaping, when there was any.
    pub dodge_burn: Option<DodgeBurnMaps>,
    /// The shine reduction, when there was any.
    pub shine: Option<ShineReduction>,
    /// How much of the allowed perceptual change was spent, `0..1`.
    ///
    /// Above one is a bug and the store refuses it.
    pub total_budget_used: f32,
    /// Operations that were reduced or skipped, and the mask kind that caused it.
    ///
    /// Section 5's own field. Empty on a frame where every mask arrived and was good, which
    /// on a build with no phase 18 is never - see the crate header.
    pub gated_by_mask_quality: Vec<(LocalOp, MaskKind)>,
    /// Why. Never empty; invariant 2.
    pub reasons: Vec<LocalReason>,
    /// How much the plan trusts itself, `0..1`.
    pub confidence: f32,
    /// The scene this was decided under.
    ///
    /// Invariant 7. Stored rather than re-read because a re-classification changes what the
    /// plan *should* have been, and a plan that silently starts describing itself under a new
    /// scene is a plan nobody can audit.
    pub scene: SceneId,
    /// The strength each operation actually ran at, `0..1`, in [`LocalOp::PRIORITY`] order.
    ///
    /// After the scene policy, the mask scaling and the governor. Zero means the operation
    /// did not run, which is not the same as running and finding nothing to do - and the
    /// panel needs to be able to tell those apart.
    pub strengths: [f32; LocalOp::COUNT],
    /// True when a photographer set the strengths by hand.
    pub user_edited: bool,
    /// True when a photographer has looked at this plan and agreed.
    pub reviewed: bool,
    /// Which learned head produced the targets.
    pub model_ver: u16,
    /// Which build's arithmetic produced the decisions.
    pub analysis_ver: u16,
    /// Which policy file the scene strengths came from.
    pub policy_ver: u16,
    /// Which build's derivation turns [`ShapingZone`]s into grids.
    ///
    /// A fourth version column, and the argument for it is the one phase 10 used to *remove*
    /// a column read backwards: the shaping grid is derived rather than stored, so a change
    /// to the derivation changes delivered pixels without changing a single stored value.
    /// Without this, that change would be invisible.
    pub shaping_ver: u16,
}

impl LocalLightPlan {
    /// A plan that does nothing, for a frame that needed nothing.
    ///
    /// Still carries a reason, because a plan with no reason is a bug rather than an empty
    /// plan.
    #[must_use]
    pub fn nothing(image_id: ImageId, scene: SceneId, reason: LocalReason) -> Self {
        Self {
            image_id,
            face_light: Vec::new(),
            subject: SubjectEnhanceDelta::NONE,
            background: BackgroundBalanceDelta::NONE,
            dodge_burn: None,
            shine: None,
            total_budget_used: 0.0,
            gated_by_mask_quality: Vec::new(),
            reasons: vec![reason],
            confidence: 1.0,
            scene,
            strengths: [0.0; LocalOp::COUNT],
            user_edited: false,
            reviewed: false,
            model_ver: 0,
            analysis_ver: 0,
            policy_ver: 0,
            shaping_ver: 0,
        }
    }

    /// True when this plan changes no pixel.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.face_light.iter().all(|(_, d)| d.is_noop())
            && self.subject.is_noop()
            && self.background.is_noop()
            && self.dodge_burn.as_ref().is_none_or(DodgeBurnMaps::is_empty)
            && self.shine.as_ref().is_none_or(ShineReduction::is_empty)
    }

    /// The strength one operation ran at.
    #[must_use]
    pub fn strength(&self, op: LocalOp) -> f32 {
        self.strengths.get(op.rank()).copied().unwrap_or(0.0)
    }

    /// True when this operation was gated by a mask.
    #[must_use]
    pub fn was_gated(&self, op: LocalOp) -> bool {
        self.gated_by_mask_quality.iter().any(|(o, _)| *o == op)
    }

    /// How many operations actually did something.
    #[must_use]
    pub fn active_operations(&self) -> usize {
        LocalOp::PRIORITY
            .iter()
            .filter(|op| self.strength(**op) > 0.0)
            .count()
    }

    /// The largest luminance difference between two faces after lighting.
    ///
    /// Section 10.1's group-fairness measurement, asked in one place so the solver, the store
    /// and the gate cannot disagree about it. Zero for a frame with fewer than two faces,
    /// which is a real answer rather than a missing one.
    #[must_use]
    pub fn inter_face_spread(&self) -> f32 {
        spread(self.face_light.iter().map(|(_, d)| d.luma_after))
    }

    /// The same difference before any lighting.
    ///
    /// The denominator of the fairness guarantee. Without it the guarantee would be a claim
    /// about the *frame* rather than about the *edit* - see [`LocalLightPlan::group_is_fair`].
    #[must_use]
    pub fn inter_face_spread_before(&self) -> f32 {
        spread(self.face_light.iter().map(|(_, d)| d.luma_before))
    }

    /// True when everybody in this frame ended up consistently lit.
    ///
    /// **The guarantee is about the edit, not about the frame**, and that distinction is the
    /// one thing to understand here. Section 10.1 asks for "inter-face luminance spread after
    /// lighting <= a documented threshold", and read as an absolute it is a promise this
    /// phase cannot keep and should not make: a family formal where one person stands two
    /// stops down under a doorway arrives with a 0.38 spread, the noise cap allows a 0.6 EV
    /// lift and no arithmetic closes the rest. The alternatives are both wrong - refuse to
    /// plan the frame, or darken everybody else to match the person nobody could light.
    ///
    /// So the guarantee is: **the lighting reaches the threshold whenever the caps allow, and
    /// it can never make a group less even than it found it.** A frame that arrives inside
    /// the threshold must stay inside it; a frame that arrives outside must come closer or
    /// stay where it is. `docs/adr/ADR-0039-local-light-sculpting.md` section 6 records the
    /// argument, and `crates/aura-brain-photo/src/local/face_light.rs` implements the second
    /// half by never brightening a face to meet the group - only ever giving back a lift.
    #[must_use]
    pub fn group_is_fair(&self) -> bool {
        let after = self.inter_face_spread();
        after <= MAX_INTER_FACE_SPREAD || after <= self.inter_face_spread_before() + 1e-4
    }

    /// True when the frame is worth a photographer's attention.
    #[must_use]
    pub fn needs_review(&self) -> bool {
        !self.reviewed && !self.user_edited && self.confidence < REVIEW_BELOW
    }

    /// What guarantee this plan breaks, if any.
    ///
    /// Five checks, and every one of them is an acceptance criterion rather than a type
    /// error: there is at least one reason, the budget was respected, the pairing held the
    /// mean luminance, the group is consistent, and no zone shaped in the wrong direction or
    /// too far. They live here rather than in the store so the solver, the eval harness and
    /// the store all refuse the same frames - and they return a sentence rather than an
    /// [`AuraError`] because `aura-brain-photo` owns this phase's error registry.
    /// `aura_brain_photo::local::guard` turns a `Some` here into `AURA-ML-5086`.
    #[must_use]
    pub fn broken_guarantee(&self) -> Option<String> {
        if self.reasons.is_empty() {
            return Some("a plan with no reason".into());
        }
        if !(0.0..=1.0).contains(&self.total_budget_used) {
            return Some(format!(
                "budget used {:.3} is outside 0..1",
                self.total_budget_used
            ));
        }
        if !self.background.is_noop() && self.background.luma_drift() > MAX_MEAN_LUMA_DRIFT {
            return Some(format!(
                "the pairing moved the frame's mean luminance by {:.3}, above \
                 {MAX_MEAN_LUMA_DRIFT:.3}",
                self.background.luma_drift()
            ));
        }
        if !self.group_is_fair() {
            return Some(format!(
                "the faces ended {:.3} apart, above {MAX_INTER_FACE_SPREAD:.3} and wider than \
                 the {:.3} they started at",
                self.inter_face_spread(),
                self.inter_face_spread_before()
            ));
        }
        if let Some(maps) = &self.dodge_burn {
            for face in &maps.faces {
                if let Some(zone) = face.zones.iter().find(|z| z.sign_is_wrong()) {
                    return Some(format!(
                        "the {} zone was deepened, and it may only be lifted",
                        zone.zone.as_str()
                    ));
                }
                if zone_gain_exceeded(&face.zones) {
                    return Some(format!(
                        "a shaping zone moved more than {:.3} EV",
                        ShapingZone::MAX_GAIN_EV
                    ));
                }
            }
        }
        None
    }

    /// True when this plan keeps every guarantee the phase makes.
    #[must_use]
    pub fn is_sound(&self) -> bool {
        self.broken_guarantee().is_none()
    }
}

/// The distance between the largest and smallest of a set of luminances.
///
/// Zero for fewer than two, which is a real answer rather than a missing one.
fn spread(values: impl Iterator<Item = f32>) -> f32 {
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    let mut count = 0usize;
    for value in values {
        lo = lo.min(value);
        hi = hi.max(value);
        count += 1;
    }
    if count < 2 {
        return 0.0;
    }
    (hi - lo).max(0.0)
}

/// True when any zone moves more than [`ShapingZone::MAX_GAIN_EV`].
fn zone_gain_exceeded(zones: &[ShapingZone]) -> bool {
    zones
        .iter()
        .any(|z| z.gain_ev.abs() > ShapingZone::MAX_GAIN_EV + 1e-4)
}

// ---------------------------------------------------------------------------
// The outline
// ---------------------------------------------------------------------------

/// What a project's local pass covered and what it found.
///
/// Phase 05's rule, inherited for the thirteenth time: report coverage when you report a
/// result, and say what the denominator is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LocalOutline {
    /// Photographs with a plan.
    pub planned: u32,
    /// Photographs in the project.
    pub photos: u32,
    /// Fraction of the project with a plan, `0..1`.
    ///
    /// The denominator is **every photograph**, as phases 09, 10, 11, 14 and 15's are.
    pub coverage: f32,
    /// Fraction of *planned* frames where at least one operation actually ran, `0..1`.
    ///
    /// **The number that matters when it is low, and this phase's own refinement of the
    /// rule.** A wedding at 100 % coverage and 4 % acted-on has been walked past rather than
    /// worked on - and because the whole point of the phase is that its work is invisible,
    /// that is a state a photographer would otherwise have no way to notice.
    pub acted_on: f32,
    /// Fraction of planned frames where every mask an operation wanted arrived, `0..1`.
    ///
    /// The second number, and the one that says whether phase 18 is doing its job. Zero on a
    /// build with no mask generator, which is the honest reading of such a build.
    pub mask_covered: f32,
    /// How many frames each operation ran on, in [`LocalOp::PRIORITY`] order.
    ///
    /// Section 11's `local.applied {ops_histogram}`.
    pub op_histogram: [u32; LocalOp::COUNT],
    /// How many operations each mask kind gated, in [`MaskKind::ALL`] order.
    ///
    /// Section 11's `local.gated {mask_kind, count}`.
    pub gated_histogram: [u32; MaskKind::COUNT],
    /// Mean fraction of the per-image budget spent, over frames that spent any.
    ///
    /// Section 11's `local.applied {mean_budget_used}`.
    pub mean_budget_used: f32,
    /// Frames where shine was reduced.
    ///
    /// Section 11's `local.shine_reduced {count}`.
    pub shine_reduced: u32,
    /// Mean shine reduction over those frames, in stops.
    pub mean_shine_ev: f32,
    /// Frames where faces were solved jointly.
    pub group_solved: u32,
    /// Frames below [`REVIEW_BELOW`] that nobody has reviewed.
    pub needs_review: u32,
    /// Frames the photographer has set by hand.
    pub user_edited: u32,
    /// Scenes with no policy row, by slug.
    pub unpolicied_scenes: Vec<String>,
    /// Which learned head produced the targets.
    pub model_ver: u16,
    /// Which build's arithmetic produced the decisions.
    pub analysis_ver: u16,
    /// Which policy file the scene strengths came from.
    pub policy_ver: u16,
    /// Which build's derivation turns zones into grids.
    pub shaping_ver: u16,
}

impl LocalOutline {
    /// True when nothing has been planned.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.planned == 0
    }
}

// ---------------------------------------------------------------------------
// Overrides
// ---------------------------------------------------------------------------

/// What the photographer set instead.
///
/// Per operation, and every field optional and independent: somebody who turned the shaping
/// off has not made a claim about the face lighting, and an override carrying all six would
/// silently freeze the five they did not touch. The same shape as
/// [`crate::contract::tone::ToneOverride`] and for the same reason.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalOverride {
    /// Strength for one operation, `0..1`, in [`LocalOp::PRIORITY`] order.
    ///
    /// `None` leaves the product's own strength in place.
    pub strengths: [Option<f32>; LocalOp::COUNT],
}

impl LocalOverride {
    /// An override that sets one operation.
    #[must_use]
    pub fn one(op: LocalOp, strength: f32) -> Self {
        let mut out = Self::default();
        if let Some(slot) = out.strengths.get_mut(op.rank()) {
            *slot = Some(strength);
        }
        out
    }

    /// True when this override sets nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strengths.iter().all(Option::is_none)
    }

    /// What is wrong with this override, if anything.
    ///
    /// `aura_brain_photo::local::guard` turns a `Some` here into `AURA-ML-5085`.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if self.is_empty() {
            return Some("the override sets nothing".into());
        }
        for (index, slot) in self.strengths.iter().enumerate() {
            if let Some(value) = slot {
                if !(0.0..=1.0).contains(value) {
                    let op = LocalOp::PRIORITY
                        .get(index)
                        .map_or("an operation", |op| op.as_str());
                    return Some(format!("{op} strength {value:.3} is outside 0..1"));
                }
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask how light was shaped inside a photograph.
///
/// Thirteenth service of its kind, and it carries the rule for the thirteenth time and for
/// the same reason: **no phase may keep its own idea of what was done locally to a frame.**
/// Phase 20 retouches skin this phase has already evened and must not do it twice, phase 25
/// normalises a gallery whose frames were each shaped differently, and phase 27 has to be
/// able to answer "why does this one look edited". Two answers to "what did we do to this
/// face" is a portrait that gets lifted twice.
pub trait LocalService: Send + Sync + fmt::Debug {
    /// What a project's pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<LocalOutline>;

    /// One photograph's plan, or `None` when it has not been planned.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plan cannot be read.
    fn of_image(&self, image: ImageId) -> AuraResult<Option<LocalLightPlan>>;

    /// The frames whose local work is worth a photographer's attention, weakest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>>;

    /// How many frames each operation ran on, across a project.
    ///
    /// Read once per panel rather than per frame, for the reason
    /// [`crate::contract::tone::ToneService::skin_loci`] gives.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn op_counts(&self, project: ProjectId) -> AuraResult<BTreeMap<LocalOp, u32>>;

    /// Record that the photographer has looked at this plan and agrees.
    ///
    /// Sets [`LocalLightPlan::reviewed`] and does not set
    /// [`LocalLightPlan::user_edited`]: accepting a suggestion is not authoring one, and
    /// phase 30's learning loop needs to be able to tell those apart.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5085` when the photograph has no plan.
    fn accept(&self, image: ImageId) -> Result<(), AuraError>;

    /// Record what the photographer set instead.
    ///
    /// Sets [`LocalLightPlan::user_edited`] and is not undone by a re-analysis: the check is
    /// inside the statement that would overwrite the row, exactly as
    /// `identities.user_locked`, `segments.user_locked`, `moments.user_locked`,
    /// `image_integrity.user_reviewed`, `image_composition.dismissed` and
    /// `image_tone_estimate.user_edited` are.
    ///
    /// **This records the disagreement; it does not move a pixel.** The pixels move when the
    /// caller writes the same strengths through `aura_recipe::schema::merge`, which is the
    /// only function in the workspace permitted to add to `user_edited_fields`.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5085` when the photograph has no plan, when the override is empty, or when a
    /// strength is outside `0..1`.
    fn set_override(&self, image: ImageId, values: LocalOverride) -> Result<(), AuraError>;
}
