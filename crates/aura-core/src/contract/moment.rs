//! FROZEN CONTRACT. The wedding's moments: what the photographer shot once, and
//! the near-identical frames inside it.
//!
//! PHASE-08 section 5 freezes these shapes before any grouping engine exists, and the
//! reason this file is in `aura-core` rather than in `aura-brain-wedding` is the same
//! list of phases that made [`crate::contract::scene`] live here: 09 (technical
//! quality), 10 (emotion), 11 (composition), 12 (culling and coverage), 13 (the
//! explainability ledger), 27 (the QC agent) and 29 (curation).
//!
//! Every one of them operates on **moments** rather than on files. Phase 08's mission
//! is "turn 3,000 loose files into roughly 700-1,100 moments", and phase 12's coverage
//! guarantees are written against moments, not frames: rejecting a burst is a moment
//! lost, whereas rejecting individual frames is tidying. None of those phases needs the
//! cadence estimator, the similarity graph or the union-find.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## Six spellings differ from the phase document
//!
//! All six are recorded in
//! `docs/adr/ADR-0017-burst-grouping-and-duplicate-policy.md` section 2.
//!
//! * **`ImageId` is `PhotoId`.** One type, aliased rather than duplicated, exactly as
//!   [`crate::contract::scene`] and [`crate::contract::people`] already do it.
//! * **`Timestamp` is the scene contract's `Timestamp`** - `i64` milliseconds since
//!   the Unix epoch - re-exported rather than redeclared. A moment's start and a
//!   chapter's start are compared directly by phase 12, and two time representations
//!   would let them disagree.
//! * **[`CameraId`] is the catalog's `camera_id` text, not `aura_index`'s dense
//!   `CameraId(u32)`.** The index's handle is a filter key that is only meaningful
//!   inside one built graph; a moment is a durable row that outlives the graph. The
//!   same distinction `aura_brain_wedding::index_handle` draws for scenes.
//! * **[`Moment`] carries `confidence` and `reasons`.** Section 5's shape has neither,
//!   and invariant 2 requires both of every AI decision. Grouping six frames into one
//!   moment is a decision a photographer will disagree with, so it has to be able to
//!   say why. Capped at [`Moment::MAX_REASONS`], exactly as `Segment::reasons` is.
//! * **[`DuplicateSet`] carries `reasons` beside the `confidence` section 5 gives it**,
//!   for the same reason: "these two frames are the same photograph" is the single
//!   most consequential claim this phase makes, and it is reviewed side by side in the
//!   UI where the evidence has to be readable.
//! * **[`MomentOutline`] is not in section 5 at all.** It is the coverage carrier, and
//!   it exists because phase 05's rule - report coverage when you report a result - has
//!   now been inherited three times. A moment list drawn over a 40 %-embedded wedding
//!   is a list of 40 % of a wedding.
//!
//! ## What is deliberately not here
//!
//! **Choosing the winner of a burst.** Section 2.2 puts that in phase 12, and
//! [`DuplicateSet::keep_hint`] is spelled *hint* throughout for that reason: it is the
//! technically strongest frame this phase can identify, offered as a starting point,
//! and it is not a decision about a photograph's fate. Nothing in this contract can
//! reject, delete or rank a frame.
//!
//! Changing anything in this file requires an ADR.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{IdentityId, MomentId, ProjectId, SegmentId};

pub use crate::contract::scene::{ImageId, Timestamp};

// ---------------------------------------------------------------------------
// Cameras
// ---------------------------------------------------------------------------

/// Which camera body took a frame, as the catalog identifies it.
///
/// The catalog's `camera.camera_id`, which is `cam_` followed by 64 hexadecimal
/// characters derived from make, model and body serial. Not `aura_index`'s
/// `CameraId(u32)`: that handle is a position in one built graph's camera list, is
/// reassigned every time the graph is rebuilt, and would be meaningless in a stored
/// row. See ADR-0017 section 2.
///
/// [`CameraId::UNKNOWN`] is the empty string, which is what a photograph with no
/// readable body serial gets. It is a real value rather than an `Option` because a
/// wedding shot on one unidentified body must still group, and "the camera I cannot
/// name" behaves like a camera for every purpose in this phase.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct CameraId(String);

