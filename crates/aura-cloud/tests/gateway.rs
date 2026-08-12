//! Section 10.1's acceptance tests, driven through the real gateway.
//!
//! Every one of these runs with no network at all: the transport is either a
//! cassette or the offline refusal. That is acceptance criterion 7, and it is
//! also what makes these tests worth running on a train.

mod support;

use std::sync::Arc;

use aura_cloud::compat;
use aura_cloud::contract::cloud::{CloudTask, Source};
use aura_cloud::google::GoogleProvider;
use aura_cloud::openai::OpenAiProvider;
use aura_cloud::provider::OfflineTransport;
use aura_cloud::tasks::{Scored, SegmentNaming};
use aura_cloud::{CallContext, CloudPolicy};
use support::Harness;

/// A context for the harness's project, with nothing cancelled.
macro_rules! context {
    ($harness:expr) => {
        CallContext {
            project: &$harness.project,
            decision_ref: Some("dec-1"),
            cancel: &$harness.cancel,
        }
    };
}

#[test]
fn a_valid_answer_comes_back_typed_with_its_provenance() {
    let harness = Harness::new();
    let task = harness.task(12);
    let input = support::segment("seg-mehndi", "candid", 410, 12);

    let result = harness
        .gateway
        .run(&task, &input, &context!(harness))
        .expect("the gateway answers");

    assert_eq!(result.source, Source::Cloud);
    assert_eq!(result.value.scene, "ritual");
    assert_eq!(result.value.ritual.as_deref(), Some("mehndi"));
    assert_eq!(result.value.tradition.as_deref(), Some("hindu"));
    assert!(result.value.confidence() > 0.8);
    assert!(!result.value.reasons().is_empty(), "invariant 2");
    assert_eq!(result.tokens_in, 2871);
    assert!(result.cost_usd > 0.0, "a cloud answer is billed");

    // The audit row is the trail acceptance criterion 4 asks for.
    let rows = harness.audit.rows();
    assert_eq!(rows.len(), 1);
    let row = rows.first().expect("one row");
    assert_eq!(row.task, SegmentNaming::NAME);
    assert_eq!(row.task_version, SegmentNaming::VERSION);
    assert_eq!(row.status, "ok");
    assert_eq!(row.retry_count, 0);
    assert_eq!(row.decision_ref.as_deref(), Some("dec-1"));
    assert_eq!(row.image_hashes.len(), 1, "one contact sheet was sent");
    assert!(!row.prompt_hash.is_empty());
}

#[test]
fn the_second_identical_question_is_answered_from_the_cache() {
    let harness = Harness::new();
    let task = harness.task(12);
    let input = support::segment("seg-mehndi", "candid", 410, 12);

    let first = harness
        .gateway
        .run(&task, &input, &context!(harness))
        .expect("first answer");
    let second = harness
        .gateway
        .run(&task, &input, &context!(harness))
        .expect("second answer");

    assert_eq!(first.source, Source::Cloud);
    assert_eq!(second.source, Source::Cache);
    // Section 10.1: a re-run produces identical decisions.
    assert_eq!(first.value, second.value);
    assert_eq!(second.cost_usd, 0.0, "a cache hit is never billed");
    assert_eq!(
        harness.transport.seen().len(),
        1,
        "the second question did not reach the provider"
    );
    assert!(harness.gateway.ledger().cache_hit_rate() >= 0.5);
}

