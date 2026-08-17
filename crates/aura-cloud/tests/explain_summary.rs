//! The one cloud call phase 13 is allowed to make, driven through the real gateway.
//!
//! Like every other test in this crate it runs with no network at all: the transport is a
//! cassette. What is tested is not "does a model answer" - a recording always answers - but
//! the four things section 7 constrains and the one thing this task exists to make
//! impossible.
//!
//! The four: no images in the payload, the cost ceiling, the cap of forty calls a wedding,
//! and a local fallback that is always available.
//!
//! The fifth is the point of the whole task. **A language model asked to explain a decision
//! will add a reason** - the light was lovely, the shutter was too slow, the exposure moved
//! 0.42 EV - and every one of those sentences is indistinguishable from a real one once it
//! is in a paragraph a photographer trusts. The output type has nowhere to put a new reason
//! and `validate` refuses any number that was not supplied, and both are tested here.

mod support;

use aura_cloud::contract::cloud::{CloudTask, Source, Validate};
use aura_cloud::explain_summary::{
    ExplainSummary, ExplainSummaryInput, ExplainSummaryOutput, ReasonFact, MAX_CALLS_PER_PROJECT,
    MAX_SUMMARY_CHARS,
};
use aura_cloud::CallContext;
use support::Harness;

macro_rules! context {
    ($harness:expr) => {
        CallContext {
            project: &$harness.project,
            decision_ref: Some("dec-explain"),
            cancel: &$harness.cancel,
        }
    };
}

fn reason(code: &str, text: &str, weight_milli: i32, severity: &str) -> ReasonFact {
    ReasonFact {
        code: code.to_string(),
        text: text.to_string(),
        weight_milli,
        severity: severity.to_string(),
        params: Vec::new(),
    }
}

/// A keeper with three reasons, one of them a caveat, at an uncalibrated confidence.
fn kept(decision: &str) -> ExplainSummaryInput {
    ExplainSummaryInput {
        decision_ref: decision.to_string(),
        kind: "cull".to_string(),
        verdict: "This photograph is in the gallery.".to_string(),
        confidence_milli: 810,
        autonomy: "suggest".to_string(),
        calibrated: false,
        reasons: vec![
            reason(
                "moment_winner",
                "the strongest frame of this moment",
                600,
                "credit",
            ),
            reason(
                "subject_soft",
                "the subject is softer than this camera should manage in this kind of photograph",
                -200,
                "fault",
            ),
            reason(
                "no_faces",
                "AURA found no faces here, so it judged this one on the frame alone",
                0,
                "caveat",
            ),
        ],
        alternative_score_milli: Some(770),
    }
}

#[test]
fn a_grounded_summary_comes_back_typed_with_its_provenance() {
    let harness = Harness::new();
    let result = harness
        .gateway
        .run(
            &ExplainSummary,
            &kept("dcn-explain-summary"),
            &context!(harness),
        )
        .expect("the gateway answers");

    assert_eq!(result.source, Source::Cloud);
    assert!(result.value.summary.contains("gallery"));
    assert!(result.value.headline.is_some());
    assert!(result.value.summary.chars().count() <= MAX_SUMMARY_CHARS);

    let rows = harness.audit.rows();
    let row = rows.first().expect("one audit row");
    assert_eq!(row.task, ExplainSummary::NAME);
    assert_eq!(row.task_version, ExplainSummary::VERSION);
    assert_eq!(row.status, "ok");
    assert_eq!(row.decision_ref.as_deref(), Some("dec-explain"));
    assert!(
        row.image_hashes.is_empty(),
        "this task sends no image, ever - that is a privacy property rather than a cost one"
    );
}

#[test]
fn an_invented_measurement_is_refused() {
    // The failure this task exists to prevent, checked directly on the validator rather
    // than through a recording: a summary that cites a shutter speed nobody supplied is a
    // summary that fabricated a measurement, and there is no way to tell a plausible
    // invention from a true one except by having said it first.
    let input = kept("dcn-explain-summary");
    let fabricated = ExplainSummaryOutput {
        summary: "AURA kept this frame; the shutter speed was 1/60 and exposure moved 0.42 EV."
            .to_string(),
        headline: None,
        allowed_numbers: input.allowed_numbers(),
    };
    let err = fabricated
        .validate()
        .expect_err("an invented number must be refused");
    assert!(err.contains("not in the supplied facts"), "{err}");

    // ...and a summary that uses only the numbers it was given passes.
    let grounded = ExplainSummaryOutput {
        summary: "This photograph is in the gallery, mostly because it is the strongest frame \
                  of its moment at 0.60. The closest alternative scored 0.77."
            .to_string(),
        headline: Some("The strongest frame of this moment".to_string()),
        allowed_numbers: input.allowed_numbers(),
    };
    grounded.validate().expect("a grounded summary is accepted");
}

#[test]
fn an_empty_or_oversized_summary_is_refused() {
    let empty = ExplainSummaryOutput {
        summary: "   ".to_string(),
        headline: None,
        allowed_numbers: Vec::new(),
    };
    assert!(
        empty.validate().is_err(),
        "an empty summary is not an explanation"
    );

    let long = ExplainSummaryOutput {
        summary: "word ".repeat(400),
        headline: None,
        allowed_numbers: Vec::new(),
    };
    assert!(long.validate().is_err(), "the panel's cap is enforced");
}

#[test]
fn the_local_fallback_is_correct_by_construction() {
    // Invariant 6: a task without a local fallback does not compile. This one is unusual
    // among the fallbacks in this crate in that it is not a degradation - every clause in
    // it is a clause a deciding phase wrote, and what the cloud adds is prose.
    let input = kept("dcn-offline");
    let answer = ExplainSummary
        .local_fallback(&input)
        .expect("the fallback cannot fail");

    assert!(answer
        .summary
        .starts_with("This photograph is in the gallery."));
    for fact in &input.reasons {
        assert!(
            answer.summary.contains(fact.text.trim()) || fact.weight_milli == 0,
            "every reason in the paragraph came from the input: {}",
            fact.code
        );
    }
    assert!(
        answer.summary.contains("rough guide"),
        "an uncalibrated confidence says so"
    );
    answer.validate().expect("the fallback grounds itself");
}

#[test]
fn the_cost_ceiling_and_the_call_cap_are_what_section_seven_says() {
    // Half a cent a call and forty calls a wedding: twenty cents, which is why this cap is
    // forty while `MomentSignificance`, at four times the price, is capped at twenty-five.
    assert!((ExplainSummary.max_cost_usd() - 0.005).abs() < f32::EPSILON);
    assert_eq!(MAX_CALLS_PER_PROJECT, 40);

    let prompt = ExplainSummary.prompt(&kept("dcn-explain-summary"));
    assert!(
        prompt.images.is_empty(),
        "no field of this task's payload can carry a photograph"
    );
    assert!(prompt.user.contains("moment_winner"), "the codes travel");
    assert!(
        prompt.user.contains("NOT been calibrated"),
        "the model is told when the confidence has not been checked against outcomes"
    );
}
