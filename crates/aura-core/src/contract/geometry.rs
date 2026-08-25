//! FROZEN CONTRACT. How one photograph's frame was finished: what the lens did to it, how
//! far it was turned to level the world, whether the architecture was squared, and what was
//! cropped away - together with everything the product refused to remove.
//!
//! PHASE-23 section 5 freezes [`GeometryPlan`] before any solver exists. The file is in
//! `aura-core` rather than in `aura-geometry` for the reason [`crate::contract::integrity`],
//! [`crate::contract::composition`], [`crate::contract::tone`] and [`crate::contract::local`]
//! are: the phases that consume a geometry decision are 27 (QC, which has to be able to say
//! *why* a frame is cropped the way it is), 29 (curation, which lays albums out of the aspect
//! variants this phase produced) and 30 (delivery, which exports them), and none of them
//! needs the distortion model, the candidate search or the profile table.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **The original framing is index zero, always, in every plan.** [`GeometryPlan::crops`] is
//! never empty and `crops[0]` is always [`CropPurpose::Original`] covering the whole frame.
//! Section 13's "original framing is always one click away" is therefore a property of the
//! shape rather than a promise about a button: there is no state of this contract in which
//! the frame a photographer shot is unrecoverable, because it is the first entry of the list
//! any panel is already rendering. [`GeometryPlan::new`] is the only constructor and it
//! inserts that entry itself.
//!
//! ## The second thing: a crop that cannot be proven safe is not a candidate
//!
//! Every rectangle in [`GeometryPlan::crops`] has passed [`CropSafetyReport`]. The filter runs
//! **before** the composition objective ever sees a candidate, not after it as a penalty -
//! phase 12's rule that a guarantee outranks a preference, in a phase where the preference is
//! a score somebody tuned and the guarantee is a bride's hands. A safety filter applied
//! afterwards invites exactly one repair - nudge the rectangle until the face is back inside -
//! and a nudged crop is a different aspect ratio, a different resolution, or a fresh violation
//! at the opposite edge.
//!
//! [`CropVariant::safe`] is therefore `true` on every variant a well-formed plan carries, and
//! it is in section 5's frozen shape anyway. It is not redundant: it is what a stored row from
//! an older `rules_ver` is re-checked against, and `false` is how a plan says "this rectangle
//! was safe under the rules that produced it and is not under these".
//!
//! ## The third thing: most photographs keep their framing
//!
//! Section 10.1 asks that at least seventy per cent of frames keep the original crop, and
//! [`IMPROVEMENT_MARGIN`] is the mechanism. A proposed primary crop must beat the original
//! framing's composition score by that margin or the original wins outright. **The margin
//! applies to the primary crop only** - an aspect variant for social delivery is asked for by
//! its purpose rather than earned by its score, and requiring a 1:1 crop of a wide reception
//! frame to *improve* the composition would produce a product with no square variants at all.
//!
//! ## The fourth thing: nothing here moves a pixel
//!
//! Seventh phase running. This module produces rectangles, angles and correction flags;
//! `aura_render` applies them, through `edit_recipes` and `aura_recipe::schema::merge` only,
//! which is phase 14's rule. There is no path, no rendered output and no `applied` flag
//! anywhere in this contract, and no field in it could hold image data.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::composition::HORIZON_ACT_AT;
use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::ProjectId;
use crate::contract::integrity::CropRect;
use crate::contract::scene::SceneId;

pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// Bands, caps and floors
// ---------------------------------------------------------------------------

/// Below this horizon confidence nothing is straightened.
///
/// Section 6.2: "rotate only when Phase 11 horizon confidence >= 0.7". It is deliberately
/// *higher* than phase 11's [`HORIZON_ACT_AT`] of 0.60, and the difference is the difference
/// between reporting and acting. Phase 11's floor is where an estimate stops being worth
/// showing a photographer; this is where it becomes worth turning their photograph by. A
/// const assertion below refuses a build in which this ever drops beneath phase 11's floor,
/// because a phase that acted on an estimate phase 11 would not even report would be acting
/// on nothing.
pub const STRAIGHTEN_ACT_AT: f32 = 0.70;

const _: () = assert!(
    STRAIGHTEN_ACT_AT >= HORIZON_ACT_AT,
    "phase 23 must not act on a tilt phase 11 would not report"
);

/// The smallest correction worth making, in degrees.
///
/// Section 6.2's band starts at 0.2. Below it the rotation costs a resample and a crop and
/// buys a change nobody can see - and the resample is not free: every straightened frame is
/// interpolated once more than an untouched one.
pub const MIN_ROTATE_DEG: f32 = 0.20;

/// The largest correction that reads as a mistake rather than as a decision, in degrees.
///
/// Section 6.2: "larger tilts are treated as intentional and left alone". Eight degrees is
/// above phase 11's [`crate::contract::composition::DUTCH_ANGLE_DEG`] of six, which is where
/// a tilt becomes a *candidate* for being deliberate; between six and eight this phase still
/// levels, and phase 11's own `tilt_intentional` is what stops it.
pub const MAX_ROTATE_DEG: f32 = 8.0;

/// The most any axis may be stretched by a keystone correction.
///
/// Section 6.2: "capped so no axis is stretched beyond a documented factor". This is the
/// documented factor. A quarter is roughly where a squared-up doorway stops looking squared
/// up and starts looking like a photograph of a doorway taken through a letterbox; past it
/// the correction is refused rather than reduced, because a keystone that has been halved to
/// fit a cap has stopped correcting anything.
pub const MAX_STRETCH: f32 = 1.25;

/// The smallest keystone worth applying, in the recipe's `-100..100` units.
///
/// Symmetric with [`MIN_ROTATE_DEG`] and for the same reason.
pub const MIN_KEYSTONE: f32 = 2.0;

/// A crop's long edge may not fall below this fraction of the **original** long edge.
///
/// Section 6.3: "resolution >= 60 % of the original long edge". Measured against the frame as
/// shot rather than against the lens-corrected frame, deliberately: a distortion correction
/// that trims three per cent of the width would otherwise raise this floor by three per cent
/// on every frame shot with a profiled lens and on none of the others, which is a resolution
/// rule that depends on which body was in somebody's hand.
pub const RESOLUTION_FLOOR: f32 = 0.60;

/// How much better a proposed primary crop must be than the original framing.
///
/// Section 6.3's "minimum margin", and the mechanism behind section 10.1's "most frames keep
/// their original framing". It is a margin on [`crate::contract::composition::CompositionResult::composition_score`],
/// which is a `0..1` composite, so five points of it is a visible improvement in framing
/// rather than a rounding difference between two candidate rectangles.
pub const IMPROVEMENT_MARGIN: f32 = 0.05;

