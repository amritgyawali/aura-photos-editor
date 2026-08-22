//! Three greps as a test: this crate writes no recipe, opens no socket, and cannot upscale.
//! PHASE-22.
//!
//! Phase 14's rule is that `aura_recipe::schema::merge` is the only function in the workspace
//! that writes one recipe into another, and phase 22 section 7 is one sentence: "No cloud AI call
//! in this phase. The phase must work with the network cable unplugged." Both are kept by
//! `Cargo.toml` - neither `aura-recipe` nor `aura-cloud` is a dependency - and both stop being
//! properties of a manifest the moment somebody adds one in a hurry.
//!
//! This is the fifth grep-as-a-test in the repository, after `colour_discipline.rs`,
//! `no_recipe_writes.rs`, `no_template_writes.rs` and `aura-retouch`'s own `boundaries.rs`. A
//! rule enforced by a tool survives a hurried change; a rule enforced by a convention does not.
//!
//! The cloud check matters more here than it did in phase 20, and the reason is what an offload
//! would *send*. Phase 20 would have sent a face crop. This phase would send the photograph - a
//! 45 MP linear buffer, which is a derivative of a RAW in the same sense that a print is. Section
//! 9 of `docs/plan/CLAUDE.md` says to send derivative data and never originals, and ADR-0045
//! section 7 records why the consent design that would make it acceptable is larger than this
//! phase.
//!
//! The third check is this phase's own. Section 2.2 puts **upscaling beyond native resolution**
//! and **generative reconstruction** out of scope for V1, and both exclusions are structural: the
//! contract has nowhere to put a scale factor or a synthesised region and migration 22 has no
//! column for either. This is a third line rather than the only one - but it is the line that
//! catches a helper function added inside a solver.

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
/// **The grep is about code, not about prose.** Every module in this crate explains at length
/// which things it deliberately does not do, and naming them is the whole value of the
/// explanation - `decide.rs`'s header says that only `aura_recipe::schema::merge` writes a
/// recipe, which is exactly the token this test is looking for. A grep that could be silenced by
/// deleting an explanation is a grep that discourages explaining.
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
        "aura-restore has acquired a dependency on aura-recipe; writing a recipe is phase 14's rule"
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
            "aura-restore has acquired `{banned}`; PHASE-22 section 7 says the gateway stays idle"
        );
    }
    for path in sources() {
        let text = code(&path);
        // The tokens are written without their trailing path separator for a reason that is not
        // cosmetic: `scripts/check-banned.sh` greps every source file in the workspace for the
        // fully-qualified forms, and a test that spelled them out would trip the very lint it
        // exists to reinforce. Phase 20's `boundaries.rs` made the same choice.
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
fn nothing_in_this_crate_can_upscale_or_synthesise() {
    // Section 2.2's two exclusions, as a grep. The contract has nowhere to put either and
    // migration 22 has no column for either; this catches the helper somebody adds inside a
    // solver, which is the layer neither of those two reaches.
    //
    // `resample` is on the list and `upsample` is not: `decide::upsample` turns phase 18's
    // coarse region grid into a per-pixel plane, which is a mask being delivered at frame
    // resolution rather than a photograph being enlarged. The distinction is exactly the one
    // section 2.2 draws - nothing here changes how many pixels the *photograph* has.
    for path in sources() {
        let text = code(&path);
        for banned in [
            "fn upscale",
            "fn super_resolve",
            "fn resample",
            "fn synthesise",
            "fn synthesize",
            "fn inpaint",
            "fn generate_pixels",
            "output_scale",
            "scale_factor",
        ] {
            assert!(
                !text.contains(banned),
                "{} mentions `{banned}`; section 2.2 puts upscaling and generative \
                 reconstruction out of scope for this phase",
                path.display()
            );
        }
    }
}

#[test]
fn every_shipped_noise_model_file_is_compiled_into_the_binary() {
    // Not a boundary but the same kind of guard, and it belongs beside them: a camera file added
    // to `config/noise_models/` and forgotten in `profiles.rs` is a body that silently falls back
    // to the reference model. That failure is invisible - the denoiser still works, it just works
    // on the wrong sensor - so it needs a test rather than a review.
    let directory = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/config/noise_models"));
    let source = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/profiles.rs"))
        .unwrap_or_default();

    let mut on_disk = Vec::new();
    for entry in fs::read_dir(directory).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "toml") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                on_disk.push(stem.to_string());
            }
        }
    }
    assert!(
        !on_disk.is_empty(),
        "there are no camera noise models on disk at all"
    );
    // Matched on the include path rather than on the `("slug", include_str!(..))` pair, because
    // `cargo fmt` wraps a long entry across three lines and a test that depended on the pair
    // being on one line would fail on formatting rather than on a missing body.
    for slug in &on_disk {
        assert!(
            source.contains(&format!("../config/noise_models/{slug}.toml")),
            "`{slug}.toml` is on disk and is not in EMBEDDED_NOISE, so that body silently \
             denoises against the reference model"
        );
    }

    // And the reverse: an entry naming a file that is not there would not compile, so the only
    // thing left to check is that the count agrees.
    let compiled = source
        .matches("include_str!(\"../config/noise_models/")
        .count();
    assert_eq!(
        compiled,
        on_disk.len(),
        "EMBEDDED_NOISE has {compiled} entries and the directory has {}",
        on_disk.len()
    );
}