impl CameraId {
    /// The body whose serial could not be read.
    pub const UNKNOWN: &'static str = "";

    /// Wrap a catalog camera id.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The catalog id, as stored.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True when no body serial was readable.
    #[must_use]
    pub fn is_unknown(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for CameraId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            f.write_str("unknown-camera")
        } else {
            f.write_str(&self.0)
        }
    }
}

// ---------------------------------------------------------------------------
// Duplicates
// ---------------------------------------------------------------------------

/// How alike two frames in one moment are.
///
/// PHASE-08 section 2.1's three classes, and the distinction between the second and
/// the third is the whole point of the class: a `NearIdentical` pair is one photograph
/// that happens to exist twice, and a `Variant` pair is two photographs of one moment.
/// Phase 12 may deliver several variants of a moment and must never deliver two near
/// identicals.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateKind {
    /// The same file, imported twice. Identical content hash.
    ///
    /// Detected at ingest in phase 01 and reported here only for completeness -
    /// section 6.3's first line. A photographer who copied a card twice sees one
    /// explanation in one place rather than two.
    Identical,
    /// Not the same file, but the same photograph: nothing a client would ever choose
    /// between. Only one frame of such a set may reach a gallery.
    NearIdentical,
    /// The same moment with a meaningful difference - an expression, an eye, a
    /// reframing. Every frame stays eligible and phase 12 chooses. The default,
    /// because it is the class that claims the least.
    #[default]
    Variant,
}

impl DuplicateKind {
    /// Every class, strongest claim first.
    pub const ALL: [Self; 3] = [Self::Identical, Self::NearIdentical, Self::Variant];

    /// Stable text for the `duplicates.kind` column and the IPC surface.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Identical => "identical",
            Self::NearIdentical => "near_identical",
            Self::Variant => "variant",
        }
    }

    /// Parse the stored text.
    ///
    /// An unrecognised value is [`DuplicateKind::Variant`], for the reason
    /// `SceneId::from_str_or_unknown` gives: a catalog written by a newer build must
    /// still open, and the safe direction to fail in is the class that claims least.
    /// Reading an unknown class as `NearIdentical` would suppress a frame.
    #[must_use]
    pub fn from_str_or_variant(text: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == text)
            .unwrap_or(Self::Variant)
    }

    /// True when at most one frame of the set may reach a gallery.
    ///
    /// The one behavioural question a later phase asks of this enum, answered here so
    /// that phases 12, 25 and 29 do not each write the same `matches!`.
    #[must_use]
    pub const fn caps_gallery_to_one(self) -> bool {
        matches!(self, Self::Identical | Self::NearIdentical)
    }
}

impl fmt::Display for DuplicateKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A set of frames inside one moment that say the same thing.
///
/// PHASE-08 section 5's frozen shape, plus `reasons`; see this module's header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DuplicateSet {
    /// How alike they are.
    pub kind: DuplicateKind,
    /// The frames, in timeline order. Always at least two.
    pub image_ids: Vec<ImageId>,
    /// The technically strongest frame - **not the final decision**.
    ///
    /// Section 6.3: "`keep_hint` is provisional: the technically strongest frame by
    /// edge energy and subject focus, replaced by the real decision in Phase 12."
    /// Nothing in this phase acts on it and nothing in this phase may.
    pub keep_hint: ImageId,
    /// How sure the classification is, `0..1`.
    pub confidence: f32,
    /// Why these frames are the same photograph. Invariant 2.
    pub reasons: Vec<String>,
    /// True when the photographer picked the `keep_hint` by hand.
    ///
    /// Set by the duplicate review panel's "keep this one". It changes which frame
    /// phase 12 starts from and it never changes whether phase 12 may look at the
    /// others.
    pub user_chosen: bool,
}

impl DuplicateSet {
    /// The most reasons one set carries.
    pub const MAX_REASONS: usize = 4;

