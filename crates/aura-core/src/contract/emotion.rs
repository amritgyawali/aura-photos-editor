//! FROZEN CONTRACT. What a photograph is *worth*: the expression on each face, what
//! the people in it are doing to one another, where the action peaked, and which
//! frames were reacting to which.
//!
//! PHASE-10 section 5 freezes these shapes before any expression head exists. The file
//! is in `aura-core` rather than in `aura-brain-wedding` for the reason
//! [`crate::contract::scene`], [`crate::contract::moment`] and
//! [`crate::contract::integrity`] are: the phases that consume an emotion score are 12
//! (culling and coverage), 13 (the explainability ledger), 27 (the QC agent) and 29
//! (curation), and none of them needs the ranker, the peak kernel or the gaze geometry.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## The one sentence to read before anything else
//!
//! **This phase scores photographic expression. It makes no claim about anybody's
//! inner state.** Section 2.2 puts "any claim about a person's inner emotional state"
//! out of scope permanently, and section 12's second failure mode is what happens when
//! a product forgets. [`FaceExpression::tears`] is *how much this face reads as crying
//! in a photograph*, and it is not a statement that somebody was sad. Every field on
//! [`FaceExpression`] is named for what is visible, every reason in [`EmotionCode`] is
//! written in the language of a photo editor rather than of a psychologist, and
//! `docs/emotion-and-moments.md` says so to the photographer in as many words.
//!
//! The second boundary is the one every phase since 05 has had to keep: **a score is
//! evidence; the deciding phase owns the cull.** Nothing in this contract can reject,
//! deliver, rank into a gallery or delete a frame. [`EmotionService::ranked`] returns
//! an ordering because "ranks every frame by emotional value" is this phase's headline
//! feature, and an ordering is not a selection: phase 12 chooses, and it chooses with
//! [`ImageEmotion::emotion_score`] beside `IntegrityResult::technical_score` and a
//! coverage guarantee neither of them knows about.
//!
//! ## Eight spellings differ from the phase document
//!
//! All eight are recorded in
//! `docs/adr/ADR-0021-emotion-taxonomy-and-moment-ranking.md` section 2.
//!
//! * **`ImageId` is `PhotoId`**, aliased rather than duplicated, exactly as the scene,
//!   people, moment and integrity contracts already do it.
//! * **[`ImageEmotion`] carries `confidence`.** Section 5's shape has `reasons` and no
//!   confidence, and invariant 2 requires both of every AI decision. A ranking a
//!   photographer will disagree with has to be able to say how sure it is.
//! * **[`ImageEmotion`] carries its faces.** Section 5 keeps [`FaceExpression`] in a
//!   separate shape with no route from one to the other. Section 9's `SFE` deliverable
//!   is "Emotion card with face crops, interaction chips, peak indicator", which is one
//!   read in the interface and would otherwise be two round trips per thumbnail.
//! * **[`Reason`] is [`EmotionReason`] with its own [`EmotionCode`].** Phase 09 froze a
//!   `Reason` whose codes are a closed set of *technical defects*; the vocabulary this
//!   phase needs shares none of them. Section 9 gives DOC "explain emotion scoring
//!   honestly in user docs", which is only a finishable job against a closed list, so
//!   the list is closed here and `docs/emotion-and-moments.md` is written against
//!   [`EmotionCode::ALL`]. The rectangle type is phase 09's [`CropRect`], reused rather
//!   than redeclared.
//! * **Gaze is measured, not predicted.** [`GazeTarget`] has an [`GazeTarget::Unknown`]
//!   variant section 2.1 does not name, because the geometry it is derived from - phase
//!   06's two eye landmarks and the face box - is absent on a face the detector found
//!   without landmarks, and a frame whose gaze nobody could measure must not read as
//!   "looking away".
//! * **[`MomentPeak`] and [`ReactionLink`] are types.** Section 5 leaves the peak as two
//!   fields on [`ImageEmotion`] and the reaction as one `Option<ImageId>`. Both survive
//!   there - [`ImageEmotion::peak_proximity`] and [`ImageEmotion::reaction_of`] are
//!   section 5's, unchanged - but the moment-level answer needs a margin, a kind and its
//!   own reasons, and a link needs its bonus and the gap it crossed.
//! * **[`EmotionOutline`] is not in section 5 at all.** It is the coverage carrier,
//!   inherited from phase 05 for the sixth time, with this phase's refinement written at
//!   [`EmotionOutline::face_aware`].
//! * **[`EmotionService`] is not in section 5 either.** Four later phases consume
//!   emotion scores and a contract with no entry point makes each of them find its own
//!   way in.
//!
//! ## Three version fields, because they invalidate three different things
//!
//! [`ImageEmotion::model_ver`] invalidates every expression value and every interaction
//! strength, because both come from a learned head. [`ImageEmotion::analysis_ver`]
//! invalidates the gaze, the peak curve, the reaction links and the feature vector,
//! because those come from this build's arithmetic.
//! [`ImageEmotion::weights_ver`] invalidates [`ImageEmotion::emotion_score`] alone,
//! because the scene and tradition weighting, the ranker's coefficients and the
//! per-scene calibration all live in one versioned file.
//!
//! There is deliberately **no fourth column for the ranker**. Its coefficients ship
//! inside `emotion_weights.toml` beside the weights, so one number invalidates the
//! score and two numbers that invalidate the same thing never exist. Phase 09's third
//! inherited rule, applied in the direction that removes a column.
//!
//! Comparing two rows across any of the three returns a plausible number that means
//! nothing. `AURA-ML-5038` exists so that comparison never happens silently. Sixth
//! phase, sixth version-drift code.
//!
//! Changing anything in this file requires an ADR.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{FaceId, IdentityId, MomentId, ProjectId};
use crate::contract::integrity::CropRect;
use crate::contract::scene::{SceneId, Source};

pub use crate::contract::scene::{ImageId, Timestamp};