#[test]
fn a_malformed_answer_is_repaired_exactly_once() {
    let harness = Harness::new();
    let task = harness.task(8);
    let input = support::segment("seg-ring-exchange", "ceremony", 600, 8);

    let result = harness
        .gateway
        .run(&task, &input, &context!(harness))
        .expect("the repair succeeds");

    assert_eq!(result.source, Source::Cloud);
    assert_eq!(result.value.ritual.as_deref(), Some("ring_exchange"));
    assert_eq!(
        harness.transport.seen().len(),
        2,
        "exactly one repair round trip"
    );

    let rows = harness.audit.rows();
    let row = rows.first().expect("one row");
    assert_eq!(row.status, "repaired");
    assert_eq!(row.retry_count, 1);
    // Both attempts are billed, so both are charged.
    assert_eq!(row.tokens_in, 2810 + 3104);
    // The counters describe the answer that was kept. The first attempt's extra
    // `lighting` field was pruned and then the whole attempt was discarded for a
    // confidence of 1.4, so the surviving answer has nothing to prune - which is
    // what these being zero means, and is worth asserting so a future change that
    // started reporting the discarded attempt's counters is caught.
    assert_eq!(row.pruned_fields, 0);
    assert_eq!(row.coerced_enums, 0);
}

#[test]
fn an_unrepairable_answer_falls_back_locally_and_never_panics() {
    let harness = Harness::new();
    let task = harness.task(8);
    let input = support::segment("seg-unrepairable", "ceremony", 550, 8);

    let result = harness
        .gateway
        .run(&task, &input, &context!(harness))
        .expect("the pipeline still completes");

    assert_eq!(result.source, Source::LocalFallback);
    assert_eq!(result.value.scene, "ceremony", "the local argmax answered");
    assert_eq!(result.cost_usd, 0.0);
    assert_eq!(
        harness.transport.seen().len(),
        2,
        "one attempt and one repair, then it stopped"
    );

    let rows = harness.audit.rows();
    let row = rows.first().expect("a row is written even for a fallback");
    assert_eq!(row.source, Source::LocalFallback);
    assert_eq!(row.status, "schema_invalid");
    assert_eq!(row.fallback_reason.as_deref(), Some("schema_invalid"));
}

#[test]
fn a_truncated_answer_is_a_schema_failure_not_a_crash() {
    let harness = Harness::new();
    let task = harness.task(6);
    let input = support::segment("seg-truncated", "speeches", 700, 6);

    let result = harness
        .gateway
        .run(&task, &input, &context!(harness))
        .expect("the pipeline still completes");

    assert_eq!(result.source, Source::LocalFallback);
    assert_eq!(result.value.scene, "speeches");
}

#[test]
fn with_the_network_unplugged_everything_still_completes() {
    let harness = Harness::builder()
        .transport(Arc::new(OfflineTransport))
        .build();
    let task = harness.task(12);

    // A whole wedding's worth of segments, every one of them answered locally.
    for index in 0..40 {
        let input = support::segment(&format!("seg-{index}"), "dancing", 480, 12);
        let result = harness
            .gateway
            .run(&task, &input, &context!(harness))
            .expect("offline must not stop the pipeline");
        assert_eq!(result.source, Source::LocalFallback);
        assert_eq!(result.value.scene, "dancing");
        assert!(
            !result.value.reasons().is_empty(),
            "invariant 2 offline too"
        );
    }

    assert_eq!(harness.gateway.ledger().fallbacks(), 40);
    assert_eq!(harness.audit.rows().len(), 40);
    assert!(harness.gateway.ledger().has_recoverable_gaps());
}

#[test]
fn offline_studio_mode_makes_the_crate_inert() {
    let harness = Harness::builder()
        .policy(CloudPolicy {
            offline_studio_mode: true,
            project_enabled: true,
            ..CloudPolicy::default()
        })
        .build();
    let task = harness.task(12);
    let input = support::segment("seg-mehndi", "candid", 410, 12);

    let result = harness
        .gateway
        .run(&task, &input, &context!(harness))
        .expect("inert is not broken");

    assert_eq!(result.source, Source::LocalFallback);
    assert!(
        harness.transport.seen().is_empty(),
        "nothing was built and nothing was sent"
    );
    let rows = harness.audit.rows();
    assert_eq!(
        rows.first().and_then(|row| row.fallback_reason.as_deref()),
        Some("disabled")
    );
}

