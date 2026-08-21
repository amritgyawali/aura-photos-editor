//! FROZEN CONTRACT. The small fixes a retoucher makes without being asked: a stray hair calmed,
//! teeth and eyes evened, a lint taken off a lapel, a reflection lifted off a pair of glasses.
//!
//! PHASE-21 section 5 freezes [`MicroOp`] and [`NaturalnessGuard`] before any solver exists. The
//! file is in `aura-core` for the reason [`crate::contract::retouch`],
//! [`crate::contract::local`], [`crate::contract::colour`] and [`crate::contract::tone`] are:
//! the phases that consume a micro-retouch decision are 25 (gallery consistency, which
//! normalises frames whose small fixes differ), 27 (QC, which has to be able to say why a face
//! looks worked on) and 28 (autopilot, which must know what ran unattended), and none of them
//! needs the detectors, the alignment search or the band arithmetic.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **Everything this phase touches is permanent.** Phase 20 could separate a spot from a mole
//! and act only on the first; there is no such separation here, because teeth, eyes, hair and
//! clothing are all features of a person rather than things that happened to them. What replaces
//! the veto is a **ceiling enforced on the pixel**: [`NaturalnessGuard`] holds three floors that
//! are measured on the rendered result and not on the parameter that was solved, and an
//! operation family that misses its floor after three re-solves is withdrawn rather than
//! attenuated. `docs/adr/ADR-0043-micro-retouch-and-cross-frame-borrowing.md` section 5 has the
//! argument.
//!
//! ## The second thing: a borrow may only replace pixels that carry no information
//!
//! [`GlareMethod::BorrowFrom`] is the first operation in this product that composites two
//! photographs, and section 2.2 forbids the version everybody asks for first - opening a closed
//! eye from a sibling frame. Both are "take pixels from another frame of the same person in the
//! same moment", so the rule that permits one and refuses the other cannot be about the
//! mechanism. It is about what is underneath: a specular sheet has destroyed the record, and a
//! closed eye *is* the record. [`MIN_SPECULAR_FRACTION`] is that rule as a number, and the
//! source image id lives inside the variant so that an undisclosed composite is not a
//! representable state.
//!
//! ## The third thing: there is no absolute colour target anywhere in here
//!
//! [`ColourLocus`] is a bounded region in CIE `u'v'` **relative to the frame's own measured
//! neutral**, which phase 15 produces, and the operators reduce a distance to it rather than
//! moving toward its centre. A chromaticity already inside is not moved at all. Phase 15 wrote
//! the rule this follows - a target is measured, never assumed, and the schema cannot express an
//! alternative - and the phase gate scans migration 21 for a constant that would break it.
//!
//! ## What this contract cannot express
//!
//! There is no field here for a displacement, a scale, a landmark move, a skin-tone target or a
//! face swap. Section 11 of `docs/plan/CLAUDE.md` forbids all of them permanently and
//! `docs/retouch-ethics.md` says so to a photographer; `crates/aura-core/tests/micro_contract.rs`
//! asserts that no variant of [`MicroOp`] carries a field that could hold one.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::composition::Box2;
use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{IdentityId, ProjectId};
use crate::contract::local::{FULL_MASK_CONFIDENCE, MIN_MASK_CONFIDENCE};
use crate::contract::scene::SceneId;

pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// Ceilings, floors and triggers
// ---------------------------------------------------------------------------

/// The largest luminance lift a teeth correction may apply, in stops.
///
/// A fifth of a stop. Section 6.2 asks for "a ceiling far below cosmetic whitening", and this is
/// what that is in a number a solver can be held to: 0.20 EV is roughly the difference between
/// teeth in shadow and teeth in the same light a moment later, and it is well under the quarter
/// stop phase 20 allows under an eye - deliberately, because a lifted eye socket reads as rested
/// and lifted teeth read as *whitened* at half the magnitude.
///
/// It is not the whole guarantee. The operator additionally refuses to raise teeth above the
/// brightest non-specular skin on that person's own face in the same frame, which is a
/// comparison against the subject rather than against a constant.
pub const MAX_TEETH_LUMA_EV: f32 = 0.20;

/// The largest share of its own excess a teeth chromaticity may be moved toward the locus.
///
/// Thirty-five per cent. A full correction to the locus boundary produces the "denture" look
/// even when the boundary itself is measured, because real teeth vary and the ones that were
/// furthest out were furthest out for a reason. A third of the way is the point at which a
/// retoucher stops describing the frame as yellow and has not yet started describing it as
/// corrected.
pub const MAX_TEETH_YELLOW: f32 = 0.35;

/// The largest sclera redness reduction, `0..1`.
///
/// Chroma only, always. Section 2.1 says "sclera redness reduction" and section 6.2 says "chroma
/// only" and "no whitening of the sclera beyond a cap"; those are the same sentence read twice,
/// and this is the cap. Thirty per cent of the measured redness excess, which takes an
/// end-of-night eye back to where it was two hours earlier and no further.
pub const MAX_SCLERA: f32 = 0.30;

/// The largest iris local-contrast gain, `0..1`.
///
/// A quarter. Above this the iris starts to read as *drawn*: the radial fibres separate, the
/// limbal ring hardens, and the eye acquires the specific artificial look that section 1 calls
/// "alien eyes". No colour move is permitted at any value, at all - see [`MicroOp::Eyes`].
pub const MAX_IRIS_CLARITY: f32 = 0.25;

/// The largest fraction of the frame flyaway reduction may act on.
///
/// Four thousandths. Section 6.1 asks for "a strict area cap (fraction of frame)" to prevent a
/// runaway edit, and the number is set from what a flyaway *is*: a few dozen strands, each a
/// pixel or two wide, over a face at a usual portrait size. An operation that wanted more than
/// this has stopped describing stray hair and started describing the hair.
pub const MAX_FLYAWAY_AREA: f32 = 0.004;

/// The largest flyaway contrast attenuation, `0..1`.
///
/// Sixty per cent. Section 6.1's own words are "reduce rather than remove ... preserving some
/// strands so the hairline still reads as real hair", and a full attenuation is a removal
/// whatever it is called. At 0.6 a strand that was clearly visible is still visible and no longer
/// draws the eye, which is what a retoucher does by hand.
pub const MAX_FLYAWAY_STRENGTH: f32 = 0.60;

/// The most detail a background may carry and still be flyaway-safe, `0..1`.
///
/// Section 6.1: "require a clean, low-detail background, otherwise skip". This is the floor
/// expressed as a ceiling on the background's own normalised edge energy. It is the single most
/// important number in the hair module and it is deliberately strict: a detector cannot tell a
/// strand from a twig, so where the background is busy the operation is skipped rather than
/// guessed.
pub const MAX_FLYAWAY_BACKGROUND_DETAIL: f32 = 0.18;

/// The largest clothing cleanup strength, `0..1`.
pub const MAX_CLOTHING_STRENGTH: f32 = 0.85;

/// The largest fraction of the frame one clothing anomaly may cover.
///
/// A thousandth. Above this it is not lint, and section 2.2 puts removing objects in phase 24.
pub const MAX_CLOTHING_AREA: f32 = 0.001;

/// The largest conservative glare reduction, `0..1`.
pub const MAX_GLARE_REDUCE: f32 = 0.70;

/// The largest fraction of the frame a cross-frame borrow may cover.
///
/// Two thousandths. Section 6.3: "cross-frame borrowing is limited to small regions". A borrow
/// this size cannot move a head, a hand or an expression, which is the property that makes the
/// bound worth having rather than the number itself.
pub const MAX_BORROW_AREA: f32 = 0.002;

/// How much of a borrow region must be blown specular in the target frame, `0..1`.
///
/// **The rule the whole borrowing design rests on.** Fifty-five per cent. See the module header
/// and ADR-0043 section 4: you may only borrow pixels that carry no information, and a region
/// that is more than half at the sensor's ceiling carries none. Below this the conservative
/// highlight reduction runs instead and the plan says which.
pub const MIN_SPECULAR_FRACTION: f32 = 0.55;

/// The alignment score a borrow needs before it is permitted, `0..1`.
///
/// Section 6.3: "requires high alignment confidence". Measured as normalised cross-correlation
/// of the aligned sibling region against the ring of *unblown* pixels around the target region -
/// the ring rather than the region, for the reason phase 20 measured a heal against the ring: the
/// thing being repaired is inside the window and would otherwise set the target it is scored
/// against.
pub const MIN_ALIGNMENT: f32 = 0.82;

/// The confidence below which an operation is skipped entirely.
///
/// Section 5's `require_confidence`. Not a scaling factor: below this the operation does not run
/// at all, because a half-strength edit made on a doubtful detection is still an edit in the
/// wrong place.
pub const MIN_OP_CONFIDENCE: f32 = 0.55;

