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
