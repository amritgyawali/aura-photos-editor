//! FROZEN CONTRACT. A photographer's own look, learned from weddings they already
//! delivered, and conditioned on what the photograph is of and what light it was made in.
//!
//! PHASE-17 section 5 freezes [`StyleProfile`], [`StyleDelta`] and [`ProfileDiagnostics`]
//! before any fitter exists. The file is in `aura-core` rather than in `aura-style` for the
//! reason [`crate::contract::tone`] and [`crate::contract::colour`] are: the phases that
//! *consume* a style are 25 (gallery consistency), 26 (shooter matching), 27 (QC), 28
//! (autopilot) and 30 (the learning loop), and none of them needs the pair matcher, the
//! recipe fitter or the regression.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one thing to understand before reading the rest
//!
//! **A style is a residual, and the baseline underneath it is never re-derived.** Every
//! number in [`StyleDelta`] is *added* to what phases 15 and 16 already decided about this
//! photograph. Nothing here can set an exposure, a temperature or a contrast; it can only say
//! how far this photographer sits from the consensus. That is why three hundred pairs are
//! enough (section 6.2), and it is also the phase's safety property: an empty profile, an
//! unpopulated bucket and a photographer who has taught AURA nothing all produce **exactly
//! the baseline**. There is no state of this system in which the feature makes a photograph
//! worse than switching it off would.
//!
//! ## The second thing: the shift happens before the guards, not after
//!
//! A [`StyleDelta`] is applied to the parameters phases 15 and 16 *solved*, and then phase
//! 15's skin-locus constraint and phase 16's clipping guard and skin guard run on the result.
//! A style that would move somebody's skin past [`crate::contract::colour::SKIN_HUE_CEILING_DEG`]
//! is a style the guard attenuates or withdraws. ADR-0035 decision 1, and phase 16's exit
//! report wrote the rule down before this phase existed: "the skin guard runs last, and it
//! runs after phase 17's shift too."
//!
//! ## The third thing: there is no skin target here either
//!
//! [`SkinBias`] is two bounded scalars that say "this photographer runs skin a little warmer
//! and a little less saturated than the consensus". It is a *difference from the baseline's
//! answer about this frame's own people*, exactly as phase 16's guarantee is a difference
//! from this frame's own skin. There is no chromaticity, no bucket, no category and no
//! constructor anywhere in this contract that could hold one, which is the same structural
//! defence [`crate::contract::tone::SkinLocus`] makes and the third time this product has
//! made it. ADR-0035 decision 7.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::colour::{CurvePoint, HslAdjustments, HslBand, ToneCurve};
use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{ProfileId, ProjectId};
use crate::contract::scene::{ChapterId, SceneId, Timestamp};

/// PHASE-07 names this `ImageId`; the catalog calls it a `PhotoId`. One type, re-exported so
/// a consumer of this contract does not have to reach into phase 07's to name a photograph.
pub use crate::contract::scene::ImageId;

// ---------------------------------------------------------------------------
// The numbers section 10.1 is measured against
// ---------------------------------------------------------------------------

/// The style-match error a trained bucket must reach on held-out pairs.
///
/// Section 10.1: "style match dE00 <= 2.5 on held-out pairs per photographer". Two and a half
/// dE00 is around the point at which a side-by-side of two versions of the same frame stops
/// reading as two edits and starts reading as one edit and a rendering difference.
pub const MATCH_DE00_CEILING: f32 = 2.5;

/// The style-match error a *weak* bucket must still reach.
///
/// Section 10.1's second half: "<= 3.5 in weak buckets". A bucket that is answering out of its
/// parent group is allowed to be worse, and it is not allowed to be arbitrary.
pub const WEAK_MATCH_DE00_CEILING: f32 = 3.5;

/// The fewest pairs a profile needs before it is worth adopting.
///
/// Three hundred, which is the headline KPI's own number and the whole point of the residual
/// formulation. Below it the product does not refuse - it reports a profile strength and lets
/// the photographer decide, because refusing at 299 pairs is a cliff nobody can explain.
pub const USABLE_PAIRS: u32 = 300;

/// At or below this many samples a bucket is called weak and named in the report.
///
/// Ten, section 10.1's own "sparse bucket (< 10 samples)" rounded to an inclusive bound so
/// that "weak" and "sparse" are one threshold rather than two that differ by one.
pub const WEAK_BUCKET_SAMPLES: u32 = 10;

/// The largest exposure shift a style may add, in stops.
///
/// Two thirds of a stop. A photographer whose galleries sit consistently brighter than the
/// consensus is expressing a taste; a two-stop difference is a different exposure decision
/// and phase 15 owns that. The bound is what stops a badly matched archive from teaching AURA
/// to overexpose a wedding.
pub const MAX_EXPOSURE_DELTA_EV: f32 = 0.67;

/// The largest colour-temperature shift a style may add, in kelvin.
///
/// Eight hundred. Enough to express "I run warm", not enough to relitigate what colour the
/// light was - which is phase 15's answer, made from this wedding's own neutrals and the
/// people in the frame.
pub const MAX_TEMPERATURE_DELTA_K: f32 = 800.0;

/// The largest tint shift a style may add, in the recipe's tint units.
pub const MAX_TINT_DELTA: f32 = 12.0;

/// The largest shift a style may add to any one of the recipe's `-100..100` parameters.
///
/// Thirty-five points. The same argument as the exposure bound, one scale down: a taste is a
/// consistent lean and anything past a third of the range is a different decision wearing a
/// style's clothes.
pub const MAX_PARAM_DELTA: f32 = 35.0;

/// The largest shift a style may add to any one HSL band's hue, saturation or luminance.
pub const MAX_HSL_DELTA: f32 = 20.0;

/// The largest shift a style may add to a band that carries skin, as a fraction of
/// [`MAX_HSL_DELTA`].
///
/// A quarter. The cheap half of the skin defence, and the same one phase 16 applies to its own
/// solver: the expensive half is that phase 16's guard measures the pixels afterwards and can
/// withdraw the whole colour half. Two defences rather than one, because this is the phase
/// where a photographer's own taste is the thing pushing.
pub const SKIN_BAND_FRACTION: f32 = 0.25;

/// The largest curve control-point offset a style may add, in the recipe's 0-255 units.
///
/// Twenty-four levels, just under a tenth of the range. A curve offset larger than this is not
/// a lean, it is a different curve, and phase 16's fitter is what draws curves.
pub const MAX_CURVE_OFFSET: f32 = 24.0;

/// The largest skin warmth bias a style may express, in degrees of hue.
///
/// One and a half degrees, deliberately **below** phase 16's
/// [`crate::contract::colour::SKIN_HUE_CEILING_DEG`] of two. A style cannot ask for a skin
/// movement larger than the guarantee allows, so the bound is checked here and measured there.
pub const MAX_SKIN_WARMTH_DEG: f32 = 1.5;

/// The largest skin chroma bias a style may express, as a fraction.
///
/// Four percent, below phase 16's six for the same reason.
pub const MAX_SKIN_CHROMA_BIAS: f32 = 0.04;

/// Below this confidence a bucket's delta is not applied at all.
///
/// A bucket AURA is not sure about produces the parent's answer rather than a hedged version
/// of its own, because a half-applied style is a look that belongs to nobody.
pub const APPLY_ABOVE: f32 = 0.35;

/// The largest a serialised profile bundle may be, in bytes.
///
/// Section 11's "profile size <= 3 MB". A profile is a few hundred coefficients per bucket; a
/// bundle approaching this is a bundle carrying something that is not a profile, and refusing
/// it before parsing is how a malformed import stays a refusal rather than an allocation.
pub const MAX_BUNDLE_BYTES: u64 = 3 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Buckets
// ---------------------------------------------------------------------------

/// The coarse family of photograph a style bucket belongs to.
///
/// Eight groups over phase 07's twenty-two scenes. They exist because a photographer's taste
/// generalises across "the ceremony" far better than it generalises across "the wedding":
/// somebody who runs ceremonies cool and receptions warm is expressing two decisions, and a
/// group is the level at which a sparse bucket borrows from something that is actually like
/// it. [`StyleBucket`] resolves upward through here on its way to the global delta.
///
/// The mapping is **closed and lives in code**, exactly as [`SceneId`] is, and for the same
/// reason: a group is a `match` arm, a row in `profile_buckets` and the shrinkage tree's
/// second level. Adding one is a migration.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum SceneGroup {
    /// Both halves of the couple preparing.
    Preparation,
    /// Objects and the empty venue - rings, shoes, flowers, the room before anybody is in it.
    Details,
    /// The ceremony and every named rite inside it.
    Ceremony,
    /// Posed photography, including the first look and golden hour.
    Portraits,
    /// The meal, the speeches and the cake.
    Reception,
    /// The first dance and the dance floor.
    Dance,
    /// Unposed frames and the departure.
    Candid,
    /// Something the vocabulary does not name. The default, and never a failure.
    #[default]
    Other,
}