/// The catchlight peak a plan must leave standing, as a fraction of what it found.
///
/// Ninety-eight per cent. Section 10.1's "catchlights preserved (specular pixel test)", and it is
/// nearly one on purpose: a catchlight is what makes an eye look alive, every operator in the eye
/// module excludes specular pixels by construction, and the two per cent of slack is for the
/// resampling rather than for the edit. Measured on the rendered result.
pub const CATCHLIGHT_FLOOR: f32 = 0.98;

/// The hair edge energy a plan must leave standing, as a fraction of what it found.
///
/// Ninety-four per cent. Section 10.1's "no bald patches or hairline damage", measured the way
/// phase 20's texture floor is measured: the region's own energy after the plan divided by the
/// energy before it, on pixels the renderer actually produced. A bald patch is a large local loss
/// of this quantity and nothing else.
pub const HAIR_ENERGY_FLOOR: f32 = 0.94;

/// The largest distance any teeth pixel may end outside the locus, in `u'v'`.
///
/// Three thousandths, which is below a just-noticeable difference at these chromaticities. The
/// number is a *tolerance on the guarantee* rather than a licence: the operator only ever removes
/// a bounded share of the distance to the locus, so it can never move a chromaticity past the
/// boundary, and a non-zero reading after rendering is resampling and interpolation rather than
/// intent.
///
/// **What is measured is the change, not the distance.** See
/// [`NaturalnessReport::teeth_excursion`]: an absolute reading would hold the guard to something
/// the operator is deliberately not permitted to achieve - `MAX_TEETH_YELLOW` removes about a
/// third of the excess, so a strongly yellow set of teeth is still outside the locus afterwards
/// and is *supposed* to be. Holding the absolute distance to a ceiling this tight would withdraw
/// the teeth family on exactly the photographs it exists for, which is what the end-to-end
/// fixture caught.
pub const TEETH_EXCURSION_CEILING: f32 = 0.003;

/// How much strength a family gives up on each re-solve.
///
/// Three quarters, phase 20's step. A gentler step needs more passes for the same result and
/// each pass is a render.
pub const NATURALNESS_RESOLVE_STEP: f32 = 0.75;

/// How many times a family may be re-solved before it is withdrawn.
pub const NATURALNESS_MAX_RESOLVES: u8 = 3;

/// The most re-solves a whole plan may take: every family, at its own limit.
///
/// Spelled as a `u8` constant rather than computed from [`OpFamily::COUNT`], because that count
/// is a `usize` and casting it here would be a narrowing conversion in a contract. The assertion
/// below is what keeps the two in step: adding a fourth family fails the build here rather than
/// silently leaving a plan able to spend twelve renders while the check still allows nine.
pub const NATURALNESS_MAX_RESOLVES_TOTAL: u8 = NATURALNESS_MAX_RESOLVES * 3;

const _: () = assert!(
    OpFamily::COUNT == 3,
    "NATURALNESS_MAX_RESOLVES_TOTAL assumes three families"
);

/// The confidence below which a plan is worth a photographer's attention.
pub const REVIEW_BELOW: f32 = 0.50;

/// The most operations one plan may carry.
///
/// Eighty. Lower than phase 20's two hundred, because these operations are each larger in
/// perceptual terms and because eighty micro-fixes on one photograph is a photograph that needed
/// a different frame.
pub const MAX_OPS: usize = 80;

// ---------------------------------------------------------------------------
// Regions
// ---------------------------------------------------------------------------

/// The regions this phase reads, as a projection of phase 18's twenty-class vocabulary.
///
/// **Not a second vocabulary.** [`MicroRegion::as_mask_str`] is total onto phase 18's own
/// spellings, the shape [`crate::contract::local::MaskKind::as_recipe_str`] uses, so this is a
/// view of one answer rather than a competing one. ADR-0043 section 7 records why it is not
/// phase 19's enum widened and not a dependency on `aura-vision`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum MicroRegion {
    /// Head hair, including the soft boundary matting refines.
    #[default]
    Hair,
    /// Visible teeth.
    Teeth,
    /// The whites of the eyes.
    Sclera,
    /// The coloured part of the eyes.
    Iris,
    /// Both eye regions, including the lids. Read to place a glare sheet, never written.
    Eyes,
    /// Worn fabric that is not a bridal dress.
    Clothing,
    /// A bridal dress or a veil.
    Dress,
    /// Visible skin. **Read as evidence and never written** - phase 20 owns skin, and a second
    /// phase smoothing it is a face flattened twice.
    Skin,
    /// The face proper. Read to bound the eye and teeth work.
    Face,
    /// Everything that is not the subject. Read to gate flyaway reduction.
    Background,
}

impl MicroRegion {
    /// Every region, in the order `docs/micro-retouch.md` documents them.
    pub const ALL: [Self; 10] = [
        Self::Hair,
        Self::Teeth,
        Self::Sclera,
        Self::Iris,
        Self::Eyes,
        Self::Clothing,
        Self::Dress,
        Self::Skin,
        Self::Face,
        Self::Background,
    ];

    /// How many regions there are.
    pub const COUNT: usize = 10;

    /// Stable text for the catalog and the wire. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hair => "hair",
            Self::Teeth => "teeth",
            Self::Sclera => "sclera",
            Self::Iris => "iris",
            Self::Eyes => "eyes",
            Self::Clothing => "clothing",
            Self::Dress => "dress",
            Self::Skin => "skin",
            Self::Face => "face",
            Self::Background => "background",
        }
    }

    /// The spelling `aura_vision::contract::mask::MaskKind::as_str` uses for the same region.
    ///
    /// Total by construction, which is the property that makes this a view rather than a second
    /// vocabulary. Every region here names exactly one of phase 18's twenty classes, and the
    /// reverse is deliberately not total: ten of phase 18's classes have nothing to do with this
    /// phase and a `From` that invented a micro region for `sky` would be a mask generator hiding
    /// in a conversion.
    #[must_use]
    pub const fn as_mask_str(self) -> &'static str {
        // Identical strings today, and written out rather than returned from `as_str` because
        // the two are different contracts: this one may not drift when phase 18 renames a class,
        // and a test in `aura-vision` is what would catch it.
        match self {
            Self::Hair => "hair",
            Self::Teeth => "teeth",
            Self::Sclera => "sclera",
            Self::Iris => "iris",
            Self::Eyes => "eyes",
            Self::Clothing => "clothing",
            Self::Dress => "dress",
            Self::Skin => "skin",
            Self::Face => "face",
            Self::Background => "background",
        }
    }

    /// Parse the catalog spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|region| region.as_str() == text)
    }

    /// True when an operation in this phase may act on this region.
    ///
    /// Four regions are read-only. [`MicroRegion::Skin`] because phase 20 owns it,
    /// [`MicroRegion::Eyes`] because it is a bound rather than a target,
    /// [`MicroRegion::Face`] for the same reason, and [`MicroRegion::Background`] because this
    /// phase never edits a background - it only asks whether one is quiet enough to work in
    /// front of.
    #[must_use]
    pub const fn is_writable(self) -> bool {
        matches!(
            self,
            Self::Hair | Self::Teeth | Self::Sclera | Self::Iris | Self::Clothing | Self::Dress
        )
    }
}

impl fmt::Display for MicroRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One region, as phase 21 reads it.
///
/// **The input port, and the whole of it.** Phase 18 fills these; this phase has no other route
/// to a region and no way to make one. Same shape and same semantics as phase 19's
/// [`crate::contract::local::MaskField`], with this phase's wider region vocabulary - see
/// ADR-0043 section 7 for why that is a projection rather than a duplicate.
///
/// The two quality numbers are two numbers for phase 18's reason: confidence is how sure the
/// class is and edge quality is how well determined the boundary is. They fail independently, and
/// here the second matters more than it has anywhere: hair against a bright window is the
/// standard bad-boundary case and it is also the region this phase most wants to work in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MicroField {
    /// What the region is of.
    pub region: MicroRegion,
    /// Who it is of, when it is of somebody.
    #[serde(default)]
    pub identity: Option<IdentityId>,
    /// Where the non-zero region sits, in frame coordinates.
    pub bounds: Box2,
    /// The grid's width in samples.
    pub width: u16,
    /// The grid's height in samples.
    pub height: u16,
    /// Coverage per sample, `0..=255`, row-major, `width * height` long, over the whole frame.
    pub alpha: Vec<u8>,
    /// How sure the generator is that this is the right region, `0..1`.
    pub confidence: f32,
    /// How good the boundary is, `0..1`.
    pub edge_quality: f32,
    /// Which phase 18 model produced it.
    pub model_ver: u16,
}

impl MicroField {
    /// The largest grid this phase will accept, per side. Phase 19's bound, and for its reason.
    pub const MAX_SIDE: u16 = 256;

