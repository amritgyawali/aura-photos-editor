//! Invariant 8 as a grep.
//!
//! ADR-0029 section 2: *"The output transform is the only tone curve. No stage before it
//! applies a display transfer function."* That sentence is only worth writing if something
//! checks it, and this is the something. It reads the crate's own source at test time and
//! fails when a second module starts encoding.
//!
//! `tonemap::curve_domain_encode` is not a counter-example and the test knows why: it is an
//! invertible change of coordinates applied and undone inside one stage, and this file looks
//! for the three *output* encoders by name rather than for the word "encode".

use std::fs;
use std::path::{Path, PathBuf};

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Strip `#[cfg(test)]` blocks by the crude but sufficient rule that everything after the
/// first `mod tests {` in a file is test code. Every module in this crate puts its tests
/// last, and a test that asserts an encoder's output is not a stage that bakes tone.
fn production_source(path: &Path) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    match text.find("mod tests {") {
        Some(at) => text[..at].to_string(),
        None => text,
    }
}

#[test]
fn only_output_encodes_a_transfer_function() {
    let mut files = Vec::new();
    rust_files(&src(), &mut files);
    assert!(files.len() > 8, "found only {} source files", files.len());

    for path in files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let source = production_source(&path);
        for encoder in ["fn encode_srgb", "fn encode_gamma_2_2", "fn encode_for"] {
            if source.contains(encoder) {
                assert_eq!(
                    name, "output.rs",
                    "{name} defines {encoder}; only the output transform may bake tone"
                );
            }
        }
    }
}

#[test]
fn only_output_and_the_tiler_quantise() {
    // `tiles.rs` calls `output::transform` per tile, which is the whole design, so it is
    // allowed to *call* the quantiser. What no module may do is write 8-bit samples of its
    // own, which is what `RenderedData::Eight(` being constructed outside `output.rs` would
    // mean.
    let mut files = Vec::new();
    rust_files(&src(), &mut files);
    for path in files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let source = production_source(&path);
        if source.contains("RenderedData::Eight(vec!") {
            assert!(
                name == "output.rs" || name == "tiles.rs",
                "{name} builds an 8-bit buffer of its own"
            );
        }
    }
}

#[test]
fn no_module_defines_its_own_working_space() {
    // Invariant 8's other half: one working space, defined once, in `aura-raw`. A second
    // set of primaries anywhere in this crate is a second renderer.
    let mut files = Vec::new();
    rust_files(&src(), &mut files);
    for path in files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let source = production_source(&path);
        if source.contains("REC2020_TO_XYZ") {
            assert!(inherits_working_primaries(&source), "{name} must inherit the working-space primaries from aura_raw and cannot redeclare them");
        }
    }
}

/// This remains a lexical guard, but checks matrix ownership instead of filenames.
/// Perceptual coordinates may use the canonical matrix without defining another space.
fn inherits_working_primaries(source: &str) -> bool {
    let code = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    if !code.contains("REC2020_TO_XYZ") {
        return true;
    }
    let tokens: Vec<_> = code
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|token| !token.is_empty())
        .collect();
    for (index, token) in tokens.iter().enumerate() {
        if matches!(*token, "const" | "static" | "let") {
            let mut next = index + 1;
            if tokens.get(next) == Some(&"mut") {
                next += 1;
            }
            if tokens
                .get(next)
                .is_some_and(|name| name.starts_with("REC2020_TO_XYZ"))
            {
                return false;
            }
        }
    }
    let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.contains("aura_raw::colour::matrix::REC2020_TO_XYZ_D65") {
        return true;
    }
    compact.split(';').any(|item| {
        let Some(start) = item.rfind("useaura_raw::colour::") else {
            return false;
        };
        let import = &item[start..];
        import.contains("REC2020_TO_XYZ_D65")
            && (import.starts_with("useaura_raw::colour::matrix::{")
                || (import.starts_with("useaura_raw::colour::{") && import.contains("matrix::{")))
    })
}

#[test]
fn canonical_matrix_imports_are_allowed_in_perceptual_helpers() {
    for source in [
        "/// The coefficients come from REC2020_TO_XYZ_D65.\nfn unrelated() {}",
        "use aura_raw::colour::matrix::{apply, REC2020_TO_XYZ_D65};",
        "use aura_raw::colour::{matrix::{apply, REC2020_TO_XYZ_D65, WHITE_D65}, working_space::xyz_d65_to_rec2020};",
        "apply(aura_raw::colour::matrix::REC2020_TO_XYZ_D65, rgb)",
    ] { assert!(inherits_working_primaries(source), "{source}"); }
}

#[test]
fn local_primaries_are_rejected_even_alongside_an_unrelated_canonical_import() {
    for declaration in [
        "const REC2020_TO_XYZ_D65: Mat3 = [[0.0; 3]; 3];",
        "static mut REC2020_TO_XYZ_D65: Mat3 = [[0.0; 3]; 3];",
        "let mut REC2020_TO_XYZ_D65 = [[0.0; 3]; 3];",
    ] {
        assert!(!inherits_working_primaries(&format!(
            "use aura_raw::colour::matrix::{{REC2020_TO_XYZ_D65}}; {declaration}"
        )));
    }
    assert!(!inherits_working_primaries(
        "use another_renderer::REC2020_TO_XYZ_D65;"
    ));
    assert!(!inherits_working_primaries(
        "// use aura_raw::colour::matrix::{REC2020_TO_XYZ_D65};\n apply(REC2020_TO_XYZ_D65, rgb)"
    ));
}

#[test]
fn nothing_in_this_crate_can_open_a_file() {
    // Invariant 1 as a property of the dependency list rather than a rule people remember.
    // `include_str!` is a compile-time read of this crate's own shaders and config, not a
    // runtime file open, so it is not what this looks for.
    let mut files = Vec::new();
    rust_files(&src(), &mut files);
    for path in files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let source = production_source(&path);
        for opener in ["std::fs::", "File::open", "File::create", "OpenOptions"] {
            assert!(
                !source.contains(opener),
                "{name} uses {opener}; this crate must not touch the filesystem"
            );
        }
    }
}
