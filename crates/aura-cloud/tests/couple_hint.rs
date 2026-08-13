//! The one cloud call phase 06 is allowed to make, driven through the real gateway.
//!
//! Like every other test in this crate, it runs with no network at all: the transport is
//! a cassette. What is tested is not "does a model answer" - a recording always answers -
//! but the five things around it that section 7 constrains, and one that it does not
//! constrain but that matters more than any of them.
//!
//! The five: the trigger, the payload, the cost ceiling, the local fallback, and the
//! repair-once-then-fall-back path.
//!
//! The sixth is the one this task exists to make impossible. **A vision model asked which
//! two people are the couple will volunteer what it thinks they are** - their gender,
//! their relationship, sometimes more. `210-anthropic-couple-hint-described` is a
//! recording of exactly that, and the test below proves the answer is refused rather than
//! stored, because the output type can only express opaque handles.

mod support;

use aura_cloud::contract::cloud::{CloudTask, Source, Validate};
use aura_cloud::couple_hint::{
    CandidateStats, CoupleHint, CoupleHintInput, CoupleHintOutput, PairStats, ASK_WITHIN_MARGIN,
    MAX_CALLS_PER_PROJECT, MAX_THUMBNAILS, PERSON_HANDLES,
};
use aura_cloud::provider::OfflineTransport;
use aura_cloud::tasks::Scored;
use aura_cloud::CallContext;
use std::sync::Arc;
use support::Harness;

macro_rules! context {
    ($harness:expr) => {
        CallContext {
            project: &$harness.project,
            decision_ref: Some("dec-couple"),
            cancel: &$harness.cancel,
        }
    };
}

fn candidate(handle: &str, frames: u32, area_milli: u16, centrality_milli: u16) -> CandidateStats {
    CandidateStats {
        handle: handle.to_string(),
        frames,
        getting_ready_frames: 0,
        ceremony_frames: 0,
        portrait_frames: 0,
        mean_area_milli: area_milli,
        mean_centrality_milli: centrality_milli,
    }
}

/// An input with two pairs a hair apart: the case section 7's trigger describes.
fn ambiguous(decision: &str) -> CoupleHintInput {
    CoupleHintInput {
        decision_ref: decision.to_string(),
        candidates: vec![
            candidate("person_1", 210, 120, 780),
            candidate("person_2", 180, 90, 610),
            candidate("person_3", 205, 118, 770),
            candidate("person_4", 96, 60, 440),
            candidate("person_5", 88, 55, 400),
        ],
        pairs: vec![
            PairStats {
                a: "person_1".to_string(),
                b: "person_3".to_string(),
                frames: 61,
                portrait_frames: 0,
                local_score_milli: 620,
            },
            PairStats {
                a: "person_1".to_string(),
                b: "person_2".to_string(),
                frames: 58,
                portrait_frames: 0,
                local_score_milli: 598,
            },
        ],
        scenes_available: false,
        faces_visible: true,
        thumbnail_hashes: (0..6).map(|index| format!("thumb-{index}")).collect(),
    }
}

/// A task instance carrying six redacted thumbnails.
fn task(_harness: &Harness) -> CoupleHint {
    // The contact-sheet builder from the phase 04 harness stands in for the redacted
    // thumbnails: what matters here is that the payload is a derivative, is content
    // hashed, and is within the size ceiling - all of which it already proves.
    CoupleHint::for_thumbnails(vec![support::sheet(6)])
}

#[test]
fn a_valid_hint_comes_back_typed_with_its_provenance() {
    let harness = Harness::new();
    let result = harness
        .gateway
        .run(
            &task(&harness),
            &ambiguous("couple-hint-clear"),
            &context!(harness),
        )
        .expect("the gateway answers");

    assert_eq!(result.source, Source::Cloud);
    assert_eq!(result.value.couple, vec!["person_1", "person_3"]);
    assert!(result.value.confidence() > 0.7);
    assert!(!result.value.reasons().is_empty(), "invariant 2");

    let rows = harness.audit.rows();
    let row = rows.first().expect("one audit row");
    assert_eq!(row.task, CoupleHint::NAME);
    assert_eq!(row.task_version, CoupleHint::VERSION);
    assert_eq!(row.status, "ok");
    assert_eq!(row.decision_ref.as_deref(), Some("dec-couple"));
    assert_eq!(row.image_hashes.len(), 1, "the thumbnails were sent");
}