impl SceneGroup {
    /// Every group, in wedding order. [`SceneGroup::Other`] is last and is not part of it.
    pub const ALL: [Self; 8] = [
        Self::Preparation,
        Self::Details,
        Self::Ceremony,
        Self::Portraits,
        Self::Reception,
        Self::Dance,
        Self::Candid,
        Self::Other,
    ];

    /// How many groups there are.
    pub const COUNT: usize = 8;

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preparation => "preparation",
            Self::Details => "details",
            Self::Ceremony => "ceremony",
            Self::Portraits => "portraits",
            Self::Reception => "reception",
            Self::Dance => "dance",
            Self::Candid => "candid",
            Self::Other => "other",
        }
    }

    /// What the bucket matrix calls it.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Preparation => "Preparation",
            Self::Details => "Details",
            Self::Ceremony => "Ceremony",
            Self::Portraits => "Portraits",
            Self::Reception => "Reception",
            Self::Dance => "Dance",
            Self::Candid => "Candid",
            Self::Other => "Other",
        }
    }

    /// Parse the stored slug. Unknown text is [`SceneGroup::Other`], for the reason
    /// [`SceneId::from_str_or_unknown`] gives: a catalog written by a newer build must open.
    #[must_use]
    pub fn from_str_or_other(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|group| group.as_str() == text)
            .unwrap_or(Self::Other)
    }

    /// The group one of phase 07's scenes belongs to.
    ///
    /// [`SceneId::GoldenHour`] is in [`SceneGroup::Portraits`] rather than in a group of its
    /// own, because golden hour is a *light* and this axis is about subject. The lighting axis
    /// is [`LightingBucket`], and [`LightingBucket::GoldenHour`] is where it lives.
    #[must_use]
    pub const fn of_scene(scene: SceneId) -> Self {
        match scene {
            SceneId::GettingReadyBride | SceneId::GettingReadyGroom => Self::Preparation,
            SceneId::Details | SceneId::Venue => Self::Details,
            SceneId::CeremonyEntrance
            | SceneId::Ceremony
            | SceneId::Ritual
            | SceneId::Vows
            | SceneId::Rings
            | SceneId::Kiss => Self::Ceremony,
            SceneId::FirstLook
            | SceneId::FamilyPortrait
            | SceneId::GroupPortrait
            | SceneId::CouplePortrait
            | SceneId::GoldenHour => Self::Portraits,
            SceneId::ReceptionEntrance | SceneId::Speeches | SceneId::Cake => Self::Reception,
            SceneId::FirstDance | SceneId::DanceFloor => Self::Dance,
            SceneId::Candid | SceneId::Exit => Self::Candid,
            SceneId::Unknown => Self::Other,
        }
    }

    /// The chapter this group is usually found in, for per-chapter profile selection.
    ///
    /// A convenience for the panel rather than a claim: phase 07's segmenter decides which
    /// chapter a frame is in, and this is only what a photographer would guess.
    #[must_use]
    pub const fn usual_chapter(self) -> ChapterId {
        match self {
            Self::Preparation => ChapterId::GettingReady,
            Self::Details => ChapterId::Details,
            Self::Ceremony => ChapterId::Ceremony,
            Self::Portraits => ChapterId::Portraits,
            Self::Reception => ChapterId::Reception,
            Self::Dance => ChapterId::Dance,
            Self::Candid | Self::Other => ChapterId::Other,
        }
    }
}

impl fmt::Display for SceneGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The kind of light a photograph was made in, as a style axis.
///
/// Ten, and nine of them are phase 15's [`crate::contract::tone::IlluminantKind`] with
/// fluorescent and LED folded together. The tenth, [`LightingBucket::GoldenHour`], is the one
/// this phase adds: it is daylight below a colour temperature, and it is a *separate style*
/// for every wedding photographer alive.
///
/// **Mixed light is deliberately not a bucket.** A frame with two illuminants still has one
/// that governs the subject - phase 15 records which - and a photographer grades the frame for
/// the light on the people in it. Mixed light is a feature the regression reads, not a
/// bucket that halves the sample count of every indoor scene.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum LightingBucket {
    /// Nothing identified the light. The default, and the answer that claims the least.
    #[default]
    Unknown,
    /// Direct sun or open daylight above [`LightingBucket::GOLDEN_BELOW_K`].
    Daylight,
    /// Low sun: daylight below [`LightingBucket::GOLDEN_BELOW_K`], outdoors.
    GoldenHour,
    /// Open shade or a blue-hour sky.
    Shade,
    /// Overcast, between daylight and shade.
    Overcast,
    /// Incandescent or halogen. The reception light phase 15 exists for.
    Tungsten,
    /// A gas-discharge tube or a cheap white LED: near a colour temperature, off the locus.
    Artificial,
    /// The camera's own flash or a strobe.
    Flash,
    /// A candle or a diya, below 2200 K.
    Candle,
    /// A saturated coloured source: an uplighter, a gel, a dance-floor wash.
    Stage,
}

impl LightingBucket {
    /// Every bucket, in the order `docs/style-profiles.md` documents them.
    pub const ALL: [Self; 10] = [
        Self::Unknown,
        Self::Daylight,
        Self::GoldenHour,
        Self::Shade,
        Self::Overcast,
        Self::Tungsten,
        Self::Artificial,
        Self::Flash,
        Self::Candle,
        Self::Stage,
    ];

    /// How many lighting buckets there are.
    pub const COUNT: usize = 10;

    /// Below this colour temperature, daylight is golden hour.
    ///
    /// Four thousand five hundred kelvin. It is a *style* boundary rather than a physical one -
    /// the sun does not change at 4500 K - and it sits where a photographer stops calling a
    /// frame "outdoors" and starts calling it "golden".
    pub const GOLDEN_BELOW_K: f32 = 4500.0;

    /// The stable slug, stored and sent on the wire. Never localised.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Daylight => "daylight",
            Self::GoldenHour => "golden_hour",
            Self::Shade => "shade",
            Self::Overcast => "overcast",
            Self::Tungsten => "tungsten",
            Self::Artificial => "artificial",
            Self::Flash => "flash",
            Self::Candle => "candle",
            Self::Stage => "stage",
        }
    }

    /// What the bucket matrix calls it.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown light",
            Self::Daylight => "Daylight",
            Self::GoldenHour => "Golden hour",
            Self::Shade => "Shade",
            Self::Overcast => "Overcast",
            Self::Tungsten => "Tungsten",
            Self::Artificial => "LED / fluorescent",
            Self::Flash => "Flash",
            Self::Candle => "Candlelight",
            Self::Stage => "Stage light",
        }
    }

    /// Parse the stored slug; unknown text is [`LightingBucket::Unknown`].
    #[must_use]
    pub fn from_str_or_unknown(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|bucket| bucket.as_str() == text)
            .unwrap_or(Self::Unknown)
    }

    /// True when this light is one a photographer deliberately keeps rather than corrects.
    ///
    /// The same three phase 15 calls intentional. A style delta learned in one of these
    /// buckets is a taste about *mood*, and it is the one place where a large temperature lean
    /// is the photographer being right rather than the fit being wrong.
    #[must_use]
    pub const fn is_intentional(self) -> bool {
        matches!(self, Self::Stage | Self::Candle | Self::GoldenHour)
    }
}

impl fmt::Display for LightingBucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One leaf of the style tree: a kind of photograph in a kind of light.
///
/// Eighty leaves - [`SceneGroup::COUNT`] times [`LightingBucket::COUNT`] - which is why
/// shrinkage is not optional. Three hundred pairs over eighty buckets is under four each, and
/// a per-bucket mean over four samples is noise. The tree exists so that four samples move a
/// bucket a little and four hundred let it decide. ADR-0035 decision 5.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub struct StyleBucket {
    /// What the photograph is of.
    pub group: SceneGroup,
    /// What light it was made in.
    pub lighting: LightingBucket,
}

impl StyleBucket {
    /// How many leaves the tree has.
    pub const COUNT: usize = SceneGroup::COUNT * LightingBucket::COUNT;

    /// A bucket.
    #[must_use]
    pub const fn new(group: SceneGroup, lighting: LightingBucket) -> Self {
        Self { group, lighting }
    }

    /// The stable slug, `group/lighting`, which is also the catalog's key.
    #[must_use]
    pub fn as_key(&self) -> String {
        format!("{}/{}", self.group.as_str(), self.lighting.as_str())
    }

    /// Parse the stored key. Anything unrecognised resolves to the two defaults, which is the
    /// bucket that borrows everything from the global profile.
    #[must_use]
    pub fn from_key(text: &str) -> Self {
        let mut parts = text.splitn(2, '/');
        let group = parts.next().unwrap_or_default();
        let lighting = parts.next().unwrap_or_default();
        Self {
            group: SceneGroup::from_str_or_other(group),
            lighting: LightingBucket::from_str_or_unknown(lighting),
        }
    }

