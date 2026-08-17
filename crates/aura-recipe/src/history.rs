//! Edit history: parameter-level undo and redo, snapshots, and the two resets.
//!
//! Section 2.1: *"Edit history: parameter-level undo/redo, snapshots, and 'reset to AI
//! suggestion' / 'reset to original'."*
//!
//! # Why an entry stores a whole recipe
//!
//! A history of *deltas* is smaller and is the wrong shape. Undo has to be exact, and a
//! chain of deltas is exact only if every delta was computed against the state that
//! actually preceded it - which stops being true the first time a merge refuses a path
//! (`AURA-RENDER-8006`) and the proposal and the result diverge. An entry here carries the
//! full document *and* the list of paths that moved, so undo is a swap and the panel can
//! still say what one step changed.
//!
//! The cost is bounded by [`MAX_ENTRIES`] and measured: a canonical recipe is about 1.4 KB,
//! so a hundred steps is 140 KB per photograph in memory and one row per step on disk.
//!
//! # The two resets are not the same
//!
//! * [`ResetTo::Original`] restores the camera's own starting point. Everything goes,
//!   including the automated passes.
//! * [`ResetTo::AiSuggestion`] restores what the product proposed *before* the person
//!   changed it, and **removes those paths from `user_edited_fields`**. That last clause is
//!   the only sanctioned way a protected path becomes unprotected, and it exists so that a
//!   photographer who tried a setting and did not like it can hand the field back rather
//!   than being stuck with their own experiment forever.

use aura_core::clock::Clock;
use aura_core::errors::render::recipe_invalid;
use aura_core::AuraResult;

use crate::contract::recipe::{EditSource, Recipe};

/// How many steps are kept. Past this the oldest is dropped, and the *original* is not one
/// of them - it lives in its own field and cannot age out.
pub const MAX_ENTRIES: usize = 100;

/// One step in the history.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    /// Position in the history, from 1. Stable across a drop of the oldest entries.
    pub seq: u64,
    /// Wall-clock milliseconds since the Unix epoch, from the injected clock.
    pub at_ms: i64,
    /// Who made this step.
    pub source: EditSource,
    /// The dotted paths this step changed, sorted.
    pub changed: Vec<String>,
    /// A short label for the panel, e.g. `Exposure`, `AI: exposure and white balance`.
    pub label: String,
    /// The complete document after this step.
    pub recipe: Recipe,
}

/// A named point a photographer can come back to.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// The photographer's own name for it.
    pub name: String,
    /// When it was taken.
    pub at_ms: i64,
    /// The complete document.
    pub recipe: Recipe,
}

/// What a reset goes back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetTo {
    /// The camera's own starting point. Every edit goes.
    Original,
    /// What the product proposed before a person changed it.
    AiSuggestion,
}

/// One photograph's edit history.
#[derive(Debug, Clone)]
pub struct History {
    original: Recipe,
    ai_suggestion: Option<Recipe>,
    entries: Vec<HistoryEntry>,
    /// Index of the current entry. `entries.len()` means "at the head".
    cursor: usize,
    snapshots: Vec<Snapshot>,
    next_seq: u64,
}

impl History {
    /// Start a history from the camera's own starting point.
    #[must_use]
    pub fn new(original: Recipe) -> Self {
        Self {
            original,
            ai_suggestion: None,
            entries: Vec::new(),
            cursor: 0,
            snapshots: Vec::new(),
            next_seq: 1,
        }
    }

    /// Rebuild a history from stored rows.
    ///
    /// The cursor lands at the head, because a stored history has no notion of "undone but
    /// not redone": an undo that was never followed by a new step is still a step the
    /// catalog recorded, and re-opening a photograph at the head is what every editor does.
    ///
    /// `ai_suggestion` is recovered by scanning backwards for the newest automated step,
    /// rather than stored as a column. One source of truth: a column could disagree with the
    /// rows, and the rows are what undo walks.
    #[must_use]
    pub fn rehydrate(
        original: Recipe,
        entries: Vec<HistoryEntry>,
        snapshots: Vec<Snapshot>,
    ) -> Self {
        let ai_suggestion = entries
            .iter()
            .rev()
            .find(|entry| entry.source.is_automated())
            .map(|entry| entry.recipe.clone());
        let next_seq = entries.iter().map(|e| e.seq).max().unwrap_or(0) + 1;
        let cursor = entries.len();
        Self {
            original,
            ai_suggestion,
            entries,
            cursor,
            snapshots,
            next_seq,
        }
    }

