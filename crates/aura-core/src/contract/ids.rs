//! FROZEN CONTRACT. Typed IDs. A `ProjectId` can never be passed where a `PhotoId`
//! is expected, which eliminates an entire class of catalog-corrupting bug.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident, $prefix:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// New random v7 ID. Time-ordered, so index locality is good and
            /// insertion order is stable within a run.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Wrap an existing UUID.
            #[must_use]
            pub const fn from_uuid(u: Uuid) -> Self {
                Self(u)
            }

            /// Borrow the inner UUID.
            #[must_use]
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }

            /// Canonical text form stored in `SQLite`: prefixed, lower case.
            #[must_use]
            pub fn to_db(&self) -> String {
                format!("{}_{}", $prefix, self.0.as_hyphenated())
            }

            /// Parse the canonical text form produced by `to_db`.
            ///
            /// # Errors
            ///
            /// Returns [`IdParseError`] when the prefix belongs to another id
            /// type or the remainder is not a UUID.
            pub fn from_db(s: &str) -> Result<Self, IdParseError> {
                let rest = s
                    .strip_prefix(concat!($prefix, "_"))
                    .ok_or(IdParseError::WrongPrefix { expected: $prefix })?;
                Uuid::parse_str(rest)
                    .map(Self)
                    .map_err(|_| IdParseError::NotAUuid)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.to_db())
            }
        }
    };
}

/// Why a typed id failed to parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IdParseError {
    /// The text carries a different id type's prefix.
    #[error("expected prefix {expected}_")]
    WrongPrefix {
        /// The prefix the target type requires.
        expected: &'static str,
    },
    /// The text after the prefix is not a UUID.
    #[error("not a valid uuid")]
    NotAUuid,
}

typed_id!(ProjectId, "prj");
typed_id!(PhotoId, "pht");
typed_id!(FileId, "fil");
typed_id!(RunId, "run");
typed_id!(ImportId, "imp");

// PHASE-06. Section 5 of the phase document writes `FaceId` and `IdentityId` into
// the frozen `PeopleService` signatures, so they are ids of the same kind as the
// five above rather than bare strings.
//
// Two of them and not one, even though both name "a thing with a face in it": a
// merge takes two `IdentityId`s and a split takes an `IdentityId` and a list of
// `FaceId`s, and getting those two arguments the wrong way round is precisely the
// catalog-corrupting mistake this macro exists to make impossible. See
// docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md section 2.
typed_id!(FaceId, "fce");
typed_id!(IdentityId, "idt");

// PHASE-07. Section 5 writes `SegmentId` into the frozen `Segment` shape, so a
// chapter of the story is an id of the same kind as the seven above.
//
// One and not two, unlike phase 06's pair: there is no operation that takes a
// segment and a chapter as separate ids, because `ChapterId` is a closed enum and
// not a row. See
// docs/adr/ADR-0015-wedding-scene-taxonomy-and-story-segmentation.md section 2.
typed_id!(SegmentId, "seg");

// PHASE-08. Section 5 writes `MomentId` into the frozen `Moment` shape, so a thing
// the photographer shot once is an id of the same kind as the eight above.
//
// One and not two, unlike phase 06's pair, and for a different reason than phase
// 07's: a burst is not a row. Section 2.1's two-tier structure stores bursts as a
// partition *inside* a moment - `moment_images.burst_ix` - because a burst has no
// identity a photographer can refer to, no lock, and no lifetime independent of the
// moment that contains it. An id for it would be an id nothing could be looked up by.
// See docs/adr/ADR-0017-burst-grouping-and-duplicate-policy.md section 2.
typed_id!(MomentId, "mom");

