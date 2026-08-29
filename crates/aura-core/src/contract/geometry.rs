//! FROZEN CONTRACT. Finishing the frame: the optics corrected, the world levelled, and a crop
//! that improves a photograph without ever cutting off what matters.
//!
//! PHASE-23 section 5 freezes [`GeometryPlan`] before any solver exists. The file is in
//! `aura-core` for the reason [`crate::contract::restore`], [`crate::contract::micro`],
//! [`crate::contract::retouch`] and [`crate::contract::local`] are: the phases that consume a
//! geometry decision are 27 (QC, which has to be able to say why a frame is tilted or why a hand
//! is missing), 29 (album layout, which picks between the aspect variants this phase generates)
//! and 30 (delivery, which exports one of them), and none of them needs the lens tables, the
//! keystone solve or the crop search.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **This is the phase where automation is most dangerous and least visible.** Phase 22 removed
//! information from a photograph and a photographer can at least see the smear. A crop removes
//! information *and the evidence that it was removed*: a delivered frame with somebody's hand
//! outside it looks like a frame that was shot that way, and there is nothing in the pixels that
//! says otherwise. Section 1 puts it as "smart crop is where automation is most dangerous".
//!
//! So every crop in this contract carries three things a lesser shape would leave out: the
//! [`CropSafetyReport`] that says what was checked, the [`CropVariant::safe`] bit that says
//! whether it passed, and [`GeometryPlan::primary_crop`] pointing at one entry of a list whose
//! **first element is always the original framing**. A plan that decided nothing and a plan
//! nobody ran are therefore different rows rather than the same absence.
//!
//! ## The second thing: this phase mostly does nothing, on purpose
//!
//! Section 10.1: "Most frames (>= 70 %) keep their original framing - the system is conservative
//! by design." [`MIN_IMPROVEMENT`] is that sentence as a number, and it is compared against an
//! objective this phase measures itself rather than against phase 11's composite - see
//! [`CropVariant::score`] for why. A recomposition that is *slightly* better than what a
//! photographer framed is not better enough to overrule them.
//!
//! ## The third thing: there is no fill anywhere in here
//!
//! Rotating a frame opens four triangular corners. Section 2.2 puts content-aware fill in phase
//! 24, so this contract has no field that could carry a synthesised region, a source frame or a
//! fill method: the corners are removed by [`rotation_crop`], which computes the largest
//! axis-aligned rectangle that stays inside the rotated frame, and if that rectangle would breach
//! a safety rule the **rotation is reduced or abandoned** rather than the corners invented.
//! `crates/aura-core/tests/geometry_contract.rs` asserts the absence.
//!
//! ## What this contract cannot express
//!
//! There is no scale factor, no output resolution, no upscaling and no way to name a second
//! photograph. A crop is a rectangle inside one frame; it can only ever remove.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::composition::Box2;
use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{IdentityId, ProjectId};
use crate::contract::scene::SceneId;

pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// Bands, ceilings and floors
// ---------------------------------------------------------------------------

/// The smallest tilt worth correcting, in degrees.
///
/// Section 6.2: "the correction is between 0.2 and 8 degrees". Below two tenths of a degree the
/// correction is smaller than the estimator's own error - phase 11's horizon gate is 0.4 degrees
/// of angle error at section 10.1 - so a rotation here would be resampling a photograph in order
/// to move it by less than the amount nobody can measure. Every rotation costs a crop, so a
/// rotation that changes nothing costs pixels for nothing.
pub const ROTATE_MIN_DEG: f32 = 0.2;

/// The largest tilt this phase will correct, in degrees.
///
/// Section 6.2: "larger tilts are treated as intentional and left alone". Eight degrees is well
/// past a horizon anybody levelled by eye and well short of a dutch angle, and the rule is a
/// *band* rather than a ceiling for a reason: a photograph twenty degrees off level is not a
/// mistake somebody made twenty degrees of, it is a decision. Straightening it is the failure
/// mode section 12 lists second.
pub const ROTATE_MAX_DEG: f32 = 8.0;

/// The horizon confidence below which nothing is rotated, `0..1`.
///
/// Section 6.2: "Rotate only when Phase 11 horizon confidence >= 0.7".
///
/// **Deliberately above phase 11's own [`crate::contract::composition::HORIZON_ACT_AT`] of
/// 0.60.** The two numbers answer two questions. Phase 11 decides what is worth *reporting* - a
/// tilt it is sure enough of to put in a panel and to hand across in a
/// [`crate::contract::composition::CropHint`]. This phase decides what is worth *acting on*, and
/// acting costs a resample and a crop. A frame between the two is one where AURA says the horizon
/// looks off and declines to move it, which is the honest answer and not a gap.
pub const ROTATE_ACT_AT: f32 = 0.70;

/// The largest ratio between the two axis scales a keystone correction may introduce.
///
/// Section 2.1: "never exceed a documented stretch factor". Twelve per cent. A keystone
/// correction is a projective warp, and undoing convergence means stretching the far end of the
/// frame toward the near end; past about a tenth the faces at the top of a photograph of a church
/// are visibly taller than the faces at the bottom, which is a worse defect than the converging
/// wall it was fixing.
///
/// The number is a *ratio between the extremes of the warp*, not a percentage on the recipe's
/// `-100..100` slider, because the slider's meaning depends on the frame's aspect ratio and the
/// defect does not. [`Keystone::stretch`] carries the measured value and
/// [`Keystone::within_cap`] is the check.
pub const MAX_STRETCH: f32 = 1.12;

/// The smallest vertical convergence worth correcting, `0..1`.
///
/// Phase 11 measures the spread of the vertical family and sets
/// [`crate::contract::composition::CompositionFlags::VERTICALS_CONVERGING`] from it. Below this
/// the verticals are as parallel as a hand-held frame gets, and a keystone correction would be
/// stretching a photograph to fix an error smaller than the measurement.
pub const KEYSTONE_ACT_AT: f32 = 0.35;

/// The smallest share of the frame that must be architectural verticals before a keystone runs.
///
/// Section 6.2: "limited to frames with strong architectural verticals". A wedding is mostly
/// people, and the vertical family in a photograph of six guests is six people rather than a
/// building. Correcting *that* leans everybody outward.
pub const KEYSTONE_MIN_VERTICAL_SHARE: f32 = 0.08;

/// The smallest long edge a crop may keep, as a fraction of the original's long edge.
///
/// Section 6.3: "resolution >= 60 % of the original long edge".
///
/// **The floor is on the long edge rather than on the area**, and the difference matters. A 16:9
/// crop of a 4:5 frame keeps 80 % of the long edge and 45 % of the area; an area floor at 60 %
/// would refuse the 16:9 variant of every portrait frame in the wedding, which is a floor that
/// forbids a feature section 2.1 requires. A long-edge floor is a statement about how large the
/// delivered file can be printed, which is the thing a photographer actually cares about - and
/// the two measures agree on a square crop of a landscape frame, which is why the example that
/// separates them is a wide crop of a tall one.
pub const MIN_LONG_EDGE_FRACTION: f32 = 0.60;

/// How much better a proposed crop must score before it replaces what was shot, `0..1`.
///
/// Section 6.3's "score improvement gate", and section 10.1's "most frames (>= 70 %) keep their
/// original framing" is what it is tuned to produce.
///
/// Six hundredths on an objective whose whole range is `0..1`. Set against the *instrument*
/// rather than against taste, which is phase 22's lesson written down: the four terms of
/// [`CropVariant::score`] are each bounded in `0..1` and the objective moves by about two
/// hundredths for a crop that shifts a subject by one per cent of the frame, so a margin below
/// about four hundredths would fire on rounding and a margin above about a tenth would never
/// fire at all. A threshold nothing can meet and a threshold everything meets are the same bug.
pub const MIN_IMPROVEMENT: f32 = 0.06;