#[test]
fn a_project_with_cloud_off_never_calls() {
    let harness = Harness::builder().policy(CloudPolicy::default()).build();
    let task = harness.task(12);
    let input = support::segment("seg-mehndi", "candid", 410, 12);

    let result = harness
        .gateway
        .run(&task, &input, &context!(harness))
        .expect("still completes");
    assert_eq!(result.source, Source::LocalFallback);
    assert!(harness.transport.seen().is_empty());
}

#[test]
fn without_consent_nothing_is_uploaded() {
    let harness = Harness::builder().without_consent().build();
    let task = harness.task(12);
    let input = support::segment("seg-mehndi", "candid", 410, 12);

    let result = harness
        .gateway
        .run(&task, &input, &context!(harness))
        .expect("still completes");

    assert_eq!(result.source, Source::LocalFallback);
    assert!(harness.transport.seen().is_empty());
    let rows = harness.audit.rows();
    assert_eq!(
        rows.first().and_then(|row| row.fallback_reason.as_deref()),
        Some("no_consent")
    );
}

#[test]
fn without_a_key_the_credential_store_is_never_even_read() {
    let harness = Harness::builder().without_key().build();
    let task = harness.task(12);
    let input = support::segment("seg-mehndi", "candid", 410, 12);

    let result = harness
        .gateway
        .run(&task, &input, &context!(harness))
        .expect("still completes");

    assert_eq!(result.source, Source::LocalFallback);
    let rows = harness.audit.rows();
    assert_eq!(
        rows.first().and_then(|row| row.fallback_reason.as_deref()),
        Some("no_key")
    );
}

#[test]
fn a_project_at_its_cap_stops_calling_and_still_finishes() {
    // A cap smaller than one call's estimate, so the first call is refused.
    let harness = Harness::builder().cap_usd(0.0005).build();
    let task = harness.task(12);

    for index in 0..5 {
        let input = support::segment(&format!("seg-{index}"), "dinner", 520, 12);
        let result = harness
            .gateway
            .run(&task, &input, &context!(harness))
            .expect("a cap is a stop, not a failure");
        assert_eq!(result.source, Source::LocalFallback);
        assert_eq!(result.value.scene, "dinner");
    }

    assert!(harness.transport.seen().is_empty(), "nothing was billed");
    let rows = harness.audit.rows();
    assert_eq!(rows.len(), 5, "every downgraded decision is recorded");
    for row in &rows {
        assert!(matches!(
            row.fallback_reason.as_deref(),
            Some("budget_reached" | "cost_ceiling")
        ));
    }
}

#[test]
fn a_rate_limited_provider_backs_off_then_answers_locally() {
    let harness = Harness::new();
    let task = harness.task(6);
    let input = support::segment("seg-busy", "dancing", 500, 6);

    let result = harness
        .gateway
        .run(&task, &input, &context!(harness))
        .expect("a busy provider must not stop the pipeline");

    assert_eq!(result.source, Source::LocalFallback);
    assert_eq!(
        harness.transport.seen().len(),
        3,
        "the bounded attempt count, and no more"
    );
}

#[test]
fn a_captive_portal_is_a_provider_error_not_a_schema_error() {
    let harness = Harness::new();
    let task = harness.task(6);
    let input = support::segment("seg-portal", "venue", 430, 6);

    let result = harness
        .gateway
        .run(&task, &input, &context!(harness))
        .expect("still completes");

    assert_eq!(result.source, Source::LocalFallback);
    let rows = harness.audit.rows();
    assert_eq!(
        rows.first().and_then(|row| row.fallback_reason.as_deref()),
        Some("provider_failed")
    );
}

