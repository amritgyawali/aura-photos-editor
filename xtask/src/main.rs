//! `cargo xtask` - repository automation that must never ship in a binary.
//!
//! Panics are permitted in this crate: it is a developer tool, not product code.
//!
//! Subcommands:
//!   * `contracts [--check]` - hash every frozen contract file and compare with `contracts.lock`.
//!   * `fixtures`            - generate the synthetic reference weddings (T16).
//!   * `bench <stage>`       - run the budget benchmarks and write `perf/results` (T18).
//!   * `models [--generate]` - generate and sign the pinned model set, or check it (phase 03).

mod models;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs};

use walkdir::WalkDir;

/// Files that are frozen contracts even though they do not live in a `contract/` directory.
const EXTRA_CONTRACTS: &[&str] = &[
    "crates/aura-catalog/migrations/0001_init.sql",
    "crates/aura-catalog/migrations/0004_cloud_audit.sql",
    "crates/aura-catalog/migrations/0005_embeddings.sql",
    "crates/aura-catalog/migrations/0006_people.sql",
    "crates/aura-catalog/migrations/0007_scenes.sql",
    "crates/aura-catalog/migrations/0008_moments.sql",
    "crates/aura-catalog/migrations/0009_integrity.sql",
    "crates/aura-catalog/migrations/0010_emotion.sql",
    "crates/aura-catalog/migrations/0011_composition.sql",
    "crates/aura-catalog/migrations/0012_selection.sql",
    "crates/aura-catalog/migrations/0013_ledger.sql",
    "crates/aura-catalog/migrations/0014_develop.sql",
    // Added in PHASE-19. 0015 was missing from this list, which was a phase 15 oversight
    // rather than a decision: every other migration since 0001 is frozen and a schema change
    // needs an ADR and a re-lock. Both are here now.
    "crates/aura-catalog/migrations/0015_tone.sql",
    "crates/aura-catalog/migrations/0016_local_light.sql",
    "ui/src/ipc/types.ts",
    "schemas/recipe.v1.json",
];

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("contracts") => contracts(args.iter().any(|a| a == "--check")),
        Some("fixtures") => fixtures(&args[1..]),
        Some("bench") => bench(&args[1..]),
        Some("models") => models::run(&args[1..]),
        _ => {
            eprintln!(
                "usage: cargo xtask [contracts [--check] | fixtures [--out DIR] | \
                 bench <stage> | models [--generate]]"
            );
            ExitCode::FAILURE
        }
    }
}

/// Hash every file under a `contract/` directory or listed in [`EXTRA_CONTRACTS`],
/// then compare against the recorded digest. Any drift fails CI.
fn contracts(check_only: bool) -> ExitCode {
    let mut current: BTreeMap<String, String> = BTreeMap::new();

    for entry in WalkDir::new("crates").into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let in_contract_dir = path.components().any(|c| c.as_os_str() == "contract");
        let is_contract = in_contract_dir
            && path
                .extension()
                .is_some_and(|e| e == "rs" || e == "sql" || e == "json" || e == "toml");
        if is_contract {
            current.insert(normalise(path), digest_of(path));
        }
    }

    for extra in EXTRA_CONTRACTS {
        let path = Path::new(extra);
        if path.exists() {
            current.insert((*extra).to_string(), digest_of(path));
        }
    }

    let lock_path = Path::new("contracts.lock");
    let rendered = current
        .iter()
        .map(|(path, digest)| format!("{digest}  {path}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    if !lock_path.exists() {
        fs::write(lock_path, &rendered).expect("write contracts.lock");
        println!("contracts.lock created with {} entries", current.len());
        return ExitCode::SUCCESS;
    }

    let recorded = fs::read_to_string(lock_path).expect("read contracts.lock");
    if recorded.replace("\r\n", "\n") == rendered {
        println!("contracts: {} entries, all locked", current.len());
        return ExitCode::SUCCESS;
    }

    eprintln!("CONTRACT DRIFT DETECTED");
    let old: BTreeMap<String, String> = recorded
        .lines()
        .filter_map(|line| line.split_once("  "))
        .map(|(digest, path)| (path.to_string(), digest.to_string()))
        .collect();

    for (path, digest) in &current {
        match old.get(path) {
            None => eprintln!("  NEW      {path}"),
            Some(previous) if previous != digest => eprintln!("  CHANGED  {path}"),
            Some(_) => {}
        }
    }
    for path in old.keys() {
        if !current.contains_key(path) {
            eprintln!("  REMOVED  {path}");
        }
    }

    eprintln!("\nA frozen contract changed. Write an ADR in docs/adr/ with a grep");
    eprintln!("blast radius, get it approved, then re-run without --check to relock.");

    if check_only {
        ExitCode::FAILURE
    } else {
        fs::write(lock_path, &rendered).expect("relock");
        eprintln!("contracts.lock updated.");
        ExitCode::SUCCESS
    }
}

/// T16 - drive the fixture generator that lives in `aura-cli` so CI has one entry point.
fn fixtures(rest: &[String]) -> ExitCode {
    let out = arg_value(rest, "--out").unwrap_or_else(|| "tests/fixtures/generated".to_string());
    let status = Command::new(env!("CARGO"))
        .args([
            "run",
            "--release",
            "-p",
            "aura-cli",
            "--",
            "fixtures",
            "--out",
            &out,
        ])
        .status()
        .expect("spawn aura-cli fixtures");
    exit_code(status.success())
}

/// T18 - run the criterion budget benchmarks for one stage and keep the report.
fn bench(stages: &[String]) -> ExitCode {
    let mut command = Command::new(env!("CARGO"));
    command.args(["bench", "--workspace"]);
    if let Some(stage) = stages.first() {
        command.args(["--", stage]);
    }
    let status = command.status().expect("spawn cargo bench");
    exit_code(status.success())
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    let index = args.iter().position(|a| a == flag)?;
    args.get(index + 1).cloned()
}

fn digest_of(path: &Path) -> String {
    let bytes = fs::read(path).expect("read contract file");
    // Determinism: normalise line endings so a CRLF checkout hashes like an LF one.
    let normalised = String::from_utf8(bytes.clone())
        .map(|text| text.replace("\r\n", "\n").into_bytes())
        .unwrap_or(bytes);
    blake3::hash(&normalised).to_hex().to_string()
}

fn normalise(path: &Path) -> String {
    PathBuf::from(path).display().to_string().replace('\\', "/")
}

fn exit_code(ok: bool) -> ExitCode {
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
