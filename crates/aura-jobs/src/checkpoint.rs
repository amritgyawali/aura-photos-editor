//! What a stage had finished, and how a resume knows whether it may trust it.
//!
//! Section 6.1. Two ideas, and the second is the one that matters.
//!
//! ## The first: a checkpoint is written with the work, not after it
//!
//! [`CheckpointWriter::commit`] takes the same transaction the stage's own rows go in. A
//! checkpoint written in a second transaction is a checkpoint that can be one unit ahead of the
//! catalog or one behind it, and both of those are how a resume duplicates work or loses it. Phase
//! 01 established this for `task`; this is the same rule at stage granularity.
//!
//! ## The second: a checkpoint is keyed by what the stage read
//!
//! [`inputs_hash`] is a digest of the stage's declared inputs. A resumed stage whose hash differs
//! from the stored one has had something change underneath it and starts again;
//! [`Invalidation`] says which of the three reasons it was.
//!
//! The alternative - keying on time, or on a run id, or on nothing - resumes happily onto stale
//! work. That is the failure this module exists to prevent and it is completely silent: a wedding
//! whose scene profiles were re-tuned between two halves of a run would deliver half a gallery
//! graded one way and half the other, with every unit test passing.
//!
//! ## What the hash covers, and what it deliberately does not
//!
//! It covers the stage's own version, the versions of everything it depends on, the policy
//! version and the unit count. It does *not* cover the run id, the wall clock, the machine, the
//! governor's action or the batch size - because none of those changes what the stage would
//! decide, and a hash that included them would invalidate every checkpoint on every resume, which
//! is a resume that is indistinguishable from a fresh run.

use aura_core::contract::ids::RunId;
use aura_core::AuraResult;
use rusqlite::Transaction;

use crate::contract::autopilot::{Checkpoint, Invalidation, StageId};

/// The version of the orchestrator's own semantics.
///
/// Bumped when the DAG's shape, the checkpoint format or the resume rules change - which
/// invalidates every stored checkpoint, because a checkpoint written by a different scheduler
/// describes a plan this one does not have. It is *not* bumped when a stage's estimate changes:
/// an estimate is an ETA input rather than a decision, and re-running a wedding because somebody
/// measured a faster laptop would be a version column counting the wrong thing.
pub const ORCHESTRATOR_VER: i64 = 1;