/// How far inside a crop's edge a protected region must sit, as a fraction of the frame.
///
/// One per cent. A face whose boundary is exactly on the crop's boundary is a face with the
/// resampler's own filter kernel hanging off the edge of the photograph, and at export that is a
/// visibly soft or clipped rim on somebody's ear. The margin is what makes "fully inside" mean
/// fully inside after the pixels have been through a filter rather than in the arithmetic.
pub const SAFETY_MARGIN: f32 = 0.01;

/// The most crop variants one plan may carry.
///
/// One original plus the four aspects section 2.1 names: 4:5, 5:4, 1:1 and 16:9. The list is
/// bounded in the contract rather than by the caller because [`GeometryPlan::primary_crop`] is an
/// index into it, and an unbounded list behind an index is how a plan ends up pointing at
/// something that is not there.
pub const MAX_VARIANTS: usize = 5;

/// The most reason codes one plan carries.
///
/// Section 5 has no limit; every phase since 09 has one, for the same reason: a panel renders
/// them and a row stores them, and a plan carrying forty reasons is a plan nobody reads.
pub const MAX_REASONS: usize = 12;

/// The largest lens vignette correction, `0..100`.
///
/// Full correction of a profiled lens. Written here rather than left to the recipe's clamp
/// because [`LensCorrection::vignette`] is what this phase *decides* and the recipe field is
/// where it lands, and a decision bounded only at its destination is a decision nobody can audit
/// before it gets there.
pub const MAX_VIGNETTE: u8 = 100;

// ---------------------------------------------------------------------------
// Where a lens correction came from
// ---------------------------------------------------------------------------

/// Where the numbers behind a lens correction came from.
///
/// Section 6.1's preference order, as a type: "Prefer embedded correction data, then the bundled
/// profile database keyed by lens id and focal length, then geometric estimation from straight
/// edges."
///
/// It is carried on every plan because the three are not interchangeable. Embedded data is what
/// the manufacturer measured; a database row is what somebody else measured; an estimate is what
/// AURA guessed from this one photograph, and a photographer who sees a distortion correction
/// they disagree with needs to know which of the three they are arguing with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LensSource {
    /// The camera wrote correction data into the file. The best answer available.
    Embedded,
    /// A row in the bundled profile database, matched on lens id and focal length.
    Database,
    /// Estimated from long straight edges in this photograph. The weakest answer, and the
    /// only one that can be wrong about a lens that is behaving correctly.
    Estimated,
    /// Nothing was found and nothing was corrected. The default, and never a failure.
    #[default]
    None,
}

impl LensSource {
    /// Every variant, strongest evidence first.
    pub const ALL: [Self; 4] = [Self::Embedded, Self::Database, Self::Estimated, Self::None];

    /// Stable slug for the wire, the schema and telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::Database => "database",
            Self::Estimated => "estimated",
            Self::None => "none",
        }
    }

    /// Parse a stored slug. Anything unknown reads as [`LensSource::None`].
    ///
    /// Unknown reads as "nothing was corrected" rather than as an error, because the failure
    /// mode of the alternative is a project that will not open after a downgrade.
    #[must_use]
    pub fn from_str_or_none(text: &str) -> Self {
        match text {
            "embedded" => Self::Embedded,
            "database" => Self::Database,
            "estimated" => Self::Estimated,
            _ => Self::None,
        }
    }

    /// True when a correction was actually available from this source.
    #[must_use]
    pub const fn is_available(self) -> bool {
        !matches!(self, Self::None)
    }

    /// True when the numbers were measured by somebody rather than inferred from one frame.
    ///
    /// The distinction the panel renders and the one section 6.1's fallback chain is ordered by.
    #[must_use]
    pub const fn is_measured(self) -> bool {
        matches!(self, Self::Embedded | Self::Database)
    }
}

impl fmt::Display for LensSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a lens correction does to one photograph.
///
/// Section 5's `{ distortion, vignette, ca, profile_id, source }` exactly, with the field types
/// chosen to match what the recipe's [`crate::contract::restore`]-era `Lens` block can carry:
/// distortion and CA are switches because a profile either models the lens or it does not, and
/// vignette is a percentage because a photographer legitimately wants half of it sometimes -
/// darkened corners are a look as often as they are a defect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LensCorrection {
    /// Correct geometric distortion.
    pub distortion: bool,
    /// Vignette correction, `0..=100`, where 100 is full correction.
    pub vignette: u8,
    /// Correct lateral chromatic aberration.
    pub ca: bool,
    /// Which profile the numbers came from, when they came from one.
    ///
    /// `None` for [`LensSource::Embedded`] - the file carried the numbers and there is no row to
    /// name - and for [`LensSource::None`].
    #[serde(default)]
    pub profile_id: Option<String>,
    /// Where the numbers came from.
    pub source: LensSource,
}

impl LensCorrection {
    /// No corrections at all. What an unknown lens gets.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            distortion: false,
            vignette: 0,
            ca: false,
            profile_id: None,
            source: LensSource::None,
        }
    }

    /// True when this correction moves no pixel.
    #[must_use]
    pub const fn is_identity(&self) -> bool {
        !self.distortion && self.vignette == 0 && !self.ca
    }

    /// Clamp the vignette into range.
    ///
    /// Clamped rather than refused, for the reason `Recipe::clamped` gives: refusing to render a
    /// wedding because one number was 101 instead of 100 is the wrong trade.
    #[must_use]
    pub fn clamped(mut self) -> Self {
        self.vignette = self.vignette.min(MAX_VIGNETTE);
        if self.source == LensSource::None {
            self.distortion = false;
            self.ca = false;
            self.vignette = 0;
            self.profile_id = None;
        }
        self
    }
}

impl Default for LensCorrection {
    fn default() -> Self {
        Self::none()
    }
}

// ---------------------------------------------------------------------------
// Keystone
// ---------------------------------------------------------------------------

/// A perspective correction, with the stretch it costs measured rather than assumed.
///
/// Section 5's `Option<Keystone>` with the comment "stretch-capped". The cap is
/// [`MAX_STRETCH`] and it is checked against [`Keystone::stretch`], which is a *measurement of
/// the warp* rather than a function of the two slider values - a keystone at vertical 40 on a
/// 16:9 frame and the same 40 on a 4:5 frame stretch by different amounts, and a cap on the
/// slider would be a cap on two different things.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Keystone {
    /// Vertical keystone, `-100.0..=100.0`, matching the recipe's `Perspective::vertical`.
    ///
    /// Positive pulls the top of the frame outward, which is what undoes the convergence of a
    /// camera pointed up at a building.
    pub vertical: f32,
    /// Horizontal keystone, `-100.0..=100.0`.
    pub horizontal: f32,
    /// The measured ratio between the largest and smallest local scale the warp introduces.
    ///
    /// `1.0` is a warp that stretches nothing. Compared against [`MAX_STRETCH`].
    pub stretch: f32,
    /// The vertical convergence this correction was solved against, `0..1`.
    ///
    /// Stored so a plan can say what it was fixing. A re-measurement that finds a different
    /// convergence is a different correction, and a row that carried only the answer would make
    /// that invisible.
    pub convergence: f32,
}

