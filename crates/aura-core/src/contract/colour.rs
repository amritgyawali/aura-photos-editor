//! FROZEN CONTRACT. How a photograph should be graded, and the guarantee that grading it
//! never turns anybody's skin unnatural.
//!
//! PHASE-16 section 5 freezes `ColourDecision` before any solver exists. The file is in
//! `aura-core` rather than in `aura-brain-photo` for the reason [`crate::contract::tone`],
//! [`crate::contract::integrity`] and [`crate::contract::composition`] are: the phases that
//! consume a grade are 17 (personal style), 18 (local adjustments), 25 (gallery
//! consistency), 27 (QC) and 29 (album curation), and none of them needs the histogram
//! reader, the curve fitter or the harmony solver.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **The skin guarantee is a measurement, not an intention.** Section 6.3 sets two
//! ceilings - [`SKIN_HUE_CEILING_DEG`] and [`SKIN_CHROMA_CEILING`] - and
//! [`SkinGuardReport`] is what a frame carries to say whether it met them. It is measured
//! *after* the grade, on the skin pixels themselves, because every failure mode this phase
//! has starts with an attenuation that was applied to a parameter while the promise was
//! made about a pixel. ADR-0033 decision 1 has the argument.
//!
//! There is no field anywhere in this contract for an ideal skin colour, exactly as there is
//! none in [`crate::contract::tone`]. What the guard compares against is *this frame's own
//! skin before the grade*: the ceiling is on how far grading moved it, so a person's skin is
//! measured against itself and the guarantee means the same thing for everybody.
//!
//! ## The second thing: monotonicity is structural
//!
//! [`ToneCurve`] can only be built through [`ToneCurve::new`], which refuses a set of
//! control points that is not strictly increasing in `x` and non-decreasing in `y`. There is
//! no evaluator here. Phase 14's renderer interpolates a point curve with Fritsch-Carlson,
//! which is monotone exactly when its control points are, so the property section 10.1 gates
//! ("every generated curve is monotonic and produces no posterisation") is a property of the
//! points and cannot be broken by any solver, any override or any stored document. ADR-0033
//! decision 2.
//!
//! ## What a decision is not
//!
//! It is not an edit. The values here reach the pixels only through
//! `aura_recipe::schema::merge`, which is phase 14's rule and the only place
//! [`ColourDecision::user_edited`] can be honoured. A decision with `user_edited = true`
//! still carries AURA's own numbers, kept beside what the photographer chose instead, so the
//! review queue can show the disagreement and phase 30's learning loop can read one.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::ProjectId;
use crate::contract::integrity::CropRect;
use crate::contract::scene::SceneId;

/// PHASE-07 names this `ImageId`; the catalog calls it a `PhotoId`. One type, re-exported
/// here so a consumer of this contract does not have to reach into phase 07's to name a
/// photograph.
pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// The numbers section 10.1 is measured against
// ---------------------------------------------------------------------------

/// The most a grade may rotate a skin pixel's hue, in degrees.
///
/// Section 10.1: "skin hue shift <= 2 deg ... on all skin-tone buckets after full grading".
/// Two degrees is below the threshold at which a side-by-side comparison of the same face
/// reads as a different colour, and it is measured in CIE `LCh` rather than in HSV: a hue
/// angle in a device space is not a perceptual quantity and two degrees of it means
/// different things at different lightnesses.
pub const SKIN_HUE_CEILING_DEG: f32 = 2.0;

/// The most a grade may change a skin pixel's chroma, as a fraction of what it was.
///
/// Section 6.3's "chroma change <= 6 %". Relative rather than absolute for the same reason
/// the hue ceiling is perceptual: six points of chroma is a small change on a saturated
/// mandap and an enormous one on a face in open shade.
pub const SKIN_CHROMA_CEILING: f32 = 0.06;

/// The fewest control points a fitted curve may have.
///
/// Section 2.1's "4-8 control points". Four is the fewest that can express a shoulder and a
/// toe at once, which is what a tone curve is for; three can only tilt.
pub const CURVE_MIN_POINTS: usize = 4;

/// The most control points a fitted curve may have.
///
/// Eight. Beyond that a curve stops being a global tone decision and becomes a local one
/// that phases 18 and 19 own, and a photographer dragging a nine-point curve in the editor
/// is somebody the product has stopped helping.
pub const CURVE_MAX_POINTS: usize = 8;

/// The largest value any tone parameter, vibrance or saturation may take.
///
/// A hundred, matching the recipe's own range so that a decision cannot express a value the
/// document cannot hold. A conversion that clamps is a conversion that silently disagrees
/// with the thing it converted.
pub const PARAM_RANGE: (f32, f32) = (-100.0, 100.0);

/// The largest total adjustment magnitude a decision may carry before it is capped.
///
/// The subtlety metric section 12's first failure mode calls for, as a number. It is the
/// root-mean-square of every normalised parameter this phase sets, and the cap is what stops
/// a frame that is wrong in six ways from being graded aggressively in all six.
pub const SUBTLETY_CEILING: f32 = 0.45;

/// Below this confidence a graded frame reaches the review queue.
///
/// The same 0.55 phase 15 uses for white balance, deliberately: a photographer working
/// through a wedding sees one queue, and two thresholds would make "worth a look" mean two
/// things.
pub const REVIEW_BELOW: f32 = 0.55;

/// How much new highlight clipping a grade may add before the clipping guard re-solves.
///
/// This is the *floor*: a scene's own `clip_tolerance` in `tone_intent.toml` may be lower
/// and never higher. Section 10.1's gate is "no new clipping introduced above the scene
/// tolerance on 99.5 % of frames", and the scene owns the tolerance because invariant 7 says
/// no threshold is global.
pub const CLIP_CEILING: f32 = 0.02;

// ---------------------------------------------------------------------------
// The eight hue bands
// ---------------------------------------------------------------------------

