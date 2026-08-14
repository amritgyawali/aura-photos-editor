//! FROZEN CONTRACT. The wedding's scene vocabulary and its ordered story.
//!
//! PHASE-07 section 5 freezes these shapes before any classifier exists, and the
//! reason this file is in `aura-core` rather than in `aura-brain-wedding` is the list
//! of phases that consume it: 09 (technical quality), 10 (emotion), 11 (composition),
//! 12 (culling and coverage), 13 (the explainability ledger), 15 to 17 (editing
//! intent), 18 to 22 (masks and retouch), 25 (gallery consistency), 27 (the QC agent)
//! and 29 (curation).
//!
//! Every one of them needs the word `ceremony` and the tolerances that hang off it.
//! None of them needs the classifier, the change-point detector or an ONNX session.
//! Invariant 7 - "no threshold is global; every threshold is a function of the
//! detected scene and subject role" - becomes a lookup in [`SceneProfile`], and a
//! lookup that costs a crate dependency is a lookup somebody will skip.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## Five spellings differ from the phase document
//!
//! All five are recorded in
//! `docs/adr/ADR-0015-wedding-scene-taxonomy-and-story-segmentation.md` section 2.
//!
//! * **`ImageId` is `PhotoId`.** One type, aliased rather than duplicated, exactly as
//!   `aura_index` and [`crate::contract::people`] already do it.
//! * **`Timestamp` is `i64` milliseconds since the Unix epoch.** The catalog stores
//!   RFC 3339 text and `aura_index::store::parse_rfc3339_ms` is the one parser in the
//!   product. A second time representation would let a chapter boundary and a person's
//!   appearance disagree about when a photograph was taken, which is exactly the
//!   45-second median this phase is graded on.
//! * **`top3` is `[SceneScore; 3]`, not `[(SceneId, f32); 3]`.** A named pair
//!   serialises to an object with named fields rather than a two-element array, which
//!   is what the IPC surface and the Explain panel need. The arity is unchanged.
//! * **[`AttrFlags`] is a hand-rolled `u16` newtype rather than a `bitflags!` macro.**
//!   `aura-core` is the crate that is forbidden to depend on anything, and adding a
//!   dependency to save forty lines of `const fn` is a bad trade.
//! * **[`Segment::reasons`] is capped at [`Segment::MAX_REASONS`].** Invariant 2
//!   requires reasons; nothing requires an unbounded number of them.
//!
//! Changing anything in this file requires an ADR.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{PhotoId, ProjectId, SegmentId};

/// The identity of a photograph in a scene query.
///
/// PHASE-07 names this `ImageId`; the catalog calls it a `PhotoId`. One type.
pub type ImageId = PhotoId;

/// Milliseconds since the Unix epoch.
///
/// See this module's header for why there is only one of these in the product.
pub type Timestamp = i64;

// ---------------------------------------------------------------------------
// Scenes
// ---------------------------------------------------------------------------

/// What one photograph is of.
///
/// Twenty-two classes, exactly PHASE-07 section 2.1's list, in the order that
/// document writes them - which is roughly the order of a wedding day and is also the
/// order the classifier's output vector is in. [`SceneId::ALL`] is that order, and
/// `as_index` is a position in it, so a posterior vector and this enum can never
/// disagree about which slot means `ceremony`.
///
/// The list is **closed and lives in code**, unlike the ritual taxonomy which lives in
/// config. The reason is in ADR-0015 section 3: a scene is a `match` arm in ten later
/// phases, a row in `scene_profiles`, and the output arity of a trained model. Adding
/// one is a retrain, a migration and a profile row - not a config edit.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum SceneId {
    /// One half of the couple preparing, with their party.
    GettingReadyBride,
    /// The other half preparing.
    GettingReadyGroom,
    /// Rings, shoes, invitations, flowers - objects, not people.
    Details,
    /// The staged first sight of one partner by the other.
    FirstLook,
    /// The processional; people arriving at the ceremony space.
    CeremonyEntrance,
    /// The ceremony proper, without a more specific label.
    Ceremony,
    /// A named tradition-specific rite. See [`RitualId`].
    Ritual,
    /// Spoken vows.
    Vows,
    /// The ring exchange.
    Rings,
    /// The kiss.
    Kiss,
    /// A posed group of family.
    FamilyPortrait,
    /// A posed group that is not specifically family.
    GroupPortrait,
    /// The couple, alone, posed.
    CouplePortrait,
    /// Low sun, usually the couple, usually outdoors.
    GoldenHour,
    /// The couple's entrance into the reception.
    ReceptionEntrance,
    /// Toasts and speeches.
    Speeches,
    /// The cake.
    Cake,
    /// The first dance.
    FirstDance,
    /// General dancing.
    DanceFloor,
    /// Unposed, unassigned to a moment.
    Candid,
    /// The place, without the people.
    Venue,
    /// The departure, the send-off, the car.
    Exit,
    /// Not one of the above, or not yet decided. The default, and never a failure.
    #[default]
    Unknown,
}