impl Keystone {
    /// A correction that does nothing.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            vertical: 0.0,
            horizontal: 0.0,
            stretch: 1.0,
            convergence: 0.0,
        }
    }

    /// True when this warp is inside [`MAX_STRETCH`].
    #[must_use]
    pub fn within_cap(&self) -> bool {
        self.stretch.is_finite() && self.stretch <= MAX_STRETCH + f32::EPSILON
    }

    /// True when this correction moves no pixel.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.vertical.abs() < f32::EPSILON && self.horizontal.abs() < f32::EPSILON
    }

    /// Clamp the two sliders into the range the recipe's `Perspective` accepts.
    #[must_use]
    pub fn clamped(mut self) -> Self {
        self.vertical = self.vertical.clamp(-100.0, 100.0);
        self.horizontal = self.horizontal.clamp(-100.0, 100.0);
        self.stretch = if self.stretch.is_finite() {
            self.stretch.clamp(1.0, 4.0)
        } else {
            1.0
        };
        self.convergence = self.convergence.clamp(0.0, 1.0);
        self
    }
}

impl Default for Keystone {
    fn default() -> Self {
        Self::identity()
    }
}

// ---------------------------------------------------------------------------
// Aspect ratios and crop variants
// ---------------------------------------------------------------------------

/// The aspect ratios a crop variant may be generated at.
///
/// Section 2.1's list: "original, 4:5, 5:4, 1:1, 16:9 for social". A closed set rather than a
/// free-form pair of numbers, because [`GeometryPlan::crops`] is stored one row per variant and
/// a free-form ratio is a table with a thousand distinct values in it that nobody can query.
///
/// [`AspectRatio::Original`] is not "no aspect": it is the frame's own, whatever that is, and it
/// is the first entry of every plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AspectRatio {
    /// Whatever the camera shot. Always present, always first, always safe.
    #[default]
    Original,
    /// 4:5 portrait. The social feed's tallest allowed frame.
    FourFive,
    /// 5:4 landscape. The classic album spread and the 8x10 print.
    FiveFour,
    /// 1:1 square.
    Square,
    /// 16:9 landscape. Slides, covers and video-shaped galleries.
    SixteenNine,
}

impl AspectRatio {
    /// Every variant, in the order a plan lists them.
    pub const ALL: [Self; MAX_VARIANTS] = [
        Self::Original,
        Self::FourFive,
        Self::FiveFour,
        Self::Square,
        Self::SixteenNine,
    ];

    /// The four that are not the frame's own.
    pub const VARIANTS: [Self; 4] = [
        Self::FourFive,
        Self::FiveFour,
        Self::Square,
        Self::SixteenNine,
    ];

    /// Stable slug for the wire, the schema and telemetry.
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

    /// Parse a stored slug. Anything unknown reads as [`AspectRatio::Original`].
    #[must_use]
    pub fn from_str_or_original(text: &str) -> Self {
        match text {
            "4:5" => Self::FourFive,
            "5:4" => Self::FiveFour,
            "1:1" => Self::Square,
            "16:9" => Self::SixteenNine,
            _ => Self::Original,
        }
    }

    /// Width divided by height, or `None` for [`AspectRatio::Original`].
    ///
    /// `None` rather than a number, because the frame's own ratio is a property of the frame and
    /// a caller that took a default here would silently crop every portrait frame to landscape.
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

    /// What this variant exists for.
    #[must_use]
    pub const fn purpose(self) -> CropPurpose {
        match self {
            Self::Original => CropPurpose::Primary,
            Self::FourFive | Self::Square => CropPurpose::Social,
            Self::FiveFour | Self::SixteenNine => CropPurpose::Album,
        }
    }
}

impl fmt::Display for AspectRatio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a crop variant is for.
///
/// Section 2.1: "Multi-aspect delivery: generate additional crop variants for social/album use
/// without duplicating files". The delivered gallery keeps the native framing and phase 29 picks
/// among the rest, so the distinction is carried rather than inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CropPurpose {
    /// The frame as delivered. Exactly one variant in every plan carries this.
    #[default]
    Primary,
    /// A social crop: a feed, a story, a square.
    Social,
    /// An album crop: a spread, a print size.
    Album,
}

impl CropPurpose {
    /// Every variant.
    pub const ALL: [Self; 3] = [Self::Primary, Self::Social, Self::Album];

    /// Stable slug for the wire, the schema and telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Social => "social",
            Self::Album => "album",
        }
    }

    /// Parse a stored slug. Anything unknown reads as [`CropPurpose::Primary`].
    #[must_use]
    pub fn from_str_or_primary(text: &str) -> Self {
        match text {
            "social" => Self::Social,
            "album" => Self::Album,
            _ => Self::Primary,
        }
    }
}

impl fmt::Display for CropPurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One proposed rectangle.
///
/// Section 5's `{ aspect, rect, purpose, score, safe }`.
///
/// ## Why `score` is measured by this phase rather than taken from phase 11
///
/// Phase 11's `composition_score` is a judgement about a **photograph**: it fuses horizon,
/// headroom, thirds, balance, clutter, colour competition and a learned aesthetic reading, and
/// several of those terms do not change when the frame is cropped - the background is as
/// cluttered inside a tighter rectangle as it was outside it. Optimising over a rectangle with an
/// objective that mostly does not depend on the rectangle finds noise.
///
/// So [`CropVariant::score`] is a **four-term geometric objective this phase computes over the
/// rectangle**: subject placement, balance, edge cleanliness and headroom, each bounded in
/// `0..1`. It is comparable *between rectangles of one photograph* and deliberately not between
/// photographs. `crates/aura-geometry/src/crop.rs` is the definition and ADR-0047 section 3 is
/// the argument.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CropVariant {
    /// Which aspect this rectangle is at.
    pub aspect: AspectRatio,
    /// The rectangle, in normalised frame coordinates, **after** any rotation and keystone.
    pub rect: Box2,
    /// What it is for.
    pub purpose: CropPurpose,
    /// The composition objective at this rectangle, `0..1`. See the type's own note.
    pub score: f32,
    /// True when every hard constraint in section 6.3 held.
    ///
    /// An unsafe variant is **still stored**, with its reason, because "why is there no square
    /// crop of this photograph" is a question the panel has to be able to answer. What an unsafe
    /// variant may never be is [`GeometryPlan::primary_crop`], and a database CHECK says so.
    pub safe: bool,
}

impl CropVariant {
    /// The whole frame at its own aspect, which is what every plan's first entry is.
    #[must_use]
    pub fn original(score: f32) -> Self {
        Self {
            aspect: AspectRatio::Original,
            rect: Box2::FULL,
            purpose: CropPurpose::Primary,
            score,
            safe: true,
        }
    }

    /// True when this rectangle is the whole frame.
    #[must_use]
    pub fn is_full_frame(&self) -> bool {
        self.rect.x.abs() < 1e-6
            && self.rect.y.abs() < 1e-6
            && (self.rect.w - 1.0).abs() < 1e-6
            && (self.rect.h - 1.0).abs() < 1e-6
    }

    /// The fraction of the frame's long edge this rectangle keeps, given the frame's aspect.
    ///
    /// `frame_aspect` is the frame's width divided by its height. The rectangle is normalised, so
    /// its own pixel dimensions are `w * frame_width` and `h * frame_height`, and which of those
    /// is the long edge depends on both.
    #[must_use]
    pub fn long_edge_fraction(&self, frame_aspect: f32) -> f32 {
        if !frame_aspect.is_finite() || frame_aspect <= 0.0 {
            return 0.0;
        }
        // Work in units where the frame's height is 1 and its width is `frame_aspect`.
        let frame_long = frame_aspect.max(1.0);
        let crop_long = (self.rect.w * frame_aspect).max(self.rect.h);
        (crop_long / frame_long).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// What a crop may never remove
// ---------------------------------------------------------------------------

/// A kind of content a crop may not cut.
///
/// Section 6.3's hard constraints, as a closed set: "every detected face fully inside, primary
/// identities' hands and joined hands inside, resolution >= 60 % of the original long edge, and
/// the moment's key content preserved."
///
/// Ordered by how loudly a photographer complains, which is also the order the reasons are ranked
/// in. A cut hand on a couple portrait is the single most-reported automated-crop failure there
/// is, and it is above a cut face on the list only because a face is impossible to miss and a
/// hand is easy to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectedContent {
    /// A face belonging to one of the two people the wedding is about.
    #[default]
    PrimaryFace,
    /// Any other detected face.
    Face,
    /// A primary identity's hands.
    Hands,
    /// Two people's hands together - the ring, the first dance, the garland.
    JoinedHands,
    /// The content phase 08 says this moment is of.
    MomentKey,
}

impl ProtectedContent {
    /// Every variant, most consequential first.
    pub const ALL: [Self; 5] = [
        Self::PrimaryFace,
        Self::Face,
        Self::Hands,
        Self::JoinedHands,
        Self::MomentKey,
    ];

