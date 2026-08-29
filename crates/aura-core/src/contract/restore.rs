//! FROZEN CONTRACT. Repairing a photograph the wedding actually produced: an ISO 12800 dance
//! floor made deliverable, an edge recovered where recovering it helps, and a slightly soft face
//! left recognisably the same person.
//!
//! PHASE-22 section 5 freezes [`RestorePlan`] before any solver exists. The file is in
//! `aura-core` for the reason [`crate::contract::micro`], [`crate::contract::retouch`],
//! [`crate::contract::local`] and [`crate::contract::colour`] are: the phases that consume a
//! restoration decision are 25 (gallery consistency, which has to notice a gallery denoised at
//! four different tiers), 27 (QC, which has to be able to say why an edge looks crunchy and
//! whether a face still looks like the person) and 28 (autopilot, which must know what ran
//! unattended), and none of them needs the noise models, the kernel estimator or the band
//! arithmetic.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **This is the first phase in the product that repairs rather than decides.** Every phase from
//! 09 to 21 either measured a photograph or chose how it should look; a wrong answer from any of
//! them is a photograph graded differently from the one a photographer wanted. A wrong answer
//! here is a photograph with information *removed* - smeared lace, a ringed edge, a face that is
//! very slightly not the same face - and none of the three is recoverable by editing afterwards.
//!
//! So every operation in this contract is bounded twice: by a ceiling that the code owns and a
//! config file may only lower, and by a **post-condition measured on the rendered pixels**.
//! [`ArtefactReport`] is that post-condition, and it carries three numbers rather than one
//! because the three are fixed by three different parameters.
//! `docs/adr/ADR-0047-restoration-denoise-sharpen-and-identity.md` section 2.1 has the argument.
//!
//! ## The second thing: the identity constraint can only ever refuse
//!
//! Phase 16's skin guard re-solves a grade until the skin has not moved. Phase 20's texture guard
//! re-solves and then withdraws the whole retouch. This one re-solves and then **skips the
//! face** - and there is deliberately no outcome in which a face whose embedding has drifted is
//! delivered at a lower strength, because a face that has drifted a little is a face that has
//! drifted. [`MAX_IDENTITY_DRIFT`] is the threshold, [`RecoveredFace::skipped`] is the outcome,
//! and the measured distance is stored on the row whether it passed or not, so section 10.1's
//! "below threshold on 100 % of fixtures" is a query rather than a sentence.
//!
//! ## The third thing: there is no scale factor anywhere in here
//!
//! Section 2.2 puts upscaling beyond native resolution out of scope for V1, and section 2.2 puts
//! generative reconstruction in phase 24. There is no field in this file that could carry an
//! output scale, a synthesised region or a source image, which makes both exclusions properties
//! of the shape rather than of anybody's memory.
//! `crates/aura-core/tests/restore_contract.rs` asserts it.
//!
//! ## What this contract cannot express
//!
//! There is no field here for a face landmark, a displacement, a skin-tone target or a second
//! photograph. Section 11 of `docs/plan/CLAUDE.md` forbids identity-changing operations
//! permanently; this phase is the one where a model could most plausibly commit one by accident,
//! which is why the constraint is measured through the renderer rather than asserted.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::composition::Box2;
use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{IdentityId, ProjectId};
use crate::contract::local::{FULL_MASK_CONFIDENCE, MIN_MASK_CONFIDENCE};
use crate::contract::scene::SceneId;

pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// Ceilings, floors and bands
// ---------------------------------------------------------------------------

/// The largest face-recovery strength, `0..1`.
///
/// Section 5 writes it into the frozen struct's comment - "strength 0..0.4, capped" - and
/// section 6.3 writes it again: "Cap strength at 0.4 and blend with the original at high
/// frequencies to keep skin realistic." Two statements of the same number in one document is a
/// number somebody argued about, and it is the only ceiling in this phase the phase document
/// fixes itself.
pub const MAX_FACE_RECOVERY: f32 = 0.40;

/// The largest cosine distance a face embedding may move, `0..1`.
///
/// **The guarantee of this phase.** Eight hundredths. Phase 06's recogniser separates two
/// different people at a distance far above this and separates two photographs of the same
/// person at a distance far below it, so a face that has moved this far under a restoration is a
/// face that has moved measurably toward being somebody else - which is the thing section 6.3
/// says the product never does.
///
/// It is deliberately not derived from phase 06's clustering threshold. That threshold answers
/// "are these two people the same", over two *different* photographs of a moving face under
/// changing light. This answers "did this operation move one crop of one face", where every
/// other variable is held fixed, so the same number would be far too permissive.
pub const MAX_IDENTITY_DRIFT: f32 = 0.08;

/// The lowest measured face sharpness face recovery will act on, `0..1`.
///
/// Section 6.3: "never on heavily blurred faces where the model would hallucinate". Below this a
/// face carries too little information to constrain a prior, so what a prior returns is the
/// prior - a plausible face, which is to say somebody else's. Checked **before** any model is
/// consulted rather than after, so an untrained head cannot be the thing that saves a frame.
pub const SOFT_FACE_LO: f32 = 0.42;

/// The highest measured face sharpness face recovery will act on, `0..1`.
///
/// Above this the face is sharp and there is nothing to recover. The band between the two is
/// deliberately narrow - section 6.3's own words are "a narrow band of measured sharpness" - and
/// a wide band is how a face-prior model ends up running on faces that did not need it.
pub const SOFT_FACE_HI: f32 = 0.68;

/// The smallest estimated blur kernel worth deconvolving, in pixels of Gaussian sigma.
///
/// Below this the frame is as sharp as the lens and the sensor made it, and deconvolution has
/// nothing to invert - so what it amplifies is the residual noise.
///
/// **One pixel, and the number is set by the measuring instrument rather than by optics.**
/// `aura_restore::kernel` estimates the kernel from the width of a Sobel gradient ridge, and a
/// Sobel operator has a width of its own. A mathematically perfect step edge - the sharpest thing
/// that can exist in a sampled image - measures a full width at half maximum of exactly two
/// samples through one, which is a sigma of `2 / 2.35482 = 0.849`. **No photograph can measure
/// below that**, so a floor under it is a floor nothing is ever under, and every frame in every
/// wedding would be reported as recoverably soft.
///
/// This is set above the instrument floor with a sample of margin: a sigma of 1.0 is a full width
/// at half maximum of 2.35 samples, which is a frame that really is slightly soft rather than one
/// the estimator cannot resolve. Phase 22 shipped 0.55 first, and a synthetic chequerboard came
/// back needing sharpening. `aura_restore::kernel`'s own test holds the two numbers apart, so a
/// change to the estimator that moved its floor fails the build rather than quietly re-opening
/// this. ADR-0047 section 11.1 records it.
pub const SHARPEN_KERNEL_LO: f32 = 1.00;

/// The largest estimated blur kernel this phase will deconvolve, in pixels of Gaussian sigma.
///
/// Section 6.2: "if blur is dominated by motion or gross defocus, do not sharpen". Above this
/// the blur is gross, a small-iteration Richardson-Lucy cannot invert it, and what it produces
/// instead is the ringing this constant exists to prevent.
pub const SHARPEN_KERNEL_HI: f32 = 2.20;

/// The largest deconvolution amount, `0..1`.
///
/// Half. Deconvolution differs from unsharp masking in that its artefact is *structured* -
/// ringing follows the edge it came from and reads as a drawn outline rather than as grain - so
/// the ceiling is lower than a capture-sharpening ceiling would be, and the amount is capped
/// again by the residual noise before it is used.
pub const MAX_SHARPEN_AMOUNT: f32 = 0.50;

/// The share of the sharpening amount withheld on skin, `0..1`.
///
/// Four fifths. Section 6.2 says "explicitly attenuated on skin"; this is what that is as a
/// number, and it is an attenuation rather than an exclusion deliberately. A face with literally
/// no sharpening inside a frame that was sharpened reads as soft rather than as protected, which
/// is the failure mode of every tool that masks skin out entirely. ADR-0047 section 4.
pub const SKIN_ATTENUATION: f32 = 0.80;

/// The most Richardson-Lucy iterations one frame gets.
///
/// Three. Section 6.2 asks for "a small iteration count"; the ringing amplitude of an RL
/// deconvolution grows with the iteration count and the recovered detail saturates well before
/// it does, so this is where the two curves cross rather than a compute budget.
pub const MAX_DECONV_ITERATIONS: u8 = 3;