impl SceneId {
    /// Every scene, in the order PHASE-07 section 2.1 writes them.
    ///
    /// This order is the classifier's output order. It is part of the contract with a
    /// trained model, and reordering it silently relabels every stored posterior.
    pub const ALL: [Self; 23] = [
        Self::GettingReadyBride,
        Self::GettingReadyGroom,
        Self::Details,
        Self::FirstLook,
        Self::CeremonyEntrance,
        Self::Ceremony,
        Self::Ritual,
        Self::Vows,
        Self::Rings,
        Self::Kiss,
        Self::FamilyPortrait,
        Self::GroupPortrait,
        Self::CouplePortrait,
        Self::GoldenHour,
        Self::ReceptionEntrance,
        Self::Speeches,
        Self::Cake,
        Self::FirstDance,
        Self::DanceFloor,
        Self::Candid,
        Self::Venue,
        Self::Exit,
        Self::Unknown,
    ];

    /// How many classes the classifier emits.
    ///
    /// Twenty-two. [`SceneId::Unknown`] is the abstention, is never a softmax slot, and
    /// is the twenty-third entry of [`SceneId::ALL`] for exactly that reason.
    pub const CLASSES: usize = 22;

    /// Stable text for the `image_scenes.scene` column and the IPC surface.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GettingReadyBride => "getting_ready_bride",
            Self::GettingReadyGroom => "getting_ready_groom",
            Self::Details => "details",
            Self::FirstLook => "first_look",
            Self::CeremonyEntrance => "ceremony_entrance",
            Self::Ceremony => "ceremony",
            Self::Ritual => "ritual",
            Self::Vows => "vows",
            Self::Rings => "rings",
            Self::Kiss => "kiss",
            Self::FamilyPortrait => "family_portrait",
            Self::GroupPortrait => "group_portrait",
            Self::CouplePortrait => "couple_portrait",
            Self::GoldenHour => "golden_hour",
            Self::ReceptionEntrance => "reception_entrance",
            Self::Speeches => "speeches",
            Self::Cake => "cake",
            Self::FirstDance => "first_dance",
            Self::DanceFloor => "dance_floor",
            Self::Candid => "candid",
            Self::Venue => "venue",
            Self::Exit => "exit",
            Self::Unknown => "unknown",
        }
    }

    /// Parse the stored text.
    ///
    /// An unrecognised value is [`SceneId::Unknown`] rather than an error, for the
    /// reason [`crate::contract::people::Role::from_str_or_unknown`] gives: a catalog
    /// written by a newer build must still open, and "a scene this build does not know"
    /// and "no scene" have the same consequence for a threshold lookup.
    #[must_use]
    pub fn from_str_or_unknown(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|scene| scene.as_str() == text)
            .unwrap_or(Self::Unknown)
    }

    /// Position in [`SceneId::ALL`], which is the classifier's output slot.
    #[must_use]
    pub fn as_index(self) -> usize {
        Self::ALL
            .into_iter()
            .position(|scene| scene == self)
            .unwrap_or(Self::CLASSES)
    }

    /// The scene at one classifier output slot, or [`SceneId::Unknown`].
    #[must_use]
    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(Self::Unknown)
    }

    /// The chapter this scene belongs to when nothing else is known.
    ///
    /// The HMM's emission prior and the segment labeller both start here. It is a
    /// total function on purpose: a scene with no chapter would be a frame that cannot
    /// appear in the story, and there is no such frame.
    #[must_use]
    pub const fn chapter(self) -> ChapterId {
        match self {
            Self::GettingReadyBride | Self::GettingReadyGroom => ChapterId::GettingReady,
            Self::Details | Self::Venue => ChapterId::Details,
            Self::FirstLook | Self::CeremonyEntrance | Self::Ceremony | Self::Vows | Self::Kiss => {
                ChapterId::Ceremony
            }
            Self::Ritual | Self::Rings => ChapterId::Rituals,
            Self::FamilyPortrait
            | Self::GroupPortrait
            | Self::CouplePortrait
            | Self::GoldenHour => ChapterId::Portraits,
            Self::ReceptionEntrance | Self::Speeches | Self::Cake => ChapterId::Reception,
            Self::FirstDance | Self::DanceFloor => ChapterId::Dance,
            Self::Exit => ChapterId::Exit,
            Self::Candid | Self::Unknown => ChapterId::Other,
        }
    }

    /// This scene in `aura_cloud::tasks::ALLOWED_SCENES`, phase 04's coarser
    /// eighteen-word vocabulary.
    ///
    /// See ADR-0015 section 4. The two vocabularies are not reconciled by renaming
    /// either: `ALLOWED_SCENES` is frozen into three recorded cassettes and every
    /// cached cloud answer in every installed build, and the coarse label is what a
    /// model can actually judge from a contact sheet. `getting_ready_bride` versus
    /// `getting_ready_groom` is a question about *who is in the frame*, which belongs
    /// to `PeopleService` and not to a vision model.
    ///
    /// Exhaustive with no wildcard arm, so a new scene added without extending this is
    /// a compile error.
    #[must_use]
    pub const fn cloud_label(self) -> &'static str {
        match self {
            Self::GettingReadyBride | Self::GettingReadyGroom => "getting_ready",
            Self::FirstLook => "first_look",
            Self::CeremonyEntrance | Self::Ceremony | Self::Vows | Self::Kiss | Self::Rings => {
                "ceremony"
            }
            Self::Ritual => "ritual",
            Self::CouplePortrait | Self::GoldenHour => "portraits",
            Self::FamilyPortrait => "family_formals",
            Self::GroupPortrait => "group_photo",
            Self::ReceptionEntrance => "reception_entrance",
            Self::Speeches => "speeches",
            Self::Cake => "cake_cutting",
            Self::FirstDance => "first_dance",
            Self::DanceFloor => "dancing",
            Self::Details => "details",
            Self::Venue => "venue",
            Self::Exit => "departure",
            Self::Candid => "candid",
            Self::Unknown => "other",
        }
    }

    /// The coarse label back, for a cloud answer that has to become a `SceneId`.
    ///
    /// Lossy by construction - `getting_ready` cannot say which half of the couple was
    /// getting ready - so it returns the *unspecified* member of each family and the
    /// caller keeps the local classifier's finer choice when the two agree at the
    /// coarse level. `aura_brain_wedding::story::naming` is the only caller.
    #[must_use]
    // One arm per member of phase 04's frozen vocabulary, in that vocabulary's order,
    // even where two of them land on the same scene. Merging `dinner` and `candid`
    // into one pattern would save a line and lose the property that makes this
    // function checkable by eye: that every one of the eighteen allowed labels appears
    // exactly once.
    #[allow(clippy::match_same_arms)]
    pub fn from_cloud_label(text: &str) -> Self {
        match text {
            "getting_ready" => Self::GettingReadyBride,
            "first_look" => Self::FirstLook,
            "ceremony" => Self::Ceremony,
            "ritual" => Self::Ritual,
            "portraits" => Self::CouplePortrait,
            "family_formals" => Self::FamilyPortrait,
            "group_photo" => Self::GroupPortrait,
            "reception_entrance" => Self::ReceptionEntrance,
            "speeches" => Self::Speeches,
            "cake_cutting" => Self::Cake,
            "first_dance" => Self::FirstDance,
            "dancing" => Self::DanceFloor,
            "dinner" => Self::Candid,
            "details" => Self::Details,
            "venue" => Self::Venue,
            "departure" => Self::Exit,
            "candid" => Self::Candid,
            _ => Self::Unknown,
        }
    }

    /// This scene in `aura_vision::face::roles`' four-word vocabulary, or `None`.
    ///
    /// Phase 06's couple contest weights three scene terms - getting ready, ceremony,
    /// couple portraits - and separates close from extended family with a fourth. It
    /// has been receiving an empty map since phase 06 shipped, which is why
    /// `roles::SCENELESS_CONFIDENCE_CEILING` exists. This function is what turns it
    /// off.
    ///
    /// `None` for the scenes that say nothing about who somebody is: a detail shot and
    /// a venue shot usually contain no people at all, and a candid contains everybody.
    #[must_use]
    pub const fn role_label(self) -> Option<&'static str> {
        match self {
            Self::GettingReadyBride | Self::GettingReadyGroom => Some("getting_ready"),
            Self::CeremonyEntrance
            | Self::Ceremony
            | Self::Ritual
            | Self::Vows
            | Self::Rings
            | Self::Kiss => Some("ceremony"),
            Self::FirstLook | Self::CouplePortrait | Self::GoldenHour => Some("portraits"),
            Self::FamilyPortrait => Some("family_formals"),
            Self::GroupPortrait
            | Self::ReceptionEntrance
            | Self::Speeches
            | Self::Cake
            | Self::FirstDance
            | Self::DanceFloor
            | Self::Candid
            | Self::Details
            | Self::Venue
            | Self::Exit
            | Self::Unknown => None,
        }
    }

    /// True when this scene is one a coverage guarantee can be written against.
    ///
    /// Phase 12 owns the guarantee; this is only the vocabulary for it, and it is here
    /// rather than there so that ten phases do not each write their own list.
    #[must_use]
    pub const fn is_pivotal(self) -> bool {
        matches!(
            self,
            Self::Ceremony
                | Self::Ritual
                | Self::Vows
                | Self::Rings
                | Self::Kiss
                | Self::FirstLook
                | Self::FamilyPortrait
                | Self::CouplePortrait
                | Self::FirstDance
                | Self::Cake
                | Self::Exit
        )
    }
}

