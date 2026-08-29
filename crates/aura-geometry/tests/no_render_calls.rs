//! A grep as a test. PHASE-23.
//!
//! **This crate decides a rectangle, an angle and a set of coefficients. It never applies
//! them.** `aura-render` applies them, from `edit_recipes` through `aura_recipe::schema::merge`
//! only, which is phase 14's rule and the only place `user_edited_fields` is honoured. A
//! dependency in this direction would let a geometry decision reach a pixel without going
//! through the merge, and two answers to "what does this photograph look like" is a gallery
//! that does not match the album.
//!
//! The fourth grep-as-a-test in the repository, after `colour_discipline.rs`,
//! `no_recipe_writes.rs` and `no_template_writes.rs`. It is a weaker guarantee than a
//! dependency that does not exist - `aura-geometry`'s manifest is what actually enforces it -
//! and this catches the change that adds the dependency along with the call.

use std::fs;
use std::path::Path;

/// Every symbol this crate may not name.
const FORBIDDEN: [&str; 6] = [
    "aura_render",
    "aura_recipe",
    "schema::merge",
    "RenderService",
    "RenderRequest",
    "PixelBuffer",
];

#[test]
fn no_module_in_this_crate_reaches_a_pixel_or_a_recipe() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offences = Vec::new();
    walk(&src, &mut |path, text| {
        for (number, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue; // A rule may be *written about* in a doc comment.
            }
            for needle in FORBIDDEN {
                if code.contains(needle) {
                    offences.push(format!("{}:{}: {needle}", path.display(), number + 1));
                }
            }
        }
    });
    assert!(
        offences.is_empty(),
        "phase 23 decides geometry and never applies it; `aura-render` owns the resample and \
         `aura_recipe::schema::merge` owns the write:\n  {}",
        offences.join("\n  ")
    );
}

#[test]
fn nothing_in_this_crate_names_a_face_detector_or_a_pose_model() {
    // The second half of the same rule, read from the other side. `ProtectedRegion` is the
    // input port phases 06 and 11 fill; a generator here would be a second answer to "where is
    // her face", and a crop that cuts one this product elsewhere insists it can see.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offences = Vec::new();
    walk(&src, &mut |path, text| {
        for (number, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            for needle in [
                "aura_vision",
                "aura_people",
                "detect_faces",
                "PeopleService",
            ] {
                if code.contains(needle) {
                    offences.push(format!("{}:{}: {needle}", path.display(), number + 1));
                }
            }
        }
    });
    assert!(
        offences.is_empty(),
        "phase 23 owns no face detector and no pose model:\n  {}",
        offences.join("\n  ")
    );
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