    /// Fraction of the frame this region covers, `0..1`.
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn coverage(&self) -> f32 {
        if self.alpha.is_empty() {
            return 0.0;
        }
        let total: u64 = self.alpha.iter().map(|&a| u64::from(a)).sum();
        (total as f64 / (self.alpha.len() as f64 * 255.0)) as f32
    }

    /// The region's own strength multiplier, `0..1`.
    ///
    /// Phase 19's ramp, reading phase 19's constants: [`MIN_MASK_CONFIDENCE`] to
    /// [`FULL_MASK_CONFIDENCE`], multiplied by the edge quality, zero below the floor. The
    /// *decision* about how much a doubtful region may do lives in those two constants and is
    /// therefore made once for the whole product; this is the three lines that read it.
    /// `crates/aura-core/tests/micro_contract.rs` asserts the two agree at the boundaries.
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
    /// A sentence rather than an [`AuraError`]: `aura-core` owns the shape and `aura-retouch`
    /// owns the error registry, the split every phase since 09 has kept.
    /// `aura_retouch::micro::guard` turns a `Some` here into `AURA-ML-5100`.
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
// The colour locus
// ---------------------------------------------------------------------------

/// A bounded region of chromaticity, **relative to the frame's own measured neutral**.
///
/// Section 5 names the type and does not define it; ADR-0043 section 3 argues for this shape.
/// The centre is an offset in CIE `u'v'` from whatever phase 15 measured the illuminant to be,
/// so the same locus describes plausible teeth under tungsten and under an overcast sky without
/// containing a single absolute colour.
///
/// **There is no method on this type that moves a chromaticity toward the centre.** The only
/// operation is [`ColourLocus::excess`], which reports how far outside the boundary a
/// chromaticity is, and the operators reduce that excess by a bounded fraction. A chromaticity
/// already inside is at excess zero and is never touched - which is the difference between a
/// correction and a target.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ColourLocus {
    /// `u'` offset from the frame's neutral.
    pub du: f32,
    /// `v'` offset from the frame's neutral.
    pub dv: f32,
    /// The radius of the region in `u'v'`.
    pub radius: f32,
}

impl ColourLocus {
    /// A locus that accepts everything, for a frame with no illuminant estimate.
    ///
    /// Radius is deliberately large rather than infinite so that
    /// [`ColourLocus::problem`] still has something to check. Nothing is corrected against it,
    /// because a locus with no origin is refused one step earlier - see
    /// [`MicroCode::NoIlluminant`].
    pub const OPEN: Self = Self {
        du: 0.0,
        dv: 0.0,
        radius: 1.0,
    };

    /// How far outside the boundary a chromaticity sits, in `u'v'`. Zero when it is inside.
    ///
    /// `u` and `v` are offsets from the same neutral the centre is expressed against, so the
    /// caller does the subtraction once per frame rather than once per pixel.
    #[must_use]
    pub fn excess(&self, du: f32, dv: f32) -> f32 {
        let distance = ((du - self.du).powi(2) + (dv - self.dv).powi(2)).sqrt();
        (distance - self.radius).max(0.0)
    }

    /// True when a chromaticity offset is inside the region.
    #[must_use]
    pub fn contains(&self, du: f32, dv: f32) -> bool {
        self.excess(du, dv) <= 0.0
    }

    /// What is wrong with this locus, if anything.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if !self.radius.is_finite() || self.radius <= 0.0 {
            return Some("a colour locus with no radius".into());
        }
        if !self.du.is_finite() || !self.dv.is_finite() {
            return Some("a colour locus with a non-finite centre".into());
        }
        if self.du.abs() > 0.5 || self.dv.abs() > 0.5 {
            // A `u'v'` offset of half a unit is off the spectral locus entirely. A file that
            // asks for one has a decimal point in the wrong place.
            return Some("a colour locus centre outside any plausible chromaticity".into());
        }
        None
    }
}

// ---------------------------------------------------------------------------
// The operations
// ---------------------------------------------------------------------------

/// What is wrong with a piece of clothing.
///
/// Section 5 names `ClothingIssue` and never defines it; these five come from section 2.1's own
/// list. [`ClothingIssue::Strap`] and [`ClothingIssue::Crease`] are the two section 2.1 marks
/// opt-in, and they are variants rather than a flag so that the opt-in matrix is keyed by the
/// same closed set the operator emits.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ClothingIssue {
    /// A speck of lint or fluff.
    #[default]
    Lint,
    /// A stray thread.
    Thread,
    /// A small stain or mark.
    Stain,
    /// A visible bra strap. **Off by default**, permanently: a garment somebody chose to wear.
    Strap,
    /// A crease or wrinkle. **Off by default**: fabric creases are what fabric does, and
    /// removing them is the single most artificial-looking operation in this phase.
    Crease,
}

impl ClothingIssue {
    /// Every issue, in the order `config/micro_retouch.toml` lists them.
    pub const ALL: [Self; 5] = [
        Self::Lint,
        Self::Thread,
        Self::Stain,
        Self::Strap,
        Self::Crease,
    ];

    /// How many kinds there are.
    pub const COUNT: usize = 5;

    /// Stable text for the catalog and the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lint => "lint",
            Self::Thread => "thread",
            Self::Stain => "stain",
            Self::Strap => "strap",
            Self::Crease => "crease",
        }
    }

    /// Parse the catalog spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|issue| issue.as_str() == text)
    }

    /// True when this issue may only run because somebody switched it on.
    ///
    /// **Enforced in the type rather than in the config loader**, so a table that set
    /// `default_on = true` for one of these two is refused rather than obeyed.
    /// `docs/retouch-ethics.md` section 4 says the same thing to a photographer.
    #[must_use]
    pub const fn is_opt_in_only(self) -> bool {
        matches!(self, Self::Strap | Self::Crease)
    }
}

impl fmt::Display for ClothingIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a glare sheet was dealt with.
///
/// Section 5's own comment - `Reduce | BorrowFrom(ImageId)` - as a type. Two variants rather than
/// a method name and a nullable source, because **a borrow that lost its source id is an
/// undisclosed composite** and there must be no representable state in which one exists.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "how")]
pub enum GlareMethod {
    /// The highlight was reduced conservatively, using only this frame.
    Reduce {
        /// How strongly, `0..1`, bounded by [`MAX_GLARE_REDUCE`].
        strength: f32,
    },
    /// The region was reconstructed from a sibling frame of the same moment.
    ///
    /// Permitted only where the target region carries no information - see
    /// [`MIN_SPECULAR_FRACTION`] - and always disclosed.
    BorrowFrom {
        /// The photograph the pixels came from.
        source: ImageId,
        /// How well the two regions aligned, `0..1`. At or above [`MIN_ALIGNMENT`].
        alignment: f32,
    },
}

impl GlareMethod {
    /// The stable text for the catalog and the wire.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Reduce { .. } => "reduce",
            Self::BorrowFrom { .. } => "borrow",
        }
    }

    /// The photograph this method borrowed from, when it borrowed.
    #[must_use]
    pub const fn source(&self) -> Option<ImageId> {
        match self {
            Self::Reduce { .. } => None,
            Self::BorrowFrom { source, .. } => Some(*source),
        }
    }

    /// True when this method composites two photographs.
    #[must_use]
    pub const fn is_composite(&self) -> bool {
        matches!(self, Self::BorrowFrom { .. })
    }
}

/// The five things this phase can do to a photograph.
///
/// Section 5's own enum. A closed set, because these operations spend against the allowance
/// phases 19 and 20 share and a budget with an open-ended list of spenders is not a budget.
///
/// **No variant carries a displacement, a scale or a landmark.** That is the structural half of
/// section 11 of `docs/plan/CLAUDE.md`: there is nowhere here to put a reshaping, so adding one
/// is a visible contract change rather than a quiet field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// `tag = "op"` rather than `tag = "kind"`: section 5 spells the clothing discriminator `kind`,
// and a serde tag may not collide with a field name. The field keeps the frozen spelling and
// the tag moves, which is the substitution that changes nothing a reader depends on.
#[serde(rename_all = "snake_case", tag = "op")]
pub enum MicroOp {
    /// Stray hair, attenuated against the background behind it.
    ///
    /// Never removed and never modified inside the hair mass - section 6.1.
    Flyaway {
        /// Where, normalised to the frame.
        region: Box2,
        /// How strongly, `0..1`, bounded by [`MAX_FLYAWAY_STRENGTH`].
        strength: f32,
    },
    /// One person's teeth, evened and de-yellowed inside the locus.
    Teeth {
        /// Whose.
        identity: IdentityId,
        /// Luminance lift in stops, bounded by [`MAX_TEETH_LUMA_EV`].
        luma: f32,
        /// Share of the chromaticity's own excess removed, `0..1`, bounded by
        /// [`MAX_TEETH_YELLOW`].
        yellow_reduce: f32,
    },
    /// One person's eyes: redness out of the sclera, a little local contrast into the iris.
    ///
    /// **No colour move and no geometry move at any value of either field.** There is nowhere
    /// here to put a hue rotation or a scale, which is the point.
    Eyes {
        /// Whose.
        identity: IdentityId,
        /// Share of the measured sclera redness excess removed, `0..1`, bounded by
        /// [`MAX_SCLERA`].
        sclera: f32,
        /// Iris local contrast gain, `0..1`, bounded by [`MAX_IRIS_CLARITY`].
        iris_clarity: f32,
    },
    /// One distraction on a garment, inpainted.
    Clothing {
        /// Where, normalised to the frame.
        region: Box2,
        /// What it is.
        kind: ClothingIssue,
        /// How strongly, `0..1`, bounded by [`MAX_CLOTHING_STRENGTH`].
        strength: f32,
    },
    /// One specular sheet over a pair of glasses.
    Glare {
        /// Where, normalised to the frame.
        region: Box2,
        /// Reduced from this frame, or borrowed from a named sibling.
        method: GlareMethod,
    },
}

