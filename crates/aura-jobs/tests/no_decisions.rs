//! The eighth grep-as-a-test: an orchestrator with an opinion is not an orchestrator.
//!
//! After `colour_discipline.rs`, `aura-brain-photo/tests/no_recipe_writes.rs`,
//! `aura-vision/tests/no_template_writes.rs`, `aura-geometry/tests/no_render_calls.rs`,
//! `aura-generative/tests/one_choke_point.rs`, `aura-brain-gallery/tests/no_recipe_writes.rs` and
//! `aura-qc/tests/no_pixel_ops.rs`.
//!
//! ## What this is guarding
//!
//! This crate schedules twenty-five stages that other phases own. The moment it can express a
//! decision about a photograph, the product has two answers to "why was this frame delivered" and
//! one of them belongs to the scheduler - which is unfixable afterwards, because nothing records
//! which of the two a gallery came from.
//!
//! The manifest is the first line of defence: `aura-jobs` depends on `aura-core` and
//! `aura-catalog` and on none of the twenty-two deciding crates, so most of these are unreachable
//! by construction. This is the second line, and it catches the version that would compile - a
//! hand-rolled score, a threshold, a recipe write, a file opened, a socket.
//!
//! ## Why it strips comments first
//!
//! Phase 27 learned this twice in one phase: a check that reads documentation as if it were code
//! fails hardest on the codebases that document themselves best. This crate's own doc comments say
//! the words "keep", "score" and "confidence" repeatedly, because explaining what an orchestrator
//! must not do requires naming it.

use std::path::Path;

/// Every `.rs` file in the crate's source, with comments and string literals removed.
///
/// String literals go too, which is one more than the earlier greps strip. This crate's error
/// messages and reason texts are full of the vocabulary being banned - "waiting for you, because
/// AURA is not confident enough" - and a scan that read them would fail on the sentences that
/// exist to explain the rule.
fn sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    collect(&root, &mut out);
    out.into_iter()
        .map(|(name, text)| (name, strip(&text)))
        .collect()
}

fn collect(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push((path.display().to_string(), text));
            }
        }
    }
}

/// Remove line comments, block comments and double-quoted string literals.
///
/// A small state machine rather than a regular expression, because the thing being removed nests
/// and a regex that got the nesting wrong would silently delete code - which is a much worse
/// failure than the one this file exists to prevent. Phase 27's `strip_sql_comments` made the same
/// argument about block comments in SQL.
fn strip(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes: Vec<char> = text.chars().collect();
    let mut index = 0usize;
    let mut depth = 0usize;
    while index < bytes.len() {
        let current = bytes.get(index).copied().unwrap_or(' ');
        let next = bytes.get(index + 1).copied();

        if depth > 0 {
            if current == '*' && next == Some('/') {
                depth -= 1;
                index += 2;
                continue;
            }
            if current == '/' && next == Some('*') {
                depth += 1;
                index += 2;
                continue;
            }
            index += 1;
            continue;
        }

        if current == '/' && next == Some('*') {
            depth = 1;
            index += 2;
            continue;
        }
        if current == '/' && next == Some('/') {
            while index < bytes.len() && bytes.get(index).copied() != Some('\n') {
                index += 1;
            }
            continue;
        }
        if current == '"' {
            index += 1;
            while index < bytes.len() {
                let c = bytes.get(index).copied().unwrap_or('"');
                if c == '\\' {
                    index += 2;
                    continue;
                }
                index += 1;
                if c == '"' {
                    break;
                }
            }
            out.push(' ');
            continue;
        }
        out.push(current);
        index += 1;
    }
    out
}

fn assert_absent(needle: &str, why: &str) {
    for (name, text) in sources() {
        assert!(!text.contains(needle), "{name} contains `{needle}`. {why}");
    }
}

#[test]
fn the_orchestrator_never_writes_a_recipe() {
    // Phase 14's rule, thirteenth application. `aura_recipe::schema::merge` is the one function in
    // the workspace that honours `user_edited_fields`, and a scheduler that called it would be a
    // scheduler overwriting a photographer's own parameters.
    assert_absent(
        "schema::merge",
        "Writing a recipe is the deciding phase's job, executed through `aura-recipe`.",
    );
    assert_absent("aura_recipe", "This crate has no recipe dependency.");
}

#[test]
fn the_orchestrator_never_reaches_a_pixel() {
    for needle in ["aura_render", "aura_raw", "aura_preview", "aura_vision"] {
        assert_absent(
            needle,
            "An orchestrator that opened a photograph would be a phase, not a scheduler.",
        );
    }
}

