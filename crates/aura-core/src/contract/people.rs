//! FROZEN CONTRACT. The subject hierarchy every later phase ranks by.
//!
//! PHASE-06 section 5 freezes these shapes before any face exists, and the reason
//! this file is in `aura-core` rather than in `aura-people` is the list of phases
//! that consume it: 09 (blink and integrity), 10 (emotion), 11 (composition), 12
//! (culling and coverage), 13 (the explainability ledger), 18 to 22 (masks and
//! retouch), 25 (gallery intelligence), 27 (the QC agent) and 29 (curation). Every
//! one of them needs the vocabulary - "is this the bride" - and none of them needs
//! the biometric store. Putting [`Role`] here means a phase can rank subjects
//! without linking the crate that holds the templates.
//!
//! `aura-core` still depends on no other workspace crate; a test asserts it.
//!
//! ## Three spellings differ from the phase document
//!
//! All three are recorded in
//! `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md` section 2.
//!
//! * **`hierarchy` and `subjects` return `AuraResult`.** The phase document has
//!   them return the value directly. Both read the catalog, and a catalog read can
//!   fail; returning a default `SubjectHierarchy` on `AURA-DB-3006` would make "this
//!   wedding has no identified people" and "the database could not be read"
//!   indistinguishable, which is exactly the silent failure invariant 9 forbids.
//! * **`HashMap` is `BTreeMap`.** A hash map's constructor is banned by
//!   `scripts/check-banned.sh` for a reason that applies here more than anywhere:
//!   these weights are iterated when a prominence score is computed, and invariant 4
//!   requires that two machines produce the same recipe. A hash map's iteration
//!   order is not part of any contract.
//! * **`ImageId` is `PhotoId`.** One type, aliased rather than duplicated, exactly
//!   as `aura_index` does it - so no conversion between a similarity query and a
//!   subject query can ever disagree.
//!
//! Changing anything in this file requires an ADR.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contract::error::{AuraError, AuraResult};
use crate::contract::ids::{FaceId, IdentityId, PhotoId, ProjectId};

/// The identity of a photograph in a subject query.
///
/// PHASE-06 names this `ImageId`; the catalog calls it a `PhotoId`. One type.
pub type ImageId = PhotoId;

/// What somebody is at this wedding.
///
/// The list is closed, and it is the list from section 2.1. Two things about it are
/// product decisions rather than modelling ones, and both are load-bearing.
///
/// **[`Role::Couple`] is a real role, not a placeholder for two.** Section 6.3 is
/// explicit that the pair is what the evidence identifies, that the couple may be
/// same-sex, and that `bride` and `groom` are *suggestions until the user confirms*.
/// So the automatic path assigns `Couple` to both members and the photographer
/// promotes one to [`Role::Bride`] or [`Role::Groom`] with one click. Nothing in
/// this product ever infers which of two people is the bride.
///
/// **[`Role::Vendor`] exists so that a caterer is not a guest.** A second
/// photographer, a planner or a DJ appears in more frames than most family members
/// and in almost none of them as the subject. Without a role for that pattern, the
/// frequency evidence in section 6.3 would promote the DJ to close family.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Confirmed by the photographer as one half of the couple.
    Bride,
    /// Confirmed by the photographer as the other half.
    Groom,
    /// Identified by evidence as one of the two people this wedding is about.
    Couple,
    /// Parents, siblings, grandparents - high co-occurrence with the couple in
    /// family scenes plus presence during getting ready.
    FamilyClose,
    /// Aunts, uncles, cousins.
    FamilyExtended,
    /// Not family, but photographed as a subject: the officiant, a best friend, a
    /// guest of honour.
    Vip,
    /// Everybody else who was there.
    Guest,
    /// Present all day, almost never the subject: planners, caterers, the band,
    /// the second photographer.
    Vendor,
    /// A child. Kept separate from `guest` because coverage rules, culling weights
    /// and retouch policy all differ, and because "did we photograph the children"
    /// is a question every set of parents asks.
    Child,
    /// Not enough evidence yet. The default, and never a failure.
    #[default]
    Unknown,
}