    /// Stable slug for the wire, the schema and telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PrimaryFace => "primary_face",
            Self::Face => "face",
            Self::Hands => "hands",
            Self::JoinedHands => "joined_hands",
            Self::MomentKey => "moment_key",
        }
    }

    /// Parse a stored slug. Anything unknown reads as [`ProtectedContent::Face`].
    ///
    /// Unknown reads as the *stricter* of the two face variants' neighbours rather than as a
    /// permissive default, because the cost of misreading a stored protection is a crop through
    /// somebody, and the cost of over-protecting is a frame that keeps its original framing.
    #[must_use]
    pub fn from_str_or_face(text: &str) -> Self {
        match text {
            "primary_face" => Self::PrimaryFace,
            "hands" => Self::Hands,
            "joined_hands" => Self::JoinedHands,
            "moment_key" => Self::MomentKey,
            _ => Self::Face,
        }
    }

    /// The human sentence the panel renders.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::PrimaryFace => "one of the couple's faces",
            Self::Face => "somebody's face",
            Self::Hands => "the couple's hands",
            Self::JoinedHands => "two people's hands together",
            Self::MomentKey => "what this moment is of",
        }
    }
}

impl fmt::Display for ProtectedContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One region a crop may not cut, and who it belongs to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProtectedRegion {
    /// What kind of thing it is.
    pub kind: ProtectedContent,
    /// Where it is, in frame coordinates, **before** any rotation or keystone.
    pub area: Box2,
    /// Whose it is, when phase 06 assigned it to somebody.
    #[serde(default)]
    pub identity: Option<IdentityId>,
}

impl ProtectedRegion {
    /// A region with no owner.
    #[must_use]
    pub const fn anonymous(kind: ProtectedContent, area: Box2) -> Self {
        Self {
            kind,
            area,
            identity: None,
        }
    }

    /// True when `rect` contains this region with [`SAFETY_MARGIN`] to spare.
    ///
    /// The margin is the whole point: a region touching the crop's edge is a region the
    /// resampler's filter kernel reads off the end of the photograph.
    #[must_use]
    pub fn inside(&self, rect: Box2) -> bool {
        self.area.x >= rect.x + SAFETY_MARGIN - 1e-6
            && self.area.y >= rect.y + SAFETY_MARGIN - 1e-6
            && self.area.x + self.area.w <= rect.x + rect.w - SAFETY_MARGIN + 1e-6
            && self.area.y + self.area.h <= rect.y + rect.h - SAFETY_MARGIN + 1e-6
    }
}

/// What the safety filter checked, and what it found.
///
/// Section 5's `{ faces_intact, resolution_ok, content_kept }`, plus the counts that make the
/// three booleans auditable. A report that said only `faces_intact: true` over a frame with no
/// faces in it would be indistinguishable from one over a frame with six faces all inside, and
/// section 10.1's hard gate - "zero auto-crops cut a detected face" - is a query over the
/// difference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CropSafetyReport {
    /// True when every detected face is fully inside the primary crop.
    pub faces_intact: bool,
    /// True when the primary crop keeps at least [`MIN_LONG_EDGE_FRACTION`] of the long edge.
    pub resolution_ok: bool,
    /// True when every other protected region survived.
    pub content_kept: bool,
    /// How many protected regions were considered.
    ///
    /// **The denominator.** Zero means nothing was protected, which on this build is the common
    /// case and is not the same as everything being safe. Phase 08's rule: say what the
    /// denominator is.
    pub considered: u32,
    /// How many of them the primary crop would have cut, before the crop was refused.
    pub at_risk: u32,
    /// The fraction of the original long edge the primary crop keeps, `0..1`.
    pub long_edge_fraction: f32,
    /// The regions themselves, worst kind first, capped at [`CropSafetyReport::MAX_REGIONS`].
    #[serde(default)]
    pub regions: Vec<ProtectedRegion>,
}

impl CropSafetyReport {
    /// The most regions one report carries.
    ///
    /// Sixteen. A frame with sixty faces in it has sixty protected regions and the safety filter
    /// checks all of them; what is capped is how many are *stored and rendered*, because a panel
    /// listing sixty rectangles is a panel nobody reads. The counts are not capped.
    pub const MAX_REGIONS: usize = 16;

    /// A report over a frame where nothing was protected.
    ///
    /// Every boolean true, `considered` zero. The pair is what makes the difference visible: a
    /// caller that reads only the booleans learns the crop is safe, and a caller that reads
    /// `considered` learns why that was easy.
    #[must_use]
    pub const fn nothing_protected(long_edge_fraction: f32) -> Self {
        Self {
            faces_intact: true,
            resolution_ok: true,
            content_kept: true,
            considered: 0,
            at_risk: 0,
            long_edge_fraction,
            regions: Vec::new(),
        }
    }

    /// True when all three constraints held.
    #[must_use]
    pub const fn is_safe(&self) -> bool {
        self.faces_intact && self.resolution_ok && self.content_kept
    }
}

// ---------------------------------------------------------------------------
// Reason codes
// ---------------------------------------------------------------------------