// ---------------------------------------------------------------------------
// Gaze
// ---------------------------------------------------------------------------

/// Where a face is looking.
///
/// PHASE-10 section 2.1's four targets plus [`GazeTarget::Unknown`]; see this module's
/// header for why the fifth exists.
///
/// **A direction, not an intention.** `Partner` means "towards the other member of the
/// pair phase 06 identified", measured from where that person's face is in this frame.
/// It does not mean affection, and nothing in this product may read it that way.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum GazeTarget {
    /// No usable landmarks, so nothing was measured. The default, because it is the
    /// answer that claims the least.
    #[default]
    Unknown,
    /// Towards the lens.
    Camera,
    /// Towards the other half of the couple, who is in this frame.
    Partner,
    /// Towards the officiant, or towards whoever is speaking.
    Officiant,
    /// Somewhere else in the frame, or out of it.
    Away,
}

impl GazeTarget {
    /// Every target, in the order the panel lists them.
    pub const ALL: [Self; 5] = [
        Self::Unknown,
        Self::Camera,
        Self::Partner,
        Self::Officiant,
        Self::Away,
    ];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Camera => "camera",
            Self::Partner => "partner",
            Self::Officiant => "officiant",
            Self::Away => "away",
        }
    }

    /// Parse the stored text. Unknown values read as [`GazeTarget::Unknown`], which is
    /// the direction that invents nothing.
    #[must_use]
    pub fn from_str_or_unknown(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|target| target.as_str() == text)
            .unwrap_or(Self::Unknown)
    }

    /// True when this face is looking at something in the room rather than at the lens.
    ///
    /// The predicate the reaction linker asks: somebody watching the kiss is not
    /// watching the photographer. [`GazeTarget::Unknown`] is false, because a face whose
    /// gaze nobody measured is not evidence of anything.
    #[must_use]
    pub const fn is_engaged(self) -> bool {
        matches!(self, Self::Partner | Self::Officiant | Self::Away)
    }
}

impl fmt::Display for GazeTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Interactions
// ---------------------------------------------------------------------------

/// What the people in a frame are doing with one another.
///
/// PHASE-10 section 2.1's nine, in that document's order - which is the interaction
/// head's output order, exactly as `SceneId::ALL` is the scene head's. Renumbering it
/// silently relabels a trained model.
///
/// Multi-label rather than one-hot: a frame can be a hug *and* tears being wiped, and
/// section 5 spells the field `Vec<(Interaction, f32)>` for that reason.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Interaction {
    /// Two or more people embracing.
    #[default]
    Hug,
    /// A kiss.
    Kiss,
    /// Hands held.
    HandHold,
    /// A dance hold: two people in frame, in contact, facing.
    DanceHold,
    /// A ring going onto a finger.
    RingExchange,
    /// A blessing, or any ritual touch: a hand on a head, feet touched, a tilak.
    Blessing,
    /// A raised glass.
    Toast,
    /// Somebody wiping somebody else's tears.
    TearsWiped,
    /// A group reacting together: applause, a cheer, arms up.
    GroupCheer,
}

impl Interaction {
    /// Every interaction, in the head's output order.
    pub const ALL: [Self; 9] = [
        Self::Hug,
        Self::Kiss,
        Self::HandHold,
        Self::DanceHold,
        Self::RingExchange,
        Self::Blessing,
        Self::Toast,
        Self::TearsWiped,
        Self::GroupCheer,
    ];

    /// How many classes the head emits.
    pub const COUNT: usize = 9;

    /// The stored text, and the wire slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hug => "hug",
            Self::Kiss => "kiss",
            Self::HandHold => "hand_hold",
            Self::DanceHold => "dance_hold",
            Self::RingExchange => "ring_exchange",
            Self::Blessing => "blessing",
            Self::Toast => "toast",
            Self::TearsWiped => "tears_wiped",
            Self::GroupCheer => "group_cheer",
        }
    }

    /// The photographer-facing label, for a chip on the Emotion card.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Hug => "Hug",
            Self::Kiss => "Kiss",
            Self::HandHold => "Holding hands",
            Self::DanceHold => "Dancing",
            Self::RingExchange => "Ring exchange",
            Self::Blessing => "Blessing",
            Self::Toast => "Toast",
            Self::TearsWiped => "Tears wiped",
            Self::GroupCheer => "Group reaction",
        }
    }

    /// Parse the stored text, or `None`.
    ///
    /// `Option` rather than a default, unlike every other parse in this file. There is
    /// no interaction that claims less than the others - [`Interaction::Hug`] is first
    /// in the list because section 2.1 writes it first, not because it is neutral - so
    /// an unreadable slug must disappear rather than become a hug.
    #[must_use]
    pub fn from_slug(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    /// Position in [`Interaction::ALL`], which is the head's output slot.
    #[must_use]
    pub fn as_index(self) -> usize {
        Self::ALL
            .into_iter()
            .position(|kind| kind == self)
            .unwrap_or(0)
    }

    /// The interaction at one output slot, or `None`.
    #[must_use]
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    /// True when this interaction is a milestone a client buys a print of.
    ///
    /// The four that carry their own narrative weight regardless of how the faces read.
    /// Advice for the peak detector and the ranker; not a decision, and phase 12 is free
    /// to disagree.
    #[must_use]
    pub const fn is_milestone(self) -> bool {
        matches!(
            self,
            Self::Kiss | Self::RingExchange | Self::Blessing | Self::TearsWiped
        )
    }
}

impl fmt::Display for Interaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Expression
// ---------------------------------------------------------------------------

