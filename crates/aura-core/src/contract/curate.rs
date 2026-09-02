//! FROZEN CONTRACT. Curation: monochrome suitability, hero photographs, the album sequence, the
//! social sets and the teaser. PHASE-29 section 5.
//!
//! Twenty-eight phases decided things about a photograph, a person, a camera body, a gallery or the
//! product's own work. This one decides what a photographer *does* with the result, and it is the
//! first phase whose subject is a **deliverable**.
//!
//! ## The one property that separates curation from the twenty-eight phases before it
//!
//! Everything here is a matter of taste. A white balance is right or wrong against a measured
//! illuminant; a closed eye is a closed eye; a texture floor is a number a guard either met or did
//! not. Whether the second-best photograph of the first dance belongs on the left-hand page of
//! spread eleven is a judgement two competent photographers can disagree about all afternoon, and
//! the product has no way to be *correct* about it.
//!
//! Every shape in this module is built around that. There is nothing here that changes a
//! photograph, and nothing here that a photographer cannot replace in one click.
//!
//! ## The six properties this contract exists to make structural
//!
//! **Nothing in this phase is applied.** [`CurateService`] has no `apply`, no `deliver` and no
//! `render`. A [`BwMix`] is a *proposal* that maps onto the `bw` block phase 14 already froze in the
//! recipe, and it gets there only when a person accepts it. Section 6.1 of the phase document: black
//! and white "is a taste decision", and a product that could convert a wedding to monochrome on its
//! own is a product that decides a wedding is monochrome. `crates/aura-curate/tests/no_outputs.rs`
//! is the grep that keeps it true.
//!
//! **The band a monochrome mix protects is measured per person, never assumed.** There is no skin
//! constant in this module, and [`BwMix`] carries no field one could go in. The band comes from
//! `ToneService::skin_loci` - phase 15's measurement of that person's own chromaticity across the
//! wedding - and when nobody in the frame has a usable locus the mix is solved on separation alone
//! and says so with [`CurateCode::SkinLocusUnavailable`]. A default skin band would be the constant
//! `docs/skin-fairness.md` says this product does not have, and a monochrome conversion is the
//! easiest place in the product to lighten somebody's face by accident.
//!
//! **Coverage is a filter and never a term.** [`AlbumPlan::coverage`] is computed *over the album*
//! and the frames that satisfy it are placed before any value ranking is consulted. An album is 60
//! to 120 images out of a gallery of hundreds, so it is a far tighter selection than the cull that
//! produced the gallery - and the frames a coverage rule protects are disproportionately frames
//! that scored moderately. A coverage term would lose to two beautiful portraits every time, and the
//! album would arrive without the ring exchange in it. Phase 12 wrote this rule, phase 23 applied it
//! to crop safety, phase 24 made it a property of the type system, phase 27 applied it to
//! replacements; this is its fifth application. ADR-0059 section 7.
//!
//! **Chapter order is inviolable, and three separate things enforce it.** The optimiser only
//! proposes swaps inside one [`ChapterSpan`], `CurateService::set_order` refuses an order that
//! reorders chapters, and the cloud task's moves are validated against the same rule before any of
//! them is applied. A wedding album whose ceremony follows its reception is not an album with an
//! unusual sequence; it is an album that is wrong, and no rhythm score is worth it.
//!
//! **Unmeasurable is a third value, never a zero and never full marks.**
//! [`AlbumPlan::rhythm_measurable`] is the share of the album whose shot scale could be measured at
//! all, and [`SpreadPair::facing_known`] is the same distinction one level down. On a build whose
//! face detector finds no faces, both are near zero - so a rhythm score of 1.000 is a claim about
//! eight per cent of an album, and a panel that rendered it as a claim about the album would be
//! lying on every wedding. Phase 27's rule, in the phase where it is cheapest to get wrong.
//!
//! **A caption may only contain words the wedding itself supplied.** [`Caption::grounded`] is set by
//! a check that runs the safe way round: a closed vocabulary built from this project's chapters,
//! rituals, scenes and role words, plus function words that carry no facts. A caption is accepted
//! when *every* content word in it is in that set. A blocklist of names cannot enumerate names, and
//! a model asked politely not to invent a venue will occasionally invent a venue. ADR-0059 section
//! 10.
//!
//! ## The one thing a later phase can get wrong
//!
//! **Curation reads a finished gallery and its denominator is the gallery, not the project.**
//! [`CurationOutline::selected`] is what everything here is measured against, as phase 18's mask
//! coverage is. A caller that ran curation before phase 12 had selected anything would be curating
//! nothing; one that measured coverage against every photograph in the project would report an album
//! as having missed four fifths of a wedding it was never asked about.

use std::cmp::Ordering;
use std::fmt;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::contract::cull::{CoverageReport, MustHave};
use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{IdentityId, MomentId, ProjectId, SpreadId};
use crate::contract::scene::{ChapterId, SceneId};

pub use crate::contract::scene::ImageId;

/// The aspect a social or album variant is delivered at.
///
/// Section 5 of the phase document names this `AspectVariant`. It **is** phase 23's [`Aspect`],
/// re-exported under section 5's name rather than redefined, because there is no second aspect
/// vocabulary in this product: `GeometryService` decides which crops of a frame are safe, and a
/// curation surface that invented a sixth ratio would be offering a crop nobody had checked.
///
/// [`Aspect`]: crate::contract::geometry::Aspect
pub use crate::contract::geometry::Aspect as AspectVariant;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The smallest album this product will compose, in images.
///
/// Section 2.1's own lower bound. Below sixty an album is a selection rather than a story, and the
/// coverage guarantees alone can consume most of it - twelve must-haves plus close family in a large
/// wedding party is thirty slots before a single frame is chosen for being good.
pub const ALBUM_MIN: u32 = 60;

/// The largest album this product will compose, in images. Section 2.1's own upper bound.
pub const ALBUM_MAX: u32 = 120;

/// The album size a photographer gets without asking. Section 2.1's default range, at its middle.
pub const ALBUM_DEFAULT: u32 = 80;

/// Images on a full spread. Two: this is a book.
pub const IMAGES_PER_SPREAD: u32 = 2;

/// The smallest teaser set. Section 2.1's own lower bound.
pub const TEASER_MIN: u32 = 15;

/// The largest teaser set. Section 2.1's own upper bound.
pub const TEASER_MAX: u32 = 30;

/// Images in the Instagram grid set. Section 6.4's "grid of 10", and its quota sums to exactly this.
pub const GRID_SIZE: u32 = 10;

/// Images in the story set.
///
/// Eight rather than ten, and it is a judgement rather than a measurement: a story set is watched
/// through once at about five seconds a frame, and past forty seconds the last frames are seen by
/// nobody. Section 6.4 names the set without naming its size.
pub const STORY_SIZE: u32 = 8;

/// How many portfolio heroes one wedding produces.
///
/// Twenty, which is section 10.1's own number - the headline gate is "top-20 overlap with
/// photographer picks" - so any other value would make the phase's own KPI unmeasurable against its
/// own output.
pub const HERO_TARGET: u32 = 20;

/// The most heroes one chapter may contribute.
///
/// Section 6.2's "at most N heroes per chapter", as a number. Four of twenty is a fifth of a
/// portfolio from one part of the day, which is the largest share that still leaves room for the
/// eight other chapters to be represented at all.
pub const MAX_HEROES_PER_CHAPTER: u32 = 4;

/// The lowest technical score a portfolio hero may have.
///
/// A **veto**, applied before any score is computed, in the sense phase 12 gave the word: a frame
/// below this is not a worse candidate, it is not a candidate. Portfolio work is looked at closely
/// by people deciding whether to hire somebody, and a soft frame in that set costs more than a
/// missing one.
///
/// Deliberately well above phase 12's own floor. Phase 12 asks "does this belong in the record of
/// the day", and the answer for a slightly soft photograph of the only ring exchange is yes. This
/// asks "does this belong on a website", and the answer is no.
pub const HERO_TECHNICAL_FLOOR: f32 = 0.55;

/// The most reasons one pick carries.
///
/// Four, matching phases 12 and 27 rather than phases 09 to 11's six. A pick with six reasons is a
/// pick nobody believes, and this is a phase whose whole product is a sentence somebody reads before
/// disagreeing with it.
pub const MAX_REASONS: usize = 4;

/// How far apart two facing frames' tonal weights may be, `0..1`.
///
/// Above this the pair is refused rather than scored, and the spread is left single. A dark
/// candlelit frame facing a white high-key portrait is not a spread a photographer would print; it
/// is two photographs that happen to be adjacent.
pub const MAX_PAIR_TONAL_GAP: f32 = 0.34;

/// How similar two frames may be and still face each other, `0..1` cosine similarity.
///
/// A **hard constraint**, not a term. Section 10.1: "no facing near-duplicates". A photographer
/// looking at a spread of the same photograph twice does not think the pairing objective weighted
/// something poorly - they think nobody looked at it, and there is no weight at which that is
/// acceptable. ADR-0059 section 9.
pub const MAX_PAIR_SIMILARITY: f32 = 0.92;

/// How far apart two facing frames' colour temperatures may be, in kelvin.
///
/// Measured *after* phase 25's normalisation, so this is the residual disagreement a consistent
/// gallery still carries. Eight hundred kelvin is about where two frames stop reading as the same
/// room.
pub const MAX_PAIR_WARMTH_GAP_K: f32 = 800.0;

/// The most passes the rhythm-and-pairing optimiser makes over one album.
///
/// Bounded rather than converged, and deterministic in order, because invariant 4 requires the same
/// gallery to produce the same album on every machine. ADR-0059 section 12 records why an annealer
/// was rejected.
pub const MAX_SWAP_PASSES: u32 = 4;

/// The most moves one cloud sequencing answer may propose. Section 7's own schema, as a constant.
pub const MAX_MOVES: usize = 20;

/// The most captions one cloud answer may propose. Section 7's own schema.
pub const MAX_CAPTIONS: usize = 24;

/// The most words a caption may contain. Section 7's system-prompt contract, verbatim.
pub const CAPTION_MAX_WORDS: usize = 12;

/// The most characters a caption may contain. Section 7's JSON schema, verbatim.
pub const CAPTION_MAX_CHARS: usize = 90;

/// The most cloud calls one wedding's curation may make. Section 7's cost control.
pub const MAX_CLOUD_CALLS: u32 = 15;

/// The largest luminance shift a monochrome mix may apply to any one band, in the recipe's units.
///
/// The `bw.mix` block phase 14 froze stores `-100 ..= 100` per band; this is the ceiling curation
/// may propose within it. Seventy rather than a hundred because a band driven to its limit is a band
/// with no tonal information left in it, which is a flat patch rather than a separation.
pub const MAX_BAND_SHIFT: i16 = 70;

/// The largest luminance shift a monochrome mix may apply to a band somebody's skin sits in.
///
/// Much smaller than [`MAX_BAND_SHIFT`], and it is the one bound in this module that is about a
/// person rather than about a photograph. The objective of a mix is **separation**, never lightness:
/// a mix may move the bands somebody's skin is competing with as far as the general ceiling allows,
/// and may barely move the skin itself. Section 11 of the operating manual forbids skin lightening
/// outright, and a monochrome conversion is where it would be least visible.
pub const MAX_SKIN_BAND_SHIFT: i16 = 18;

/// The lowest suitability at which a frame is offered as a monochrome candidate.
///
/// Section 10.1's gate is that B&W picks are accepted at 70 %, which is a statement about the
/// *offered* set. A threshold this high produces a short list a photographer can look through,
/// which is the shape that gate can be met in; a low one produces four hundred candidates and an
/// acceptance rate measured on a list nobody read to the end.
pub const BW_CANDIDATE_FLOOR: f32 = 0.62;

/// The longest note a photographer may attach to a decision, in bytes.
pub const MAX_NOTE: usize = 500;