impl MicroOp {
    /// The stable operator name, which is also the recipe's `micro[].op` spelling.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Flyaway { .. } => "flyaway",
            Self::Teeth { .. } => "teeth",
            Self::Eyes { .. } => "eyes",
            Self::Clothing { .. } => "clothing",
            Self::Glare { .. } => "glare",
        }
    }

    /// Every operator name, in the order `docs/micro-retouch.md` documents them.
    pub const NAMES: [&'static str; 5] = ["flyaway", "teeth", "eyes", "clothing", "glare"];

    /// Which naturalness measurement holds this operation.
    ///
    /// The mapping the guard withdraws by - see [`NaturalnessReport`] and ADR-0043 section 5.
    /// Clothing has no family because no naturalness floor covers fabric: its guarantee is the
    /// area cap and the fabric-texture test in the eval harness, which are checks on the
    /// *operation* rather than on the rendered region.
    #[must_use]
    pub const fn family(&self) -> Option<OpFamily> {
        match self {
            Self::Flyaway { .. } => Some(OpFamily::Hair),
            Self::Teeth { .. } => Some(OpFamily::Teeth),
            Self::Eyes { .. } | Self::Glare { .. } => Some(OpFamily::Eyes),
            Self::Clothing { .. } => None,
        }
    }

    /// How strongly this operation runs, `0..1`.
    ///
    /// The two that carry two magnitudes report the larger as a fraction of its own cap, which
    /// is the number the budget spends against and the number the panel shows.
    #[must_use]
    pub fn strength(&self) -> f32 {
        match self {
            Self::Flyaway { strength, .. } | Self::Clothing { strength, .. } => *strength,
            Self::Teeth {
                luma,
                yellow_reduce,
                ..
            } => ((luma.abs() / MAX_TEETH_LUMA_EV).max(yellow_reduce.abs() / MAX_TEETH_YELLOW))
                .clamp(0.0, 1.0),
            Self::Eyes {
                sclera,
                iris_clarity,
                ..
            } => ((sclera.abs() / MAX_SCLERA).max(iris_clarity.abs() / MAX_IRIS_CLARITY))
                .clamp(0.0, 1.0),
            Self::Glare { method, .. } => match method {
                // A borrow is a full replacement of a region that carried nothing, so it spends
                // its cap rather than a fraction of it.
                GlareMethod::BorrowFrom { .. } => 1.0,
                GlareMethod::Reduce { strength } => *strength,
            },
        }
    }

    /// Where on the frame this operation acts, when it names a rectangle.
    ///
    /// `None` for the two that act through a landmark region. The panel draws an evidence
    /// rectangle for the first three and highlights a region for the others.
    #[must_use]
    pub fn region(&self) -> Option<Box2> {
        match self {
            Self::Flyaway { region, .. }
            | Self::Clothing { region, .. }
            | Self::Glare { region, .. } => Some(*region),
            Self::Teeth { .. } | Self::Eyes { .. } => None,
        }
    }

    /// The photograph this operation borrowed from, when it borrowed from one.
    #[must_use]
    pub const fn borrowed_from(&self) -> Option<ImageId> {
        match self {
            Self::Glare { method, .. } => method.source(),
            _ => None,
        }
    }

    /// What is wrong with this operation, if anything.
    ///
    /// Every ceiling in section 5 is checked here, so the solver, the store, the IPC layer and
    /// the eval harness all refuse the same operations. `aura_retouch::micro::guard` turns a
    /// `Some` into `AURA-ML-5097`.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn problem(&self) -> Option<String> {
        match self {
            Self::Flyaway { region, strength } => {
                if !(0.0..=1.0).contains(strength) {
                    return Some(format!(
                        "a flyaway strength of {strength:.3} is outside 0..1"
                    ));
                }
                if *strength > MAX_FLYAWAY_STRENGTH + 1e-4 {
                    return Some(format!(
                        "a flyaway strength of {strength:.3} is above {MAX_FLYAWAY_STRENGTH:.2}"
                    ));
                }
                if region.w <= 0.0 || region.h <= 0.0 {
                    return Some("a flyaway region with no area".into());
                }
                if region.w * region.h > MAX_FLYAWAY_AREA + 1e-6 {
                    return Some(format!(
                        "a flyaway region covering {:.4} of the frame is above \
                         {MAX_FLYAWAY_AREA:.4}",
                        region.w * region.h
                    ));
                }
                None
            }
            Self::Teeth {
                luma,
                yellow_reduce,
                ..
            } => {
                if luma.abs() > MAX_TEETH_LUMA_EV + 1e-4 {
                    return Some(format!(
                        "a teeth lift of {luma:.3} EV is above {MAX_TEETH_LUMA_EV:.2}"
                    ));
                }
                if *luma < -1e-4 {
                    // Darkening somebody's teeth is not an operation this product performs, and
                    // a negative lift arriving here means a solver sign error rather than an
                    // intent.
                    return Some("a teeth correction may not darken".into());
                }
                if !(0.0..=1.0).contains(yellow_reduce) {
                    return Some(format!(
                        "a yellow reduction of {yellow_reduce:.3} is outside 0..1"
                    ));
                }
                if *yellow_reduce > MAX_TEETH_YELLOW + 1e-4 {
                    return Some(format!(
                        "a yellow reduction of {yellow_reduce:.3} is above {MAX_TEETH_YELLOW:.2}"
                    ));
                }
                None
            }
            Self::Eyes {
                sclera,
                iris_clarity,
                ..
            } => {
                if !(0.0..=1.0).contains(sclera) {
                    return Some(format!(
                        "a sclera correction of {sclera:.3} is outside 0..1"
                    ));
                }
                if *sclera > MAX_SCLERA + 1e-4 {
                    return Some(format!(
                        "a sclera correction of {sclera:.3} is above {MAX_SCLERA:.2}"
                    ));
                }
                if !(0.0..=1.0).contains(iris_clarity) {
                    return Some(format!(
                        "an iris clarity of {iris_clarity:.3} is outside 0..1"
                    ));
                }
                if *iris_clarity > MAX_IRIS_CLARITY + 1e-4 {
                    return Some(format!(
                        "an iris clarity of {iris_clarity:.3} is above {MAX_IRIS_CLARITY:.2}"
                    ));
                }
                None
            }
            Self::Clothing {
                region, strength, ..
            } => {
                if !(0.0..=1.0).contains(strength) {
                    return Some(format!(
                        "a clothing strength of {strength:.3} is outside 0..1"
                    ));
                }
                if *strength > MAX_CLOTHING_STRENGTH + 1e-4 {
                    return Some(format!(
                        "a clothing strength of {strength:.3} is above {MAX_CLOTHING_STRENGTH:.2}"
                    ));
                }
                if region.w <= 0.0 || region.h <= 0.0 {
                    return Some("a clothing region with no area".into());
                }
                if region.w * region.h > MAX_CLOTHING_AREA + 1e-6 {
                    return Some(format!(
                        "a clothing region covering {:.5} of the frame is above \
                         {MAX_CLOTHING_AREA:.5}; removing something that large is phase 24",
                        region.w * region.h
                    ));
                }
                None
            }
            Self::Glare { region, method } => {
                if region.w <= 0.0 || region.h <= 0.0 {
                    return Some("a glare region with no area".into());
                }
                match method {
                    GlareMethod::Reduce { strength } => {
                        if !(0.0..=1.0).contains(strength) {
                            return Some(format!(
                                "a glare reduction of {strength:.3} is outside 0..1"
                            ));
                        }
                        if *strength > MAX_GLARE_REDUCE + 1e-4 {
                            return Some(format!(
                                "a glare reduction of {strength:.3} is above {MAX_GLARE_REDUCE:.2}"
                            ));
                        }
                        None
                    }
                    GlareMethod::BorrowFrom { alignment, .. } => {
                        if region.w * region.h > MAX_BORROW_AREA + 1e-6 {
                            return Some(format!(
                                "a borrowed region covering {:.4} of the frame is above \
                                 {MAX_BORROW_AREA:.4}",
                                region.w * region.h
                            ));
                        }
                        if *alignment < MIN_ALIGNMENT - 1e-4 {
                            return Some(format!(
                                "a borrow aligned at {alignment:.3} is below {MIN_ALIGNMENT:.2}"
                            ));
                        }
                        if !(0.0..=1.0).contains(alignment) {
                            return Some("a borrow alignment outside 0..1".into());
                        }
                        None
                    }
                }
            }
        }
    }
}