/// How far outside a protected region a crop edge must stay, as a fraction of the frame.
///
/// A face that ends exactly at the crop boundary is a face that is cut by one pixel of
/// rounding at export, and a rule enforced to the pixel is a rule enforced to whichever
/// rounding mode the resampler happened to use.
pub const SAFETY_MARGIN: f32 = 0.01;

/// The most crop variants one photograph may carry, including the original.
///
/// Six: the original, the primary, and one for each of the four delivery aspects. A plan that
/// wanted more would be storing a search rather than a decision.
pub const MAX_VARIANTS: usize = 6;

// ---------------------------------------------------------------------------
// Lens corrections
// ---------------------------------------------------------------------------

/// Where a lens correction's numbers came from.
///
/// Section 6.1 gives the order of preference - embedded, then the bundled table, then
/// estimation - and a stored correction that does not say which of the three produced it
/// cannot be audited when a lens turns out to be corrected wrongly. It is also what makes
/// [`GeometryCode::LensEstimated`] honest: a distortion coefficient read out of a maker note
/// and one fitted to eleven straight edges are two different claims about the same lens.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum LensSource {
    /// Nothing corrected the optics. The default, and the answer that claims the least.
    #[default]
    None,
    /// The camera wrote correction data into the file and AURA used it.
    Embedded,
    /// The bundled profile table matched the lens id and focal length.
    Profile,
    /// No profile existed, so the distortion was fitted from long straight edges.
    Estimated,
}

impl LensSource {
    /// Every source, in the order section 6.1 prefers them.
    pub const ALL: [Self; 4] = [Self::None, Self::Embedded, Self::Profile, Self::Estimated];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Embedded => "embedded",
            Self::Profile => "profile",
            Self::Estimated => "estimated",
        }
    }

    /// Parse the stored text. Unknown values read as [`LensSource::None`].
    #[must_use]
    pub fn from_str_or_none(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|source| source.as_str() == text)
            .unwrap_or(Self::None)
    }

    /// True when a lens was actually corrected.
    #[must_use]
    pub const fn is_corrected(self) -> bool {
        !matches!(self, Self::None)
    }

    /// True when the numbers were measured by somebody rather than fitted by AURA.
    ///
    /// The distinction the panel draws: a measured profile corrects chromatic aberration, an
    /// estimated one does not, because a CA correction fitted from the same edges it is meant
    /// to clean is a correction that will happily invent fringing of the opposite colour.
    #[must_use]
    pub const fn is_measured(self) -> bool {
        matches!(self, Self::Embedded | Self::Profile)
    }
}

impl fmt::Display for LensSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the optics did, and what was undone about it.
///
/// Section 5's `{ distortion, vignette, ca, profile_id, source }`, with the coefficients the
/// renderer needs rather than three booleans: `aura_recipe::Lens` carries the booleans,
/// because a recipe says *what to apply* and the profile table says *by how much*. A recipe
/// that carried coefficients would be a recipe that renders differently after a profile
/// update without any field in it having changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LensCorrection {
    /// The Brown-Conrady radial terms `k1, k2, k3`, in normalised radius.
    ///
    /// All zero when nothing is corrected. Positive `k1` is barrel distortion, which is what
    /// a wide zoom at its short end has and what most reception frames are shot on.
    pub distortion: [f32; 3],
    /// Vignette correction strength, `0..1`, where one is full correction.
    ///
    /// A fraction rather than the recipe's `0..100` integer, because this is the *decision*
    /// and the recipe is the instruction. Section 6.1 applies it in linear light before the
    /// creative operations so it does not fight phase 15's exposure.
    pub vignette: f32,
    /// Per-channel radial scale factors for red and blue, relative to green.
    ///
    /// `[1.0, 1.0]` when nothing is corrected. Lateral chromatic aberration is green-referred
    /// by construction: the green channel is the one the sensor has twice as many of and the
    /// one a focus system was aimed with, so scaling it would move the whole image.
    pub ca: [f32; 2],
    /// The profile that produced the numbers, when one did.
    #[serde(default)]
    pub profile_id: Option<String>,
    /// Which of section 6.1's three routes produced them.
    pub source: LensSource,
    /// The lens as the file named it, for the missing-profile telemetry.
    ///
    /// Kept even when a profile *was* found, because `geometry.lens_profile_missing` is only
    /// answerable as a histogram if the lens id is on the row whether or not it matched.
    #[serde(default)]
    pub lens_id: Option<String>,
}

impl Default for LensCorrection {
    /// Corrects nothing.
    ///
    /// The same default `aura_recipe::Lens` takes and for the same reason: a lens whose
    /// profile is unknown corrects nothing rather than guessing.
    fn default() -> Self {
        Self {
            distortion: [0.0; 3],
            vignette: 0.0,
            ca: [1.0, 1.0],
            source: LensSource::None,
            profile_id: None,
            lens_id: None,
        }
    }
}

impl LensCorrection {
    /// True when this correction would move a pixel.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.distortion.iter().all(|k| k.abs() < f32::EPSILON)
            && self.vignette.abs() < f32::EPSILON
            && (self.ca[0] - 1.0).abs() < f32::EPSILON
            && (self.ca[1] - 1.0).abs() < f32::EPSILON
    }

    /// True when the distortion model would move a pixel.
    #[must_use]
    pub fn corrects_distortion(&self) -> bool {
        self.distortion.iter().any(|k| k.abs() >= f32::EPSILON)
    }

    /// True when the chromatic aberration model would move a pixel.
    #[must_use]
    pub fn corrects_ca(&self) -> bool {
        (self.ca[0] - 1.0).abs() >= f32::EPSILON || (self.ca[1] - 1.0).abs() >= f32::EPSILON
    }
}

// ---------------------------------------------------------------------------
// Keystone
// ---------------------------------------------------------------------------

/// A perspective correction, already checked against [`MAX_STRETCH`].
///
/// Section 5's `Option<Keystone>` - "stretch-capped" - with the cap expressed as a field
/// rather than as a comment, so that a stored row can be re-checked against a later cap
/// without re-deriving the correction that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keystone {
    /// Vertical keystone, in the recipe's `-100..100` units. Positive squares up a camera
    /// that was pointed upward, which is every photograph of a church.
    pub vertical: f32,
    /// Horizontal keystone, same units.
    pub horizontal: f32,
    /// The scale needed to hide the corners the correction opened, `1.0..`.
    ///
    /// Not a preference: a keystone leaves empty wedges at two corners, and either they are
    /// cropped away or they are filled - and filling them is phase 24, explicitly out of
    /// scope here by section 2.2.
    pub scale: f32,
    /// The largest per-axis stretch this correction actually applies.
    ///
    /// Bounded by [`MAX_STRETCH`] at construction. Stored because the cap is a rule that can
    /// move and the correction is a decision that was made under one version of it.
    pub stretch: f32,
    /// How many strong vertical lines the correction was fitted to.
    ///
    /// Section 6.2 limits keystone "to frames with strong architectural verticals", and a
    /// count of one is a vanishing point fitted through a lamp post.
    pub verticals: u16,
}

