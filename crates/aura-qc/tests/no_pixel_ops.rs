//! A grep as a test. PHASE-27, ADR-0055 section 11.
//!
//! The seventh in this repository, after `colour_discipline.rs`, `no_recipe_writes.rs` (twice),
//! `no_template_writes.rs`, `no_render_calls.rs` and `one_choke_point.rs`.
//!
//! ## Why this phase needs one more than the others
//!
//! Every earlier grep guards a crate against acquiring a capability it never had. This one guards a
//! crate against acquiring one it has the strongest possible reason to want.
//!
//! This is the first phase permitted to **undo** another phase's work. `Remedy::RevertOp` changes
//! what a delivered photograph looks like and `Remedy::ReplaceFrame` changes which photograph is
//! delivered at all - so the shortest path from a QC finding to a corrected gallery runs straight
//! through `aura_recipe::schema::merge`, and every argument for taking it will sound like
//! efficiency.
//!
//! It is the wrong path for the reason phase 14 wrote and eleven phases have inherited: `merge` is
//! the one function in the workspace that honours `user_edited_fields`, and a second caller is a
//! second place a photographer's own edit can be overwritten. What this crate produces is a
//! *decision*; `reedit::Remediator` is the port a caller implements to execute it, and the caller is
//! where the recipe is written.
//!
//! ## What is checked
//!
//! Five things, and each one is a capability rather than a spelling:
//!
//! 1. **No recipe writes.** `schema::merge`, `Recipe`, `edit_recipes`.
//! 2. **No renderer.** `aura_render`, `RenderService`, `render(`.
//! 3. **No pixels.** No file handle, no decode, no preview service. Every inspection is a
//!    comparison between numbers other phases already stored, which is what makes ten checks over a
//!    thousand frames affordable and each check a pure function.
//! 4. **No provider.** Phase 04's rule: `aura-cloud` is the only route to a model, and the planner
//!    goes through `CloudTask` rather than a socket.
//! 5. **No fixed skin target.** Phase 15's rule, in the phase that judges skin. There is no
//!    constant here a person's face could be compared against; every number is a distance from that
//!    person's own measured target.
//!
//! Each check names what a violation would *mean* rather than only what it matches, because the
//! next person to hit this test needs to know why the shortcut is wrong and not only that it is.

use std::fs;
use std::path::{Path, PathBuf};

/// Every `.rs` file in this crate's `src`, with its path.
fn sources() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    walk(&root, &mut out);
    assert!(
        out.len() >= 15,
        "expected the whole crate, found {} files",
        out.len()
    );
    out
}

fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            if let Ok(text) = fs::read_to_string(&path) {
                out.push((path, text));
            }
        }
    }
}