/// Which naturalness floor holds a family of operations.
///
/// Three families, three measurements, three independent withdrawals. ADR-0043 section 5.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum OpFamily {
    /// Flyaway reduction, held to [`HAIR_ENERGY_FLOOR`].
    #[default]
    Hair,
    /// Teeth correction, held to [`TEETH_EXCURSION_CEILING`].
    Teeth,
    /// Eye work and glare, held to [`CATCHLIGHT_FLOOR`].
    Eyes,
}

impl OpFamily {
    /// Every family, in the order the guard measures them.
    pub const ALL: [Self; 3] = [Self::Hair, Self::Teeth, Self::Eyes];

    /// How many families there are.
    pub const COUNT: usize = 3;

    /// Stable text for the catalog and the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hair => "hair",
            Self::Teeth => "teeth",
            Self::Eyes => "eyes",
        }
    }

    /// Parse the catalog spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|family| family.as_str() == text)
    }
}

impl fmt::Display for OpFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// The guard
// ---------------------------------------------------------------------------

/// The ceilings a plan is held to, as section 5 freezes them.
///
/// Loaded from `crates/aura-retouch/config/micro_retouch.toml`, **bounded by the code**: the
/// loader refuses a file that raises any of these above the constants in this module. A studio
/// may lower a ceiling and may switch an operation off; no studio setting can widen one, which
/// is what makes `docs/retouch-ethics.md` a promise rather than a description of the defaults.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NaturalnessGuard {
    /// The largest teeth luminance lift, in stops. At most [`MAX_TEETH_LUMA_EV`].
    pub teeth_max_luma: f32,
    /// The region plausible teeth chromaticities sit in, relative to the frame's neutral.
    pub teeth_locus: ColourLocus,
    /// The largest sclera redness reduction. At most [`MAX_SCLERA`].
    pub sclera_max: f32,
    /// The largest iris local contrast gain. At most [`MAX_IRIS_CLARITY`].
    pub iris_max: f32,
    /// The largest fraction of the frame flyaway reduction may act on. At most
    /// [`MAX_FLYAWAY_AREA`].
    pub flyaway_max_area_frac: f32,
    /// Below this an operation is skipped entirely. At least [`MIN_OP_CONFIDENCE`].
    pub require_confidence: f32,
}

impl NaturalnessGuard {
    /// The guard the contract itself permits: every ceiling at its maximum.
    ///
    /// What a table is compared against, and what the phase gate attempts to exceed.
    pub const CEILING: Self = Self {
        teeth_max_luma: MAX_TEETH_LUMA_EV,
        teeth_locus: ColourLocus {
            du: 0.0,
            dv: 0.0,
            radius: 0.030,
        },
        sclera_max: MAX_SCLERA,
        iris_max: MAX_IRIS_CLARITY,
        flyaway_max_area_frac: MAX_FLYAWAY_AREA,
        require_confidence: MIN_OP_CONFIDENCE,
    };

    /// What is wrong with this guard, if anything.
    ///
    /// Five ceilings that may not be raised and one floor that may not be lowered. A `Some` here
    /// is `AURA-ML-5099` and it is run-blocking: half a ceiling table would even the ceremony
    /// against measured limits and the reception against nothing, and that inconsistency is
    /// invisible in a delivered gallery.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if self.teeth_max_luma < 0.0 || self.teeth_max_luma > MAX_TEETH_LUMA_EV + 1e-6 {
            return Some(format!(
                "teeth_max_luma {:.3} is outside 0..{MAX_TEETH_LUMA_EV:.2}",
                self.teeth_max_luma
            ));
        }
        if let Some(problem) = self.teeth_locus.problem() {
            return Some(problem);
        }
        if self.sclera_max < 0.0 || self.sclera_max > MAX_SCLERA + 1e-6 {
            return Some(format!(
                "sclera_max {:.3} is outside 0..{MAX_SCLERA:.2}",
                self.sclera_max
            ));
        }
        if self.iris_max < 0.0 || self.iris_max > MAX_IRIS_CLARITY + 1e-6 {
            return Some(format!(
                "iris_max {:.3} is outside 0..{MAX_IRIS_CLARITY:.2}",
                self.iris_max
            ));
        }
        if self.flyaway_max_area_frac < 0.0 || self.flyaway_max_area_frac > MAX_FLYAWAY_AREA + 1e-9
        {
            return Some(format!(
                "flyaway_max_area_frac {:.5} is outside 0..{MAX_FLYAWAY_AREA:.4}",
                self.flyaway_max_area_frac
            ));
        }
        if self.require_confidence < MIN_OP_CONFIDENCE - 1e-6 || self.require_confidence > 1.0 {
            return Some(format!(
                "require_confidence {:.3} is outside {MIN_OP_CONFIDENCE:.2}..1",
                self.require_confidence
            ));
        }
        None
    }

    /// True when this guard keeps every bound the contract owns.
    #[must_use]
    pub fn is_sound(&self) -> bool {
        self.problem().is_none()
    }
}

/// What the guard measured on the rendered result.
///
/// Section 5 freezes the thresholds and says nothing about what was found. Phase 16 wrote the
/// rule and phase 20 repeated it: **a guarantee is measured, not asserted**, and a product that
/// could only assert one has no way to discover it has stopped keeping it. Section 0's headline
/// KPI is then `SELECT MIN(catchlight_ratio)` over a wedding rather than a sentence in a
/// document.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NaturalnessReport {
    /// Peak luminance inside the iris regions after the plan, over the same before it.
    ///
    /// One when there was nothing to measure. Held to [`CATCHLIGHT_FLOOR`].
    pub catchlight_ratio: f32,
    /// Edge energy in the hair region after the plan, over the same before it.
    ///
    /// Held to [`HAIR_ENERGY_FLOOR`]. A bald patch is a large local loss of this and nothing
    /// else.
    pub hair_energy_ratio: f32,
    /// How much further outside the locus the plan pushed the teeth, in `u'v'`.
    ///
    /// The **increase**, `max(0, after - before)`, not the absolute distance. The operator is
    /// bounded to removing a share of the excess, so teeth that started well outside the locus
    /// end nearer to it and still outside it - that is the design. What must never happen is that
    /// a correction takes a tooth *further* from natural, or overshoots past the locus and out the
    /// other side, and both of those are increases.
    ///
    /// Zero for a plan that only reduced the excess, which is every plan the solver intends to
    /// produce. Held below [`TEETH_EXCURSION_CEILING`].
    pub teeth_excursion: f32,
    /// How many pixels the three measurements were taken over, summed.
    ///
    /// A ratio over eleven samples is arithmetic rather than evidence, and the panel says so
    /// rather than printing three decimals. Phase 20's `measured_on`, for the same reason.
    pub measured_on: u32,
    /// How many times a family gave up strength to reach its floor, summed over the three.
    pub resolves: u8,
    /// Which families were withdrawn entirely because their floor could not be met.
    ///
    /// **Per family rather than whole-plan**, which is where this departs from phase 20's
    /// texture report: the three measurements are over three disjoint regions, so a frame whose
    /// teeth could not be evened safely still gets its lint removed. ADR-0043 section 5.
    pub withdrawn: [bool; OpFamily::COUNT],
}

impl NaturalnessReport {
    /// A report for a plan that changed nothing.
    pub const UNTOUCHED: Self = Self {
        catchlight_ratio: 1.0,
        hair_energy_ratio: 1.0,
        teeth_excursion: 0.0,
        measured_on: 0,
        resolves: 0,
        withdrawn: [false; OpFamily::COUNT],
    };

    /// The fewest samples a ratio needs before it is evidence rather than arithmetic.
    pub const WELL_MEASURED: u32 = 256;

    /// True when the measurements are over enough pixels to mean something.
    #[must_use]
    pub const fn is_well_measured(&self) -> bool {
        self.measured_on >= Self::WELL_MEASURED
    }