// ---------------------------------------------------------------------------
// Shot scale
// ---------------------------------------------------------------------------

/// How close the photographer was, as far as it can be told.
///
/// Section 6.3's rhythm is "alternate wide establishing, medium action and tight emotional frames",
/// which needs a scale per frame. This is that scale, and it is **measured** rather than predicted:
/// from the largest face's area fraction where phase 06 found faces, and from the scene label where
/// the scene's scale is known by definition - a detail is tight, a venue establishing shot is wide.
///
/// [`ShotScale::Unknown`] is a real and, on this build, common answer. A frame with no face and an
/// ordinary scene cannot be scaled from stored numbers, and it is **excluded from the rhythm score's
/// denominator** rather than counted as a miss. Phase 27's rule: clean and skipped are different
/// values.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ShotScale {
    /// The room, the landscape, the crowd. Establishing.
    Wide,
    /// A person or a group at working distance. Where most action lives.
    Medium,
    /// A face, two hands, a ring. Where most emotion lives.
    Tight,
    /// Nothing stored could say. The default, because it claims the least.
    #[default]
    Unknown,
}

impl ShotScale {
    /// Every scale, wide to tight, with [`ShotScale::Unknown`] last.
    pub const ALL: [Self; 4] = [Self::Wide, Self::Medium, Self::Tight, Self::Unknown];

    /// How many there are.
    pub const COUNT: usize = 4;

    /// The stored text, on the wire and in the catalog.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wide => "wide",
            Self::Medium => "medium",
            Self::Tight => "tight",
            Self::Unknown => "unknown",
        }
    }

    /// Parse the stored text. Anything unrecognised reads as [`ShotScale::Unknown`].
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text {
            "wide" => Self::Wide,
            "medium" => Self::Medium,
            "tight" => Self::Tight,
            _ => Self::Unknown,
        }
    }

    /// True when this scale was actually measured.
    #[must_use]
    pub const fn is_known(self) -> bool {
        !matches!(self, Self::Unknown)
    }

    /// The words a photographer reads.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Wide => "wide",
            Self::Medium => "medium",
            Self::Tight => "close",
            Self::Unknown => "not measured",
        }
    }
}

// ---------------------------------------------------------------------------
// The monochrome mix
// ---------------------------------------------------------------------------

/// The eight hue bands a monochrome mix is expressed in, in the recipe's own order.
///
/// The same eight `aura_recipe::HSL_BANDS` names and the same eight
/// [`crate::contract::colour::HslBand`] variants. Not a ninth vocabulary: a mix this phase proposes
/// is written into the `bw.mix` block phase 14 froze, key for key.
pub const MIX_BANDS: [&str; 8] = [
    "red", "orange", "yellow", "green", "aqua", "blue", "purple", "magenta",
];

/// A per-frame monochrome conversion: how much each hue band contributes to grey.
///
/// Eight numbers in the recipe's own units - `-100 ..= 100` per band, zero meaning "convert this
/// band the way a neutral desaturation would" - so [`BwMix::to_recipe_mix`] is a rename rather than
/// a translation.
///
/// # What is deliberately not here
///
/// **No contrast, no curve, no grade name.** A monochrome conversion wants a contrast decision and
/// this shape cannot express one, on purpose: phase 16's `ColourService` is the only way to ask how
/// a photograph should be graded, and a B&W proposal that also re-graded would be a second answer to
/// that question living in a panel nobody associates with tone. A photographer who wants more
/// contrast in a monochrome frame has the tone panel that phase 16 shipped.
///
/// **No strength.** Section 6.1 asks for "a tailored B&W mix per frame rather than a single
/// preset", and a preset scaled by a number is a preset. There is no strength on the IPC surface
/// either. ADR-0060 section 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BwMix {
    /// Per-band luminance weight, in [`MIX_BANDS`] order, each `-100 ..= 100`.
    pub bands: [i16; 8],
}

impl BwMix {
    /// A neutral desaturation: every band at zero.
    #[must_use]
    pub const fn neutral() -> Self {
        Self { bands: [0; 8] }
    }

    /// True when every band is inside [`MAX_BAND_SHIFT`].
    ///
    /// The general ceiling only. [`BwMix::within_skin_bound`] is the per-person one, and it needs to
    /// be told which bands somebody's skin was measured into - which is not something a mix can know
    /// about itself.
    #[must_use]
    pub fn within_bounds(&self) -> bool {
        self.bands.iter().all(|v| v.abs() <= MAX_BAND_SHIFT)
    }

    /// True when no band that somebody's skin sits in moved further than [`MAX_SKIN_BAND_SHIFT`].
    ///
    /// `skin_bands` is the set of band indices `ToneService::skin_loci` put the frame's identities
    /// in. An empty set means nobody in the frame has a usable locus, in which case there is no skin
    /// bound to check and this is vacuously true - which is honest, and is why
    /// [`CurateCode::SkinLocusUnavailable`] is a reason a caller has to emit separately rather than
    /// something this predicate could imply.
    #[must_use]
    pub fn within_skin_bound(&self, skin_bands: &[usize]) -> bool {
        skin_bands.iter().all(|ix| {
            self.bands
                .get(*ix)
                .is_none_or(|v| v.abs() <= MAX_SKIN_BAND_SHIFT)
        })
    }

    /// The mix as the recipe's own `bw.mix` map: band name to weight.
    ///
    /// Pairs with [`MIX_BANDS`] in order. A caller writes this into `aura_recipe::Bw::mix` when a
    /// photographer accepts the proposal, and never before - nothing in phase 29 writes a recipe.
    #[must_use]
    pub fn to_recipe_mix(&self) -> Vec<(&'static str, i16)> {
        MIX_BANDS
            .iter()
            .copied()
            .zip(self.bands.iter().copied())
            .collect()
    }

    /// How far this mix departs from a neutral desaturation, as a mean absolute band shift.
    ///
    /// The number the panel shows beside the bar chart, so "this is nearly a plain desaturation" and
    /// "this is a heavily filtered conversion" are visibly different suggestions.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn departure(&self) -> f32 {
        let total: i32 = self.bands.iter().map(|v| i32::from(v.abs())).sum();
        total as f32 / 8.0
    }
}

/// The five measured terms a monochrome suitability score is built from.
///
/// Section 6.1's own list, each `0..1`. They travel beside the score rather than being folded away
/// because "why does this frame suit black and white" is answered by which of the five is high, and
/// a panel with only a score leaves a photographer comparing two numbers that differ by 0.02.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BwTerms {
    /// How far apart the frame's tones sit once the colour is gone, `0..1`.
    pub tonal_separation: f32,
    /// How much saturated colour is pulling attention away from the subject, `0..1`.
    pub colour_distraction: f32,
    /// How strongly the frame is carried by what people are doing, `0..1`.
    pub gesture: f32,
    /// Phase 10's emotion score for the frame, `0..1`.
    pub emotion: f32,
    /// How well this frame's noise would read as grain, `0..1`.
    pub grain: f32,
}

impl BwTerms {
    /// Every term, with the name the panel labels it with.
    #[must_use]
    pub fn labelled(&self) -> [(&'static str, f32); 5] {
        [
            ("tonal separation", self.tonal_separation),
            ("colour distraction", self.colour_distraction),
            ("gesture", self.gesture),
            ("emotion", self.emotion),
            ("grain", self.grain),
        ]
    }

    /// True when every term is a finite number inside `0..=1`.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.labelled()
            .iter()
            .all(|(_, v)| v.is_finite() && (0.0..=1.0).contains(v))
    }
}

/// One frame offered for monochrome, with the mix solved for it.
///
/// Section 5 writes this as `(ImageId, BwMix, f32)`. The triple's three members are the first three
/// fields here, in that order; the rest is what section 13's fourth acceptance criterion - "every
/// pick is explained" - requires and a bare tuple has nowhere to put. ADR-0059 section 4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BwPick {
    /// The photograph.
    pub image_id: ImageId,
    /// The mix solved for this frame and no other.
    pub mix: BwMix,
    /// Suitability, `0..1`. Above [`BW_CANDIDATE_FLOOR`] or it is not offered at all.
    pub score: f32,
    /// What the score was built from.
    pub terms: BwTerms,
    /// Which bands somebody's measured skin locus put them in, if anybody's did.
    ///
    /// Empty when no identity in the frame has a usable locus - which is not the same as "no people
    /// in the frame" and is why [`CurateCode::SkinLocusUnavailable`] exists as its own reason.
    pub skin_bands: Vec<u8>,
    /// Why, strongest first. At most [`MAX_REASONS`].
    pub reasons: Vec<CurateReason>,
    /// How sure this suggestion is, `0..1`. Invariant 2.
    pub confidence: f32,
    /// What the photographer said, when they have said anything.
    pub accepted: Option<bool>,
}

impl BwPick {
    /// True when the shape can be stored and rendered without a caller having to guess.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.score.is_finite()
            && (0.0..=1.0).contains(&self.score)
            && self.confidence.is_finite()
            && (0.0..=1.0).contains(&self.confidence)
            && self.terms.is_well_formed()
            && self.mix.within_bounds()
            && !self.reasons.is_empty()
            && self.reasons.len() <= MAX_REASONS
            && self.mix.within_skin_bound(
                &self
                    .skin_bands
                    .iter()
                    .map(|b| *b as usize)
                    .collect::<Vec<_>>(),
            )
    }
}

// ---------------------------------------------------------------------------
// Heroes
// ---------------------------------------------------------------------------

/// Which diversity constraint was binding when a hero was chosen.
///
/// Section 6.2 enforces three: at most [`MAX_HEROES_PER_CHAPTER`] per chapter, at most one per
/// moment, and a spread across shot scales. This says which of them eliminated the higher-scoring
/// candidates that were passed over to reach this pick.
///
/// It is on the wire because "why is this one a hero and that one not" is answered by the constraint
/// far more often than by the score. Two frames from the same kiss can differ by 0.004; what decided
/// between them was that one of them was in a moment already represented.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum HeroBinding {
    /// Nothing was in the way: this was simply the next best frame.
    #[default]
    Unconstrained,
    /// A chapter had already contributed [`MAX_HEROES_PER_CHAPTER`] heroes.
    ChapterQuota,
    /// A higher-scoring frame came from a moment already represented.
    MomentExhausted,
    /// A higher-scoring frame would have made the set too uniform in shot scale.
    ScaleQuota,
}

impl HeroBinding {
    /// Every binding.
    pub const ALL: [Self; 4] = [
        Self::Unconstrained,
        Self::ChapterQuota,
        Self::MomentExhausted,
        Self::ScaleQuota,
    ];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unconstrained => "unconstrained",
            Self::ChapterQuota => "chapter_quota",
            Self::MomentExhausted => "moment_exhausted",
            Self::ScaleQuota => "scale_quota",
        }
    }

    /// Parse the stored text. Anything unrecognised reads as [`HeroBinding::Unconstrained`].
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text {
            "chapter_quota" => Self::ChapterQuota,
            "moment_exhausted" => Self::MomentExhausted,
            "scale_quota" => Self::ScaleQuota,
            _ => Self::Unconstrained,
        }
    }

    /// The sentence the hero grid shows under a pick.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::Unconstrained => "the next strongest frame in the wedding",
            Self::ChapterQuota => {
                "stronger frames were passed over because their part of the day already had four"
            }
            Self::MomentExhausted => {
                "stronger frames were passed over because they were the same shot again"
            }
            Self::ScaleQuota => {
                "stronger frames were passed over to keep the set from being all close-ups"
            }
        }
    }
}