#[test]
fn a_rejected_key_opens_the_breaker_after_one_rejection() {
    let harness = Harness::new();
    let task = harness.task(6);

    let rejected = support::segment("seg-rejected", "portraits", 400, 6);
    let first = harness
        .gateway
        .run(&task, &rejected, &context!(harness))
        .expect("still completes");
    assert_eq!(first.source, Source::LocalFallback);
    assert!(harness.gateway.client().breaker().is_open());

    // The breaker means the next question is not even attempted, which is the
    // difference between one rejection and seventy-five.
    let before = harness.transport.seen().len();
    let good = support::segment("seg-mehndi", "candid", 410, 12);
    let second = harness
        .gateway
        .run(&harness.task(12), &good, &context!(harness))
        .expect("still completes");
    assert_eq!(second.source, Source::LocalFallback);
    assert_eq!(harness.transport.seen().len(), before);
}

#[test]
fn the_same_task_on_three_providers_gives_comparable_decisions() {
    let anthropic = Harness::new();
    let openai = Harness::builder()
        .provider(Arc::new(OpenAiProvider::new("https://api.openai.com")))
        .build();
    let google = Harness::builder()
        .provider(Arc::new(GoogleProvider::new(
            "https://generativelanguage.googleapis.com",
        )))
        .build();
    let local = Harness::builder()
        .provider(Arc::new(compat::provider(
            "http://127.0.0.1:11434",
            "local-model",
        )))
        .build();

    let input = support::segment("seg-mehndi", "candid", 410, 12);
    for harness in [&anthropic, &openai, &google, &local] {
        let task = harness.task(12);
        let result = harness
            .gateway
            .run(&task, &input, &context!(harness))
            .expect("every provider answers");
        assert_eq!(result.source, Source::Cloud, "schema-valid on every vendor");
        assert_eq!(result.value.scene, "ritual");
        assert_eq!(result.value.ritual.as_deref(), Some("mehndi"));
        assert!(!result.value.reasons().is_empty());
    }

    // A local server bills nothing, and the governor must not invent a cost.
    let task = local.task(12);
    let free = local
        .gateway
        .run(
            &task,
            &support::segment("seg-mehndi", "candid", 410, 12),
            &context!(local),
        )
        .expect("answers");
    assert_eq!(free.cost_usd, 0.0);
}

#[test]
fn cancelling_stops_before_the_provider_is_asked() {
    let harness = Harness::new();
    harness.cancel.cancel();
    let task = harness.task(12);
    let input = support::segment("seg-mehndi", "candid", 410, 12);

    let err = harness
        .gateway
        .run(&task, &input, &context!(harness))
        .expect_err("a cancelled run stops");
    assert_eq!(err.code.0, "AURA-CLOUD-6013");
    assert!(harness.transport.seen().is_empty());
}

#[test]
fn every_cassette_is_played_by_something() {
    // A recording nobody plays is a test that stopped running, or a matcher that
    // stopped matching. Both are worth failing over, so this test drives every
    // scenario once and then asserts the set is empty.
    let harness = Harness::new();
    // `seg-busy` is last on purpose: three rate-limited attempts trip the
    // breaker, and nothing after it would be attempted at all.
    for (segment, tiles, best) in [
        ("seg-mehndi", 12, "candid"),
        ("seg-ring-exchange", 8, "ceremony"),
        ("seg-unrepairable", 8, "ceremony"),
        ("seg-truncated", 6, "speeches"),
        ("seg-portal", 6, "venue"),
        ("seg-busy", 6, "dancing"),
    ] {
        let task = harness.task(tiles);
        let input = support::segment(segment, best, 500, tiles);
        drop(harness.gateway.run(&task, &input, &context!(harness)));
    }
    // The rejected-key and probe cassettes are played by their own tests, and
    // the other three vendors' by the provider-swap test; this run covers the
    // Anthropic scenarios only, so the remaining names are listed explicitly.
    let unplayed = harness.transport.unplayed();
    let expected = [
        "040-openai-mehndi",
        "050-google-mehndi",
        "060-compat-mehndi",
        "070-anthropic-key-rejected",
        "100-anthropic-probe-ok",
    ];
    for name in unplayed {
        assert!(
            expected.contains(&name.as_str()),
            "{name} is a recording nothing plays"
        );
    }
}