/// The lowest band-energy ratio a denoised frame may keep outside the face, `0..1` and above.
///
/// Ninety per cent. **The smearing floor.** Section 6.4's first self-check: the high-band energy
/// of the non-face detail regions after the plan, divided by the same energy before it. A
/// denoiser that has removed a tenth of the fine structure in a bride's lace has removed the
/// lace. Measured through the real renderer, in [`ArtefactReport::texture_retention`].
pub const MIN_TEXTURE_RETENTION: f32 = 0.90;

/// The largest permitted edge overshoot, `0..1`.
///
/// **The ringing ceiling.** Two hundredths of the working space's diffuse white, measured as the
/// mean excursion beyond the local extremes on the strongest edges in the frame. Deconvolution
/// ringing that stays under this is invisible at 100 % zoom; above it, an edge acquires the pale
/// outline that makes a photograph read as processed.
pub const MAX_RINGING: f32 = 0.020;

/// The factor a strength is multiplied by when a self-check fails.
///
/// Three quarters, the same step phases 20 and 21 use, and for the same reason: a step small
/// enough that the second attempt is usually enough and large enough that three attempts span
/// most of the range.
pub const RESOLVE_STEP: f32 = 0.75;

/// The most times one operation may give up strength before it is abandoned.
pub const MAX_RESOLVES: u8 = 3;

/// The confidence below which a plan is worth a photographer's attention.
pub const REVIEW_BELOW: f32 = 0.55;

/// The most faces one plan records a recovery outcome for.
///
/// Sixteen. A frame with more faces than this is a group shot, where no individual face is large
/// enough to be inside [`SOFT_FACE_LO`]'s band anyway; the cap keeps a 60-face fixture from
/// writing 60 rows that all say the same thing.
pub const MAX_RECOVERED_FACES: usize = 16;

// ---------------------------------------------------------------------------
// The regions this phase reads
// ---------------------------------------------------------------------------

/// The regions this phase reads, as a projection of phase 18's twenty-class vocabulary.
///
/// **Not a second vocabulary.** [`RestoreRegion::as_mask_str`] is total onto phase 18's own
/// spellings, the shape [`crate::contract::micro::MicroRegion::as_mask_str`] uses, so this is a
/// view of one answer rather than a competing one.
///
/// Seven regions and every one of them is *read*. Nothing in this phase edits a region: denoise
/// acts on the whole frame conditioned by the sensor's noise model, sharpening acts everywhere
/// it is not excluded, and face recovery acts inside a face box phase 06 produced. The regions
/// decide **where an operation is withheld**, which is the opposite of how phases 19 to 21 use
/// them.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum RestoreRegion {
    /// Visible human skin. Sharpening is **attenuated** here, never excluded.
    #[default]
    Skin,
    /// The face proper. Bounds face recovery, and excluded from the smearing measurement.
    Face,
    /// Sky. Excluded from sharpening outright: there is no edge in a sky worth recovering, and
    /// a deconvolved sky is a sky with structured noise in it.
    Sky,
    /// Everything that is not the subject. The region the bokeh test is run over.
    Background,
    /// The salient person or people. Where sharpening acts.
    Subject,
    /// A bridal dress or a veil. The chroma-detail region denoising must not smear.
    Dress,
    /// Worn fabric that is not a bridal dress. The second chroma-detail region.
    Clothing,
}

impl RestoreRegion {
    /// Every region, in the order `docs/restoration.md` documents them.
    pub const ALL: [Self; 7] = [
        Self::Skin,
        Self::Face,
        Self::Sky,
        Self::Background,
        Self::Subject,
        Self::Dress,
        Self::Clothing,
    ];

    /// How many regions there are.
    pub const COUNT: usize = 7;

    /// Stable text for the catalog and the wire. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skin => "skin",
            Self::Face => "face",
            Self::Sky => "sky",
            Self::Background => "background",
            Self::Subject => "subject",
            Self::Dress => "dress",
            Self::Clothing => "clothing",
        }
    }

    /// The spelling `aura_vision::contract::mask::MaskKind::as_str` uses for the same region.
    ///
    /// Total by construction, which is what makes this a view rather than a second vocabulary.
    /// The reverse is deliberately not total: thirteen of phase 18's classes have nothing to do
    /// with this phase, and a `From` that invented a restore region for `teeth` would be a mask
    /// generator hiding in a conversion.
    #[must_use]
    pub const fn as_mask_str(self) -> &'static str {
        // Identical strings today, and written out rather than returned from `as_str` because
        // the two are different contracts: this one may not drift when phase 18 renames a class,
        // and a test in `aura-vision` is what would catch it.
        match self {
            Self::Skin => "skin",
            Self::Face => "face",
            Self::Sky => "sky",
            Self::Background => "background",
            Self::Subject => "subject",
            Self::Dress => "dress",
            Self::Clothing => "clothing",
        }
    }

    /// Parse the catalog spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|region| region.as_str() == text)
    }

    /// True when deconvolution sharpening is withheld from this region entirely.
    ///
    /// Sky and background. Skin is deliberately absent: it is attenuated by
    /// [`SKIN_ATTENUATION`] rather than excluded, for the reason that constant records.
    #[must_use]
    pub const fn excluded_from_sharpen(self) -> bool {
        matches!(self, Self::Sky | Self::Background)
    }

    /// True when this region's fine chroma detail is what denoising must not smear.
    ///
    /// Section 6.1: "Preserve chroma detail separately from luminance detail; wedding fabrics
    /// and skin suffer most from chroma smearing."
    #[must_use]
    pub const fn is_chroma_detail(self) -> bool {
        matches!(self, Self::Dress | Self::Clothing | Self::Skin)
    }
}

impl fmt::Display for RestoreRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// The mask port
// ---------------------------------------------------------------------------

/// One region, as phase 18 delivers it to this phase.
///
/// The port phase 19 established and phase 21 repeated, at phase 19's grid size and with phase
/// 19's two quality numbers. A phase that consumes another phase's output owns no fallback for
/// it: when no field arrives, sharpening is **refused** rather than run over the whole frame,
/// and `RestoreCode::SharpenNoRegions` says so.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RestoreField {
    /// What the region is of.
    pub region: RestoreRegion,
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

impl RestoreField {
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
    /// Phase 19's ramp, reading phase 19's constants, so the decision about how much a doubtful
    /// region may do is made once for the whole product.
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
    /// A sentence rather than an [`AuraError`]: `aura-core` owns the shape and `aura-restore`
    /// owns the error registry, the split every phase since 09 has kept.
    /// `aura_restore::decide` turns a `Some` here into `AURA-ML-5112`.
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
// The camera's noise model
// ---------------------------------------------------------------------------

/// What a camera body's sensor does to a photon count, as measured by COL.
///
/// The photon transfer curve, in the one form that matters here: the variance of a linear
/// sample at signal level `y` is `read^2 + shot * y`, where `read` is fixed per body per ISO and
/// `shot` is the reciprocal of the conversion gain. Section 6.1 asks the denoiser to be
/// conditioned on it "so it removes the right amount rather than a learned average".
///
/// **`measured` is the field that matters in this build.** There are no camera files in this
/// repository, so every model that ships is synthetic and every one of them is `false`. A
/// synthetic figure that is too *low* under-denoises, which a photographer can see and correct;
/// one that is too *high* over-denoises, which is the smeared lace this phase exists to avoid.
/// An unmeasured model therefore caps the tier at [`DenoiseTier::Standard`] - the asymmetry
/// written down, in [`NoiseModel::tier_ceiling`]. ADR-0047 section 3.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NoiseModel {
    /// The body this describes, as EXIF spells it.
    pub camera: String,
    /// The ISO the two coefficients were measured at.
    pub iso: u32,
    /// Read noise standard deviation, in linear working-space units at diffuse white.
    pub read: f32,
    /// Shot noise slope: the variance added per unit of signal.
    pub shot: f32,
    /// True when a photographed reference produced these numbers.
    pub measured: bool,
    /// Which table version they came from.
    pub table_ver: u16,
}