/// One face's expression, as eight continuous readings.
///
/// PHASE-10 section 5's frozen shape. **Continuous, not one-hot** - section 2.1 is
/// explicit - because a face can be laughing and crying at once and a wedding is full
/// of faces that are.
///
/// **[`FaceExpression::composed`] is a positive class.** Section 6.1: "composure is a
/// positive class: in `ritual` and `vows` scenes, `composed` with mutual gaze can
/// outscore a smile". Weights come from `emotion_weights.toml` per scene and per
/// tradition, and section 10.1's composure-fairness gate measures that they do. A
/// product whose emotion model rewards only Western expressiveness delivers a Hindu
/// ceremony as an empty gallery, and this field plus that weight table is the whole of
/// the defence.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FaceExpression {
    /// The face phase 06 found.
    pub face_id: FaceId,
    /// Who that face is, when the identity pass has run and assigned one.
    ///
    /// `None` means unassigned, which is not the same as "a guest": an unclustered face
    /// has no role weight, so it contributes to the frame's expression at the floor
    /// weight rather than at a guessed one.
    pub identity: Option<IdentityId>,
    /// How much of a smile, `0..1`.
    pub smile: f32,
    /// How much the smile reads as unposed, `0..1`.
    ///
    /// Section 6.1's Duchenne cue: eye-region activation alongside mouth shape, so a
    /// posed grin scores lower than real laughter. Exposed as
    /// [`EmotionCode::PosedSmile`] rather than folded silently into the score, because
    /// a photographer who disagrees is entitled to see which half of the face decided
    /// it.
    ///
    /// Meaningless on its own: genuineness at 0.9 with `smile` at 0.05 is a face that
    /// is not smiling, and the ranker multiplies rather than adds for that reason.
    pub genuineness: f32,
    /// How much of a laugh, `0..1`. Open mouth, raised cheeks, usually a head tilt.
    pub laughter: f32,
    /// How much this face reads as crying, `0..1`.
    ///
    /// **Deliberately high-precision.** Section 6.1: "tears detection uses eye-region
    /// specular/wet cues plus expression context, with deliberately high precision (a
    /// wrongly detected tear is embarrassing)". No reason text mentions tears below
    /// [`TEARS_CERTAIN`], and phase 09's third intent rule reads the same constant.
    pub tears: f32,
    /// How much of a surprise, `0..1`.
    pub surprise: f32,
    /// How much tenderness the face reads as, `0..1`. Softened eyes, a small mouth, a
    /// lowered chin - the first-look expression.
    pub tenderness: f32,
    /// How composed the face is, `0..1`. See this type's header.
    pub composed: f32,
    /// How much discomfort or awkwardness the face reads as, `0..1`.
    ///
    /// The one channel that counts *against* a frame, and the only one a photographer
    /// would want to filter for rather than towards.
    pub discomfort: f32,
    /// Where this face is looking.
    pub gaze: GazeTarget,
    /// How sure the head is about this face, `0..1`. Invariant 2.
    pub confidence: f32,
    /// Where the face is, for the crop the Emotion card shows.
    ///
    /// Phase 06's `FaceRef::bbox`, carried rather than re-derived: the crop the card
    /// shows for an expression *is* that box, expanded for context, exactly as phase
    /// 09's evidence crop is.
    pub crop: CropRect,
}

impl FaceExpression {
    /// How many continuous channels one face carries.
    ///
    /// Eight, and it is the expression head's output arity. [`FaceExpression::channels`]
    /// returns them in the head's order.
    pub const CHANNELS: usize = 8;

    /// The eight channels in the head's output order.
    ///
    /// Smile, genuineness, laughter, tears, surprise, tenderness, composed, discomfort -
    /// section 5's order, which is the model's order. Reordering this relabels a trained
    /// head.
    #[must_use]
    pub const fn channels(&self) -> [f32; Self::CHANNELS] {
        [
            self.smile,
            self.genuineness,
            self.laughter,
            self.tears,
            self.surprise,
            self.tenderness,
            self.composed,
            self.discomfort,
        ]
    }

    /// The stable slug of each channel, in [`FaceExpression::channels`] order.
    pub const CHANNEL_NAMES: [&'static str; Self::CHANNELS] = [
        "smile",
        "genuineness",
        "laughter",
        "tears",
        "surprise",
        "tenderness",
        "composed",
        "discomfort",
    ];

    /// A face with nothing read from it, at a named position.
    ///
    /// Every channel zero and the gaze unknown - which is *not* a neutral face but an
    /// unread one, and [`FaceExpression::confidence`] at zero is what says so.
    #[must_use]
    pub const fn unread(face_id: FaceId, crop: CropRect) -> Self {
        Self {
            face_id,
            identity: None,
            smile: 0.0,
            genuineness: 0.0,
            laughter: 0.0,
            tears: 0.0,
            surprise: 0.0,
            tenderness: 0.0,
            composed: 0.0,
            discomfort: 0.0,
            gaze: GazeTarget::Unknown,
            confidence: 0.0,
            crop,
        }
    }

    /// How strongly this face is expressing anything at all, `0..1`.
    ///
    /// The maximum of the seven channels that are *presence* of an expression -
    /// discomfort excluded, because a frame is not more expressive for being awkward.
    /// Used as the peak curve's per-face term and nowhere else; the ranker reads the
    /// channels individually because a scene weights them differently.
    #[must_use]
    pub fn intensity(&self) -> f32 {
        [
            self.smile,
            self.laughter,
            self.tears,
            self.surprise,
            self.tenderness,
            self.composed,
        ]
        .into_iter()
        .fold(0.0f32, f32::max)
        .clamp(0.0, 1.0)
    }

    /// True when this face reads as crying strongly enough to say so out loud.
    ///
    /// The gate on [`EmotionCode::Tears`], on phase 09's third intent rule, and on
    /// anything a photographer reads. See [`TEARS_CERTAIN`].
    ///
    /// **The threshold is on the tear channel, not on the whole face's confidence**, and
    /// the distinction is worth stating because the obvious alternative is wrong.
    /// [`FaceExpression::confidence`] measures how far the *eight* channels sit from a
    /// half, so a face that is emphatically crying and unremarkable in every other
    /// respect scores around 0.7 on it - and gating a tear claim on that number would
    /// mean the product could only say "crying" about a face that was also emphatically
    /// something else. The channel's own value above 0.85 is the claim; the confidence
    /// only has to establish that the face was read at all rather than being a
    /// [`FaceExpression::unread`] placeholder.
    #[must_use]
    pub fn reads_as_crying(&self) -> bool {
        self.tears >= TEARS_CERTAIN && self.confidence > f32::EPSILON
    }

