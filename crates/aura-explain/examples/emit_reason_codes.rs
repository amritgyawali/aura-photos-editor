//! Writes `docs/reason-codes.md` from the registry.
//!
//! Section 6.2 makes the reason-code reference a public document, and a public document that
//! is maintained by hand is a public document that disagrees with the product. So it is
//! generated from `aura_explain::reason::Catalog`, which is itself assembled from the frozen
//! vocabularies - and `tests/eval/explain_eval.rs` asserts that every code in the registry
//! appears in the file, so a phase that adds a code and forgets to re-run this fails the
//! build rather than shipping an undocumented reason.
//!
//! ```text
//! cargo run --package aura-explain --example emit_reason_codes > docs/reason-codes.md
//! ```

use aura_explain::reason::{
    Catalog, ReasonSeverity, COMPOSITION, CURATION, DEVELOP, EMOTION, LEDGER, QUALITY, SELECTION,
    TECHNICAL,
};

fn main() {
    let catalog = Catalog::shipped();

    println!("# The reason codes");
    println!();
    println!(
        "Every explanation AURA gives is one of the codes below. A code is stable, a \
         sentence is not: the wording here is what the product says today and a release may \
         improve it, but the code is what is stored, counted and translated against."
    );
    println!();
    println!(
        "**Generated from the registry** by `cargo run --package aura-explain --example \
         emit_reason_codes`. Do not edit by hand - `tests/eval/explain_eval.rs` asserts that \
         this file documents every code the product can emit, so an edit here that \
         disagrees with the code is a failing build rather than a wrong document."
    );
    println!();
    println!("## How to read the severity column");
    println!();
    println!("| Severity | What it means |");
    println!("|---|---|");
    println!("| `credit` | Something good about the photograph. It argued for the decision. |");
    println!(
        "| `note` | Neutral, or a suspicion explicitly cleared - a deliberate tilt, a \
         blink at a kiss, a frame that simply lost to a better one. |"
    );
    println!(
        "| `caveat` | **Something was not measured.** The decision was made with less than \
         it wanted, and the confidence is lower for it. Never a fault: \"AURA did not check \
         this\" and \"AURA checked this and it is bad\" are opposite sentences. |"
    );
    println!("| `fault` | Something is wrong with the photograph. It argued against. |");
    println!();

    for (domain, title, blurb) in [
        (
            TECHNICAL,
            "Technical - whether the frame worked",
            "Phase 09's vocabulary. Focus, motion, exposure, noise and eyes.",
        ),
        (
            EMOTION,
            "Emotion - what the frame is worth",
            "Phase 10's vocabulary. Expressions, interactions, gaze and peaks.",
        ),
        (
            COMPOSITION,
            "Composition - how the frame is built",
            "Phase 11's vocabulary. Horizon, crops, thirds, balance and background.",
        ),
        (
            SELECTION,
            "Selection - whether the frame is delivered",
            "Phase 12's vocabulary. What put a photograph in the gallery, and what kept it \
             out.",
        ),
        (
            DEVELOP,
            "Develop - how the photograph was treated",
            "Phases 15 and 16's vocabularies. What the light was, what was done about it, and \
             what the grade did to skin. Added to this reference in phase 30, when the learning \
             loop needed a develop decision to attribute a correction to.",
        ),
        (
            CURATION,
            "Curation - what to show",
            "Phase 29's vocabulary. Monochrome, portfolio, album sequence, social sets and \
             captions.",
        ),
        (
            QUALITY,
            "Quality control - what AURA found wrong with its own work",
            "Phase 27's vocabulary. Every one of these is a caveat by construction: a QC finding \
             is a fault the product found in itself, and phase 27's vocabulary carries no \
             positive code.",
        ),
        (
            LEDGER,
            "The record itself",
            "Phase 13's own codes. These are facts about the decision rather than about the \
             photograph.",
        ),
    ] {
        println!("## {title}");
        println!();
        println!("{blurb}");
        println!();
        println!("| Code | Severity | What AURA says |");
        println!("|---|---|---|");
        for entry in catalog.domain(domain) {
            println!(
                "| `{}` | `{}` | {} |",
                entry.code,
                severity_slug(entry.severity),
                entry.template
            );
        }
        println!();
    }

    println!("---");
    println!();
    println!(
        "{} codes in total. See `docs/how-confidence-works.md` for what the number beside \
         them means.",
        catalog.len()
    );
}

fn severity_slug(severity: ReasonSeverity) -> &'static str {
    severity.as_str()
}
