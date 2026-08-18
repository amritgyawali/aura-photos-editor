//! The recipe migration framework.
//!
//! Section 2.1 asks for *"a recipe migration framework so v1 recipes still render correctly
//! under v2+ engines"*, and section 6.4 for *"migration functions per schema version, plus a
//! golden test that v1 recipes render identically under later engines within tolerance."*
//!
//! # Three rules, from ADR-0029 section 7
//!
//! 1. **A field is never removed.** A v2 that stops using `clarity` must keep reading it and
//!    mapping it, because a v1 recipe in a catalog from 2026 has to render in 2029.
//!    [`DEPRECATED`] is the list and it is only ever appended to.
//! 2. **An unknown field is preserved, not dropped.** `Recipe::extra` round-trips anything a
//!    newer build wrote. A v1 engine reading a v2 recipe raises `AURA-RENDER-8003`, renders
//!    what it understands, and writes the document back intact.
//! 3. **A migration is tested against a frozen document.**
//!    `crates/aura-recipe/tests/migration.rs` migrates a byte-frozen v1 recipe and asserts
//!    the recipe hash of the result equals the hash of the original, which is section 10.1's
//!    last row.
//!
//! # Why the table is empty today
//!
//! There is exactly one schema version, so [`STEPS`] has no entries. The framework ships
//! anyway, with its tests, because the alternative is writing it under pressure on the day
//! a v2 is needed - which is the day a mistake costs somebody's catalog.

use aura_core::errors::render::recipe_from_future;
use aura_core::{AuraError, AuraResult};

use crate::contract::recipe::{Recipe, SCHEMA_VERSION};

/// One migration step: take a document at `from` and return it at `from + 1`.
///
/// A step may read `Recipe::extra` - that is where a field this build does not know about
/// arrives - and may move a value out of it into a typed field. It may never *delete* a key
/// it did not consume.
type Step = fn(&mut Recipe);

/// The ordered migration table, one entry per version step.
///
/// `STEPS[i]` migrates version `i + 1` to version `i + 2`. Empty at schema v1, and it is
/// meant to be: adding a v2 means appending exactly one function here and bumping
/// [`SCHEMA_VERSION`].
pub const STEPS: &[Step] = &[];

/// Fields that no longer do anything but are still read, still written, and still migrated.
///
/// Appended to, never edited, never removed. A field on this list keeps its meaning for
/// every document that already carries it; what changes is that no new document sets it.
pub const DEPRECATED: &[&str] = &[];

/// Bring a document to [`SCHEMA_VERSION`], if it can be brought.
///
/// Returns the migrated recipe and, for a document from a *newer* schema, the warning that
/// says so. A newer document is not an error: what this build understands is applied and
/// `Recipe::extra` carries the rest through untouched.
///
/// # Errors
///
/// Never fails today. The signature returns `AuraResult` because a step that cannot map a
/// value must be able to refuse, and changing a public signature later is worse than
/// carrying an unused `Result` now.
pub fn to_current(recipe: &Recipe) -> AuraResult<(Recipe, Option<AuraError>)> {
    let mut out = recipe.clone();

    if out.schema > SCHEMA_VERSION {
        // Rule 2. Nothing is stripped; the caller is told and the document is left alone.
        return Ok((
            out.clone(),
            Some(recipe_from_future(out.schema, SCHEMA_VERSION)),
        ));
    }

    while out.schema < SCHEMA_VERSION {
        let index = usize::from(out.schema).saturating_sub(1);
        let Some(step) = STEPS.get(index) else {
            // No step for this version. The document cannot be brought forward, so it is
            // returned as it is rather than half-migrated: a partially applied migration is
            // worse than an old document, because it looks current.
            break;
        };
        step(&mut out);
        out.schema += 1;
    }

    Ok((out, None))
}

/// True when this build can render every field the document carries.
#[must_use]
pub fn is_fully_understood(recipe: &Recipe) -> bool {
    recipe.schema <= SCHEMA_VERSION && recipe.extra.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::hash::recipe_hash;
    use serde_json::json;

    #[test]
    fn a_current_document_is_returned_unchanged() {
        let recipe = fixtures::reference();
        let (out, warning) = to_current(&recipe).expect("migrate");
        assert!(warning.is_none());
        assert_eq!(
            recipe_hash(&out).expect("out"),
            recipe_hash(&recipe).expect("in")
        );
    }

    #[test]
    fn a_newer_document_warns_and_keeps_every_field_it_does_not_understand() {
        let mut recipe = fixtures::reference();
        recipe.schema = SCHEMA_VERSION + 1;
        recipe.extra.insert(
            "film_emulation".to_string(),
            json!({ "stock": "portra_400", "strength": 0.4 }),
        );

        let (out, warning) = to_current(&recipe).expect("migrate");
        assert_eq!(
            warning.map(|e| e.code.to_string()),
            Some("AURA-RENDER-8003".to_string())
        );
        assert_eq!(
            out.extra.get("film_emulation"),
            recipe.extra.get("film_emulation"),
            "a newer build's work survives being opened by an older one"
        );
        assert!(!is_fully_understood(&out));
    }

    #[test]
    fn an_unknown_field_survives_a_serialisation_round_trip() {
        let mut recipe = fixtures::reference();
        recipe
            .extra
            .insert("grain".to_string(), json!({ "size": 2, "amount": 30 }));
        let text = crate::hash::canonical(&recipe).expect("canonical");
        let parsed: Recipe = serde_json::from_str(&text).expect("parse");
        assert_eq!(parsed.extra, recipe.extra);
    }

    #[test]
    fn the_deprecated_list_is_only_ever_appended_to() {
        // A guard rather than a behaviour: if a later phase deletes an entry from
        // DEPRECATED, this test's expected length stops matching and somebody has to say
        // why in a review. The number is the whole assertion.
        assert_eq!(DEPRECATED.len(), 0, "phase 14 deprecates nothing");
    }

    #[test]
    fn the_step_table_matches_the_schema_version() {
        assert_eq!(
            STEPS.len(),
            usize::from(SCHEMA_VERSION) - 1,
            "one step per version transition"
        );
    }
}