    /// True when this family was withdrawn.
    #[must_use]
    pub fn is_withdrawn(&self, family: OpFamily) -> bool {
        OpFamily::ALL
            .iter()
            .position(|candidate| *candidate == family)
            .and_then(|index| self.withdrawn.get(index).copied())
            .unwrap_or(false)
    }

    /// True when at least one family was withdrawn.
    #[must_use]
    pub fn any_withdrawn(&self) -> bool {
        self.withdrawn.iter().any(|flag| *flag)
    }

    /// True when every floor this report measured was met.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.catchlight_ratio >= CATCHLIGHT_FLOOR - 1e-4
            && self.hair_energy_ratio >= HAIR_ENERGY_FLOOR - 1e-4
            && self.teeth_excursion <= TEETH_EXCURSION_CEILING + 1e-6
    }

    /// What is wrong with this report, if anything.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if !(0.0..=4.0).contains(&self.catchlight_ratio) {
            return Some(format!(
                "a catchlight ratio of {:.3} is outside 0..4",
                self.catchlight_ratio
            ));
        }
        if !(0.0..=4.0).contains(&self.hair_energy_ratio) {
            return Some(format!(
                "a hair energy ratio of {:.3} is outside 0..4",
                self.hair_energy_ratio
            ));
        }
        if self.teeth_excursion < 0.0 || !self.teeth_excursion.is_finite() {
            return Some("a negative teeth excursion".into());
        }
        if self.resolves > NATURALNESS_MAX_RESOLVES_TOTAL {
            return Some(format!(
                "{} re-solves is above the {NATURALNESS_MAX_RESOLVES_TOTAL} three families may                  take",
                self.resolves,
            ));
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why a micro-retouch plan came out the way it did.
///
/// A closed vocabulary, phase 09's rule for the eighth time: reasons store their **code** rather
/// than their sentence, because a stored sentence is copy a release can change and a catalog full
/// of English cannot be translated.
///
/// Thirty-three codes, and two thirds of them are withdrawals - the highest proportion of any
/// phase so far, which is the shape of a phase whose whole job is to refuse to do too much.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicroCode {
    // --- the placeholder, first on every plan --------------------------------------------
    /// The three detectors are untrained placeholders and none is consulted.
    HeadUntrained,

    // --- refusals before anything is looked at -------------------------------------------
    /// Micro-retouch is switched off for this project.
    Disabled,
    /// This kind of photograph gets no micro-retouch.
    SceneLimited,
    /// No region arrived from phase 18, so nothing could be located.
    RegionUnavailable,
    /// The region arrived but is too doubtful to act through.
    RegionDoubtful,
    /// The face is too small in frame for any of this to be visible.
    FaceTooSmall,
    /// Phase 06 found nobody in this photograph.
    NoFaces,
    /// The studio has this operation switched off.
    OptedOut,

    // --- hair -----------------------------------------------------------------------------
    /// Stray hair was found and calmed.
    FlyawayCalmed,
    /// The background behind the hair is too detailed to work against.
    BackgroundBusy,
    /// The flyaway area cap was reached and the rest were left alone.
    FlyawayAreaCapped,
    /// The hairline lost too much of its own energy, so the reduction was withdrawn.
    HairEnergyLost,
    /// Nothing stray was found around the hair.
    NoFlyawayFound,

    // --- teeth ----------------------------------------------------------------------------
    /// Teeth were evened and de-yellowed.
    TeethCorrected,
    /// The correction hit its ceiling and stopped there.
    TeethCapped,
    /// The teeth were already inside the locus, so nothing was done.
    TeethAlreadyNatural,
    /// The mouth is too small in frame to correct.
    MouthTooSmall,
    /// There is no illuminant estimate, so the locus has no origin and no colour move was made.
    NoIlluminant,
    /// The lift would have taken the teeth above the face's own brightest skin.
    TeethWouldOutshineSkin,

    // --- eyes -----------------------------------------------------------------------------
    /// Sclera redness was reduced.
    ScleraCleared,
    /// Iris local contrast was raised a little.
    IrisClarified,
    /// A catchlight would have been dulled, so the eye work was withdrawn.
    CatchlightAtRisk,
    /// Phase 06 recorded no eye landmarks for this face.
    NoEyeLandmarks,
    /// The eyes needed nothing.
    EyesAlreadyClear,

    // --- clothing --------------------------------------------------------------------------
    /// A distraction was cleaned off a garment.
    ClothingCleaned,
    /// Something was found on the clothing and left alone because it is too large.
    ClothingTooLarge,
    /// The fabric is too textured to inpaint into safely.
    FabricTooTextured,

    // --- glare -----------------------------------------------------------------------------
    /// A specular sheet was reduced using this frame only.
    GlareReduced,
    /// A specular sheet was reconstructed from a sibling frame. **Always disclosed.**
    BorrowedFromSibling,
    /// A borrow was refused because the region still carries information.
    BorrowRefusedInformative,
    /// A borrow was refused because no sibling aligned well enough.
    BorrowNoAlignedSibling,
    /// A borrow was refused because the destroyed region is too large to rebuild.
    ///
    /// Distinct from [`MicroCode::BorrowRefusedInformative`] on purpose: that one says the
    /// photograph still holds the eye, and this one says the photograph does not but the repair
    /// would be a composite rather than a patch. A photographer reads them differently and so
    /// does phase 27.
    BorrowRefusedTooLarge,

    // --- the shared allowance ---------------------------------------------------------------
    /// The per-image perceptual allowance ran out and the lowest-priority operations were
    /// dropped.
    BudgetExhausted,
}

impl MicroCode {
    /// Every code, in the order `docs/reason-codes.md` lists them.
    pub const ALL: [Self; 33] = [
        Self::HeadUntrained,
        Self::Disabled,
        Self::SceneLimited,
        Self::RegionUnavailable,
        Self::RegionDoubtful,
        Self::FaceTooSmall,
        Self::NoFaces,
        Self::OptedOut,
        Self::FlyawayCalmed,
        Self::BackgroundBusy,
        Self::FlyawayAreaCapped,
        Self::HairEnergyLost,
        Self::NoFlyawayFound,
        Self::TeethCorrected,
        Self::TeethCapped,
        Self::TeethAlreadyNatural,
        Self::MouthTooSmall,
        Self::NoIlluminant,
        Self::TeethWouldOutshineSkin,
        Self::ScleraCleared,
        Self::IrisClarified,
        Self::CatchlightAtRisk,
        Self::NoEyeLandmarks,
        Self::EyesAlreadyClear,
        Self::ClothingCleaned,
        Self::ClothingTooLarge,
        Self::FabricTooTextured,
        Self::GlareReduced,
        Self::BorrowedFromSibling,
        Self::BorrowRefusedInformative,
        Self::BorrowNoAlignedSibling,
        Self::BorrowRefusedTooLarge,
        Self::BudgetExhausted,
    ];

    /// How many codes there are.
    pub const COUNT: usize = 33;