    /// What the matrix calls it.
    #[must_use]
    pub fn title(&self) -> String {
        format!("{} - {}", self.group.title(), self.lighting.title())
    }

    /// Every bucket, in matrix order: group-major, lighting-minor.
    #[must_use]
    pub fn all() -> Vec<Self> {
        let mut out = Vec::with_capacity(Self::COUNT);
        for group in SceneGroup::ALL {
            for lighting in LightingBucket::ALL {
                out.push(Self::new(group, lighting));
            }
        }
        out
    }
}

impl fmt::Display for StyleBucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_key())
    }
}

/// Which level of the tree actually answered.
///
/// Ordered strongest first, so a comparison reads the way the resolution does. It is stored on
/// every styled decision and reported in section 11's `style.bucket_fallback`, because a
/// wedding graded entirely out of the global profile is a wedding whose scene conditioning did
/// nothing - which is the quiet version of section 12's third failure mode.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum FallbackLevel {
    /// The bucket had enough of its own evidence.
    Bucket,
    /// The bucket borrowed from its scene group.
    Group,
    /// The group borrowed from the profile's global delta.
    Global,
    /// Nothing was learned that applies here, so phases 15 and 16's own answer stands.
    ///
    /// The default, and **never a failure**: it is what an unadopted profile, an empty bucket
    /// and a photographer who has taught AURA nothing all produce.
    #[default]
    Factory,
}

impl FallbackLevel {
    /// Every level, strongest first.
    pub const ALL: [Self; 4] = [Self::Bucket, Self::Group, Self::Global, Self::Factory];

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bucket => "bucket",
            Self::Group => "group",
            Self::Global => "global",
            Self::Factory => "factory",
        }
    }

    /// Parse the stored slug; unknown text is [`FallbackLevel::Factory`].
    #[must_use]
    pub fn from_str_or_factory(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|level| level.as_str() == text)
            .unwrap_or(Self::Factory)
    }

    /// True when the photographer's own evidence for this kind of frame is what answered.
    #[must_use]
    pub const fn is_taught(self) -> bool {
        matches!(self, Self::Bucket)
    }
}

impl fmt::Display for FallbackLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// The delta
// ---------------------------------------------------------------------------

/// Offsets applied to a fitted tone curve's control points.
///
/// Section 5's `CurveShift`, as offsets at **five fixed input levels** rather than as a list
/// of moved points. Phase 16's fitter produces between four and eight control points and the
/// count depends on the frame, so a shift expressed as "point three moves up" would mean
/// different things on different photographs - and on some of them would mean nothing, because
/// there is no point three.
///
/// Five anchors at 0, 64, 128, 192 and 255 cover a toe, a low mid, a mid, a shoulder and a
/// white point, which is the vocabulary photographers actually use about a curve. The applier
/// evaluates the offset at each existing control point by linear interpolation between anchors
/// and then re-checks monotonicity, so a shift can never produce a curve
/// [`crate::contract::colour::ToneCurve::new`] would refuse.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CurveShift {
    /// Output offset at input 0, in the recipe's 0-255 units. Positive lifts the toe.
    pub black: f32,
    /// Output offset at input 64.
    pub low: f32,
    /// Output offset at input 128.
    pub mid: f32,
    /// Output offset at input 192.
    pub high: f32,
    /// Output offset at input 255. Positive is impossible to render and is clamped to zero by
    /// [`CurveShift::clamped`]: white is white.
    pub white: f32,
}

impl CurveShift {
    /// The input levels the five offsets sit at.
    pub const ANCHORS: [u16; 5] = [0, 64, 128, 192, 255];

    /// The offsets, in anchor order.
    #[must_use]
    pub const fn as_array(&self) -> [f32; 5] {
        [self.black, self.low, self.mid, self.high, self.white]
    }

    /// Build from an array in anchor order.
    #[must_use]
    pub const fn from_array(values: [f32; 5]) -> Self {
        Self {
            black: values[0],
            low: values[1],
            mid: values[2],
            high: values[3],
            white: values[4],
        }
    }

    /// True when this shift changes nothing.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        self.as_array().iter().all(|value| value.abs() < 1e-4)
    }

    /// Every offset inside [`MAX_CURVE_OFFSET`], with the white anchor held at or below zero.
    #[must_use]
    pub fn clamped(&self) -> Self {
        let bound = MAX_CURVE_OFFSET;
        Self {
            black: self.black.clamp(-bound, bound),
            low: self.low.clamp(-bound, bound),
            mid: self.mid.clamp(-bound, bound),
            high: self.high.clamp(-bound, bound),
            // A positive offset at input 255 asks for an output above white. It renders as a
            // flat top - a posterised band and new clipping in one move, which is the exact
            // defect phase 16's fitter stopped clamping to avoid.
            white: self.white.clamp(-bound, 0.0),
        }
    }

    /// This shift scaled, for shrinkage.
    #[must_use]
    pub fn scaled(&self, factor: f32) -> Self {
        Self::from_array(self.as_array().map(|value| value * factor))
    }

    /// The sum of two shifts, for the resolution chain.
    #[must_use]
    pub fn plus(&self, other: &Self) -> Self {
        let (left, right) = (self.as_array(), other.as_array());
        let mut out = [0.0_f32; 5];
        for (index, slot) in out.iter_mut().enumerate() {
            *slot =
                left.get(index).copied().unwrap_or(0.0) + right.get(index).copied().unwrap_or(0.0);
        }
        Self::from_array(out)
    }

    /// The offset at one input level, linearly interpolated between the anchors.
    #[must_use]
    pub fn at(&self, level: u16) -> f32 {
        let values = self.as_array();
        let level = f32::from(level.min(255));
        for window in 0..Self::ANCHORS.len().saturating_sub(1) {
            let (Some(&lo), Some(&hi)) = (
                Self::ANCHORS.get(window),
                Self::ANCHORS.get(window.saturating_add(1)),
            ) else {
                break;
            };
            if level <= f32::from(hi) {
                let span = f32::from(hi) - f32::from(lo);
                let t = if span <= 0.0 {
                    0.0
                } else {
                    (level - f32::from(lo)) / span
                };
                let a = values.get(window).copied().unwrap_or(0.0);
                let b = values.get(window.saturating_add(1)).copied().unwrap_or(0.0);
                return a + (b - a) * t;
            }
        }
        values.last().copied().unwrap_or(0.0)
    }

    /// The largest offset anywhere, for the magnitude metric.
    #[must_use]
    pub fn magnitude(&self) -> f32 {
        self.as_array()
            .iter()
            .fold(0.0_f32, |worst, value| worst.max(value.abs()))
    }

    /// One curve with this shift applied to every control point.
    ///
    /// **The applier lives here, in the contract, and not in either crate that calls it.** Phase
    /// 16 solves the curve and phase 17 shifts it, and a second implementation of "add an offset
    /// to a control point" is two curves that drift apart while looking identical - the same
    /// argument that moved Fritsch-Carlson into `aura_raw::colour::curve` when the fitter became
    /// its second consumer.
    ///
    /// Monotonicity is preserved by construction rather than by care: the shifted points go
    /// through [`ToneCurve::new`], which refuses a set that is not monotone, and a refusal
    /// returns the **original** curve unchanged. So a style can make a curve gentler or
    /// stronger and can never make it posterise, and the caller does not have to remember that.
    #[must_use]
    pub fn applied_to(&self, curve: &ToneCurve) -> ToneCurve {
        if self.is_neutral() {
            return curve.clone();
        }
        let shifted: Vec<CurvePoint> = curve
            .points()
            .iter()
            .map(|point| {
                let offset = self.at(point.x);
                let y = (f32::from(point.y) + offset).clamp(0.0, f32::from(ToneCurve::MAX_LEVEL));
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                CurvePoint::new(point.x, y.round() as u16)
            })
            .collect();
        ToneCurve::new(shifted).unwrap_or_else(|_| curve.clone())
    }
}

/// How far this photographer sits from the consensus about skin.
///
/// **Two bounded scalars, and neither of them is a colour.** `warmth` is a hue lean in degrees
/// and `chroma` is a saturation lean as a fraction, both measured as the difference between
/// what the photographer's own edit did to the skin in their own frames and what phases 15 and
/// 16 would have done to the same pixels. There is no chromaticity here, no skin-tone bucket
/// and no target - the same structural argument [`crate::contract::tone::SkinLocus`] makes,
/// made for the third time. ADR-0035 decision 7.
///
/// The bounds sit **below** phase 16's ceilings ([`MAX_SKIN_WARMTH_DEG`] against
/// [`crate::contract::colour::SKIN_HUE_CEILING_DEG`]), so a style cannot even ask for a
/// movement larger than the guarantee allows - and phase 16's guard measures the result
/// afterwards regardless, because a bound on a parameter is not a promise about a pixel.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkinBias {
    /// Hue lean in degrees. Positive is warmer.
    pub warmth_deg: f32,
    /// Chroma lean as a fraction of the baseline's. Positive is more saturated.
    pub chroma: f32,
}

