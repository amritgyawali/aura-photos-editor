// R1 exemption: `serde_json::json!` unwraps internally, and `scripts/check-banned.sh`
// already exempts `*/tests/*` from the no-unwrap rule. The allow is here so
// `clippy -D warnings` agrees with that policy rather than fighting it.
#![allow(clippy::disallowed_methods)]

//! The validator, the pruner and the extractor.
//!
//! Section 10.1: "malformed, truncated and extra-field responses trigger exactly
//! one repair, then fallback; never a panic". The gateway test proves the repair
//! behaviour; this file proves the thing underneath it - that the validator says
//! precisely what is wrong, in the same words on every machine.

use aura_cloud::contract::cloud::Validate;
use aura_cloud::schema::Schema;
use aura_cloud::tasks::{Scored, SegmentNamingOutput, SEGMENT_NAMING_SCHEMA};
use aura_cloud::validate;
use serde_json::json;

fn segment_schema() -> Schema {
    Schema::parse(SEGMENT_NAMING_SCHEMA).expect("the shipped schema parses")
}

#[test]
fn the_shipped_schema_is_inside_the_supported_subset() {
    // A schema using a keyword the validator does not implement must fail here,
    // at the task's own test, rather than silently passing everything at runtime.
    drop(segment_schema());
}

#[test]
fn an_unimplemented_keyword_is_refused_at_parse_time() {
    let with_ref = "{\"type\":\"object\",\"$ref\":\"#/$defs/other\"}";
    let err = Schema::parse(with_ref)
        .expect_err("a reference is not implemented and must not be ignored");
    assert_eq!(err.code.0, "AURA-CLOUD-6005");
}

#[test]
fn a_valid_answer_produces_no_errors() {
    let schema = segment_schema();
    let document = json!({
        "scene": "ceremony",
        "ritual": "vows",
        "tradition": "christian",
        "confidence": 0.72,
        "reasons": ["the couple face each other at an altar"],
        "boundary_hint": null,
        "notes": null
    });
    assert!(schema.validate(&document).is_empty());
    assert_eq!(schema.explain(&document), None);
}

#[test]
fn every_problem_is_reported_at_once_and_in_a_stable_order() {
    let schema = segment_schema();
    let document = json!({
        "confidence": 1.4,
        "reasons": ["a", "b", "c", "d", "e", "f"]
    });

    let first = schema.explain(&document).expect("three problems");
    let second = schema.explain(&document).expect("three problems");
    assert_eq!(first, second, "the same document must explain identically");

    assert!(first.contains("/confidence"), "{first}");
    assert!(first.contains("/reasons"), "{first}");
    assert!(first.contains("/scene"), "{first}");
    assert!(first.contains("at most 1"), "{first}");
    assert!(first.contains("at most 5 items"), "{first}");
    assert!(first.contains("required, but missing"), "{first}");
}

#[test]
fn a_type_mismatch_names_what_was_expected_and_what_was_found() {
    let schema = segment_schema();
    let document = json!({ "scene": 7, "confidence": 0.5, "reasons": [] });
    let complaint = schema.explain(&document).expect("a type error");
    assert!(
        complaint.contains("/scene: expected string, found number"),
        "{complaint}"
    );
}

#[test]
fn a_nullable_field_accepts_both_a_value_and_null() {
    let schema = segment_schema();
    for ritual in [json!("vows"), json!(null)] {
        let document = json!({
            "scene": "ceremony",
            "ritual": ritual,
            "confidence": 0.5,
            "reasons": ["evidence"]
        });
        assert!(schema.validate(&document).is_empty());
    }
}

#[test]
fn an_undefined_field_is_pruned_rather_than_rejected() {
    let schema = segment_schema();
    let mut document = json!({
        "scene": "dancing",
        "confidence": 0.6,
        "reasons": ["a mirror ball and motion blur"],
        "lighting": "tungsten",
        "camera_count": 2
    });

    assert_eq!(schema.validate(&document).len(), 2, "before pruning");
    assert_eq!(schema.prune(&mut document), 2);
    assert!(schema.validate(&document).is_empty(), "after pruning");
    assert!(document.get("lighting").is_none());
}