#[test]
fn a_described_person_is_refused_repaired_once_and_then_accepted() {
    // The failure this task exists to prevent. The first answer describes two people -
    // "the woman in red" - and volunteers a gender inference in its reasons. Neither
    // string is a handle, so `validate` refuses, the gateway repairs once, and the second
    // answer is correct.
    let harness = Harness::new();
    let result = harness
        .gateway
        .run(
            &task(&harness),
            &ambiguous("couple-hint-described"),
            &context!(harness),
        )
        .expect("the repair must succeed");

    assert_eq!(result.source, Source::Cloud);
    assert_eq!(result.value.couple, vec!["person_2", "person_4"]);
    for handle in &result.value.couple {
        assert!(
            PERSON_HANDLES.contains(&handle.as_str()),
            "{handle} is not an opaque handle"
        );
    }

    let rows = harness.audit.rows();
    let row = rows.first().expect("one audit row");
    assert_eq!(row.retry_count, 1, "exactly one repair, never two");
}

#[test]
fn the_local_fallback_keeps_the_pipeline_complete() {
    // Invariant 6. With no network at all, the answer is the local scorer's own argmax,
    // deliberately at low confidence and saying why.
    let harness = Harness::builder()
        .transport(Arc::new(OfflineTransport))
        .build();
    let input = ambiguous("couple-hint-offline");
    let result = harness
        .gateway
        .run(&task(&harness), &input, &context!(harness))
        .expect("a fallback is not a failure");

    assert_eq!(result.source, Source::LocalFallback);
    assert_eq!(result.value.couple, vec!["person_1", "person_3"]);
    assert!(
        result.value.confidence() <= 0.45,
        "an unresolved question must not come back confident: {}",
        result.value.confidence()
    );
    assert!(
        result
            .value
            .reasons()
            .iter()
            .any(|reason| reason.contains("confirm")),
        "{:?}",
        result.value.reasons()
    );
    assert_eq!(result.cost_usd, 0.0, "a fallback is not billed");
}

#[test]
fn the_fallback_survives_a_project_with_no_pairs() {
    let harness = Harness::builder()
        .transport(Arc::new(OfflineTransport))
        .build();
    let mut input = ambiguous("couple-hint-empty");
    input.pairs.clear();
    let result = harness
        .gateway
        .run(&task(&harness), &input, &context!(harness))
        .expect("a fallback must always exist");
    assert!(result.value.couple.is_empty());
    assert_eq!(result.value.confidence(), 0.0);
    assert!(!result.value.reasons().is_empty());
}

#[test]
fn the_trigger_fires_only_when_the_local_scorer_cannot_separate_the_pair() {
    let close = ambiguous("couple-hint-clear");
    assert!(close.local_margin() < ASK_WITHIN_MARGIN);
    assert!(CoupleHint::is_worth_asking(&close));

    let mut clear = close.clone();
    if let Some(second) = clear.pairs.get_mut(1) {
        second.local_score_milli = 300;
    }
    assert!(
        !CoupleHint::is_worth_asking(&clear),
        "a confident local answer must not be paid for"
    );

    let mut single = close.clone();
    single.pairs.truncate(1);
    assert!(
        !CoupleHint::is_worth_asking(&single),
        "one candidate pair is under-analysed, not ambiguous"
    );
}

#[test]
fn the_prompt_says_when_scene_labels_are_missing() {
    // The weakness has to be in the prompt, or the model answers as if co-occurrence were
    // as good as a couple portrait - and a bride and her mother are constantly
    // co-occurrent.
    let harness = Harness::new();
    let starved = task(&harness).prompt(&ambiguous("couple-hint-clear"));
    assert!(
        starved.user.contains("NOT available"),
        "the user turn must say the scene labels are missing"
    );

    let mut with_scenes = ambiguous("couple-hint-clear");
    with_scenes.scenes_available = true;
    let informed = task(&harness).prompt(&with_scenes);
    assert!(informed.user.contains("scene labels are available"));
}

#[test]
fn the_prompt_carries_the_non_inference_rule() {
    let harness = Harness::new();
    let spec = task(&harness).prompt(&ambiguous("couple-hint-clear"));
    assert!(
        spec.system
            .contains("Do not infer gender, ethnicity, religion"),
        "section 7's second rule must be in the system prompt verbatim"
    );
    assert!(spec.system.contains("Return ONLY JSON"));
    assert!(
        (spec.temperature - 0.0).abs() < f32::EPSILON,
        "temperature zero"
    );
    assert!(spec.user.contains("allowed_handles:"));
}