    /// True when the smile is more posed than felt.
    ///
    /// Both halves are required: a face that is not smiling is not posing.
    #[must_use]
    pub fn posed_smile(&self) -> bool {
        self.smile >= 0.45 && self.genuineness < 0.40
    }
}

/// How certain a tear must be before the product says the word.
///
/// Section 12's fourth failure mode: "false tear/crying detection embarrasses the
/// product - high-precision thresholds, cross-check with interaction context, and no
/// tear-based reason text unless confidence >= 0.85."
///
/// It is in the contract rather than in the analyser because **three unrelated places
/// read it**: this crate's [`FaceExpression::reads_as_crying`], the reason writer in
/// `aura_brain_wedding::emotion::score`, and phase 09's third eye-intent rule in
/// `aura_brain_photo::integrity::eyes`. Three copies of 0.85 is three places for it to
/// drift, and the third one lives in a crate that cannot see the other two.
pub const TEARS_CERTAIN: f32 = 0.85;

// ---------------------------------------------------------------------------
// Reasons
// ---------------------------------------------------------------------------

/// Why a frame scored what it scored, as a closed set.
///
/// Section 9 gives DOC "explain emotion scoring honestly in user docs; avoid
/// overclaiming", which is only a finishable job if the set of things the product can
/// say is enumerable. `docs/emotion-and-moments.md` is written against
/// [`EmotionCode::ALL`] and a test asserts every variant appears there.
///
/// **Every sentence describes what is visible.** Not "she was moved" but "this face
/// reads as crying". Section 2.2 puts psychological claims permanently out of scope,
/// and a closed vocabulary is how that becomes structural rather than remembered: a
/// sentence that overclaims cannot be written at a call site, because call sites do not
/// write sentences.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum EmotionCode {
    /// Nothing in particular stood out. The reason a quiet frame carries, so that every
    /// frame has at least one.
    #[default]
    Unremarkable,
    /// A face is smiling and it reads as unposed.
    GenuineSmile,
    /// A face is smiling and it reads as held for the camera.
    ///
    /// **Not a penalty in a posed scene.** A family portrait is *supposed* to be posed,
    /// and `emotion_weights.toml` gives `genuineness` almost no weight there.
    PosedSmile,
    /// A face is laughing.
    Laughter,
    /// A face reads as crying. Only ever written above [`TEARS_CERTAIN`].
    Tears,
    /// A face reads as surprised.
    Surprise,
    /// A face reads as tender.
    Tenderness,
    /// A face is composed, in a scene where composure is what the moment asks for.
    ///
    /// The single most culturally load-bearing code in the product. See
    /// [`FaceExpression::composed`].
    Composure,
    /// A face reads as uncomfortable or caught mid-word.
    Discomfort,
    /// Two people are looking at each other.
    MutualGaze,
    /// The subjects are looking at the lens.
    LookingAtCamera,
    /// An interaction was detected. The strength and the kind are in the text.
    InteractionDetected,
    /// This is the strongest frame of its moment.
    PeakFrame,
    /// This frame is close to its moment's peak.
    NearPeak,
    /// This frame is far from its moment's peak.
    OffPeak,
    /// This frame is a reaction to another frame.
    ReactionTo,
    /// Another frame reacts to this one.
    ActionOf,
    /// A cloud model was asked how significant this moment is to the wedding's story.
    NarrativeWeight,
    /// No face was found, so the score is about the frame rather than about anybody in
    /// it.
    NoFaces,
    /// This wedding has no scene labels, so neutral weights were used.
    ///
    /// Invariant 7 degraded rather than broken, and it lowers
    /// [`ImageEmotion::confidence`] rather than the score.
    NoScene,
}

impl EmotionCode {
    /// Every code, in the order `docs/emotion-and-moments.md` documents them.
    pub const ALL: [Self; 20] = [
        Self::Unremarkable,
        Self::GenuineSmile,
        Self::PosedSmile,
        Self::Laughter,
        Self::Tears,
        Self::Surprise,
        Self::Tenderness,
        Self::Composure,
        Self::Discomfort,
        Self::MutualGaze,
        Self::LookingAtCamera,
        Self::InteractionDetected,
        Self::PeakFrame,
        Self::NearPeak,
        Self::OffPeak,
        Self::ReactionTo,
        Self::ActionOf,
        Self::NarrativeWeight,
        Self::NoFaces,
        Self::NoScene,
    ];