impl fmt::Display for SceneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One entry of [`SceneResult::top3`].
///
/// A named pair rather than a tuple; see this module's header.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SceneScore {
    /// The class.
    pub scene: SceneId,
    /// Its posterior, `0..1`.
    pub score: f32,
}

impl SceneScore {
    /// A score of zero for an unknown scene. The identity element of a top-3 list.
    pub const NONE: Self = Self {
        scene: SceneId::Unknown,
        score: 0.0,
    };
}

impl Default for SceneScore {
    fn default() -> Self {
        Self::NONE
    }
}

// ---------------------------------------------------------------------------
// Attributes
// ---------------------------------------------------------------------------

/// The fourteen independent attributes of one photograph.
///
/// PHASE-07 section 2.1's list, as a bitfield. Independent sigmoids rather than a
/// softmax: a frame can be indoors *and* backlit *and* have a crowd, and a
/// mutually-exclusive encoding would force the model to choose.
///
/// A hand-rolled `u16` rather than `bitflags!`; see this module's header. Two bits
/// spare, which is deliberate: the fifteenth attribute somebody wants will not need a
/// migration.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(transparent)]
pub struct AttrFlags(u16);

impl AttrFlags {
    /// No attribute set. Not the same as "not measured"; see
    /// [`SceneResult::attributes_measured`].
    pub const EMPTY: Self = Self(0);