/// The five terms a hero score is built from.
///
/// Section 6.2's own list, each `0..1`. A **weighted arithmetic blend** rather than phase 12's
/// geometric mean, and ADR-0059 section 6 has the argument: culling decides what is delivered and a
/// near-zero term must be able to drag the whole thing down, whereas a portfolio is a ranking among
/// frames that already passed that test, so a second multiplicative penalty would re-rank a
/// technically sound set by technical quality again.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeroTerms {
    /// Phase 09's technical score, `0..1`. Also the veto: below [`HERO_TECHNICAL_FLOOR`] there is no
    /// candidate to score.
    pub technical: f32,
    /// Phase 10's emotion score, `0..1`.
    pub emotion: f32,
    /// Phase 11's composition score, `0..1`.
    pub composition: f32,
    /// How unlike the already-chosen heroes this frame is, `0..1`, from the phase 05 index.
    pub uniqueness: f32,
    /// How much this moment matters to the wedding's story, `0..1`.
    pub story: f32,
}

impl HeroTerms {
    /// Every term, with the name the panel labels it with.
    #[must_use]
    pub fn labelled(&self) -> [(&'static str, f32); 5] {
        [
            ("technical", self.technical),
            ("emotion", self.emotion),
            ("composition", self.composition),
            ("uniqueness", self.uniqueness),
            ("story", self.story),
        ]
    }

    /// True when every term is a finite number inside `0..=1`.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.labelled()
            .iter()
            .all(|(_, v)| v.is_finite() && (0.0..=1.0).contains(v))
    }
}

/// One portfolio pick.
///
/// Section 5 writes this as `(ImageId, f32, Vec<Reason>)`. Those three are the first, third and
/// seventh fields; the rest is what a hero grid needs to explain itself. ADR-0059 section 4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeroPick {
    /// The photograph.
    pub image_id: ImageId,
    /// Where it came, `0` first. Stable across a re-run given the same gallery. Invariant 4.
    pub rank: u32,
    /// The blended score, `0..1`.
    pub score: f32,
    /// What the score was built from.
    pub terms: HeroTerms,
    /// Which part of the day it came from.
    pub chapter: ChapterId,
    /// The moment it came from, when it is in one.
    pub moment: Option<MomentId>,
    /// How close the photographer was, as far as it could be told.
    pub scale: ShotScale,
    /// Which diversity constraint was binding when this pick was made.
    pub binding: HeroBinding,
    /// Why, strongest first. At most [`MAX_REASONS`].
    pub reasons: Vec<CurateReason>,
    /// How sure this suggestion is, `0..1`. Invariant 2.
    pub confidence: f32,
    /// What the photographer said, when they have said anything.
    pub accepted: Option<bool>,
}

impl HeroPick {
    /// True when the shape can be stored and rendered without a caller having to guess.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.score.is_finite()
            && (0.0..=1.0).contains(&self.score)
            && self.confidence.is_finite()
            && (0.0..=1.0).contains(&self.confidence)
            && self.terms.is_well_formed()
            && self.terms.technical >= HERO_TECHNICAL_FLOOR
            && !self.reasons.is_empty()
            && self.reasons.len() <= MAX_REASONS
    }
}

// ---------------------------------------------------------------------------
// The album
// ---------------------------------------------------------------------------

/// How well two facing photographs work together.
///
/// Four measurements and a combined score. All four are on the wire because a photographer who
/// disagrees with a pairing wants to know *which* of the four the optimiser was happy with -
/// "these two are the same tonal weight but one is much warmer" is actionable and a single number
/// is not.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpreadPair {
    /// Difference in tonal weight, `0..1`. Refused above [`MAX_PAIR_TONAL_GAP`].
    pub tonal_gap: f32,
    /// Difference in colour temperature after phase 25's normalisation, in kelvin.
    pub warmth_gap_k: f32,
    /// How well the subjects face inward, `0..1`. Zero when [`SpreadPair::facing_known`] is false.
    pub facing_score: f32,
    /// Whether anything could be measured about which way the subjects are facing.
    ///
    /// **Not a quality number.** A spread whose facing could not be measured is not a spread whose
    /// subjects face outward; it is a spread nobody could check, it scores zero on that term rather
    /// than full marks, and this is what tells a panel to render it in grey rather than in red.
    /// Phase 24's rule: an absent input is ignorance, not permission. On this build - where phase
    /// 06's detector finds no faces - this is false almost everywhere.
    pub facing_known: bool,
    /// How alike the two frames are, `0..1` cosine similarity. Refused above
    /// [`MAX_PAIR_SIMILARITY`], and refused outright when both come from the same moment.
    pub similarity: f32,
    /// The combined pairing score, `0..1`.
    pub score: f32,
}

impl SpreadPair {
    /// The pair a single-image spread has: nothing measured, nothing claimed.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            tonal_gap: 0.0,
            warmth_gap_k: 0.0,
            facing_score: 0.0,
            facing_known: false,
            similarity: 0.0,
            score: 0.0,
        }
    }

    /// True when this pair breaks none of the hard constraints.
    ///
    /// The two hard ones only. `warmth_gap_k` is a *term*, and a pair whose warmth is 900 K apart is
    /// a worse pair rather than a refused one - which is a different judgement from the two here and
    /// is why it is not in this predicate.
    #[must_use]
    pub fn is_permitted(&self) -> bool {
        self.tonal_gap <= MAX_PAIR_TONAL_GAP && self.similarity <= MAX_PAIR_SIMILARITY
    }
}

/// Two facing pages.
///
/// Section 5 writes `{ left: Option<ImageId>, right: Option<ImageId>, single: bool }`. Those three
/// are here unchanged; the other four are what a spread has to carry to be reordered, explained and
/// pointed at after an edit. ADR-0059 section 4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spread {
    /// Stable across a reorder, which is what makes an accepted pairing survive a drag.
    pub id: SpreadId,
    /// Where it sits in the album, `0` first.
    pub index: u32,
    /// The left-hand page.
    pub left: Option<ImageId>,
    /// The right-hand page.
    pub right: Option<ImageId>,
    /// True when this spread carries one image across both pages.
    pub single: bool,
    /// The chapter this spread belongs to. Never changes once the album is allocated.
    pub chapter: ChapterId,
    /// How well the two frames work together. [`SpreadPair::none`] on a single.
    pub pair: SpreadPair,
    /// Why these two, or why this one alone. At most [`MAX_REASONS`].
    pub reasons: Vec<CurateReason>,
}

impl Spread {
    /// Every image on this spread, left first.
    #[must_use]
    pub fn images(&self) -> Vec<ImageId> {
        [self.left, self.right].into_iter().flatten().collect()
    }

    /// How many images this spread carries. Zero, one or two.
    #[must_use]
    pub fn len(&self) -> usize {
        self.images().len()
    }

    /// True when the spread carries nothing. Never produced by the composer; refused by the store.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.left.is_none() && self.right.is_none()
    }

    /// True when the shape is self-consistent.
    ///
    /// A single carries exactly one image and an empty spread is not a spread. The composer leaves a
    /// page blank rather than a spread empty: a chapter that ran one image short ends on a single,
    /// which is a design a photographer recognises, rather than on a blank opening.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        if self.is_empty() {
            return false;
        }
        if self.single {
            return self.len() == 1;
        }
        self.reasons.len() <= MAX_REASONS
    }
}

/// Which spreads belong to one chapter.
///
/// Section 5 writes `(ChapterId, Range<usize>)`. This is that pair as a struct, plus the number of
/// spreads the allocator *wanted* to give the chapter - which is the number the panel shows when a
/// chapter came up short, and which a `Range` cannot carry. `std::ops::Range` is also not `Copy`,
/// not `Ord` and does not round-trip through serde the way the rest of this contract does.
/// ADR-0059 section 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterSpan {
    /// The chapter.
    pub chapter: ChapterId,
    /// Index of its first spread.
    pub first: u32,
    /// How many spreads it got. Zero is legal and means the chapter is not in the album.
    pub len: u32,
    /// How many spreads the allocator wanted to give it.
    ///
    /// Equal to `len` on a chapter that had enough frames. Larger when the chapter ran out, which is
    /// the case [`CurateCode::ChapterUnderAllocated`] reports.
    pub target: u32,
}

impl ChapterSpan {
    /// The half-open range of spread indices, which is what section 5's `Range` meant.
    #[must_use]
    pub const fn range(&self) -> (u32, u32) {
        (self.first, self.first + self.len)
    }

    /// True when the chapter got everything the allocator asked for.
    #[must_use]
    pub const fn is_satisfied(&self) -> bool {
        self.len >= self.target
    }
}

/// A sequenced album.
///
/// Section 5's shape, with `chapter_map` carried as [`ChapterSpan`]s and four counters added that
/// section 13 requires and section 5's fields have nowhere to put.
///
/// # Why this shape and [`CurationResult`] carry no `serde` derive
///
/// [`CoverageReport`] does not have one - it is phase 12's frozen shape and adding a derive to it
/// would be an amendment to a frozen contract for the sake of a convenience. That is the smaller
/// half of the reason. The larger half is that `CurateService::export` publishes the album as a
/// **specification another tool reads**, and a derived serialiser makes that format a consequence
/// of Rust field names: renaming `rhythm_measurable` would silently change a published format under
/// every album-design script in the world. `aura_curate::export` writes it by hand with a
/// documented key order, and `docs/curation.md` is where the format lives.
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumPlan {
    /// The spreads, in album order.
    pub spreads: Vec<Spread>,
    /// Which spreads belong to which chapter, in wedding order.
    pub chapter_map: Vec<ChapterSpan>,
    /// What the **album** guarantees.
    ///
    /// Computed over the album rather than over the gallery, which is the whole point: phase 12
    /// already reported that the gallery covers the ring exchange, and the question here is whether
    /// the album does. Same vocabulary, different set. ADR-0059 section 7.
    pub coverage: CoverageReport,
    /// How well the sequence alternates wide, medium and tight, `0..1`.
    ///
    /// **Read it with [`AlbumPlan::rhythm_measurable`].** Measured only over frames whose shot scale
    /// could be measured at all.
    pub rhythm_score: f32,
    /// The share of the album whose shot scale could be measured, `0..1`.
    ///
    /// On this build - where phase 06's detector finds no faces - this is low, and a rhythm score of
    /// 1.000 over eight per cent of an album is not a claim about the album.
    pub rhythm_measurable: f32,
    /// The mean pairing score over the spreads that carry two images, `0..1`.
    pub pairing_score: f32,
    /// How many images the album carries.
    pub size: u32,
    /// How many were asked for.
    pub target_size: u32,
    /// True when a photographer has reordered this album by hand.
    ///
    /// A re-run never overwrites an order somebody set; it reports what it would have done instead.
    /// The operating manual's fifth code rule, and the mechanism phase 30's learning loop reads.
    pub user_ordered: bool,
    /// Everything a photographer should read about this album. At most [`MAX_REASONS`] per subject,
    /// unbounded here because an album-level note is one line per finding.
    pub reasons: Vec<CurateReason>,
}

impl AlbumPlan {
    /// An album with nothing in it. What a project with no gallery produces.
    #[must_use]
    pub fn empty(target_size: u32) -> Self {
        Self {
            spreads: Vec::new(),
            chapter_map: Vec::new(),
            coverage: CoverageReport {
                must_haves: Vec::new(),
                identity_coverage: Vec::new(),
                chapter_counts: Vec::new(),
                warnings: Vec::new(),
            },
            rhythm_score: 0.0,
            rhythm_measurable: 0.0,
            pairing_score: 0.0,
            size: 0,
            target_size,
            user_ordered: false,
            reasons: Vec::new(),
        }
    }

    /// Every image in the album, in album order.
    #[must_use]
    pub fn images(&self) -> Vec<ImageId> {
        self.spreads.iter().flat_map(Spread::images).collect()
    }

    /// The chapters that got fewer spreads than the allocator wanted.
    #[must_use]
    pub fn under_allocated(&self) -> Vec<ChapterId> {
        self.chapter_map
            .iter()
            .filter(|span| !span.is_satisfied())
            .map(|span| span.chapter)
            .collect()
    }