    /// The stable slug, stored and sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unremarkable => "unremarkable",
            Self::GenuineSmile => "genuine_smile",
            Self::PosedSmile => "posed_smile",
            Self::Laughter => "laughter",
            Self::Tears => "tears",
            Self::Surprise => "surprise",
            Self::Tenderness => "tenderness",
            Self::Composure => "composure",
            Self::Discomfort => "discomfort",
            Self::MutualGaze => "mutual_gaze",
            Self::LookingAtCamera => "looking_at_camera",
            Self::InteractionDetected => "interaction_detected",
            Self::PeakFrame => "peak_frame",
            Self::NearPeak => "near_peak",
            Self::OffPeak => "off_peak",
            Self::ReactionTo => "reaction_to",
            Self::ActionOf => "action_of",
            Self::NarrativeWeight => "narrative_weight",
            Self::NoFaces => "no_faces",
            Self::NoScene => "no_scene",
        }
    }

    /// Parse the stored slug. Unknown text is [`EmotionCode::Unremarkable`].
    #[must_use]
    pub fn from_str_or_unremarkable(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|code| code.as_str() == text)
            .unwrap_or(Self::Unremarkable)
    }

    /// The sentence this code says when nothing more specific is known.
    ///
    /// **Stored rows carry the code, not the sentence.** Phase 09's decision, inherited
    /// without change and for the same three reasons: a sentence is sixty to a hundred
    /// and twenty bytes against section 11's 900-byte budget; the wording is copy a
    /// release can change, and stored copy leaves last year's frames explaining
    /// themselves in last year's words; and the wording is what a translation replaces.
    ///
    /// A reason whose text differs from this - one that names an interaction, or a
    /// margin - stores its text as well, and only then.
    #[must_use]
    pub const fn user_text(self) -> &'static str {
        match self {
            Self::Unremarkable => {
                "nothing here stands out as a moment, which is not the same as nothing being wrong \
                 with it"
            }
            Self::GenuineSmile => "somebody is smiling here, and it reads as unposed",
            Self::PosedSmile => {
                "the smile here reads as held for the camera rather than caught, which is exactly \
                 right in a posed frame and less so in a candid one"
            }
            Self::Laughter => "somebody is laughing here",
            Self::Tears => {
                "a face here reads as crying: wet eyes and the expression around them agree"
            }
            Self::Surprise => "a face here reads as surprised",
            Self::Tenderness => "the expression here reads as tender rather than as a smile",
            Self::Composure => {
                "the faces here are composed, and in this part of the day that is what the moment \
                 asks for rather than an absence of feeling"
            }
            Self::Discomfort => {
                "a face here reads as awkward, or caught mid-word, which is usually a frame either \
                 side of a better one"
            }
            Self::MutualGaze => "two people are looking at each other here",
            Self::LookingAtCamera => "the subjects are looking at the lens",
            Self::InteractionDetected => "something is happening between the people in this frame",
            Self::PeakFrame => {
                "of everything shot in this moment, this frame is where the action is strongest"
            }
            Self::NearPeak => "this is close to the strongest frame of its moment",
            Self::OffPeak => {
                "this frame is either side of the strongest one in its moment, which is often the \
                 run-up or the settle rather than a fault"
            }
            Self::ReactionTo => "this frame is somebody reacting to something in another frame",
            Self::ActionOf => "another frame catches somebody reacting to this one",
            Self::NarrativeWeight => {
                "AURA asked a cloud model how important this moment is to the day's story, and this \
                 is what it said"
            }
            Self::NoFaces => {
                "no face was found here, so this reading is about the frame rather than about \
                 anybody in it"
            }
            Self::NoScene => {
                "AURA has not worked out what part of the day this is, so it has weighed the \
                 expression neutrally rather than for this kind of photograph"
            }
        }
    }

    /// True when this code is a note about the reading rather than about the frame.
    ///
    /// The three that say *how much to trust the number* rather than what is in the
    /// photograph. The panel renders them apart from the rest, and the eval harness
    /// asserts they never carry a positive weight.
    #[must_use]
    pub const fn is_caveat(self) -> bool {
        matches!(self, Self::NoFaces | Self::NoScene | Self::NarrativeWeight)
    }
}

impl fmt::Display for EmotionCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One thing that moved the emotion score, with the pixels behind it.
///
/// Invariant 2 makes it mandatory. The crop is the face the reading came from, so that
/// section 13's fifth criterion - "the Emotion card explains the score with crops and
/// short editorial reasons" - is met by the data rather than by the interface guessing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EmotionReason {
    /// Which reason this is.
    pub code: EmotionCode,
    /// The sentence a photographer reads, in the product's voice.
    pub text: String,
    /// How much of the score this moved.
    ///
    /// Signed: negative for something that cost the frame, positive for something that
    /// earned it. Most reasons in this phase are positive, which is the opposite of
    /// phase 09's - a technical verdict explains penalties and an emotion score explains
    /// what it found.
    pub weight: f32,
    /// The face, or `None` when the reason is about the whole frame.
    pub evidence: Option<CropRect>,
}

impl EmotionReason {
    /// A reason about the whole frame.
    #[must_use]
    pub fn frame(code: EmotionCode, text: impl Into<String>, weight: f32) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: None,
        }
    }

    /// A reason about one face.
    #[must_use]
    pub fn at(code: EmotionCode, text: impl Into<String>, weight: f32, evidence: CropRect) -> Self {
        Self {
            code,
            text: text.into(),
            weight,
            evidence: Some(evidence),
        }
    }

    /// True when this reason earned the frame something.
    #[must_use]
    pub fn is_credit(&self) -> bool {
        self.weight > 0.0
    }
}

// ---------------------------------------------------------------------------
// Peaks
// ---------------------------------------------------------------------------

/// What kind of peak a moment has.
///
/// Section 6.2 names four explicitly - "kiss apex, tear release, bouquet-in-air and
/// ring-slide are trained as explicit peak types because they are the frames clients
/// buy" - plus the general case and the case where there is nothing to peak.
///
/// **In this build the four named kinds are derived, not trained.** They come from the
/// scene label and the detected interaction rather than from a peak-type head, and the
/// phase 10 exit report records that as an open condition. The vocabulary is frozen now
/// so that a trained head later changes how the field is filled and not who reads it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum PeakKind {
    /// The moment has a strongest frame and nothing names it more precisely. The
    /// default.
    #[default]
    Expression,
    /// The kiss.
    KissApex,
    /// The frame a tear is at its clearest.
    TearRelease,
    /// The bouquet, mid-air.
    BouquetInAir,
    /// The ring going on.
    RingSlide,
    /// Nothing in the moment separated itself. See [`MomentPeak::is_resolved`].
    Flat,
}

impl PeakKind {
    /// Every kind, in the order the panel lists them.
    pub const ALL: [Self; 6] = [
        Self::Expression,
        Self::KissApex,
        Self::TearRelease,
        Self::BouquetInAir,
        Self::RingSlide,
        Self::Flat,
    ];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Expression => "expression",
            Self::KissApex => "kiss_apex",
            Self::TearRelease => "tear_release",
            Self::BouquetInAir => "bouquet_in_air",
            Self::RingSlide => "ring_slide",
            Self::Flat => "flat",
        }
    }

    /// Parse the stored text. Unknown values read as [`PeakKind::Expression`].
    #[must_use]
    pub fn from_str_or_expression(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == text)
            .unwrap_or(Self::Expression)
    }
}