impl SkinBias {
    /// True when this photographer treats skin the way the consensus does.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        self.warmth_deg.abs() < 1e-4 && self.chroma.abs() < 1e-4
    }

    /// Both leans inside their bounds.
    #[must_use]
    pub fn clamped(&self) -> Self {
        Self {
            warmth_deg: self
                .warmth_deg
                .clamp(-MAX_SKIN_WARMTH_DEG, MAX_SKIN_WARMTH_DEG),
            chroma: self
                .chroma
                .clamp(-MAX_SKIN_CHROMA_BIAS, MAX_SKIN_CHROMA_BIAS),
        }
    }

    /// This bias scaled, for shrinkage.
    #[must_use]
    pub fn scaled(&self, factor: f32) -> Self {
        Self {
            warmth_deg: self.warmth_deg * factor,
            chroma: self.chroma * factor,
        }
    }

    /// The sum of two biases, for the resolution chain.
    #[must_use]
    pub fn plus(&self, other: &Self) -> Self {
        Self {
            warmth_deg: self.warmth_deg + other.warmth_deg,
            chroma: self.chroma + other.chroma,
        }
    }
}

/// Additive deltas on the decisions phases 15 and 16 already made.
///
/// PHASE-17 section 5's struct, verbatim in every field it names. **Every number is a
/// difference**, and the zero value is "this photographer edits like the consensus", which is
/// also what an untrained profile, an unpopulated bucket and a cancelled training all produce.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StyleDelta {
    /// Exposure offset in stops, within [`MAX_EXPOSURE_DELTA_EV`].
    pub exposure: f32,
    /// Colour-temperature offset in kelvin, within [`MAX_TEMPERATURE_DELTA_K`].
    pub temperature_k: f32,
    /// Tint offset, within [`MAX_TINT_DELTA`].
    pub tint: f32,
    /// Contrast offset, within [`MAX_PARAM_DELTA`].
    pub contrast: f32,
    /// Highlight-recovery offset.
    pub highlights: f32,
    /// Shadow-lift offset.
    pub shadows: f32,
    /// White-point offset.
    pub whites: f32,
    /// Black-point offset.
    pub blacks: f32,
    /// Control-point offsets on the fitted curve.
    pub curve_shift: CurveShift,
    /// Per-band hue, saturation and luminance offsets.
    pub hsl: HslAdjustments,
    /// Vibrance offset.
    pub vibrance: f32,
    /// Flat saturation offset.
    pub saturation: f32,
    /// How this photographer treats skin, relative to the consensus.
    pub skin_bias: SkinBias,
    /// How sure this delta is, `0..1`. Invariant 2.
    pub confidence: f32,
    /// How many pairs it was fitted from.
    pub samples: u32,
}

impl StyleDelta {
    /// The delta that changes nothing, at zero confidence.
    ///
    /// Not [`Default::default`] with a comment: this is named because it is the answer the
    /// product gives in five different situations and every one of them is a normal state.
    #[must_use]
    pub fn neutral() -> Self {
        Self::default()
    }

    /// True when this delta changes nothing about a photograph.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        self.exposure.abs() < 1e-4
            && self.temperature_k.abs() < 1e-2
            && self.tint.abs() < 1e-3
            && self.contrast.abs() < 1e-3
            && self.highlights.abs() < 1e-3
            && self.shadows.abs() < 1e-3
            && self.whites.abs() < 1e-3
            && self.blacks.abs() < 1e-3
            && self.vibrance.abs() < 1e-3
            && self.saturation.abs() < 1e-3
            && self.curve_shift.is_neutral()
            && self.hsl.is_neutral()
            && self.skin_bias.is_neutral()
    }

    /// Every field inside its documented bound, with the skin bands held to
    /// [`SKIN_BAND_FRACTION`] of the rest.
    ///
    /// **This is the cheap half of the skin defence and it is applied on construction**, so a
    /// delta that exists is a delta already inside its bounds. The expensive half is phase 16's
    /// guard, which measures the pixels afterwards and can withdraw the whole colour half.
    #[must_use]
    pub fn clamped(&self) -> Self {
        let param = |value: f32| value.clamp(-MAX_PARAM_DELTA, MAX_PARAM_DELTA);
        let mut hsl = HslAdjustments::default();
        for band in HslBand::ALL {
            let shift = self.hsl.get(band);
            let bound = if band.is_skin_band() {
                MAX_HSL_DELTA * SKIN_BAND_FRACTION
            } else if band.is_skin_adjacent() {
                // Halfway. Red and yellow carry the deepest shadows and the brightest
                // highlights of a face at every skin tone, so they are neither free nor as
                // tightly held as orange.
                MAX_HSL_DELTA * (SKIN_BAND_FRACTION + 1.0) * 0.5
            } else {
                MAX_HSL_DELTA
            };
            hsl.set(
                band,
                crate::contract::colour::HslShift {
                    h: shift.h.clamp(-bound, bound),
                    s: shift.s.clamp(-bound, bound),
                    l: shift.l.clamp(-bound, bound),
                },
            );
        }
        Self {
            exposure: self
                .exposure
                .clamp(-MAX_EXPOSURE_DELTA_EV, MAX_EXPOSURE_DELTA_EV),
            temperature_k: self
                .temperature_k
                .clamp(-MAX_TEMPERATURE_DELTA_K, MAX_TEMPERATURE_DELTA_K),
            tint: self.tint.clamp(-MAX_TINT_DELTA, MAX_TINT_DELTA),
            contrast: param(self.contrast),
            highlights: param(self.highlights),
            shadows: param(self.shadows),
            whites: param(self.whites),
            blacks: param(self.blacks),
            curve_shift: self.curve_shift.clamped(),
            hsl,
            vibrance: param(self.vibrance),
            saturation: param(self.saturation),
            skin_bias: self.skin_bias.clamped(),
            confidence: self.confidence.clamp(0.0, 1.0),
            samples: self.samples,
        }
    }

    /// This delta scaled by a shrinkage factor.
    ///
    /// `samples` is **not** scaled: it is a count of evidence, not a magnitude, and a shrunk
    /// delta was still fitted from the pairs it was fitted from.
    #[must_use]
    pub fn scaled(&self, factor: f32) -> Self {
        Self {
            exposure: self.exposure * factor,
            temperature_k: self.temperature_k * factor,
            tint: self.tint * factor,
            contrast: self.contrast * factor,
            highlights: self.highlights * factor,
            shadows: self.shadows * factor,
            whites: self.whites * factor,
            blacks: self.blacks * factor,
            curve_shift: self.curve_shift.scaled(factor),
            hsl: self.hsl.attenuated(factor.clamp(0.0, 1.0)),
            vibrance: self.vibrance * factor,
            saturation: self.saturation * factor,
            skin_bias: self.skin_bias.scaled(factor),
            confidence: self.confidence,
            samples: self.samples,
        }
    }

    /// The sum of two deltas, which is how the resolution chain composes.
    ///
    /// The confidence of a sum is the **larger** of the two, and the sample count is the
    /// larger too: a bucket delta shrunk toward its group and then added to it is one answer
    /// about one frame, and reporting the sum of two sample counts would double-count the
    /// pairs that produced both.
    #[must_use]
    pub fn plus(&self, other: &Self) -> Self {
        let mut hsl = HslAdjustments::default();
        for band in HslBand::ALL {
            let (left, right) = (self.hsl.get(band), other.hsl.get(band));
            hsl.set(
                band,
                crate::contract::colour::HslShift {
                    h: left.h + right.h,
                    s: left.s + right.s,
                    l: left.l + right.l,
                },
            );
        }
        Self {
            exposure: self.exposure + other.exposure,
            temperature_k: self.temperature_k + other.temperature_k,
            tint: self.tint + other.tint,
            contrast: self.contrast + other.contrast,
            highlights: self.highlights + other.highlights,
            shadows: self.shadows + other.shadows,
            whites: self.whites + other.whites,
            blacks: self.blacks + other.blacks,
            curve_shift: self.curve_shift.plus(&other.curve_shift),
            hsl,
            vibrance: self.vibrance + other.vibrance,
            saturation: self.saturation + other.saturation,
            skin_bias: self.skin_bias.plus(&other.skin_bias),
            confidence: self.confidence.max(other.confidence),
            samples: self.samples.max(other.samples),
        }
    }

    /// One eight-band block with this delta's own bands added to it.
    ///
    /// Here rather than in either caller for the reason [`CurveShift::applied_to`] is: phase 16
    /// solves the bands and phase 17 shifts them, and one addition written twice is one addition
    /// that eventually disagrees with itself about what the skin band is allowed to do.
    ///
    /// The result is clamped into the recipe's range, and the skin bands are already held to
    /// [`SKIN_BAND_FRACTION`] of the rest by [`StyleDelta::clamped`] before this is ever called.
    #[must_use]
    pub fn applied_to_hsl(&self, base: &HslAdjustments) -> HslAdjustments {
        let mut out = *base;
        for band in HslBand::ALL {
            let (theirs, mine) = (base.get(band), self.hsl.get(band));
            out.set(
                band,
                crate::contract::colour::HslShift {
                    h: theirs.h + mine.h,
                    s: theirs.s + mine.s,
                    l: theirs.l + mine.l,
                },
            );
        }
        out.clamped()
    }

    /// How large this delta is, normalised so that 1.0 is every bound at once.
    ///
    /// Root-mean-square over the normalised fields rather than a maximum, so a style that
    /// leans a little on everything scores lower than one that pushes a single parameter to its
    /// limit - which matches what a photographer's consistent taste actually looks like.
    #[must_use]
    pub fn magnitude(&self) -> f32 {
        let terms = [
            self.exposure / MAX_EXPOSURE_DELTA_EV,
            self.temperature_k / MAX_TEMPERATURE_DELTA_K,
            self.tint / MAX_TINT_DELTA,
            self.contrast / MAX_PARAM_DELTA,
            self.highlights / MAX_PARAM_DELTA,
            self.shadows / MAX_PARAM_DELTA,
            self.whites / MAX_PARAM_DELTA,
            self.blacks / MAX_PARAM_DELTA,
            self.vibrance / MAX_PARAM_DELTA,
            self.saturation / MAX_PARAM_DELTA,
            self.curve_shift.magnitude() / MAX_CURVE_OFFSET,
            self.hsl.magnitude(),
            self.skin_bias.warmth_deg / MAX_SKIN_WARMTH_DEG,
        ];
        #[allow(clippy::cast_precision_loss)]
        let count = terms.len() as f32;
        (terms.iter().map(|term| term * term).sum::<f32>() / count).sqrt()
    }
}