#[test]
fn the_orchestrator_never_reaches_a_deciding_crate() {
    // Every one of these is absent from the manifest, so this is a second lock on a door that is
    // already bolted - and it is the lock that catches somebody adding the dependency and the call
    // in the same commit.
    for needle in [
        "aura_brain_photo",
        "aura_brain_wedding",
        "aura_brain_gallery",
        "aura_cull",
        "aura_qc",
        "aura_retouch",
        "aura_restore",
        "aura_geometry",
        "aura_generative",
        "aura_style",
        "aura_people",
        "aura_index",
        "aura_explain",
    ] {
        assert_absent(
            needle,
            "A stage is executed through `StageRunner`; the deciding crates are reached by \
             `aura-app` and never from here.",
        );
    }
}

#[test]
fn the_orchestrator_never_reaches_a_provider() {
    // Section 7 of the phase document: "No cloud AI call in this phase. The phase must work with
    // the network cable unplugged."
    for needle in ["aura_cloud", "reqwest", "TcpStream", "hyper"] {
        assert_absent(needle, "This phase makes no cloud call of its own.");
    }
}

#[test]
fn the_orchestrator_never_runs_a_model() {
    for needle in ["aura_infer", "aura_models", "InferService"] {
        assert_absent(
            needle,
            "Phase 03's rule: `InferService` is the only way to run a model, and a scheduler is \
             not a caller of it.",
        );
    }
}

#[test]
fn the_orchestrator_has_no_score_of_its_own() {
    // The version that would compile. A scheduler cannot reach a photograph, but it could grow a
    // number it called a score and then use it to decide something - which is exactly how a
    // twenty-eighth opinion about a wedding gets into the product.
    for needle in [
        "fn keep_score",
        "fn quality_score",
        "fn confidence",
        "keep_score",
        "raw_confidence",
    ] {
        assert_absent(
            needle,
            "The orchestrator sequences work; it does not judge it.",
        );
    }
}

#[test]
fn no_shape_in_this_crate_can_hold_a_decision_about_a_photograph() {
    // Field names rather than functions. A `RunProgress` that gained a `keep: bool` would compile,
    // would look harmless, and would be the scheduler holding a copy of phase 12's answer.
    //
    // `pub threshold` is deliberately **not** on this list, and the first version of this test had
    // it. `ResourceEvent::threshold` is degrees Celsius: it is what the governor compared a
    // temperature against, and banning the word rather than the meaning failed the build on the
    // one piece of code in this crate that is unambiguously about hardware. Phases 19, 21, 22 and
    // 25 each shipped a threshold a correct implementation could not satisfy; this is the same
    // family - a check that flags correct code is as broken as one that misses a defect, and the
    // fix in every case was to state the check against what it actually guards.
    for needle in [
        "PhotoId",
        "pub keep",
        "pub rejected",
        "pub strength",
        "pub score",
        "keep_hint",
    ] {
        assert_absent(
            needle,
            "A run is about stages. `ImageId` appears in `RunProgress::current_image` for the \
             thumbnail and nothing else.",
        );
    }
}

#[test]
fn the_orchestrator_grants_itself_no_autonomy() {
    // `StageVerdict::from_band` is the only constructor of a verdict that runs, and its input is
    // the gate's. A crate that computed a band would be a crate one edit away from acting on a
    // wedding unattended.
    //
    // What is banned is the *machinery of calibration*, not the word "band". `AutonomyGate::band`
    // is the port's own declaration and `FixedGate::band` is a fixture answering it - both are the
    // design working, and a grep for `fn band(` flags exactly them. The three needles below are
    // the things that could only be present if this crate had started deriving a band from a
    // confidence, which is the actual failure.
    for needle in [
        "AutonomyPolicy",
        "calibration_ver",
        "isotonic",
        "autonomy_bands",
    ] {
        assert_absent(
            needle,
            "A band comes from `AutonomyGate` and is never computed here. Phase 13 owns \
             calibration; this crate owns the order stages run in.",
        );
    }
}

#[test]
fn the_orchestrator_opens_no_file_of_its_own() {
    // Section 12 of the phase document: cancellation must leave no partial exports. The strongest
    // form of that is a scheduler that cannot write a file at all.
    for needle in ["File::create", "fs::write", "OpenOptions", "fs::remove"] {
        assert_absent(
            needle,
            "Writing a delivered file is phase 30's, and a scheduler that could write one could \
             leave half of one behind.",
        );
    }
}

#[test]
fn the_strip_leaves_code_and_removes_prose() {
    // The scan is only worth anything if it reads code. Phase 27 shipped two greps that matched
    // their own documentation; this asserts the stripper does the opposite of that mistake as well
    // as that one.
    let text = "// keep_score in a comment\nlet a = 1; /* keep_score */ let b = \"keep_score\";";
    let stripped = strip(text);
    assert!(!stripped.contains("keep_score"), "{stripped}");
    assert!(stripped.contains("let a = 1;"), "{stripped}");
    assert!(stripped.contains("let b ="), "{stripped}");
}

#[test]
fn the_strip_handles_nested_block_comments() {
    let stripped = strip("let a = 1; /* outer /* inner */ still */ let b = 2;");
    assert!(!stripped.contains("inner"));
    assert!(stripped.contains("let b = 2;"), "{stripped}");
}