impl Keystone {
    /// The fewest converging verticals a correction may be fitted from.
    ///
    /// Three, because two lines always meet somewhere and calling that a vanishing point is
    /// how a keystone tool squares up a frame containing one door frame and a guest.
    pub const MIN_VERTICALS: u16 = 3;

    /// Build a keystone, refusing one that breaks the stretch cap.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5092` when `stretch` exceeds [`MAX_STRETCH`], when fewer than
    /// [`Keystone::MIN_VERTICALS`] lines were fitted, or when `scale` is below one - a
    /// keystone that scaled *down* would open the corners it exists to close.
    pub fn new(
        vertical: f32,
        horizontal: f32,
        scale: f32,
        stretch: f32,
        verticals: u16,
    ) -> AuraResult<Self> {
        let refuse = |detail: &str| -> AuraError { crate::errors::ml::geometry_failed(detail) };
        if !stretch.is_finite() || stretch > MAX_STRETCH || stretch < 1.0 {
            return Err(refuse("keystone stretch outside the documented cap"));
        }
        if verticals < Self::MIN_VERTICALS {
            return Err(refuse("keystone fitted to too few verticals"));
        }
        if !scale.is_finite() || scale < 1.0 {
            return Err(refuse("keystone scale would open the corners"));
        }
        Ok(Self {
            vertical,
            horizontal,
            scale,
            stretch,
            verticals,
        })
    }

    /// True when this correction is too small to be worth a resample.
    #[must_use]
    pub fn is_negligible(&self) -> bool {
        self.vertical.abs() < MIN_KEYSTONE && self.horizontal.abs() < MIN_KEYSTONE
    }
}

// ---------------------------------------------------------------------------
// Crops
// ---------------------------------------------------------------------------

/// What a crop is for.
///
/// A closed set, because section 2.1's "multi-aspect delivery" is only implementable without
/// duplicating files if a consumer can tell which entry to reach for. Phase 29 lays albums
/// out of [`CropPurpose::Album`] and phase 30 uploads [`CropPurpose::Social`]; neither of
/// them may pick by looking at the aspect ratio, because two purposes can share one.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum CropPurpose {
    /// The frame as it was shot. Always present, always index zero.
    #[default]
    Original,
    /// What AURA would deliver. Equal to the original when the improvement margin was not met.
    Primary,
    /// A portrait crop for an album page.
    Album,
    /// A square or vertical crop for social delivery.
    Social,
    /// A wide crop for a gallery header or a spread.
    Wide,
}

impl CropPurpose {
    /// Every purpose.
    pub const ALL: [Self; 5] = [
        Self::Original,
        Self::Primary,
        Self::Album,
        Self::Social,
        Self::Wide,
    ];

    /// How many there are.
    pub const COUNT: usize = 5;

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Primary => "primary",
            Self::Album => "album",
            Self::Social => "social",
            Self::Wide => "wide",
        }
    }

    /// What the panel calls it.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Original => "As shot",
            Self::Primary => "Delivered",
            Self::Album => "Album",
            Self::Social => "Social",
            Self::Wide => "Wide",
        }
    }

    /// Parse the stored text. Unknown values read as [`CropPurpose::Original`].
    #[must_use]
    pub fn from_str_or_original(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|purpose| purpose.as_str() == text)
            .unwrap_or(Self::Original)
    }

    /// True when this variant must earn its place against [`IMPROVEMENT_MARGIN`].
    ///
    /// Only the primary does. **This is the decision most likely to be re-argued**, so it is
    /// spelled as a method rather than as a branch in the solver: an aspect variant is asked
    /// for by its purpose, and a 1:1 crop of a wide reception frame will essentially never
    /// score better than the frame it came out of, because it has thrown away the context
    /// that made the composition work. Requiring an improvement from the variants is a
    /// product with no variants in it.
    #[must_use]
    pub const fn must_improve(self) -> bool {
        matches!(self, Self::Primary)
    }
}

impl fmt::Display for CropPurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A delivery aspect ratio.
///
/// Section 2.1 names them: original, 4:5, 5:4, 1:1 and 16:9. [`Aspect::Original`] is not a
/// ratio at all - it means "whatever this frame already is" - and it is in the enum because
/// the alternative is an `Option<Aspect>` whose `None` reads as "no aspect" rather than as
/// "the one it came with".
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Aspect {
    /// Keep whatever the frame is.
    #[default]
    Original,
    /// 4:5 portrait.
    FourFive,
    /// 5:4 landscape.
    FiveFour,
    /// 1:1 square.
    Square,
    /// 16:9 wide.
    SixteenNine,
}

impl Aspect {
    /// Every aspect.
    pub const ALL: [Self; 5] = [
        Self::Original,
        Self::FourFive,
        Self::FiveFour,
        Self::Square,
        Self::SixteenNine,
    ];

    /// How many there are.
    pub const COUNT: usize = 5;

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::FourFive => "4:5",
            Self::FiveFour => "5:4",
            Self::Square => "1:1",
            Self::SixteenNine => "16:9",
        }
    }

    /// Parse the stored text. Unknown values read as [`Aspect::Original`].
    #[must_use]
    pub fn from_str_or_original(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|aspect| aspect.as_str() == text)
            .unwrap_or(Self::Original)
    }

    /// Width over height, or `None` for [`Aspect::Original`].
    #[must_use]
    pub const fn ratio(self) -> Option<f32> {
        match self {
            Self::Original => None,
            Self::FourFive => Some(0.8),
            Self::FiveFour => Some(1.25),
            Self::Square => Some(1.0),
            Self::SixteenNine => Some(16.0 / 9.0),
        }
    }
}

impl fmt::Display for Aspect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One proposed rectangle.
///
/// Section 5's `{ aspect, rect, purpose, score, safe }`.
///
/// **The rectangle is in the coordinates of the corrected frame**, not of the file. Lens
/// distortion correction and a keystone both move pixels before this rectangle is applied, so
/// a crop expressed against the raw frame would be a crop that drifts by however much the
/// optics were bent. [`GeometryPlan::lens`] and [`GeometryPlan::keystone`] together are the
/// transform these coordinates are relative to, which is why they are on the same row.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CropVariant {
    /// Which delivery aspect this satisfies.
    pub aspect: Aspect,
    /// The rectangle, normalised to the corrected frame.
    pub rect: CropRect,
    /// What it is for.
    pub purpose: CropPurpose,
    /// Its composition score under phase 11's objective, `0..1`.
    ///
    /// Comparable with [`crate::contract::composition::CompositionResult::composition_score`]
    /// by construction - it is the same objective evaluated over a sub-rectangle - which is
    /// what makes [`IMPROVEMENT_MARGIN`] a margin rather than a tuning constant.
    pub score: f32,
    /// True when this rectangle passed every safety rule.
    pub safe: bool,
}