impl fmt::Display for PeakKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where one moment's action was strongest.
///
/// Not in section 5, which keeps the peak as two fields on [`ImageEmotion`]. Both of
/// those survive; this is the moment-level answer, which needs a margin and a kind that
/// a per-frame field has nowhere to put.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MomentPeak {
    /// The moment.
    pub moment_id: MomentId,
    /// The strongest frame.
    pub image_id: ImageId,
    /// Its position in the moment's frames, zero-based.
    pub index: u32,
    /// How many frames the moment held when this was computed.
    pub frames: u32,
    /// How clearly the peak wins, `0..1`.
    ///
    /// Section 6.2's `peak_margin`. `(top - runner_up) / top`, so zero means two frames
    /// tied and one means everything else was flat. Below [`MomentPeak::MIN_MARGIN`] the
    /// kind is [`PeakKind::Flat`] and `AURA-ML-5042` is logged: a peak nobody can
    /// separate is a claim this phase has not earned, and a burst of fourteen
    /// near-identical frames genuinely has no apex.
    pub margin: f32,
    /// What kind of peak it is.
    pub kind: PeakKind,
    /// How sure the choice is, `0..1`. Invariant 2.
    pub confidence: f32,
    /// Why this frame, at most [`MomentPeak::MAX_REASONS`]. Invariant 2.
    pub reasons: Vec<EmotionReason>,
    /// True when the photographer picked the peak by hand.
    ///
    /// Checked inside the statement a re-analysis would overwrite it with, exactly as
    /// `identities.user_locked`, `segments.user_locked`, `moments.user_locked` and
    /// `image_integrity.user_reviewed` are. Fifth phase, fifth unbeatable decision.
    pub user_chosen: bool,
}

impl MomentPeak {
    /// The most reasons one peak carries.
    pub const MAX_REASONS: usize = 4;

    /// Below this margin the peak is not separated from its neighbours.
    ///
    /// Section 10.1 measures the chosen frame against the human top-2, which is a
    /// forgiving target precisely because moments where the top two frames are within a
    /// hair are moments where either answer is right. This is where the product stops
    /// pretending otherwise.
    pub const MIN_MARGIN: f32 = 0.04;

    /// True when the peak is separated well enough to be worth saying.
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        self.margin >= Self::MIN_MARGIN && !matches!(self.kind, PeakKind::Flat)
    }
}

// ---------------------------------------------------------------------------
// Reactions
// ---------------------------------------------------------------------------

/// One frame reacting to another.
///
/// Section 6.3, "a feature no competitor ships": the mother's tears while the couple
/// kisses, linked across cameras, so that phase 12 can guarantee the pair survives a
/// cull together and phase 29 can build the spread.
///
/// The link is **directional and asymmetric**: an action has many reactions and a
/// reaction has one action. [`ImageEmotion::reaction_of`] is the reaction's view of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReactionLink {
    /// The frame something happened in.
    pub action: ImageId,
    /// The frame somebody reacted in.
    pub reaction: ImageId,
    /// Milliseconds between them, signed: negative when the reaction is earlier.
    ///
    /// Signed rather than absolute because a reaction *before* its action is real - a
    /// second shooter's clock is not the first shooter's, and phase 01's timeline
    /// alignment is good to about a second - and a sign that is thrown away is a
    /// diagnosis nobody can make later.
    pub gap_ms: i64,
    /// How much this link adds to the reaction's score, `0..1`.
    ///
    /// Section 6.3: "reactions are scored with a bonus proportional to the action's
    /// significance and the reactor's role weight."
    pub bonus: f32,
    /// How sure the link is, `0..1`. Invariant 2.
    pub confidence: f32,
    /// Why these two frames, at most [`ReactionLink::MAX_REASONS`]. Invariant 2.
    pub reasons: Vec<EmotionReason>,
}

impl ReactionLink {
    /// The most reasons one link carries.
    pub const MAX_REASONS: usize = 3;

    /// The widest gap a link may span, in milliseconds.
    ///
    /// Section 6.3's "+/- 4 s". Wide enough for a parent to turn their head and a second
    /// shooter to press the shutter; narrow enough that two unrelated moments in a busy
    /// reception do not get joined. It is a hard bound rather than a threshold: a
    /// candidate outside it is not scored at all.
    pub const WINDOW_MS: i64 = 4_000;

    /// True when the reaction came after the action, as most do.
    #[must_use]
    pub const fn is_after(&self) -> bool {
        self.gap_ms >= 0
    }
}

// ---------------------------------------------------------------------------
// The per-image result
// ---------------------------------------------------------------------------