    /// How many frames are in the set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.image_ids.len()
    }

    /// True when the set has no frames. Never true for a stored set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.image_ids.is_empty()
    }

    /// Frames other than the hint, which are the ones a review panel shows beside it.
    #[must_use]
    pub fn others(&self) -> Vec<ImageId> {
        self.image_ids
            .iter()
            .copied()
            .filter(|id| *id != self.keep_hint)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Moments
// ---------------------------------------------------------------------------

/// One thing the photographer shot, however many times they pressed the shutter.
///
/// PHASE-08 section 5's frozen shape, plus `confidence` and `reasons`; see this
/// module's header.
///
/// **The two-tier structure is in the two fields.** `image_ids` is the moment - the
/// same subject and action - and `bursts` is the tighter sub-grouping inside it, which
/// is what a camera's drive mode produced. Fourteen frames of a bouquet toss are one
/// moment; the six that came off at 10 fps as it left her hand are one burst inside it.
///
/// `bursts` partitions `image_ids`: every frame appears in exactly one burst, and a
/// frame with no neighbours close enough to be a burst is a burst of one. A partition
/// rather than a subset, so that a caller iterating bursts never silently drops a
/// frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Moment {
    /// The moment.
    pub id: MomentId,
    /// The chapter it sits in. Phase 07's `Segment`.
    pub segment_id: SegmentId,
    /// Every frame, ordered by `timeline_ts`.
    pub image_ids: Vec<ImageId>,
    /// Tighter sub-groups. A partition of `image_ids`; see this type's header.
    pub bursts: Vec<Vec<ImageId>>,
    /// First frame's timeline time.
    pub start_ts: Timestamp,
    /// Last frame's timeline time. Inclusive.
    pub end_ts: Timestamp,
    /// Bodies that contributed, in catalog order. More than one means two
    /// photographers caught the same instant.
    pub cameras: Vec<CameraId>,
    /// Who is in it, from `PeopleService`'s assignment. Empty when no face pass has
    /// run, which is not the same as "nobody is in it".
    pub identities: Vec<IdentityId>,
    /// Mean pairwise embedding distance within the moment, normalised to `0..1`.
    ///
    /// Zero means the photographer bracketed a static subject and every frame says the
    /// same thing; one means the action evolved across the moment. Section 6.4: phase
    /// 12 allows a high-diversity moment more than one keeper and caps a low-diversity
    /// one at a single frame.
    pub diversity: f32,
    /// The sets inside this moment that constrain what may be delivered.
    ///
    /// `Identical` and `NearIdentical` only. A `Variant` set would say "all frames stay
    /// eligible and phase 12 chooses", which is a statement that nothing is
    /// constrained - and the alternatives it would list are already `bursts`. See
    /// ADR-0017 section 7; the class still exists as a classification outcome and as
    /// what the review panel shows.
    pub duplicate_sets: Vec<DuplicateSet>,
    /// True when the photographer split, merged or pinned this moment.
    ///
    /// Checked inside the statement that would overwrite it, exactly as
    /// `segments.user_locked` and `identities.user_locked` are. A photographer's
    /// grouping is unbeatable.
    pub user_locked: bool,
    /// How sure the grouping is, `0..1`. Invariant 2.
    pub confidence: f32,
    /// Why these frames are one moment, at most [`Moment::MAX_REASONS`]. Invariant 2.
    pub reasons: Vec<String>,
}

impl Moment {
    /// The most reasons one moment carries.
    pub const MAX_REASONS: usize = 6;

