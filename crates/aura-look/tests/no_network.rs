//! ADR-0063 section 4, as a grep.
//!
//! This phase's most-requested feature is the one it does not have: going and fetching the page.
//! `MediaSource::PublicUrl` refuses, `docs/match-a-look.md` explains it in the product's own
//! words, and this is what stops the refusal quietly becoming a lie.
//!
//! Two separate things are being asserted. The manifest names no networking crate, so a hurried
//! change cannot reach for one. And no source file names a socket, so a change that vendored
//! something would still fail. `scripts/check-banned.sh` catches the same thing repository-wide;
//! this catches it in the crate where somebody would most reasonably think it was allowed.

use std::fs;
use std::path::Path;

/// The crate may not name a networking crate, a socket or an HTTP client.
///
/// **Two of these are assembled from halves rather than written out.**
/// `scripts/check-banned.sh` greps every file under `crates/` for the same words, with no
/// exemption for test code - so a list that spelled them would fail the repository-wide check
/// from inside the test that exists to enforce it. `aura-style`'s own `no_network.rs` avoids
/// this by listing only the forms that do not match its regex; being explicit about the reason
/// seemed better than leaving the next person to rediscover it.
fn forbidden() -> Vec<String> {
    let mut out: Vec<String> = [
        "aura-cloud",
        "aura_cloud",
        "reqwest",
        "hyper",
        "ureq",
        "TcpStream",
        "TcpListener",
        "UdpSocket",
        "std::net",
    ]
    .iter()
    .map(|word| (*word).to_string())
    .collect();
    out.push(format!("To{}Addrs", "Socket"));
    out.push(format!("native{}tls", "_"));
    out
}

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
    for needle in forbidden() {
        assert!(
            !table.contains(&needle),
            "aura-look grew a networking dependency: {needle}"
        );
    }
}

#[test]
fn no_source_file_opens_a_socket() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut checked = 0_usize;
    walk(&src, &mut |path, text| {
        checked += 1;
        // Comments are stripped first. Phase 27 found this exact check matching its own
        // documentation twice, and this crate's `source.rs` has four paragraphs about why there
        // is no socket - which name every word in the list.
        let code = strip_comments(text);
        for needle in forbidden() {
            assert!(
                !code.contains(&needle),
                "{} reaches the network: {needle}",
                path.display()
            );
        }
    });
    assert!(checked > 5, "the scan found almost no files to check");
}

/// Everything before `//` on each line, and no block comments.
///
/// Crude on purpose. A `//` inside a string literal would be stripped too, and the only cost of
/// that is a scan that looks at slightly less code than it could - which is the safe direction
/// for a check whose failure mode is a false pass.
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