// PHASE-13. Section 5 writes `DecisionId` into the frozen `Decision` shape, so a
// thing the product decided is an id of the same kind as the nine above.
//
// It is the first id in this file that names an *event* rather than a thing: a
// project, a photograph, a face, a chapter and a moment all exist in the world,
// and a decision exists only because the product made it. That is exactly why it
// needs an id of its own. `aura replay <decision_id>` is a support command a
// photographer reads down a telephone, the ledger is append-only so a correction
// is a second row pointing at the first, and both of those need something stable
// to point at. A composite key of (subject, kind, timestamp) would have been the
// alternative and it fails on the first re-run inside the same millisecond.
//
// See docs/adr/ADR-0027-decision-ledger-and-confidence.md section 2.
typed_id!(DecisionId, "dcn");

// PHASE-17. Section 5 writes `ProfileId` into the frozen `StyleProfile` shape, so a
// photographer's learned look is an id of the same kind as the ten above.
//
// It is the first id in this file that names something a person *makes* rather than
// something the product finds or records. A project, a photograph and a face exist in
// the world; a decision exists because the product made it; a profile exists because a
// photographer pointed AURA at four weddings and pressed a button.
//
// One and not two, though the alternative was real: a `BucketId` for the eighty leaves
// would have made `profile_buckets` a two-column key. It is not here because a bucket is
// a *coordinate* and not a row - `(SceneGroup, LightingBucket)` names it completely, the
// pair is closed in code, and an id for it would be an id nothing could be looked up by
// that the coordinate could not. The same argument phase 08 made about a burst.
//
// A profile also has a *version*, and the version is deliberately not part of this id:
// `aura-style` keeps every version of a name so that a gallery delivered under version 3
// still reproduces after version 4 is adopted, and each of them is its own row with its
// own `ProfileId`. Two profiles that share a name are two profiles.
//
// See docs/adr/ADR-0035-style-learning-and-personal-profiles.md section "Decision 8".
typed_id!(ProfileId, "prf");

// PHASE-18. Section 5 writes `MaskId` into the frozen `Mask` shape, so a region of a
// photograph is an id of the same kind as the eleven above.
//
// It is the first id in this file that names a *part of* something rather than a whole
// thing. A project, a photograph, a face and a chapter are all things you can point at; a
// mask is a claim about which pixels of a photograph are her hair. That is exactly why it
// needs an id: a composition refers to its operands, a brush stroke refers to what it edits,
// a recipe's local parameter block refers to the region it applies inside, and all three of
// those need something stable to point at that survives a re-analysis.
//
// One and not two, and the alternative was real: a `MaskSetId` for "every mask of one
// photograph" would have made the store's primary key one column instead of two. It is not
// here because a mask set is not a thing that can be edited, locked, referred to or
// regenerated independently - `(image_id, kind, identity)` names it completely, and phase 08
// made the same argument about a burst.
//
// See docs/adr/ADR-0037-semantic-masks-matting-and-quality-gating.md decision 1.
typed_id!(MaskId, "msk");

// PHASE-24. Section 5 writes `ProposalId` into the frozen `CleanupProposal` shape, so a proposed
// removal is an id of the same kind as the twelve above.
//
// It is the second id in this file that names a *part of* something rather than a whole thing,
// after `MaskId`, and the first that names something that may never happen. A proposal is a
// suggestion; most of them are rejected, and the rejected ones are exactly the rows the delivery
// report and the adversarial audit are read from. Something that is refused still needs a name,
// because "which one did you refuse" is the question both of those ask.
//
// It is not a `CleanupId` on the applied removal, and the alternative was real: an id issued only
// when a removal happens would make the applied table's key one column. It is not here because
// then a rejection would have no identity, and a photographer who rejects a proposal and re-runs
// the pass would be shown it again with nothing to say they had already answered.
//
// See docs/adr/ADR-0049-generative-cleanup-and-the-safety-engine.md decision 10.
typed_id!(ProposalId, "prp");