impl CropVariant {
    /// The whole frame, as shot.
    #[must_use]
    pub fn original() -> Self {
        Self {
            aspect: Aspect::Original,
            rect: CropRect::FULL,
            purpose: CropPurpose::Original,
            score: 0.0,
            safe: true,
        }
    }

    /// True when this variant removes nothing.
    #[must_use]
    pub fn is_full_frame(&self) -> bool {
        self.rect.x.abs() < 1e-4
            && self.rect.y.abs() < 1e-4
            && (self.rect.w - 1.0).abs() < 1e-4
            && (self.rect.h - 1.0).abs() < 1e-4
    }

    /// The fraction of the original long edge this crop's long edge keeps.
    ///
    /// Compared against [`RESOLUTION_FLOOR`]. Takes the frame's own aspect ratio because a
    /// 4:5 crop out of a landscape frame keeps most of the height and little of the width,
    /// and which of those is the long edge changes with the crop rather than with the file.
    #[must_use]
    pub fn long_edge_fraction(&self, frame_aspect: f32) -> f32 {
        let width = self.rect.w * frame_aspect;
        let height = self.rect.h;
        let long = width.max(height);
        let frame_long = frame_aspect.max(1.0);
        if frame_long <= 0.0 {
            return 0.0;
        }
        long / frame_long
    }
}

// ---------------------------------------------------------------------------
// Safety
// ---------------------------------------------------------------------------

/// Which of the hard rules a crop was checked against, and what it did to each.
///
/// Section 5's `{ faces_intact, resolution_ok, content_kept }`, plus the counts that make an
/// audit possible. Three booleans is enough to gate a crop and not enough to explain one, and
/// section 9 gives QAIQ "audit 300 auto-crops: any cut hand, cut face or worse framing is a
/// bug" - which is only a finishable job if a failing crop says which face it cut.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CropSafetyReport {
    /// Every detected face is fully inside the primary crop.
    pub faces_intact: bool,
    /// The crop keeps at least [`RESOLUTION_FLOOR`] of the original long edge.
    pub resolution_ok: bool,
    /// Phase 11's crop hint region and the moment's key content are inside.
    pub content_kept: bool,
    /// How many faces were checked.
    ///
    /// Zero means nothing was checked, which is **not** the same as "no face was cut" - and
    /// on this build it is what every frame reports, because phase 06's detector finds no
    /// faces. [`CropSafetyReport::is_evidence`] is how a caller tells the two apart.
    pub faces_checked: u32,
    /// How many candidate rectangles the filter refused, by reason.
    ///
    /// Indexed by [`GeometryCode`]'s refusal codes in [`GeometryCode::REFUSALS`] order.
    /// Section 11's `geometry.crop_refused {reason_histogram}` is this, summed.
    pub refused: [u32; GeometryCode::REFUSAL_COUNT],
}

impl CropSafetyReport {
    /// Everything passed and something was actually checked.
    #[must_use]
    pub fn all_clear(&self) -> bool {
        self.faces_intact && self.resolution_ok && self.content_kept
    }

    /// True when the face rule was checked against at least one face.
    ///
    /// **The distinction phase 09's rule about denominators applies here.** A crop over a
    /// frame with no detected faces satisfies `faces_intact` trivially, and reporting that as
    /// a passed safety check would make a build with no face detector look like a build whose
    /// crops are provably safe. `docs/geometry-and-cropping.md` says so in the panel's words.
    #[must_use]
    pub const fn is_evidence(&self) -> bool {
        self.faces_checked > 0
    }

    /// How many candidates the filter refused in total.
    #[must_use]
    pub fn refused_total(&self) -> u32 {
        self.refused.iter().sum()
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why the frame was finished the way it was, as a closed set.
///
/// Section 9 gives DOC "document lens profile coverage and crop safety rules", which is only
/// a finishable job if the reasons are enumerable. `docs/geometry-and-cropping.md` is written
/// against [`GeometryCode::ALL`] and a test asserts every variant appears there.
///
/// **Eleven of these twenty-four describe something that did not happen.** That ratio is
/// higher than any phase before it, and it is the phase's own shape rather than an accident:
/// section 10.1 asks that seventy per cent of frames keep their framing, so on most
/// photographs the only thing there is to explain is a refusal.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum GeometryCode {
    /// Nothing needed doing. The reason a well-shot, well-profiled frame carries, so that
    /// every frame has at least one.
    #[default]
    Clean,
    // --- lens -------------------------------------------------------------------------
    /// The camera's own correction data was used.
    LensEmbedded,
    /// The bundled profile table matched this lens.
    LensProfiled,
    /// No profile existed, so the distortion was fitted from straight edges.
    LensEstimated,
    /// No profile existed and there were not enough straight edges to fit one.
    LensProfileMissing,
    /// Fringing was corrected on a measured profile.
    CaCorrected,
    /// Fringing was left alone because the profile was estimated rather than measured.
    CaWithheld,
    /// Vignetting was corrected before the creative operations.
    VignetteCorrected,
    // --- straightening ----------------------------------------------------------------
    /// The frame was levelled.
    Levelled,
    /// The tilt reads as a decision, so it was left alone.
    TiltIntentional,
    /// The horizon estimate was not confident enough to act on.
    HorizonUncertain,
    /// The tilt was below the smallest correction worth making.
    TiltNegligible,
    /// The rotation was reduced because the crop it implied was not safe.
    RotationReduced,
    /// The rotation was abandoned because no safe crop existed at any angle.
    RotationRefused,
    // --- keystone ---------------------------------------------------------------------
    /// Converging verticals were squared up.
    KeystoneApplied,
    /// The correction would have stretched an axis past the cap.
    KeystoneCapped,
    /// There were not enough strong verticals to fit a vanishing point.
    KeystoneNoVerticals,
    // --- cropping ---------------------------------------------------------------------
    /// A better framing was found and taken.
    CropImproved,
    /// The original framing was kept: nothing beat it by enough.
    CropKeptOriginal,
    /// A candidate was refused because it cut a face.
    CropCutsFace,
    /// A candidate was refused because it cut a primary identity's hands.
    CropCutsHands,
    /// A candidate was refused because it fell below the resolution floor.
    CropTooSmall,
    /// A candidate was refused because it removed content phase 11 asked to keep.
    CropLosesContent,
    /// An aspect variant was produced.
    VariantAdded,
}

impl GeometryCode {
    /// Every code.
    pub const ALL: [Self; 24] = [
        Self::Clean,
        Self::LensEmbedded,
        Self::LensProfiled,
        Self::LensEstimated,
        Self::LensProfileMissing,
        Self::CaCorrected,
        Self::CaWithheld,
        Self::VignetteCorrected,
        Self::Levelled,
        Self::TiltIntentional,
        Self::HorizonUncertain,
        Self::TiltNegligible,
        Self::RotationReduced,
        Self::RotationRefused,
        Self::KeystoneApplied,
        Self::KeystoneCapped,
        Self::KeystoneNoVerticals,
        Self::CropImproved,
        Self::CropKeptOriginal,
        Self::CropCutsFace,
        Self::CropCutsHands,
        Self::CropTooSmall,
        Self::CropLosesContent,
        Self::VariantAdded,
    ];