    /// The recipe as it stands.
    #[must_use]
    pub fn current(&self) -> &Recipe {
        self.entries
            .get(self.cursor.wrapping_sub(1))
            .map_or(&self.original, |entry| &entry.recipe)
    }

    /// The camera's own starting point.
    #[must_use]
    pub const fn original(&self) -> &Recipe {
        &self.original
    }

    /// What the product last proposed, if it has proposed anything.
    #[must_use]
    pub const fn ai_suggestion(&self) -> Option<&Recipe> {
        self.ai_suggestion.as_ref()
    }

    /// Every step, oldest first.
    #[must_use]
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Every snapshot, in the order they were taken.
    #[must_use]
    pub fn snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    /// True when [`Self::undo`] would do something.
    #[must_use]
    pub const fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    /// True when [`Self::redo`] would do something.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.cursor < self.entries.len()
    }

    /// Record a step.
    ///
    /// Anything that was undone and not redone is discarded first, which is what every
    /// editor does and what a photographer expects: making a new change after undoing three
    /// abandons the three.
    ///
    /// A step that changed nothing is not recorded. An automated pass that proposed only
    /// protected paths produces no history entry, because "nothing happened" is not a step
    /// somebody should have to undo past.
    pub fn push(
        &mut self,
        recipe: Recipe,
        source: EditSource,
        changed: Vec<String>,
        label: impl Into<String>,
        clock: &dyn Clock,
    ) {
        if changed.is_empty() {
            return;
        }
        if source.is_automated() {
            self.ai_suggestion = Some(recipe.clone());
        }
        self.entries.truncate(self.cursor);
        let mut sorted = changed;
        sorted.sort_unstable();
        sorted.dedup();
        self.entries.push(HistoryEntry {
            seq: self.next_seq,
            at_ms: (clock.now_utc().unix_timestamp_nanos() / 1_000_000) as i64,
            source,
            changed: sorted,
            label: label.into(),
            recipe,
        });
        self.next_seq += 1;
        if self.entries.len() > MAX_ENTRIES {
            self.entries.remove(0);
        }
        self.cursor = self.entries.len();
    }

    /// Step back one, returning the recipe that is now current.
    pub fn undo(&mut self) -> Option<&Recipe> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        Some(self.current())
    }

    /// Step forward one, returning the recipe that is now current.
    pub fn redo(&mut self) -> Option<&Recipe> {
        if self.cursor >= self.entries.len() {
            return None;
        }
        self.cursor += 1;
        Some(self.current())
    }

    /// Take a named snapshot of the current state.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8002` when the name is empty or already taken. A second snapshot called
    /// "final" is how somebody loses the first one.
    pub fn snapshot(&mut self, name: impl Into<String>, clock: &dyn Clock) -> AuraResult<()> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(recipe_invalid("snapshot.name", "must not be empty"));
        }
        if self.snapshots.iter().any(|s| s.name == name) {
            return Err(recipe_invalid("snapshot.name", "already taken"));
        }
        self.snapshots.push(Snapshot {
            name,
            at_ms: (clock.now_utc().unix_timestamp_nanos() / 1_000_000) as i64,
            recipe: self.current().clone(),
        });
        Ok(())
    }

    /// Go back to a named snapshot, recording the move as a step.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8002` when no snapshot has that name.
    pub fn restore(&mut self, name: &str, clock: &dyn Clock) -> AuraResult<()> {
        let Some(snapshot) = self.snapshots.iter().find(|s| s.name == name).cloned() else {
            return Err(recipe_invalid(
                "snapshot.name",
                "no snapshot with that name",
            ));
        };
        let changed = changed_paths(self.current(), &snapshot.recipe);
        self.push(
            snapshot.recipe,
            EditSource::User,
            changed,
            format!("Restore snapshot: {name}"),
            clock,
        );
        Ok(())
    }

    /// Reset to the original or to the product's own suggestion.
    ///
    /// [`ResetTo::AiSuggestion`] also removes the reset paths from
    /// `user_edited_fields`; see the module documentation for why that is the only
    /// sanctioned way a protected path becomes unprotected.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8002` when [`ResetTo::AiSuggestion`] is asked for and no automated pass
    /// has run on this photograph.
    pub fn reset(&mut self, to: ResetTo, clock: &dyn Clock) -> AuraResult<()> {
        let (target, label) = match to {
            ResetTo::Original => (self.original.clone(), "Reset to original"),
            ResetTo::AiSuggestion => {
                let Some(suggestion) = self.ai_suggestion.clone() else {
                    return Err(recipe_invalid(
                        "reset",
                        "no automated pass has run on this photograph",
                    ));
                };
                (suggestion, "Reset to AI suggestion")
            }
        };

        let changed = changed_paths(self.current(), &target);
        let mut next = target;
        // The reset hands the fields back. This is the only place in the workspace that
        // shortens `user_edited_fields`, and it is reached only from a person's own action.
        next.provenance
            .user_edited_fields
            .retain(|path| !changed.contains(path));
        next.provenance.source = EditSource::User;

        self.push(next, EditSource::User, changed, label, clock);
        Ok(())
    }
}