impl NoiseModel {
    /// The neutral reference model, for a body with no row of its own.
    ///
    /// A current-generation full-frame sensor at ISO 100: about 3 electrons of read noise into a
    /// 55,000-electron well, which normalises to `read = 3 / 55_000` and `shot = 1 / 55_000`. The
    /// well is deliberately at the *large* end of the twenty bodies in
    /// `crates/aura-restore/config/noise_models/`, which makes both terms small - and that is the
    /// conservative direction in the sense ADR-0047 section 3 argues for: a model that
    /// under-estimates the noise under-denoises, which a photographer can see and correct, while
    /// one that over-estimates it smears lace, which they cannot.
    #[must_use]
    pub fn reference() -> Self {
        Self {
            camera: "reference".to_string(),
            iso: 100,
            read: 3.0 / 55_000.0,
            shot: 1.0 / 55_000.0,
            measured: false,
            table_ver: 0,
        }
    }

    /// The predicted noise sigma at one signal level, in linear working-space units.
    ///
    /// `sqrt(read^2 + shot * y)`, scaled from the model's own ISO to the frame's. Shot noise
    /// scales with the gain and read noise scales with it too on every sensor this table
    /// describes, so one ratio serves both - which is a simplification and is documented as one
    /// in `docs/restoration.md` rather than hidden here.
    #[must_use]
    // ISO values are four or five significant figures, so the f32 mantissa is exact over the
    // whole range this table describes. The lint is about u32s near 2^24 and there are none.
    #[allow(clippy::cast_precision_loss)]
    pub fn sigma_at(&self, signal: f32, iso: u32) -> f32 {
        let gain = if self.iso == 0 {
            1.0
        } else {
            (iso.max(1) as f32) / (self.iso as f32)
        };
        let read = self.read * gain;
        let variance = read.mul_add(read, self.shot * gain * signal.max(0.0));
        variance.max(0.0).sqrt()
    }

    /// The strongest tier this model may be used at.
    ///
    /// [`DenoiseTier::Strong`] when the model was measured, [`DenoiseTier::Standard`] when it was
    /// not. **The whole of the unmeasured-camera policy is these three lines**, so there is one
    /// place to change it and one place for a test to read it.
    #[must_use]
    pub const fn tier_ceiling(&self) -> DenoiseTier {
        if self.measured {
            DenoiseTier::Strong
        } else {
            DenoiseTier::Standard
        }
    }

    /// What is wrong with this model, if anything.
    ///
    /// `aura_restore::noise_model` turns a `Some` here into `AURA-ML-5111`.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if self.camera.trim().is_empty() {
            return Some("the model names no camera".into());
        }
        if self.iso == 0 {
            return Some("the model names no ISO".into());
        }
        if !self.read.is_finite() || self.read < 0.0 || self.read > 1.0 {
            return Some(format!("read noise {} is outside 0..1", self.read));
        }
        if !self.shot.is_finite() || self.shot < 0.0 || self.shot > 1.0 {
            return Some(format!("shot slope {} is outside 0..1", self.shot));
        }
        None
    }
}

// ---------------------------------------------------------------------------
// The denoise decision
// ---------------------------------------------------------------------------

/// How hard this photograph is denoised. Section 5's frozen vocabulary, verbatim.
///
/// Four values and no number, because the *decision* is which of four, and how much luminance
/// and chroma reduction each of them becomes on this body at this ISO is a property of the
/// [`NoiseModel`] rather than of the tier. A tier alone is not reproducible; [`DenoiseSpec`] is
/// what makes a stored plan re-renderable.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum DenoiseTier {
    /// Nothing. The scene already tolerates what is in the frame.
    #[default]
    Off,
    /// A touch, on a frame just past its scene's tolerance.
    Light,
    /// The ordinary reception answer.
    Standard,
    /// A dance floor at ISO 12800 and above.
    Strong,
}

impl DenoiseTier {
    /// Every tier, weakest first.
    pub const ALL: [Self; 4] = [Self::Off, Self::Light, Self::Standard, Self::Strong];

    /// How many tiers there are.
    pub const COUNT: usize = 4;

    /// Stable text for the catalog, the recipe and the wire. Never localised.
    ///
    /// This is what `recipe.restoration.denoise` carries, which is why `Off` is spelled `off`:
    /// phase 14 documented that field as "`auto`, `off`, or a named model" and `off` is the one
    /// spelling it fixed.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Light => "light",
            Self::Standard => "standard",
            Self::Strong => "strong",
        }
    }

    /// Parse the catalog spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tier| tier.as_str() == text)
    }

    /// Position in the ladder, from zero.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Light => 1,
            Self::Standard => 2,
            Self::Strong => 3,
        }
    }

    /// The tier one step stronger, or this one when there is no stronger tier.
    #[must_use]
    pub const fn stronger(self) -> Self {
        match self {
            Self::Off => Self::Light,
            Self::Light => Self::Standard,
            Self::Standard | Self::Strong => Self::Strong,
        }
    }

    /// The tier one step weaker, or this one when there is no weaker tier.
    #[must_use]
    pub const fn weaker(self) -> Self {
        match self {
            Self::Off | Self::Light => Self::Off,
            Self::Standard => Self::Light,
            Self::Strong => Self::Standard,
        }
    }

    /// The weaker of two tiers.
    #[must_use]
    pub fn clamped_by(self, ceiling: Self) -> Self {
        if self.rank() <= ceiling.rank() {
            self
        } else {
            ceiling
        }
    }

    /// How many multiples of the predicted sigma this tier removes.
    ///
    /// The one number that turns a decision into an amount, and it is deliberately sub-linear
    /// in the rank: `Strong` is a little over twice `Light` rather than three times it, because
    /// the difference between a frame at 1.2 and a frame at 3.0 tolerances is mostly *how much
    /// noise there is* and not how aggressively it should be attacked.
    #[must_use]
    pub const fn sigma_multiple(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Light => 0.9,
            Self::Standard => 1.5,
            Self::Strong => 2.1,
        }
    }
}

impl fmt::Display for DenoiseTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The tier as the renderer receives it: three amounts, the sigma they came from, and the model.
///
/// See ADR-0047 section 2.1 for why the plan carries this beside [`RestorePlan::denoise`]. The
/// three amounts are the recipe's own `global.noise.{luminance,colour,detail}` scale, `0..1`,
/// which the store multiplies by 100 on the way out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DenoiseSpec {
    /// Luminance noise reduction, `0..1`.
    pub luminance: f32,
    /// Chroma noise reduction, `0..1`.
    ///
    /// Always at or above [`DenoiseSpec::luminance`]. Chroma noise is spatially low-frequency
    /// and carries no detail a photographer wants; luminance noise is half a stop away from
    /// being grain. Section 6.1's "preserve chroma detail separately from luminance detail" is
    /// about the *radius*, which lives in the renderer, and this is the amount.
    pub colour: f32,
    /// How much fine detail is protected against the luminance pass, `0..1`.
    pub detail: f32,
    /// The noise sigma the tier was chosen from, in linear working-space units.
    pub sigma: f32,
    /// The camera body whose model conditioned it.
    pub camera: String,
    /// True when that model was measured rather than synthesised.
    pub measured_model: bool,
}

impl DenoiseSpec {
    /// What is wrong with this spec, if anything.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        for (name, value) in [
            ("luminance", self.luminance),
            ("colour", self.colour),
            ("detail", self.detail),
        ] {
            if !(0.0..=1.0).contains(&value) {
                return Some(format!("denoise {name} {value:.3} is outside 0..1"));
            }
        }
        if self.colour < self.luminance - 1e-4 {
            return Some(format!(
                "chroma reduction {:.3} is below luminance reduction {:.3}",
                self.colour, self.luminance
            ));
        }
        if !self.sigma.is_finite() || self.sigma < 0.0 {
            return Some("the sigma is not a non-negative number".into());
        }
        if self.camera.trim().is_empty() {
            return Some("the spec names no camera".into());
        }
        None
    }
}

// ---------------------------------------------------------------------------
// The sharpen decision
// ---------------------------------------------------------------------------

/// Which regions the deconvolution was withheld from, and how much of the frame it reached.
///
/// Section 5 names [`SharpenSpec::mask`] and does not define it; ADR-0047 section 2.1 argues for
/// this shape. **It holds no pixels.** A plan is a decision, and `aura-core` has carried no image
/// data since phase 01.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SharpenMask {
    /// Which regions were excluded entirely, in [`RestoreRegion::ALL`] order.
    pub excluded: [bool; RestoreRegion::COUNT],
    /// Fraction of the frame the sharpening actually acts on, `0..1`.
    pub coverage: f32,
    /// True when phase 18 supplied the regions.
    ///
    /// **The difference between "there was nothing to exclude" and "AURA could not see where the
    /// sky was".** False means the sharpening was refused rather than applied blind - ADR-0047
    /// section 4, bullet 4 - so a `SharpenSpec` with this false is not a representable state on a
    /// sound plan, and [`RestorePlan::broken_guarantee`] says so.
    pub from_regions: bool,
}