    /// How many there are.
    pub const COUNT: usize = 24;

    /// The four ways the safety filter refuses a candidate, in histogram order.
    ///
    /// Section 11's `geometry.crop_refused {reason_histogram}` is indexed by this array, and
    /// [`CropSafetyReport::refused`] is sized by it, so adding a fifth refusal is a change
    /// that fails to compile in three places rather than one that silently shifts a bucket.
    pub const REFUSALS: [Self; 4] = [
        Self::CropCutsFace,
        Self::CropCutsHands,
        Self::CropTooSmall,
        Self::CropLosesContent,
    ];

    /// How many refusal codes there are.
    pub const REFUSAL_COUNT: usize = 4;

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::LensEmbedded => "lens_embedded",
            Self::LensProfiled => "lens_profiled",
            Self::LensEstimated => "lens_estimated",
            Self::LensProfileMissing => "lens_profile_missing",
            Self::CaCorrected => "ca_corrected",
            Self::CaWithheld => "ca_withheld",
            Self::VignetteCorrected => "vignette_corrected",
            Self::Levelled => "levelled",
            Self::TiltIntentional => "tilt_intentional",
            Self::HorizonUncertain => "horizon_uncertain",
            Self::TiltNegligible => "tilt_negligible",
            Self::RotationReduced => "rotation_reduced",
            Self::RotationRefused => "rotation_refused",
            Self::KeystoneApplied => "keystone_applied",
            Self::KeystoneCapped => "keystone_capped",
            Self::KeystoneNoVerticals => "keystone_no_verticals",
            Self::CropImproved => "crop_improved",
            Self::CropKeptOriginal => "crop_kept_original",
            Self::CropCutsFace => "crop_cuts_face",
            Self::CropCutsHands => "crop_cuts_hands",
            Self::CropTooSmall => "crop_too_small",
            Self::CropLosesContent => "crop_loses_content",
            Self::VariantAdded => "variant_added",
        }
    }

    /// Parse the stored text. Unknown values read as [`GeometryCode::Clean`].
    #[must_use]
    pub fn from_str_or_clean(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|code| code.as_str() == text)
            .unwrap_or(Self::Clean)
    }

    /// The sentence a photographer reads.
    ///
    /// Stored as a code and rendered as a sentence, which is phase 09's rule: a stored
    /// sentence is copy a release can change, and a catalog full of English cannot be
    /// translated.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::Clean => "The frame needed no geometry work.",
            Self::LensEmbedded => "The camera's own lens corrections were used.",
            Self::LensProfiled => "A measured profile for this lens corrected the optics.",
            Self::LensEstimated => {
                "There is no profile for this lens, so the distortion was estimated from \
                 straight edges in the frame."
            }
            Self::LensProfileMissing => {
                "There is no profile for this lens and not enough straight lines to estimate \
                 one, so the optics were left alone."
            }
            Self::CaCorrected => "Colour fringing along high-contrast edges was removed.",
            Self::CaWithheld => {
                "Fringing was left alone: the lens correction was estimated rather than \
                 measured, and an estimated one can invent fringing of its own."
            }
            Self::VignetteCorrected => "Darkening at the corners was evened out.",
            Self::Levelled => "The horizon was levelled.",
            Self::TiltIntentional => "The tilt reads as a decision, so it was left as shot.",
            Self::HorizonUncertain => {
                "There was no reliable horizon to measure, so nothing was rotated."
            }
            Self::TiltNegligible => "The frame was already level.",
            Self::RotationReduced => {
                "The frame was levelled less than fully: turning it further would have cropped \
                 into somebody."
            }
            Self::RotationRefused => {
                "The frame was left as shot: there was no way to level it without cropping \
                 into somebody."
            }
            Self::KeystoneApplied => "Converging vertical lines were squared up.",
            Self::KeystoneCapped => {
                "The perspective correction was left off: squaring the verticals would have \
                 stretched the frame too far."
            }
            Self::KeystoneNoVerticals => {
                "There were not enough strong vertical lines to correct the perspective from."
            }
            Self::CropImproved => "A tighter framing improved the composition, so it was taken.",
            Self::CropKeptOriginal => {
                "The framing as shot was kept: nothing AURA tried was clearly better."
            }
            Self::CropCutsFace => "A tighter framing was rejected because it cut somebody's face.",
            Self::CropCutsHands => {
                "A tighter framing was rejected because it cut the couple's hands."
            }
            Self::CropTooSmall => {
                "A tighter framing was rejected because it threw away too much resolution."
            }
            Self::CropLosesContent => {
                "A tighter framing was rejected because it removed something the frame is about."
            }
            Self::VariantAdded => "An extra crop was prepared for album or social delivery.",
        }
    }

    /// True when this code describes something the product declined to do.
    ///
    /// The panel groups by this. Eleven of the twenty-four, which is the highest ratio of any
    /// phase, and section 12's first failure mode is what happens when a product that mostly
    /// declines cannot say so.
    #[must_use]
    pub const fn is_restraint(self) -> bool {
        matches!(
            self,
            Self::LensProfileMissing
                | Self::CaWithheld
                | Self::TiltIntentional
                | Self::HorizonUncertain
                | Self::TiltNegligible
                | Self::RotationReduced
                | Self::RotationRefused
                | Self::KeystoneCapped
                | Self::KeystoneNoVerticals
                | Self::CropKeptOriginal
                | Self::CropLosesContent
        )
    }

    /// True when this code is one of the four safety refusals.
    #[must_use]
    pub const fn is_refusal(self) -> bool {
        matches!(
            self,
            Self::CropCutsFace | Self::CropCutsHands | Self::CropTooSmall | Self::CropLosesContent
        )
    }

    /// This code's index in [`GeometryCode::REFUSALS`], when it is one.
    #[must_use]
    pub const fn refusal_index(self) -> Option<usize> {
        match self {
            Self::CropCutsFace => Some(0),
            Self::CropCutsHands => Some(1),
            Self::CropTooSmall => Some(2),
            Self::CropLosesContent => Some(3),
            _ => None,
        }
    }
}

impl fmt::Display for GeometryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One reason, with the pixels behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryReason {
    /// Which reason this is.
    pub code: GeometryCode,
    /// The sentence a photographer reads.
    pub text: String,
    /// How much this moved the confidence. Negative for a doubt.
    pub weight: f32,
    /// The pixels to show, when there are any more specific than the whole frame.
    ///
    /// For a refusal this is the rectangle that would have been cut - a face, a pair of
    /// hands - rather than the crop that would have cut it, because the photographer's
    /// question is "what would I have lost".
    #[serde(default)]
    pub evidence: Option<CropRect>,
}