/// One of the eight hue bands an HSL adjustment operates on.
///
/// The order and the slugs are `aura_recipe::contract::recipe::HSL_BANDS`', and they have to
/// be, because a decision is written into that document by name. `aura-core` depends on no
/// workspace crate and so cannot read the array; `aura-app` can see both and asserts they
/// agree. ADR-0033 decision 3.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum HslBand {
    /// Reds. Where the deepest skin shadows and most wedding decor sit.
    #[default]
    Red,
    /// Oranges. **The skin band.** Most of a face, at every skin tone, is here.
    Orange,
    /// Yellows. Candlelight, brass, gold and the yellow half of foliage.
    Yellow,
    /// Greens. Foliage, and the single most common wedding colour problem.
    Green,
    /// Aquas. Pool water, some LED uplighting, and the cool edge of foliage.
    Aqua,
    /// Blues. Sky, denim, and the shadow side of anything lit by sky.
    Blue,
    /// Purples. Uplighting, and the band a dance floor lives in.
    Purple,
    /// Magentas. Flowers, saris, and the warm edge of stage lighting.
    Magenta,
}

impl HslBand {
    /// Every band, in the recipe's order.
    pub const ALL: [Self; 8] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Aqua,
        Self::Blue,
        Self::Purple,
        Self::Magenta,
    ];

    /// How many bands there are.
    pub const COUNT: usize = 8;

    /// The stable slug, which is also the recipe's key.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Aqua => "aqua",
            Self::Blue => "blue",
            Self::Purple => "purple",
            Self::Magenta => "magenta",
        }
    }

    /// The centre of this band's hue, in degrees on the colour wheel.
    ///
    /// Adobe's eight-band layout, which is what a photographer's muscle memory expects: red
    /// at 0, then every 45 degrees. The bands overlap when they are applied - a pixel at 30
    /// degrees is part red and part orange - and that overlap is the renderer's business.
    #[must_use]
    pub const fn centre_deg(self) -> f32 {
        match self {
            Self::Red => 0.0,
            Self::Orange => 45.0,
            Self::Yellow => 90.0,
            Self::Green => 135.0,
            Self::Aqua => 180.0,
            Self::Blue => 225.0,
            Self::Purple => 270.0,
            Self::Magenta => 315.0,
        }
    }

    /// The band a hue angle falls in.
    #[must_use]
    pub fn of_hue(degrees: f32) -> Self {
        let wrapped = degrees.rem_euclid(360.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = ((wrapped + 22.5) / 45.0).floor().max(0.0) as usize % Self::COUNT;
        Self::ALL.get(index).copied().unwrap_or(Self::Red)
    }

    /// Parse the stored slug. Unknown text is [`HslBand::Red`].
    #[must_use]
    pub fn from_str_or_red(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|band| band.as_str() == text)
            .unwrap_or(Self::Red)
    }

    /// True when this band carries most of a face.
    ///
    /// Only orange, and that is the point: a solver that could freely move red, yellow and
    /// orange would be a solver that can recolour skin in three steps of one band each. The
    /// skin guard measures the pixels rather than trusting this, but the bands nearest the
    /// face are also attenuated by default, which is the cheap half of the defence.
    #[must_use]
    pub const fn is_skin_band(self) -> bool {
        matches!(self, Self::Orange)
    }

    /// True when this band is adjacent to the skin band.
    #[must_use]
    pub const fn is_skin_adjacent(self) -> bool {
        matches!(self, Self::Red | Self::Yellow)
    }
}

impl fmt::Display for HslBand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One band's hue, saturation and luminance shift, in the recipe's units.
///
/// `-100 ..= 100` on every axis, and the three are independent: a solver that reduced the
/// saturation of foliage has not made a claim about its hue.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HslShift {
    /// Hue rotation within the band.
    pub h: f32,
    /// Saturation within the band.
    pub s: f32,
    /// Luminance within the band.
    pub l: f32,
}

impl HslShift {
    /// True when this band is untouched.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        self.h.abs() < f32::EPSILON && self.s.abs() < f32::EPSILON && self.l.abs() < f32::EPSILON
    }

    /// The magnitude of this shift, for the subtlety metric.
    ///
    /// Euclidean over the three axes rather than a sum, so that a band moved a little on
    /// every axis costs less than a band moved a lot on one - which matches what an expert
    /// edit looks like.
    #[must_use]
    pub fn magnitude(&self) -> f32 {
        (self.h * self.h + self.s * self.s + self.l * self.l).sqrt()
    }

    /// This shift with every axis clamped into the recipe's range.
    #[must_use]
    pub fn clamped(&self) -> Self {
        Self {
            h: self.h.clamp(PARAM_RANGE.0, PARAM_RANGE.1),
            s: self.s.clamp(PARAM_RANGE.0, PARAM_RANGE.1),
            l: self.l.clamp(PARAM_RANGE.0, PARAM_RANGE.1),
        }
    }

    /// This shift scaled by an attenuation factor, `0..1`.
    #[must_use]
    pub fn attenuated(&self, factor: f32) -> Self {
        let factor = factor.clamp(0.0, 1.0);
        Self {
            h: self.h * factor,
            s: self.s * factor,
            l: self.l * factor,
        }
    }
}

/// The eight bands, in the recipe's order.
///
/// A fixed array rather than a map, for phase 14's reason: the canonical serialisation sorts
/// keys and a typo'd band name would otherwise be carried silently through a hash.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HslAdjustments {
    /// One shift per band, indexed by [`HslBand::ALL`]'s order.
    pub bands: [HslShift; HslBand::COUNT],
}

impl HslAdjustments {
    /// One band's shift.
    #[must_use]
    pub fn get(&self, band: HslBand) -> HslShift {
        self.bands
            .get(band as usize)
            .copied()
            .unwrap_or(HslShift::default())
    }

    /// Set one band's shift.
    pub fn set(&mut self, band: HslBand, shift: HslShift) {
        if let Some(slot) = self.bands.get_mut(band as usize) {
            *slot = shift;
        }
    }

    /// True when nothing is adjusted at all.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        self.bands.iter().all(HslShift::is_neutral)
    }

    /// How many bands were moved.
    #[must_use]
    pub fn touched(&self) -> usize {
        self.bands.iter().filter(|s| !s.is_neutral()).count()
    }

    /// The root-mean-square magnitude over every band, normalised to `0..1`.
    ///
    /// The HSL half of the subtlety metric. Divided by the full band count rather than by
    /// the touched count, so that moving one band a lot and moving eight bands a little are
    /// not scored the same - the second is a filter and the first is an edit.
    #[must_use]
    pub fn magnitude(&self) -> f32 {
        let total: f32 = self
            .bands
            .iter()
            .map(|shift| shift.magnitude() * shift.magnitude())
            .sum();
        #[allow(clippy::cast_precision_loss)]
        let count = HslBand::COUNT as f32;
        (total / count).sqrt() / PARAM_RANGE.1
    }

    /// Every band clamped into the recipe's range.
    #[must_use]
    pub fn clamped(&self) -> Self {
        let mut out = *self;
        for slot in &mut out.bands {
            *slot = slot.clamped();
        }
        out
    }

    /// Every band scaled by an attenuation factor.
    #[must_use]
    pub fn attenuated(&self, factor: f32) -> Self {
        let mut out = *self;
        for slot in &mut out.bands {
            *slot = slot.attenuated(factor);
        }
        out
    }
}