/// Everything phase 10 knows about one photograph.
///
/// PHASE-10 section 5's frozen shape, plus the three additions this module's header
/// lists. Every field is a reading or something derived from one; none of them is a
/// decision about whether the photograph is delivered.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageEmotion {
    /// The photograph.
    pub image_id: ImageId,
    /// Every face in the frame, in the order phase 06 ranks them.
    ///
    /// Not in section 5; see this module's header. Empty is not "nobody is in it" - it
    /// is "no face pass has run, or none was found", and [`EmotionCode::NoFaces`] is
    /// what says which.
    pub faces: Vec<FaceExpression>,
    /// What the people are doing, with the head's strength for each, strongest first.
    ///
    /// Only entries above the head's detection floor are stored; an empty vector means
    /// nothing was detected rather than that nothing was happening.
    pub interactions: Vec<(Interaction, f32)>,
    /// True when two faces in the frame are looking at each other.
    pub mutual_gaze: bool,
    /// Where this frame sits on its moment's action curve, `0..1`.
    ///
    /// Section 5: "1.0 at the moment's peak frame". `0.5` when the frame is not in a
    /// moment or is alone in one - the neutral value, because a frame with no siblings
    /// is neither the peak nor off it. The same convention
    /// `IntegrityResult::relative_sharpness` uses, deliberately.
    pub peak_proximity: f32,
    /// The frame this one reacts to, when it reacts to one.
    pub reaction_of: Option<ImageId>,
    /// The scene-weighted, calibrated composite, `0..1`.
    ///
    /// Section 6.4: the ranker's output, mapped through a per-scene isotonic
    /// calibration, so that the number is comparable with
    /// `IntegrityResult::technical_score` in phase 12 and so that 0.8 means the same
    /// thing in a ceremony and on a dance floor.
    ///
    /// **Not a keep decision.** A frame at 0.22 may be the only photograph of the ring
    /// exchange, and phase 12 knows that; this crate does not, and must not.
    pub emotion_score: f32,
    /// How important this moment is to the wedding's story, `0..1`.
    ///
    /// Zero unless a cloud `MomentSignificance` call was made and returned - section 7's
    /// offline fallback is "local ranker output only, with `narrative_weight = 0` and
    /// lower confidence". It is carried separately from
    /// [`ImageEmotion::emotion_score`] rather than folded into it so that a caller can
    /// always tell what the local model said on its own.
    pub narrative_weight: f32,
    /// Which scene the weights were conditioned on.
    ///
    /// Invariant 7: a stored score that does not say which scene it assumed is not
    /// reproducible. [`SceneId::Unknown`] means neutral weights were used and
    /// [`EmotionCode::NoScene`] is in the reasons.
    pub scene: SceneId,
    /// Why, at most [`ImageEmotion::MAX_REASONS`], strongest credit first. Invariant 2.
    pub reasons: Vec<EmotionReason>,
    /// How sure the whole reading is, `0..1`. Invariant 2.
    ///
    /// Lowered by a missing scene label, a missing face pass, a low-confidence head and
    /// an unresolved peak. A score drawn with three of its inputs missing has to be able
    /// to say so.
    pub confidence: f32,
    /// Whether the local model, a cloud answer or the photographer had the last word.
    ///
    /// Section 5's `source`, and it is [`crate::contract::scene::Source`] rather than a
    /// new enum: phase 07 already froze exactly this three-valued question about exactly
    /// this kind of row.
    pub source: Source,
    /// Which learned heads produced the expressions and the interactions.
    pub model_ver: u16,
    /// Which build's arithmetic produced the gaze, the peaks and the links.
    pub analysis_ver: u16,
    /// Which weight, ranker and calibration table produced the score.
    pub weights_ver: u16,
}

impl ImageEmotion {
    /// The most reasons one frame carries.
    ///
    /// Six, matching `Moment::MAX_REASONS` and `IntegrityResult::MAX_REASONS`. A card
    /// that lists nine reasons is a card nobody reads, and the ranking that picks the
    /// six is part of the explanation.
    pub const MAX_REASONS: usize = 6;

    /// The strongest interaction and its strength, when there is one.
    #[must_use]
    pub fn top_interaction(&self) -> Option<(Interaction, f32)> {
        self.interactions.first().copied()
    }

    /// True when the frame carries a milestone interaction at any strength.
    #[must_use]
    pub fn has_milestone(&self) -> bool {
        self.interactions
            .iter()
            .any(|(kind, _)| kind.is_milestone())
    }

    /// The face this frame is most about, by expression intensity then by crop area.
    ///
    /// The face the Emotion card leads with.
    #[must_use]
    pub fn lead_face(&self) -> Option<&FaceExpression> {
        self.faces.iter().max_by(|left, right| {
            left.intensity()
                .total_cmp(&right.intensity())
                .then((left.crop.w * left.crop.h).total_cmp(&(right.crop.w * right.crop.h)))
        })
    }

    /// Faces that read as crying, above [`TEARS_CERTAIN`].
    #[must_use]
    pub fn crying(&self) -> Vec<&FaceExpression> {
        self.faces
            .iter()
            .filter(|face| face.reads_as_crying())
            .collect()
    }

    /// True when any face in this frame reads as crying.
    ///
    /// **Phase 09's third eye-intent rule reads exactly this.** Section 6.4 of PHASE-09
    /// lists "tears detected in Phase 10" as one of four reasons a closed eye is not a
    /// defect, and `IntegrityPass::with_emotion` is the wire. Written here rather than
    /// at the call site so that the crate which cannot see this one still asks the
    /// question the same way.
    #[must_use]
    pub fn has_tears(&self) -> bool {
        self.faces.iter().any(FaceExpression::reads_as_crying)
    }

    /// True when the three version fields match this build's.
    ///
    /// The check `AURA-ML-5038` is raised on. Written here rather than in the analyser
    /// so that every consumer asks the question the same way.
    #[must_use]
    pub const fn matches_versions(&self, model: u16, analysis: u16, weights: u16) -> bool {
        self.model_ver == model && self.analysis_ver == analysis && self.weights_ver == weights
    }
}

// ---------------------------------------------------------------------------
// Preference
// ---------------------------------------------------------------------------

/// One photographer's answer to "which of these two would you deliver?".
///
/// Section 2.1's preference-learning hook and section 6.4's Bradley-Terry ranker. The
/// comparisons are collected now and **fitted offline**: this build ships the
/// coefficients that `ml/models/emotion/train_ranker.py` produced, and phase 30 is where
/// a photographer's own comparisons start moving them.
///
/// Stored rather than applied, deliberately. A ranker that refits itself while a
/// photographer is culling would change the order of the grid under their hands, and
/// invariant 4 would stop being true the moment it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Preference {
    /// The frame the photographer would deliver.
    pub winner: ImageId,
    /// The frame they would not.
    pub loser: ImageId,
}

