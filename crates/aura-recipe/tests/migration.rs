//! Section 10.1's last row: *"v1 recipes render correctly after a simulated schema v2
//! migration."*
//!
//! The document under test is byte-frozen in `tests/fixtures/recipe_v1_golden.json`. That
//! matters more than it looks: a migration tested against a recipe the current build
//! constructed is a migration tested against itself, and would keep passing while the
//! meaning of every field drifted.

#![allow(clippy::disallowed_methods)] // test code: an unwrap here is an assertion

use std::fs;
use std::path::Path;

use aura_recipe::{migrate, recipe_hash, sidecar, Recipe, SCHEMA_VERSION};
use serde_json::json;

fn golden_text() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("recipe_v1_golden.json");
    fs::read_to_string(path).expect("the frozen v1 document")
}

/// Re-write the frozen document from `fixtures::reference()`.
///
/// Ignored by default, and that is the whole design: a golden that regenerates itself on
/// every run is not a golden. Run it deliberately - `cargo test -p aura-recipe --test
/// migration -- --ignored bless` - when the schema changes, and expect to justify the diff
/// in review.
#[test]
#[ignore = "re-blesses the frozen v1 document; run deliberately"]
fn bless_the_frozen_v1_document() {
    let text = aura_recipe::canonical(&aura_recipe::fixtures::reference()).expect("canonical");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    fs::create_dir_all(&dir).expect("fixtures dir");
    fs::write(dir.join("recipe_v1_golden.json"), text.as_bytes()).expect("write");
}

#[test]
fn the_frozen_v1_document_parses_and_is_valid() {
    let recipe = sidecar::parse_checked(&golden_text()).expect("a valid v1 recipe");
    assert_eq!(recipe.schema, 1);
    assert_eq!(recipe.image.camera, "ILCE-7M4");
    assert_eq!(recipe.masks.len(), 2);
    assert_eq!(recipe.retouch.len(), 1);
}

#[test]
fn the_frozen_document_is_already_canonical() {
    // If this fails, either the canonical writer changed or somebody hand-edited the golden.
    // Both need a person to look, which is why the assertion is here rather than the golden
    // being re-canonicalised on read.
    let text = golden_text();
    let recipe: Recipe = serde_json::from_str(&text).expect("parse");
    assert_eq!(
        aura_recipe::canonical(&recipe).expect("canonical"),
        text.trim_end(),
        "the frozen v1 document must be the canonical form of itself"
    );
}

#[test]
fn migrate_v1_renders_identically_under_a_simulated_v2() {
    let recipe: Recipe = serde_json::from_str(&golden_text()).expect("parse");
    let before = recipe_hash(&recipe).expect("hash");

    // A v2 that adds a field this build does not know about. The v1 engine must render the
    // same pixels it always did, which means the same recipe hash over the fields it reads.
    let mut from_v2 = recipe.clone();
    from_v2.schema = SCHEMA_VERSION + 1;
    from_v2
        .extra
        .insert("film_emulation".to_string(), json!({"stock": "portra_400"}));

    let (migrated, warning) = migrate::to_current(&from_v2).expect("migrate");
    assert_eq!(
        warning.map(|e| e.code.to_string()),
        Some("AURA-RENDER-8003".to_string()),
        "a newer document is reported, not silently accepted"
    );

    // The unknown field survives.
    assert_eq!(
        migrated.extra.get("film_emulation"),
        from_v2.extra.get("film_emulation")
    );

    // And every field the v1 engine reads is untouched, so the render is unchanged.
    let mut as_v1 = migrated.clone();
    as_v1.schema = 1;
    as_v1.extra.clear();
    assert_eq!(recipe_hash(&as_v1).expect("hash"), before);
}

#[test]
fn a_v1_document_round_trips_through_a_newer_build_without_losing_anything() {
    // The other direction, and the one that actually costs a photographer work: a newer
    // build writes a field, an older build opens the project and saves it, and the newer
    // build opens it again.
    let recipe: Recipe = serde_json::from_str(&golden_text()).expect("parse");
    let mut newer = recipe;
    newer.schema = SCHEMA_VERSION + 1;
    newer.extra.insert(
        "grain".to_string(),
        json!({"size": 2, "amount": 30, "roughness": 0.4}),
    );

    let written = aura_recipe::canonical(&newer).expect("canonical");
    let reopened: Recipe = serde_json::from_str(&written).expect("an older build reads it");
    let rewritten = aura_recipe::canonical(&reopened).expect("and writes it back");

    assert_eq!(
        written, rewritten,
        "an older build must not destroy a newer build's work"
    );
}

#[test]
fn the_step_table_is_consistent_with_the_schema_version() {
    assert_eq!(migrate::STEPS.len(), usize::from(SCHEMA_VERSION) - 1);
    assert!(
        migrate::DEPRECATED.is_empty(),
        "phase 14 deprecates nothing"
    );
}

#[test]
fn a_current_document_migrates_to_itself() {
    let recipe: Recipe = serde_json::from_str(&golden_text()).expect("parse");
    let (out, warning) = migrate::to_current(&recipe).expect("migrate");
    assert!(warning.is_none());
    assert_eq!(
        recipe_hash(&out).expect("out"),
        recipe_hash(&recipe).expect("in")
    );
    assert!(migrate::is_fully_understood(&out));
}