    /// How many frames the moment holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.image_ids.len()
    }

    /// True when the moment holds no frames. Never true for a stored moment.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.image_ids.is_empty()
    }

    /// How long the photographer spent on it, in milliseconds.
    #[must_use]
    pub const fn duration_ms(&self) -> i64 {
        self.end_ts.saturating_sub(self.start_ts)
    }

    /// True when more than one body contributed.
    ///
    /// The cross-camera case section 2.1 calls "critical for Phase 26": the same
    /// instant shot by two photographers is one moment with per-camera sub-groups.
    #[must_use]
    pub fn is_multi_camera(&self) -> bool {
        self.cameras.len() > 1
    }

    /// Frames that only one of them may reach a gallery.
    ///
    /// Flattened across every `Identical` and `NearIdentical` set, which is the
    /// question phase 12 actually asks. Variants are excluded by construction.
    #[must_use]
    pub fn suppressed_candidates(&self) -> Vec<ImageId> {
        self.duplicate_sets
            .iter()
            .filter(|set| set.kind.caps_gallery_to_one())
            .flat_map(DuplicateSet::others)
            .collect()
    }

    /// How many keepers phase 12 may take from this moment before it is arguing with
    /// the evidence.
    ///
    /// One for a bracketed static subject, more as the action evolves. **Advice, not
    /// a limit**: phase 12 owns the culling decision and this crate owns no part of
    /// it, exactly as phase 05 owns distances and phase 07 owns scene tolerances.
    /// The number is capped at three because a moment is one thing that happened.
    #[must_use]
    pub fn suggested_keepers(&self) -> u32 {
        if self.diversity >= 0.55 {
            3
        } else if self.diversity >= 0.25 {
            2
        } else {
            1
        }
    }
}

/// A project's moments, with the coverage the conclusion was drawn over.
///
/// Returned once per project and cached by the caller, exactly as `StoryOutline` and
/// `SubjectHierarchy` are.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MomentOutline {
    /// Every moment, earliest first.
    pub moments: Vec<Moment>,
    /// Fraction of the project's photographs that are in a moment, `0..1`.
    ///
    /// Phase 05's rule, inherited a fourth time. A frame with no embedding cannot be
    /// grouped, so a moment list over a half-embedded wedding describes half a wedding
    /// and this is how a caller finds out.
    pub coverage: f32,
    /// Moments the photographer has locked.
    pub locked: u32,
    /// Which embedding produced the distances underneath.
    pub embed_ver: u16,
    /// Which grouping implementation produced the moments.
    pub group_ver: u16,
    /// Which threshold table it read.
    pub profile_ver: u16,
}