    /// True when the spreads are in chapter order and every chapter's span is contiguous.
    ///
    /// **The one invariant of this shape**, and the reason it is a method rather than a comment:
    /// `set_order` checks it before storing a photographer's reorder, the cloud validator checks it
    /// before applying a move, and the phase gate checks it on every album it builds. A wedding
    /// album whose ceremony follows its reception is not an album with an unusual sequence.
    #[must_use]
    pub fn chapters_are_ordered(&self) -> bool {
        let mut next = 0u32;
        for span in &self.chapter_map {
            if span.first != next {
                return false;
            }
            next = next.saturating_add(span.len);
            let ordered = ChapterId::ALL
                .iter()
                .position(|c| *c == span.chapter)
                .is_some();
            if !ordered {
                return false;
            }
        }
        if next as usize != self.spreads.len() {
            return false;
        }
        // Chapters appear at most once, and in the vocabulary's own order.
        let mut last: Option<usize> = None;
        for span in &self.chapter_map {
            let Some(ix) = ChapterId::ALL.iter().position(|c| *c == span.chapter) else {
                return false;
            };
            if last.is_some_and(|prev| ix <= prev) {
                return false;
            }
            last = Some(ix);
        }
        // Every spread agrees with the span it sits in.
        for span in &self.chapter_map {
            for offset in 0..span.len {
                let Some(spread) = self.spreads.get((span.first + offset) as usize) else {
                    return false;
                };
                if spread.chapter != span.chapter {
                    return false;
                }
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Social and teaser
// ---------------------------------------------------------------------------

/// What a slot in the grid set is for.
///
/// Section 6.4's quota verbatim: "one hero, two portraits, two details, two candids, two
/// family/group and one exit-style frame". The variants are the quota, and [`SocialSlot::GRID_QUOTA`]
/// sums to [`GRID_SIZE`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum SocialSlot {
    /// The single strongest frame.
    #[default]
    Hero,
    /// A portrait of one or both of the couple.
    Portrait,
    /// An object: rings, flowers, the dress, the cake.
    Detail,
    /// An unposed frame of people.
    Candid,
    /// Family or a group.
    Group,
    /// The departure, or whatever ends the day.
    Exit,
}

impl SocialSlot {
    /// Every slot, in the order the grid is filled.
    pub const ALL: [Self; 6] = [
        Self::Hero,
        Self::Portrait,
        Self::Detail,
        Self::Candid,
        Self::Group,
        Self::Exit,
    ];

    /// Section 6.4's quota: how many of each the grid set carries. Sums to [`GRID_SIZE`].
    pub const GRID_QUOTA: [(Self, u32); 6] = [
        (Self::Hero, 1),
        (Self::Portrait, 2),
        (Self::Detail, 2),
        (Self::Candid, 2),
        (Self::Group, 2),
        (Self::Exit, 1),
    ];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hero => "hero",
            Self::Portrait => "portrait",
            Self::Detail => "detail",
            Self::Candid => "candid",
            Self::Group => "group",
            Self::Exit => "exit",
        }
    }

    /// Parse the stored text. Anything unrecognised reads as [`SocialSlot::Candid`], which is the
    /// least specific claim about a photograph rather than the first variant.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text {
            "hero" => Self::Hero,
            "portrait" => Self::Portrait,
            "detail" => Self::Detail,
            "group" => Self::Group,
            "exit" => Self::Exit,
            _ => Self::Candid,
        }
    }

    /// How many of this slot the grid set wants.
    #[must_use]
    pub fn grid_quota(self) -> u32 {
        Self::GRID_QUOTA
            .iter()
            .find(|(slot, _)| *slot == self)
            .map_or(0, |(_, n)| *n)
    }
}

/// One frame chosen for a social set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialPick {
    /// The photograph.
    pub image_id: ImageId,
    /// Which crop it is delivered at. Only ever a variant `GeometryService` says is safe.
    pub aspect: AspectVariant,
    /// What this frame is doing in the set.
    pub slot: SocialSlot,
    /// How well it reads at thumbnail size, `0..1`.
    ///
    /// Section 6.4's "chosen for thumbnail legibility (strong subject, clear silhouette at small
    /// size)". A frame that is beautiful at full size and mud at 150 px is a frame nobody stops
    /// scrolling for.
    pub legibility: f32,
    /// Why. At most [`MAX_REASONS`].
    pub reasons: Vec<CurateReason>,
    /// What the photographer said, when they have said anything.
    pub accepted: Option<bool>,
}

/// Where a caption came from.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum CaptionSource {
    /// Assembled locally from this wedding's own labels. Passes the grounding check by construction.
    #[default]
    Template,
    /// Drafted by a model and **then** checked against the same closed vocabulary.
    Cloud,
}

impl CaptionSource {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Template => "template",
            Self::Cloud => "cloud",
        }
    }

    /// Parse the stored text. Anything unrecognised reads as [`CaptionSource::Template`].
    #[must_use]
    pub fn parse(text: &str) -> Self {
        if text == "cloud" {
            Self::Cloud
        } else {
            Self::Template
        }
    }
}

/// A sentence a photographer may post, or edit, or delete.
///
/// Section 5 keys captions by `ImageId` and section 7's JSON schema keys them by chapter. Both are
/// here: a caption with no `image_id` is a chapter caption, which is what the cloud task returns and
/// what the album's section headings use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caption {
    /// The photograph this belongs to, or `None` for a caption about a whole chapter.
    pub image_id: Option<ImageId>,
    /// Which part of the day it describes.
    pub chapter: ChapterId,
    /// The sentence. At most [`CAPTION_MAX_WORDS`] words and [`CAPTION_MAX_CHARS`] characters.
    pub text: String,
    /// Where it came from.
    pub source: CaptionSource,
    /// Whether every content word in it came from this wedding's own labels.
    ///
    /// **Always true on a stored caption**, because an ungrounded one is replaced by the template
    /// rather than stored with a flag. It is on the shape anyway so that the check has an output a
    /// test can assert on and a panel can show, and so that a future caption path cannot quietly
    /// skip it. Section 10.1's automated grounding check reads this.
    pub grounded: bool,
}

impl Caption {
    /// True when the sentence is inside both bounds.
    #[must_use]
    pub fn within_bounds(&self) -> bool {
        self.text.chars().count() <= CAPTION_MAX_CHARS
            && self.text.split_whitespace().count() <= CAPTION_MAX_WORDS
            && !self.text.trim().is_empty()
    }
}

/// The three sets a photographer posts from.
///
/// Section 5's shape, with `hero` as an `Option`: a project with no keepers has no hero, and a tuple
/// that must exist would force one to be fabricated. ADR-0059 section 4.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialSets {
    /// The grid set. [`GRID_SIZE`] frames when the gallery could fill section 6.4's quota.
    pub grid: Vec<SocialPick>,
    /// The story set. [`STORY_SIZE`] frames.
    pub story: Vec<SocialPick>,
    /// The single strongest frame, when there is one.
    pub hero: Option<SocialPick>,
    /// Captions, grounded in this wedding's own labels.
    pub captions: Vec<Caption>,
}

impl SocialSets {
    /// Which grid slots could not be filled from this gallery.
    ///
    /// Reported rather than substituted. A wedding with no exit photographs gets a nine-image grid
    /// and a sentence, not a tenth frame promoted out of another slot to make the number right.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn unfilled_slots(&self) -> Vec<(SocialSlot, u32)> {
        SocialSlot::GRID_QUOTA
            .iter()
            .filter_map(|(slot, want)| {
                let have = self.grid.iter().filter(|p| p.slot == *slot).count() as u32;
                (have < *want).then_some((*slot, want - have))
            })
            .collect()
    }
}

/// One frame in the wedding-night teaser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeaserPick {
    /// The photograph.
    pub image_id: ImageId,
    /// What it is doing in the set. Section 6.4: "hero, couple, ceremony peak, one family, one
    /// detail, one dance".
    pub slot: SocialSlot,
    /// Where it came, `0` first.
    pub rank: u32,
    /// Why. At most [`MAX_REASONS`].
    pub reasons: Vec<CurateReason>,
    /// What the photographer said, when they have said anything.
    pub accepted: Option<bool>,
}

// ---------------------------------------------------------------------------
// The whole result
// ---------------------------------------------------------------------------

/// Everything one curation pass produced.
///
/// Section 5's shape. `bw`, `heroes` and `teaser` carry the richer per-pick types this module
/// defines rather than tuples and bare ids; ADR-0059 section 4 records each.
#[derive(Debug, Clone, PartialEq)]
pub struct CurationResult {
    /// Frames that would gain from monochrome, best first.
    pub bw: Vec<BwPick>,
    /// The portfolio, best first.
    pub heroes: Vec<HeroPick>,
    /// The album draft.
    pub album: AlbumPlan,
    /// The sets a photographer posts from.
    pub social: SocialSets,
    /// The wedding-night teaser, best first.
    pub teaser: Vec<TeaserPick>,
}

impl CurationResult {
    /// A result with nothing in it. What a project with no gallery produces.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            bw: Vec::new(),
            heroes: Vec::new(),
            album: AlbumPlan::empty(ALBUM_DEFAULT),
            social: SocialSets::default(),
            teaser: Vec::new(),
        }
    }

    /// True when nothing was curated at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bw.is_empty()
            && self.heroes.is_empty()
            && self.album.spreads.is_empty()
            && self.social.grid.is_empty()
            && self.teaser.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why curation did what it did.
///
/// Thirty-nine codes in five groups. A closed vocabulary rather than a sentence, for the reason
/// phase 09 established and phase 27 finished: a stored sentence is copy a release has to maintain,
/// a catalog full of English cannot be translated, and a code is something a gate can count.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum CurateCode {
    // -- monochrome ---------------------------------------------------------------------
    /// The frame's tones stay far apart once the colour is gone.
    #[default]
    StrongTonalSeparation,
    /// Saturated colour away from the subject is pulling the eye; monochrome removes the competition.
    ColourDistraction,
    /// The frame is carried by what people are doing rather than by what colour anything is.
    GestureLed,
    /// The moment is strong enough that the colour is not what anybody will remember.
    HighEmotion,
    /// The noise in this frame would read as grain rather than as noise.
    GrainTolerant,
    /// Desaturating this frame would flatten it: the separation is in the colour.
    FlatWhenDesaturated,
    /// The colour **is** the subject here - a sari, a bouquet, a sunset - so monochrome loses it.
    ColourIsTheSubject,
    /// Nobody in the frame has a measured skin locus, so the mix was solved on separation alone.
    ///
    /// **Not the same as "there are no people in the frame."** ADR-0059 section 5.
    SkinLocusUnavailable,
    /// The mix was held back so that somebody's skin band barely moved.
    SkinSeparationHeld,
    /// The mix wanted to go further than [`MAX_BAND_SHIFT`] and was bounded.
    MixBounded,

    // -- heroes -------------------------------------------------------------------------
    /// Technically excellent: sharp where it matters, exposed, clean.
    TechnicalExcellence,
    /// The peak of its moment.
    EmotionalPeak,
    /// The framing is the strongest thing about it.
    StrongComposition,
    /// Unlike anything else already in the portfolio.
    UniqueFrame,
    /// Its moment matters to the story of the day.
    StoryImportant,
    /// Its chapter had already contributed [`MAX_HEROES_PER_CHAPTER`] heroes.
    ChapterQuotaBinding,
    /// Its moment is already represented in the portfolio.
    MomentAlreadyRepresented,
    /// Taking it would have made the portfolio too uniform in shot scale.
    ScaleQuotaBinding,
    /// Below [`HERO_TECHNICAL_FLOOR`], so never a candidate. A **veto**.
    TechnicalVeto,
    /// The phase 05 descriptors this frame needed are missing, so uniqueness could not be measured.
    UniquenessUnavailable,

    // -- the album ----------------------------------------------------------------------
    /// In the album because a coverage rule protects it. Placed before any ranking was consulted.
    CoverageProtected,
    /// In the album because somebody close to the couple would otherwise not appear.
    IdentityCoverage,
    /// Moved because the sequence's rhythm improved.
    RhythmImproved,
    /// The shot scale of this frame could not be measured, so it does not count toward the rhythm.
    RhythmUnmeasurable,
    /// This chapter has fewer spreads than the allocator wanted, because it ran out of frames.
    ChapterUnderAllocated,
    /// These two frames work together: tonal weight, warmth and direction.
    SpreadPaired,
    /// These two frames are further apart in tonal weight than is comfortable.
    SpreadTonalGap,
    /// Nothing stored could say which way the subjects are facing.
    SpreadFacingUnknown,
    /// A pairing was refused because the two frames are nearly the same photograph.
    FacingNearDuplicateRefused,
    /// This spread carries one image, because the chapter ran one short or nothing paired with it.
    SingleSpread,
    /// A photographer set this order by hand, and a re-run left it alone.
    UserOrdered,
    /// A cloud sequencing move was applied, because the local objective agreed it was an improvement.
    CloudMoveApplied,
    /// A cloud sequencing move was refused.
    CloudMoveRefused,

    // -- social and teaser --------------------------------------------------------------
    /// Reads clearly at thumbnail size.
    ThumbnailLegible,
    /// A safe crop at this aspect ratio exists, from phase 23.
    AspectVariantAvailable,
    /// No safe crop at this aspect ratio exists, so the frame is delivered at its own.
    AspectVariantAbsent,
    /// A slot in the set could not be filled from this gallery.
    SlotUnfilled,
    /// Every content word in this caption came from this wedding's own labels.
    CaptionGrounded,
    /// A drafted caption was refused because it contained something this wedding did not supply.
    CaptionRefused,
}