/// The digest of what one stage read.
///
/// The parts are joined with a separator that cannot appear in any of them, and the whole is
/// hashed rather than stored, so a stage that grows a fourteenth input does not grow the row.
#[must_use]
pub fn inputs_hash(parts: &[(&str, &str)]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"aura-autopilot-inputs-v1");
    for (key, value) in parts {
        hasher.update(b"\x1f");
        hasher.update(key.as_bytes());
        hasher.update(b"\x1e");
        hasher.update(value.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// Whether a stored checkpoint may be continued from.
///
/// Two ways to be wrong, and they mean different things: a changed unit count is a project
/// somebody imported into, and a moved hash is an upstream that re-decided. Both restart the stage
/// and only the second is worth a sentence in the run summary, because only the second is
/// something the product did to itself.
///
/// [`Invalidation::UnknownStage`] is not decided here and cannot be: `Checkpoint::stage` is a
/// [`StageId`], so by the time a checkpoint exists as a value its stage is one this build has. It
/// is produced by [`crate::store`] when a stored row names a stage this build does not know -
/// which is a checkpoint written by a different release - and it is a variant of this enum rather
/// than a store error so the two live in one place a reader can compare.
#[must_use]
pub fn invalidation(stored: &Checkpoint, current_hash: &str, current_units: u32) -> Invalidation {
    if stored.items_total != current_units {
        return Invalidation::ScopeChanged;
    }
    if stored.inputs_hash != current_hash {
        return Invalidation::InputsMoved;
    }
    Invalidation::None
}

/// Where a resumed stage starts from.
///
/// Zero when the checkpoint is invalid, and `items_done` when it is not. Bounded by the current
/// unit count, because a checkpoint claiming more finished units than the project has photographs
/// is a checkpoint that would make a stage skip work it never did.
#[must_use]
pub fn resume_from(stored: Option<&Checkpoint>, invalidation: Invalidation, units: u32) -> u32 {
    if invalidation.restarts() {
        return 0;
    }
    stored.map_or(0, |checkpoint| checkpoint.items_done.min(units))
}

/// Writes a checkpoint inside the caller's own transaction.
///
/// A struct rather than a free function so the store owns the SQL and this module owns the
/// semantics. `aura-jobs` has had a catalog dependency since phase 01, so there is no indirection
/// to justify here - what there is, is a rule: nothing may write a checkpoint outside the
/// transaction the work went in.
#[derive(Debug, Clone, Copy)]
pub struct CheckpointWriter;

impl CheckpointWriter {
    /// Write or replace one stage's checkpoint in `tx`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the statement fails.
    pub fn commit(tx: &Transaction<'_>, checkpoint: &Checkpoint) -> AuraResult<()> {
        crate::store::write_checkpoint(tx, checkpoint)
    }

    /// A fresh checkpoint for a stage that is about to start.
    #[must_use]
    pub fn opening(
        run_id: RunId,
        stage: StageId,
        items_total: u32,
        inputs_hash: String,
        attempts: u8,
    ) -> Checkpoint {
        Checkpoint {
            run_id,
            stage,
            items_done: 0,
            items_total,
            inputs_hash,
            attempts,
            outcome: None,
            elapsed_ms: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkpoint(items_done: u32, items_total: u32, hash: &str) -> Checkpoint {
        Checkpoint {
            run_id: RunId::new(),
            stage: StageId::Embed,
            items_done,
            items_total,
            inputs_hash: hash.to_string(),
            attempts: 1,
            outcome: None,
            elapsed_ms: 0,
        }
    }

    #[test]
    fn the_same_inputs_hash_the_same_way_every_time() {
        let parts = [("embed_ver", "3"), ("units", "412")];
        assert_eq!(inputs_hash(&parts), inputs_hash(&parts));
    }

    #[test]
    fn a_different_input_hashes_differently() {
        assert_ne!(
            inputs_hash(&[("embed_ver", "3")]),
            inputs_hash(&[("embed_ver", "4")])
        );
    }

    #[test]
    fn the_separator_cannot_be_forged_by_a_value() {
        // Two different input sets that would collide under naive concatenation. `("ab", "c")`
        // and `("a", "bc")` are the classic pair, and a digest that treated them as the same
        // would be a digest that missed an upstream change.
        assert_ne!(inputs_hash(&[("ab", "c")]), inputs_hash(&[("a", "bc")]));
    }

    #[test]
    fn an_unchanged_stage_continues_where_it_left_off() {
        let stored = checkpoint(120, 400, "abc");
        let verdict = invalidation(&stored, "abc", 400);
        assert_eq!(verdict, Invalidation::None);
        assert_eq!(resume_from(Some(&stored), verdict, 400), 120);
    }

    #[test]
    fn a_moved_upstream_restarts_the_stage_and_nothing_else() {
        let stored = checkpoint(120, 400, "abc");
        let verdict = invalidation(&stored, "def", 400);
        assert_eq!(verdict, Invalidation::InputsMoved);
        assert!(verdict.restarts());
        assert_eq!(resume_from(Some(&stored), verdict, 400), 0);
    }

    #[test]
    fn importing_more_photographs_restarts_the_stage() {
        let stored = checkpoint(400, 400, "abc");
        assert_eq!(
            invalidation(&stored, "abc", 900),
            Invalidation::ScopeChanged
        );
    }

    #[test]
    fn a_checkpoint_claiming_more_than_the_project_has_is_bounded() {
        // Not reachable through the runner, and it is bounded anyway: the alternative is a stage
        // that starts at unit 900 of 400 and finishes immediately having done nothing.
        let stored = checkpoint(900, 400, "abc");
        assert_eq!(resume_from(Some(&stored), Invalidation::None, 400), 400);
    }

    #[test]
    fn no_checkpoint_starts_at_the_beginning() {
        assert_eq!(resume_from(None, Invalidation::None, 400), 0);
    }
}