    /// Stable slug for the catalog, the wire and the reason registry.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HeadUntrained => "micro_head_untrained",
            Self::Disabled => "micro_disabled",
            Self::SceneLimited => "micro_scene_limited",
            Self::RegionUnavailable => "micro_region_unavailable",
            Self::RegionDoubtful => "micro_region_doubtful",
            Self::FaceTooSmall => "micro_face_too_small",
            Self::NoFaces => "micro_no_faces",
            Self::OptedOut => "micro_opted_out",
            Self::FlyawayCalmed => "micro_flyaway_calmed",
            Self::BackgroundBusy => "micro_background_busy",
            Self::FlyawayAreaCapped => "micro_flyaway_area_capped",
            Self::HairEnergyLost => "micro_hair_energy_lost",
            Self::NoFlyawayFound => "micro_no_flyaway_found",
            Self::TeethCorrected => "micro_teeth_corrected",
            Self::TeethCapped => "micro_teeth_capped",
            Self::TeethAlreadyNatural => "micro_teeth_already_natural",
            Self::MouthTooSmall => "micro_mouth_too_small",
            Self::NoIlluminant => "micro_no_illuminant",
            Self::TeethWouldOutshineSkin => "micro_teeth_would_outshine_skin",
            Self::ScleraCleared => "micro_sclera_cleared",
            Self::IrisClarified => "micro_iris_clarified",
            Self::CatchlightAtRisk => "micro_catchlight_at_risk",
            Self::NoEyeLandmarks => "micro_no_eye_landmarks",
            Self::EyesAlreadyClear => "micro_eyes_already_clear",
            Self::ClothingCleaned => "micro_clothing_cleaned",
            Self::ClothingTooLarge => "micro_clothing_too_large",
            Self::FabricTooTextured => "micro_fabric_too_textured",
            Self::GlareReduced => "micro_glare_reduced",
            Self::BorrowedFromSibling => "micro_borrowed_from_sibling",
            Self::BorrowRefusedInformative => "micro_borrow_refused_informative",
            Self::BorrowNoAlignedSibling => "micro_borrow_no_aligned_sibling",
            Self::BorrowRefusedTooLarge => "micro_borrow_refused_too_large",
            Self::BudgetExhausted => "micro_budget_exhausted",
        }
    }

    /// The sentence a photographer reads.
    ///
    /// English lives here rather than in the catalog, which is what makes translation a change to
    /// one file. Phase 09's rule, eighth phase running.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::HeadUntrained => {
                "AURA's small-fix detectors are not trained in this build, so it measured rather \
                 than predicted"
            }
            Self::Disabled => "The small fixes are switched off for this wedding",
            Self::SceneLimited => "This kind of photograph does not get the small fixes",
            Self::RegionUnavailable => {
                "AURA could not tell where the hair, teeth, eyes or clothing are in this \
                 photograph, so it left them alone"
            }
            Self::RegionDoubtful => {
                "AURA is not sure enough where one of these regions ends, so it left it alone"
            }
            Self::FaceTooSmall => "The face is too small here for any of this to show",
            Self::NoFaces => "There is nobody in this photograph to work on",
            Self::OptedOut => "Your studio has this fix switched off",
            Self::FlyawayCalmed => "Stray hairs were calmed against the background",
            Self::BackgroundBusy => {
                "The background behind the hair is too busy to work against safely, so the stray \
                 hairs were left"
            }
            Self::FlyawayAreaCapped => {
                "There were more stray hairs than AURA will touch on one photograph, so it \
                 calmed the most distracting ones"
            }
            Self::HairEnergyLost => {
                "Calming the stray hairs would have softened the hairline, so AURA undid it"
            }
            Self::NoFlyawayFound => "No stray hairs to calm",
            Self::TeethCorrected => "Teeth were evened out and a little of the yellow taken off",
            Self::TeethCapped => "The teeth correction reached AURA's limit and stopped there",
            Self::TeethAlreadyNatural => "The teeth already look natural in this light",
            Self::MouthTooSmall => "The mouth is too small in this frame to correct",
            Self::NoIlluminant => {
                "AURA has not worked out what colour the light was here, so it did not change any \
                 colours"
            }
            Self::TeethWouldOutshineSkin => {
                "Brightening the teeth any further would have made them brighter than the skin, \
                 so AURA stopped"
            }
            Self::ScleraCleared => "A little redness was taken out of the eyes",
            Self::IrisClarified => "The irises were given a little more definition",
            Self::CatchlightAtRisk => {
                "The eye work would have dulled a catchlight, so AURA undid it"
            }
            Self::NoEyeLandmarks => "AURA could not find the eyes on this face",
            Self::EyesAlreadyClear => "The eyes needed nothing",
            Self::ClothingCleaned => "Lint or a small mark was cleaned off the clothing",
            Self::ClothingTooLarge => {
                "There is something on the clothing that is too big for AURA to remove safely"
            }
            Self::FabricTooTextured => {
                "The fabric is too textured to clean into without damaging it"
            }
            Self::GlareReduced => "Reflection on the glasses was reduced",
            Self::BorrowedFromSibling => {
                "Part of the glasses was rebuilt from another photograph of the same moment"
            }
            Self::BorrowRefusedInformative => {
                "The reflection still shows the eye underneath, so AURA reduced it rather than \
                 rebuilding it"
            }
            Self::BorrowNoAlignedSibling => {
                "No other photograph of this moment lined up well enough to rebuild from"
            }
            Self::BorrowRefusedTooLarge => {
                "Too much of the glasses was lost to rebuild it from another photograph, so AURA                  reduced the reflection instead"
            }
            Self::BudgetExhausted => {
                "This photograph had already been adjusted as much as AURA allows, so the \
                 smallest fixes were left out"
            }
        }
    }

    /// True when this code withdraws a claim rather than making one.
    ///
    /// What the panel greys out and what [`MicroReason::is_doubt`] reads. Twenty-two of the
    /// thirty-three.
    #[must_use]
    pub const fn is_doubt(self) -> bool {
        matches!(
            self,
            Self::HeadUntrained
                | Self::Disabled
                | Self::SceneLimited
                | Self::RegionUnavailable
                | Self::RegionDoubtful
                | Self::FaceTooSmall
                | Self::NoFaces
                | Self::OptedOut
                | Self::BackgroundBusy
                | Self::FlyawayAreaCapped
                | Self::HairEnergyLost
                | Self::MouthTooSmall
                | Self::NoIlluminant
                | Self::TeethWouldOutshineSkin
                | Self::CatchlightAtRisk
                | Self::NoEyeLandmarks
                | Self::ClothingTooLarge
                | Self::FabricTooTextured
                | Self::BorrowRefusedInformative
                | Self::BorrowNoAlignedSibling
                | Self::BorrowRefusedTooLarge
                | Self::BudgetExhausted
        )
    }

    /// Parse the stored slug.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|code| code.as_str() == text)
    }
}

impl fmt::Display for MicroCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One reason, with the evidence behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct MicroReason {
    /// The code.
    pub code: MicroCode,
    /// The sentence a photographer reads.
    pub text: String,
    /// How much this moved the confidence. Negative for a doubt.
    pub weight: f32,
    /// Where on the frame, when the reason is about a place.
    pub evidence: Option<Box2>,
}

impl MicroReason {
    /// A reason about the whole frame.
    #[must_use]
    pub fn frame(code: MicroCode, text: impl Into<String>, weight: f32) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: None,
        }
    }

    /// A reason about one place on the frame.
    #[must_use]
    pub fn at(code: MicroCode, text: impl Into<String>, weight: f32, evidence: Box2) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: Some(evidence),
        }
    }

    /// A reason carrying the code's own sentence.
    #[must_use]
    pub fn plain(code: MicroCode, weight: f32) -> Self {
        Self::frame(code, code.user_text(), weight)
    }

    /// A reason carrying the code's own sentence and a rectangle.
    #[must_use]
    pub fn plain_at(code: MicroCode, weight: f32, evidence: Box2) -> Self {
        Self::at(code, code.user_text(), weight, evidence)
    }

    /// True when this reason withdraws a claim.
    #[must_use]
    pub fn is_doubt(&self) -> bool {
        self.code.is_doubt()
    }
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// One photograph's micro-retouch decision.
///
/// **It is not an edit.** The operations here reach the pixels only through
/// `aura_recipe::schema::merge` writing `recipe.micro[]`, which is phase 14's rule for the fifth
/// phase running. A plan with `user_edited = true` still carries AURA's own numbers, so phase
/// 30's learning loop can read the disagreement.
#[derive(Debug, Clone, PartialEq)]
pub struct MicroPlan {
    /// The photograph.
    pub image_id: ImageId,
    /// What was done, in a deterministic order.
    pub ops: Vec<MicroOp>,
    /// What the guard measured on the rendered result.
    pub naturalness: NaturalnessReport,
    /// Which operations were permitted on this frame, in [`MicroOp::NAMES`] order.
    ///
    /// Stored on the plan rather than looked up, because the matrix is a project setting that
    /// can change and a plan has to remain explicable after it does.
    pub allowed: [bool; 5],
    /// Why. Never empty; invariant 2.
    pub reasons: Vec<MicroReason>,
    /// How much the plan trusts itself, `0..1`.
    pub confidence: f32,
    /// The scene this was decided under. Invariant 7.
    pub scene: SceneId,
    /// How much of the shared per-image perceptual allowance this plan spent, `0..1`.
    ///
    /// **Phase 19's allowance, shared for the third time.** Twelve operations across three
    /// phases that each stay inside their own budget still add up to a photograph that looks
    /// worked on. ADR-0043 section 8.
    pub budget_used: f32,
    /// True when a photographer changed what may run on this frame.
    pub user_edited: bool,
    /// True when a photographer has looked at this plan and agreed.
    pub reviewed: bool,
    /// Which learned heads produced the detections.
    pub model_ver: u16,
    /// Which build's arithmetic produced the decisions.
    pub analysis_ver: u16,
    /// Which matrix file the ceilings and switches came from.
    pub matrix_ver: u16,
}

