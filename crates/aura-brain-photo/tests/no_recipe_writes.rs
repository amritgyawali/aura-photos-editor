//! A grep as a test: this crate reads the renderer and never writes a recipe. PHASE-16.
//!
//! Phase 14's rule is that `aura_recipe::schema::merge` is the only function in the workspace
//! that writes one recipe into another, and that it is where `user_edited_fields` is honoured.
//! Until phase 16, `aura-brain-photo` kept that rule by not depending on `aura-recipe` at all.
//!
//! Phase 16 changed the dependency graph: the skin guard measures what the **renderer** does
//! to a skin pixel, so this crate now depends on `aura-render` - and `aura-recipe` arrives
//! with it. ADR-0033 decision 1 has the argument, and its consequence is that the rule stops
//! being a property of `Cargo.toml` and needs a test.
//!
//! This is that test. It is `aura-render`'s own `colour_discipline.rs` in a new place, and for
//! the same reason: a rule enforced by a tool survives a hurried change, and a rule enforced
//! by a convention does not.

use std::fs;
use std::path::{Path, PathBuf};

/// Every `.rs` file under this crate's `src`.
fn sources() -> Vec<PathBuf> {
    fn walk(directory: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
        &mut out,
    );
    out.sort();
    out
}

#[test]
fn this_crate_never_writes_a_recipe() {
    // The four ways to write one. `merge` is phase 14's protected path, `save` and
    // `RecipeStore` are the catalog side of it, and `Provenance` is the field that records
    // who did it - all four belong to `aura-app`, which is the crate that can see both a
    // decision and a document.
    const FORBIDDEN: [&str; 4] = [
        "schema::merge",
        "RecipeStore",
        "recipe_store",
        "user_edited_fields",
    ];
    let mut offences: Vec<String> = Vec::new();
    for path in sources() {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            // Comments are exempt, and deliberately: this crate's headers explain phase 14's
            // rule at length, and a grep that could not tell an explanation from a call would
            // punish the documentation that makes the rule findable.
            if line.trim_start().starts_with("//") {
                continue;
            }
            for needle in FORBIDDEN {
                if line.contains(needle) {
                    offences.push(format!("{}:{}: {needle}", path.display(), number + 1));
                }
            }
        }
    }
    assert!(
        offences.is_empty(),
        "this crate must not write a recipe; `aura-app::colour_commands` is where a decision \
         becomes an edit:\n{}",
        offences.join("\n")
    );
}

#[test]
fn the_recipe_types_this_crate_does_touch_are_the_two_the_renderer_needs() {
    // `aura-recipe` is reachable, so the honest thing is to say exactly what is used and why
    // rather than to claim it is unused. `Curve` and `HslShift` are the two shapes
    // `aura_render::tonemap` takes as arguments; nothing else from that crate should appear.
    const ALLOWED: [&str; 2] = ["Curve", "HslShift"];
    let mut used: Vec<String> = Vec::new();
    for path in sources() {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("use aura_recipe") {
                continue;
            }
            for token in trimmed
                .trim_start_matches("use aura_recipe::")
                .trim_end_matches(';')
                .trim_matches(|c| c == '{' || c == '}')
                .split(',')
            {
                let name = token.trim().split(" as ").next().unwrap_or("").trim();
                if name.is_empty() || ALLOWED.contains(&name) {
                    continue;
                }
                used.push(format!("{}: {name}", path.display()));
            }
        }
    }
    assert!(
        used.is_empty(),
        "only the two shapes the renderer takes may be imported from `aura-recipe`:\n{}",
        used.join("\n")
    );
}