impl CurateCode {
    /// Every code, in the order this module declares them.
    pub const ALL: [Self; 39] = [
        Self::StrongTonalSeparation,
        Self::ColourDistraction,
        Self::GestureLed,
        Self::HighEmotion,
        Self::GrainTolerant,
        Self::FlatWhenDesaturated,
        Self::ColourIsTheSubject,
        Self::SkinLocusUnavailable,
        Self::SkinSeparationHeld,
        Self::MixBounded,
        Self::TechnicalExcellence,
        Self::EmotionalPeak,
        Self::StrongComposition,
        Self::UniqueFrame,
        Self::StoryImportant,
        Self::ChapterQuotaBinding,
        Self::MomentAlreadyRepresented,
        Self::ScaleQuotaBinding,
        Self::TechnicalVeto,
        Self::UniquenessUnavailable,
        Self::CoverageProtected,
        Self::IdentityCoverage,
        Self::RhythmImproved,
        Self::RhythmUnmeasurable,
        Self::ChapterUnderAllocated,
        Self::SpreadPaired,
        Self::SpreadTonalGap,
        Self::SpreadFacingUnknown,
        Self::FacingNearDuplicateRefused,
        Self::SingleSpread,
        Self::UserOrdered,
        Self::CloudMoveApplied,
        Self::CloudMoveRefused,
        Self::ThumbnailLegible,
        Self::AspectVariantAvailable,
        Self::AspectVariantAbsent,
        Self::SlotUnfilled,
        Self::CaptionGrounded,
        Self::CaptionRefused,
    ];

    /// How many codes there are.
    pub const COUNT: usize = 39;

    /// The stable slug, in the catalog, on the wire and in `docs/reason-codes.md`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StrongTonalSeparation => "strong_tonal_separation",
            Self::ColourDistraction => "colour_distraction",
            Self::GestureLed => "gesture_led",
            Self::HighEmotion => "high_emotion",
            Self::GrainTolerant => "grain_tolerant",
            Self::FlatWhenDesaturated => "flat_when_desaturated",
            Self::ColourIsTheSubject => "colour_is_the_subject",
            Self::SkinLocusUnavailable => "skin_locus_unavailable",
            Self::SkinSeparationHeld => "skin_separation_held",
            Self::MixBounded => "mix_bounded",
            Self::TechnicalExcellence => "technical_excellence",
            Self::EmotionalPeak => "emotional_peak",
            Self::StrongComposition => "strong_composition",
            Self::UniqueFrame => "unique_frame",
            Self::StoryImportant => "story_important",
            Self::ChapterQuotaBinding => "chapter_quota_binding",
            Self::MomentAlreadyRepresented => "moment_already_represented",
            Self::ScaleQuotaBinding => "scale_quota_binding",
            Self::TechnicalVeto => "technical_veto",
            Self::UniquenessUnavailable => "uniqueness_unavailable",
            Self::CoverageProtected => "coverage_protected",
            Self::IdentityCoverage => "identity_coverage",
            Self::RhythmImproved => "rhythm_improved",
            Self::RhythmUnmeasurable => "rhythm_unmeasurable",
            Self::ChapterUnderAllocated => "chapter_under_allocated",
            Self::SpreadPaired => "spread_paired",
            Self::SpreadTonalGap => "spread_tonal_gap",
            Self::SpreadFacingUnknown => "spread_facing_unknown",
            Self::FacingNearDuplicateRefused => "facing_near_duplicate_refused",
            Self::SingleSpread => "single_spread",
            Self::UserOrdered => "user_ordered",
            Self::CloudMoveApplied => "cloud_move_applied",
            Self::CloudMoveRefused => "cloud_move_refused",
            Self::ThumbnailLegible => "thumbnail_legible",
            Self::AspectVariantAvailable => "aspect_variant_available",
            Self::AspectVariantAbsent => "aspect_variant_absent",
            Self::SlotUnfilled => "slot_unfilled",
            Self::CaptionGrounded => "caption_grounded",
            Self::CaptionRefused => "caption_refused",
        }
    }

    /// Parse the stored slug.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5142` when the slug is not one this build knows, which is what a catalog written by
    /// a newer release looks like.
    pub fn parse(slug: &str) -> AuraResult<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|code| code.as_str() == slug)
            .ok_or_else(|| {
                AuraError::new(
                    crate::contract::error::ErrorCode("AURA-ML-5142"),
                    crate::contract::error::Severity::Degraded,
                    crate::contract::error::Recovery::Fallback,
                    format!("unknown curation reason code `{slug}`"),
                    "AURA found a curation note it does not recognise, which usually means this \
                     wedding was curated by a newer version. Re-running curation will refresh it.",
                )
            })
    }

    /// The sentence a photographer reads.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::StrongTonalSeparation => {
                "the light and dark in this frame stay far apart without the colour"
            }
            Self::ColourDistraction => "colour away from the subject is pulling the eye here",
            Self::GestureLed => "this frame is carried by what people are doing",
            Self::HighEmotion => "the moment is strong enough that the colour is not the point",
            Self::GrainTolerant => "the noise in this frame would read as grain",
            Self::FlatWhenDesaturated => "without the colour this frame goes flat",
            Self::ColourIsTheSubject => "the colour is the subject here",
            Self::SkinLocusUnavailable => {
                "AURA has not measured anybody's skin in this wedding yet, so the mix protects \
                 nobody in particular"
            }
            Self::SkinSeparationHeld => "the mix was held back so skin barely moved",
            Self::MixBounded => "the mix wanted to go further and AURA stopped it",
            Self::TechnicalExcellence => "sharp where it matters, and cleanly exposed",
            Self::EmotionalPeak => "the peak of its moment",
            Self::StrongComposition => "the framing is the strongest thing about it",
            Self::UniqueFrame => "unlike anything else already in the set",
            Self::StoryImportant => "this moment matters to the story of the day",
            Self::ChapterQuotaBinding => {
                "stronger frames were passed over because that part of the day already had four"
            }
            Self::MomentAlreadyRepresented => "that shot is already in the set",
            Self::ScaleQuotaBinding => "chosen to keep the set from being all close-ups",
            Self::TechnicalVeto => "not sharp enough for portfolio work",
            Self::UniquenessUnavailable => {
                "AURA could not tell how similar this is to the rest, so it did not count either way"
            }
            Self::CoverageProtected => "in the album because it is the only frame of this moment",
            Self::IdentityCoverage => {
                "in the album because somebody close to the couple would not otherwise appear"
            }
            Self::RhythmImproved => "moved here because the sequence reads better",
            Self::RhythmUnmeasurable => {
                "AURA could not tell how close this frame is, so it does not count toward the rhythm"
            }
            Self::ChapterUnderAllocated => {
                "this part of the day has fewer pages than planned, because there were not enough \
                 frames"
            }
            Self::SpreadPaired => "these two work together across the fold",
            Self::SpreadTonalGap => "one of these is a good deal darker than the other",
            Self::SpreadFacingUnknown => {
                "AURA could not tell which way anybody is facing on this spread"
            }
            Self::FacingNearDuplicateRefused => {
                "AURA would not put two versions of the same shot opposite each other"
            }
            Self::SingleSpread => "this page stands alone",
            Self::UserOrdered => "you set this order, and AURA has left it alone",
            Self::CloudMoveApplied => "a suggested move that made the sequence read better",
            Self::CloudMoveRefused => "a suggested move that would have made the sequence worse",
            Self::ThumbnailLegible => "this reads clearly at thumbnail size",
            Self::AspectVariantAvailable => "a safe crop at this shape exists",
            Self::AspectVariantAbsent => {
                "no safe crop at this shape exists, so it is posted as it was shot"
            }
            Self::SlotUnfilled => "there was nothing in this wedding for this slot",
            Self::CaptionGrounded => "every word here came from this wedding's own labels",
            Self::CaptionRefused => "a suggested caption said something this wedding did not supply",
        }
    }

    /// True when this code says the product **could not check something**.
    ///
    /// Six of the thirty-nine, and they are a different kind of statement from the other
    /// thirty-three. `SpreadTonalGap` says a measurement came out badly; `SpreadFacingUnknown` says
    /// there was no measurement. `FacingNearDuplicateRefused` says a rule fired, which is the
    /// product working; `UniquenessUnavailable` says a rule could not be evaluated, which is the
    /// product admitting a gap.
    ///
    /// It exists because of what happens to a reason list at [`MAX_REASONS`]. A pick with four
    /// strong arguments in its favour and one caveat truncates the caveat away, so the *only* frames
    /// that would show "AURA could not tell how similar this is to the rest" are the frames with
    /// nothing else to say - which is exactly backwards. `aura_curate::explain::rank_reasons`
    /// reserves a slot for the strongest caveat, and this is the predicate it reads.
    ///
    /// Phase 24 wrote the rule this serves - an absent input is ignorance, not permission - and
    /// phase 27 wrote its second half, that clean and skipped are different values. This is the
    /// third: a skip that a stronger reason can hide is a skip nobody sees.
    #[must_use]
    pub const fn is_caveat(self) -> bool {
        matches!(
            self,
            Self::SkinLocusUnavailable
                | Self::UniquenessUnavailable
                | Self::RhythmUnmeasurable
                | Self::SpreadFacingUnknown
                | Self::AspectVariantAbsent
                | Self::SlotUnfilled
        )
    }

    /// Which of the five groups this code belongs to.
    ///
    /// The panel groups by this, and the phase gate asserts that every group has at least one code
    /// a synthetic gallery can actually reach - a vocabulary with unreachable entries is a
    /// vocabulary nobody has checked.
    #[must_use]
    pub const fn group(self) -> CurateGroup {
        match self {
            Self::StrongTonalSeparation
            | Self::ColourDistraction
            | Self::GestureLed
            | Self::HighEmotion
            | Self::GrainTolerant
            | Self::FlatWhenDesaturated
            | Self::ColourIsTheSubject
            | Self::SkinLocusUnavailable
            | Self::SkinSeparationHeld
            | Self::MixBounded => CurateGroup::Bw,
            Self::TechnicalExcellence
            | Self::EmotionalPeak
            | Self::StrongComposition
            | Self::UniqueFrame
            | Self::StoryImportant
            | Self::ChapterQuotaBinding
            | Self::MomentAlreadyRepresented
            | Self::ScaleQuotaBinding
            | Self::TechnicalVeto
            | Self::UniquenessUnavailable => CurateGroup::Hero,
            Self::CoverageProtected
            | Self::IdentityCoverage
            | Self::RhythmImproved
            | Self::RhythmUnmeasurable
            | Self::ChapterUnderAllocated
            | Self::SpreadPaired
            | Self::SpreadTonalGap
            | Self::SpreadFacingUnknown
            | Self::FacingNearDuplicateRefused
            | Self::SingleSpread
            | Self::UserOrdered
            | Self::CloudMoveApplied
            | Self::CloudMoveRefused => CurateGroup::Album,
            Self::ThumbnailLegible
            | Self::AspectVariantAvailable
            | Self::AspectVariantAbsent
            | Self::SlotUnfilled => CurateGroup::Social,
            Self::CaptionGrounded | Self::CaptionRefused => CurateGroup::Caption,
        }
    }
}

