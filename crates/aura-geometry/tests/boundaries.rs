//! Four greps as a test: this crate writes no recipe, opens no socket, cannot upscale, and keeps
//! no face detector of its own. PHASE-23.
//!
//! Phase 14's rule is that `aura_recipe::schema::merge` is the only function in the workspace that
//! writes one recipe into another, and PHASE-23 section 7 is one sentence: "No cloud AI call in
//! this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from
//! Phase 04 stays idle here." Both are kept by `Cargo.toml` - neither `aura-recipe` nor
//! `aura-cloud` is a dependency - and both stop being properties of a manifest the moment somebody
//! adds one in a hurry.
//!
//! This is the sixth grep-as-a-test in the repository, after `colour_discipline.rs`,
//! `no_recipe_writes.rs`, `no_template_writes.rs` and the `boundaries.rs` files in `aura-retouch`
//! and `aura-restore`. A rule enforced by a tool survives a hurried change; a rule enforced by a
//! convention does not.
//!
//! ## The fourth check is this phase's own, and it is the important one
//!
//! `aura-vision` is deliberately absent from the manifest even though this crate measures line
//! structure, because everything it needs about *people* arrives through phase 06's frozen
//! `PeopleService` and everything it needs about *pixels* is a gradient. A dependency there would
//! put a face detector one import away from a crop solver, and **the one thing this phase must
//! never do is find its own faces**: a crop is checked against the faces phase 06 found, and a
//! second detector is a second answer to "is there somebody at the edge of this frame". The two
//! answers would disagree on exactly the frames where it matters - small faces at the boundary -
//! and the delivered photograph would be the one where the second detector was wrong.

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
        .join("\n")
}

