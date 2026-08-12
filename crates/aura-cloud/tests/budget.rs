//! The cost governor, the cache key, and the retry schedule.
//!
//! The tests that matter here are the ones about *not* spending: a call priced
//! before it is made, a tier dropped rather than a decision lost, and a cache key
//! that does not move when the toolchain does.

use std::sync::Arc;

use aura_cloud::anthropic;
use aura_cloud::budget::{
    BudgetState, BudgetStore, CostGovernor, MemoryBudget, DOWNGRADE_THRESHOLD, MONTH_SCOPE,
};
use aura_cloud::cache::{self, CacheEntry, MemoryCache, ResponseCache};
use aura_cloud::contract::cloud::{ImagePart, PromptSpec, Tier};
use aura_cloud::provider::{ProviderConfig, ProviderKind, RetryPolicy};

const PROJECT: &str = "prj_00000000-0000-7000-8000-000000000001";

fn config() -> ProviderConfig {
    ProviderConfig {
        kind: ProviderKind::Anthropic,
        endpoint: "https://api.anthropic.com".to_string(),
        aliases: anthropic::default_aliases(),
        pinned_host: None,
    }
}

fn prompt(images: usize) -> PromptSpec {
    let parts: Vec<ImagePart> = (0..images)
        .map(|index| ImagePart {
            media_type: "image/jpeg".to_string(),
            bytes: vec![0xFF, 0xD8, 0xFF],
            content_hash: format!("hash-{index}"),
            width: 1536,
            height: 1152,
        })
        .collect();
    PromptSpec::new("system", "user")
        .with_images(parts)
        .with_max_tokens(512)
        .with_min_tier(Tier::Balanced)
}

fn governor(cap_usd: f64) -> CostGovernor {
    let store = MemoryBudget::new();
    store.set_cap(PROJECT, cap_usd, true).expect("set");
    store.set_cap(MONTH_SCOPE, 30.0, true).expect("set");
    CostGovernor::new(Arc::new(store))
}

#[test]
fn a_call_is_priced_before_it_is_made() {
    let governor = governor(1.50);
    let plan = governor
        .plan("segment_naming", PROJECT, &config(), &prompt(1), 0.02)
        .expect("affordable");

    assert_eq!(plan.tier, Tier::Balanced);
    assert_eq!(plan.model.model, "claude-sonnet-4-5");
    assert!(plan.tokens_in > 2_000, "the sheet dominates the estimate");
    assert!(plan.estimate_usd > 0.0 && plan.estimate_usd <= 0.02);
    assert!(!plan.downgraded);
}

#[test]
fn a_seventy_five_call_wedding_fits_the_budget() {
    // Section 11: <= 75 calls and <= USD 1.50 for a 3,000 image wedding. This is
    // that arithmetic, asserted rather than hoped for.
    let governor = governor(1.50);
    let plan = governor
        .plan("segment_naming", PROJECT, &config(), &prompt(1), 0.02)
        .expect("affordable");
    let projected = plan.estimate_usd * 75.0;
    assert!(
        projected <= 1.50,
        "75 calls at {:.4} is {projected:.2}, over the USD 1.50 budget",
        plan.estimate_usd
    );
}

#[test]
fn a_low_budget_drops_a_tier_rather_than_the_decision() {
    let store = MemoryBudget::new();
    store.set_cap(PROJECT, 1.00, true).expect("set");
    store.set_cap(MONTH_SCOPE, 30.0, true).expect("set");
    // Spend past the downgrade threshold but not past the cap.
    store
        .charge(PROJECT, 1.00 * (1.0 - DOWNGRADE_THRESHOLD / 2.0), false)
        .expect("charge");
    let governor = CostGovernor::new(Arc::new(store));

    let plan = governor
        .plan("segment_naming", PROJECT, &config(), &prompt(1), 0.02)
        .expect("still affordable, one tier down");
    assert_eq!(plan.tier, Tier::Cheap);
    assert!(plan.downgraded, "the downgrade is recorded");
}

#[test]
fn a_cap_that_is_already_spent_refuses_before_anything_is_built() {
    let store = MemoryBudget::new();
    store.set_cap(PROJECT, 0.10, true).expect("set");
    store.charge(PROJECT, 0.10, false).expect("charge");
    let governor = CostGovernor::new(Arc::new(store));

    let err = governor
        .plan("segment_naming", PROJECT, &config(), &prompt(1), 0.02)
        .expect_err("the cap is reached");
    assert_eq!(err.code.0, "AURA-CLOUD-6006");
}

#[test]
fn a_task_ceiling_smaller_than_the_cheapest_tier_is_its_own_error() {
    // "You set a small cap" and "this task is priced wrong" are different
    // conversations, so they are different codes.
    let governor = governor(100.0);
    let err = governor
        .plan("segment_naming", PROJECT, &config(), &prompt(4), 0.000_01)
        .expect_err("no tier is that cheap");
    assert_eq!(err.code.0, "AURA-CLOUD-6014");
}

#[test]
fn a_soft_cap_does_not_stop_the_run() {
    let store = MemoryBudget::new();
    store.set_cap(PROJECT, 0.10, false).expect("soft cap");
    store.charge(PROJECT, 0.50, false).expect("overspent");
    let state = store.state(PROJECT).expect("read");
    assert!(!state.is_stopped(), "a soft cap warns rather than stops");
}