/// Dotted paths where two recipes differ, sorted.
///
/// Uses the same flattening as the merge, so "what changed" and "what is protected" are the
/// same vocabulary. A path that appears in one and not the other would be a protection that
/// silently does not apply.
#[must_use]
pub fn changed_paths(before: &Recipe, after: &Recipe) -> Vec<String> {
    let mut out = Vec::new();
    for path in crate::schema::paths(after) {
        if path.starts_with("provenance") {
            continue;
        }
        if crate::schema::leaf_differs(before, after, &path) {
            out.push(path);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use aura_core::clock::FixedClock;
    use std::sync::Arc;
    use time::OffsetDateTime;

    fn clock() -> Arc<FixedClock> {
        FixedClock::at(OffsetDateTime::UNIX_EPOCH)
    }

    fn ai_pass(history: &mut History, exposure: f32, clock: &dyn Clock) {
        let mut proposal = history.current().clone();
        proposal.global.exposure = exposure;
        proposal.provenance.source = EditSource::Ai;
        let changed = changed_paths(history.current(), &proposal);
        history.push(proposal, EditSource::Ai, changed, "AI: exposure", clock);
    }

    #[test]
    fn undo_and_redo_walk_the_history() {
        let clock = clock();
        let mut history = History::new(fixtures::neutral(fixtures::FIXTURE_HASH, "ILCE-7M4"));
        assert!(!history.can_undo());

        ai_pass(&mut history, 0.5, clock.as_ref());
        ai_pass(&mut history, 1.5, clock.as_ref());
        assert!((history.current().global.exposure - 1.5).abs() < 1e-6);

        history.undo().expect("undo");
        assert!((history.current().global.exposure - 0.5).abs() < 1e-6);
        history.undo().expect("undo");
        assert!(
            history.current().global.exposure.abs() < 1e-6,
            "back at the original"
        );
        assert!(history.undo().is_none());

        history.redo().expect("redo");
        assert!((history.current().global.exposure - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_new_step_after_an_undo_discards_the_redo_tail() {
        let clock = clock();
        let mut history = History::new(fixtures::neutral(fixtures::FIXTURE_HASH, "ILCE-7M4"));
        ai_pass(&mut history, 0.5, clock.as_ref());
        ai_pass(&mut history, 1.5, clock.as_ref());
        history.undo();
        ai_pass(&mut history, -2.0, clock.as_ref());
        assert!(!history.can_redo());
        assert_eq!(history.entries().len(), 2);
        assert!((history.current().global.exposure - (-2.0)).abs() < 1e-6);
    }

    #[test]
    fn a_step_that_changed_nothing_is_not_recorded() {
        let clock = clock();
        let mut history = History::new(fixtures::reference());
        let same = history.current().clone();
        history.push(same, EditSource::Ai, Vec::new(), "nothing", clock.as_ref());
        assert!(history.entries().is_empty());
    }

    #[test]
    fn a_snapshot_can_be_taken_and_restored() {
        let clock = clock();
        let mut history = History::new(fixtures::neutral(fixtures::FIXTURE_HASH, "ILCE-7M4"));
        ai_pass(&mut history, 0.8, clock.as_ref());
        history
            .snapshot("before the reception", clock.as_ref())
            .expect("snapshot");
        ai_pass(&mut history, -3.0, clock.as_ref());

        history
            .restore("before the reception", clock.as_ref())
            .expect("restore");
        assert!((history.current().global.exposure - 0.8).abs() < 1e-6);
    }

    #[test]
    fn two_snapshots_may_not_share_a_name() {
        let clock = clock();
        let mut history = History::new(fixtures::reference());
        history.snapshot("final", clock.as_ref()).expect("first");
        let err = history
            .snapshot("final", clock.as_ref())
            .expect_err("second");
        assert_eq!(err.code.to_string(), "AURA-RENDER-8002");
    }

    #[test]
    fn reset_to_original_removes_every_edit() {
        let clock = clock();
        let original = fixtures::neutral(fixtures::FIXTURE_HASH, "ILCE-7M4");
        let mut history = History::new(original.clone());
        ai_pass(&mut history, 1.2, clock.as_ref());

        history
            .reset(ResetTo::Original, clock.as_ref())
            .expect("reset");
        assert!(history.current().global.exposure.abs() < 1e-6);
        assert!(history.can_undo(), "the reset is itself a step");
    }

    #[test]
    fn reset_to_the_ai_suggestion_hands_the_field_back() {
        let clock = clock();
        let mut history = History::new(fixtures::neutral(fixtures::FIXTURE_HASH, "ILCE-7M4"));
        ai_pass(&mut history, 0.9, clock.as_ref());

        // The photographer changes it, which protects the path.
        let mut edited = history.current().clone();
        edited.global.exposure = -0.4;
        edited.provenance.source = EditSource::User;
        edited.provenance.user_edited_fields = vec!["global.exposure".to_string()];
        let changed = changed_paths(history.current(), &edited);
        history.push(
            edited,
            EditSource::User,
            changed,
            "Exposure",
            clock.as_ref(),
        );
        assert!(history
            .current()
            .provenance
            .user_edited_fields
            .contains(&"global.exposure".to_string()));

        history
            .reset(ResetTo::AiSuggestion, clock.as_ref())
            .expect("reset");
        assert!((history.current().global.exposure - 0.9).abs() < 1e-6);
        assert!(
            !history
                .current()
                .provenance
                .user_edited_fields
                .contains(&"global.exposure".to_string()),
            "the reset is the only way a protected path becomes unprotected"
        );
    }

    #[test]
    fn reset_to_the_ai_suggestion_is_refused_when_nothing_has_proposed_anything() {
        let clock = clock();
        let mut history = History::new(fixtures::neutral(fixtures::FIXTURE_HASH, "ILCE-7M4"));
        let err = history
            .reset(ResetTo::AiSuggestion, clock.as_ref())
            .expect_err("nothing to reset to");
        assert_eq!(err.code.to_string(), "AURA-RENDER-8002");
    }

    #[test]
    fn the_history_is_bounded_and_the_original_cannot_age_out() {
        let clock = clock();
        let original = fixtures::neutral(fixtures::FIXTURE_HASH, "ILCE-7M4");
        let mut history = History::new(original.clone());
        for i in 0..(MAX_ENTRIES + 20) {
            ai_pass(&mut history, i as f32 * 0.01, clock.as_ref());
        }
        assert_eq!(history.entries().len(), MAX_ENTRIES);
        assert!(history.original().global.exposure.abs() < 1e-6);
    }
}