    /// Shot indoors. The one attribute whose absence is meaningful - clear means
    /// outdoors, not unknown.
    pub const INDOOR: Self = Self(1 << 0);
    /// The flash fired.
    pub const FLASH: Self = Self(1 << 1);
    /// Two or more illuminants of different colour temperature.
    pub const MIXED_LIGHT: Self = Self(1 << 2);
    /// The main subject is lit from behind.
    pub const BACKLIT: Self = Self(1 << 3);
    /// Shot after dark.
    pub const NIGHT: Self = Self(1 << 4);
    /// Tungsten-dominant illumination.
    pub const TUNGSTEN: Self = Self(1 << 5);
    /// Many people, most of them not the subject.
    pub const CROWD: Self = Self(1 << 6);
    /// A stage, an altar, a mandap - a raised or framed focal area.
    pub const STAGE: Self = Self(1 << 7);
    /// A car, a carriage, a horse - a vehicle is in the frame.
    pub const VEHICLE: Self = Self(1 << 8);
    /// Food or drink is the subject or a major element.
    pub const FOOD: Self = Self(1 << 9);
    /// Decor: flowers, drapes, signage, table settings.
    pub const DECOR: Self = Self(1 << 10);
    /// Children are present.
    pub const KIDS: Self = Self(1 << 11);
    /// Animals are present.
    pub const PETS: Self = Self(1 << 12);
    /// Visible rain or wet ground.
    pub const RAIN: Self = Self(1 << 13);

    /// Every attribute, in the order PHASE-07 section 2.1 writes them.
    ///
    /// This order is the attribute head's output order, for the same reason
    /// [`SceneId::ALL`] is the scene head's.
    pub const ALL: [Self; 14] = [
        Self::INDOOR,
        Self::FLASH,
        Self::MIXED_LIGHT,
        Self::BACKLIT,
        Self::NIGHT,
        Self::TUNGSTEN,
        Self::CROWD,
        Self::STAGE,
        Self::VEHICLE,
        Self::FOOD,
        Self::DECOR,
        Self::KIDS,
        Self::PETS,
        Self::RAIN,
    ];

    /// How many attributes the head emits.
    pub const COUNT: usize = 14;