impl GeometryReason {
    /// A reason about the whole frame.
    #[must_use]
    pub fn frame(code: GeometryCode, text: impl Into<String>, weight: f32) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: None,
        }
    }

    /// A reason about one region.
    #[must_use]
    pub fn at(code: GeometryCode, text: impl Into<String>, weight: f32, evidence: CropRect) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: Some(evidence),
        }
    }

    /// A reason carrying only the code's own sentence.
    #[must_use]
    pub fn plain(code: GeometryCode, weight: f32) -> Self {
        Self::frame(code, code.user_text(), weight)
    }
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// Everything phase 23 decided about one photograph's frame.
///
/// PHASE-23 section 5's frozen shape, plus the scene, the three version columns and the
/// override flag every phase since 06 carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryPlan {
    /// The photograph.
    pub image_id: ImageId,
    /// What the optics did and what was undone about it.
    pub lens: LensCorrection,
    /// Rotation in degrees, positive clockwise. Zero when nothing was levelled.
    pub rotate_deg: f32,
    /// How sure the rotation is, `0..1`. Compared against [`STRAIGHTEN_ACT_AT`].
    pub rotate_conf: f32,
    /// The perspective correction, when one survived the cap.
    #[serde(default)]
    pub keystone: Option<Keystone>,
    /// Every crop this frame carries. Never empty; `crops[0]` is always the original.
    pub crops: Vec<CropVariant>,
    /// Which entry is delivered. Zero when the original framing won.
    pub primary_crop: usize,
    /// What the safety filter checked and found.
    pub safety: CropSafetyReport,
    /// Why, worst first.
    pub reasons: Vec<GeometryReason>,
    /// How sure the whole plan is, `0..1`.
    pub confidence: f32,
    /// Which scene the bands were conditioned on. Invariant 7.
    pub scene: SceneId,
    /// Which lens profile table produced the corrections.
    pub profile_ver: u16,
    /// Which arithmetic produced the rotation, the keystone and the crop search.
    pub analysis_ver: u16,
    /// Which `crop_rules.toml` the safety margins and aspects came from.
    pub rules_ver: u16,
    /// True when the photographer has set the framing themselves.
    ///
    /// Checked **inside** the statement that would overwrite the row, which is the ninth
    /// migration to write that rule and the same window it closes every time.
    pub user_edited: bool,
}

impl GeometryPlan {
    /// The most reasons one plan carries.
    ///
    /// Six, as phase 11's is. A panel that lists nine reasons for one photograph is a panel
    /// nobody reads to the end, and the seventh reason is always a variant note.
    pub const MAX_REASONS: usize = 6;

    /// A plan that changes nothing, carrying the original framing and one reason.
    ///
    /// The only constructor, so that [`GeometryPlan::crops`] can never be empty and
    /// `crops[0]` can never be anything but the original. Section 13's "original framing is
    /// always one click away" is enforced here rather than in the panel.
    #[must_use]
    pub fn new(image_id: ImageId, scene: SceneId) -> Self {
        Self {
            image_id,
            lens: LensCorrection::default(),
            rotate_deg: 0.0,
            rotate_conf: 0.0,
            keystone: None,
            crops: vec![CropVariant::original()],
            primary_crop: 0,
            safety: CropSafetyReport {
                faces_intact: true,
                resolution_ok: true,
                content_kept: true,
                faces_checked: 0,
                refused: [0; GeometryCode::REFUSAL_COUNT],
            },
            reasons: vec![GeometryReason::plain(GeometryCode::Clean, 0.0)],
            confidence: 1.0,
            scene,
            profile_ver: 0,
            analysis_ver: 0,
            rules_ver: 0,
            user_edited: false,
        }
    }

    /// The crop that will be delivered.
    ///
    /// Falls back to the original rather than to `None`: `primary_crop` is an index, an index
    /// read back from a catalog written by another build can be out of range, and a delivery
    /// pipeline that received `None` here would have to invent a rectangle.
    #[must_use]
    pub fn primary(&self) -> CropVariant {
        self.crops
            .get(self.primary_crop)
            .copied()
            .unwrap_or_else(CropVariant::original)
    }

    /// The variant for one purpose, when the plan carries one.
    #[must_use]
    pub fn for_purpose(&self, purpose: CropPurpose) -> Option<CropVariant> {
        self.crops
            .iter()
            .find(|variant| variant.purpose == purpose)
            .copied()
    }

    /// True when this plan would move a pixel.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.lens.is_identity()
            && self.rotate_deg.abs() < f32::EPSILON
            && self.keystone.is_none()
            && self.primary().is_full_frame()
    }

    /// True when the frame was delivered as shot.
    ///
    /// The numerator of section 10.1's "at least seventy per cent keep their original
    /// framing". Deliberately about the *crop* rather than about the whole plan: a levelled
    /// frame has not kept its framing in the sense the resample cares about, and it has in the
    /// sense the photographer means.
    #[must_use]
    pub fn kept_original_framing(&self) -> bool {
        self.primary_crop == 0 || self.primary().is_full_frame()
    }

    /// Every reason describing something the product declined to do.
    #[must_use]
    pub fn restraints(&self) -> Vec<&GeometryReason> {
        self.reasons
            .iter()
            .filter(|reason| reason.code.is_restraint())
            .collect()
    }

    /// True when these versions match the ones a caller is working against.
    ///
    /// Three columns because they invalidate three different things: `profile_ver` the lens
    /// corrections, `analysis_ver` the rotation, the keystone and the crop search, and
    /// `rules_ver` every safety margin those rectangles were checked against.
    /// `AURA-ML-5090` is raised when a comparison would cross any of them.
    #[must_use]
    pub const fn matches_versions(&self, profile: u16, analysis: u16, rules: u16) -> bool {
        self.profile_ver == profile && self.analysis_ver == analysis && self.rules_ver == rules
    }

    /// The first of this phase's own guarantees this plan breaks, if it breaks one.
    ///
    /// Asked by the solver, the store, the IPC layer and the evaluation harness, so that none
    /// of them can disagree about what a sound plan is. The rule phase 19 wrote for
    /// `LocalLightPlan::broken_guarantee`, in a phase where four of the six clauses are the
    /// safety filter restated as a post-condition - because a filter that runs before the
    /// objective is only a guarantee if nothing downstream can put a rejected rectangle back.
    ///
    /// **A refused plan is stored as no plan rather than as a weak one.** A geometry plan that
    /// cannot be trusted is a delivered JPEG with somebody's hands missing from it.
    #[must_use]
    pub fn broken_guarantee(&self) -> Option<String> {
        if self.crops.is_empty() {
            return Some("a plan carries no crop at all".into());
        }
        match self.crops.first() {
            Some(first) if first.purpose == CropPurpose::Original && first.is_full_frame() => {}
            _ => return Some("the original framing is not the first crop".into()),
        }
        if self.crops.len() > MAX_VARIANTS {
            return Some(format!("a plan carries more than {MAX_VARIANTS} crops"));
        }
        if self.primary_crop >= self.crops.len() {
            return Some("the primary crop is not one of the crops".into());
        }
        for variant in &self.crops {
            if !variant.safe {
                return Some(format!(
                    "the {} crop was kept after failing the safety filter",
                    variant.purpose
                ));
            }
            if variant.rect.is_empty() {
                return Some(format!("the {} crop is degenerate", variant.purpose));
            }
        }
        if self.rotate_deg.abs() >= f32::EPSILON {
            if self.rotate_conf < STRAIGHTEN_ACT_AT {
                return Some(format!(
                    "the frame was rotated at confidence {:.2}, below {STRAIGHTEN_ACT_AT:.2}",
                    self.rotate_conf
                ));
            }
            if self.rotate_deg.abs() < MIN_ROTATE_DEG || self.rotate_deg.abs() > MAX_ROTATE_DEG {
                return Some(format!(
                    "the frame was rotated {:.2} degrees, outside the {MIN_ROTATE_DEG:.2} to \
                     {MAX_ROTATE_DEG:.2} band",
                    self.rotate_deg
                ));
            }
        }
        if let Some(keystone) = self.keystone {
            if keystone.stretch > MAX_STRETCH {
                return Some(format!(
                    "a keystone stretching {:.3} survived the {MAX_STRETCH:.2} cap",
                    keystone.stretch
                ));
            }
        }
        if self.reasons.is_empty() {
            return Some("a plan carries no reason".into());
        }
        if self.reasons.len() > Self::MAX_REASONS + CropPurpose::COUNT {
            return Some("a plan carries more reasons than a panel renders".into());
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Some(format!("the confidence {:.3} is outside 0..1", self.confidence));
        }
        None
    }
}