/// Why this phase did what it did, or did not.
///
/// Thirty codes in four subjects - the lens, the rotation, the keystone and the crop. Twenty of
/// the thirty are **refusals**, which is the honest shape for a phase whose section 10.1 requires
/// that at least seventy per cent of frames are left alone: the commonest question a photographer
/// has about this phase is why a particular photograph was *not* straightened or cropped, and a
/// vocabulary that could only describe actions would leave the panel silent on the majority of
/// the wedding.
///
/// Codes rather than sentences on the wire and in the schema. Phase 09's rule, tenth phase
/// running: a stored sentence is copy a release can change, and a catalog full of English cannot
/// be translated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometryCode {
    // --- the lens ---------------------------------------------------------
    /// The camera's own correction data was used.
    LensEmbedded,
    /// A row in the bundled profile database was used.
    LensProfileMatched,
    /// The distortion was estimated from this photograph's own straight edges.
    LensEstimated,
    /// No profile, and not enough straight edges to estimate from. Nothing was corrected.
    LensProfileMissing,
    /// A profile matched the lens but not this focal length, and interpolating would have been a
    /// guess dressed as a measurement.
    LensFocalOutOfRange,
    /// Lateral chromatic aberration was corrected.
    LensCaCorrected,
    /// The frame carries no high-contrast edge off-centre, so a CA correction could not be
    /// verified and was not applied.
    LensCaUnverifiable,
    /// The vignette correction was reduced because correcting it fully would have clipped.
    LensVignetteReduced,

    // --- straightening ----------------------------------------------------
    /// The frame was rotated to level the horizon.
    Straightened,
    /// The tilt is inside the band but the horizon estimate is below [`ROTATE_ACT_AT`].
    HorizonUnsure,
    /// There is no horizon in this photograph to measure.
    HorizonAbsent,
    /// The tilt is smaller than [`ROTATE_MIN_DEG`]. The frame is level enough.
    TiltNegligible,
    /// The tilt is larger than [`ROTATE_MAX_DEG`], so it reads as a decision.
    TiltTooLarge,
    /// Phase 11 says the tilt is intentional.
    TiltIntentional,
    /// The rotation was reduced because the crop it implied would have cut something.
    RotationReduced,
    /// The rotation was abandoned because no reduced angle kept everything in frame.
    RotationRefused,

    // --- keystone ---------------------------------------------------------
    /// Converging verticals were corrected.
    KeystoneApplied,
    /// The verticals converge by less than [`KEYSTONE_ACT_AT`].
    KeystoneNotNeeded,
    /// There is no architectural vertical family in this frame - the verticals are people.
    KeystoneNoArchitecture,
    /// The correction the convergence asked for would have exceeded [`MAX_STRETCH`].
    KeystoneStretchCapped,
    /// The correction was abandoned because the crop it implied would have cut something.
    KeystoneRefused,

    // --- the crop ---------------------------------------------------------
    /// A proposed crop replaced the original framing.
    CropProposed,
    /// The original framing is what is delivered. **The commonest code in the product.**
    CropKeptOriginal,
    /// No candidate beat the original by [`MIN_IMPROVEMENT`].
    CropNoImprovement,
    /// A candidate would have cut a face.
    CropCutsFace,
    /// A candidate would have cut a primary identity's hands.
    CropCutsHands,
    /// A candidate would have dropped below [`MIN_LONG_EDGE_FRACTION`].
    CropBelowResolution,
    /// A candidate would have removed what phase 08 says this moment is of.
    CropDropsMomentKey,
    /// An aspect variant could not be generated safely at all.
    VariantUnsafe,
    /// The photographer set this geometry by hand and automation left it alone.
    UserFramed,
}

impl GeometryCode {
    /// Every code, in the order the panel groups them.
    pub const ALL: [Self; Self::COUNT] = [
        Self::LensEmbedded,
        Self::LensProfileMatched,
        Self::LensEstimated,
        Self::LensProfileMissing,
        Self::LensFocalOutOfRange,
        Self::LensCaCorrected,
        Self::LensCaUnverifiable,
        Self::LensVignetteReduced,
        Self::Straightened,
        Self::HorizonUnsure,
        Self::HorizonAbsent,
        Self::TiltNegligible,
        Self::TiltTooLarge,
        Self::TiltIntentional,
        Self::RotationReduced,
        Self::RotationRefused,
        Self::KeystoneApplied,
        Self::KeystoneNotNeeded,
        Self::KeystoneNoArchitecture,
        Self::KeystoneStretchCapped,
        Self::KeystoneRefused,
        Self::CropProposed,
        Self::CropKeptOriginal,
        Self::CropNoImprovement,
        Self::CropCutsFace,
        Self::CropCutsHands,
        Self::CropBelowResolution,
        Self::CropDropsMomentKey,
        Self::VariantUnsafe,
        Self::UserFramed,
    ];

    /// How many there are.
    pub const COUNT: usize = 30;

    /// Stable slug for the wire, the schema, telemetry and `docs/reason-codes.md`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LensEmbedded => "geometry_lens_embedded",
            Self::LensProfileMatched => "geometry_lens_profile_matched",
            Self::LensEstimated => "geometry_lens_estimated",
            Self::LensProfileMissing => "geometry_lens_profile_missing",
            Self::LensFocalOutOfRange => "geometry_lens_focal_out_of_range",
            Self::LensCaCorrected => "geometry_lens_ca_corrected",
            Self::LensCaUnverifiable => "geometry_lens_ca_unverifiable",
            Self::LensVignetteReduced => "geometry_lens_vignette_reduced",
            Self::Straightened => "geometry_straightened",
            Self::HorizonUnsure => "geometry_horizon_unsure",
            Self::HorizonAbsent => "geometry_horizon_absent",
            Self::TiltNegligible => "geometry_tilt_negligible",
            Self::TiltTooLarge => "geometry_tilt_too_large",
            Self::TiltIntentional => "geometry_tilt_intentional",
            Self::RotationReduced => "geometry_rotation_reduced",
            Self::RotationRefused => "geometry_rotation_refused",
            Self::KeystoneApplied => "geometry_keystone_applied",
            Self::KeystoneNotNeeded => "geometry_keystone_not_needed",
            Self::KeystoneNoArchitecture => "geometry_keystone_no_architecture",
            Self::KeystoneStretchCapped => "geometry_keystone_stretch_capped",
            Self::KeystoneRefused => "geometry_keystone_refused",
            Self::CropProposed => "geometry_crop_proposed",
            Self::CropKeptOriginal => "geometry_crop_kept_original",
            Self::CropNoImprovement => "geometry_crop_no_improvement",
            Self::CropCutsFace => "geometry_crop_cuts_face",
            Self::CropCutsHands => "geometry_crop_cuts_hands",
            Self::CropBelowResolution => "geometry_crop_below_resolution",
            Self::CropDropsMomentKey => "geometry_crop_drops_moment_key",
            Self::VariantUnsafe => "geometry_variant_unsafe",
            Self::UserFramed => "geometry_user_framed",
        }
    }

    /// Parse a stored slug.
    ///
    /// `None` rather than a default, because a code this build does not know is a code from a
    /// newer build, and rendering it as "the original framing was kept" would be a sentence about
    /// a decision nobody made.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|code| code.as_str() == text)
    }

    /// The sentence a photographer reads.
    ///
    /// Written in the product's voice rather than the engine's: a photographer wants to know what
    /// happened to their photograph, not which branch was taken.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::LensEmbedded => "the camera recorded its own lens corrections and AURA used them",
            Self::LensProfileMatched => "AURA corrected this lens from its profile database",
            Self::LensEstimated => {
                "there is no profile for this lens, so the distortion was estimated \
                 from the straight lines in this photograph"
            }
            Self::LensProfileMissing => {
                "there is no profile for this lens and not enough straight lines to \
                 estimate one, so nothing was corrected"
            }
            Self::LensFocalOutOfRange => {
                "there is a profile for this lens but not at this focal length, so \
                 nothing was corrected"
            }
            Self::LensCaCorrected => "colour fringing at high-contrast edges was corrected",
            Self::LensCaUnverifiable => {
                "there is no high-contrast edge away from the centre of this frame, \
                 so a fringing correction could not be checked and was not applied"
            }
            Self::LensVignetteReduced => {
                "the corners were lifted less than the profile asks for, because \
                 correcting them fully would have clipped"
            }
            Self::Straightened => "the horizon was levelled",
            Self::HorizonUnsure => {
                "the horizon looks off level, but not clearly enough to move the \
                 photograph without being asked"
            }
            Self::HorizonAbsent => "there is no horizon in this photograph to level against",
            Self::TiltNegligible => "the horizon is already level",
            Self::TiltTooLarge => {
                "this photograph is tilted far enough that it reads as a decision, \
                 so it was left alone"
            }
            Self::TiltIntentional => "the tilt in this photograph reads as deliberate",
            Self::RotationReduced => {
                "the photograph was levelled part of the way, because levelling it \
                 fully would have cropped into somebody"
            }
            Self::RotationRefused => {
                "levelling this photograph would have cropped into somebody, so it \
                 was left as it was shot"
            }
            Self::KeystoneApplied => "the converging verticals were straightened",
            Self::KeystoneNotNeeded => "the verticals in this photograph are already parallel",
            Self::KeystoneNoArchitecture => {
                "the upright lines in this photograph are people rather than \
                 architecture, so no perspective correction was made"
            }
            Self::KeystoneStretchCapped => {
                "straightening these verticals fully would have stretched the \
                 photograph too far, so the correction was limited"
            }
            Self::KeystoneRefused => {
                "correcting the perspective would have cropped into somebody, so the \
                 photograph was left as it was shot"
            }
            Self::CropProposed => "AURA suggests a tighter frame",
            Self::CropKeptOriginal => "the framing you shot is the framing that is delivered",
            Self::CropNoImprovement => {
                "no tighter frame was clearly better than the one you shot"
            }
            Self::CropCutsFace => "a tighter frame would have cut somebody's face",
            Self::CropCutsHands => "a tighter frame would have cut the couple's hands",
            Self::CropBelowResolution => {
                "a tighter frame would have left too little of the photograph to deliver"
            }
            Self::CropDropsMomentKey => {
                "a tighter frame would have left out what this moment is of"
            }
            Self::VariantUnsafe => {
                "this aspect ratio cannot be cropped from this photograph without \
                 cutting somebody"
            }
            Self::UserFramed => "you framed this photograph yourself, so AURA left it alone",
        }
    }

    /// True when this code describes something that was **not** done.
    ///
    /// Twenty of thirty. The panel groups on it, and `docs/geometry.md` is organised by it,
    /// because a careful product that only listed its actions would read as a careless one -
    /// phase 20's rule, inherited a third time.
    #[must_use]
    pub const fn is_refusal(self) -> bool {
        matches!(
            self,
            Self::LensProfileMissing
                | Self::LensFocalOutOfRange
                | Self::LensCaUnverifiable
                | Self::LensVignetteReduced
                | Self::HorizonUnsure
                | Self::HorizonAbsent
                | Self::TiltNegligible
                | Self::TiltTooLarge
                | Self::TiltIntentional
                | Self::RotationReduced
                | Self::RotationRefused
                | Self::KeystoneNotNeeded
                | Self::KeystoneNoArchitecture
                | Self::KeystoneStretchCapped
                | Self::KeystoneRefused
                | Self::CropKeptOriginal
                | Self::CropNoImprovement
                | Self::CropCutsFace
                | Self::CropCutsHands
                | Self::CropBelowResolution
                | Self::CropDropsMomentKey
                | Self::VariantUnsafe
                | Self::UserFramed
        )
    }

    /// True when this code is about a crop that was declined for a safety rule.
    ///
    /// The histogram section 11's `geometry.crop_refused` telemetry event carries, and the one
    /// section 10.1's hard gate is checked against.
    #[must_use]
    pub const fn is_safety_refusal(self) -> bool {
        matches!(
            self,
            Self::CropCutsFace
                | Self::CropCutsHands
                | Self::CropBelowResolution
                | Self::CropDropsMomentKey
                | Self::VariantUnsafe
        )
    }
}