// ---------------------------------------------------------------------------
// The curve
// ---------------------------------------------------------------------------

/// One control point of a tone curve, in the recipe's 0-255 input and output units.
///
/// Integers rather than floats, and they are the recipe's own units, so that a stored curve
/// and a rendered curve are the same numbers rather than two roundings of one number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CurvePoint {
    /// Input level, `0..=255`.
    pub x: u16,
    /// Output level, `0..=255`.
    pub y: u16,
}

impl CurvePoint {
    /// A point.
    #[must_use]
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

/// A monotonic point curve with between [`CURVE_MIN_POINTS`] and [`CURVE_MAX_POINTS`] control
/// points.
///
/// **The invariant is the type.** [`ToneCurve::new`] is the only constructor and it refuses
/// anything that is not strictly increasing in `x` and non-decreasing in `y`, anchored at 0
/// and 255. The renderer's Fritsch-Carlson interpolation is monotone exactly when its
/// control points are, so a curve that exists is a curve that cannot posterise or invert.
/// ADR-0033 decision 2, and section 6.1's "monotonicity is enforced structurally, so no AI
/// decision can ever produce a posterised or inverted curve".
///
/// There is deliberately **no evaluator here**. Two implementations of "what does this curve
/// do to a pixel" is two answers, and phase 14 already owns one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToneCurve {
    points: Vec<CurvePoint>,
}

impl Default for ToneCurve {
    fn default() -> Self {
        Self::identity()
    }
}

impl ToneCurve {
    /// The largest input or output level a point may have.
    pub const MAX_LEVEL: u16 = 255;

    /// The curve that changes nothing.
    ///
    /// Two points rather than four, and it is the one curve allowed to have fewer than
    /// [`CURVE_MIN_POINTS`]: "this frame needed no curve" has to be expressible, and
    /// expressing it as four collinear points would make `is_identity` a numerical question
    /// rather than a structural one.
    #[must_use]
    pub fn identity() -> Self {
        Self {
            points: vec![CurvePoint::new(0, 0), CurvePoint::new(255, 255)],
        }
    }

    /// Build a curve, refusing anything that is not monotone.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5066` when there are too few or too many points, when the first point is not
    /// at `x = 0`, when the last is not at `x = 255`, when `x` does not strictly increase,
    /// when `y` decreases anywhere, or when any level is above [`ToneCurve::MAX_LEVEL`].
    pub fn new(points: Vec<CurvePoint>) -> AuraResult<Self> {
        let refuse =
            |detail: &str| -> AuraError { crate::errors::ml::curve_refused(detail, points.len()) };
        if points.len() < 2 {
            return Err(refuse("a curve needs at least two points"));
        }
        if points.len() > CURVE_MAX_POINTS {
            return Err(refuse(
                "a curve with more than eight points is a local adjustment, not a global tone \
                 decision",
            ));
        }
        let Some(first) = points.first() else {
            return Err(refuse("a curve needs at least two points"));
        };
        let Some(last) = points.last() else {
            return Err(refuse("a curve needs at least two points"));
        };
        if first.x != 0 {
            return Err(refuse("the first control point must sit at x = 0"));
        }
        if last.x != Self::MAX_LEVEL {
            return Err(refuse("the last control point must sit at x = 255"));
        }
        for point in &points {
            if point.x > Self::MAX_LEVEL || point.y > Self::MAX_LEVEL {
                return Err(refuse("a control point is outside the 0..255 range"));
            }
        }
        for pair in points.windows(2) {
            let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            if b.x <= a.x {
                return Err(refuse(
                    "the control points must strictly increase in x; a curve that goes backwards \
                     has no correct interpretation",
                ));
            }
            if b.y < a.y {
                return Err(refuse(
                    "the control points must never decrease in y; a decrease inverts contrast in \
                     a band and reads as a bug in the renderer",
                ));
            }
        }
        Ok(Self { points })
    }

    /// The control points, in order.
    #[must_use]
    pub fn points(&self) -> &[CurvePoint] {
        &self.points
    }

    /// The control points as the recipe's own `[input, output]` pairs.
    #[must_use]
    pub fn recipe_points(&self) -> Vec<[u16; 2]> {
        self.points.iter().map(|p| [p.x, p.y]).collect()
    }

    /// True when this curve changes nothing.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.points
            .iter()
            .all(|point| point.x == point.y && point.x <= Self::MAX_LEVEL)
    }

    /// True when the point count is inside section 2.1's range.
    ///
    /// The identity curve is not, and that is correct: it is not a *fitted* curve.
    #[must_use]
    pub fn is_fitted(&self) -> bool {
        (CURVE_MIN_POINTS..=CURVE_MAX_POINTS).contains(&self.points.len())
    }

    /// The largest distance any control point sits from the identity line, `0..1`.
    ///
    /// The curve's contribution to the subtlety metric. A curve nobody would notice moves no
    /// point more than a few percent.
    #[must_use]
    pub fn magnitude(&self) -> f32 {
        self.points
            .iter()
            .map(|point| {
                (f32::from(point.y) - f32::from(point.x)).abs() / f32::from(Self::MAX_LEVEL)
            })
            .fold(0.0_f32, f32::max)
    }
}

// ---------------------------------------------------------------------------
// The skin guarantee
// ---------------------------------------------------------------------------

