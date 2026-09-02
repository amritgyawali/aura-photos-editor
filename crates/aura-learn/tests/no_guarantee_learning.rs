//! The tenth grep-as-a-test in this repository, after `colour_discipline.rs`, `no_recipe_writes.rs`
//! (twice), `no_template_writes.rs`, `no_render_calls.rs`, `one_choke_point.rs`, `no_pixel_ops.rs`,
//! `no_decisions.rs` and `no_outputs.rs`.
//!
//! ## What it protects
//!
//! `Learnable` is closed at fifteen members and every one of them is a **preference**. What is not
//! there is the guarantee: a mask boundary, a retouch texture floor, a crop safety margin, a
//! cleanup permission, a skin guard, an identity-drift cap, a coverage rule.
//!
//! A loop that could move one of those would learn its way past a promise, one wedding at a time,
//! with every gate green - and the phase that noticed would have no way to tell which weddings had
//! been delivered under a floor that had drifted. `docs/retouch-ethics.md` is a promise about the
//! product rather than a description of its defaults, and this is what keeps that true.
//!
//! ## Why a grep rather than a type
//!
//! The type does half of it: `Learnable` has no `Other` and a unit test in `aura-core` asserts that
//! no member's name contains one of eight words a guarantee is spelled with. What a grep adds is
//! the version where somebody adds a member *and* the code that applies it in one commit - which is
//! exactly the shape phase 28 wrote its `no_decisions.rs` for.

use std::fs;
use std::path::{Path, PathBuf};

fn crate_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every `.rs` file in this crate's `src/`, with inline test modules cut off.
///
/// The cut is `scripts/check-banned.sh`'s and it exists for the reason phase 27 wrote down twice: a
/// check that reads documentation and test code as if it were production code fails hardest on the
/// codebases that document themselves best.
fn sources() -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![crate_src()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read src") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_some_and(|e| e == "rs") {
                let text = fs::read_to_string(&path).expect("read file");
                let body = match text.find("#[cfg(test)]") {
                    Some(at) => text[..at].to_string(),
                    None => text,
                };
                out.push((path, strip_comments(&body)));
            }
        }
    }
    assert!(
        out.len() >= 8,
        "expected this crate's modules, got {}",
        out.len()
    );
    out
}

/// Strip `//` comments and doc comments, so a paragraph explaining why a thing is forbidden does
/// not read as the thing.
///
/// Phase 27 found this twice in one phase: a grep asserting the skin module holds no fixed skin
/// target matched its own test name, and a schema scan matched a migration's four paragraphs about
/// why there is no `diagnosis` column.
fn strip_comments(text: &str) -> String {
    text.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn nothing_in_this_crate_names_a_guarantee_it_could_learn() {
    // The eight words a guarantee is spelled with in this product. Every one of them names
    // something a phase from 18 to 25 promised, measured and enforced - and none of them is a
    // preference a photographer's habit should move.
    const FORBIDDEN: [&str; 10] = [
        "texture_floor",
        "identity_drift",
        "skin_target",
        "skin_guard",
        "crop_safety",
        "cleanup_policy",
        "mask_allowance",
        "coverage_rule",
        "tattoo",
        "retouch_ceiling",
    ];

    for (path, body) in sources() {
        for word in FORBIDDEN {
            assert!(
                !body.contains(word),
                "{} names `{word}`, which is a guarantee rather than a preference. \
                 See ADR-0061 decision 7 before adding it.",
                path.display()
            );
        }
    }
}

#[test]
fn this_crate_cannot_write_a_profile_a_recipe_or_a_photograph() {
    // A learning loop that could write directly into a style profile is a learning loop whose
    // adoption step is decorative. It computes an offset and stores a snapshot; applying one is
    // `aura-style`'s, through the frozen service, with a person's click in between.
    const FORBIDDEN: [&str; 7] = [
        "schema::merge",
        "aura_recipe",
        "aura_render",
        "aura_style",
        "RecipeStore",
        "StyleStore",
        "RenderService",
    ];

    for (path, body) in sources() {
        for word in FORBIDDEN {
            assert!(
                !body.contains(word),
                "{} reaches `{word}`; this crate proposes and never applies",
                path.display()
            );
        }
    }
}

#[test]
fn this_crate_opens_no_file_and_reaches_no_provider() {
    // Nothing here reads a photograph, writes one, or leaves the machine. Section 6.3: "all
    // learning is local by default", and contributing anonymised data is a separate, opt-in path
    // that is not built in this phase.
    const FORBIDDEN: [&str; 8] = [
        "std::fs",
        "File::open",
        "File::create",
        "TcpStream",
        "reqwest",
        "CloudAiGateway",
        "aura_cloud",
        "std::net",
    ];

    for (path, body) in sources() {
        for word in FORBIDDEN {
            assert!(
                !body.contains(word),
                "{} reaches `{word}`; this crate is arithmetic over stored rows",
                path.display()
            );
        }
    }
}

#[test]
fn only_one_statement_in_this_crate_sets_adopted() {
    // Section 10.1: "no learning update is adopted without explicit user action". The database
    // refuses an INSERT that arrives adopted; this is the other half - there is exactly one
    // UPDATE that sets the column, and it lives in `store::adopt`.
    let mut setters = Vec::new();
    for (path, body) in sources() {
        for line in body.lines() {
            let squashed: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
            if squashed.contains("SET adopted = 1") {
                setters.push(path.display().to_string());
            }
        }
    }
    assert_eq!(
        setters.len(),
        1,
        "expected exactly one statement that adopts an update, found {setters:?}"
    );
    assert!(
        setters[0].ends_with("store.rs"),
        "adoption belongs in the store, not in {}",
        setters[0]
    );
}

#[test]
fn the_split_is_drawn_from_a_hash_and_never_from_a_shuffle() {
    // The single easiest way for this feature to become a number generator, and the sort of thing
    // that gets added later by somebody making a test less flaky.
    const FORBIDDEN: [&str; 6] = [
        "shuffle",
        "thread_rng",
        "rand::",
        "SmallRng",
        "StdRng",
        "choose_multiple",
    ];
    for (path, body) in sources() {
        for word in FORBIDDEN {
            assert!(
                !body.contains(word),
                "{} reaches `{word}`; the held-out split is deterministic in the correction's \
                 own id. ADR-0061 decision 6.",
                path.display()
            );
        }
    }
}