// ---------------------------------------------------------------------------
// The bucket model
// ---------------------------------------------------------------------------

/// One bucket's fitted model.
///
/// Section 5's `BucketModel`, as the shrunk delta plus what the diagnostics need to be honest
/// about it. The **regression coefficients are not here**: section 6.2 fits a small ridge model
/// per delta on frame features, and the coefficients are an implementation of `aura-style` that
/// a consumer of this contract has no use for. What a consumer needs is the answer and how much
/// to believe it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BucketModel {
    /// Which leaf.
    pub bucket: StyleBucket,
    /// The shrunk delta this bucket contributes on top of its group's.
    pub delta: StyleDelta,
    /// How many pairs landed here.
    pub samples: u32,
    /// How many of those were held out of the fit and used to measure it.
    pub held_out: u32,
    /// The measured style-match error on the held-out pairs, in dE00.
    ///
    /// `None` when nothing was held out. **Not zero** - a bucket trained on eleven pairs and
    /// evaluated on none has no measurement, and zero would read as a perfect match. ADR-0036
    /// decision 2.
    pub match_de00: Option<f32>,
    /// The shrinkage factor applied toward the parent, `0..1`. One means the bucket kept its
    /// own answer entirely.
    pub shrink: f32,
}

impl BucketModel {
    /// A bucket nobody taught.
    #[must_use]
    pub fn empty(bucket: StyleBucket) -> Self {
        Self {
            bucket,
            delta: StyleDelta::neutral(),
            samples: 0,
            held_out: 0,
            match_de00: None,
            shrink: 0.0,
        }
    }

    /// True when this bucket has too little evidence to be trusted on its own.
    #[must_use]
    pub const fn is_weak(&self) -> bool {
        self.samples <= WEAK_BUCKET_SAMPLES
    }

    /// True when this bucket met the ceiling that applies to it.
    ///
    /// A weak bucket is held to [`WEAK_MATCH_DE00_CEILING`] and a populated one to
    /// [`MATCH_DE00_CEILING`]. An unmeasured bucket is neither met nor failed and the caller
    /// has to decide what to say about it - which is deliberate, because the two honest
    /// sentences are different.
    #[must_use]
    pub fn met_ceiling(&self) -> Option<bool> {
        let ceiling = if self.is_weak() {
            WEAK_MATCH_DE00_CEILING
        } else {
            MATCH_DE00_CEILING
        };
        self.match_de00.map(|measured| measured <= ceiling)
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

/// One row of the honest report.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BucketDiagnostic {
    /// Which leaf.
    pub bucket: StyleBucket,
    /// How many pairs landed here.
    pub samples: u32,
    /// The measured style-match error, or `None` when nothing was held out.
    pub match_de00: Option<f32>,
    /// Which level of the tree answers for this bucket at inference time.
    pub level: FallbackLevel,
}

/// What the photographer is told about the profile they just trained.
///
/// PHASE-17 section 5's struct, with the `Vec<(StyleBucket, u32, f32)>` written out as a named
/// row type so that the third field can be `Option<f32>`. A tuple whose third element is `0.0`
/// for "not measured" is a tuple that will be rendered as a perfect score by somebody, once,
/// in a panel nobody reviewed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProfileDiagnostics {
    /// Every populated bucket, in matrix order.
    pub per_bucket: Vec<BucketDiagnostic>,
    /// The buckets the photographer should add weddings for, worst first.
    pub weak_buckets: Vec<StyleBucket>,
    /// The style-match error over every held-out pair, in dE00.
    pub overall_de00: f32,
    /// How many pairs the fit accepted.
    pub accepted_pairs: u32,
    /// How many it rejected, and therefore how honest the number above is.
    pub rejected_pairs: u32,
    /// What to add next, as a sentence. Assembled from the gap rather than free text.
    pub recommendation: String,
}

impl ProfileDiagnostics {
    /// The fraction of scanned pairs that survived the residual check, `0..1`.
    #[must_use]
    pub fn acceptance(&self) -> f32 {
        let total = self.accepted_pairs.saturating_add(self.rejected_pairs);
        if total == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.accepted_pairs as f32 / total as f32
        }
    }

    /// True when the overall figure met section 10.1's ceiling.
    #[must_use]
    pub fn met_ceiling(&self) -> bool {
        self.overall_de00 <= MATCH_DE00_CEILING
    }

    /// How many buckets have a measurement at all.
    #[must_use]
    pub fn measured_buckets(&self) -> usize {
        self.per_bucket
            .iter()
            .filter(|row| row.match_de00.is_some())
            .count()
    }
}

// ---------------------------------------------------------------------------
// The profile
// ---------------------------------------------------------------------------

/// Where a profile is in its life.
///
/// Three states and the transitions are one-way except the last. A profile is trained as a
/// candidate, adopted deliberately, and retired when a newer version of the same name is
/// adopted - never deleted, because a gallery delivered under version 3 has to stay
/// reproducible after version 4 exists. Invariant 4's determinism reaches this far.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ProfileStatus {
    /// Trained and measured, and applied to nothing. The default after training.
    #[default]
    Candidate,
    /// In use. At most one version of a given name is adopted at a time.
    Adopted,
    /// Superseded by a newer adopted version, and kept so old edits still reproduce.
    Retired,
}

impl ProfileStatus {
    /// Every status.
    pub const ALL: [Self; 3] = [Self::Candidate, Self::Adopted, Self::Retired];

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::Adopted => "adopted",
            Self::Retired => "retired",
        }
    }

    /// Parse the stored slug; unknown text is [`ProfileStatus::Candidate`], which is the state
    /// that applies to nothing.
    #[must_use]
    pub fn from_str_or_candidate(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == text)
            .unwrap_or(Self::Candidate)
    }
}