impl fmt::Display for GeometryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One reason, with the rectangle it is about when there is one.
///
/// The same shape every phase since 09 has used. `weight` is negative when the reason cost the
/// frame something and positive when it earned it, which is what lets [`GeometryPlan::reasons`]
/// be ranked without a second field saying which direction each one points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GeometryReason {
    /// Which code.
    pub code: GeometryCode,
    /// How much it mattered, `-1..1`.
    pub weight: f32,
    /// The rectangle the panel highlights, when the reason is about a place.
    #[serde(default)]
    pub evidence: Option<Box2>,
}

impl GeometryReason {
    /// A reason about the whole frame.
    #[must_use]
    pub const fn plain(code: GeometryCode, weight: f32) -> Self {
        Self {
            code,
            weight,
            evidence: None,
        }
    }

    /// A reason about one rectangle.
    #[must_use]
    pub const fn at(code: GeometryCode, weight: f32, evidence: Box2) -> Self {
        Self {
            code,
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
// The plan
// ---------------------------------------------------------------------------

/// Everything this phase decided about one photograph's geometry.
///
/// PHASE-23 section 5's frozen shape, plus the four fields every stored decision in this product
/// has carried since phase 09: the scene it was decided under, the version columns, and the
/// photographer's own override bit.
///
/// ## Reading a plan
///
/// The plan is applied in the render graph's order, which is not the order the fields are
/// declared in: the lens corrections run in the input half, before white balance has finished
/// being creative with the pixels, and the rotation, the keystone and the crop run at
/// `Stage::Geometry`, second to last. Section 6.1's "apply corrections in linear light before
/// creative operations so vignette correction does not fight exposure decisions" is a statement
/// about `aura_render::graph::ORDER` rather than about this struct.
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryPlan {
    /// The photograph.
    pub image_id: ImageId,
    /// The scene it was decided under. Invariant 7.
    pub scene: SceneId,
    /// What the lens correction does.
    pub lens: LensCorrection,
    /// How far the frame is rotated to level it, in degrees, positive clockwise.
    ///
    /// Zero when nothing was rotated, and a reason code says which of the six reasons why.
    pub rotate_deg: f32,
    /// How sure the horizon estimate behind [`GeometryPlan::rotate_deg`] was, `0..1`.
    ///
    /// Stored even when the rotation was declined, because "the horizon looks off and AURA is not
    /// sure enough to move it" is the answer to the commonest question this phase gets, and a row
    /// carrying only the applied angle could not give it.
    pub rotate_conf: f32,
    /// The perspective correction, or `None`.
    pub keystone: Option<Keystone>,
    /// Every rectangle, **always at least one**, and the first is always the original framing.
    ///
    /// Bounded by [`MAX_VARIANTS`].
    pub crops: Vec<CropVariant>,
    /// Which entry of [`GeometryPlan::crops`] is delivered.
    ///
    /// An index rather than a copy of the rectangle, so that there is exactly one place a
    /// delivered crop is written down. Always in range, always pointing at a `safe` variant, and
    /// a database CHECK enforces the second.
    pub primary_crop: usize,
    /// What the safety filter checked, and what it found.
    pub safety: CropSafetyReport,
    /// Why, strongest first, at most [`MAX_REASONS`].
    pub reasons: Vec<GeometryReason>,
    /// How sure the whole plan is, `0..1`. Invariant 2.
    pub confidence: f32,
    /// True when the photographer set this geometry by hand.
    ///
    /// Checked inside the statement a re-analysis would overwrite the row with. A photographer's
    /// decision is unbeatable, for the twelfth phase running - and here it is the strongest form
    /// of the rule in the product, because a re-crop of a frame somebody framed by hand throws
    /// away work that cannot be recovered from anything.
    pub user_edited: bool,
    /// True when the photographer looked at what AURA proposed and agreed.
    pub reviewed: bool,
    /// Which build's arithmetic produced the geometry.
    pub analysis_ver: u16,
    /// Which lens profile database and crop rule table it was decided against.
    pub profile_ver: u16,
}

impl GeometryPlan {
    /// A plan that changes nothing, over a frame nobody could measure.
    ///
    /// Not a failure: it is what a photograph with an unknown lens, no horizon and no faces
    /// legitimately gets, and it is the most common plan in a wedding shot on primes indoors.
    #[must_use]
    pub fn untouched(image_id: ImageId, scene: SceneId) -> Self {
        Self {
            image_id,
            scene,
            lens: LensCorrection::none(),
            rotate_deg: 0.0,
            rotate_conf: 0.0,
            keystone: None,
            crops: vec![CropVariant::original(0.0)],
            primary_crop: 0,
            safety: CropSafetyReport::nothing_protected(1.0),
            reasons: vec![GeometryReason::plain(GeometryCode::CropKeptOriginal, 0.0)],
            confidence: 0.0,
            user_edited: false,
            reviewed: false,
            analysis_ver: 0,
            profile_ver: 0,
        }
    }

    /// The rectangle that is delivered.
    ///
    /// Falls back to the whole frame when the index is out of range, which cannot happen through
    /// any constructor here and can happen through a row somebody edited by hand.
    #[must_use]
    pub fn primary(&self) -> Box2 {
        self.crops
            .get(self.primary_crop)
            .map_or(Box2::FULL, |variant| variant.rect)
    }

    /// True when this plan moves no pixel at all.
    ///
    /// The state at least seventy per cent of a wedding is expected to be in.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.lens.is_identity()
            && self.rotate_deg.abs() < f32::EPSILON
            && self.keystone.is_none_or(|k| k.is_identity())
            && self
                .crops
                .get(self.primary_crop)
                .is_some_and(CropVariant::is_full_frame)
    }

    /// The variants that are not the delivered one and are safe to use.
    ///
    /// What phase 29 reads.
    #[must_use]
    pub fn alternates(&self) -> Vec<&CropVariant> {
        self.crops
            .iter()
            .enumerate()
            .filter(|(index, variant)| *index != self.primary_crop && variant.safe)
            .map(|(_, variant)| variant)
            .collect()
    }

    /// True when this plan carries a code.
    #[must_use]
    pub fn has(&self, code: GeometryCode) -> bool {
        self.reasons.iter().any(|reason| reason.code == code)
    }
}

// ---------------------------------------------------------------------------
// The override
// ---------------------------------------------------------------------------

/// What a photographer may change, and the whole of it.
///
/// Section 8's step 8 asks for "AI proposals, manual override and revert-to-original", and this
/// is the second and third of the three. Every field is `Option`, and `None` means "leave what
/// AURA decided alone" rather than "set it to nothing" - the difference between an override that
/// changes the crop and one that silently un-corrects a lens.
///
/// ## What is deliberately not here
///
/// **There is no way to widen a safety rule.** A photographer may crop tighter than AURA
/// proposed, and the rectangle they set is stored with `user_edited = 1` and never touched again;
/// what they cannot do through this shape is tell AURA that cutting faces is acceptable *in
/// general*, because the next four hundred frames would then be cropped through people by a
/// setting somebody changed once.
///
/// **There is no scale.** [`GeometryOverride::crop`] is a rectangle inside the frame.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GeometryOverride {
    /// The rectangle to deliver, in normalised frame coordinates.
    ///
    /// Stored as the primary variant with `user_edited = 1`.
    #[serde(default)]
    pub crop: Option<Box2>,
    /// The aspect the photographer chose, when they chose one.
    #[serde(default)]
    pub aspect: Option<AspectRatio>,
    /// The rotation, in degrees. Bounded to `-45..45` by the recipe.
    #[serde(default)]
    pub rotate_deg: Option<f32>,
    /// Whether to correct lens distortion.
    #[serde(default)]
    pub distortion: Option<bool>,
    /// Vignette correction, `0..=100`.
    #[serde(default)]
    pub vignette: Option<u8>,
    /// Whether to correct lateral chromatic aberration.
    #[serde(default)]
    pub ca: Option<bool>,
    /// Set to true to throw away every geometry decision and return to what was shot.
    ///
    /// Section 13: "Original framing is always one click away." This is that click, and it is a
    /// separate field rather than "set the crop to the whole frame" because reverting must also
    /// clear the rotation, the keystone and `user_edited` - a photographer who reverts wants the
    /// photograph back, not a hand-set full-frame crop that automation will never revisit.
    #[serde(default)]
    pub revert: bool,
}

impl GeometryOverride {
    /// True when this override asks for nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.revert
            && self.crop.is_none()
            && self.aspect.is_none()
            && self.rotate_deg.is_none()
            && self.distortion.is_none()
            && self.vignette.is_none()
            && self.ca.is_none()
    }