impl SharpenMask {
    /// True when this region was excluded.
    #[must_use]
    pub fn excludes(&self, region: RestoreRegion) -> bool {
        RestoreRegion::ALL
            .iter()
            .position(|r| *r == region)
            .and_then(|index| self.excluded.get(index).copied())
            .unwrap_or(false)
    }

    /// What is wrong with this mask, if anything.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if !(0.0..=1.0).contains(&self.coverage) {
            return Some(format!(
                "sharpen coverage {:.3} is outside 0..1",
                self.coverage
            ));
        }
        if !self.from_regions {
            return Some("sharpening ran without regions from phase 18".into());
        }
        for region in RestoreRegion::ALL {
            if region.excluded_from_sharpen() && !self.excludes(region) {
                return Some(format!("{region} was not excluded from sharpening"));
            }
        }
        None
    }
}

/// One frame's deconvolution sharpening. Section 5's four fields, verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SharpenSpec {
    /// The estimated blur kernel width, in pixels of Gaussian sigma.
    ///
    /// Measured from edge profiles rather than assumed - section 6.2 - and inside
    /// [`SHARPEN_KERNEL_LO`] to [`SHARPEN_KERNEL_HI`] or the operation does not run at all.
    pub kernel_sigma: f32,
    /// How much of the deconvolution is applied, `0..=`[`MAX_SHARPEN_AMOUNT`].
    pub amount: f32,
    /// Where it was withheld.
    pub mask: SharpenMask,
    /// The share of [`SharpenSpec::amount`] withheld on skin, `0..1`.
    pub skin_attenuation: f32,
    /// How many Richardson-Lucy iterations ran, at most [`MAX_DECONV_ITERATIONS`].
    pub iterations: u8,
}

impl SharpenSpec {
    /// What is wrong with this spec, if anything.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if !(SHARPEN_KERNEL_LO..=SHARPEN_KERNEL_HI).contains(&self.kernel_sigma) {
            return Some(format!(
                "kernel sigma {:.3} is outside {SHARPEN_KERNEL_LO}..{SHARPEN_KERNEL_HI}",
                self.kernel_sigma
            ));
        }
        if !(0.0..=MAX_SHARPEN_AMOUNT).contains(&self.amount) {
            return Some(format!(
                "sharpen amount {:.3} is above {MAX_SHARPEN_AMOUNT}",
                self.amount
            ));
        }
        if !(0.0..=1.0).contains(&self.skin_attenuation) {
            return Some("the skin attenuation is outside 0..1".into());
        }
        if self.skin_attenuation < SKIN_ATTENUATION - 1e-4 {
            return Some(format!(
                "skin attenuation {:.3} is below the contract's {SKIN_ATTENUATION}",
                self.skin_attenuation
            ));
        }
        if self.iterations == 0 || self.iterations > MAX_DECONV_ITERATIONS {
            return Some(format!(
                "{} iterations is outside 1..{MAX_DECONV_ITERATIONS}",
                self.iterations
            ));
        }
        self.mask.problem()
    }

    /// The amount that reaches skin.
    #[must_use]
    pub fn amount_on_skin(&self) -> f32 {
        self.amount * (1.0 - self.skin_attenuation).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Face recovery
// ---------------------------------------------------------------------------

/// What happened to one face.
///
/// **The identity record.** Every face this phase considered gets a row, whether it was
/// recovered, skipped for being too blurred, skipped for being sharp enough already, or skipped
/// because its embedding moved. `identity_drift` is filled on every row that reached a render,
/// which is what makes section 10.1's gate a query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RecoveredFace {
    /// Whose face, when phase 06 has assigned one.
    #[serde(default)]
    pub identity: Option<IdentityId>,
    /// Where it is, in frame coordinates.
    pub bounds: Box2,
    /// The measured sharpness that decided whether it was in the band, `0..1`.
    pub sharpness: f32,
    /// The strength that survived, `0..=`[`MAX_FACE_RECOVERY`]. Zero when skipped.
    pub strength: f32,
    /// How far the embedding moved, `0..1`. Zero when nothing was rendered.
    pub identity_drift: f32,
    /// How many times the strength was reduced to get there.
    pub resolves: u8,
    /// True when nothing was applied to this face.
    pub skipped: bool,
    /// Why it was skipped, when it was.
    #[serde(default)]
    pub skipped_because: Option<RestoreCode>,
}

impl RecoveredFace {
    /// What is wrong with this record, if anything.
    ///
    /// Four checks, and the third is the guarantee: a face that was not skipped may not carry a
    /// drift above the ceiling, ever, at any strength.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if !(0.0..=MAX_FACE_RECOVERY).contains(&self.strength) {
            return Some(format!(
                "face recovery strength {:.3} is above {MAX_FACE_RECOVERY}",
                self.strength
            ));
        }
        if !(0.0..=1.0).contains(&self.identity_drift) {
            return Some("the identity drift is outside 0..1".into());
        }
        if !self.skipped && self.identity_drift > MAX_IDENTITY_DRIFT + 1e-6 {
            return Some(format!(
                "a face was kept at identity drift {:.4}, above {MAX_IDENTITY_DRIFT}",
                self.identity_drift
            ));
        }
        if self.skipped && self.strength > 0.0 {
            return Some("a skipped face carries a strength".into());
        }
        if self.skipped && self.skipped_because.is_none() {
            return Some("a skipped face carries no reason".into());
        }
        if self.resolves > MAX_RESOLVES {
            return Some(format!(
                "{} resolves is above {MAX_RESOLVES}",
                self.resolves
            ));
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Where the work runs
// ---------------------------------------------------------------------------

/// Where the heavy pixels are pushed. Section 5's frozen vocabulary, verbatim.
///
/// **[`RunWhere::Cloud`] exists and nothing in this build returns it.** Section 7 of PHASE-22 is
/// one sentence - "No cloud AI call in this phase" - and section 2.1 lists an offload anyway.
/// The variant is here because section 5 freezes it and a variant that is absent cannot be added
/// later without a contract change; `aura-restore` does not depend on `aura-cloud` and a test
/// fails the build if it ever does. ADR-0047 section 7.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum RunWhere {
    /// A device backend. What section 11's budgets are written about.
    LocalGpu,
    /// The processor reference path. What this build has.
    #[default]
    LocalCpu,
    /// A provider, with consent. Not reachable in this build.
    Cloud,
}

impl RunWhere {
    /// Every destination, in the order the panel lists them.
    pub const ALL: [Self; 3] = [Self::LocalGpu, Self::LocalCpu, Self::Cloud];

    /// Stable text for the catalog and the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalGpu => "local_gpu",
            Self::LocalCpu => "local_cpu",
            Self::Cloud => "cloud",
        }
    }

    /// Parse the catalog spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|where_| where_.as_str() == text)
    }

    /// True when this destination sends the photograph off the device.
    #[must_use]
    pub const fn leaves_the_device(self) -> bool {
        matches!(self, Self::Cloud)
    }
}

impl fmt::Display for RunWhere {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// When the work runs. Section 6.4, as a type with no third variant.
///
/// "Restoration never runs on the interactive path" is enforced by there being nowhere to say
/// otherwise, rather than by a check somebody could forget.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum RestoreWhen {
    /// At export, on the frames being delivered.
    #[default]
    Export,
    /// As an explicit background enhancement pass, with progress and cancellation.
    Background,
}

impl RestoreWhen {
    /// Both occasions.
    pub const ALL: [Self; 2] = [Self::Export, Self::Background];

    /// Stable text for telemetry and the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Export => "export",
            Self::Background => "background",
        }
    }

    /// Parse the wire spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|when| when.as_str() == text)
    }
}

// ---------------------------------------------------------------------------
// The self-check
// ---------------------------------------------------------------------------