/// What grading actually did to the skin in one frame.
///
/// Section 5's `{ mask_area, max_hue_shift_deg, attenuation }`, plus the two things a
/// guarantee needs to be checkable: the chroma change the hue ceiling's sibling bounds, and
/// whether the measurement happened at all.
///
/// **`measured = false` is not `max_hue_shift_deg = 0`.** A frame with no faces has no skin
/// to protect and no measurement to report; a frame whose faces were too small to sample has
/// a real gap. Reporting either as a perfect score would make the guarantee's own coverage
/// invisible, which is the failure phase 09's `subject_aware` and phase 15's
/// `face_anchored` both exist to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkinGuardReport {
    /// The fraction of the frame the skin mask covers, `0..1`.
    pub mask_area: f32,
    /// The largest hue rotation any sampled skin region suffered, in degrees.
    pub max_hue_shift_deg: f32,
    /// The largest relative chroma change any sampled skin region suffered.
    pub max_chroma_change: f32,
    /// The factor every HSL, vibrance and saturation operation was scaled by inside the
    /// mask, `0..1`. One means nothing was attenuated.
    pub attenuation: f32,
    /// How many times the grade was re-solved to meet the ceilings.
    pub resolves: u8,
    /// True when there was skin to measure and it was measured.
    pub measured: bool,
}

impl SkinGuardReport {
    /// A report for a frame with nobody in it.
    #[must_use]
    pub const fn unmeasured() -> Self {
        Self {
            mask_area: 0.0,
            max_hue_shift_deg: 0.0,
            max_chroma_change: 0.0,
            attenuation: 1.0,
            resolves: 0,
            measured: false,
        }
    }

    /// True when both ceilings were met.
    ///
    /// An unmeasured frame passes, because there is nothing to fail - and
    /// [`SkinGuardReport::measured`] is what a caller checks before drawing a conclusion
    /// from it.
    #[must_use]
    pub fn within_ceilings(&self) -> bool {
        !self.measured
            || (self.max_hue_shift_deg <= SKIN_HUE_CEILING_DEG
                && self.max_chroma_change <= SKIN_CHROMA_CEILING)
    }

    /// True when the guard had to intervene.
    #[must_use]
    pub fn intervened(&self) -> bool {
        self.resolves > 0 || self.attenuation < 1.0
    }
}

// ---------------------------------------------------------------------------
// Content
// ---------------------------------------------------------------------------

/// A kind of thing a frame contains, for the HSL solver.
///
/// Six, and they are section 6.2's list. They are *inferred from colour statistics and
/// position*, not segmented - there is no segmentation model in this build and phase 18 is
/// where masks are built - so every decision that acts on one also carries
/// [`ColourCode::ContentInferred`]. ADR-0033 decision 4.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ContentBand {
    /// Foliage, grass, hedges. The classic wedding problem: usually too yellow-green and too
    /// saturated.
    #[default]
    Greenery,
    /// Open sky, in daylight or at blue hour.
    Sky,
    /// The dress, the tablecloth, the sheets - anything bright and near-neutral.
    Dress,
    /// Timber, tables, floors, the warm mid-saturation mid-luminance mass of a venue.
    Wood,
    /// Flowers, saris, uplighting, signage. Whatever is left and saturated.
    Decor,
    /// Skin. Reported so that the panel can show what the guard protected; **never
    /// adjusted**, which is why it has no target in `tone_intent.toml`.
    Skin,
}

impl ContentBand {
    /// Every band, in the order the config file documents them.
    pub const ALL: [Self; 6] = [
        Self::Greenery,
        Self::Sky,
        Self::Dress,
        Self::Wood,
        Self::Decor,
        Self::Skin,
    ];

    /// How many content bands there are.
    pub const COUNT: usize = 6;

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Greenery => "greenery",
            Self::Sky => "sky",
            Self::Dress => "dress",
            Self::Wood => "wood",
            Self::Decor => "decor",
            Self::Skin => "skin",
        }
    }

    /// Parse the stored slug. Unknown text is [`ContentBand::Decor`], which is the band that
    /// means "something saturated AURA could not name" and is therefore the safe default.
    #[must_use]
    pub fn from_str_or_decor(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|band| band.as_str() == text)
            .unwrap_or(Self::Decor)
    }

    /// True when this band may never be adjusted.
    #[must_use]
    pub const fn is_protected(self) -> bool {
        matches!(self, Self::Skin)
    }
}

impl fmt::Display for ContentBand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What was found of one kind of content in one frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BandReading {
    /// Which kind.
    pub band: ContentBand,
    /// The fraction of the frame it covers, `0..1`.
    pub area: f32,
    /// Its mean hue, in degrees.
    pub hue_deg: f32,
    /// Its mean saturation, `0..1`.
    pub saturation: f32,
    /// Its mean luminance, `0..1`.
    pub luma: f32,
    /// How sure the inference is, `0..1`. Below the analyser's floor the band is reported
    /// and not adjusted.
    pub confidence: f32,
}

// ---------------------------------------------------------------------------
// Variants
// ---------------------------------------------------------------------------

/// Which direction an alternative takes.
///
/// Section 6.4's three: "flatter, punchier, warmer". Named rather than numbered, because a
/// switcher that offers "alternative 2" is a switcher nobody uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariantKind {
    /// Less contrast, gentler curve, lifted blacks. What a photographer reaches for when a
    /// frame is already dramatic.
    Flatter,
    /// More contrast, deeper blacks, a stronger shoulder.
    Punchier,
    /// The same tone, shifted warm through the curve's shadows and the HSL's warm bands.
    /// **Not a white-balance change** - phase 15 owns the temperature and this cannot reach
    /// it.
    Warmer,
}

impl VariantKind {
    /// Every kind, in the order the panel offers them.
    pub const ALL: [Self; 3] = [Self::Flatter, Self::Punchier, Self::Warmer];

    /// How many kinds there are, and therefore the most alternatives a decision can carry.
    pub const COUNT: usize = 3;

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flatter => "flatter",
            Self::Punchier => "punchier",
            Self::Warmer => "warmer",
        }
    }

    /// Parse the stored slug.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    /// What the panel calls it.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::Flatter => "flatter",
            Self::Punchier => "punchier",
            Self::Warmer => "warmer",
        }
    }
}