    /// The revert.
    #[must_use]
    pub fn reverted() -> Self {
        Self {
            revert: true,
            ..Self::default()
        }
    }
}

// ---------------------------------------------------------------------------
// The outline
// ---------------------------------------------------------------------------

/// What a project's geometry pass covered and did.
///
/// Coverage is measured against **every photograph** in the project, as phases 09, 10 and 22
/// measure theirs, because a geometry decision needs only pixels and EXIF. `acted_on` is the
/// second number and it is the one that matters: section 10.1 requires that most frames keep
/// their original framing, so a wedding where `acted_on` approaches `planned` is a wedding where
/// something has gone wrong with the improvement margin.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GeometryOutline {
    /// Photographs in the project.
    pub photos: u64,
    /// How many have a plan.
    pub planned: u64,
    /// `planned / photos`, `0..1`.
    pub coverage: f32,
    /// How many plans move at least one pixel.
    pub acted_on: u64,
    /// How many frames were straightened.
    pub straightened: u64,
    /// The mean absolute rotation applied, in degrees, over the frames that were rotated.
    pub mean_rotation_deg: f32,
    /// How many frames got a keystone correction.
    pub keystoned: u64,
    /// How many frames are delivered at something other than their original framing.
    pub cropped: u64,
    /// How many keep the framing they were shot at.
    ///
    /// **Section 10.1's conservatism gate**, as a stored number: `kept_original / planned` must
    /// be at least 0.70.
    pub kept_original: u64,
    /// How many aspect variants were generated across the project.
    pub variants: u64,
    /// How many crop candidates were refused, by code.
    ///
    /// Section 11's `geometry.crop_refused` telemetry event, as a stored histogram.
    pub crop_refusals: Vec<(GeometryCode, u64)>,
    /// How many frames carried a lens profile, by source.
    pub lens_sources: [u64; 4],
    /// The lens ids nothing could be found for, most frequent first.
    ///
    /// Section 11's `geometry.lens_profile_missing`. The one thing on this outline a studio can
    /// act on: a lens on this list is a lens whose distortion nobody has measured.
    pub lenses_missing: Vec<(String, u64)>,
    /// How many faces the safety filter checked across the project.
    ///
    /// **The denominator behind the hard gate.** Section 10.1's "zero auto-crops cut a detected
    /// face" over a wedding with no faces in it is arithmetic rather than evidence, and this is
    /// how a caller finds that out - phase 21's rule.
    pub faces_checked: u64,
    /// How many delivered crops cut a face. **Must be zero.**
    pub faces_cut: u64,
    /// How many frames the photographer framed by hand.
    pub user_edited: u64,
    /// How many frames are waiting for somebody to look.
    pub pending_review: u64,
}