/// What the self-check measured on the rendered result. Section 6.4, as three numbers.
///
/// **Three, not one.** Smearing is fixed by lowering the denoise tier, ringing by lowering the
/// sharpen amount and drift by lowering the face-recovery strength; a single score would leave
/// the automatic reduction section 6.4 requires with no way to know which lever to pull.
/// ADR-0047 section 2.1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ArtefactReport {
    /// High-band energy outside the face after the plan over the same energy before it.
    ///
    /// One is a restoration that cost no texture. Held at or above [`MIN_TEXTURE_RETENTION`], or
    /// the denoise tier is stepped down.
    pub texture_retention: f32,
    /// Mean edge overshoot on the strongest edges, `0..1`.
    ///
    /// Zero is a restoration that rang nowhere. Held below [`MAX_RINGING`], or the sharpen
    /// amount is reduced.
    pub ringing: f32,
    /// The largest face-embedding movement over the faces that were kept, `0..1`.
    ///
    /// Held at or below [`MAX_IDENTITY_DRIFT`] on every kept face, without exception.
    pub identity_drift: f32,
    /// How many pixels the first two measurements were taken over.
    ///
    /// The panel shows three decimal places only when this is large enough to mean something. A
    /// ratio over eleven samples is arithmetic rather than evidence - phase 21's rule.
    pub measured_on: u32,
    /// How many times a strength was reduced to reach the bounds, summed over the three
    /// operations.
    pub resolves: u8,
    /// True when denoising was stepped down by the self-check.
    pub denoise_reduced: bool,
    /// True when sharpening was reduced or withdrawn by the self-check.
    pub sharpen_reduced: bool,
    /// True when at least one face was skipped for drift.
    pub face_skipped: bool,
}

impl ArtefactReport {
    /// The report for a frame nothing was done to.
    pub const UNTOUCHED: Self = Self {
        texture_retention: 1.0,
        ringing: 0.0,
        identity_drift: 0.0,
        measured_on: 0,
        resolves: 0,
        denoise_reduced: false,
        sharpen_reduced: false,
        face_skipped: false,
    };

    /// True when every bound in this phase is met.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.texture_retention >= MIN_TEXTURE_RETENTION - 1e-4
            && self.ringing <= MAX_RINGING + 1e-6
            && self.identity_drift <= MAX_IDENTITY_DRIFT + 1e-6
    }

    /// What is wrong with this report, if anything.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if !(0.0..=4.0).contains(&self.texture_retention) {
            return Some(format!(
                "texture retention {:.3} is outside 0..4",
                self.texture_retention
            ));
        }
        if !(0.0..=1.0).contains(&self.ringing) {
            return Some(format!("ringing {:.4} is outside 0..1", self.ringing));
        }
        if !(0.0..=1.0).contains(&self.identity_drift) {
            return Some("the identity drift is outside 0..1".into());
        }
        if self.resolves > MAX_RESOLVES * 3 {
            return Some(format!(
                "{} resolves is above {}",
                self.resolves,
                MAX_RESOLVES * 3
            ));
        }
        if self.measured_on == 0 && !self.is_clean() {
            return Some("a report that measured nothing claims a violation".into());
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Which of the three decisions a reason is about.
///
/// Section 5 has one `denoise_reason` field; a plan makes three decisions and a photographer
/// asking "why is this still soft" is asking about one of them. ADR-0047 section 2.1.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum RestoreSubject {
    /// The denoise tier.
    #[default]
    Denoise,
    /// The deconvolution sharpening.
    Sharpen,
    /// Face recovery.
    FaceRecovery,
    /// The plan as a whole: scheduling, versions, coverage.
    Plan,
}

impl RestoreSubject {
    /// Every subject, in the order the panel groups them.
    pub const ALL: [Self; 4] = [Self::Denoise, Self::Sharpen, Self::FaceRecovery, Self::Plan];

    /// How many subjects there are.
    pub const COUNT: usize = 4;

    /// Stable text for the catalog and the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Denoise => "denoise",
            Self::Sharpen => "sharpen",
            Self::FaceRecovery => "face_recovery",
            Self::Plan => "plan",
        }
    }
}

impl fmt::Display for RestoreSubject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a restoration came out the way it did.
///
/// Thirty codes. Two thirds of them are refusals, which is the honest shape of a phase whose
/// section 6.2 has three refusals in four bullets: the commonest question this phase generates is
/// not "why did AURA sharpen this" but "why did it not".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreCode {
    // --- denoise -----------------------------------------------------------
    /// The measured noise is inside what this scene tolerates.
    NoiseWithinTolerance,
    /// The tier came from the measured sigma relative to the scene's tolerance.
    TierFromMeasuredNoise,
    /// The tier was raised because the subject fills the frame.
    TierRaisedForSubject,
    /// The tier was raised because the delivery is large.
    TierRaisedForOutput,
    /// The tier was lowered to the scene's own ceiling.
    TierCappedByScene,
    /// The tier was lowered because this camera's noise model was never measured.
    TierCappedUnmeasuredCamera,
    /// The tier was lowered by the self-check after texture was lost.
    TierReducedBySelfCheck,
    /// Chroma was reduced further than luminance to protect fabric detail.
    ChromaFavouredOverLuminance,
    /// No noise reading arrived from phase 09, so nothing was denoised.
    NoNoiseReading,
    /// This camera has no noise model of its own; the reference model conditioned it.
    ReferenceNoiseModel,

    // --- sharpen -----------------------------------------------------------
    /// The estimated kernel was inside the band and the frame was sharpened.
    KernelInBand,
    /// The frame is already as sharp as the lens made it.
    KernelTooSmall,
    /// The blur is gross; deconvolution would ring rather than recover.
    KernelTooLarge,
    /// The blur is motion rather than defocus. Section 2.2 puts this out of scope.
    MotionDominated,
    /// The focus landed in front of or behind the subject.
    GrossDefocus,
    /// Phase 18 supplied no regions, so sharpening was refused rather than run blind.
    SharpenNoRegions,
    /// The amount was capped by the noise left after denoising.
    AmountCappedByNoise,
    /// The amount was reduced by the self-check after ringing was measured.
    AmountReducedBySelfCheck,
    /// Sharpening was withdrawn entirely because the ringing bound could not be met.
    SharpenWithdrawn,
    /// Skin was attenuated rather than excluded.
    SkinAttenuated,
    /// Sky and out-of-focus background were excluded.
    SkyAndBokehExcluded,

    // --- face recovery -----------------------------------------------------
    /// A face was recovered inside the band, and its identity held.
    FaceRecovered,
    /// The face is sharp enough already.
    FaceSharpEnough,
    /// The face is too blurred; a prior would invent one.
    FaceTooBlurred,
    /// The strength was reduced because the embedding had moved.
    StrengthReducedForIdentity,
    /// The face was skipped because the embedding still moved too far.
    IdentityDriftSkipped,
    /// No face-recovery head is trained in this build, so nothing was recovered.
    RecoveryHeadUntrained,
    /// Phase 06 found no faces here.
    NoFaces,

    // --- plan --------------------------------------------------------------
    /// The work was scheduled off the interactive path.
    ScheduledOffInteractive,
    /// A region arrived from phase 18 that could not be read.
    RegionUnusable,
}

impl RestoreCode {
    /// Every code, in the order this file declares them.
    pub const ALL: [Self; 30] = [
        Self::NoiseWithinTolerance,
        Self::TierFromMeasuredNoise,
        Self::TierRaisedForSubject,
        Self::TierRaisedForOutput,
        Self::TierCappedByScene,
        Self::TierCappedUnmeasuredCamera,
        Self::TierReducedBySelfCheck,
        Self::ChromaFavouredOverLuminance,
        Self::NoNoiseReading,
        Self::ReferenceNoiseModel,
        Self::KernelInBand,
        Self::KernelTooSmall,
        Self::KernelTooLarge,
        Self::MotionDominated,
        Self::GrossDefocus,
        Self::SharpenNoRegions,
        Self::AmountCappedByNoise,
        Self::AmountReducedBySelfCheck,
        Self::SharpenWithdrawn,
        Self::SkinAttenuated,
        Self::SkyAndBokehExcluded,
        Self::FaceRecovered,
        Self::FaceSharpEnough,
        Self::FaceTooBlurred,
        Self::StrengthReducedForIdentity,
        Self::IdentityDriftSkipped,
        Self::RecoveryHeadUntrained,
        Self::NoFaces,
        Self::ScheduledOffInteractive,
        Self::RegionUnusable,
    ];

    /// How many codes there are.
    pub const COUNT: usize = 30;