impl fmt::Display for VariantKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A complete alternative grade.
///
/// **A whole parameter set, never a delta.** The clipping guard and the skin guard act on a
/// whole set, so a variant assembled by adding a delta to a guarded primary would be a set
/// nobody guarded - and "switch instantly" would mean "switch to something unchecked".
/// ADR-0033 decision 6.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ColourVariant {
    /// Which direction it takes.
    pub kind: VariantKind,
    /// Contrast, `-100..100`.
    pub contrast: f32,
    /// Highlight recovery, `-100..100`.
    pub highlights: f32,
    /// Shadow lift, `-100..100`.
    pub shadows: f32,
    /// White point, `-100..100`.
    pub whites: f32,
    /// Black point, `-100..100`.
    pub blacks: f32,
    /// Vibrance, `-100..100`.
    pub vibrance: f32,
    /// Saturation, `-100..100`.
    pub saturation: f32,
    /// Its own curve, which has been through the same monotonicity check.
    pub curve: ToneCurve,
    /// Its own HSL, which has been through the same skin guard.
    pub hsl: HslAdjustments,
    /// What it does to clipping: highlight fraction, shadow fraction.
    pub clipping_after: (f32, f32),
    /// Its skin guard report. Every variant is guarded.
    pub skin_guard: SkinGuardReport,
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why a photograph was graded the way it was, as a closed set.
///
/// Twenty-nine codes. **Thirteen of them withdraw a claim rather than making one** - they say the
/// product declined to grade, or graded less than it wanted to, and this phase needs that
/// distinction more than any before it: section 12's first failure mode is over-graded,
/// HDR-looking results, and the defence against it is a solver that can say "I stopped".
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ColourCode {
    // -- tone ------------------------------------------------------------
    /// The frame was already at the contrast this kind of photograph wants. **A
    /// withdrawal**, and what a frame that needed nothing carries.
    #[default]
    ToneAlreadyRight,
    /// Contrast was added to reach the scene's intent.
    ContrastAdded,
    /// Contrast was reduced because the frame was already harder than the scene wants.
    ContrastReduced,
    /// Highlights were pulled back to recover detail.
    HighlightsRecovered,
    /// Shadows were lifted.
    ShadowsLifted,
    /// The lift stopped short of the intent because phase 09's noise estimate said the
    /// shadows would not carry it. **A withdrawal**, and section 6.4's noise guard.
    ShadowLiftHeldForNoise,
    /// The white point was moved to place the brightest meaningful tone.
    WhitesSet,
    /// The black point was moved to place the darkest meaningful tone.
    BlacksSet,
    /// Nothing measurable was found and the tone was left alone. **A withdrawal.**
    ToneUnavailable,

    // -- curve -----------------------------------------------------------
    /// A curve was fitted to hit the scene's subject-contrast intent.
    CurveFitted,
    /// The curve was flattened because the fitted one would have clipped. **A withdrawal.**
    CurveFlattenedForClipping,
    /// No curve was needed. **A withdrawal.**
    CurveIdentity,

    // -- content and HSL -------------------------------------------------
    /// The content bands were inferred from colour statistics rather than segmented.
    ///
    /// **A withdrawal**, and it is on every frame this phase adjusts an HSL band on. When
    /// phase 18's masks exist this code stops being emitted and the sentence stops being
    /// said.
    ContentInferred,
    /// Foliage was pulled back from yellow-green toward green and desaturated a little.
    GreeneryTamed,
    /// The sky's blue was deepened slightly.
    SkyDeepened,
    /// Wood and warm venue tones were brought toward the scene's target.
    WoodWarmed,
    /// The most distracting saturated colour in the frame was reduced.
    DistractorTamed,
    /// Two saturated hues were competing with the subject.
    HueConflictFound,
    /// A near-neutral bright region - a dress, a tablecloth - was kept neutral.
    DressProtected,
    /// A band was found but not adjusted, because the inference was not confident enough.
    /// **A withdrawal.**
    BandUnconfident,
    /// No band needed adjusting. **A withdrawal.**
    HslNeutral,

    // -- the skin guarantee ----------------------------------------------
    /// Skin was excluded or attenuated inside every colour operation.
    SkinProtected,
    /// The grade was re-solved because the first answer moved skin too far.
    SkinGuardResolved,
    /// The ceilings could not be met, so the colour operations were withdrawn entirely.
    /// **A withdrawal**, and the strongest one in the phase.
    SkinGuardWithdrew,
    /// There was no skin in the frame to protect. **A withdrawal**, because it says the
    /// guarantee did not apply rather than that it was met.
    NoSkinInFrame,

    // -- guards and provenance -------------------------------------------
    /// A parameter was reduced because the grade would have introduced new clipping.
    ClippingGuardResolved,
    /// The total adjustment was scaled back to stay inside the subtlety ceiling. **A
    /// withdrawal.**
    SubtletyCapped,
    /// This scene has no intent row and the neutral one was used. **A withdrawal.**
    NoIntentRow,
    /// The photographer set this by hand. **A withdrawal**, and the only unbeatable one.
    UserOverride,
}

impl ColourCode {
    /// Every code, in the order `docs/tone-and-colour.md` documents them.
    pub const ALL: [Self; 29] = [
        Self::ToneAlreadyRight,
        Self::ContrastAdded,
        Self::ContrastReduced,
        Self::HighlightsRecovered,
        Self::ShadowsLifted,
        Self::ShadowLiftHeldForNoise,
        Self::WhitesSet,
        Self::BlacksSet,
        Self::ToneUnavailable,
        Self::CurveFitted,
        Self::CurveFlattenedForClipping,
        Self::CurveIdentity,
        Self::ContentInferred,
        Self::GreeneryTamed,
        Self::SkyDeepened,
        Self::WoodWarmed,
        Self::DistractorTamed,
        Self::HueConflictFound,
        Self::DressProtected,
        Self::BandUnconfident,
        Self::HslNeutral,
        Self::SkinProtected,
        Self::SkinGuardResolved,
        Self::SkinGuardWithdrew,
        Self::NoSkinInFrame,
        Self::ClippingGuardResolved,
        Self::SubtletyCapped,
        Self::NoIntentRow,
        Self::UserOverride,
    ];