// ---------------------------------------------------------------------------
// Coverage
// ---------------------------------------------------------------------------

/// What a project's emotion pass covered and what it found.
///
/// Not in section 5. Phase 05's rule, inherited for the sixth time: report coverage when
/// you report a result, and say what the denominator is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EmotionOutline {
    /// Photographs with a score.
    pub scored: u32,
    /// Photographs in the project.
    pub photos: u32,
    /// Fraction of the project with a score, `0..1`.
    ///
    /// The denominator is **every photograph**, as phase 09's is and unlike phase 08's.
    /// The interaction head reads the frame, so a photograph with no faces and no
    /// embedding still gets a reading; a frame with no score is this phase's gap.
    pub coverage: f32,
    /// Fraction of *scored* frames that carried at least one face expression, `0..1`.
    ///
    /// This phase's refinement of the coverage rule, and the exact parallel of
    /// `IntegrityOutline::subject_aware`. A wedding scored at 100 % coverage with
    /// `face_aware` at 0.03 has been ranked on interactions and scene alone - which is
    /// very nearly ranking on nothing, since seven of the nine feature terms come from
    /// faces - and a caller about to trust an ordering needs to know that before it
    /// does.
    pub face_aware: f32,
    /// Moments in the project.
    pub moments: u32,
    /// Moments whose peak separated itself. See [`MomentPeak::is_resolved`].
    pub peaked: u32,
    /// Reaction links found.
    pub links: u32,
    /// How many frames carry each interaction, in [`Interaction::ALL`] order.
    ///
    /// Section 11's `emotion.scored {interaction_histogram}` event, computed once.
    pub interaction_histogram: [u32; Interaction::COUNT],
    /// Mean emotion score over scored frames.
    pub mean_score: f32,
    /// Mean peak margin over resolved peaks.
    ///
    /// Section 11's `emotion.peaks {mean_margin}`.
    pub mean_margin: f32,
    /// Pairwise preferences the photographer has recorded.
    pub preferences: u32,
    /// Which learned heads produced the numbers.
    pub model_ver: u16,
    /// Which build's arithmetic produced them.
    pub analysis_ver: u16,
    /// Which weight and ranker table scored them.
    pub weights_ver: u16,
}

impl EmotionOutline {
    /// True when nothing has been scored.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.scored == 0
    }

    /// How many frames carry one interaction.
    #[must_use]
    pub fn count(&self, kind: Interaction) -> u32 {
        self.interaction_histogram
            .get(kind.as_index())
            .copied()
            .unwrap_or(0)
    }

    /// Fraction of moments whose peak separated itself, `0..1`.
    ///
    /// The number that says whether "the peak frame of each moment" is a claim this
    /// wedding supports. A wedding of fourteen-frame bracketed bursts has very few
    /// resolved peaks and that is the correct answer, not a failure.
    #[must_use]
    pub fn peak_rate(&self) -> f32 {
        if self.moments == 0 {
            return 0.0;
        }
        let per_mille = u64::from(self.peaked)
            .saturating_mul(1_000)
            .checked_div(u64::from(self.moments))
            .unwrap_or(0)
            .min(1_000);
        f32::from(u16::try_from(per_mille).unwrap_or(1_000)) / 1_000.0
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask what a photograph is worth.
///
/// Frozen. Implemented by `aura_brain_wedding::emotion::Emotion`, and the only route
/// from any later phase to an expression, an interaction, a peak or a reaction link.
///
/// The rule phase 05 wrote for `SimilarityIndex`, phase 06 for `PeopleService`, phase 07
/// for `StoryService`, phase 08 for `MomentService` and phase 09 for `IntegrityService`,
/// a sixth time and for the same reason: **no phase may keep its own expression model or
/// its own idea of what a moment is worth.** Two answers to "which of these six frames
/// is the one" is two galleries that disagree.
pub trait EmotionService: Send + Sync + fmt::Debug {
    /// What a project's pass covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the scores cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<EmotionOutline>;

    /// One photograph's reading, or `None` when it has not been scored.
    ///
    /// `None` means **nobody looked**, and it is not the same as a score of zero. Every
    /// consumer of this trait is expected to render the difference.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the reading cannot be read.
    fn of_image(&self, image: ImageId) -> AuraResult<Option<ImageEmotion>>;

    /// One moment's peak, or `None` when the moment has not been scored.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the peak cannot be read.
    fn peak_of(&self, moment: MomentId) -> AuraResult<Option<MomentPeak>>;

    /// Every frame that reacts to this one, earliest first.
    ///
    /// The action's view of the link. A frame's own view - the one action it reacts to -
    /// is [`ImageEmotion::reaction_of`] and needs no call.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the links cannot be read.
    fn reactions_to(&self, action: ImageId) -> AuraResult<Vec<ReactionLink>>;

    /// A project's frames ordered by emotion score, strongest first.
    ///
    /// **An ordering, not a selection.** This phase's headline is "ranks every frame by
    /// emotional value", and section 2.2 puts the choosing in phase 12. `limit` bounds
    /// the answer because the caller is a page.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the scores cannot be read.
    fn ranked(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<(ImageId, f32)>>;

    /// Record that the photographer would deliver one of two frames.
    ///
    /// Section 2.1's preference hook. It is written to the journal and **does not change
    /// any score in this build**; see [`Preference`].
    ///
    /// # Errors
    ///
    /// `AURA-ML-5041` when either photograph is unknown, when they are the same
    /// photograph, or when they belong to different projects.
    fn prefer(&self, choice: Preference) -> Result<(), AuraError>;

    /// Record that the photographer disagrees with a moment's peak frame.
    ///
    /// Sets [`MomentPeak::user_chosen`], which a re-analysis will not overwrite. It
    /// changes which frame the browser leads with and it changes nothing about what is
    /// delivered.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5041` when the moment is unknown or the photograph is not in it.
    fn set_peak(&self, moment: MomentId, image: ImageId) -> Result<(), AuraError>;
}