// PHASE-25. Section 5 writes `NodeId` into the frozen `SceneNode` shape, so a lighting group
// inside a chapter is an id of the same kind as the thirteen above.
//
// It is the third id in this file that names a *part of* something rather than a whole thing,
// after `MaskId` and `ProposalId`. A node is a sub-range of a phase 07 segment: the frames of one
// chapter that were shot under one light, which is not the same set as the chapter and not a set
// anybody named.
//
// It is not `(segment_id, ordinal)`, and the alternative was real: an ordinal inside a segment
// would name a node with no new id at all. It is not here because a node is split by a change
// point, merged by a photographer and re-parented as the tree grows, and an ordinal renumbers on
// every one of those - while an anchor row, a delta row and an outlier row all have to keep
// pointing at the same node across a re-analysis.
//
// See docs/adr/ADR-0051-gallery-consistency-and-normalisation.md section 10.
typed_id!(NodeId, "nod");

// PHASE-26. Section 4 gives matched pairs their own table and section 9 gives SFE a matched-pair
// viewer, so a pair is a thing a photographer looks at rather than a tuple inside a solver.
//
// It is the fourth id in this file that names a *relationship* rather than a thing, and the first
// that names one between two photographs. It is not `(left, right)` for the reason `NodeId` is not
// `(segment_id, ordinal)`: a pair is re-formed on every pass as the scene tree moves under it,
// while its held-out flag has to stay attached to the same pair across a re-solve - otherwise the
// held-out split changes between the fit and the check, which is the one thing that would make
// section 6.2's verification meaningless while looking like it ran.
//
// See docs/adr/ADR-0053-camera-matching-and-appearance-transforms.md section 2.
typed_id!(PairId, "pai");

// PHASE-27. Section 5 writes `TicketId` into the frozen `QcTicket` shape, so a quality-control
// finding is an id of the same kind as the fifteen above.
//
// It is the first id in this file that names a *problem* rather than a thing, a part of a thing or
// a relationship between two things - and the first whose subject may cease to exist while the id
// must not. A ticket outlives what it is about: a frame replaced by its runner-up leaves a ticket
// that has to keep pointing at the replacement it caused, and a remedy that was reverted leaves a
// ticket whose whole value is the record that the product tried something and put it back.
//
// It is not `(image_id, category)`, and the alternative was real: ten categories over one
// photograph would name every finding with no new id at all. It is not here for two reasons. One
// image can carry two findings in the same category on two different faces, and section 6.3's
// bounded loop attaches rounds to a ticket rather than to a category - so a second round on the
// same `(image, category)` would either overwrite the first round's record or be indistinguishable
// from it, and "we tried twice" and "we tried once" are the two things the loop bound exists to
// tell apart.
//
// See docs/adr/ADR-0055-quality-control-tickets-and-the-re-edit-loop.md section 2.
typed_id!(TicketId, "tkt");

/// Content address: BLAKE3 of the file bytes. Two files with the same digest
/// are the same file, no matter what they are called or where they live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Wrap 32 raw digest bytes.
    #[must_use]
    pub const fn from_bytes(b: [u8; 32]) -> Self {
        Self(b)
    }

    /// Borrow the raw digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lower-case hex form, 64 characters, as stored in the catalog.
    #[must_use]
    pub fn to_hex(&self) -> String {
        use std::fmt::Write as _;

        let mut out = String::with_capacity(64);
        for byte in &self.0 {
            // Writing into a String cannot fail; the result is ignored deliberately.
            let _ = write!(out, "{byte:02x}");
        }
        out
    }

    /// Parse the 64-character hex form.
    ///
    /// # Errors
    ///
    /// Returns [`IdParseError::NotAUuid`] when the text is not 64 hex digits.
    pub fn from_hex(hex: &str) -> Result<Self, IdParseError> {
        if hex.len() != 64 {
            return Err(IdParseError::NotAUuid);
        }
        let mut out = [0u8; 32];
        for (i, slot) in out.iter_mut().enumerate() {
            let pair = hex.get(i * 2..i * 2 + 2).ok_or(IdParseError::NotAUuid)?;
            *slot = u8::from_str_radix(pair, 16).map_err(|_| IdParseError::NotAUuid)?;
        }
        Ok(Self(out))
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}