    /// Wrap a raw bit pattern, dropping the two spare bits.
    #[must_use]
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits & 0x3FFF)
    }

    /// The raw bit pattern, as stored in `image_scenes.attrs`.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// True when every bit of `other` is set here.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Both sets of bits.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Set or clear one attribute.
    #[must_use]
    pub const fn with(self, other: Self, on: bool) -> Self {
        if on {
            Self(self.0 | other.0)
        } else {
            Self(self.0 & !other.0)
        }
    }

    /// True when nothing is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Stable text for one single-bit attribute, or `"unknown"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self.0 {
            1 => "indoor",
            2 => "flash",
            4 => "mixed_light",
            8 => "backlit",
            16 => "night",
            32 => "tungsten",
            64 => "crowd",
            128 => "stage",
            256 => "vehicle",
            512 => "food",
            1024 => "decor",
            2048 => "kids",
            4096 => "pets",
            8192 => "rain",
            _ => "unknown",
        }
    }

    /// The names of every set attribute, in [`AttrFlags::ALL`] order.
    #[must_use]
    pub fn names(self) -> Vec<&'static str> {
        Self::ALL
            .into_iter()
            .filter(|flag| self.contains(*flag))
            .map(Self::name)
            .collect()
    }
}

impl fmt::Display for AttrFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = self.names();
        if names.is_empty() {
            f.write_str("none")
        } else {
            f.write_str(&names.join("+"))
        }
    }
}

// ---------------------------------------------------------------------------
// Rituals
// ---------------------------------------------------------------------------

/// A tradition-specific rite, as an id authored in a taxonomy file.
///
/// Rituals live in `crates/aura-brain-wedding/config/rituals/*.toml` rather than in
/// this enum, because section 12's first failure mode is cultural blind spots and a
/// tradition that needs a rite this product has never heard of must be addable by
/// editing a file. See ADR-0015 section 3.
///
/// The id is **authored in the file**, never derived from file order: deriving it
/// would mean that inserting one rite renumbered every rite after it and silently
/// relabelled every stored row. The catalog stores the slug, not this number, so a
/// catalog stays readable when a taxonomy file is edited.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(transparent)]
pub struct RitualId(u16);

impl RitualId {
    /// The reserved id meaning "no ritual, and that is a decision".
    pub const NONE: Self = Self(0);

    /// Wrap an authored id.
    #[must_use]
    pub const fn new(id: u16) -> Self {
        Self(id)
    }

    /// The authored id.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// True when this is [`RitualId::NONE`].
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for RitualId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ritual#{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Chapters
// ---------------------------------------------------------------------------

/// One chapter of the wedding's story.
///
/// The phase card's ordered list - Getting Ready, Details, Ceremony, Rituals,
/// Portraits, Reception, Dance, Exit - plus [`ChapterId::Other`], which section 6.4
/// requires: "unknown or genuinely novel events map to `other` with a description
/// rather than a wrong confident label".
///
/// Closed, and in code rather than config, because the HMM transition matrix is
/// indexed by it. A transition matrix whose dimensions are read from a file at runtime
/// is a matrix that can be the wrong size.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ChapterId {
    /// Preparation, both parties.
    GettingReady,
    /// Objects and the empty venue.
    Details,
    /// The ceremony.
    Ceremony,
    /// Named rites inside or around the ceremony.
    Rituals,
    /// Posed photography.
    Portraits,
    /// The meal, the speeches, the cake.
    Reception,
    /// Dancing.
    Dance,
    /// The departure.
    Exit,
    /// Something real that the vocabulary does not name. The default.
    #[default]
    Other,
}

impl ChapterId {
    /// Every chapter, in wedding order. [`ChapterId::Other`] is last and is not part of
    /// the order.
    pub const ALL: [Self; 9] = [
        Self::GettingReady,
        Self::Details,
        Self::Ceremony,
        Self::Rituals,
        Self::Portraits,
        Self::Reception,
        Self::Dance,
        Self::Exit,
        Self::Other,
    ];

    /// How many chapters the transition matrix is square in.
    pub const COUNT: usize = 9;

    /// Stable text for the `segments.chapter` column and the IPC surface.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GettingReady => "getting_ready",
            Self::Details => "details",
            Self::Ceremony => "ceremony",
            Self::Rituals => "rituals",
            Self::Portraits => "portraits",
            Self::Reception => "reception",
            Self::Dance => "dance",
            Self::Exit => "exit",
            Self::Other => "other",
        }
    }

    /// The photographer-facing name, for the chapter strip.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::GettingReady => "Getting Ready",
            Self::Details => "Details",
            Self::Ceremony => "Ceremony",
            Self::Rituals => "Rituals",
            Self::Portraits => "Portraits",
            Self::Reception => "Reception",
            Self::Dance => "Dance",
            Self::Exit => "Exit",
            Self::Other => "Other",
        }
    }

    /// Parse the stored text; an unrecognised value is [`ChapterId::Other`].
    #[must_use]
    pub fn from_str_or_other(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|chapter| chapter.as_str() == text)
            .unwrap_or(Self::Other)
    }

    /// Position in [`ChapterId::ALL`], which is the transition matrix's index.
    #[must_use]
    pub fn as_index(self) -> usize {
        Self::ALL
            .into_iter()
            .position(|chapter| chapter == self)
            .unwrap_or(Self::COUNT - 1)
    }

    /// The chapter at one transition-matrix index, or [`ChapterId::Other`].
    #[must_use]
    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(Self::Other)
    }
}