    /// Stable slug for the catalog, the ledger and the wire. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoiseWithinTolerance => "restore_noise_within_tolerance",
            Self::TierFromMeasuredNoise => "restore_tier_from_measured_noise",
            Self::TierRaisedForSubject => "restore_tier_raised_for_subject",
            Self::TierRaisedForOutput => "restore_tier_raised_for_output",
            Self::TierCappedByScene => "restore_tier_capped_by_scene",
            Self::TierCappedUnmeasuredCamera => "restore_tier_capped_unmeasured_camera",
            Self::TierReducedBySelfCheck => "restore_tier_reduced_by_self_check",
            Self::ChromaFavouredOverLuminance => "restore_chroma_favoured_over_luminance",
            Self::NoNoiseReading => "restore_no_noise_reading",
            Self::ReferenceNoiseModel => "restore_reference_noise_model",
            Self::KernelInBand => "restore_kernel_in_band",
            Self::KernelTooSmall => "restore_kernel_too_small",
            Self::KernelTooLarge => "restore_kernel_too_large",
            Self::MotionDominated => "restore_motion_dominated",
            Self::GrossDefocus => "restore_gross_defocus",
            Self::SharpenNoRegions => "restore_sharpen_no_regions",
            Self::AmountCappedByNoise => "restore_amount_capped_by_noise",
            Self::AmountReducedBySelfCheck => "restore_amount_reduced_by_self_check",
            Self::SharpenWithdrawn => "restore_sharpen_withdrawn",
            Self::SkinAttenuated => "restore_skin_attenuated",
            Self::SkyAndBokehExcluded => "restore_sky_and_bokeh_excluded",
            Self::FaceRecovered => "restore_face_recovered",
            Self::FaceSharpEnough => "restore_face_sharp_enough",
            Self::FaceTooBlurred => "restore_face_too_blurred",
            Self::StrengthReducedForIdentity => "restore_strength_reduced_for_identity",
            Self::IdentityDriftSkipped => "restore_identity_drift_skipped",
            Self::RecoveryHeadUntrained => "restore_recovery_head_untrained",
            Self::NoFaces => "restore_no_faces",
            Self::ScheduledOffInteractive => "restore_scheduled_off_interactive",
            Self::RegionUnusable => "restore_region_unusable",
        }
    }

    /// Parse the stored slug.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|code| code.as_str() == text)
    }

    /// Which decision this code is about.
    #[must_use]
    pub const fn subject(self) -> RestoreSubject {
        match self {
            Self::NoiseWithinTolerance
            | Self::TierFromMeasuredNoise
            | Self::TierRaisedForSubject
            | Self::TierRaisedForOutput
            | Self::TierCappedByScene
            | Self::TierCappedUnmeasuredCamera
            | Self::TierReducedBySelfCheck
            | Self::ChromaFavouredOverLuminance
            | Self::NoNoiseReading
            | Self::ReferenceNoiseModel => RestoreSubject::Denoise,
            Self::KernelInBand
            | Self::KernelTooSmall
            | Self::KernelTooLarge
            | Self::MotionDominated
            | Self::GrossDefocus
            | Self::SharpenNoRegions
            | Self::AmountCappedByNoise
            | Self::AmountReducedBySelfCheck
            | Self::SharpenWithdrawn
            | Self::SkinAttenuated
            | Self::SkyAndBokehExcluded => RestoreSubject::Sharpen,
            Self::FaceRecovered
            | Self::FaceSharpEnough
            | Self::FaceTooBlurred
            | Self::StrengthReducedForIdentity
            | Self::IdentityDriftSkipped
            | Self::RecoveryHeadUntrained
            | Self::NoFaces => RestoreSubject::FaceRecovery,
            Self::ScheduledOffInteractive | Self::RegionUnusable => RestoreSubject::Plan,
        }
    }

    /// True when this code says an operation did **not** run, or ran less than it wanted to.
    ///
    /// Twenty of the thirty. The panel renders these at least as prominently as the ten that say
    /// something happened - phase 20's rule, and this is the phase where it matters most,
    /// because a photographer who cannot see why a frame was left alone assumes it was missed.
    #[must_use]
    pub const fn is_restraint(self) -> bool {
        matches!(
            self,
            Self::NoiseWithinTolerance
                | Self::TierCappedByScene
                | Self::TierCappedUnmeasuredCamera
                | Self::TierReducedBySelfCheck
                | Self::NoNoiseReading
                | Self::ReferenceNoiseModel
                | Self::KernelTooSmall
                | Self::KernelTooLarge
                | Self::MotionDominated
                | Self::GrossDefocus
                | Self::SharpenNoRegions
                | Self::AmountCappedByNoise
                | Self::AmountReducedBySelfCheck
                | Self::SharpenWithdrawn
                | Self::SkinAttenuated
                | Self::SkyAndBokehExcluded
                | Self::FaceSharpEnough
                | Self::FaceTooBlurred
                | Self::StrengthReducedForIdentity
                | Self::IdentityDriftSkipped
                | Self::RecoveryHeadUntrained
                | Self::NoFaces
                | Self::RegionUnusable
        )
    }

    /// The sentence a photographer reads.
    ///
    /// Stored as a code and rendered as a sentence, phase 09's rule for the ninth phase running:
    /// a catalog full of English cannot be translated, and a stored sentence is copy a release
    /// can change.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::NoiseWithinTolerance => {
                "there is no more noise here than this kind of photograph carries well, so nothing was removed"
            }
            Self::TierFromMeasuredNoise => {
                "the amount of noise reduction was chosen from how much noise was actually measured in this frame"
            }
            Self::TierRaisedForSubject => {
                "the subject fills this frame, so the noise is on somebody rather than on a wall, and a little more was removed"
            }
            Self::TierRaisedForOutput => {
                "this is being delivered large, where noise that is invisible on a screen becomes visible in print"
            }
            Self::TierCappedByScene => {
                "this kind of photograph is allowed only so much noise reduction, and that limit was reached"
            }
            Self::TierCappedUnmeasuredCamera => {
                "AURA has not measured this camera's own noise, so it has held back from the strongest setting"
            }
            Self::TierReducedBySelfCheck => {
                "AURA checked the result, found it was losing fine texture, and denoised more gently"
            }
            Self::ChromaFavouredOverLuminance => {
                "colour speckle was removed more than luminance grain, which is what keeps lace and fabric looking like fabric"
            }
            Self::NoNoiseReading => {
                "AURA has not measured the noise in this photograph yet, so it has not denoised it"
            }
            Self::ReferenceNoiseModel => {
                "AURA has no measurements for this camera body, so it used its general model of a sensor"
            }
            Self::KernelInBand => {
                "this frame is slightly soft in a way that can be recovered, so it was sharpened"
            }
            Self::KernelTooSmall => "this frame is already as sharp as the lens made it",
            Self::KernelTooLarge => {
                "this frame is too soft to recover; sharpening it would draw outlines rather than detail"
            }
            Self::MotionDominated => {
                "the softness here is movement rather than focus, and AURA does not try to undo movement"
            }
            Self::GrossDefocus => {
                "the focus landed in front of or behind the subject, which sharpening cannot fix"
            }
            Self::SharpenNoRegions => {
                "AURA could not tell where the skin, the sky and the out-of-focus background were, so it did not sharpen anything"
            }
            Self::AmountCappedByNoise => {
                "sharpening makes noise more visible, so it was limited by how much noise was left"
            }
            Self::AmountReducedBySelfCheck => {
                "AURA checked the result, found pale outlines forming along edges, and sharpened more gently"
            }
            Self::SharpenWithdrawn => {
                "AURA could not sharpen this frame without drawing outlines along its edges, so it left it alone"
            }
            Self::SkinAttenuated => "skin was sharpened far less than the rest of the frame",
            Self::SkyAndBokehExcluded => {
                "the sky and the out-of-focus background were left alone, because there is no detail there to recover"
            }
            Self::FaceRecovered => {
                "this face was slightly soft, and detail was recovered without changing who it is"
            }
            Self::FaceSharpEnough => "this face is sharp already",
            Self::FaceTooBlurred => {
                "this face is too blurred to recover; anything AURA put back would be invented rather than recovered"
            }
            Self::StrengthReducedForIdentity => {
                "AURA eased off because the face was starting to measure as a slightly different person"
            }
            Self::IdentityDriftSkipped => {
                "AURA stopped, because it could not recover this face without changing what the person looks like"
            }
            Self::RecoveryHeadUntrained => {
                "face recovery is not available in this build, so no face was changed"
            }
            Self::NoFaces => "no faces were found in this photograph",
            Self::ScheduledOffInteractive => {
                "restoration runs at export or in the background, never while you are editing"
            }
            Self::RegionUnusable => {
                "AURA was not sure enough where something was in this frame, so it left that area out"
            }
        }
    }
}

