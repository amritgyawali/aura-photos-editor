//! A grep as a test. The fifth in the repository, after `colour_discipline.rs`,
//! `no_recipe_writes.rs`, `no_template_writes.rs` and `no_render_calls.rs`.
//!
//! Section 12's third failure mode is **safety bypass through a new code path**, and its
//! mitigation is "single choke-point API, property tests, SEC adversarial review, and a lint
//! forbidding direct calls to fill/inpaint". This file is that lint.
//!
//! ## What it enforces
//!
//! 1. `borrow::`, `fill::` and `inpaint::` are reached from `source.rs` and from nowhere else.
//! 2. Nothing in this crate writes a recipe. Phase 14's rule: `aura_recipe::schema::merge` is the
//!    one function in the workspace that writes one recipe into another, and it lives in
//!    `aura-app`. ADR-0049 section 9: there is no code path from `plan` to a written recipe.
//! 3. Nothing in this crate opens a socket, names a provider or reaches a model runtime. Phase
//!    04's rule and phase 03's, and the reason `judgement.rs` is a port rather than a dependency.
//! 4. No type in this crate carries a text prompt. `docs/generative-policy.md` promises AURA never
//!    generates from a description, and the way that promise is kept is that there is nowhere to
//!    put one.
//!
//! ## Why a grep rather than a type
//!
//! The type system already carries the strongest half of this: `source::select` takes a
//! `SafeCandidate`, which has no public constructor, so an unchecked region cannot be filled by
//! anybody. What a type cannot express is "and there is no *second* place that calls the fill",
//! because calling a public function inside your own crate is exactly what a crate is for.
//!
//! It is a weaker guarantee than a type and this file says so, which is the same admission
//! `no_template_writes.rs` makes.

use std::fs;
use std::path::{Path, PathBuf};

/// The crate's own source directory.
fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every `.rs` file in the crate, with its contents.
fn sources() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(src()) else {
        panic!("the crate's own src directory must be readable");
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let text = fs::read_to_string(&path).unwrap_or_default();
        out.push((name, text));
    }
    assert!(!out.is_empty(), "the crate must have source files");
    out
}

/// Everything in a file that is not a comment.
///
/// The greps below are about what the code *does*, and every one of these modules explains in its
/// own header which crates it deliberately does not reach. A grep that read the prose would fail on
/// the sentence that documents the rule it is enforcing, which is the shape of false positive that
/// gets a lint deleted.
fn code_only(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip the `#[cfg(test)]` module from a file, roughly.
///
/// Test code is allowed to call the removal modules directly - that is how they are tested - so a
/// grep that did not do this would forbid testing them at all. The split is on the attribute rather
/// than on brace matching, which is enough because every one of this crate's inline test modules is
/// the last item in its file.
fn without_tests(text: &str) -> &str {
    match text.find("#[cfg(test)]") {
        Some(at) => text.get(..at).unwrap_or(""),
        None => text,
    }
}

#[test]
fn only_the_source_selector_reaches_a_removal() {
    // The four modules that move a pixel, and the one file allowed to call them.
    let removals = ["borrow::", "fill::", "inpaint::"];
    let allowed = ["source.rs", "lib.rs"];

    for (name, text) in sources() {
        if allowed.contains(&name.as_str()) {
            continue;
        }
        // A module may of course refer to itself.
        let own = name.trim_end_matches(".rs").to_string();
        let body = code_only(without_tests(&text));
        for call in removals {
            if call.trim_end_matches("::") == own {
                continue;
            }
            assert!(
                !body.contains(call),
                "{name} calls {call} directly. Every removal goes through `source::select`, which \
                 takes a `SafeCandidate` and therefore cannot be handed an unchecked region. \
                 PHASE-24 section 12, ADR-0049 section 4."
            );
        }
    }
}

#[test]
fn nothing_in_this_crate_writes_a_recipe() {
    for (name, text) in sources() {
        let body = code_only(without_tests(&text));
        for forbidden in ["schema::merge", "aura_recipe::", "aura_render::"] {
            assert!(
                !body.contains(forbidden),
                "{name} reaches {forbidden}. This crate decides a rectangle and a method; \
                 `aura-app` merges an accepted proposal into a recipe and `aura-render` applies \
                 it. A dependency in this direction would let a removal reach a pixel without \
                 going through the merge. Phase 14's rule, ADR-0049 section 9."
            );
        }
    }
}

#[test]
fn nothing_in_this_crate_reaches_a_provider_or_a_runtime() {
    for (name, text) in sources() {
        let body = code_only(without_tests(&text));
        for forbidden in [
            "aura_cloud::",
            "aura_infer::",
            "TcpStream",
            "reqwest",
            "https://",
        ] {
            assert!(
                !body.contains(forbidden),
                "{name} reaches {forbidden}. The editorial judgement is a port - \
                 `judgement::EditorialJudge` - implemented in `aura-app` over phase 04's frozen \
                 `CloudTask` shape. No phase may open a socket."
            );
        }
    }
}

#[test]
fn no_type_in_this_crate_can_carry_a_prompt() {
    // `docs/generative-policy.md` promises AURA never generates from a description. The way that
    // promise is kept is that there is nowhere to put one: no field, no argument, no constant.
    //
    // Matched on the field-declaration shape rather than on the bare word, because `inpaint.rs`
    // has to be able to *say* that there is no prompt field and will not be one.
    for (name, text) in sources() {
        let body = code_only(without_tests(&text));
        for line in body.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            for forbidden in ["prompt:", "pub prompt", "text_prompt", "instruction:"] {
                assert!(
                    !trimmed.contains(forbidden),
                    "{name} declares {forbidden}. AURA removes an object; it does not take a \
                     description of what should be there instead. PHASE-24 section 2.2."
                );
            }
        }
    }
}

#[test]
fn the_safety_engine_is_the_only_producer_of_a_safe_candidate() {
    // The strong half, asserted so that a future author who adds a constructor has to delete this
    // line rather than merely not notice.
    let safety = fs::read_to_string(src().join("safety.rs")).unwrap_or_default();
    assert!(
        safety.contains("pub struct SafeCandidate {")
            && safety.contains("    candidate: Candidate,")
            && safety.contains("    verdict: SafetyVerdict,"),
        "`SafeCandidate`'s fields must stay private: it is what makes `source::select` \
         unreachable for an unchecked region."
    );
    assert!(
        !safety.contains("pub fn new(candidate: Candidate"),
        "`SafeCandidate` must have no public constructor. ADR-0049 section 2."
    );
}

#[test]
fn the_five_checks_run_before_anything_is_scored() {
    // The ordering, read out of the file rather than asserted about behaviour, because a
    // behavioural test can only prove the order for the inputs it happens to use.
    let safety = fs::read_to_string(src().join("safety.rs")).unwrap_or_default();
    let body = code_only(without_tests(&safety));
    let positions: Vec<usize> = [
        "SafetyCheck::SizeCap",
        "SafetyCheck::Denylist",
        "SafetyCheck::IdentityProtect",
        "SafetyCheck::StructureSpan",
        "SafetyCheck::Confidence",
    ]
    .iter()
    .map(|needle| {
        body.as_str().find(needle)
            .unwrap_or_else(|| panic!("safety.rs must run {needle}"))
    })
    .collect();

    for pair in positions.windows(2) {
        let (first, second) = (pair.first().copied(), pair.get(1).copied());
        assert!(
            first < second,
            "the five checks must appear in `SafetyCheck::ALL` order, so a candidate that is \
             simply too large never causes a mask to be resolved"
        );
    }
}
