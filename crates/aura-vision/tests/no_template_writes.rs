//! A grep as a test: this crate gained a catalog and must still not write biometrics.
//!
//! # What this replaces
//!
//! Phase 06 wrote, in `crates/aura-vision/src/lib.rs` and again in `face/mod.rs`:
//!
//! > Templates are biometric data, and everything durable about them - the envelope, the key,
//! > the project scoping, the erasure - lives in `aura-people`. This crate has no catalog
//! > dependency, so it *cannot* write one; the separation is structural rather than a rule
//! > people remember.
//!
//! Phase 18 put the mask store here, as section 4 of the phase document names it, so
//! `aura-vision` now depends on `aura-catalog` and that sentence has stopped being true. What
//! replaces it is this file. It is the third grep-as-a-test in the repository -
//! `crates/aura-render/tests/colour_discipline.rs` and
//! `crates/aura-brain-photo/tests/no_recipe_writes.rs` are the other two - and it exists for
//! the same reason both of those do: it catches the *second* module to break a rule, rather
//! than the first person to forget it.
//!
//! See `docs/adr/ADR-0037-semantic-masks-matting-and-quality-gating.md` decision 11.

use std::path::{Path, PathBuf};

/// Tables `aura-people` owns. Nothing in this crate may name any of them in a statement.
///
/// The face and identity tables from migration 6, plus the sealed crop store. A write to any of
/// them from here is a biometric record written outside the crate that owns consent, erasure
/// and the key.
const FORBIDDEN_TABLES: [&str; 6] = [
    "face_templates",
    "identities",
    "identity_faces",
    "face_crops",
    "biometric_keys",
    "decision_journal",
];

/// Statement verbs that write. A `SELECT` naming one of the tables above would be a read, and a
/// read is not the thing being prevented - `aura-people` is welcome to hand this crate a face.
const WRITE_VERBS: [&str; 5] = [
    "INSERT",
    "UPDATE",
    "DELETE FROM",
    "REPLACE INTO",
    "CREATE TABLE",
];

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            source_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn this_crate_writes_no_table_that_aura_people_owns() {
    let mut files = Vec::new();
    source_files(Path::new("src"), &mut files);
    assert!(!files.is_empty(), "no sources were scanned");

    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        let upper = text.to_uppercase();
        for table in FORBIDDEN_TABLES {
            let needle = table.to_uppercase();
            let Some(at) = upper.find(&needle) else {
                continue;
            };
            // The table is named somewhere. It is only a failure if a write verb appears in the
            // same statement, which for the SQL in this repository means the same string
            // literal - so the window is the 200 characters before the name.
            let start = at.saturating_sub(200);
            let window = upper.get(start..at).unwrap_or_default();
            for verb in WRITE_VERBS {
                assert!(
                    !window.contains(verb),
                    "{}: a `{verb}` statement names `{table}`, which belongs to aura-people",
                    file.display()
                );
            }
        }
    }
}

#[test]
fn only_the_mask_store_holds_a_catalog_handle() {
    // The dependency was added for one module. A second module reaching for the catalog is the
    // change this test is here to make visible in review, not to forbid outright - but it must
    // be a deliberate edit to this list rather than an import nobody noticed.
    const ALLOWED: [&str; 2] = ["mask\\store.rs", "mask/store.rs"];

    let mut files = Vec::new();
    source_files(Path::new("src"), &mut files);
    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        if !text.contains("aura_catalog::Catalog") {
            continue;
        }
        let shown = file.display().to_string();
        assert!(
            ALLOWED.iter().any(|a| shown.ends_with(a)),
            "{shown} holds a catalog handle; add it to ALLOWED here if that is intended"
        );
    }
}