impl fmt::Display for ChapterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// The frozen results
// ---------------------------------------------------------------------------

/// Where a label came from.
///
/// Ordered by authority, weakest first, so `max` is "the strongest thing that has said
/// anything about this frame". A [`Source::UserOverride`] is never replaced by
/// automation; that is invariant 1's user-override rule applied to a scene.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// The local multi-head classifier. The default and the overwhelming majority.
    #[default]
    Local,
    /// A cloud `SegmentNaming` answer, merged into the local one.
    Cloud,
    /// The photographer said so.
    UserOverride,
}

impl Source {
    /// Stable text for the `image_scenes.source` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Cloud => "cloud",
            Self::UserOverride => "user",
        }
    }

    /// Parse the stored text; an unrecognised value is [`Source::Local`].
    #[must_use]
    pub fn from_str_or_local(text: &str) -> Self {
        match text {
            "cloud" => Self::Cloud,
            "user" => Self::UserOverride,
            _ => Self::Local,
        }
    }

    /// True when automation must not overwrite this.
    #[must_use]
    pub const fn is_locked(self) -> bool {
        matches!(self, Self::UserOverride)
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What one photograph is, as the classifier and the story see it.
///
/// PHASE-07 section 5's frozen shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SceneResult {
    /// The photograph.
    pub image_id: ImageId,
    /// The chosen class.
    pub scene: SceneId,
    /// Its posterior after smoothing, `0..1`.
    pub scene_conf: f32,
    /// The three strongest classes, strongest first. Padded with
    /// [`SceneScore::NONE`] when fewer than three were above the floor.
    pub top3: [SceneScore; 3],
    /// The fourteen attribute bits.
    pub attributes: AttrFlags,
    /// True when the attribute head actually ran.
    ///
    /// Not in the phase document's shape, and load-bearing: [`AttrFlags::EMPTY`] means
    /// "outdoors, no flash, daylight, nobody around", which is a *description*, and
    /// "the head did not run" is not. Without this flag a caller cannot tell a quiet
    /// outdoor portrait from an unmeasured frame, and invariant 9 forbids that.
    pub attributes_measured: bool,
    /// The named rite, when the ritual head named one above its abstention threshold.
    pub ritual: Option<RitualId>,
    /// The rite's confidence, `0..1`. Zero when `ritual` is `None`.
    pub ritual_conf: f32,
    /// Where the label came from.
    pub source: Source,
    /// Which classifier produced it.
    pub model_ver: u16,
}

impl SceneResult {
    /// An unlabelled frame. Never a failure; see [`SceneId::Unknown`].
    #[must_use]
    pub fn unknown(image_id: ImageId, model_ver: u16) -> Self {
        Self {
            image_id,
            scene: SceneId::Unknown,
            scene_conf: 0.0,
            top3: [SceneScore::NONE; 3],
            attributes: AttrFlags::EMPTY,
            attributes_measured: false,
            ritual: None,
            ritual_conf: 0.0,
            source: Source::Local,
            model_ver,
        }
    }

    /// True when the photographer has fixed this label.
    #[must_use]
    pub const fn is_locked(&self) -> bool {
        self.source.is_locked()
    }
}

/// One chapter of the story, over a contiguous run of the timeline.
///
/// PHASE-07 section 5's frozen shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Segment {
    /// The segment.
    pub id: SegmentId,
    /// Which chapter it is.
    pub chapter: ChapterId,
    /// First frame's timeline time.
    pub start_ts: Timestamp,
    /// Last frame's timeline time. Inclusive.
    pub end_ts: Timestamp,
    /// The scene most of its frames are.
    pub dominant_scene: SceneId,
    /// How sure the chapter label is, `0..1`.
    pub confidence: f32,
    /// The frame that represents the chapter in the strip.
    pub key_frame: ImageId,
    /// Frames in the segment.
    pub image_count: u32,
    /// Why this chapter, at most [`Segment::MAX_REASONS`] of them. Invariant 2.
    pub reasons: Vec<String>,
    /// True when the photographer renamed, split, merged or moved this segment.
    pub user_locked: bool,
}

impl Segment {
    /// The most reasons one segment carries. See this module's header.
    pub const MAX_REASONS: usize = 6;