    /// How many reason codes there are.
    pub const COUNT: usize = 29;

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ToneAlreadyRight => "tone_already_right",
            Self::ContrastAdded => "contrast_added",
            Self::ContrastReduced => "contrast_reduced",
            Self::HighlightsRecovered => "highlights_recovered",
            Self::ShadowsLifted => "shadows_lifted",
            Self::ShadowLiftHeldForNoise => "shadow_lift_held_for_noise",
            Self::WhitesSet => "whites_set",
            Self::BlacksSet => "blacks_set",
            Self::ToneUnavailable => "tone_unavailable",
            Self::CurveFitted => "curve_fitted",
            Self::CurveFlattenedForClipping => "curve_flattened_for_clipping",
            Self::CurveIdentity => "curve_identity",
            Self::ContentInferred => "content_inferred",
            Self::GreeneryTamed => "greenery_tamed",
            Self::SkyDeepened => "sky_deepened",
            Self::WoodWarmed => "wood_warmed",
            Self::DistractorTamed => "distractor_tamed",
            Self::HueConflictFound => "hue_conflict_found",
            Self::DressProtected => "dress_protected",
            Self::BandUnconfident => "band_unconfident",
            Self::HslNeutral => "hsl_neutral",
            Self::SkinProtected => "skin_protected",
            Self::SkinGuardResolved => "skin_guard_resolved",
            Self::SkinGuardWithdrew => "skin_guard_withdrew",
            Self::NoSkinInFrame => "no_skin_in_frame",
            Self::ClippingGuardResolved => "clipping_guard_resolved",
            Self::SubtletyCapped => "subtlety_capped",
            Self::NoIntentRow => "no_intent_row",
            Self::UserOverride => "user_override",
        }
    }

    /// Parse the stored slug. Unknown text is [`ColourCode::ToneAlreadyRight`].
    #[must_use]
    pub fn from_str_or_already_right(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|code| code.as_str() == text)
            .unwrap_or(Self::ToneAlreadyRight)
    }

    /// True when this code withdraws a claim rather than making one.
    #[must_use]
    pub const fn is_withdrawal(self) -> bool {
        matches!(
            self,
            Self::ToneAlreadyRight
                | Self::ShadowLiftHeldForNoise
                | Self::ToneUnavailable
                | Self::CurveFlattenedForClipping
                | Self::CurveIdentity
                | Self::ContentInferred
                | Self::BandUnconfident
                | Self::HslNeutral
                | Self::SkinGuardWithdrew
                | Self::NoSkinInFrame
                | Self::SubtletyCapped
                | Self::NoIntentRow
                | Self::UserOverride
        )
    }

    /// The sentence this code says when nothing more specific is known.
    ///
    /// **Stored rows carry the code, not the sentence**, for phase 09's three reasons: size,
    /// copy that changes without behaviour changing, and translation.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::ToneAlreadyRight => {
                "this frame was already at the contrast this kind of photograph wants, so the \
                 tone was left where it was"
            }
            Self::ContrastAdded => {
                "contrast was added, because this kind of photograph is usually a little more \
                 separated than the camera recorded it"
            }
            Self::ContrastReduced => {
                "contrast was reduced, because the light here was already harder than this kind \
                 of photograph carries"
            }
            Self::HighlightsRecovered => {
                "the brightest areas were pulled back so the detail in them survives"
            }
            Self::ShadowsLifted => "the darkest areas were opened up",
            Self::ShadowLiftHeldForNoise => {
                "the shadows were opened less than usual, because lifting them further would \
                 have made this frame noisy"
            }
            Self::WhitesSet => "the white point was placed on the brightest real tone in the frame",
            Self::BlacksSet => "the black point was placed on the darkest real tone in the frame",
            Self::ToneUnavailable => {
                "AURA could not measure this frame well enough to grade it, so it was left as it \
                 was"
            }
            Self::CurveFitted => {
                "a tone curve was drawn to give the people in this frame the separation this kind \
                 of photograph wants"
            }
            Self::CurveFlattenedForClipping => {
                "the curve was made gentler than intended, because the stronger one would have \
                 clipped the brightest areas"
            }
            Self::CurveIdentity => "this frame did not need a curve",
            Self::ContentInferred => {
                "the greenery, sky and decor here were worked out from colour rather than \
                 outlined, so the colour adjustments are deliberately small"
            }
            Self::GreeneryTamed => {
                "the foliage was brought back from yellow-green toward green and calmed down a \
                 little"
            }
            Self::SkyDeepened => "the sky was deepened slightly",
            Self::WoodWarmed => "the wood and warm surfaces were brought toward each other",
            Self::DistractorTamed => {
                "the most distracting colour in the frame was reduced, rather than the whole \
                 frame being desaturated"
            }
            Self::HueConflictFound => {
                "two strong colours here were competing with the people in the frame"
            }
            Self::DressProtected => {
                "the bright near-white areas were kept neutral rather than picking up a colour \
                 cast"
            }
            Self::BandUnconfident => {
                "AURA was not sure enough about what one of these colours belongs to, so it left \
                 it alone"
            }
            Self::HslNeutral => "no colour in this frame needed adjusting",
            Self::SkinProtected => {
                "every colour adjustment was held back over skin, so nobody's colour moved"
            }
            Self::SkinGuardResolved => {
                "the first grade moved skin further than AURA allows, so it was worked out again \
                 more gently"
            }
            Self::SkinGuardWithdrew => {
                "the colour adjustments were dropped entirely, because there was no version of \
                 them that left skin where it was"
            }
            Self::NoSkinInFrame => {
                "there is nobody in this frame, so the skin protection did not apply"
            }
            Self::ClippingGuardResolved => {
                "one setting was reduced, because the grade would have clipped more of this frame \
                 than this kind of photograph accepts"
            }
            Self::SubtletyCapped => {
                "the whole grade was scaled back, because everything together would have looked \
                 processed"
            }
            Self::NoIntentRow => {
                "AURA has no grading guidance recorded for this kind of photograph yet, so it \
                 graded cautiously"
            }
            Self::UserOverride => "you set this, and AURA has not changed it",
        }
    }
}

impl fmt::Display for ColourCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One thing that moved a grade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ColourReason {
    /// The stable code.
    pub code: ColourCode,
    /// The sentence, rendered from the code and whatever numbers it names.
    pub text: String,
    /// How much confidence it cost. Negative is doubt.
    pub weight: f32,
    /// The pixels behind it, when the reason is about a region.
    ///
    /// Section 9 gives QAIQ "expert review of 400 graded frames: subtlety, skin, greenery,
    /// dress texture", and a reason about greenery that cannot point at the greenery is a
    /// reason nobody can check.
    pub evidence: Option<CropRect>,
}