impl MomentOutline {
    /// How many moments the wedding has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.moments.len()
    }

    /// True when nothing has been grouped.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.moments.is_empty()
    }

    /// Frames placed in a moment.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.moments.iter().map(Moment::len).sum()
    }

    /// Mean frames per moment. Zero for an empty outline.
    ///
    /// The headline number of the phase: section 0 wants 3,000 files to become roughly
    /// 700-1,100 moments, which is this figure between 2.7 and 4.3.
    ///
    /// Per-hundredth integer arithmetic then one lossless widening, which is the shape
    /// every ratio in this workspace uses. `aura-core` does not relax
    /// `clippy::cast_precision_loss` the way the ML crates do - it is the crate every
    /// other crate reads, and a silent `usize as f32` here would be a rounding decision
    /// nine callers inherited without knowing.
    #[must_use]
    pub fn mean_size(&self) -> f32 {
        if self.moments.is_empty() {
            return 0.0;
        }
        let hundredths = self
            .frames()
            .saturating_mul(100)
            .checked_div(self.moments.len())
            .unwrap_or(0);
        f32::from(u16::try_from(hundredths).unwrap_or(u16::MAX)) / 100.0
    }

    /// The moment containing a frame, if any.
    #[must_use]
    pub fn of_image(&self, image: ImageId) -> Option<&Moment> {
        self.moments
            .iter()
            .find(|moment| moment.image_ids.contains(&image))
    }

    /// Every duplicate set in the wedding, by class.
    ///
    /// The `duplicates.found` telemetry event's three counters, computed once.
    #[must_use]
    pub fn duplicate_counts(&self) -> [usize; 3] {
        let mut out = [0usize; 3];
        for set in self
            .moments
            .iter()
            .flat_map(|moment| &moment.duplicate_sets)
        {
            let slot = match set.kind {
                DuplicateKind::Identical => 0,
                DuplicateKind::NearIdentical => 1,
                DuplicateKind::Variant => 2,
            };
            if let Some(counter) = out.get_mut(slot) {
                *counter += 1;
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// The one way to ask what was shot once.
///
/// Frozen. Implemented by `aura_brain_wedding::moments::Moments`, and the only route
/// from any later phase to a moment, a burst or a duplicate class.
///
/// The same rule phase 05 wrote for `SimilarityIndex`, phase 06 for `PeopleService`
/// and phase 07 for `StoryService`, a fourth time and for the same reason: **no phase
/// may keep its own grouping.** Two answers to "are these the same shot" is two
/// culling decisions that disagree, and phase 12's coverage guarantee is written
/// against this one.
///
/// Not in PHASE-08 section 5, which freezes only the two data shapes. Added here and
/// recorded in ADR-0017 section 2, because seven later phases consume moments and a
/// contract with no entry point would make each of them find its own.
pub trait MomentService: Send + Sync + fmt::Debug {
    /// Every moment in one wedding, earliest first.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the moments cannot be read.
    fn outline(&self, project: ProjectId) -> AuraResult<MomentOutline>;

    /// The moment a photograph belongs to, or `None` when it has not been grouped.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the moment cannot be read.
    fn moment_of(&self, image: ImageId) -> AuraResult<Option<Moment>>;

    /// The duplicate sets inside one moment.
    ///
    /// A separate call from [`MomentService::moment_of`] because the review panel asks
    /// about one moment at a time and the grid asks about thousands without ever
    /// needing the sets.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the sets cannot be read.
    fn duplicates(&self, moment: MomentId) -> AuraResult<Vec<DuplicateSet>>;

    /// Break a moment in two at a photograph, returning the new later half.
    ///
    /// Locks **both** halves. A split is the photographer saying two things were
    /// grouped that should not have been, and re-analysis must not undo either side of
    /// that statement.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5029` when the moment is unknown, when the photograph is not in it, or
    /// when the split would leave a side empty.
    fn split(&self, moment: MomentId, at: ImageId) -> Result<MomentId, AuraError>;

    /// Fold two moments into one, returning the survivor.
    ///
    /// Locks the survivor. The two need not be adjacent - two photographers' takes on
    /// one kiss can be minutes apart on a mis-set clock - but they must be in the same
    /// project and the same chapter.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5029` when either id is unknown, when they belong to different
    /// projects, or when they belong to different chapters.
    fn merge(&self, a: MomentId, b: MomentId) -> Result<MomentId, AuraError>;

    /// Pin a moment against re-analysis without otherwise changing it.
    ///
    /// The photographer looked at the grouping and agreed with it. Unpinning is the
    /// same call with `locked = false`, which is what makes an accidental pin
    /// recoverable.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5029` when the moment is unknown.
    fn set_locked(&self, moment: MomentId, locked: bool) -> Result<(), AuraError>;

    /// Record which frame of a duplicate set the photographer would keep.
    ///
    /// Sets `user_chosen`. It moves the hint and it does not cull: phase 12 still
    /// decides, and section 2.2 keeps that boundary structural.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5029` when the moment is unknown or the photograph is not in the set.
    fn set_keep_hint(&self, moment: MomentId, keep: ImageId) -> Result<(), AuraError>;

    /// Undo the last grouping edit in a project, returning what it was.
    ///
    /// Section 13: "manual grouping edits are permanent and undoable". Permanent means
    /// a re-analysis will not revert them; undoable means the photographer can.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5029` when there is nothing to undo.
    fn undo(&self, project: ProjectId) -> Result<MomentEdit, AuraError>;
}

/// One grouping edit, as the undo journal records it.
///
/// The same shape phase 06's `identity_links` takes and for the same reason: an edit
/// that can be replayed onto a fresh grouping is an edit that survives a re-analysis,
/// and one that can be reversed is an edit a photographer can risk making.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MomentEdit {
    /// `split`, `merge`, `lock`, `unlock` or `keep_hint`.
    pub action: String,
    /// The moment the photographer acted on.
    pub moment: MomentId,
    /// The other moment, for a merge or a split.
    pub other: Option<MomentId>,
    /// The photograph, for a split or a keep-hint change.
    pub image: Option<ImageId>,
    /// How many frames the moment held at the time, for the `moments.user_edit`
    /// telemetry event. Never the frames themselves: a telemetry event carrying photo
    /// ids is a telemetry event carrying a shoot.
    pub moment_size: u32,
}