impl fmt::Display for ProfileStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One photographer's learned look.
///
/// PHASE-17 section 5's struct, with `BTreeMap` in place of `HashMap` (ADR-0035 decision 3 -
/// a profile is hashed and signed, and a map whose iteration order is not a function of its
/// contents would produce two signatures for one profile), and with the four fields the
/// lifecycle needs and section 5 does not name: the status, the project it was trained from,
/// the diagnostics' own version, and the digest the bundle is signed over.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StyleProfile {
    /// The profile.
    pub id: ProfileId,
    /// What the photographer calls it - "personal", "light and airy", "second shooter".
    pub name: String,
    /// Which training produced it. Monotonically increasing per name.
    pub version: u16,
    /// Where it is in its life.
    pub status: ProfileStatus,
    /// The lean that applies to every photograph, before any conditioning.
    pub global: StyleDelta,
    /// Per scene group, on top of the global.
    pub groups: BTreeMap<SceneGroup, StyleDelta>,
    /// Per leaf, on top of its group.
    pub buckets: BTreeMap<StyleBucket, BucketModel>,
    /// The honest report.
    pub diagnostics: ProfileDiagnostics,
    /// How many pairs it was trained on, accepted only.
    pub trained_pairs: u32,
    /// When, in milliseconds since the Unix epoch.
    pub trained_at: Timestamp,
    /// The render engine the fit was measured against.
    ///
    /// `aura_recipe::contract::recipe::ENGINE`. A profile fitted against one renderer and
    /// applied by another is a profile whose measured dE00 is about a build that no longer
    /// exists, and `AURA-ML-5077` is what says so rather than letting it pass.
    pub engine_ver: String,
    /// Which build's fitter and shrinkage produced it.
    pub analysis_ver: u16,
}

impl StyleProfile {
    /// An empty profile: the one that changes nothing.
    ///
    /// Named because it is a real state and not an error - a profile scanned from an archive
    /// with no usable pairs is exactly this, and it produces the factory baseline everywhere.
    #[must_use]
    pub fn empty(id: ProfileId, name: impl Into<String>, engine: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            version: 1,
            status: ProfileStatus::Candidate,
            global: StyleDelta::neutral(),
            groups: BTreeMap::new(),
            buckets: BTreeMap::new(),
            diagnostics: ProfileDiagnostics::default(),
            trained_pairs: 0,
            trained_at: 0,
            engine_ver: engine.into(),
            analysis_ver: 0,
        }
    }

    /// The delta that applies to one bucket, and which level of the tree answered.
    ///
    /// **The whole resolution rule, in one place.** Bucket, then group, then global, then the
    /// factory baseline; each level is *added* to the one above it, and a level whose
    /// confidence is below [`APPLY_ABOVE`] is skipped rather than half-applied. ADR-0035
    /// decision 5.
    #[must_use]
    pub fn resolve(&self, bucket: StyleBucket) -> (StyleDelta, FallbackLevel) {
        let global = if self.global.confidence >= APPLY_ABOVE {
            self.global.clone()
        } else {
            StyleDelta::neutral()
        };
        let group = self
            .groups
            .get(&bucket.group)
            .filter(|delta| delta.confidence >= APPLY_ABOVE);
        let leaf = self
            .buckets
            .get(&bucket)
            .filter(|model| model.delta.confidence >= APPLY_ABOVE && model.samples > 0);

        match (leaf, group) {
            (Some(model), Some(group_delta)) => (
                global.plus(group_delta).plus(&model.delta).clamped(),
                FallbackLevel::Bucket,
            ),
            (Some(model), None) => (global.plus(&model.delta).clamped(), FallbackLevel::Bucket),
            (None, Some(group_delta)) => (global.plus(group_delta).clamped(), FallbackLevel::Group),
            (None, None) => {
                if global.is_neutral() {
                    (StyleDelta::neutral(), FallbackLevel::Factory)
                } else {
                    (global.clamped(), FallbackLevel::Global)
                }
            }
        }
    }

    /// How many leaves carry the photographer's own evidence.
    #[must_use]
    pub fn taught_buckets(&self) -> usize {
        self.buckets
            .values()
            .filter(|model| model.samples > 0)
            .count()
    }

    /// True when this profile has enough evidence to be worth adopting.
    ///
    /// It is a *report*, not a gate: nothing in this contract refuses to adopt a profile with
    /// two hundred pairs, because refusing at 299 is a cliff and the photographer can see the
    /// number. Section 12's fourth failure mode is users expecting one-click magic from twenty
    /// photographs, and the mitigation is a strength meter rather than a false ready state.
    #[must_use]
    pub const fn is_usable(&self) -> bool {
        self.trained_pairs >= USABLE_PAIRS
    }

    /// How strong this profile is, `0..1`, for the meter section 12 asks for.
    ///
    /// The geometric mean of the things that can each independently make a profile useless: how
    /// much evidence there is, how much of the tree it covers, and - **when it has been
    /// measured** - how well the held-out pairs actually matched. Geometric rather than
    /// arithmetic for the reason phases 09 and 12 give: a profile with four thousand pairs in
    /// one bucket is not strong, and an average would say it was.
    ///
    /// A profile whose held-out set has not been evaluated has an `overall_de00` of zero, and
    /// that is **not** an accuracy of zero. "Nobody has measured this yet" and "this matches
    /// nothing" are different statements, and a meter that showed them the same way would be
    /// worse than no meter - the first is the ordinary state of a profile between training and
    /// evaluation. The mean is taken over the terms that exist.
    ///
    /// The coverage denominator is **twelve** rather than [`StyleBucket::COUNT`], deliberately.
    /// A photographer who only shoots outdoor daylight weddings populates a handful of leaves
    /// and has a perfectly good profile; scoring them against eighty would report five percent
    /// for somebody whose profile is excellent at everything they actually shoot.
    #[must_use]
    pub fn strength(&self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let evidence = (self.trained_pairs as f32 / USABLE_PAIRS as f32).clamp(0.0, 1.0);
        #[allow(clippy::cast_precision_loss)]
        let coverage = (self.taught_buckets() as f32 / 12.0).clamp(0.0, 1.0);
        if self.diagnostics.overall_de00 <= 0.0 {
            return (evidence * coverage).max(0.0).sqrt();
        }
        let accuracy = (MATCH_DE00_CEILING / self.diagnostics.overall_de00).clamp(0.0, 1.0);
        (evidence * coverage * accuracy).max(0.0).cbrt()
    }
}

// ---------------------------------------------------------------------------
// Pairs
// ---------------------------------------------------------------------------

/// How a RAW original was matched to a delivered final.
///
/// Ordered by how much the match can be trusted, strongest first, so `min` over a set is "the
/// weakest link in this archive". Section 2.1 names all four.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum MatchMethod {
    /// The final's embedded source digest equals the original's content hash. Exact.
    ContentHash,
    /// The file names share a stem. Nearly always right and occasionally catastrophic, which
    /// is why the residual check exists.
    FilenameStem,
    /// The capture times agree to the second and nothing else in the archive is that close.
    CaptureTime,
    /// Neither name nor time matched, and the two images look like the same photograph.
    Perceptual,
    /// Nothing matched. The default, and the state a file is in before the matcher runs.
    #[default]
    Unmatched,
}

impl MatchMethod {
    /// Every method, strongest first.
    pub const ALL: [Self; 5] = [
        Self::ContentHash,
        Self::FilenameStem,
        Self::CaptureTime,
        Self::Perceptual,
        Self::Unmatched,
    ];

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContentHash => "content_hash",
            Self::FilenameStem => "filename_stem",
            Self::CaptureTime => "capture_time",
            Self::Perceptual => "perceptual",
            Self::Unmatched => "unmatched",
        }
    }

    /// Parse the stored slug; unknown text is [`MatchMethod::Unmatched`].
    #[must_use]
    pub fn from_str_or_unmatched(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|method| method.as_str() == text)
            .unwrap_or(Self::Unmatched)
    }

    /// True when the match is exact rather than inferred.
    #[must_use]
    pub const fn is_exact(self) -> bool {
        matches!(self, Self::ContentHash)
    }
}

impl fmt::Display for MatchMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where the photographer's own parameters came from.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractSource {
    /// An XMP sidecar or a catalogue held the parameters. Exact and free.
    Xmp,
    /// Only a rendered final existed, and the recipe was fitted to reproduce it.
    Fitted,
    /// Neither. The default.
    #[default]
    None,
}

impl ExtractSource {
    /// Every source.
    pub const ALL: [Self; 3] = [Self::Xmp, Self::Fitted, Self::None];

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Xmp => "xmp",
            Self::Fitted => "fitted",
            Self::None => "none",
        }
    }

    /// Parse the stored slug; unknown text is [`ExtractSource::None`].
    #[must_use]
    pub fn from_str_or_none(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|source| source.as_str() == text)
            .unwrap_or(Self::None)
    }
}