impl Role {
    /// Stable text for the `identities.role` column and the IPC surface.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bride => "bride",
            Self::Groom => "groom",
            Self::Couple => "couple",
            Self::FamilyClose => "family_close",
            Self::FamilyExtended => "family_extended",
            Self::Vip => "vip",
            Self::Guest => "guest",
            Self::Vendor => "vendor",
            Self::Child => "child",
            Self::Unknown => "unknown",
        }
    }

    /// Parse the stored text. An unknown value is [`Role::Unknown`] rather than an
    /// error: a catalog written by a newer build must still open, and "a role this
    /// build does not know" and "no role" have the same consequence for ranking.
    #[must_use]
    pub fn from_str_or_unknown(text: &str) -> Self {
        match text {
            "bride" => Self::Bride,
            "groom" => Self::Groom,
            "couple" => Self::Couple,
            "family_close" => Self::FamilyClose,
            "family_extended" => Self::FamilyExtended,
            "vip" => Self::Vip,
            "guest" => Self::Guest,
            "vendor" => Self::Vendor,
            "child" => Self::Child,
            _ => Self::Unknown,
        }
    }

    /// Every role, in ranking order, most important first.
    ///
    /// The order is the product's opinion about a wedding and it is used by the
    /// default weight table in `aura_vision::face::prominence`. It is *not* used as
    /// a tie-break anywhere a photographer's own importance setting applies.
    pub const ALL: [Self; 10] = [
        Self::Bride,
        Self::Groom,
        Self::Couple,
        Self::FamilyClose,
        Self::FamilyExtended,
        Self::Vip,
        Self::Child,
        Self::Guest,
        Self::Vendor,
        Self::Unknown,
    ];

    /// True when this role is one half of the couple, however it was arrived at.
    ///
    /// The one predicate every later phase actually wants. Written once here so that
    /// nine phases do not each write `role == Bride || role == Groom || role ==
    /// Couple` and one of them forget a case.
    #[must_use]
    pub const fn is_couple(self) -> bool {
        matches!(self, Self::Bride | Self::Groom | Self::Couple)
    }

    /// True when this role is family of any degree.
    #[must_use]
    pub const fn is_family(self) -> bool {
        matches!(self, Self::FamilyClose | Self::FamilyExtended)
    }

    /// True when a photographer, rather than the evidence, must have chosen this.
    ///
    /// `bride` and `groom` are only ever set by a person; see the note on
    /// [`Role::Couple`].
    #[must_use]
    pub const fn requires_confirmation(self) -> bool {
        matches!(self, Self::Bride | Self::Groom)
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One face, as a subject reference.
///
/// Deliberately small: a box, a quality, an identity and the numbers a ranking
/// decision needs. Not a template, not a crop, not a landmark array. A phase that
/// ranks subjects has no business holding biometric data, and this is the type that
/// makes that structural rather than advisory.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FaceRef {
    /// The face.
    pub face_id: FaceId,
    /// Who it was assigned to, when it was assigned to anybody.
    pub identity_id: Option<IdentityId>,
    /// Fraction of the frame the face covers, `0..1`.
    pub area_frac: f32,
    /// How central it is, `0..1`.
    pub centrality: f32,
    /// Local sharpness of the face, `0..1`. Not the frame's sharpness: a frame can
    /// be tack sharp on the cake and soft on the bride.
    pub sharpness: f32,
    /// Fused usability from the quality gate, `0..1`.
    pub quality: f32,
    /// True when this face was good enough to vote on identity.
    pub votes: bool,
}

