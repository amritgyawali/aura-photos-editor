//! A grep as a test. The sixth in the repository, after `colour_discipline.rs`,
//! `aura-brain-photo`'s `no_recipe_writes.rs`, `aura-vision`'s `no_template_writes.rs`,
//! `aura-geometry`'s `no_render_calls.rs` and `aura-generative`'s `one_choke_point.rs`.
//!
//! Five properties, all of them about what this crate must **not** do.
//!
//! **It writes no recipe.** `aura_recipe::schema::merge` is the one function in the workspace
//! permitted to write a recipe and the only place `user_edited_fields` is honoured. A gallery delta
//! reaches a pixel through `aura-app` calling that merge, and a call from here would be a second
//! way to edit a photograph - which is exactly the failure phase 14's rule exists to prevent.
//!
//! **It opens no file.** `PreviewService` is the one route to pixels, the rule since phase 09, and
//! a crate that read a proxy off disk would be a second decoder producing a second answer about the
//! same photograph.
//!
//! **It reaches no provider.** Section 7 says no cloud call happens in this phase and the gateway
//! stays idle. `aura-cloud` is absent from `Cargo.toml`, which makes it a property of the
//! dependency graph; this catches the case where somebody adds it back.
//!
//! **It keeps no tone solver of its own.** Everything this phase knows about the light in one
//! photograph comes through `ToneService`, and a second illuminant estimator here would be a second
//! answer to "what temperature was this room" - phase 15's rule, and the one that decides whether
//! an album matches its gallery.
//!
//! **It has no ideal-skin constant.** Section 6.3's fairness argument is that a fixed target is how
//! an editor lightens dark skin while believing it is correcting a cast, and the defence is that
//! nothing in this code path has a constant it could compare a person against. Phase 15 wrote it;
//! this is where the same scan runs for this crate.
//!
//! A grep is weaker than a type. It is what is available for "this crate does not do X" when X is
//! reachable through a dependency the crate legitimately has, and the exit report says so.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(clippy::panic, clippy::float_cmp, clippy::disallowed_methods)]
#![allow(clippy::uninlined_format_args, clippy::assertions_on_constants)]

use std::fs;
use std::path::{Path, PathBuf};

/// Every `.rs` file in this crate's `src/`, in a stable order.
fn sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    collect(&root, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The body of a file with its comments and doc comments removed.
///
/// Every one of the five scans below is about *code*, and every one of the five things being
/// scanned for is named in a doc comment in this crate somewhere - the module headers explain what
/// they do not do. A scan over raw text would fail on its own documentation, which is the classic
/// way a grep-as-a-test gets deleted three months later.
fn code_of(path: &Path) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn this_crate_never_writes_a_recipe() {
    for path in sources() {
        let code = code_of(&path);
        for needle in ["schema::merge", "aura_recipe", "EditRecipe"] {
            assert!(
                !code.contains(needle),
                "{} mentions `{needle}`. A gallery delta reaches a pixel through \
                 `aura_recipe::schema::merge` called from `aura-app`, and a call from here would \
                 be a second way to edit a photograph. ADR-0051 section 8.",
                path.display()
            );
        }
    }
}

#[test]
fn this_crate_never_opens_a_file_of_its_own() {
    for path in sources() {
        let code = code_of(&path);
        for needle in ["File::open", "fs::File", "std::fs::read(", "image::open"] {
            assert!(
                !code.contains(needle),
                "{} mentions `{needle}`. `PreviewService` is the one route to pixels, the rule \
                 since phase 09, and a second decoder is a second answer about the same \
                 photograph.",
                path.display()
            );
        }
        // `fs::read_to_string` is legitimate in exactly two places, and both of them read a
        // *settings* file rather than a photograph: phase 25's policy loader reading
        // `consistency.toml`, phase 26's reading `camera_match.toml`, and phase 26's baseline
        // loader reading a studio's own `assets/camera_baselines/<brand>.toml`. None of them can
        // reach a pixel - a TOML file has no image in it - and the ban this test enforces is about
        // a second decoder.
        if code.contains("read_to_string") {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            assert!(
                matches!(name, "policy.rs" | "baseline.rs"),
                "{} reads a file. Only the two settings loaders may, and only their own tables.",
                path.display()
            );
        }
    }
}

#[test]
fn this_crate_never_reaches_a_provider() {
    for path in sources() {
        let code = code_of(&path);
        for needle in [
            "aura_cloud",
            "CloudTask",
            "TcpStream",
            "http://",
            "https://",
            "reqwest",
        ] {
            assert!(
                !code.contains(needle),
                "{} mentions `{needle}`. Section 7: no cloud AI call happens in this phase and the \
                 gateway stays idle.",
                path.display()
            );
        }
    }
}

#[test]
fn this_crate_keeps_no_tone_solver_of_its_own() {
    for path in sources() {
        let code = code_of(&path);
        for needle in [
            "aura_brain_photo",
            "aura_brain_wedding",
            "grey_world",
            "white_patch",
            "estimate_illuminant",
        ] {
            assert!(
                !code.contains(needle),
                "{} mentions `{needle}`. What colour the light was comes through `ToneService` \
                 and through nothing else; a second estimator here is a second answer to \
                 'what temperature was this room'. Phase 15's rule.",
                path.display()
            );
        }
        // The locus *conversions* are a different thing from a locus *estimator*, and phase 26 is
        // where that distinction had to be drawn. `cct_to_uv` and `cct_from_uv` turn one
        // representation of a chromaticity into another; a camera transform has to move a
        // chromaticity along the Planckian locus, and the alternative to calling the shared
        // conversion is writing a second copy of it - which is the thing this file exists to
        // prevent rather than an example of it. What stays banned is *defining* one here.
        for needle in ["fn cct_to_uv", "fn cct_from_uv", "fn uv_to_cct"] {
            assert!(
                !code.contains(needle),
                "{} defines `{needle}`. The locus arithmetic has one implementation, in \
                 `aura_raw::colour::illuminant`, beside the camera matrices and the monotone \
                 curve - phase 23's rule for the lens polynomial, and two copies of a colour \
                 conversion is two answers about the same photograph.",
                path.display()
            );
        }
    }
}

#[test]
fn this_crate_has_no_ideal_skin_constant() {
    for path in sources() {
        let code = code_of(&path);
        for needle in [
            "IDEAL_SKIN",
            "REFERENCE_SKIN",
            "TARGET_SKIN_UV",
            "SKIN_REFERENCE",
            "CAUCASIAN",
            "monk_scale",
            "skin_tone_bucket",
        ] {
            assert!(
                !code.contains(needle),
                "{} mentions `{needle}`. A skin target is measured from that person's own frames, \
                 never assumed, and a code path with no constant to compare a person against \
                 cannot lighten one toward a reference. Section 6.3 and `docs/skin-fairness.md`.",
                path.display()
            );
        }
    }
}

#[test]
fn the_scan_reads_something() {
    // A grep that matched no files would pass every assertion above. Phase 21's lesson: a refusal
    // test that cannot tell a working guard from an empty input proves nothing.
    let files = sources();
    assert!(
        files.len() >= 12,
        "only {} source files were scanned; the walk is broken",
        files.len()
    );
    assert!(files.iter().any(|path| path.ends_with("normalise.rs")));
    // And the comment stripper must not strip code.
    let code = code_of(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/normalise.rs"));
    assert!(code.contains("pub fn solve"), "the stripper ate the code");
}