/// One source file with its comments stripped, for the same reason the manifest is.
///
/// **The grep is about code, not about prose.** Every module in this crate explains at length what
/// it deliberately does not do, and naming the thing is the whole value of the explanation -
/// `decide.rs`'s header says that only `aura_recipe::schema::merge` writes a recipe, which is
/// exactly the token this test looks for. A grep that could be silenced by deleting an explanation
/// is a grep that discourages explaining.
fn code(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| match line.find("//") {
            Some(index) => line.get(..index).unwrap_or_default(),
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn this_crate_never_writes_a_recipe() {
    assert!(
        !manifest().contains("aura-recipe"),
        "aura-geometry has acquired a dependency on aura-recipe; writing a recipe is phase 14's \
         rule and lives in aura-app"
    );
    for path in sources() {
        let text = code(&path);
        for banned in ["schema::merge", "aura_recipe::"] {
            assert!(
                !text.contains(banned),
                "{} mentions `{banned}`; only `aura-app` writes a recipe",
                path.display()
            );
        }
    }
}

#[test]
fn this_crate_never_opens_a_socket() {
    for banned in [
        "aura-cloud",
        "reqwest",
        "hyper",
        "ureq",
        "tokio-tungstenite",
    ] {
        assert!(
            !manifest().contains(banned),
            "aura-geometry has acquired `{banned}`; PHASE-23 section 7 says the gateway stays idle"
        );
    }
    for path in sources() {
        let text = code(&path);
        // Written without their trailing path separator for the reason phases 20 and 22 gave:
        // `scripts/check-banned.sh` greps every source file in the workspace for the
        // fully-qualified forms, and a test that spelled them out would trip the very lint it
        // exists to reinforce.
        for banned in [
            "TcpStream",
            "std::net",
            "aura_cloud",
            "reqwest",
            "CloudTask",
        ] {
            assert!(
                !text.contains(banned),
                "{} mentions `{banned}`; this phase works with the cable unplugged",
                path.display()
            );
        }
    }
}

#[test]
fn nothing_in_this_crate_can_upscale_or_fill() {
    // Section 2.2's exclusions, as a grep. The contract has nowhere to put a scale or a
    // synthesised region and migration 23 has no column for either; this catches the helper
    // somebody adds inside a solver, which is the layer neither of those two reaches.
    //
    // `fn resample` is on the list and this crate genuinely has none: every resample in this phase
    // belongs to `aura-render`, which is what makes "geometry is applied once, at high quality,
    // inside the render graph" - section 13's last acceptance criterion - a property of the
    // dependency graph rather than a promise.
    for path in sources() {
        let text = code(&path);
        for banned in [
            "fn upscale",
            "fn super_resolve",
            "fn resample",
            "fn synthesise",
            "fn synthesize",
            "fn inpaint",
            "fn fill_corner",
            "fn generate_pixels",
            "output_scale",
            "scale_factor",
        ] {
            assert!(
                !text.contains(banned),
                "{} mentions `{banned}`; section 2.2 puts fill in phase 24 and this phase can \
                 only ever remove pixels",
                path.display()
            );
        }
    }
}

#[test]
fn this_crate_keeps_no_face_detector_of_its_own() {
    // The check that matters most in this phase. See the module header: a crop is checked against
    // the faces phase 06 found, and a second detector is a second answer to "is there somebody at
    // the edge of this frame".
    assert!(
        !manifest().contains("aura-vision"),
        "aura-geometry has acquired a dependency on aura-vision; faces arrive through phase 06's \
         frozen PeopleService and nowhere else"
    );
    for banned in ["aura-brain-photo", "aura-brain-wedding", "aura-people"] {
        assert!(
            !manifest().contains(banned),
            "aura-geometry has acquired `{banned}`; everything this phase needs from phases 06, \
             07, 08 and 11 arrives through the frozen traits in aura-core"
        );
    }
    for path in sources() {
        let text = code(&path);
        for banned in [
            "fn detect_face",
            "fn find_faces",
            "FaceDetector",
            "aura_vision::",
            "InferService",
        ] {
            assert!(
                !text.contains(banned),
                "{} mentions `{banned}`; phase 06's PeopleService is the only way to ask who is \
                 in a photograph",
                path.display()
            );
        }
    }
}

#[test]
fn this_phase_ships_no_model() {
    // The third phase in the product with no model at all, after 08 and 17. Section 9's MLL row
    // asks for an *objective spec* rather than a network, the four terms of that objective have
    // closed forms, and what is missing is expert crop labels rather than weights.
    //
    // A grep rather than a sentence in a document, because the failure mode is silent: a model
    // added here would need a card, a signature, a `model_ver` column and an entry in
    // `models.lock`, and none of those exist for this phase. The first three would be noticed;
    // the fourth would be a version-drift code nobody could raise.
    assert!(
        !manifest().contains("aura-infer") && !manifest().contains("aura-models"),
        "aura-geometry has acquired an inference dependency; this phase ships no model and \
         `geometry_plan` has no `model_ver` column to invalidate one with"
    );
    for path in sources() {
        let text = code(&path);
        for banned in ["MODEL_VER", "model_ver", "ModelId", "session_pool"] {
            assert!(
                !text.contains(banned),
                "{} mentions `{banned}`; this phase has two version columns and neither is a \
                 model",
                path.display()
            );
        }
    }
}

#[test]
fn the_shipped_lens_table_is_reachable_and_says_it_was_not_measured() {
    // Not a boundary but the same kind of guard, and it belongs beside them: a lens profile
    // directory that stops compiling into the binary is a build where every frame silently falls
    // through to the estimator. That failure is invisible - the product still works, it just
    // corrects nothing - so it needs a test rather than a review.
    let database = aura_render::geometry::database();
    assert!(
        !database.rows.is_empty(),
        "the bundled lens table did not parse, so no lens in this build can be corrected"
    );
    assert!(
        database.is_all_reference(),
        "a row in `assets/lens_profiles/profiles.toml` calls itself measured; nothing in this \
         repository was measured and `ATTRIBUTION.md` says so"
    );
    // And the attribution file is there, because a profile database without one acquires an
    // authority it never earned the second time somebody ships it.
    let attribution = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/lens_profiles/ATTRIBUTION.md"
    ));
    assert!(
        attribution.exists(),
        "assets/lens_profiles/ATTRIBUTION.md is missing"
    );
}