/// Who matters at this wedding, and by how much.
///
/// Returned once per project and cached by the caller. Every downstream weight in
/// the product is a lookup in [`SubjectHierarchy::weights`], which is why that map
/// is total over the project's identities rather than sparse: a missing key would
/// make a caller choose a default, and nine callers would choose four defaults.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubjectHierarchy {
    /// The couple. One or two identities; empty when the evidence was not
    /// sufficient and the photographer has not said.
    pub primary: Vec<IdentityId>,
    /// Close family and anybody marked important.
    pub secondary: Vec<IdentityId>,
    /// Every identity in the project, with its weight in `0..1`.
    pub weights: BTreeMap<IdentityId, f32>,
    /// True when the couple was identified by evidence rather than confirmed by a
    /// person. A caller that is about to make an irreversible decision - a delivery,
    /// a gallery, an automatic cull - is expected to ask.
    pub couple_unconfirmed: bool,
    /// Fraction of the project's photographs that have been scanned for faces,
    /// `0..1`.
    ///
    /// Phase 05's rule, inherited: report coverage when you report a result. A
    /// hierarchy derived from a 40 %-scanned wedding is a hierarchy of 40 % of a
    /// wedding, and a caller that does not know that will present it as fact.
    pub coverage: f32,
}

impl SubjectHierarchy {
    /// The weight of one identity, or zero when the project has never seen it.
    #[must_use]
    pub fn weight_of(&self, identity: IdentityId) -> f32 {
        self.weights.get(&identity).copied().unwrap_or(0.0)
    }

    /// True when this identity is one of the two the wedding is about.
    #[must_use]
    pub fn is_primary(&self, identity: IdentityId) -> bool {
        self.primary.contains(&identity)
    }
}

/// Who is in one photograph, and how much of it is about them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImageSubjects {
    /// Every face in the frame, strongest detection first.
    pub faces: Vec<FaceRef>,
    /// The identity this photograph is most about, when there is one.
    pub dominant: Option<IdentityId>,
    /// Per-identity prominence within this frame, `0..1`.
    pub prominence: BTreeMap<IdentityId, f32>,
    /// The prominence-weighted sharpness of the faces present, `0..1`.
    ///
    /// **The single number phases 09 and 12 use instead of global sharpness.** A
    /// frame that is razor sharp on the centrepiece and soft on the bride has a high
    /// global sharpness and a low subject focus score, and only the second one
    /// describes whether the photograph is usable.
    pub subject_focus_score: f32,
    /// People in the frame, including the ones whose faces are not visible.
    pub people_count: u32,
    /// Which version of the weight table produced these numbers.
    pub weights_ver: u16,
}

/// The one way to ask who is in a photograph.
///
/// Frozen. Implemented by `aura_people::People`, and the only route from any later
/// phase to the biometric store - which means the store's encryption, its
/// project scoping and its erasure semantics are enforced in one place rather than
/// trusted in nine.
pub trait PeopleService: Send + Sync + fmt::Debug {
    /// Who matters at this wedding.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the identities cannot be read, and `AURA-SEC-9001` when
    /// the project's biometric key is unavailable.
    fn hierarchy(&self, project: ProjectId) -> AuraResult<SubjectHierarchy>;

    /// Who is in one photograph.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the faces cannot be read.
    fn subjects(&self, image: ImageId) -> AuraResult<ImageSubjects>;

    /// Fold identity `b` into identity `a`, returning the survivor.
    ///
    /// Recorded in `identity_links` so it can be undone and so it survives a
    /// re-analysis.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9005` when either id is unknown or the two belong to different
    /// projects - which is the cross-project refusal section 6.5 requires to be
    /// impossible rather than merely unlikely.
    fn merge(&self, a: IdentityId, b: IdentityId) -> Result<IdentityId, AuraError>;

    /// Move `faces` out of `id` into a new identity, returning the new one.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9005` when the identity is unknown, when the faces do not all
    /// belong to it, or when the split would leave nothing behind.
    fn split(&self, id: IdentityId, faces: &[FaceId]) -> Result<IdentityId, AuraError>;

    /// Set a role, and lock it against automation.
    ///
    /// Sets `user_locked`. A photographer who says "this is the bride" has ended the
    /// argument, and no later re-analysis, cloud hint or model update may overwrite
    /// it.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9005` when the identity is unknown.
    fn set_role(&self, id: IdentityId, role: Role) -> Result<(), AuraError>;
}
