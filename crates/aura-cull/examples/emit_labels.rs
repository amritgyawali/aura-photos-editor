//! Write `tests/fixtures/labels/keepers_*.json` from the fixture weddings.
//!
//! PHASE-12 section 4 asks for "human keeper ground truth per fixture wedding" as checked-in
//! JSON. The labels themselves are produced by `aura_cull::fixtures`, so this example exists
//! to make them *readable* - a reviewer arguing with the ground truth should be able to open
//! a file rather than a Rust function - and to make a change to the label model show up as a
//! diff in a pull request rather than only as a shifted gate number.
//!
//! Run it from the workspace root:
//!
//! ```text
//! cargo run -p aura-cull --example emit_labels
//! ```
//!
//! The output is deterministic, so a run that changes a file means the label model changed
//! and somebody should say why in the commit message.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;

use aura_cull::fixtures::{self, SKIP_WORST};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("labels");
    if let Err(err) = std::fs::create_dir_all(&root) {
        eprintln!("cannot create {}: {err}", root.display());
        return;
    }

    for wedding in fixtures::all() {
        let path = root.join(format!("keepers_{}.json", wedding.name));
        let keepers: BTreeSet<String> = wedding
            .labels
            .keepers
            .iter()
            .map(aura_core::PhotoId::to_db)
            .collect();
        let rules: Vec<String> = wedding
            .labels
            .must_have_candidates
            .iter()
            .map(|rule| rule.as_str().to_string())
            .collect();
        let moments: BTreeSet<String> = wedding
            .input
            .candidates
            .iter()
            .filter(|candidate| wedding.labels.keepers.contains(&candidate.image_id))
            .filter_map(|candidate| candidate.moment_id)
            .map(|id| id.to_db())
            .collect();

        let mut json = String::new();
        let _ = writeln!(json, "{{");
        let _ = writeln!(json, "  \"schema\": \"aura.keeper-labels.v1\",");
        let _ = writeln!(json, "  \"name\": \"{}\",", wedding.name);
        let _ = writeln!(
            json,
            "  \"description\": \"{}\",",
            description(wedding.name)
        );
        let _ = writeln!(
            json,
            "  \"labelled_by\": \"PHASE-12 section 4, authored ground truth: the strongest \
             frame of every moment, minus the weakest {:.0}% of each scene's moments, plus a \
             second frame from every moment that genuinely varied. Scene-relative on \
             purpose - a global quality threshold would delete a whole chapter shot in bad \
             light, which is the failure this engine exists to fix.\",",
            SKIP_WORST * 100.0
        );
        let _ = writeln!(json, "  \"frames\": {},", wedding.input.candidates.len());
        let _ = writeln!(json, "  \"moments\": {},", wedding.input.moments.len());
        let _ = writeln!(json, "  \"keeper_frames\": {},", keepers.len());
        let _ = writeln!(json, "  \"keeper_moments\": {},", moments.len());
        let _ = writeln!(
            json,
            "  \"must_have_candidates\": [{}],",
            rules
                .iter()
                .map(|rule| format!("\"{rule}\""))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let _ = writeln!(json, "  \"keepers\": [");
        let last = keepers.len().saturating_sub(1);
        for (index, id) in keepers.iter().enumerate() {
            let comma = if index == last { "" } else { "," };
            let _ = writeln!(json, "    \"{id}\"{comma}");
        }
        let _ = writeln!(json, "  ]");
        let _ = writeln!(json, "}}");

        match std::fs::write(&path, json) {
            Ok(()) => println!(
                "{}: {} keeper frames across {} moments",
                path.display(),
                keepers.len(),
                moments.len()
            ),
            Err(err) => eprintln!("cannot write {}: {err}", path.display()),
        }
    }
}

/// What each fixture is for, in one sentence, so the file explains itself.
fn description(name: &str) -> &'static str {
    match name {
        "hindu_night" => {
            "A long ritual sequence in bad light. The wedding a Western-trained culling \
             model would damage most."
        }
        "christian_daylight" => {
            "The textbook case: every must-have present, good light, a heavy portrait \
             session."
        }
        "nepali_reception" => {
            "Mixed light, an enormous dance chapter and forty guests. The diversity pass's \
             fixture."
        }
        _ => {
            "Four hundred frames, no cake, no formals, no reception entrance and no exit. \
             The fixture for the state that is correct rather than broken."
        }
    }
}