#[test]
fn the_payload_carries_counts_and_never_a_name() {
    // What actually leaves the machine. No name, no identity id, no template, no
    // landmark - only opaque handles and counts.
    let harness = Harness::new();
    let spec = task(&harness).prompt(&ambiguous("couple-hint-clear"));
    for handle in PERSON_HANDLES.iter().take(5) {
        assert!(spec.user.contains(handle));
    }
    assert!(!spec.user.contains("idt_"), "no identity id may be sent");
    assert!(!spec.user.contains("fce_"), "no face id may be sent");
    assert!(!spec.user.contains("prj_"), "no project id may be sent");
}

#[test]
fn the_thumbnail_cap_is_enforced_by_the_type() {
    let too_many: Vec<_> = (0..MAX_THUMBNAILS + 4).map(|_| support::sheet(2)).collect();
    let task = CoupleHint::for_thumbnails(too_many);
    assert_eq!(
        task.thumbnails().len(),
        MAX_THUMBNAILS,
        "section 7's cap is a property of the task, not of a caller's discipline"
    );
}

#[test]
fn the_cost_ceiling_is_two_cents_and_the_call_count_is_capped() {
    let harness = Harness::new();
    assert!((task(&harness).max_cost_usd() - 0.02).abs() < f32::EPSILON);
    assert_eq!(MAX_CALLS_PER_PROJECT, 2, "section 7's cost control");
}

// ---------------------------------------------------------------------------
// `Validate`: the rules the JSON Schema cannot express.
// ---------------------------------------------------------------------------

fn output(couple: &[&str], family: &[&str], confidence: f32, reasons: &[&str]) -> CoupleHintOutput {
    CoupleHintOutput {
        couple: couple.iter().map(|s| (*s).to_string()).collect(),
        close_family: family.iter().map(|s| (*s).to_string()).collect(),
        confidence,
        reasons: reasons.iter().map(|s| (*s).to_string()).collect(),
    }
}

#[test]
fn a_description_of_a_person_is_not_a_handle() {
    let described = output(
        &["the woman in red", "the man in the cream sherwani"],
        &[],
        0.8,
        &["the bride is clearly the woman in red"],
    );
    let error = described
        .validate()
        .expect_err("a description must be refused");
    assert!(
        error.contains("never with a description of a person"),
        "{error}"
    );
}

#[test]
fn a_couple_of_one_is_not_a_couple() {
    let error = output(&["person_1"], &[], 0.6, &["one"])
        .validate()
        .expect_err("one is not a pair");
    assert!(error.contains("two people or none"), "{error}");
}

#[test]
fn an_empty_couple_is_a_legitimate_answer() {
    output(&[], &[], 0.1, &["the evidence does not support a pair"])
        .validate()
        .expect("saying nothing must be allowed");
}

#[test]
fn the_same_person_cannot_be_both_halves() {
    let error = output(&["person_2", "person_2"], &[], 0.7, &["twice"])
        .validate()
        .expect_err("a duplicate must be refused");
    assert!(error.contains("same person"), "{error}");
}

#[test]
fn a_member_of_the_couple_cannot_also_be_close_family() {
    let error = output(&["person_1", "person_2"], &["person_1"], 0.7, &["both"])
        .validate()
        .expect_err("a contradiction must be refused");
    assert!(error.contains("close family"), "{error}");
}

#[test]
fn a_confident_answer_must_give_reasons() {
    let error = output(&["person_1", "person_2"], &[], 0.9, &[])
        .validate()
        .expect_err("invariant 2 is a validation rule, not a hope");
    assert!(error.contains("must give at least one reason"), "{error}");

    // A genuinely unsure answer may be silent.
    output(&[], &[], 0.1, &[])
        .validate()
        .expect("low confidence, no reasons");
}

#[test]
fn an_empty_reason_is_not_a_reason() {
    let error = output(&["person_1", "person_2"], &[], 0.8, &["   "])
        .validate()
        .expect_err("whitespace is not an explanation");
    assert!(error.contains("empty reason"), "{error}");
}

#[test]
fn a_confidence_outside_the_range_is_refused() {
    assert!(output(&[], &[], 1.4, &[]).validate().is_err());
    assert!(output(&[], &[], -0.2, &[]).validate().is_err());
}

#[test]
fn a_hallucinated_family_member_is_refused() {
    let error = output(&["person_1", "person_2"], &["person_99"], 0.7, &["ok"])
        .validate()
        .expect_err("a handle that was never supplied must be refused");
    assert!(error.contains("candidate handles"), "{error}");
}