impl fmt::Display for RestoreCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One reason, with its weight and the evidence behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RestoreReason {
    /// Which code.
    pub code: RestoreCode,
    /// The sentence, resolved at read time.
    pub text: String,
    /// How much this reason moved the decision, `-1..1`.
    pub weight: f32,
    /// Where in the frame, when naming a place helps.
    #[serde(default)]
    pub evidence: Option<Box2>,
}

impl RestoreReason {
    /// A reason carrying the code's own sentence.
    #[must_use]
    pub fn plain(code: RestoreCode, weight: f32) -> Self {
        Self {
            code,
            text: code.user_text().to_string(),
            weight,
            evidence: None,
        }
    }

    /// A reason carrying the code's own sentence and a rectangle.
    #[must_use]
    pub fn plain_at(code: RestoreCode, weight: f32, evidence: Box2) -> Self {
        Self {
            code,
            text: code.user_text().to_string(),
            weight,
            evidence: Some(evidence),
        }
    }

    /// Which decision this reason is about.
    #[must_use]
    pub const fn subject(&self) -> RestoreSubject {
        self.code.subject()
    }

    /// True when this reason says something did not happen.
    #[must_use]
    pub const fn is_restraint(&self) -> bool {
        self.code.is_restraint()
    }
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// One photograph's restoration decision. PHASE-22 section 5, plus the five spellings
/// ADR-0047 section 2.1 argues for.
///
/// **It is not an edit.** The three operations reach the pixels only through
/// `aura_recipe::schema::merge` writing `global.noise`, `restoration.face_recovery` and
/// `global.sharpen` - phase 14's rule for the sixth phase running. A plan with
/// `user_edited = true` still carries AURA's own numbers, so phase 30's learning loop can read
/// the disagreement.
#[derive(Debug, Clone, PartialEq)]
pub struct RestorePlan {
    /// The photograph.
    pub image_id: ImageId,
    /// How hard it is denoised. Section 5, verbatim.
    pub denoise: DenoiseTier,
    /// What that tier became under this frame's noise model. `Some` exactly when the tier is not
    /// [`DenoiseTier::Off`].
    pub denoise_spec: Option<DenoiseSpec>,
    /// The deconvolution sharpening, when there is any. Section 5, verbatim.
    pub sharpen: Option<SharpenSpec>,
    /// The plan-wide face-recovery strength that survived. Section 5, verbatim.
    pub face_recovery: Option<f32>,
    /// What happened to each face, at most [`MAX_RECOVERED_FACES`].
    pub recovered: Vec<RecoveredFace>,
    /// Where the heavy pixels are pushed. Section 5, verbatim.
    pub run_where: RunWhere,
    /// When they are pushed there.
    pub when: RestoreWhen,
    /// What the self-check measured. Section 5, verbatim.
    pub selfcheck: Option<ArtefactReport>,
    /// Why. Never empty; invariant 2. Section 5's `denoise_reason`, widened to three decisions.
    pub reasons: Vec<RestoreReason>,
    /// How much the plan trusts itself, `0..1`. Section 5, verbatim.
    pub confidence: f32,
    /// The scene this was decided under. Invariant 7.
    pub scene: SceneId,
    /// True when phase 18 supplied at least one usable region for this frame.
    pub region_covered: bool,
    /// True when a photographer changed the tier on this frame.
    pub user_edited: bool,
    /// True when a photographer has looked at this plan and agreed.
    pub reviewed: bool,
    /// Which learned heads produced the decisions.
    pub model_ver: u16,
    /// Which build's arithmetic produced them.
    pub analysis_ver: u16,
    /// Which scene-profile and noise-model tables the ceilings came from.
    pub profile_ver: u16,
}

impl RestorePlan {
    /// The most reasons one plan carries.
    ///
    /// Twelve. Larger than every phase since 09, because this plan explains three decisions
    /// rather than one and a panel that groups by [`RestoreSubject`] shows four at a time.
    pub const MAX_REASONS: usize = 12;

    /// A plan that does nothing, for a frame that needed nothing.
    ///
    /// Still carries a reason, because a plan with no reason is a bug rather than an empty plan.
    #[must_use]
    pub fn nothing(image_id: ImageId, scene: SceneId, reason: RestoreReason) -> Self {
        Self {
            image_id,
            denoise: DenoiseTier::Off,
            denoise_spec: None,
            sharpen: None,
            face_recovery: None,
            recovered: Vec::new(),
            run_where: RunWhere::LocalCpu,
            when: RestoreWhen::Export,
            selfcheck: None,
            reasons: vec![reason],
            confidence: 1.0,
            scene,
            region_covered: false,
            user_edited: false,
            reviewed: false,
            model_ver: 0,
            analysis_ver: 0,
            profile_ver: 0,
        }
    }

    /// True when this plan changes no pixel.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.denoise == DenoiseTier::Off
            && self.sharpen.is_none()
            && self.face_recovery.unwrap_or(0.0) <= 0.0
    }

    /// Section 5's `denoise_reason`, as a view over the widened list.
    ///
    /// The frozen field, still answerable. ADR-0047 section 2.1.
    #[must_use]
    pub fn denoise_reasons(&self) -> Vec<&RestoreReason> {
        self.reasons_for(RestoreSubject::Denoise)
    }

    /// The reasons about one of the three decisions.
    #[must_use]
    pub fn reasons_for(&self, subject: RestoreSubject) -> Vec<&RestoreReason> {
        self.reasons
            .iter()
            .filter(|reason| reason.subject() == subject)
            .collect()
    }

    /// How many faces this plan actually recovered.
    #[must_use]
    pub fn faces_recovered(&self) -> usize {
        self.recovered.iter().filter(|face| !face.skipped).count()
    }

    /// How many faces this plan refused for identity drift.
    ///
    /// **The guarantee's own counter.** Phase 27 reads it, and so does section 10.1.
    #[must_use]
    pub fn faces_skipped_for_identity(&self) -> usize {
        self.recovered
            .iter()
            .filter(|face| face.skipped_because == Some(RestoreCode::IdentityDriftSkipped))
            .count()
    }

    /// The largest identity movement over the faces that were kept, `0..1`.
    #[must_use]
    pub fn worst_kept_drift(&self) -> f32 {
        self.recovered
            .iter()
            .filter(|face| !face.skipped)
            .map(|face| face.identity_drift)
            .fold(0.0_f32, f32::max)
    }

    /// True when the frame is worth a photographer's attention.
    #[must_use]
    pub fn needs_review(&self) -> bool {
        !self.reviewed && !self.user_edited && self.confidence < REVIEW_BELOW
    }

