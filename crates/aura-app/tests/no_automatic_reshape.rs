//! A grep as a test: no automatic pass may write the Studio's finishing tools. ADR-0108.
//!
//! Face and body reshaping, liquify, background replacement and feature colour exist only as a
//! photographer's explicit choice. The one module allowed to write the extension is the command
//! the panel calls; Auto enhance, Auto retouch, the advanced workflow, the unattended run and
//! every other pass must never name it. A rule enforced by a tool survives a hurried change.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]

use std::fs;
use std::path::{Path, PathBuf};

fn sources(directory: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("src").flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn only_the_finishing_command_writes_the_finishing_extension() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
    let mut files = Vec::new();
    sources(root, &mut files);
    let allowed = ["finish_commands.rs", "lib.rs"];
    let mut offenders = Vec::new();
    for file in files {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if allowed.contains(&name) {
            continue;
        }
        let text = fs::read_to_string(&file).unwrap_or_default();
        if text.contains("studio_finish") {
            offenders.push(file.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "automatic code names the finishing tools: {offenders:?}"
    );
}
