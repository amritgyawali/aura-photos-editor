//! The eleventh grep-as-a-test in this repository, after `colour_discipline.rs`,
//! `no_recipe_writes.rs` in `aura-brain-photo`, `no_render_calls.rs`, `one_choke_point.rs`,
//! `no_template_writes.rs`, `aura-brain-gallery`'s own `no_recipe_writes.rs`, `no_pixel_ops.rs`,
//! `no_decisions.rs`, `no_guarantee_learning.rs` and `aura-style`'s `no_network.rs`.
//!
//! Three properties, and the first is the one that matters.
//!
//! **A look is a proposal, never an application.** Phase 14's rule is that
//! `aura_recipe::schema::merge` is the only function in the workspace that writes one recipe
//! into another, and that a parameter a person set is never overwritten. This crate renders -
//! it has to, because a match that is not measured is not a match - but every render in it goes
//! into a *measurement* and none of them is stored. The temptation here is real and specific: a
//! look is a `StyleDelta`, `verify::shift` already produces the recipe it implies, and writing
//! that recipe is one line away. What would come out is AURA deciding a wedding looks like
//! somebody else's page.
//!
//! **No skin constant.** Phases 15, 16, 17 and 25 each scan for one. This is the fifth, and the
//! form it would take here is a hue window used to find skin in a stranger's photograph.
//!
//! **No text a model wrote.** There is no prompt, no completion and no free-text field
//! automation can fill, because migration 31 stores codes and the sentences are rendered.

use std::fs;
use std::path::Path;

#[test]
fn this_crate_never_writes_a_recipe() {
    scan(&[
        "schema::merge",
        "merge_recipe",
        "put_recipe",
        "save_recipe",
        "write_recipe",
        "RecipeStore",
    ]);
}

#[test]
fn this_crate_has_no_constant_it_could_compare_a_person_against() {
    // The names a skin target would arrive under. `skin_locus` is phase 15's own per-identity
    // measurement and is not one of them - but nothing in this crate reads it either, which is
    // why the list can be this blunt.
    scan(&[
        "SKIN_HUE",
        "SKIN_TARGET",
        "IDEAL_SKIN",
        "PREFERRED_SKIN",
        "skin_target",
        "skin_reference",
        "skin_bias:",
    ]);
}

#[test]
fn this_crate_has_nowhere_for_a_prompt_to_go() {
    scan(&["prompt", "completion", "system_message", "CloudTask"]);
}

#[test]
fn nothing_here_can_delete_or_move_a_photograph() {
    // Phase 12's rule: a decision is reversible and nothing on disk moves. This crate *reads*
    // files - it has to walk a reference folder - and it must never write one.
    scan(&[
        "fs::remove_file",
        "fs::remove_dir",
        "fs::rename",
        "fs::copy",
    ]);
}

/// Fail the build if any source file in this crate names one of these, outside a comment.
fn scan(forbidden: &[&str]) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut checked = 0_usize;
    walk(&src, &mut |path, text| {
        // `fixtures.rs` writes files by design - it is how a test gets a reference folder to
        // walk - and it is the one file exempt from the filesystem half of this scan. It is
        // exempt by name rather than by a marker comment, so adding a second writing module
        // needs this line changed and reviewed.
        if path.file_name().is_some_and(|name| name == "fixtures.rs") {
            return;
        }
        checked += 1;
        let code = strip_comments(text);
        for needle in forbidden {
            assert!(
                !code.contains(needle),
                "{} names {needle}, which this phase must not do",
                path.display()
            );
        }
    });
    assert!(checked > 5, "the scan found almost no files to check");
}

/// Everything before `//` on each line. Phase 27's lesson: a check that reads documentation as
/// if it were code fails hardest on the codebases that document themselves best, and every
/// module in this crate explains at length what it does not do.
fn strip_comments(text: &str) -> String {
    text.lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

fn walk(dir: &Path, visit: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            walk(&path, visit);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            if let Ok(text) = fs::read_to_string(&path) {
                visit(&path, &text);
            }
        }
    }
}