#[test]
fn spending_is_charged_against_both_the_project_and_the_month() {
    let store = Arc::new(MemoryBudget::new());
    let governor = CostGovernor::new(Arc::clone(&store) as Arc<dyn BudgetStore>);
    governor.charge(PROJECT, 0.02, true).expect("charge");

    assert!((store.state(PROJECT).expect("read").spent_usd - 0.02).abs() < 1e-9);
    assert!((store.state(MONTH_SCOPE).expect("read").spent_usd - 0.02).abs() < 1e-9);
    assert_eq!(store.state(PROJECT).expect("read").downgrades, 1);
}

#[test]
fn the_remaining_fraction_drives_the_downgrade_decision() {
    let state = BudgetState {
        cap_usd: 1.0,
        spent_usd: 0.8,
        ..BudgetState::default()
    };
    assert!((state.remaining_fraction() - 0.2).abs() < 1e-9);
    assert!(state.should_downgrade());

    let healthy = BudgetState {
        cap_usd: 1.0,
        spent_usd: 0.5,
        ..BudgetState::default()
    };
    assert!(!healthy.should_downgrade());
}

#[test]
fn a_region_pin_refuses_any_other_host() {
    let mut pinned = config();
    pinned.pinned_host = Some("api.eu.anthropic.com".to_string());
    assert!(!pinned.host_is_permitted());

    pinned.endpoint = "https://api.eu.anthropic.com".to_string();
    assert!(pinned.host_is_permitted());
}

#[test]
fn a_tier_a_provider_does_not_offer_falls_back_to_a_cheaper_one() {
    let mut sparse = config();
    sparse.aliases.remove(&Tier::Reasoning);
    let chosen = sparse.alias(Tier::Reasoning).expect("something answers");
    assert_eq!(chosen.model, "claude-sonnet-4-5");
}

#[test]
fn the_cache_key_covers_everything_that_changes_the_answer() {
    let base = cache::key("segment_naming", 1, &prompt(1), "claude-sonnet-4-5");

    assert_ne!(
        base,
        cache::key("other_task", 1, &prompt(1), "claude-sonnet-4-5")
    );
    assert_ne!(
        base,
        cache::key("segment_naming", 2, &prompt(1), "claude-sonnet-4-5")
    );
    assert_ne!(
        base,
        cache::key("segment_naming", 1, &prompt(2), "claude-sonnet-4-5")
    );
    assert_ne!(
        base,
        cache::key("segment_naming", 1, &prompt(1), "claude-haiku-4-5")
    );

    // And it is stable for the same inputs, which is the whole point.
    assert_eq!(
        base,
        cache::key("segment_naming", 1, &prompt(1), "claude-sonnet-4-5")
    );
    assert_eq!(base.len(), 64);
}

#[test]
fn the_prompt_hash_ignores_the_token_ceiling_but_not_the_text() {
    let one = prompt(1);
    let two = one.clone().with_max_tokens(4096);
    assert_eq!(one.prompt_hash(), two.prompt_hash());

    let different = PromptSpec::new("system", "a different question");
    assert_ne!(one.prompt_hash(), different.prompt_hash());
}

#[test]
fn the_cache_round_trips_and_counts_its_hits() {
    let cache = MemoryCache::new();
    let key = cache::key("segment_naming", 1, &prompt(1), "claude-sonnet-4-5");

    assert_eq!(cache.get(&key).expect("read"), None);
    cache
        .put(&CacheEntry {
            key: &key,
            task: "segment_naming",
            task_version: 1,
            model: "claude-sonnet-4-5",
            response_json: "{\"scene\":\"ritual\"}",
        })
        .expect("write");

    assert_eq!(
        cache.get(&key).expect("read").as_deref(),
        Some("{\"scene\":\"ritual\"}")
    );
    assert_eq!(cache.stats().expect("stats").entries, 1);
    assert_eq!(cache.stats().expect("stats").hits, 1);

    // A task version bump orphans its own entries, which is how a prompt edit
    // invalidates the answers made under the old prompt.
    assert_eq!(cache.purge_task("segment_naming", 2).expect("purge"), 0);
    assert_eq!(cache.purge_task("segment_naming", 1).expect("purge"), 1);
    assert_eq!(cache.get(&key).expect("read"), None);
}

#[test]
fn the_backoff_schedule_is_deterministic_and_bounded() {
    let policy = RetryPolicy::default();
    let seed = "a1b2c3";

    let first: Vec<u64> = (1..=3).map(|n| policy.backoff_ms(n, seed, None)).collect();
    let second: Vec<u64> = (1..=3).map(|n| policy.backoff_ms(n, seed, None)).collect();
    assert_eq!(
        first, second,
        "the same request retries on the same schedule"
    );

    for wait in &first {
        assert!(*wait <= policy.max_backoff_ms, "{wait} is over the ceiling");
    }
    // Two different requests spread out rather than retrying in lockstep.
    assert_ne!(
        first,
        (1..=3)
            .map(|n| policy.backoff_ms(n, "z9y8x7", None))
            .collect::<Vec<_>>()
    );

    // A provider that asked for a specific pause gets it, capped.
    assert_eq!(policy.backoff_ms(1, seed, Some(2_000)), 2_000);
    assert_eq!(
        policy.backoff_ms(1, seed, Some(60_000)),
        policy.max_backoff_ms
    );
}