impl fmt::Display for ExtractSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What happened to one original-and-final pair.
///
/// **A verdict, and never a photograph.** There is no field here that could hold image bytes,
/// which is what makes "AURA never uploads your archive" a property of the shape rather than a
/// promise about the code. ADR-0036 decision 1, and the same argument
/// [`crate::contract::ledger::Evidence`] makes about a support bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StylePair {
    /// The original's file name, without its directory.
    pub original: String,
    /// The final's file name, without its directory.
    pub final_image: String,
    /// How the two were matched.
    pub matched_by: MatchMethod,
    /// Where the parameters came from.
    pub extracted_from: ExtractSource,
    /// Which leaf it landed in.
    pub bucket: StyleBucket,
    /// The residual the fit could not explain, in dE00. Zero for an XMP pair, which is exact.
    pub residual_de00: f32,
    /// True when the pair was used in the fit.
    ///
    /// False means rejected, and the reason is [`StylePair::rejection`]. A rejected pair is
    /// **counted and named**, never silently dropped: section 6.1's "report how many pairs were
    /// used versus rejected - honesty here prevents 'why doesn't it look like me' support
    /// tickets".
    pub accepted: bool,
    /// Why it was rejected, when it was.
    pub rejection: Option<StyleCode>,
    /// True when it was held out of the fit and used to measure it.
    pub held_out: bool,
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why a style did what it did, as a closed set.
///
/// Twenty codes. **Nine of them withdraw a claim rather than making one** - they say the
/// product declined to apply a style, or applied less of one than it learned. This phase needs
/// that distinction as much as phase 16 did and for a subtler reason: an over-applied personal
/// style does not look wrong, it looks like *somebody else's* work, and the only defence is a
/// product that can say "I did not have enough evidence for this kind of frame".
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum StyleCode {
    // -- application -----------------------------------------------------
    /// This bucket had the photographer's own evidence and it was applied.
    BucketApplied,
    /// The bucket was sparse, so its scene group's lean was applied instead. **A withdrawal.**
    GroupFallback,
    /// Neither the bucket nor the group had evidence, so the profile's global lean was
    /// applied. **A withdrawal.**
    GlobalFallback,
    /// Nothing was learned that applies here, so phases 15 and 16's own answer stands. **A
    /// withdrawal**, and the default.
    #[default]
    NoProfile,
    /// The delta was below the confidence floor and was not applied. **A withdrawal.**
    BelowConfidenceFloor,

    // -- bounds ----------------------------------------------------------
    /// One or more style parameters were clamped to their documented bounds. **A withdrawal.**
    DeltaBounded,
    /// The skin bands were held to a fraction of the rest, as they always are.
    SkinBandsHeld,
    /// Phase 16's skin guard attenuated or withdrew the styled colour operations. **A
    /// withdrawal**, and the strongest one in the phase.
    SkinGuardOverrode,
    /// Phase 16's clipping guard reduced a styled parameter. **A withdrawal.**
    ClipGuardOverrode,
    /// Phase 15's skin-locus constraint refused the styled white balance. **A withdrawal.**
    LocusConstraintOverrode,

    // -- pairs and training ----------------------------------------------
    /// The parameters came from an XMP sidecar or a catalogue, exactly.
    ParametersFromXmp,
    /// The parameters were fitted to reproduce a rendered final.
    ParametersFitted,
    /// The fit could not reproduce the final closely enough, so the pair was rejected. **A
    /// withdrawal.**
    FitResidualTooHigh,
    /// The original could not be matched to a final. **A withdrawal.**
    PairUnmatched,
    /// The final is a crop or a composite the develop pipeline cannot express. **A
    /// withdrawal.**
    UnmodelledWork,
    /// One archive's contribution was capped so it could not dominate the profile. **A
    /// withdrawal.**
    ArchiveInfluenceCapped,
    /// A bucket has too few pairs to be trusted on its own and was shrunk toward its parent.
    BucketShrunk,
    /// The profile has fewer than [`USABLE_PAIRS`] pairs and is reported as weak. **A
    /// withdrawal.**
    ProfileUnderTrained,

    // -- lifecycle -------------------------------------------------------
    /// The profile was fitted against a different render engine than the one applying it.
    /// **A withdrawal.**
    EngineMismatch,
    /// The photographer set this by hand. **A withdrawal**, and the only unbeatable one.
    UserOverride,
}

impl StyleCode {
    /// Every code, in the order `docs/style-profiles.md` documents them.
    pub const ALL: [Self; 20] = [
        Self::BucketApplied,
        Self::GroupFallback,
        Self::GlobalFallback,
        Self::NoProfile,
        Self::BelowConfidenceFloor,
        Self::DeltaBounded,
        Self::SkinBandsHeld,
        Self::SkinGuardOverrode,
        Self::ClipGuardOverrode,
        Self::LocusConstraintOverrode,
        Self::ParametersFromXmp,
        Self::ParametersFitted,
        Self::FitResidualTooHigh,
        Self::PairUnmatched,
        Self::UnmodelledWork,
        Self::ArchiveInfluenceCapped,
        Self::BucketShrunk,
        Self::ProfileUnderTrained,
        Self::EngineMismatch,
        Self::UserOverride,
    ];

    /// How many reason codes there are.
    pub const COUNT: usize = 20;

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BucketApplied => "bucket_applied",
            Self::GroupFallback => "group_fallback",
            Self::GlobalFallback => "global_fallback",
            Self::NoProfile => "no_profile",
            Self::BelowConfidenceFloor => "below_confidence_floor",
            Self::DeltaBounded => "delta_bounded",
            Self::SkinBandsHeld => "skin_bands_held",
            Self::SkinGuardOverrode => "skin_guard_overrode",
            Self::ClipGuardOverrode => "clip_guard_overrode",
            Self::LocusConstraintOverrode => "locus_constraint_overrode",
            Self::ParametersFromXmp => "parameters_from_xmp",
            Self::ParametersFitted => "parameters_fitted",
            Self::FitResidualTooHigh => "fit_residual_too_high",
            Self::PairUnmatched => "pair_unmatched",
            Self::UnmodelledWork => "unmodelled_work",
            Self::ArchiveInfluenceCapped => "archive_influence_capped",
            Self::BucketShrunk => "bucket_shrunk",
            Self::ProfileUnderTrained => "profile_under_trained",
            Self::EngineMismatch => "engine_mismatch",
            Self::UserOverride => "user_override",
        }
    }

    /// Parse the stored slug. Unknown text is [`StyleCode::NoProfile`], which is the code that
    /// means "the baseline stands" and is therefore the safe default.
    #[must_use]
    pub fn from_str_or_none(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|code| code.as_str() == text)
            .unwrap_or(Self::NoProfile)
    }

    /// True when this code withdraws a claim rather than making one.
    #[must_use]
    pub const fn is_withdrawal(self) -> bool {
        matches!(
            self,
            Self::GroupFallback
                | Self::GlobalFallback
                | Self::NoProfile
                | Self::BelowConfidenceFloor
                | Self::DeltaBounded
                | Self::SkinGuardOverrode
                | Self::ClipGuardOverrode
                | Self::LocusConstraintOverrode
                | Self::FitResidualTooHigh
                | Self::PairUnmatched
                | Self::UnmodelledWork
                | Self::ArchiveInfluenceCapped
                | Self::ProfileUnderTrained
                | Self::EngineMismatch
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
            Self::BucketApplied => {
                "this was edited the way you edit this kind of photograph in this kind of light"
            }
            Self::GroupFallback => {
                "AURA has not seen enough of your photographs in this light yet, so it used how \
                 you edit this kind of photograph generally"
            }
            Self::GlobalFallback => {
                "AURA has not seen enough of this kind of photograph from you yet, so it used \
                 your overall look"
            }
            Self::NoProfile => {
                "no style profile applies to this photograph, so AURA edited it the way it edits \
                 everybody's"
            }
            Self::BelowConfidenceFloor => {
                "what AURA has learned about this kind of photograph is not consistent enough to \
                 act on yet, so it left the edit as it was"
            }
            Self::DeltaBounded => {
                "your style asked for a larger change than AURA allows a style to make, so it was \
                 reduced to the limit"
            }
            Self::SkinBandsHeld => {
                "the colours that carry skin were adjusted far less than the rest, as they always \
                 are"
            }
            Self::SkinGuardOverrode => {
                "applying your style here would have moved somebody's skin colour, so AURA held \
                 it back"
            }
            Self::ClipGuardOverrode => {
                "applying your style here would have clipped the brightest areas, so one setting \
                 was reduced"
            }
            Self::LocusConstraintOverrode => {
                "your usual warmth would have made somebody's skin implausible in this light, so \
                 AURA kept the light's own colour"
            }
            Self::ParametersFromXmp => "your own settings were read directly from the sidecar",
            Self::ParametersFitted => {
                "your settings were worked out by reproducing your finished photograph"
            }
            Self::FitResidualTooHigh => {
                "AURA could not reproduce this photograph closely enough to learn from it, so it \
                 was left out"
            }
            Self::PairUnmatched => {
                "AURA could not tell which original this finished photograph came from, so it was \
                 left out"
            }
            Self::UnmodelledWork => {
                "this photograph has work in it that AURA cannot copy - a crop, a composite or \
                 local retouching - so it was left out of the style"
            }
            Self::ArchiveInfluenceCapped => {
                "one wedding was unusual enough that AURA limited how much it could change your \
                 overall look"
            }
            Self::BucketShrunk => {
                "there were only a few photographs of this kind, so AURA leant on the rest of \
                 your work as well"
            }
            Self::ProfileUnderTrained => {
                "this profile has fewer photographs than AURA needs to be confident, so it is \
                 applied gently"
            }
            Self::EngineMismatch => {
                "this profile was learned with an older version of AURA's editing engine, so it \
                 is applied cautiously until it is retrained"
            }
            Self::UserOverride => "you set this, and AURA has not changed it",
        }
    }
}