    /// What guarantee this plan breaks, if any.
    ///
    /// Nine checks, and every one is an acceptance criterion rather than a type error: there is
    /// at least one reason and not too many, the denoise spec agrees with the tier, the sharpen
    /// spec is inside its bounds and carries regions, the face-recovery strength is capped, every
    /// kept face is inside the identity ceiling, a skipped face carries a reason, the self-check
    /// is coherent, and a plan that acted has something to show for it. They live here so the
    /// solver, the store, the IPC layer and the eval harness all refuse the same frames.
    /// `aura_restore::selfcheck` turns a `Some` here into `AURA-ML-5109`.
    #[must_use]
    pub fn broken_guarantee(&self) -> Option<String> {
        if self.reasons.is_empty() {
            return Some("a plan with no reason".into());
        }
        if self.reasons.len() > Self::MAX_REASONS {
            return Some(format!(
                "{} reasons is above {}",
                self.reasons.len(),
                Self::MAX_REASONS
            ));
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Some(format!("confidence {:.3} is outside 0..1", self.confidence));
        }
        match (self.denoise, self.denoise_spec.as_ref()) {
            (DenoiseTier::Off, Some(_)) => {
                return Some("a plan denoising nothing carries a denoise spec".into())
            }
            (DenoiseTier::Off, None) => {}
            (tier, None) => return Some(format!("tier {tier} carries no denoise spec")),
            (_, Some(spec)) => {
                if let Some(problem) = spec.problem() {
                    return Some(problem);
                }
            }
        }
        if let Some(sharpen) = &self.sharpen {
            if let Some(problem) = sharpen.problem() {
                return Some(problem);
            }
        }
        if let Some(strength) = self.face_recovery {
            if !(0.0..=MAX_FACE_RECOVERY).contains(&strength) {
                return Some(format!(
                    "face recovery strength {strength:.3} is above {MAX_FACE_RECOVERY}"
                ));
            }
        }
        if self.recovered.len() > MAX_RECOVERED_FACES {
            return Some(format!(
                "{} face records is above {MAX_RECOVERED_FACES}",
                self.recovered.len()
            ));
        }
        for face in &self.recovered {
            if let Some(problem) = face.problem() {
                return Some(problem);
            }
        }
        if let Some(report) = &self.selfcheck {
            if let Some(problem) = report.problem() {
                return Some(problem);
            }
            if !report.is_clean() {
                return Some(
                    "a stored plan carries a self-check that is still outside its bounds".into(),
                );
            }
        } else if !self.is_noop() {
            return Some("a plan that changes pixels carries no self-check".into());
        }
        if self.sharpen.is_some() && !self.region_covered {
            return Some("sharpening ran on a frame with no regions from phase 18".into());
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

/// What a project's restoration pass covered and what it did.
///
/// Phase 05's rule, inherited for the sixteenth time: report coverage when you report a result,
/// and say what the denominator is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RestoreOutline {
    /// Photographs with a plan.
    pub planned: u32,
    /// Photographs in the project.
    pub photos: u32,
    /// Fraction of the project with a plan, `0..1`.
    ///
    /// **The denominator is every photograph**, as phases 09 to 15, 20 and 21 all are.
    pub coverage: f32,
    /// Photographs where at least one operation ran.
    pub acted_on: u32,
    /// Photographs where the regions this phase needs arrived from phase 18.
    ///
    /// The number that separates "there was nothing to sharpen" from "AURA could not see where
    /// the sky was". Zero on a build with no mask generator wired in.
    pub region_covered: u32,
    /// How many frames got each tier, in [`DenoiseTier::ALL`] order.
    pub tier_histogram: [u32; DenoiseTier::COUNT],
    /// Frames that were sharpened.
    pub sharpened: u32,
    /// Frames where sharpening was refused, by reason, in [`RestoreCode::ALL`] order.
    ///
    /// A histogram rather than a count, because "AURA sharpened nothing in this wedding" has six
    /// different causes and five of them are somebody else's bug.
    pub sharpen_refusals: Vec<(RestoreCode, u32)>,
    /// Faces recovered across the project.
    pub faces_recovered: u32,
    /// Faces skipped because the embedding moved. **The guarantee's counter.**
    pub faces_skipped_identity: u32,
    /// The largest identity movement over every kept face in the project, `0..1`.
    pub worst_identity_drift: f32,
    /// Mean texture retention over frames that were denoised.
    pub mean_texture_retention: f32,
    /// Mean ringing over frames that were sharpened.
    pub mean_ringing: f32,
    /// Frames where the self-check reduced a strength.
    pub reduced: u32,
    /// Frames that ran on each destination, in [`RunWhere::ALL`] order.
    pub run_where_histogram: [u32; 3],
    /// Frames below the review threshold.
    pub needs_review: u32,
    /// Frames a photographer has changed by hand.
    pub user_edited: u32,
    /// Camera bodies with no measured noise model, which were denoised against a synthetic one.
    ///
    /// **The condition that closes when a photographed reference arrives.** A wedding shot
    /// entirely on unmeasured bodies is a wedding capped at [`DenoiseTier::Standard`], and this
    /// is how a photographer finds that out.
    pub unmeasured_cameras: Vec<String>,
    /// Scenes with no row in the profile file, which were planned against the neutral row.
    pub unlisted_scenes: Vec<String>,
    /// The versions the stored plans were made under.
    pub model_ver: u16,
    /// The build arithmetic they were made under.
    pub analysis_ver: u16,
    /// The profile tables they were made under.
    pub profile_ver: u16,
}

impl RestoreOutline {
    /// True when every stored plan agrees with the running build about all three versions.
    #[must_use]
    pub const fn versions_agree(&self, current: (u16, u16, u16)) -> bool {
        self.model_ver == current.0
            && self.analysis_ver == current.1
            && self.profile_ver == current.2
    }

    /// True when this project delivered a frame whose identity moved further than the ceiling.
    ///
    /// Always false on a sound catalog; it is here so that a caller can assert it rather than
    /// having to know how.
    #[must_use]
    pub fn identity_guarantee_broken(&self) -> bool {
        self.worst_identity_drift > MAX_IDENTITY_DRIFT + 1e-6
    }
}

// ---------------------------------------------------------------------------
// Overrides
// ---------------------------------------------------------------------------

/// What the photographer chose instead.
///
/// Every field optional and independent, the shape phases 15, 19, 20 and 21 all use.
///
/// **There is a tier here and there is no strength anywhere.** A photographer chooses which of
/// the four tiers a frame gets and whether sharpening and face recovery may run at all; how much
/// each of those does at that tier is a product decision bounded by the contract. A surface that
/// could raise a ceiling would make the guarantees in `docs/restoration.md` a description of the
/// defaults rather than a promise - phase 21's rule, inherited.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOverride {
    /// Which tier this frame gets. `None` leaves AURA's own choice alone.
    #[serde(default)]
    pub denoise: Option<DenoiseTier>,
    /// Whether sharpening may run. `None` leaves the decision alone.
    #[serde(default)]
    pub sharpen: Option<bool>,
    /// Whether face recovery may run. `None` leaves the decision alone.
    ///
    /// Separate from `sharpen` because a photographer can want a frame sharpened and want no
    /// model near anybody's face, and collapsing the two would force them to choose.
    #[serde(default)]
    pub face_recovery: Option<bool>,
}

impl RestoreOverride {
    /// True when this override sets nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.denoise.is_none() && self.sharpen.is_none() && self.face_recovery.is_none()
    }

    /// What is wrong with this override, if anything.
    ///
    /// `aura_restore::store` turns a `Some` here into `AURA-ML-5110`.
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

/// The one way to ask what was repaired in a photograph.
///
/// Eighteenth service of its kind, and it carries the rule for the eighteenth time: **no phase
/// may keep its own denoiser, its own kernel estimator or its own idea of how far a face may
/// move.** Phase 23 straightens and crops after this phase sharpens and must not sharpen again;
/// phase 24 fills generatively and inherits this phase's identity constraint rather than
/// re-deriving one; phase 25 normalises a gallery denoised at four different tiers; phase 27 has
/// to be able to say why an edge looks crunchy; phase 28 runs unattended and must know what ran.
/// Two answers to "how much noise reduction did this frame get" is a delivery where the album and
/// the gallery disagree about the same photograph's grain.
pub trait RestoreService: Send + Sync + fmt::Debug {
    /// What a project's pass covered and did.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<RestoreOutline>;

    /// One photograph's plan, or `None` when it has not been planned.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plan cannot be read.
    fn of_image(&self, image: ImageId) -> AuraResult<Option<RestorePlan>>;

    /// Every face in the project whose recovery was refused for identity drift, worst first.
    ///
    /// **The guarantee's own query, and it is on the frozen surface deliberately.** Section 10.1
    /// gates identity preservation at 100 %, and a gate that can only be checked by opening four
    /// hundred plans one at a time is a gate nobody checks. Phase 27 reads this, and so does the
    /// exit report.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the faces cannot be read.
    fn identity_refusals(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>>;

    /// The frames whose restoration is worth a photographer's attention, weakest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>>;

    /// Record that the photographer has looked at this plan and agrees.
    ///
    /// Sets [`RestorePlan::reviewed`] and does not set [`RestorePlan::user_edited`]: accepting a
    /// suggestion is not authoring one, and phase 30's learning loop needs to tell them apart.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5110` when the photograph has no plan.
    fn accept(&self, image: ImageId) -> Result<(), AuraError>;

    /// Record what the photographer chose for one photograph.
    ///
    /// Sets `user_edited` on the row and is not undone by a re-analysis: the check is inside the
    /// statement that would overwrite it, exactly as `identities.user_locked`,
    /// `segments.user_locked`, `moments.user_locked`, `masks.user_edited`,
    /// `local_light_plan.user_edited`, `retouch_plan.user_edited` and `micro_matrix.user_edited`
    /// are.
    ///
    /// **This records the decision; it does not move a pixel.** The pixels move when the caller
    /// writes the plan through `aura_recipe::schema::merge`.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5110` when the override sets nothing, or the photograph has no plan.
    fn set_override(&self, image: ImageId, values: RestoreOverride) -> Result<(), AuraError>;
}