    /// How long the chapter lasted, in milliseconds.
    #[must_use]
    pub const fn duration_ms(&self) -> i64 {
        self.end_ts.saturating_sub(self.start_ts)
    }

    /// True when this chapter is uncertain enough to be worth asking about.
    ///
    /// PHASE-07 section 6.4's 0.75. The threshold is [`NEEDS_REVIEW_BELOW`] and it is
    /// *the same number* as `aura_cloud::tasks::ASK_BELOW_CONFIDENCE`, because "flag it
    /// for the photographer" and "ask a model" are the same question about the same
    /// segment. A test asserts the two agree.
    #[must_use]
    pub fn needs_review(&self) -> bool {
        !self.user_locked && self.confidence < NEEDS_REVIEW_BELOW
    }
}

/// Below this confidence a chapter is flagged for review rather than presented as
/// fact. PHASE-07 section 6.4.
pub const NEEDS_REVIEW_BELOW: f32 = 0.75;

/// How a chapter should be graded.
///
/// Phases 15 to 17 translate this into tone and colour targets. It is the mechanism
/// behind "the ceremony looks like a ceremony and the dance floor looks like a party",
/// and it is a five-valued enum rather than a set of numbers because it is a
/// *direction*, not a parameter: the numbers belong to the phase that grades.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum EditIntent {
    /// Light, low contrast, lifted shadows.
    Airy,
    /// No opinion. The default.
    #[default]
    Neutral,
    /// Warmer white balance, richer skin.
    Warm,
    /// Deeper shadows, lower key.
    Moody,
    /// High contrast, saturated.
    Punchy,
}

impl EditIntent {
    /// Stable text for `scene_profiles.editing_intent`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Airy => "airy",
            Self::Neutral => "neutral",
            Self::Warm => "warm",
            Self::Moody => "moody",
            Self::Punchy => "punchy",
        }
    }

    /// Parse the stored text; an unrecognised value is [`EditIntent::Neutral`].
    #[must_use]
    pub fn from_str_or_neutral(text: &str) -> Self {
        match text {
            "airy" => Self::Airy,
            "warm" => Self::Warm,
            "moody" => Self::Moody,
            "punchy" => Self::Punchy,
            _ => Self::Neutral,
        }
    }
}

impl fmt::Display for EditIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The tolerances and weights one scene is judged by.
///
/// PHASE-07 section 5's frozen shape, and **the single most consequential type this
/// phase adds**. Invariant 7 says no threshold is global; this is where that stops
/// being a slogan. A dark dance frame and a formal family portrait are judged by
/// different rows of the same table from this phase onward.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SceneProfile {
    /// Which scene this describes.
    pub scene: SceneId,
    /// Expected survival ratio, min and max. Phase 12 culls towards this band.
    pub keeper_rate: (f32, f32),
    /// Scene-relative noise tolerance, `0..1`. Higher means noisier is acceptable.
    pub max_acceptable_noise: f32,
    /// Scene-relative blur tolerance, `0..1`. Higher means softer is acceptable.
    pub max_acceptable_blur: f32,
    /// How much subject sharpness dominates the technical score, `0..1`.
    pub subject_focus_weight: f32,
    /// How much expression matters, `0..1`.
    pub emotion_weight: f32,
    /// How much framing matters, `0..1`.
    pub composition_weight: f32,
    /// The grading direction for phases 15 to 17.
    pub editing_intent: EditIntent,
    /// True when phase 12's coverage guarantee applies to this scene.
    pub must_cover: bool,
}

impl SceneProfile {
    /// The profile a scene gets when the registry has no row for it.
    ///
    /// Deliberately unopinionated, and deliberately **not** a refusal: a scene with no
    /// profile must still be gradeable, or one missing config row takes a wedding out
    /// of the product. The registry logs `AURA-ML-5023` when it substitutes this.
    #[must_use]
    pub const fn neutral(scene: SceneId) -> Self {
        Self {
            scene,
            keeper_rate: (0.20, 0.60),
            max_acceptable_noise: 0.50,
            max_acceptable_blur: 0.35,
            // Summing to exactly 1.0, unlike several shipped profiles. This is the
            // *substitution* for a missing row, so it must not be systematically
            // weaker than a real profile: a consuming phase that did not normalise by
            // the weight sum would score every unprofiled scene below every profiled
            // one, which would look like "the product dislikes this kind of
            // photograph" rather than like a missing config row.
            subject_focus_weight: 0.40,
            emotion_weight: 0.35,
            composition_weight: 0.25,
            editing_intent: EditIntent::Neutral,
            must_cover: false,
        }
    }