impl fmt::Display for StyleCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One thing that moved, or did not move, a styled edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StyleReason {
    /// The stable code.
    pub code: StyleCode,
    /// The sentence, rendered from the code and whatever numbers it names.
    pub text: String,
    /// How much confidence it cost. Negative is doubt.
    pub weight: f32,
}

impl StyleReason {
    /// A reason whose sentence is the code's own.
    #[must_use]
    pub fn plain(code: StyleCode, weight: f32) -> Self {
        Self {
            code,
            text: code.user_text().to_string(),
            weight,
        }
    }

    /// A reason with a sentence of its own.
    #[must_use]
    pub fn detailed(code: StyleCode, text: impl Into<String>, weight: f32) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
        }
    }

    /// True when this reason cost confidence.
    #[must_use]
    pub fn is_doubt(&self) -> bool {
        self.weight < 0.0
    }
}

// ---------------------------------------------------------------------------
// What a consumer asks for
// ---------------------------------------------------------------------------

/// Everything outside the profile that resolving one frame's style depends on.
///
/// Assembled by the caller from the frozen services - `StoryService` for the scene,
/// `ToneService` for the light - so `aura-style` keeps taking no dependency on the crates that
/// implement them, which is the same shape phase 16's `FrameContext` has.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StyleQuery {
    /// The project the photograph belongs to, for the per-project and per-chapter selection.
    pub project: Option<ProjectId>,
    /// What phase 07 says the photograph is of.
    pub scene: SceneId,
    /// Which chapter phase 07 put it in, for the per-chapter override.
    pub chapter: ChapterId,
    /// What kind of light phase 15 found on the subject.
    pub lighting: LightingBucket,
    /// The subject's own luminance before the exposure moved, `0..1`.
    ///
    /// One of the six features section 6.2 conditions the regression on, and the one that
    /// separates a photographer who lifts dark frames from one who lifts every frame.
    pub subject_luma: f32,
    /// The colour temperature phase 15 settled on, in kelvin.
    pub cct_k: f32,
    /// The frame's dynamic range in stops.
    pub dynamic_range_ev: f32,
    /// The ISO EXIF recorded. Zero when it had none.
    pub iso: u32,
    /// True when phase 15 found two illuminants disagreeing spatially.
    pub mixed_light: bool,
    /// True when the flash fired.
    pub flash: bool,
}

/// What a profile says about one photograph.
///
/// The delta, which level answered, how sure it is, and why. Invariant 2 lives here: **a
/// [`StyleDelta`] on its own carries a confidence and no reasons**, and this is the shape that
/// carries both, so nothing can apply a style without being able to say where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleAdvice {
    /// Which profile answered, when one did.
    pub profile: Option<ProfileId>,
    /// Which leaf the frame landed in.
    pub bucket: StyleBucket,
    /// The delta to add to phases 15 and 16's decisions, already bounded.
    pub delta: StyleDelta,
    /// Which level of the tree answered.
    pub level: FallbackLevel,
    /// How sure the advice is, `0..1`. Invariant 2.
    pub confidence: f32,
    /// Why, at most [`StyleAdvice::MAX_REASONS`], strongest doubt first.
    pub reasons: Vec<StyleReason>,
}

impl StyleAdvice {
    /// The most reasons one piece of advice carries.
    ///
    /// Six, matching every phase since 09.
    pub const MAX_REASONS: usize = 6;

    /// The advice that changes nothing, with the one reason that says so.
    ///
    /// **This is what a caller gets when there is no profile**, and it is a complete, valid,
    /// explained answer rather than an absence. A `None` here would let a caller render "no
    /// style" as "a style that did nothing", and those are different sentences.
    #[must_use]
    pub fn none(bucket: StyleBucket) -> Self {
        Self {
            profile: None,
            bucket,
            delta: StyleDelta::neutral(),
            level: FallbackLevel::Factory,
            confidence: 0.0,
            reasons: vec![StyleReason::plain(StyleCode::NoProfile, 0.0)],
        }
    }

    /// True when this advice changes nothing about the photograph.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        self.delta.is_neutral()
    }

    /// The reasons that cost confidence, strongest first.
    #[must_use]
    pub fn doubts(&self) -> Vec<&StyleReason> {
        let mut out: Vec<&StyleReason> = self.reasons.iter().filter(|r| r.is_doubt()).collect();
        out.sort_by(|left, right| left.weight.total_cmp(&right.weight));
        out
    }
}

// ---------------------------------------------------------------------------
// The outline
// ---------------------------------------------------------------------------

/// What one project knows about style.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StyleOutline {
    /// Every profile in the catalog, adopted and candidate.
    pub profiles: u32,
    /// The profile this project uses, when one is selected.
    pub active: Option<ProfileId>,
    /// What it is called.
    pub active_name: String,
    /// Which version.
    pub active_version: u16,
    /// How many pairs it was trained on.
    pub trained_pairs: u32,
    /// Its strength, `0..1`.
    pub strength: f32,
    /// Its measured style-match error, in dE00.
    pub overall_de00: f32,
    /// Which chapters have an override, by slug.
    pub chapter_overrides: Vec<String>,
    /// How many of this project's photographs resolved at each level.
    ///
    /// **The number that matters when it is skewed.** A wedding whose frames all resolved at
    /// [`FallbackLevel::Global`] has had its scene conditioning do nothing, which is the quiet
    /// version of "one global style" - the exact thing this phase exists to beat.
    pub level_counts: BTreeMap<String, u32>,
    /// Which build's fitter produced the active profile.
    pub analysis_ver: u16,
}

impl StyleOutline {
    /// The fraction of styled frames that resolved at their own bucket, `0..1`.
    #[must_use]
    pub fn bucket_ratio(&self) -> f64 {
        let total: u32 = self.level_counts.values().copied().sum();
        if total == 0 {
            return 0.0;
        }
        let bucket = self
            .level_counts
            .get(FallbackLevel::Bucket.as_str())
            .copied()
            .unwrap_or(0);
        f64::from(bucket) / f64::from(total)
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The only way to ask what a photographer's own look is.
///
/// Thirteenth service of its kind. Phase 25 normalises a gallery toward these values, 26
/// matches a second shooter to them, 27 checks them, 28 acts on them unattended and 30's
/// learning loop updates them. **No phase may keep its own style profile, its own bucket
/// vocabulary or its own idea of how far a personal look may move a photograph**: two answers
/// to "how does this photographer edit a ceremony" is an album that does not match the gallery,
/// which is the failure this phase was built to prevent in the first place.
///
/// It is also the first service in the product that phases 15 and 16 *consume*, rather than
/// the other way round. The direction is deliberate: the style is applied to the solved
/// parameters and then re-guarded, so the guarantee re-runs after the shift. ADR-0035
/// decision 1.
pub trait StyleService: Send + Sync + fmt::Debug {
    /// What one project knows about style.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    fn outline(&self, project: ProjectId) -> AuraResult<StyleOutline>;

    /// Every profile, newest first.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    fn profiles(&self) -> AuraResult<Vec<StyleProfile>>;

    /// One profile by id, or `None` when nobody has trained it.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    fn profile(&self, id: ProfileId) -> AuraResult<Option<StyleProfile>>;

    /// What this photographer's look says about one photograph.
    ///
    /// **Never fails to answer.** A project with no profile, a bucket with no evidence and a
    /// profile below the confidence floor all return [`StyleAdvice::none`], which is a
    /// complete explained answer that changes nothing - because a caller that had to handle
    /// `None` would be a caller that could forget to.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised while resolving which profile applies.
    fn advise(&self, query: &StyleQuery) -> AuraResult<StyleAdvice>;

    /// Adopt one profile: it becomes what [`StyleService::advise`] answers from.
    ///
    /// Deliberately separate from training. Section 6.3: "adoption is an explicit action", and
    /// a training run that adopted on completion would change how every future photograph
    /// looks as a side effect of a progress bar finishing.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5075` when the profile does not exist or was fitted against a different render
    /// engine.
    fn adopt(&self, id: ProfileId) -> Result<(), AuraError>;

    /// Select which profile a project uses, and optionally which one one chapter uses.
    ///
    /// `profile` of `None` clears the selection. `chapter` of `None` sets the project default.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5075` when the profile does not exist or has not been adopted.
    fn select(
        &self,
        project: ProjectId,
        chapter: Option<ChapterId>,
        profile: Option<ProfileId>,
    ) -> Result<(), AuraError>;
}