impl ColourReason {
    /// A reason about the whole frame.
    #[must_use]
    pub fn frame(code: ColourCode, text: impl Into<String>, weight: f32) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: None,
        }
    }

    /// A reason about a region.
    #[must_use]
    pub fn at(code: ColourCode, text: impl Into<String>, weight: f32, evidence: CropRect) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: Some(evidence),
        }
    }

    /// A reason whose sentence is the code's own.
    #[must_use]
    pub fn plain(code: ColourCode, weight: f32) -> Self {
        Self {
            code,
            text: code.user_text().to_string(),
            weight,
            evidence: None,
        }
    }

    /// True when this reason cost confidence.
    #[must_use]
    pub fn is_doubt(&self) -> bool {
        self.weight < 0.0
    }
}

// ---------------------------------------------------------------------------
// The decision
// ---------------------------------------------------------------------------

/// Everything phase 16 decided about one photograph's tone and colour.
///
/// PHASE-16 section 5's frozen shape, plus the fields sections 6.1 to 6.4 need and section 5
/// does not name: the scene the intents came from, the content readings the HSL solver acted
/// on, the subtlety figure section 12's first failure mode is defended by, the two bits that
/// protect a photographer's own answer, and the three versions.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct ColourDecision {
    /// The photograph.
    pub image_id: ImageId,
    /// Contrast, `-100..100`.
    pub contrast: f32,
    /// Highlight recovery, `-100..100`. Negative pulls highlights down.
    pub highlights: f32,
    /// Shadow lift, `-100..100`. Positive opens shadows.
    pub shadows: f32,
    /// White point, `-100..100`.
    pub whites: f32,
    /// Black point, `-100..100`.
    pub blacks: f32,
    /// The fitted point curve. Monotone by construction.
    pub curve: ToneCurve,
    /// Per-band hue, saturation and luminance shifts.
    pub hsl: HslAdjustments,
    /// Saturation weighted away from already-saturated colours, `-100..100`.
    pub vibrance: f32,
    /// Flat saturation, `-100..100`.
    pub saturation: f32,
    /// What grading did to the skin, measured.
    pub skin_guard: SkinGuardReport,
    /// Clipping after the grade: highlight fraction, shadow fraction.
    ///
    /// Section 5's `(f32, f32)`. The *absolute* fractions rather than the additions, because
    /// section 10.1's gate is "no frame gains new clipping beyond its scene tolerance" and
    /// the comparison needs a before as well as an after -
    /// [`ColourDecision::clipping_before`] is the other half.
    pub clipping_after: (f32, f32),
    /// Clipping before the grade, measured on the same pixels.
    pub clipping_before: (f32, f32),
    /// What the frame contains, as the HSL solver read it.
    pub bands: Vec<BandReading>,
    /// Complete alternatives, at most [`VariantKind::COUNT`].
    pub alternatives: Vec<ColourVariant>,
    /// Why, at most [`ColourDecision::MAX_REASONS`], strongest doubt first.
    pub reasons: Vec<ColourReason>,
    /// How sure the grade is, `0..1`. Invariant 2.
    pub confidence: f32,
    /// The total adjustment magnitude, `0..1`. Lower is subtler.
    pub subtlety: f32,
    /// Which scene the intents were conditioned on. Invariant 7.
    pub scene: SceneId,
    /// True when there was skin in the frame and the guard was measured on it.
    ///
    /// **This phase's most important single bit**, for the reason `face_anchored` is phase
    /// 15's: the guarantee in the headline KPI is about skin, and a wedding at 100 %
    /// coverage and 3 % skin-measured has had its guarantee checked on almost nothing.
    pub skin_measured: bool,
    /// True when the photographer set these values by hand.
    pub user_edited: bool,
    /// True when the photographer has looked at this frame in the review queue.
    pub reviewed: bool,
    /// Which learned head produced the prediction.
    pub model_ver: u16,
    /// Which build's arithmetic produced the solve.
    pub analysis_ver: u16,
    /// Which intent table the targets came from.
    pub intent_ver: u16,
}

impl ColourDecision {
    /// The most reasons one decision carries.
    ///
    /// Six, matching every phase since 09.
    pub const MAX_REASONS: usize = 6;

    /// True when this frame belongs in the low-confidence review queue.
    #[must_use]
    pub fn needs_review(&self) -> bool {
        !self.reviewed && !self.user_edited && self.confidence < REVIEW_BELOW
    }

    /// How much new highlight clipping the grade added.
    #[must_use]
    pub fn highlight_clipping_added(&self) -> f32 {
        (self.clipping_after.0 - self.clipping_before.0).max(0.0)
    }

    /// How much new shadow clipping the grade added.
    #[must_use]
    pub fn shadow_clipping_added(&self) -> f32 {
        (self.clipping_after.1 - self.clipping_before.1).max(0.0)
    }

    /// The reasons that cost confidence, strongest first.
    #[must_use]
    pub fn doubts(&self) -> Vec<&ColourReason> {
        let mut out: Vec<&ColourReason> = self.reasons.iter().filter(|r| r.is_doubt()).collect();
        out.sort_by(|left, right| left.weight.total_cmp(&right.weight));
        out
    }

    /// The reasons that withdrew a claim, in the order they were made.
    #[must_use]
    pub fn withdrawals(&self) -> Vec<&ColourReason> {
        self.reasons
            .iter()
            .filter(|reason| reason.code.is_withdrawal())
            .collect()
    }

    /// One alternative, by kind.
    #[must_use]
    pub fn variant(&self, kind: VariantKind) -> Option<&ColourVariant> {
        self.alternatives.iter().find(|v| v.kind == kind)
    }

    /// One content band's reading.
    #[must_use]
    pub fn band(&self, band: ContentBand) -> Option<&BandReading> {
        self.bands.iter().find(|reading| reading.band == band)
    }

    /// True when the three version fields match this build's.
    #[must_use]
    pub const fn matches_versions(&self, model: u16, analysis: u16, intent: u16) -> bool {
        self.model_ver == model && self.analysis_ver == analysis && self.intent_ver == intent
    }

    /// True when this decision met section 10.1's two skin ceilings.
    #[must_use]
    pub fn skin_within_ceilings(&self) -> bool {
        self.skin_guard.within_ceilings()
            && self
                .alternatives
                .iter()
                .all(|variant| variant.skin_guard.within_ceilings())
    }
}

// ---------------------------------------------------------------------------
// The outline
// ---------------------------------------------------------------------------