impl fmt::Display for CurateCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The five families a curation reason belongs to.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum CurateGroup {
    /// Monochrome suitability and the mix.
    #[default]
    Bw,
    /// The portfolio.
    Hero,
    /// The album sequence and its spreads.
    Album,
    /// The social and teaser sets.
    Social,
    /// Captions and their grounding.
    Caption,
}

impl CurateGroup {
    /// Every group, in the order the panel tabs them.
    pub const ALL: [Self; 5] = [
        Self::Bw,
        Self::Hero,
        Self::Album,
        Self::Social,
        Self::Caption,
    ];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bw => "bw",
            Self::Hero => "hero",
            Self::Album => "album",
            Self::Social => "social",
            Self::Caption => "caption",
        }
    }

    /// The words a photographer reads.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Bw => "Black and white",
            Self::Hero => "Portfolio",
            Self::Album => "Album",
            Self::Social => "Social",
            Self::Caption => "Captions",
        }
    }
}

/// One explanation.
///
/// The same three-field shape phase 12's `CullReason` has, deliberately: a code, the sentence, and
/// how much it moved the decision. `weight` is positive for a reason that argued *for* a pick and
/// negative for one that argued against, and exactly `-1.0` for a veto - because a veto did not move
/// a score, it replaced one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurateReason {
    /// Which reason this is.
    pub code: CurateCode,
    /// The sentence a photographer reads, in the product's voice.
    pub text: String,
    /// How much this moved the decision. Positive argues for, negative against, `-1.0` is a veto.
    pub weight: f32,
}

impl CurateReason {
    /// A reason carrying the code's own sentence.
    #[must_use]
    pub fn plain(code: CurateCode, weight: f32) -> Self {
        Self {
            code,
            text: code.user_text().to_string(),
            weight,
        }
    }

    /// A reason whose sentence names something specific about this frame.
    #[must_use]
    pub fn detailed(code: CurateCode, text: impl Into<String>, weight: f32) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
        }
    }

    /// True when this reason is a veto.
    #[must_use]
    pub fn is_veto(&self) -> bool {
        (self.weight + 1.0).abs() < f32::EPSILON
    }

    /// Order reasons strongest first, vetoes ahead of everything.
    ///
    /// Total and deterministic - `partial_cmp` on the weight would be `None` on a NaN and leave the
    /// order dependent on the sort's implementation, which invariant 4 forbids.
    #[must_use]
    pub fn rank(&self, other: &Self) -> Ordering {
        match (self.is_veto(), other.is_veto()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => other
                .weight
                .abs()
                .total_cmp(&self.weight.abs())
                .then_with(|| self.code.cmp(&other.code)),
        }
    }
}

// ---------------------------------------------------------------------------
// The outline
// ---------------------------------------------------------------------------

/// What a project's curation covered and found.
///
/// The header every curation panel draws, and the shape section 11's four telemetry events are
/// assembled from: `curate.album` reads `spreads`, `rhythm_score`, `pairing_score` and `cloud_used`;
/// `curate.heroes` reads `heroes` and `chapters_covered`; `curate.user_reorder` reads `reorders` and
/// `album_size`; `curate.bw_accepted` reads `bw_offered` and `bw_accepted`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurationOutline {
    /// Photographs in the project.
    pub photos: u32,
    /// Photographs phase 12 selected. **The denominator for everything in this phase.**
    ///
    /// Phase 18's rule rather than phases 09 to 15's: a frame nobody selected is not a gap in
    /// curation, it is a frame nobody asked about. `photos` is on the outline anyway so that a
    /// project whose cull has not run is visibly different from one whose gallery is small.
    pub selected: u32,
    /// Selected photographs curation could read at all.
    pub curated: u32,
    /// Monochrome candidates offered.
    pub bw_offered: u32,
    /// Monochrome candidates a photographer accepted.
    pub bw_accepted: u32,
    /// Monochrome candidates a photographer rejected.
    pub bw_rejected: u32,
    /// Heroes chosen.
    pub heroes: u32,
    /// How many chapters contributed at least one hero.
    pub chapters_covered: u32,
    /// Heroes a photographer accepted.
    pub heroes_accepted: u32,
    /// Spreads in the album.
    pub spreads: u32,
    /// Images in the album.
    pub album_size: u32,
    /// The album's rhythm score, `0..1`.
    pub rhythm_score: f32,
    /// The share of the album whose shot scale could be measured, `0..1`.
    pub rhythm_measurable: f32,
    /// The album's mean pairing score, `0..1`.
    pub pairing_score: f32,
    /// Spreads whose subjects' facing could not be measured.
    pub facing_unknown: u32,
    /// Pairings refused because the two frames were nearly the same photograph.
    pub duplicates_refused: u32,
    /// Must-have rules the album satisfies.
    pub album_covered: u32,
    /// Must-have rules the album misses.
    pub album_missing: u32,
    /// How many times a photographer has reordered the album.
    pub reorders: u32,
    /// Grid slots that could not be filled from this gallery.
    pub slots_unfilled: u32,
    /// Captions stored.
    pub captions: u32,
    /// Captions a model drafted and the grounding check refused.
    pub captions_refused: u32,
    /// Whether the cloud sequencing task was reached at all.
    pub cloud_used: bool,
    /// Cloud moves the local objective agreed with.
    pub cloud_moves_applied: u32,
    /// Cloud moves the local objective refused.
    pub cloud_moves_refused: u32,
    /// Bytes migration 29 occupies for this project.
    pub bytes: u64,
    /// Which `curation.toml` produced this.
    pub policy_ver: u16,
    /// Which build's arithmetic produced this.
    pub analysis_ver: u16,
    /// Which phase 05 embedding the uniqueness and pairing terms were measured against.
    pub embed_ver: u16,
    /// Whether either of this phase's two heads is trained.
    ///
    /// False in this build. A panel that did not show it would be presenting a deterministic
    /// solver's answer as a learned one, which is the failure phases 15, 16, 18 and 22 each guarded
    /// against in their own way.
    pub heads_trained: bool,
}

impl CurationOutline {
    /// What share of the selected gallery curation could read, `0..1`.
    ///
    /// The denominator is the **gallery**, not the project. ADR-0059 section 13.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn coverage(&self) -> f32 {
        if self.selected == 0 {
            return 0.0;
        }
        self.curated as f32 / self.selected as f32
    }

    /// What share of the offered monochrome candidates a photographer took, `0..1`.
    ///
    /// Section 10.1's third gate. `None` until somebody has answered at least one, because a rate
    /// over zero decisions is not a low acceptance rate - it is no evidence at all.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn bw_acceptance(&self) -> Option<f32> {
        let answered = self.bw_accepted + self.bw_rejected;
        (answered > 0).then(|| self.bw_accepted as f32 / answered as f32)
    }

    /// True when the rhythm score is measured over enough of the album to mean anything.
    ///
    /// A third of the album, which is the point below which a panel renders the score in grey and
    /// the phase gate refuses to report it as a result.
    #[must_use]
    pub fn rhythm_is_meaningful(&self) -> bool {
        self.rhythm_measurable >= 0.33
    }
}

// ---------------------------------------------------------------------------
// Overrides
// ---------------------------------------------------------------------------

/// What kind of pick a photographer is deciding about.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum PickKind {
    /// A portfolio hero.
    #[default]
    Hero,
    /// A monochrome suggestion.
    Bw,
    /// A frame in the grid set.
    SocialGrid,
    /// A frame in the story set.
    SocialStory,
    /// The single social hero.
    SocialHero,
    /// A frame in the teaser.
    Teaser,
}

impl PickKind {
    /// Every kind.
    pub const ALL: [Self; 6] = [
        Self::Hero,
        Self::Bw,
        Self::SocialGrid,
        Self::SocialStory,
        Self::SocialHero,
        Self::Teaser,
    ];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hero => "hero",
            Self::Bw => "bw",
            Self::SocialGrid => "social_grid",
            Self::SocialStory => "social_story",
            Self::SocialHero => "social_hero",
            Self::Teaser => "teaser",
        }
    }

    /// Parse the stored text.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5143` when the text names no kind this build has.
    pub fn parse(text: &str) -> AuraResult<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == text)
            .ok_or_else(|| {
                AuraError::new(
                    crate::contract::error::ErrorCode("AURA-ML-5143"),
                    crate::contract::error::Severity::ItemFailed,
                    crate::contract::error::Recovery::AskUser,
                    format!("unknown curation pick kind `{text}`"),
                    "AURA could not record that choice. Nothing about the photograph has changed; \
                     reopen the panel and try again.",
                )
            })
    }
}

/// A photographer disagreeing with one pick.
///
/// Two fields, and there is deliberately no third. There is no strength, no threshold and no way to
/// ask for a different mix: a photographer who wants a different monochrome conversion has the tone
/// panel phase 16 shipped, and a surface that let one be requested here would make the mix a preset
/// scaled by a number. ADR-0060 section 6.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurateOverride {
    /// Which kind of pick.
    pub kind: PickKind,
    /// Yes or no. There is no third state: clearing a decision is deleting the row.
    pub accepted: bool,
    /// Why, in the photographer's own words. At most [`MAX_NOTE`] bytes.
    pub note: Option<String>,
}

impl CurateOverride {
    /// True when the note is inside [`MAX_NOTE`].
    #[must_use]
    pub fn within_bounds(&self) -> bool {
        self.note.as_ref().is_none_or(|n| n.len() <= MAX_NOTE)
    }
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Which specification `CurateService::export` produces.
///
/// Section 2.1 asks for "export to album software formats (JSON/CSV/PSD-ready layer lists)". Three
/// formats and no fourth: this produces a **specification**, never a file of pixels. Section 2.2
/// puts album page rendering out of scope and phase 30 owns delivery.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    /// The whole plan, machine-readable. What another tool imports.
    #[default]
    Json,
    /// One row per image: spread, page, chapter. What a spreadsheet opens.
    Csv,
    /// One line per layer, in stacking order. What a PSD template script reads.
    LayerList,
}

impl ExportFormat {
    /// Every format.
    pub const ALL: [Self; 3] = [Self::Json, Self::Csv, Self::LayerList];

    /// The stored text and the wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
            Self::LayerList => "layer_list",
        }
    }

    /// Parse the wire value.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5143` when the text names no format this build has.
    pub fn parse(text: &str) -> AuraResult<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|f| f.as_str() == text)
            .ok_or_else(|| {
                AuraError::new(
                    crate::contract::error::ErrorCode("AURA-ML-5143"),
                    crate::contract::error::Severity::ItemFailed,
                    crate::contract::error::Recovery::AskUser,
                    format!("unknown curation export format `{text}`"),
                    "AURA does not have that export format. Choose JSON, CSV or a layer list.",
                )
            })
    }

    /// The file extension a shell should offer.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
            Self::LayerList => "txt",
        }
    }
}

/// Which set is being exported.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ExportSubject {
    /// The album plan.
    #[default]
    Album,
    /// The social sets and their captions.
    Social,
    /// The portfolio.
    Heroes,
    /// The wedding-night teaser.
    Teaser,
}