impl MicroPlan {
    /// A plan that does nothing, for a frame that needed nothing.
    ///
    /// Still carries a reason, because a plan with no reason is a bug rather than an empty plan.
    #[must_use]
    pub fn nothing(image_id: ImageId, scene: SceneId, reason: MicroReason) -> Self {
        Self {
            image_id,
            ops: Vec::new(),
            naturalness: NaturalnessReport::UNTOUCHED,
            allowed: [false; 5],
            reasons: vec![reason],
            confidence: 1.0,
            scene,
            budget_used: 0.0,
            user_edited: false,
            reviewed: false,
            model_ver: 0,
            analysis_ver: 0,
            matrix_ver: 0,
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

    /// Every photograph this plan borrowed pixels from.
    ///
    /// **The disclosure query.** Section 6.3 requires that a borrow is never hidden, and this is
    /// the one call the Explain panel, the delivery report and the QC agent all make.
    #[must_use]
    pub fn borrowed_from(&self) -> Vec<ImageId> {
        let mut out: Vec<ImageId> = self.ops.iter().filter_map(MicroOp::borrowed_from).collect();
        out.sort();
        out.dedup();
        out
    }

    /// True when this plan composites pixels from another photograph.
    #[must_use]
    pub fn is_composite(&self) -> bool {
        self.ops.iter().any(|op| op.borrowed_from().is_some())
    }

    /// True when the frame is worth a photographer's attention.
    #[must_use]
    pub fn needs_review(&self) -> bool {
        !self.reviewed && !self.user_edited && self.confidence < REVIEW_BELOW
    }

    /// What guarantee this plan breaks, if any.
    ///
    /// Eight checks, and every one is an acceptance criterion rather than a type error: there is
    /// at least one reason, the plan is inside the operation cap and the shared allowance, every
    /// operation is inside its own bounds, no operation runs that the matrix forbade, a withdrawn
    /// family really is empty, the naturalness report is coherent, and a composite frame really
    /// does carry a disclosed source. They live here so the solver, the store, the IPC layer and
    /// the eval harness all refuse the same frames. `aura_retouch::micro::guard` turns a `Some`
    /// here into `AURA-ML-5097`.
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
            let index = MicroOp::NAMES
                .iter()
                .position(|name| *name == op.as_str())
                .unwrap_or(usize::MAX);
            if !self.allowed.get(index).copied().unwrap_or(false) {
                return Some(format!(
                    "a `{}` operation ran while the matrix forbade it",
                    op.as_str()
                ));
            }
            if let Some(family) = op.family() {
                if self.naturalness.is_withdrawn(family) {
                    return Some(format!(
                        "the {family} family was withdrawn and still carries operations"
                    ));
                }
            }
        }
        if let Some(problem) = self.naturalness.problem() {
            return Some(problem);
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

/// What a project's micro pass covered and what it found.
///
/// Phase 05's rule, inherited for the fifteenth time: report coverage when you report a result,
/// and say what the denominator is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MicroOutline {
    /// Photographs with a plan.
    pub planned: u32,
    /// Photographs in the project.
    pub photos: u32,
    /// Fraction of the project with a plan, `0..1`.
    ///
    /// **The denominator is every photograph**, as phases 09 to 15 and 20 all are.
    pub coverage: f32,
    /// Photographs where at least one operation ran.
    pub acted_on: u32,
    /// Photographs where the regions this phase needs arrived from phase 18.
    ///
    /// The number that separates "there was nothing to fix" from "AURA could not see where the
    /// teeth were". Zero on a build with no mask generator wired in.
    pub region_covered: u32,
    /// How many operations of each kind ran, in [`MicroOp::NAMES`] order.
    pub op_histogram: [u32; 5],
    /// How many frames borrowed pixels from a sibling.
    ///
    /// **The disclosure number.** A gallery's composite count belongs on the project header
    /// rather than buried per frame, because the question a photographer asks is "did any of
    /// this get composited" and not "did this one".
    pub borrows: u32,
    /// How many families were withdrawn across the project, in [`OpFamily::ALL`] order.
    pub withdrawn_histogram: [u32; OpFamily::COUNT],
    /// Frames where a family gave up strength to reach its floor.
    pub resolved: u32,
    /// Mean catchlight ratio over frames that had eye work.
    pub mean_catchlight_ratio: f32,
    /// Mean hair energy ratio over frames that had hair work.
    pub mean_hair_energy_ratio: f32,
    /// Frames below the review threshold.
    pub needs_review: u32,
    /// Frames a photographer has changed by hand.
    pub user_edited: u32,
    /// Scenes with no row in the matrix file, which were planned against the neutral row.
    pub unlisted_scenes: Vec<String>,
    /// The versions the stored plans were made under.
    pub model_ver: u16,
    /// The build arithmetic they were made under.
    pub analysis_ver: u16,
    /// The matrix file they were made under.
    pub matrix_ver: u16,
}

impl MicroOutline {
    /// True when every stored plan agrees with the running build about all three versions.
    #[must_use]
    pub const fn versions_agree(&self, current: (u16, u16, u16)) -> bool {
        self.model_ver == current.0
            && self.analysis_ver == current.1
            && self.matrix_ver == current.2
    }
}

// ---------------------------------------------------------------------------
// Overrides
// ---------------------------------------------------------------------------

/// What the photographer allowed instead.
///
/// Every field optional and independent, the shape phases 15, 19 and 20 all use: somebody who
/// switched glare off has not made a claim about teeth, and an override carrying both would
/// silently freeze the one they did not touch.
///
/// **There is no strength field and no ceiling field.** A photographer chooses *which* operations
/// run; how far each one may go is a product decision bounded by the contract, and a surface that
/// could raise a ceiling would make `docs/retouch-ethics.md` a description of the defaults rather
/// than a promise.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroOverride {
    /// Which operations may run, in [`MicroOp::NAMES`] order. `None` leaves the matrix alone.
    #[serde(default)]
    pub allowed: Option<[bool; 5]>,
    /// Which clothing issues may be cleaned. `None` leaves the matrix alone.
    #[serde(default)]
    pub clothing: Option<[bool; ClothingIssue::COUNT]>,
    /// Whether cross-frame borrowing is permitted at all. `None` leaves the matrix alone.
    ///
    /// Separate from `allowed` because a studio can want glare reduced and want no composites in
    /// the delivery, and collapsing the two would force them to choose.
    #[serde(default)]
    pub borrowing: Option<bool>,
}

impl MicroOverride {
    /// True when this override sets nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.allowed.is_none() && self.clothing.is_none() && self.borrowing.is_none()
    }

    /// What is wrong with this override, if anything.
    ///
    /// `aura_retouch::micro::guard` turns a `Some` here into `AURA-ML-5098`.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if self.is_empty() {
            return Some("the override sets nothing".into());
        }
        None
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask what small fixes were made to a photograph.
///
/// Seventeenth service of its kind, and it carries the rule for the seventeenth time: **no phase
/// may keep its own flyaway detector, its own teeth locus or its own idea of what a borrow is.**
/// Phase 22 restores and sharpens and must not sharpen an iris this phase has already clarified;
/// phase 25 normalises a gallery of these decisions; phase 27 has to be able to say why a face
/// looks worked on; phase 28 runs unattended and must know what ran. Two answers to "what did we
/// do to her eyes" is a delivery in which the album and the gallery disagree about somebody's
/// face.
pub trait MicroService: Send + Sync + fmt::Debug {
    /// What a project's pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<MicroOutline>;

    /// One photograph's plan, or `None` when it has not been planned.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plan cannot be read.
    fn of_image(&self, image: ImageId) -> AuraResult<Option<MicroPlan>>;

    /// Every frame in the project that borrowed pixels from another, with its sources.
    ///
    /// **The disclosure call, and it is on the frozen surface deliberately.** A composite that
    /// can only be found by opening four hundred plans one at a time is a composite nobody finds.
    /// The delivery report, the Explain panel and the QC agent all read this.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn composites(&self, project: ProjectId) -> AuraResult<BTreeMap<ImageId, Vec<ImageId>>>;

    /// The frames whose micro-retouch is worth a photographer's attention, weakest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>>;

    /// Which operations this project currently permits.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the matrix cannot be read.
    fn matrix(&self, project: ProjectId) -> AuraResult<MicroOverride>;

    /// Record that the photographer has looked at this plan and agrees.
    ///
    /// Sets [`MicroPlan::reviewed`] and does not set [`MicroPlan::user_edited`]: accepting a
    /// suggestion is not authoring one, and phase 30's learning loop needs to tell them apart.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5098` when the photograph has no plan.
    fn accept(&self, image: ImageId) -> Result<(), AuraError>;

    /// Record which operations the photographer allows on this project.
    ///
    /// Sets `user_edited` on the matrix row and is not undone by a re-analysis: the check is
    /// inside the statement that would overwrite the row, exactly as `identities.user_locked`,
    /// `segments.user_locked`, `moments.user_locked`, `masks.user_edited`,
    /// `local_light_plan.user_edited` and `retouch_plan.user_edited` are.
    ///
    /// **This records the decision; it does not move a pixel.** The pixels move when the caller
    /// writes the plans through `aura_recipe::schema::merge`.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5098` when the override sets nothing.
    fn set_matrix(&self, project: ProjectId, values: MicroOverride) -> Result<(), AuraError>;
}
