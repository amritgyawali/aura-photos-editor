//! PHASE-17 section 7 and section 13's fifth criterion, as a grep.
//!
//! > No cloud AI call in this phase. The phase must work with the network cable unplugged.
//!
//! > Training runs locally with progress, cancel and resume, and **never uploads imagery**.
//!
//! Section 9 gives SEC exactly one task - "ensure archives are never uploaded" - and the
//! cheapest way to make that checkable rather than promised is for the capability not to exist.
//! `aura-style` depends on no cloud crate, opens no socket, and has no field anywhere on its
//! surface that could hold an image.
//!
//! This is the same shape as `aura-brain-photo`'s `no_recipe_writes.rs`: a grep as a test, so
//! the property fails the build rather than a review.

use std::fs;
use std::path::Path;

/// The crate may not name a networking crate, a socket or an HTTP client.
///
/// `aura-cloud` is the one this is really about - phase 04's rule is that it is the *only* way
/// to reach a model provider, so a phase that must not reach one must not depend on it - and the
/// rest are here because a hurried change that wanted to upload something would reach for one of
/// them rather than for the gateway.
const FORBIDDEN: &[&str] = &[
    "aura-cloud",
    "aura_cloud",
    "reqwest",
    "hyper",
    "ureq",
    "TcpStream",
    "UdpSocket",
    "std::net",
];

#[test]
fn the_manifest_names_no_networking_dependency() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .unwrap_or_default();
    // The comment block in `Cargo.toml` explains the omission and names `aura-cloud`, so the
    // scan looks only at the dependency table rather than at the whole file.
    let table = manifest
        .split("# Deliberately absent")
        .next()
        .unwrap_or_default()
        .to_string();
    for needle in FORBIDDEN {
        assert!(
            !table.contains(needle),
            "aura-style grew a networking dependency: {needle}"
        );
    }
}

#[test]
fn no_source_file_opens_a_socket_or_reaches_a_provider() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut checked = 0_usize;
    walk(&src, &mut |path, text| {
        checked += 1;
        for needle in FORBIDDEN {
            assert!(
                !text.contains(needle),
                "{} names {needle}; PHASE-17 section 7 says this phase reaches no network",
                path.display()
            );
        }
    });
    assert!(checked >= 10, "the scan found only {checked} source files");
}

#[test]
fn nothing_on_the_surface_could_carry_an_image() {
    // The other half of the same guarantee, and the structural one: even with a network, there
    // is no field in this crate's public shapes that an image could travel in. `StylePair`
    // carries two file *names* and a verdict; migration 17 stores the same.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    walk(&src, &mut |path, text| {
        for needle in ["base64", "data:image", "Vec<u8>>>"] {
            assert!(
                !text.contains(needle),
                "{} looks like it carries encoded imagery: {needle}",
                path.display()
            );
        }
    });
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