impl ExportSubject {
    /// Every subject.
    pub const ALL: [Self; 4] = [Self::Album, Self::Social, Self::Heroes, Self::Teaser];

    /// The wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Album => "album",
            Self::Social => "social",
            Self::Heroes => "heroes",
            Self::Teaser => "teaser",
        }
    }

    /// Parse the wire value.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5143` when the text names no subject this build has.
    pub fn parse(text: &str) -> AuraResult<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|s| s.as_str() == text)
            .ok_or_else(|| {
                AuraError::new(
                    crate::contract::error::ErrorCode("AURA-ML-5143"),
                    crate::contract::error::Severity::ItemFailed,
                    crate::contract::error::Recovery::AskUser,
                    format!("unknown curation export subject `{text}`"),
                    "AURA does not have that to export. Choose the album, the social sets, the \
                     portfolio or the teaser.",
                )
            })
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask what a finished gallery becomes.
///
/// Twenty-fifth service of its kind and the first whose subject is a **deliverable**. Phase 30
/// exports these plans, posts these sets and reads [`CurateService::set_order`]'s rows as its
/// learning signal. No phase may keep its own hero ranker, its own album sequencer or its own idea
/// of what suits monochrome - two answers to "what is the album" is a book that does not match the
/// proof that does not match the post.
///
/// # What is not on this trait
///
/// There is no `apply`, no `deliver` and no `render`. Nothing in phase 29 changes a photograph, and
/// `crates/aura-curate/tests/no_outputs.rs` fails the build if the implementing crate grows the
/// means to. [`CurateService::export`] produces a **specification** as text; the shell saves it.
/// ADR-0059 section 3 and ADR-0060 section 5.
pub trait CurateService: Send + Sync + fmt::Debug {
    /// What a project's curation covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<CurationOutline>;

    /// Everything the last pass produced, or `None` when the project has not been curated.
    ///
    /// `None` is not an empty result. A project nobody has curated and a project whose gallery had
    /// nothing worth putting in an album are different answers, and a caller that rendered them the
    /// same would show a photographer an empty album for a wedding the product never looked at.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn result(&self, project: ProjectId) -> AuraResult<Option<CurationResult>>;

    /// The monochrome candidates, best first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn bw(&self, project: ProjectId) -> AuraResult<Vec<BwPick>>;

    /// The portfolio, best first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn heroes(&self, project: ProjectId) -> AuraResult<Vec<HeroPick>>;

    /// The album draft, or `None` when the project has not been curated.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn album(&self, project: ProjectId) -> AuraResult<Option<AlbumPlan>>;

    /// One spread, or `None` when it is unknown.
    ///
    /// A spread rather than the album, because the spread view is the screen a photographer spends
    /// the most time on and fetching 120 spreads to draw two frames is the shape ADR-0060 section 2
    /// rejects.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the spread cannot be read.
    fn spread(&self, spread: SpreadId) -> AuraResult<Option<Spread>>;

    /// The three sets a photographer posts from.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn social(&self, project: ProjectId) -> AuraResult<SocialSets>;

    /// The wedding-night teaser, best first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn teaser(&self, project: ProjectId) -> AuraResult<Vec<TeaserPick>>;

    /// Record a photographer's album order.
    ///
    /// The whole order rather than a move, and it is refused rather than repaired when it reorders
    /// chapters, names an image outside the gallery, or repeats one. ADR-0060 section 4.
    ///
    /// A stored order is never overwritten by a re-run: the next pass reports what it would have
    /// done and leaves the album alone. The operating manual's fifth code rule, and the row phase
    /// 30's learning loop reads.
    ///
    /// **This records the order; it does not re-lay-out the spreads.** Which two images share a
    /// spread is still AURA's decision - a photographer chose a *sequence*, and the near-duplicate
    /// refusal and the tonal ceiling still apply to whatever ends up adjacent - so the spreads are
    /// re-formed by the next curation pass, which has the readings that decision needs. The IPC
    /// command runs both in one go, which is what makes a drag look instant. ADR-0060 section 4.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5143` when the order is refused. `AURA-DB-3006` when it cannot be written.
    fn set_order(&self, project: ProjectId, order: &[ImageId]) -> Result<(), AuraError>;

    /// Record what a photographer decided about one pick.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5143` when the pick is unknown or the note is too long. `AURA-DB-3006` when it
    /// cannot be written.
    fn decide(
        &self,
        project: ProjectId,
        image: ImageId,
        decision: CurateOverride,
    ) -> Result<(), AuraError>;

    /// One set as a specification another tool can read.
    ///
    /// Text, never a file. Nothing in this phase opens one.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    fn export(
        &self,
        project: ProjectId,
        subject: ExportSubject,
        format: ExportFormat,
    ) -> AuraResult<String>;
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

/// The album as a short human-readable summary.
///
/// Rendered rather than stored, which is phase 27's rule at its conclusion: there is no `summary`
/// column in migration 29 and no free-text field automation can write into, so a stored sentence
/// cannot become copy a release has to maintain or a place a cloud answer gets quoted back as a
/// measurement.
#[must_use]
pub fn album_summary(plan: &AlbumPlan) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} images across {} spreads (asked for {}).",
        plan.size,
        plan.spreads.len(),
        plan.target_size
    );
    if plan.user_ordered {
        let _ = writeln!(out, "You set this order; AURA has left it alone.");
    }
    if plan.rhythm_measurable >= 0.33 {
        let _ = writeln!(
            out,
            "Rhythm {:.2} over {:.0}% of the album; pairing {:.2}.",
            plan.rhythm_score,
            plan.rhythm_measurable * 100.0,
            plan.pairing_score
        );
    } else {
        let _ = writeln!(
            out,
            "Rhythm could only be measured on {:.0}% of the album, which is too little to report; \
             pairing {:.2}.",
            plan.rhythm_measurable * 100.0,
            plan.pairing_score
        );
    }
    let missing = plan.coverage.missing();
    if missing.is_empty() {
        let _ = writeln!(out, "Every moment the gallery covers is in the album.");
    } else {
        let names: Vec<&str> = missing.iter().map(|m| MustHave::title(*m)).collect();
        let _ = writeln!(out, "Not in the album: {}.", names.join(", "));
    }
    let short = plan.under_allocated();
    if !short.is_empty() {
        let names: Vec<&str> = short.iter().map(|c| c.as_str()).collect();
        let _ = writeln!(
            out,
            "These parts of the day have fewer pages than planned: {}.",
            names.join(", ")
        );
    }
    out
}

/// Which identities the album must carry at least `minimum` frames of.
///
/// A helper rather than a policy: the number comes from `coverage_rules.toml`, which is phase 12's
/// file and stays phase 12's file. There is no second close-family rule in this product.
#[must_use]
pub fn under_covered(report: &CoverageReport, minimum: u32) -> Vec<IdentityId> {
    report
        .identity_coverage
        .iter()
        .filter(|(_, count)| *count < minimum)
        .map(|(id, _)| *id)
        .collect()
}

/// Which scenes a shot scale can be read straight off.
///
/// Section 6.3's rhythm needs a scale per frame and phase 06's faces supply most of them. These are
/// the scenes whose scale is known *by definition* - a ring macro is tight whether or not anybody's
/// face is in it, and a venue establishing shot is wide because that is what the label means.
///
/// Deliberately short. Every scene not in this list falls back to the face measurement and then to
/// [`ShotScale::Unknown`], because guessing that a ceremony is "medium" would put a number in the
/// rhythm score that came from a label rather than from a photograph.
#[must_use]
pub const fn scale_of_scene(scene: SceneId) -> ShotScale {
    match scene {
        SceneId::Details => ShotScale::Tight,
        SceneId::Venue => ShotScale::Wide,
        SceneId::CeremonyEntrance | SceneId::ReceptionEntrance | SceneId::DanceFloor => {
            ShotScale::Wide
        }
        _ => ShotScale::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::assertions_on_constants,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::disallowed_methods,
    clippy::panic
)]
mod tests {
    use super::*;

    fn spread(index: u32, chapter: ChapterId) -> Spread {
        Spread {
            id: SpreadId::new(),
            index,
            left: Some(ImageId::new()),
            right: Some(ImageId::new()),
            single: false,
            chapter,
            pair: SpreadPair::none(),
            reasons: Vec::new(),
        }
    }

    fn plan(chapters: &[(ChapterId, u32)]) -> AlbumPlan {
        let mut spreads = Vec::new();
        let mut map = Vec::new();
        let mut next = 0u32;
        for (chapter, len) in chapters {
            map.push(ChapterSpan {
                chapter: *chapter,
                first: next,
                len: *len,
                target: *len,
            });
            for _ in 0..*len {
                spreads.push(spread(next, *chapter));
                next += 1;
            }
        }
        let mut out = AlbumPlan::empty(ALBUM_DEFAULT);
        out.size = spreads.len() as u32 * IMAGES_PER_SPREAD;
        out.spreads = spreads;
        out.chapter_map = map;
        out
    }

    #[test]
    fn every_reason_code_has_a_slug_a_sentence_and_a_group() {
        let mut slugs = std::collections::BTreeSet::new();
        for code in CurateCode::ALL {
            assert!(slugs.insert(code.as_str()), "duplicate slug {code}");
            assert!(!code.user_text().is_empty(), "{code} has no sentence");
            assert!(CurateGroup::ALL.contains(&code.group()));
            assert_eq!(CurateCode::parse(code.as_str()).unwrap(), code);
        }
        assert_eq!(CurateCode::ALL.len(), CurateCode::COUNT);
    }

    #[test]
    fn a_caveat_says_nothing_was_checked_and_a_refusal_says_a_rule_fired() {
        // The distinction the reason list's reserved slot depends on.
        assert!(CurateCode::SpreadFacingUnknown.is_caveat());
        assert!(CurateCode::UniquenessUnavailable.is_caveat());
        assert!(CurateCode::SkinLocusUnavailable.is_caveat());
        // A rule that fired is the product working, not a gap in it.
        assert!(!CurateCode::FacingNearDuplicateRefused.is_caveat());
        assert!(!CurateCode::SpreadTonalGap.is_caveat());
        assert!(!CurateCode::CaptionRefused.is_caveat());
        assert!(!CurateCode::TechnicalVeto.is_caveat());

        let caveats = CurateCode::ALL.iter().filter(|c| c.is_caveat()).count();
        assert_eq!(caveats, 6);
    }

    #[test]
    fn every_group_has_at_least_one_code() {
        for group in CurateGroup::ALL {
            assert!(
                CurateCode::ALL.iter().any(|c| c.group() == group),
                "{group:?} has no codes, so nothing could ever explain itself under that tab"
            );
        }
    }

    #[test]
    fn an_unknown_reason_slug_is_a_degraded_error_rather_than_a_default() {
        // A vocabulary that silently defaulted would make a catalog written by a newer release
        // look like a catalog full of `StrongTonalSeparation`.
        let err = CurateCode::parse("something_a_later_build_wrote").unwrap_err();
        assert_eq!(err.code.0, "AURA-ML-5142");
    }

    #[test]
    fn the_grid_quota_sums_to_the_grid_size() {
        let total: u32 = SocialSlot::GRID_QUOTA.iter().map(|(_, n)| n).sum();
        assert_eq!(total, GRID_SIZE);
        for (slot, want) in SocialSlot::GRID_QUOTA {
            assert_eq!(slot.grid_quota(), want);
        }
    }

    #[test]
    fn the_skin_bound_is_far_tighter_than_the_general_one() {
        // The whole of ADR-0059 section 5 in one assertion: a mix may move the bands somebody's
        // skin competes with much further than it may move the skin.
        assert!(MAX_SKIN_BAND_SHIFT * 3 < MAX_BAND_SHIFT);
    }