/// A source file with its comments and doc comments removed.
///
/// Every one of the forbidden names appears in prose in this crate - the module headers explain
/// exactly which capabilities are absent and why - so a grep over raw text would match its own
/// documentation. This is the same defect the first version of `checks::skin`'s inline grep had, and
/// the reason all six earlier greps in this repository scan files other than themselves.
fn code_only(text: &str) -> String {
    text.lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_absent(needles: &[&str], why: &str) {
    for (path, text) in sources() {
        let code = code_only(&text);
        for needle in needles {
            assert!(
                !code.contains(needle),
                "`{needle}` appears in {}.\n\n{why}",
                path.display()
            );
        }
    }
}

#[test]
fn this_crate_never_writes_a_recipe() {
    assert_absent(
        &["schema::merge", "aura_recipe", "edit_recipes"],
        "Phase 14's rule, twelfth application: `aura_recipe::schema::merge` is the one function in \
         the workspace permitted to write a recipe, because it is the one that honours \
         `user_edited_fields`. A second caller is a second place a photographer's own edit can be \
         overwritten silently.\n\n\
         This phase decides remedies. `reedit::Remediator` is the port a caller implements to \
         execute one, and the caller is where the recipe is written. That indirection is the whole \
         reason the loop's arithmetic can be driven by a test with no catalog and no renderer.",
    );
}

#[test]
fn this_crate_never_renders() {
    assert_absent(
        &["aura_render", "RenderService", "RenderLevel"],
        "`RenderService` is the only way to turn a recipe into pixels, and this phase turns nothing \
         into pixels. Every measurement it reads was made through the renderer by the phase that \
         owns the operation - phase 16's skin guard, phase 20's texture report, phase 22's artefact \
         self-check - which is what makes those numbers facts about a delivered photograph rather \
         than predictions about one.\n\n\
         A QC agent with its own renderer would be a second answer to what a photograph looks like.",
    );
}

#[test]
fn this_crate_never_opens_a_photograph() {
    assert_absent(
        &[
            "std::fs",
            "File::open",
            "aura_preview",
            "PreviewService",
            "aura_raw",
        ],
        "No check in this phase opens a photograph. Every inspection is a comparison between \
         numbers phases 08 to 26 already measured and stored, and that is what makes ten checks \
         over a thousand frames affordable inside section 11's 90 s budget.\n\n\
         It is also what makes each check a pure function a test can drive with a literal, which is \
         how `fixtures::defects` can inject twenty known problems without a single pixel.",
    );
}

#[test]
fn this_crate_never_reaches_a_provider() {
    assert_absent(
        &["reqwest", "TcpStream", "http://", "https://api."],
        "Phase 04's rule: `aura-cloud` is the only route to a model provider, and \
         `scripts/check-banned.sh` enforces it for the whole workspace. The planner in \
         `planner.rs` implements `CloudTask` and reaches nothing itself.\n\n\
         That indirection is also what gives the planner its safety property: an unreachable \
         provider, a spent budget and a malformed answer all produce the same outcome as a cautious \
         model, because `local_fallback` is an escalation.",
    );
}

#[test]
fn this_crate_holds_no_fixed_skin_target() {
    assert_absent(
        &[
            "ideal_skin",
            "IDEAL_SKIN",
            "REFERENCE_SKIN",
            "SKIN_TARGET_UV",
            "target_skin_uv",
        ],
        "Phase 15's rule, in the phase that judges skin: a skin target is measured, never assumed, \
         and the schema cannot express an alternative.\n\n\
         Every number in `checks::skin` is a distance from **that person's own** gallery target, \
         built by phase 25 from their own well-lit frames. A fixed target here is how an editor \
         lightens dark skin while believing it is correcting a cast - and the defence is that there \
         is no constant anywhere in this crate a face could be compared against.",
    );
}

#[test]
fn this_crate_grows_no_operator_of_its_own() {
    assert_absent(
        &["fn apply_", "fn blend_", "fn composite_", "fn inpaint"],
        "The remedies re-run phases 15 to 26; they do not add an eleventh way to change a \
         photograph. `Remedy` has five variants and no sixth, and there is no `Adjust { param, \
         value }` - a remedy that could *set* a parameter would make this phase a place a \
         photograph can be edited from.\n\n\
         What a remedy carries instead is a constraint, which narrows the deciding phase's own \
         solve rather than replacing it. The phase that owns an operation is the phase that decides \
         how strong it is.",
    );
}

#[test]
fn the_comment_stripper_really_strips_comments() {
    // The control, and it is not decoration. Phase 21's lesson: a refusal test that cannot tell a
    // working guard from a broken fixture proves nothing, and five tests that scan an empty string
    // pass exactly as loudly as five tests that scan a clean crate.
    //
    // Two assertions. `code_only` removes something from every real source file, and it removes
    // specifically the forbidden names where they appear in prose - the module headers in this
    // crate explain which capabilities are absent and why, using those exact names, so a stripper
    // that had stopped working would make the tests above fail rather than pass vacuously. This
    // proves the stripper is doing work rather than returning its input.
    let mut stripped_something = 0usize;
    let mut names_in_prose = 0usize;
    for (_path, text) in sources() {
        let code = code_only(&text);
        if code.len() < text.len() {
            stripped_something += 1;
        }
        if text.contains("aura_recipe") && !code.contains("aura_recipe") {
            names_in_prose += 1;
        }
    }
    assert!(
        stripped_something >= 15,
        "the stripper removed nothing from {} of the crate's files",
        sources().len() - stripped_something
    );
    assert!(
        names_in_prose >= 1,
        "no module header discusses the recipe writer it must not call, so the stripper is          untested against the case that matters"
    );
}