impl GeometryOutline {
    /// The share of planned frames that kept their original framing, `0..1`.
    ///
    /// One when nothing has been planned, which is the reading that does not fail a
    /// conservatism gate over an empty project.
    #[must_use]
    pub fn conservatism(&self) -> f32 {
        if self.planned == 0 {
            return 1.0;
        }
        (self.kept_original as f64 / self.planned as f64) as f32
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask what was done to a photograph's geometry.
///
/// Nineteenth service of its kind, and the rule is the same one every phase since 05 has written:
/// phase 27 has to be able to say why a frame is tilted or why a hand is missing, phase 29 picks
/// between the aspect variants this phase generates, and phase 30 exports one of them. Two
/// answers to "what is this photograph's crop" is a delivered gallery whose album does not match
/// it - and unlike a grade, a crop cannot be reconciled afterwards, because the two answers
/// disagree about which pixels exist.
///
/// Frozen. Implemented by `aura_geometry::Geometry`.
pub trait GeometryService: Send + Sync + fmt::Debug {
    /// What a project's pass covered and did.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<GeometryOutline>;

    /// One photograph's geometry, or `None` when it has not been planned.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plan cannot be read.
    fn of_image(&self, image: ImageId) -> AuraResult<Option<GeometryPlan>>;

    /// The frames whose geometry is worth a look, least confident first.
    ///
    /// A queue, not a shortlist: nothing here says which frames to keep, and `limit` bounds the
    /// answer because a 4,000-frame wedding would otherwise return all of it.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plans cannot be read.
    fn review_queue(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>>;

    /// Every safe crop variant for one photograph, for phase 29 to choose between.
    ///
    /// Separate from [`GeometryService::of_image`] because an album layout wants this and nothing
    /// else, and decoding a whole plan to take one list out of it is how a layout pass ends up
    /// parsing reason codes it never renders.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plan cannot be read.
    fn variants(&self, image: ImageId) -> AuraResult<Vec<CropVariant>>;

    /// Record that the photographer looked and agrees.
    ///
    /// Does **not** set `user_edited`: agreeing with a proposal is not the same as making one,
    /// and a later re-analysis with a better lens profile should still improve a frame somebody
    /// merely approved. Phase 15's distinction, inherited.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plan cannot be written, and `AURA-ML-5111` when the image has no
    /// plan to accept.
    fn accept(&self, image: ImageId) -> Result<GeometryPlan, AuraError>;

    /// Record the photographer's own geometry, or revert to what was shot.
    ///
    /// Sets `user_edited`, which is checked inside every statement a re-analysis would overwrite
    /// the row with. [`GeometryOverride::revert`] clears it again, because a photographer who
    /// asked for the original framing back has asked automation to resume rather than to stop.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the plan cannot be written, and `AURA-ML-5111` when the image has no
    /// plan or the override names a rectangle that is not inside the frame.
    ///
    /// **A photographer's own rectangle is not checked against the protected regions.** The
    /// safety filter binds what *automation* may propose over four hundred frames; a person
    /// cropping one photograph of their own may crop it as tightly as they like, and refusing
    /// them would be the product overruling the photographer on their own work.
    fn set_override(
        &self,
        image: ImageId,
        change: &GeometryOverride,
    ) -> Result<GeometryPlan, AuraError>;
}

// ---------------------------------------------------------------------------
// The one piece of geometry two crates must agree on
// ---------------------------------------------------------------------------

/// The largest axis-aligned rectangle of the frame's own aspect that stays inside it after a
/// rotation, as a normalised centred [`Box2`].
///
/// **In the contract rather than in the solver**, and this is the only function in this file that
/// computes anything. `aura-geometry` needs it to know how much a rotation costs *before* it
/// decides whether the rotation is worth making, and `aura-render` needs it to know which pixels
/// exist after the resample. Two implementations of it is a plan that reports one crop and a
/// render that produces another - which is a delivered frame with a triangle of black in the
/// corner, or a face just outside a rectangle a safety filter had cleared.
///
/// # The maths, and why it is not the formula everybody reaches for
///
/// The widely copied `rotatedRectWithMaxArea` maximises **area**, and the rectangle it returns
/// does not have the frame's aspect ratio: on a 6000x4000 frame at five degrees it keeps 95 % of
/// the width and 88 % of the height, which delivers a 1.63:1 photograph from a 3:2 one. That is a
/// change of shape nobody asked for on every straightened frame in a wedding, and it is invisible
/// until somebody lays two frames side by side in an album. Straightening keeps the shape.
///
/// So this solves the aspect-preserving problem instead, which has a closed form and no branches.
/// A rectangle `s*W` by `s*H` centred on the frame and rotated by `a` has corner extents
/// `s/2 * (W cos a + H sin a)` horizontally and `s/2 * (W sin a + H cos a)` vertically. Both must
/// stay inside the frame's own half-extents, so
///
/// ```text
/// s = min( W / (W cos a + H sin a),  H / (W sin a + H cos a) )
/// ```
///
/// and the returned rectangle is `s` on both normalised axes, centred. The bound is tight: at the
/// minimum, one of the two constraints is an equality, so the rectangle touches the rotated frame.
///
/// Angles are taken modulo ninety degrees in magnitude, since a rotation by `90 + a` inscribes the
/// same rectangle as one by `a` in a frame whose sides have swapped.
///
/// # What it does not do
///
/// It returns the largest **centred** rectangle. A smaller rectangle may be translated, and
/// `aura_geometry::straighten` uses that freedom when the centred crop would cut a face - which
/// is exactly why this function returns the maximum rather than a final answer.
///
/// # Panics
///
/// Never. A degenerate frame returns [`Box2::FULL`], because a rotation of nothing is nothing.
#[must_use]
pub fn rotation_crop(width: u32, height: u32, degrees: f32) -> Box2 {
    if width == 0 || height == 0 || !degrees.is_finite() {
        return Box2::FULL;
    }
    let angle = degrees.abs().to_radians() % std::f32::consts::FRAC_PI_2;
    let (sin_a, cos_a) = (angle.sin().abs(), angle.cos().abs());
    if sin_a < 1e-7 {
        return Box2::FULL;
    }

    let w = width as f32;
    let h = height as f32;
    let horizontal = w * cos_a + h * sin_a;
    let vertical = w * sin_a + h * cos_a;
    if horizontal <= 0.0 || vertical <= 0.0 {
        return Box2::FULL;
    }
    let scale = (w / horizontal).min(h / vertical);
    if !scale.is_finite() || scale <= 0.0 {
        return Box2::FULL;
    }
    let scale = scale.clamp(0.0, 1.0);
    Box2 {
        x: (1.0 - scale) / 2.0,
        y: (1.0 - scale) / 2.0,
        w: scale,
        h: scale,
    }
}

/// The largest rectangle of a given aspect ratio that fits inside `bounds`, centred on
/// `(centre_x, centre_y)` and shifted only as far as it must be to stay inside.
///
/// `aspect` is width over height **in pixels**, and `frame_aspect` is the frame's own, because a
/// normalised rectangle is not square even when the crop is. The centre is a *preference*: a
/// rectangle whose requested centre would put it outside `bounds` is slid back in rather than
/// shrunk, because shrinking to honour a centre is how a square crop of a frame with a subject
/// near the edge ends up at half the resolution it could have had.
///
/// # Panics
///
/// Never. Degenerate inputs return `bounds`.
#[must_use]
pub fn fit_aspect(bounds: Box2, frame_aspect: f32, aspect: f32, centre: (f32, f32)) -> Box2 {
    if !frame_aspect.is_finite()
        || !aspect.is_finite()
        || frame_aspect <= 0.0
        || aspect <= 0.0
        || bounds.w <= 0.0
        || bounds.h <= 0.0
    {
        return bounds;
    }
    // In normalised coordinates a rectangle of pixel aspect `aspect` has
    // `w / h = aspect / frame_aspect`.
    let target = aspect / frame_aspect;
    let (mut w, mut h) = if bounds.w / bounds.h > target {
        (bounds.h * target, bounds.h)
    } else {
        (bounds.w, bounds.w / target)
    };
    w = w.min(bounds.w);
    h = h.min(bounds.h);
    // `.max(bounds.x)` because `bounds.x + bounds.w - w` is a few ulps *below* `bounds.x` when
    // the fitted rectangle is exactly the bounds, and `f32::clamp` aborts when its minimum
    // exceeds its maximum. The doc comment above promises this function never panics; without
    // these two guards it does, on the commonest input there is - a crop that fills its bounds.
    let x = (centre.0 - w / 2.0).clamp(bounds.x, (bounds.x + bounds.w - w).max(bounds.x));
    let y = (centre.1 - h / 2.0).clamp(bounds.y, (bounds.y + bounds.h - h).max(bounds.y));
    Box2 { x, y, w, h }
}