    #[test]
    fn a_mix_that_moves_a_skin_band_too_far_is_refused_and_one_that_moves_another_band_is_not() {
        let mut mix = BwMix::neutral();
        mix.bands[3] = MAX_BAND_SHIFT; // green: a hedge, not a person
        assert!(mix.within_bounds());
        assert!(mix.within_skin_bound(&[1])); // orange is where this person was measured

        let mut skin_heavy = BwMix::neutral();
        skin_heavy.bands[1] = MAX_SKIN_BAND_SHIFT + 1;
        assert!(skin_heavy.within_bounds());
        assert!(!skin_heavy.within_skin_bound(&[1]));
    }

    #[test]
    fn an_empty_skin_band_set_is_vacuously_inside_the_bound() {
        // Honest rather than convenient: nobody measured, so there is no skin bound to check, and
        // `CurateCode::SkinLocusUnavailable` is what a caller emits instead. A predicate that
        // returned false here would make "we did not measure" indistinguishable from "we broke it".
        let mut mix = BwMix::neutral();
        mix.bands[1] = MAX_BAND_SHIFT;
        assert!(mix.within_skin_bound(&[]));
    }

    #[test]
    fn the_mix_maps_onto_the_recipes_own_band_names() {
        // Not a ninth vocabulary. The keys have to be exactly `aura_recipe::HSL_BANDS`.
        let mix = BwMix::neutral();
        let pairs = mix.to_recipe_mix();
        assert_eq!(pairs.len(), 8);
        for (ix, (name, _)) in pairs.iter().enumerate() {
            assert_eq!(*name, MIX_BANDS[ix]);
        }
    }

    #[test]
    fn chapters_in_wedding_order_are_accepted_and_chapters_out_of_order_are_not() {
        let good = plan(&[(ChapterId::Ceremony, 2), (ChapterId::Reception, 3)]);
        assert!(good.chapters_are_ordered());

        let bad = plan(&[(ChapterId::Reception, 3), (ChapterId::Ceremony, 2)]);
        assert!(
            !bad.chapters_are_ordered(),
            "an album whose ceremony follows its reception is not an album with an unusual sequence"
        );
    }

    #[test]
    fn a_chapter_that_appears_twice_is_refused() {
        let twice = plan(&[(ChapterId::Ceremony, 1), (ChapterId::Ceremony, 1)]);
        assert!(!twice.chapters_are_ordered());
    }

    #[test]
    fn a_span_that_does_not_cover_the_spreads_is_refused() {
        let mut short = plan(&[(ChapterId::Ceremony, 2)]);
        short.spreads.push(spread(2, ChapterId::Ceremony));
        assert!(!short.chapters_are_ordered());
    }

    #[test]
    fn a_pair_is_refused_on_similarity_and_on_tonal_gap_but_not_on_warmth() {
        let base = SpreadPair {
            tonal_gap: 0.1,
            warmth_gap_k: 100.0,
            facing_score: 0.5,
            facing_known: true,
            similarity: 0.5,
            score: 0.7,
        };
        assert!(base.is_permitted());

        let twins = SpreadPair {
            similarity: MAX_PAIR_SIMILARITY + 0.01,
            ..base
        };
        assert!(!twins.is_permitted());

        let split = SpreadPair {
            tonal_gap: MAX_PAIR_TONAL_GAP + 0.01,
            ..base
        };
        assert!(!split.is_permitted());

        // Warmth is a *term*: 900 K apart is a worse pair, not a refused one.
        let warm = SpreadPair {
            warmth_gap_k: MAX_PAIR_WARMTH_GAP_K + 100.0,
            ..base
        };
        assert!(warm.is_permitted());
    }

    #[test]
    fn an_unmeasured_facing_is_not_a_facing_of_zero() {
        // Two spreads with the same `facing_score`, meaning completely different things.
        let unknown = SpreadPair::none();
        assert!(!unknown.facing_known);
        assert_eq!(unknown.facing_score, 0.0);

        let measured = SpreadPair {
            facing_known: true,
            ..SpreadPair::none()
        };
        assert!(measured.facing_known);
        assert_eq!(measured.facing_score, 0.0);
        assert_ne!(unknown.facing_known, measured.facing_known);
    }

    #[test]
    fn an_empty_spread_is_not_well_formed_and_a_single_carries_one_image() {
        let mut s = spread(0, ChapterId::Ceremony);
        s.left = None;
        s.right = None;
        assert!(!s.is_well_formed());

        let mut single = spread(0, ChapterId::Ceremony);
        single.single = true;
        assert!(!single.is_well_formed(), "a single carrying two is a bug");
        single.right = None;
        assert!(single.is_well_formed());
    }

    #[test]
    fn a_hero_below_the_technical_floor_is_not_well_formed() {
        let mut pick = HeroPick {
            image_id: ImageId::new(),
            rank: 0,
            score: 0.8,
            terms: HeroTerms {
                technical: HERO_TECHNICAL_FLOOR,
                emotion: 0.9,
                composition: 0.7,
                uniqueness: 0.6,
                story: 0.5,
            },
            chapter: ChapterId::Ceremony,
            moment: None,
            scale: ShotScale::Tight,
            binding: HeroBinding::Unconstrained,
            reasons: vec![CurateReason::plain(CurateCode::EmotionalPeak, 0.9)],
            confidence: 0.7,
            accepted: None,
        };
        assert!(pick.is_well_formed());
        pick.terms.technical = HERO_TECHNICAL_FLOOR - 0.01;
        assert!(!pick.is_well_formed(), "the veto is part of the shape");
    }

    #[test]
    fn a_pick_with_no_reason_is_not_well_formed() {
        // Invariant 2, as a predicate rather than a convention.
        let pick = BwPick {
            image_id: ImageId::new(),
            mix: BwMix::neutral(),
            score: 0.7,
            terms: BwTerms {
                tonal_separation: 0.8,
                colour_distraction: 0.2,
                gesture: 0.5,
                emotion: 0.6,
                grain: 0.4,
            },
            skin_bands: Vec::new(),
            reasons: Vec::new(),
            confidence: 0.6,
            accepted: None,
        };
        assert!(!pick.is_well_formed());
    }

    #[test]
    fn reasons_rank_vetoes_first_and_then_by_magnitude() {
        let mut reasons = [
            CurateReason::plain(CurateCode::StrongComposition, 0.3),
            CurateReason::plain(CurateCode::TechnicalVeto, -1.0),
            CurateReason::plain(CurateCode::EmotionalPeak, 0.9),
        ];
        reasons.sort_by(CurateReason::rank);
        assert_eq!(reasons[0].code, CurateCode::TechnicalVeto);
        assert_eq!(reasons[1].code, CurateCode::EmotionalPeak);
        assert_eq!(reasons[2].code, CurateCode::StrongComposition);
    }

    #[test]
    fn the_outline_denominator_is_the_gallery_and_an_unanswered_rate_is_none() {
        let outline = CurationOutline {
            photos: 3_000,
            selected: 800,
            curated: 800,
            ..CurationOutline::default()
        };
        assert_eq!(outline.coverage(), 1.0, "the denominator is the gallery");

        assert!(
            outline.bw_acceptance().is_none(),
            "a rate over zero decisions is no evidence, not a low rate"
        );

        let answered = CurationOutline {
            bw_accepted: 7,
            bw_rejected: 3,
            ..outline
        };
        assert_eq!(answered.bw_acceptance(), Some(0.7));
    }

    #[test]
    fn a_rhythm_measured_over_almost_nothing_is_not_meaningful() {
        let thin = CurationOutline {
            rhythm_score: 1.0,
            rhythm_measurable: 0.08,
            ..CurationOutline::default()
        };
        assert!(!thin.rhythm_is_meaningful());

        let fat = CurationOutline {
            rhythm_measurable: 0.7,
            ..thin
        };
        assert!(fat.rhythm_is_meaningful());
    }

    #[test]
    fn unfilled_grid_slots_are_reported_rather_than_substituted() {
        let sets = SocialSets {
            grid: vec![SocialPick {
                image_id: ImageId::new(),
                aspect: AspectVariant::Square,
                slot: SocialSlot::Hero,
                legibility: 0.9,
                reasons: Vec::new(),
                accepted: None,
            }],
            ..SocialSets::default()
        };
        let unfilled = sets.unfilled_slots();
        assert!(!unfilled.iter().any(|(slot, _)| *slot == SocialSlot::Hero));
        let portraits = unfilled
            .iter()
            .find(|(slot, _)| *slot == SocialSlot::Portrait)
            .unwrap();
        assert_eq!(portraits.1, 2);
    }

    #[test]
    fn a_caption_over_either_bound_is_out_of_bounds() {
        let mut caption = Caption {
            image_id: None,
            chapter: ChapterId::Ceremony,
            text: "The ceremony, and the vows".into(),
            source: CaptionSource::Template,
            grounded: true,
        };
        assert!(caption.within_bounds());

        caption.text =
            "one two three four five six seven eight nine ten eleven twelve thirteen".to_string();
        assert!(!caption.within_bounds(), "thirteen words is over the bound");

        caption.text = "x".repeat(CAPTION_MAX_CHARS + 1);
        assert!(!caption.within_bounds());

        caption.text = "   ".into();
        assert!(!caption.within_bounds(), "a blank caption is not a caption");
    }

    #[test]
    fn only_scenes_whose_scale_is_known_by_definition_report_one() {
        assert_eq!(scale_of_scene(SceneId::Details), ShotScale::Tight);
        assert_eq!(scale_of_scene(SceneId::Venue), ShotScale::Wide);
        // A ceremony is not "medium": guessing would put a number in the rhythm score that came
        // from a label rather than from a photograph.
        assert_eq!(scale_of_scene(SceneId::Ceremony), ShotScale::Unknown);
        assert!(!ShotScale::Unknown.is_known());
    }

    #[test]
    fn an_override_can_carry_no_strength_and_a_long_note_is_refused() {
        let ok = CurateOverride {
            kind: PickKind::Hero,
            accepted: false,
            note: Some("not my style".into()),
        };
        assert!(ok.within_bounds());
        let long = CurateOverride {
            note: Some("x".repeat(MAX_NOTE + 1)),
            ..ok
        };
        assert!(!long.within_bounds());
    }

    #[test]
    fn every_pick_kind_and_export_shape_round_trips_through_its_slug() {
        for kind in PickKind::ALL {
            assert_eq!(PickKind::parse(kind.as_str()).unwrap(), kind);
        }
        for format in ExportFormat::ALL {
            assert_eq!(ExportFormat::parse(format.as_str()).unwrap(), format);
            assert!(!format.extension().is_empty());
        }
        for subject in ExportSubject::ALL {
            assert_eq!(ExportSubject::parse(subject.as_str()).unwrap(), subject);
        }
        for scale in ShotScale::ALL {
            assert_eq!(ShotScale::parse(scale.as_str()), scale);
        }
        for binding in HeroBinding::ALL {
            assert_eq!(HeroBinding::parse(binding.as_str()), binding);
        }
        for source in [CaptionSource::Template, CaptionSource::Cloud] {
            assert_eq!(CaptionSource::parse(source.as_str()), source);
        }
    }

    #[test]
    fn the_album_summary_says_when_the_rhythm_could_not_be_measured() {
        let mut plan = AlbumPlan::empty(80);
        plan.rhythm_measurable = 0.08;
        plan.rhythm_score = 1.0;
        let text = album_summary(&plan);
        assert!(
            text.contains("too little to report"),
            "a rhythm of 1.000 over 8 % of an album must not be reported as a result: {text}"
        );
    }

    #[test]
    fn the_album_size_bounds_bracket_the_default() {
        assert!(ALBUM_MIN < ALBUM_DEFAULT && ALBUM_DEFAULT < ALBUM_MAX);
        assert!(TEASER_MIN < TEASER_MAX);
        assert!(HERO_TARGET > MAX_HEROES_PER_CHAPTER);
    }
}