/// How much of a wedding has been graded, and what the grade looked like.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ColourOutline {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs with a stored decision.
    pub decided: u32,
    /// `decided / photos`, `0..1`.
    ///
    /// The denominator is **every photograph**, as phases 09, 11, 14 and 15's are.
    pub coverage: f64,
    /// Photographs where there was skin to protect and it was measured.
    ///
    /// The number that matters when it is low. See [`ColourDecision::skin_measured`].
    pub skin_measured: u32,
    /// Photographs where the skin guard had to intervene.
    pub skin_guard_triggered: u32,
    /// Photographs where the clipping guard re-solved.
    pub clip_guard_resolved: u32,
    /// Photographs whose grade was capped by the subtlety ceiling.
    pub subtlety_capped: u32,
    /// Photographs below [`REVIEW_BELOW`].
    pub needs_review: u32,
    /// Photographs the photographer has set by hand.
    pub user_edited: u32,
    /// Mean contrast over decided frames.
    pub mean_contrast: f32,
    /// Mean shadow lift over decided frames.
    pub mean_shadow_lift: f32,
    /// Mean subtlety over decided frames.
    pub mean_subtlety: f32,
    /// The largest skin hue shift anywhere in the project, in degrees.
    ///
    /// The one number that falsifies the phase's headline guarantee, so it is reported
    /// rather than derived: a project whose worst frame moved skin three degrees has broken
    /// the promise even if its mean is perfect.
    pub worst_skin_hue_shift: f32,
    /// Scenes graded against the neutral intent row, by slug.
    pub untargeted_scenes: Vec<String>,
    /// The lowest model version present.
    pub model_ver: u16,
    /// The lowest analysis version present.
    pub analysis_ver: u16,
    /// The lowest intent-table version present.
    pub intent_ver: u16,
}

impl ColourOutline {
    /// The fraction of decided frames where skin was measured.
    #[must_use]
    pub fn skin_measured_ratio(&self) -> f64 {
        if self.decided == 0 {
            return 0.0;
        }
        f64::from(self.skin_measured) / f64::from(self.decided)
    }

    /// True when every stored decision met both ceilings.
    #[must_use]
    pub fn guarantee_held(&self) -> bool {
        self.worst_skin_hue_shift <= SKIN_HUE_CEILING_DEG
    }
}

// ---------------------------------------------------------------------------
// The override
// ---------------------------------------------------------------------------

/// What the photographer set instead.
///
/// Every field is optional and independent, exactly as [`crate::contract::tone::ToneOverride`]
/// is: somebody who reduced the contrast has not made a claim about the greenery.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ColourOverride {
    /// Contrast.
    pub contrast: Option<f32>,
    /// Highlight recovery.
    pub highlights: Option<f32>,
    /// Shadow lift.
    pub shadows: Option<f32>,
    /// White point.
    pub whites: Option<f32>,
    /// Black point.
    pub blacks: Option<f32>,
    /// Vibrance.
    pub vibrance: Option<f32>,
    /// Flat saturation.
    pub saturation: Option<f32>,
    /// The whole curve, or nothing. A curve is not a set of independent numbers.
    pub curve: Option<ToneCurve>,
    /// The whole HSL block, or nothing, for the same reason.
    pub hsl: Option<HslAdjustments>,
}

impl ColourOverride {
    /// True when nothing was set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.contrast.is_none()
            && self.highlights.is_none()
            && self.shadows.is_none()
            && self.whites.is_none()
            && self.blacks.is_none()
            && self.vibrance.is_none()
            && self.saturation.is_none()
            && self.curve.is_none()
            && self.hsl.is_none()
    }

    /// The dotted recipe paths this override owns.
    ///
    /// What `aura_recipe::schema::merge` adds to `user_edited_fields`, listed here so that
    /// the contract and the command module cannot disagree about which paths a person now
    /// owns.
    #[must_use]
    pub fn recipe_paths(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (set, path) in [
            (self.contrast.is_some(), "global.contrast"),
            (self.highlights.is_some(), "global.highlights"),
            (self.shadows.is_some(), "global.shadows"),
            (self.whites.is_some(), "global.whites"),
            (self.blacks.is_some(), "global.blacks"),
            (self.vibrance.is_some(), "global.vibrance"),
            (self.saturation.is_some(), "global.saturation"),
            (self.curve.is_some(), "global.curve.points"),
            (self.hsl.is_some(), "global.hsl"),
        ] {
            if set {
                out.push(path.to_string());
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The only way to ask how a photograph should be graded.
///
/// Twelfth service of its kind. Phase 17 shifts these values toward a photographer's own
/// style, 18 grades locally on top of them, 25 normalises a gallery of them, 27 checks them
/// and 29 builds albums out of them. **No phase may keep its own tone curve, its own HSL
/// solver or its own idea of what protects skin**: two answers to "how much contrast does
/// this frame want" is an album that does not match the gallery.
pub trait ColourService: Send + Sync + fmt::Debug {
    /// How much of a project has been graded.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    fn outline(&self, project: ProjectId) -> AuraResult<ColourOutline>;

    /// One photograph's decision, or `None` when nobody has graded it.
    ///
    /// `None` is not a neutral grade, and a caller that renders it as one has turned a gap
    /// in coverage into a decision.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    fn of_image(&self, image: ImageId) -> AuraResult<Option<ColourDecision>>;

    /// The frames whose grade is worth a photographer's attention, weakest first.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>>;

    /// Record that the photographer has looked at one decision and agrees.
    ///
    /// It does not set `user_edited`: accepting a suggestion is not authoring one, and phase
    /// 30's learning loop needs to tell those apart.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5067` when the photograph has no decision.
    fn accept(&self, image: ImageId) -> Result<(), AuraError>;

    /// Record what the photographer set instead.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5067` when the photograph has no decision, the override is empty, or a value
    /// is outside its documented range.
    fn set_override(&self, image: ImageId, values: ColourOverride) -> Result<(), AuraError>;

    /// Promote one stored alternative to the primary grade.
    ///
    /// Section 6.4's "switch instantly without recomputation". It is safe because every
    /// variant has already been through the clipping guard and the skin guard - ADR-0033
    /// decision 6 - so the promoted set is one somebody checked.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5067` when the photograph has no decision or no variant of that kind.
    fn select_variant(&self, image: ImageId, kind: VariantKind) -> Result<(), AuraError>;
}
