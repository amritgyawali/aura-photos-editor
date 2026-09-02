//! Rollback: putting the previous version back, exactly.
//!
//! ## "Exactly" is a byte comparison, not a re-derivation
//!
//! Section 10.1 asks that rollback "restores the previous profile exactly". The tempting
//! implementation is to subtract the update's offsets, which is exact on paper and is not: a
//! profile version carries diagnostics, a trained-pairs count and a timestamp that no arithmetic
//! reproduces, and floating-point subtraction of a value that was clamped on the way in does not
//! give the value back.
//!
//! So a snapshot is the whole profile document, stored with its own digest, and [`restore`]
//! returns both. Phase 14 made the same call about `edit_history.body` for the same reason: a chain
//! of deltas is exact only if every delta was computed against the state that actually preceded it,
//! and an undo that is nearly right is worse than no undo.
//!
//! ## The depth is bounded and the bound is a product decision
//!
//! `ROLLBACK_DEPTH` versions. A photographer rolls back to the version before the one that went
//! wrong, not to the version from last spring, and an unbounded history is a table that grows with
//! usage forever.

use aura_core::contract::ids::ProfileId;
use aura_core::contract::learn::{LearnCode, LearnReason};
use aura_core::AuraResult;

use crate::errors::rollback_failed;
use crate::store::LearnStore;

/// A restored profile document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restored {
    /// The version that is now current.
    pub version: u16,
    /// The whole document, byte for byte as it was stored.
    pub body: String,
    /// Its digest, recomputed on read.
    pub body_hash: String,
}

/// Restore the version before the current one.
///
/// # Errors
///
/// `AURA-LRN-11005` when there is no earlier version, or when the stored bytes do not hash to the
/// digest recorded beside them - which is a corrupt snapshot, and putting one back would be worse
/// than refusing.
pub fn restore(store: &LearnStore, profile: ProfileId) -> AuraResult<(Restored, Vec<LearnReason>)> {
    let current = store.current_version(profile)?;
    let Some(current) = current else {
        return Err(rollback_failed(format!(
            "profile {} has no versions",
            profile.to_db()
        )));
    };
    if current <= 1 {
        return Err(rollback_failed(format!(
            "profile {} is at version {current} and has nothing behind it",
            profile.to_db()
        )));
    }

    let previous = current - 1;
    let Some((body, stored_hash)) = store.snapshot(profile, previous)? else {
        return Err(rollback_failed(format!(
            "profile {} has no snapshot at version {previous}",
            profile.to_db()
        )));
    };

    // A snapshot whose bytes do not match its digest is a corrupt snapshot, and putting one back
    // is worse than refusing: the photographer would be on a profile nobody wrote.
    let actual = blake3::hash(body.as_bytes()).to_hex().to_string();
    if actual != stored_hash {
        return Err(rollback_failed(format!(
            "the snapshot of profile {} at version {previous} does not match its digest",
            profile.to_db()
        )));
    }

    store.set_current_version(profile, previous)?;

    Ok((
        Restored {
            version: previous,
            body,
            body_hash: actual,
        },
        vec![
            LearnReason::plain(LearnCode::RolledBack),
            LearnReason::with(LearnCode::RollbackExact, format!("version {previous}")),
        ],
    ))
}

/// Whether a profile has anything to roll back to.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn can_roll_back(store: &LearnStore, profile: ProfileId) -> AuraResult<bool> {
    Ok(store.current_version(profile)?.is_some_and(|v| v > 1))
}
