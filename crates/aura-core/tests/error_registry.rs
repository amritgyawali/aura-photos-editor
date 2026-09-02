//! The registry is the contract between code, UI copy and support runbooks.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize)]
struct Registry {
    error: Vec<Entry>,
}

#[derive(serde::Deserialize)]
struct Entry {
    code: String,
    severity: String,
    recovery: String,
    summary: String,
    user_message: String,
    runbook: String,
    introduced: String,
}

/// Runbook paths in `errors.toml` are workspace-relative; tests run in the crate dir.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn registry() -> Registry {
    let text = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("errors.toml"))
        .expect("errors.toml");
    toml::from_str(&text).expect("parse errors.toml")
}

/// Collect every `AURA-DOMAIN-NNNN` literal that appears in a Rust source file.
fn codes_in(source: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let bytes = source.as_bytes();
    for (start, _) in source.match_indices("AURA-") {
        let mut end = start + "AURA-".len();
        while end < bytes.len() && bytes[end].is_ascii_uppercase() {
            end += 1;
        }
        if end >= bytes.len() || bytes[end] != b'-' {
            continue;
        }
        end += 1;
        let digits_start = end;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end - digits_start == 4 {
            found.insert(source[start..end].to_string());
        }
    }
    found
}

#[test]
fn every_registered_code_is_well_formed_and_has_a_runbook() {
    let reg = registry();
    let root = workspace_root();
    let mut seen = BTreeSet::new();

    for e in &reg.error {
        assert!(seen.insert(e.code.clone()), "duplicate code {}", e.code);

        let parts: Vec<&str> = e.code.split('-').collect();
        assert_eq!(parts.len(), 3, "malformed code {}", e.code);
        assert_eq!(parts[0], "AURA");

        let n: u32 = parts[2].parse().expect("numeric suffix");
        let expected_range = match parts[1] {
            "IO" => 1000..2000,
            "RAW" => 2000..3000,
            "DB" => 3000..4000,
            "GPU" => 4000..5000,
            "ML" => 5000..6000,
            "CLOUD" => 6000..7000,
            "JOB" => 7000..8000,
            // PHASE-14 renamed the reserved-but-empty `EXPORT` block to `RENDER`: an
            // export is a render written to a file, and phase 30 will want codes in the
            // same domain for the same subject. ADR-0029 section 9.
            "RENDER" => 8000..9000,
            "SEC" => 9000..10000,
            // PHASE-30 adds three domains, and three rather than one because this phase adds
            // three genuinely separate areas to the product. DLV is about getting a finished
            // file somewhere else and fails the way a network and a filesystem fail; LRN is
            // about the product changing its own future behaviour and fails statistically;
            // REL is about shipping the application and fails on a machine nobody is watching.
            // Its *export* codes are not here: phase 14 renamed the reserved-but-empty EXPORT
            // block to RENDER for exactly this phase, and an export is a render written to a
            // file. ADR-0061 section 8.
            "DLV" => 10000..11000,
            "LRN" => 11000..12000,
            "REL" => 12000..13000,
            other => panic!("unknown domain {other}"),
        };
        assert!(
            expected_range.contains(&n),
            "{} is outside its domain range",
            e.code
        );

        // A user message that names a file path or an exception leaks detail.
        assert!(!e.user_message.is_empty());
        assert!(
            e.user_message.len() <= 180,
            "{} user_message too long",
            e.code
        );
        for banned in ["Error:", "errno", "panicked", "None", "unwrap", "\\", "::"] {
            assert!(
                !e.user_message.contains(banned),
                "{} leaks technical detail: {banned}",
                e.code
            );
        }

        let runbook = root.join(&e.runbook);
        assert!(
            runbook.exists(),
            "{} has no runbook at {}",
            e.code,
            e.runbook
        );
        assert!(!e.summary.is_empty() && !e.introduced.is_empty());
        assert!(
            [
                "warning",
                "degraded",
                "item_failed",
                "run_blocking",
                "fatal"
            ]
            .contains(&e.severity.as_str()),
            "{} has an unknown severity",
            e.code
        );
        assert!(
            ["retry", "fallback", "quarantine", "halt", "ask_user"].contains(&e.recovery.as_str()),
            "{} has an unknown recovery",
            e.code
        );
    }
}

#[test]
fn every_code_used_in_source_is_registered() {
    let reg = registry();
    let registered: BTreeSet<String> = reg.error.iter().map(|e| e.code.clone()).collect();

    let mut used = BTreeSet::new();
    for entry in walkdir::WalkDir::new(workspace_root().join("crates"))
        .max_depth(6)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        let is_rust_source = path.extension().is_some_and(|e| e == "rs")
            && !path.to_string_lossy().contains("target");
        if is_rust_source {
            let source = fs::read_to_string(path).unwrap_or_default();
            used.extend(codes_in(&source));
        }
    }

    let missing: Vec<_> = used.difference(&registered).collect();
    assert!(
        missing.is_empty(),
        "codes used but not registered: {missing:?}"
    );
}

#[test]
fn phase_01_codes_are_all_present() {
    let reg = registry();
    let registered: BTreeSet<String> = reg.error.iter().map(|e| e.code.clone()).collect();
    for code in [
        "AURA-IO-1001",
        "AURA-IO-1002",
        "AURA-IO-1003",
        "AURA-IO-1004",
        "AURA-IO-1005",
        "AURA-IO-1006",
        "AURA-IO-1007",
        "AURA-IO-1008",
        "AURA-IO-1010",
        "AURA-DB-3005",
    ] {
        assert!(
            registered.contains(code),
            "{code} is missing from errors.toml"
        );
    }
}
