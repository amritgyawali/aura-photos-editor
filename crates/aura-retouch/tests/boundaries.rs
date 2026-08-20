//! Two greps as a test: this crate writes no recipe and opens no socket. PHASE-20.
//!
//! Phase 14 rule is that `aura_recipe::schema::merge` is the only function in the workspace
//! that writes one recipe into another, and phase 20 section 7 is one sentence: no cloud AI call
//! in this phase. Both are kept by `Cargo.toml` - neither `aura-recipe` nor `aura-cloud` is a
//! dependency - and both stop being properties of a manifest the moment somebody adds one in a
//! hurry.
//!
//! This is the fourth grep-as-a-test in the repository, after `colour_discipline.rs`,
//! `no_recipe_writes.rs` and `no_template_writes.rs`. A rule enforced by a tool survives a
//! hurried change; a rule enforced by a convention does not.
//!
//! The third check is this phase own, and it is the one worth having: **nothing in this crate
//! may reshape a body, lighten skin or name a skin-tone target.** Section 11 of
//! `docs/plan/CLAUDE.md` forbids all three permanently. The contract has nowhere to put them and
//! the schema has no column for them, so this is a third line rather than the only one - but it
//! is the line that catches a helper function added inside a solver.

use std::fs;
use std::path::{Path, PathBuf};

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

/// The manifest with its comments stripped.
///
/// The comments in `Cargo.toml` explain at length which crates are deliberately absent and name
/// them while doing it, so a grep over the raw file finds the very words it is looking for. What
/// matters is the dependency lines.
fn manifest() -> String {
    fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join(
            "
",
        )
}

#[test]
fn this_crate_never_writes_a_recipe() {
    assert!(
        !manifest().contains("aura-recipe"),
        "aura-retouch has acquired a dependency on aura-recipe; writing a recipe is phase 14 rule"
    );
    for path in sources() {
        let text = fs::read_to_string(&path).unwrap_or_default();
        for banned in ["schema::merge", "aura_recipe::"] {
            assert!(
                !text.contains(banned),
                "{} mentions {banned}: this crate decides and `aura-app` writes",
                path.display()
            );
        }
    }
}

#[test]
fn this_crate_reaches_no_network() {
    assert!(
        !manifest().contains("aura-cloud"),
        "aura-retouch has acquired a dependency on aura-cloud; section 7 says there is no cloud \
         call in this phase"
    );
    for path in sources() {
        let text = fs::read_to_string(&path).unwrap_or_default();
        for banned in ["TcpStream", "reqwest", "std::net", "CloudTask"] {
            assert!(
                !text.contains(banned),
                "{} mentions {banned}: no face crop leaves the device for retouching",
                path.display()
            );
        }
    }
}

#[test]
fn nothing_here_reshapes_a_person() {
    // The words a body-reshaping, skin-lightening or face-swapping feature would have to use.
    // `slim` and `smooth_skin` are the two that would most plausibly arrive as a helper inside a
    // solver, which is exactly the change this test exists to fail.
    const FORBIDDEN: [&str; 8] = [
        "fn reshape",
        "fn slim",
        "waist",
        "skin_lighten",
        "lighten_skin",
        "target_skin_tone",
        "ideal_skin",
        "face_swap",
    ];
    for path in sources() {
        let text = fs::read_to_string(&path).unwrap_or_default();
        for banned in FORBIDDEN {
            assert!(
                !text.contains(banned),
                "{} mentions {banned}: section 11 of docs/plan/CLAUDE.md forbids it permanently, \
                 and adding it needs a CTO-role ADR rather than a commit",
                path.display()
            );
        }
    }
}