// ---------------------------------------------------------------------------
// The outline
// ---------------------------------------------------------------------------

/// What a project's geometry pass covered and found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GeometryOutline {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs with a plan.
    pub planned: u32,
    /// Fraction of the project with a plan, `0..1`.
    ///
    /// The denominator is **every photograph**, as phases 09 to 19's are.
    pub coverage: f32,
    /// Fraction of planned frames that kept their framing, `0..1`.
    ///
    /// Section 10.1's conservatism gate, as a number a photographer can see. **Above** 0.70
    /// is the passing direction, which is the only outline number in the product where more
    /// restraint is a better result.
    pub kept_original: f32,
    /// Fraction of planned frames whose lens was corrected from a measured profile, `0..1`.
    ///
    /// The second number and the one that says whether the bundled table is doing its job.
    pub profile_covered: f32,
    /// How many frames were levelled.
    pub levelled: u32,
    /// Mean absolute rotation over the levelled frames, in degrees.
    pub mean_rotate_deg: f32,
    /// How many frames were keystoned.
    pub keystoned: u32,
    /// How many crop variants were produced, in [`CropPurpose::ALL`] order.
    pub variant_histogram: [u32; CropPurpose::COUNT],
    /// How many candidates the safety filter refused, in [`GeometryCode::REFUSALS`] order.
    ///
    /// Section 11's `geometry.crop_refused {reason_histogram}`.
    pub refused_histogram: [u32; GeometryCode::REFUSAL_COUNT],
    /// Lens ids with no profile, most frequent first.
    ///
    /// Section 11's `geometry.lens_profile_missing {lens_id}`, aggregated. Bounded to the
    /// twenty most frequent, because a wedding shot on rented glass can carry a long tail and
    /// the panel shows a list.
    pub missing_profiles: Vec<String>,
    /// Frames whose plan is worth a photographer's attention.
    pub needs_review: u32,
    /// Frames whose framing the photographer set themselves.
    pub user_edited: u32,
    /// Which lens profile table.
    pub profile_ver: u16,
    /// Which arithmetic.
    pub analysis_ver: u16,
    /// Which rules file.
    pub rules_ver: u16,
}

impl GeometryOutline {
    /// The most missing lens ids the outline carries.
    pub const MAX_MISSING: usize = 20;

    /// True when nothing has been planned.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.planned == 0
    }

    /// How many candidates were refused in total.
    #[must_use]
    pub fn refused_total(&self) -> u32 {
        self.refused_histogram.iter().sum()
    }
}

// ---------------------------------------------------------------------------
// The override
// ---------------------------------------------------------------------------

/// The framing a photographer set themselves.
///
/// **There is no field here for a lens correction.** A photographer disagreeing with a
/// distortion model is disagreeing with a measurement rather than with a judgement, and the
/// route for that is the recipe's own `lens` block through phase 14's merge - which is
/// already protected by `user_edited_fields`. An override path that could also write the
/// correction would be a second way to set the same value, and the two would disagree.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeometryOverride {
    /// The photograph.
    pub image_id: ImageId,
    /// The rectangle they chose, normalised to the corrected frame.
    pub rect: CropRect,
    /// The angle they chose, in degrees.
    pub rotate_deg: f32,
    /// Which aspect they were working in.
    pub aspect: Aspect,
}

impl GeometryOverride {
    /// Reverting to the frame as shot.
    ///
    /// Section 13's "original framing is always one click away", as a value rather than as a
    /// code path: reverting is recording an override equal to the original, which means it
    /// survives a re-analysis exactly as any other override does. A revert implemented as
    /// *clearing* the row would be a revert the next pass undoes.
    #[must_use]
    pub fn revert(image_id: ImageId) -> Self {
        Self {
            image_id,
            rect: CropRect::FULL,
            rotate_deg: 0.0,
            aspect: Aspect::Original,
        }
    }