    /// The midpoint of the keeper band.
    #[must_use]
    pub fn keeper_target(&self) -> f32 {
        f32::midpoint(self.keeper_rate.0, self.keeper_rate.1)
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The wedding's story, as one answer.
///
/// Returned once per project and cached by the caller, exactly as
/// [`crate::contract::people::SubjectHierarchy`] is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StoryOutline {
    /// Ordered chapters, earliest first.
    pub segments: Vec<Segment>,
    /// Fraction of the project's photographs that carry a scene label, `0..1`.
    ///
    /// Phase 05's rule, inherited twice now: report coverage when you report a result.
    /// A story drawn over a 40 %-classified wedding is a story about 40 % of a wedding.
    pub coverage: f32,
    /// Chapters below [`NEEDS_REVIEW_BELOW`] that the photographer should look at.
    pub needs_review: Vec<SegmentId>,
    /// Which classifier produced the labels underneath.
    pub scene_ver: u16,
    /// Which taxonomy named the rituals.
    pub taxonomy_ver: u16,
}

impl StoryOutline {
    /// How many chapters the story has.
    #[must_use]
    pub fn chapter_count(&self) -> usize {
        self.segments.len()
    }

    /// The chapter containing a moment, if any.
    #[must_use]
    pub fn at(&self, when: Timestamp) -> Option<&Segment> {
        self.segments
            .iter()
            .find(|segment| segment.start_ts <= when && when <= segment.end_ts)
    }

    /// Frames per chapter, in wedding order.
    #[must_use]
    pub fn counts(&self) -> BTreeMap<ChapterId, u32> {
        let mut out: BTreeMap<ChapterId, u32> = BTreeMap::new();
        for segment in &self.segments {
            *out.entry(segment.chapter).or_insert(0) += segment.image_count;
        }
        out
    }
}

/// The one way to ask what a photograph is of, and how the day was structured.
///
/// Frozen. Implemented by `aura_brain_wedding::story::Story`, and the only route from
/// any later phase to a scene label or a chapter - which means the profile registry,
/// the user-override lock and the version checks are enforced in one place rather than
/// trusted in ten.
///
/// The same rule phase 05 wrote for `SimilarityIndex` and phase 06 for `PeopleService`:
/// **no phase may keep its own scene classifier or its own idea of where the ceremony
/// was.** A second answer to "is this the ceremony" is two thresholds that disagree.
pub trait StoryService: Send + Sync + fmt::Debug {
    /// The ordered story of one wedding.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the segments cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<StoryOutline>;

    /// What one photograph is of, or `None` when it has not been classified.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the scene row cannot be read.
    fn scene(&self, image: ImageId) -> AuraResult<Option<SceneResult>>;

    /// Which chapter a photograph belongs to, or `None`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the segments cannot be read.
    fn segment_of(&self, image: ImageId) -> AuraResult<Option<Segment>>;

    /// The tolerances and weights one scene is judged by.
    ///
    /// Infallible: a missing row yields [`SceneProfile::neutral`] and a logged
    /// `AURA-ML-5023`, because a scene that cannot be graded takes a wedding out of the
    /// product and a neutral grade does not.
    fn profile(&self, scene: SceneId) -> SceneProfile;

    /// Rename a chapter, and lock it against automation.
    ///
    /// Sets `user_locked`. PHASE-07 section 6.4: "anything the user touches is
    /// `user_locked` and re-analysis never overwrites it."
    ///
    /// # Errors
    ///
    /// `AURA-ML-5025` when the segment is unknown.
    fn set_chapter(
        &self,
        segment: SegmentId,
        chapter: ChapterId,
        label: Option<&str>,
    ) -> Result<(), AuraError>;

    /// Move the boundary between `segment` and the one after it.
    ///
    /// Locks both. A boundary is shared, so moving it is one decision about two
    /// chapters and locking only one of them would let the next re-analysis move it
    /// back from the other side.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5025` when the segment is unknown, when it is the last one, or when the
    /// new boundary would empty either side.
    fn move_boundary(&self, segment: SegmentId, new_end: Timestamp) -> Result<(), AuraError>;

    /// Split a chapter in two at a photograph, returning the new later half.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5025` when the segment is unknown, when the photograph is not in it, or
    /// when the split would leave a side empty.
    fn split_segment(&self, segment: SegmentId, at: ImageId) -> Result<SegmentId, AuraError>;

    /// Fold two adjacent chapters into one, returning the survivor.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5025` when either id is unknown, when they belong to different
    /// projects, or when they are not adjacent.
    fn merge_segments(&self, a: SegmentId, b: SegmentId) -> Result<SegmentId, AuraError>;
}