#[test]
fn an_unknown_enum_member_becomes_unknown_when_the_schema_offers_it() {
    let schema =
        Schema::parse(r#"{"type":"object","properties":{"tier":{"enum":["a","b","unknown"]}}}"#)
            .expect("parses");

    let mut document = json!({ "tier": "z" });
    assert_eq!(schema.coerce_unknown_enums(&mut document), 1);
    assert_eq!(
        document.get("tier").and_then(|v| v.as_str()),
        Some("unknown")
    );

    // A schema that does not offer `unknown` would rather fail than guess.
    let strict = Schema::parse(r#"{"type":"object","properties":{"tier":{"enum":["a","b"]}}}"#)
        .expect("parses");
    let mut other = json!({ "tier": "z" });
    assert_eq!(strict.coerce_unknown_enums(&mut other), 0);
    assert!(!strict.validate(&other).is_empty());
}

#[test]
fn json_is_found_inside_a_markdown_fence() {
    let fenced = "```json\n{\"scene\":\"details\"}\n```";
    assert_eq!(
        validate::extract_json(fenced),
        Some("{\"scene\":\"details\"}")
    );
}

#[test]
fn json_is_found_after_a_sentence_of_preamble() {
    let chatty =
        "Sure! Here is the result:\n{\"scene\":\"details\"}\nLet me know if you need more.";
    assert_eq!(
        validate::extract_json(chatty),
        Some("{\"scene\":\"details\"}")
    );
}

#[test]
fn a_brace_inside_a_string_does_not_confuse_the_extractor() {
    let tricky = r#"{"notes":"the sign read \"} closed {\" in the background","scene":"venue"}"#;
    assert_eq!(validate::extract_json(tricky), Some(tricky));
}

#[test]
fn a_truncated_object_yields_no_json_at_all() {
    let truncated = "{\"scene\":\"speeches\",\"reasons\":[\"a man with a micro";
    assert_eq!(validate::extract_json(truncated), None);
}

#[test]
fn the_whole_chain_produces_a_typed_value() {
    let schema = segment_schema();
    let text = "```json\n{\"scene\":\"ritual\",\"ritual\":\"haldi\",\"tradition\":\"hindu\",\
                \"confidence\":0.9,\"reasons\":[\"turmeric paste on the arms\"],\"extra\":1}\n```";

    let parsed = validate::parse::<SegmentNamingOutput>("segment_naming", &schema, text)
        .expect("the chain recovers a fenced answer with an extra field");
    assert_eq!(parsed.value.scene, "ritual");
    assert_eq!(parsed.pruned, 1);
    assert_eq!(parsed.coerced, 0);
    assert!(parsed.value.confidence() > 0.8);
}

#[test]
fn a_scene_outside_the_vocabulary_fails_the_semantic_check() {
    let output = SegmentNamingOutput {
        scene: "elephant_procession".to_string(),
        ritual: None,
        tradition: None,
        confidence: 0.7,
        reasons: vec!["a decorated animal".to_string()],
        boundary_hint: None,
        notes: None,
    };
    let complaint = output.validate().expect_err("not in allowed_scenes");
    assert!(complaint.contains("/scene"), "{complaint}");
    assert!(complaint.contains("allowed_scenes"), "{complaint}");
}

#[test]
fn a_confident_decision_without_reasons_is_a_bug_and_is_rejected() {
    // Invariant 2, enforced by the type rather than by review.
    let output = SegmentNamingOutput {
        scene: "ceremony".to_string(),
        ritual: None,
        tradition: None,
        confidence: 0.95,
        reasons: Vec::new(),
        boundary_hint: None,
        notes: None,
    };
    let complaint = output
        .validate()
        .expect_err("no reasons at high confidence");
    assert!(complaint.contains("/reasons"), "{complaint}");

    // An unsure answer with no reasons is allowed: it is honest.
    let unsure = SegmentNamingOutput {
        confidence: 0.2,
        ..output
    };
    assert!(unsure.validate().is_ok());
}

#[test]
fn an_empty_reason_is_not_a_reason() {
    let output = SegmentNamingOutput {
        scene: "portraits".to_string(),
        ritual: None,
        tradition: None,
        confidence: 0.8,
        reasons: vec!["  ".to_string()],
        boundary_hint: None,
        notes: None,
    };
    assert!(output.validate().is_err());
}