    /// Why this override cannot be applied, if it cannot.
    ///
    /// The predicate `aura_geometry::guard` turns into `AURA-ML-5091`. `aura-core` owns the
    /// shape and the predicate; the implementing crate owns the error registry, which is the
    /// split every phase since 09 has kept.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if self.rect.is_empty() {
            return Some("the rectangle covers no area".into());
        }
        if self.rect.x < 0.0
            || self.rect.y < 0.0
            || self.rect.x + self.rect.w > 1.0 + 1e-4
            || self.rect.y + self.rect.h > 1.0 + 1e-4
        {
            return Some("the rectangle leaves the frame".into());
        }
        if !self.rotate_deg.is_finite() || self.rotate_deg.abs() > 45.0 {
            return Some(format!(
                "the angle {:.2} is outside -45..45",
                self.rotate_deg
            ));
        }
        None
    }

    /// True when this override restores the frame as shot.
    #[must_use]
    pub fn is_revert(&self) -> bool {
        self.rotate_deg.abs() < f32::EPSILON
            && self.rect.x.abs() < 1e-4
            && self.rect.y.abs() < 1e-4
            && (self.rect.w - 1.0).abs() < 1e-4
            && (self.rect.h - 1.0).abs() < 1e-4
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask how a photograph's frame was finished.
///
/// Frozen. Implemented by `aura_geometry::Geometry`, and the only route from any later phase
/// to a lens correction, a rotation, a keystone or a crop.
///
/// The rule phase 05 wrote for `SimilarityIndex` and every phase since has written for its
/// own service, a sixteenth time: **no phase may keep its own idea of where the frame ends.**
/// Phase 27 checks these crops, phase 29 lays albums out of the variants and phase 30 exports
/// them; two answers to "what is this photograph's frame" is an album page cropped from a
/// rectangle the gallery never delivered.
pub trait GeometryService: Send + Sync + fmt::Debug {
    /// What a project's pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<GeometryOutline>;

    /// One photograph's plan, or `None` when it has not been planned.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plan cannot be read.
    fn of_image(&self, image: ImageId) -> AuraResult<Option<GeometryPlan>>;

    /// The rectangle one purpose is delivered at, or `None` when the plan has no such variant.
    ///
    /// Separate from [`GeometryService::of_image`] because phase 29 wants this and nothing
    /// else, and decoding a whole plan to take one rectangle out of it is how an album
    /// layout ends up parsing reasons it never renders.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plan cannot be read.
    fn variant(&self, image: ImageId, purpose: CropPurpose) -> AuraResult<Option<CropVariant>>;

    /// Frames whose geometry is worth a photographer's attention, weakest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>>;

    /// Record the framing the photographer chose.
    ///
    /// Sets `user_edited`, which is not undone by a re-analysis: the check is inside the
    /// statement that would overwrite the row, exactly as `identities.user_locked`,
    /// `segments.user_locked`, `moments.user_locked`, `image_integrity.user_reviewed` and
    /// `masks.user_edited` are.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5091` when the photograph has no plan, when the rectangle is degenerate, or
    /// when the angle is outside `-45..45`.
    fn set_framing(&self, over: GeometryOverride) -> Result<(), AuraError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> ImageId {
        ImageId::new()
    }

    #[test]
    fn a_new_plan_always_carries_the_original_framing_at_index_zero() {
        let plan = GeometryPlan::new(image(), SceneId::Unknown);
        assert_eq!(plan.crops.len(), 1);
        let first = plan.crops.first().copied().unwrap_or_else(CropVariant::original);
        assert_eq!(first.purpose, CropPurpose::Original);
        assert!(first.is_full_frame());
        assert!(plan.kept_original_framing());
        assert!(plan.is_identity());
    }

    #[test]
    fn the_primary_falls_back_to_the_original_rather_than_to_nothing() {
        let mut plan = GeometryPlan::new(image(), SceneId::Unknown);
        plan.primary_crop = 97;
        assert!(plan.primary().is_full_frame());
    }

    #[test]
    fn only_the_primary_crop_must_earn_its_place() {
        assert!(CropPurpose::Primary.must_improve());
        for purpose in [CropPurpose::Album, CropPurpose::Social, CropPurpose::Wide] {
            assert!(!purpose.must_improve(), "{purpose} must not need a margin");
        }
    }

    #[test]
    fn a_keystone_past_the_cap_is_refused_rather_than_reduced() {
        assert!(Keystone::new(20.0, 0.0, 1.1, MAX_STRETCH + 0.01, 5).is_err());
        assert!(Keystone::new(20.0, 0.0, 1.1, 1.10, 2).is_err());
        assert!(Keystone::new(20.0, 0.0, 0.9, 1.10, 5).is_err());
        assert!(Keystone::new(20.0, 0.0, 1.1, 1.10, 5).is_ok());
    }

    #[test]
    fn an_unchecked_face_rule_is_not_a_passed_face_rule() {
        let report = CropSafetyReport {
            faces_intact: true,
            resolution_ok: true,
            content_kept: true,
            faces_checked: 0,
            refused: [0; GeometryCode::REFUSAL_COUNT],
        };
        assert!(report.all_clear());
        assert!(!report.is_evidence());
    }

    #[test]
    fn every_refusal_code_has_a_histogram_slot_and_no_other_code_does() {
        for code in GeometryCode::ALL {
            assert_eq!(
                code.is_refusal(),
                code.refusal_index().is_some(),
                "{code} disagrees with itself about being a refusal"
            );
        }
        let mut seen = [false; GeometryCode::REFUSAL_COUNT];
        for code in GeometryCode::REFUSALS {
            let index = code.refusal_index().unwrap_or(usize::MAX);
            assert!(index < GeometryCode::REFUSAL_COUNT);
            assert!(!seen[index], "{code} shares a slot");
            seen[index] = true;
        }
        assert!(seen.iter().all(|hit| *hit));
    }

    #[test]
    fn every_code_round_trips_through_its_stored_text_and_has_a_sentence() {
        for code in GeometryCode::ALL {
            assert_eq!(GeometryCode::from_str_or_clean(code.as_str()), code);
            assert!(!code.user_text().is_empty(), "{code} has no sentence");
        }
        assert_eq!(GeometryCode::ALL.len(), GeometryCode::COUNT);
    }

    #[test]
    fn reverting_is_a_value_rather_than_an_absence() {
        let over = GeometryOverride::revert(image());
        assert!(over.is_revert());
        assert_eq!(over.aspect, Aspect::Original);
    }

    #[test]
    fn the_long_edge_fraction_follows_the_crop_rather_than_the_file() {
        // A 3:2 landscape frame. A 4:5 crop out of it keeps the full height and 0.53 of the
        // width, so its long edge is the height - and the height was never the long edge of
        // the frame it came from.
        let variant = CropVariant {
            aspect: Aspect::FourFive,
            rect: CropRect {
                x: 0.23,
                y: 0.0,
                w: 0.533,
                h: 1.0,
            },
            purpose: CropPurpose::Album,
            score: 0.5,
            safe: true,
        };
        let frac = variant.long_edge_fraction(1.5);
        assert!((frac - (1.0 / 1.5)).abs() < 1e-3, "{frac}");
    }

    #[test]
    fn every_aspect_and_source_round_trips() {
        for aspect in Aspect::ALL {
            assert_eq!(Aspect::from_str_or_original(aspect.as_str()), aspect);
        }
        for source in LensSource::ALL {
            assert_eq!(LensSource::from_str_or_none(source.as_str()), source);
        }
        for purpose in CropPurpose::ALL {
            assert_eq!(CropPurpose::from_str_or_original(purpose.as_str()), purpose);
        }
    }

    #[test]
    fn ca_is_withheld_on_an_estimated_profile() {
        assert!(LensSource::Embedded.is_measured());
        assert!(LensSource::Profile.is_measured());
        assert!(!LensSource::Estimated.is_measured());
        assert!(LensSource::Estimated.is_corrected());
    }
}
