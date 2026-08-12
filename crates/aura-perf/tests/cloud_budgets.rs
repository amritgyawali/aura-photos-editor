//! Phase 04 budgets, asserted against the real gateway.
//!
//! Provider latency is not here and never will be: it is somebody else's
//! service, it varies by an order of magnitude between a good afternoon and a
//! bad one, and a budget nobody can act on is a flaky test. What is asserted is
//! everything AURA actually controls - the gateway's own cost per call, the time
//! to build the thing that gets uploaded, how many calls a wedding makes, what
//! they cost, and how much of a run a total outage costs.
//!
//! The transport is the offline refusal or a cassette, so these numbers are the
//! gateway's own work with nothing else in them, and the test runs on a machine
//! with no network.
//!
//! **Run these in release.** A budget is a statement about the binary a
//! photographer runs, and the payload builder is a pixel loop: unoptimised it is
//! more than an order of magnitude slower, which would either fail honestly or
//! force a budget so loose it stopped meaning anything. CI runs
//! `cargo test --release --package aura-perf`; a debug run reports the number
//! and says it did not assert it, rather than asserting the wrong one.

use std::sync::Arc;

use aura_catalog::consent::AlwaysConsent;
use aura_cloud::anthropic::{self, AnthropicProvider};
use aura_cloud::audit::{AuditSink, MemoryAudit};
use aura_cloud::budget::{BudgetStore, CostGovernor, MemoryBudget};
use aura_cloud::cache::{MemoryCache, ResponseCache};
use aura_cloud::contract::cloud::{ImagePart, PromptSpec, Tier};
use aura_cloud::keys::{KeyStore, MemoryKeyStore};
use aura_cloud::payload::{self, PayloadPolicy, SourceImage};
use aura_cloud::provider::{
    estimate_tokens_in, OfflineTransport, ProviderClient, ProviderConfig, ProviderKind,
    RecordingSleeper, RetryPolicy, Transport,
};
use aura_cloud::redact::ExifSummary;
use aura_cloud::tasks::{SceneGuess, SegmentInput, SegmentNaming, IMAGES_PER_CALL};
use aura_cloud::{CallContext, CloudAiGateway, CloudPolicy};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::consent::ConsentGate;
use aura_core::progress::CancelToken;
use aura_core::ProjectId;
use aura_perf::{Budgets, Measurement, StageTimer};
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};

/// Images in the reference wedding, from section 11.
const WEDDING_IMAGES: u32 = 3_000;

/// Assert a timing budget, or explain why it was only reported.
fn assert_timing(measurement: &Measurement) {
    if cfg!(debug_assertions) {
        println!(
            "{}: {} ms - not asserted in a debug build; \
             run `cargo test --release --package aura-perf`",
            measurement.stage, measurement.elapsed_ms
        );
        return;
    }
    if let Err(breach) = budgets().check(measurement) {
        panic!("{breach}");
    }
}

fn budgets() -> Budgets {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../perf/budgets.toml");
    Budgets::load(&path).expect("perf/budgets.toml must parse")
}

fn clock() -> Arc<dyn Clock> {
    Arc::new(SystemClock::default())
}

fn frame(width: u32, height: u32, seed: u8) -> PixelBuffer {
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.push(((x * 3 + u32::from(seed)) % 256) as u8);
            pixels.push(((y * 5 + u32::from(seed) * 7) % 256) as u8);
            pixels.push((((x + y) * 2 + u32::from(seed) * 11) % 256) as u8);
        }
    }
    PixelBuffer {
        width,
        height,
        data: PixelData::Srgb8(pixels),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 1,
    }
}

fn sheet(tiles: usize) -> ImagePart {
    let frames: Vec<PixelBuffer> = (0..tiles)
        .map(|index| frame(640, 480, u8::try_from(index).unwrap_or(0)))
        .collect();
    let sources: Vec<SourceImage<'_>> = frames.iter().map(SourceImage::new).collect();
    payload::contact_sheet(&sources, PayloadPolicy::default()).expect("the sheet must build")
}

fn segment(id: &str) -> SegmentInput {
    SegmentInput {
        segment_id: id.to_string(),
        start_offset_s: 0,
        end_offset_s: 1_800,
        frame_count: IMAGES_PER_CALL,
        local_guesses: vec![SceneGuess::new("dancing", 480)],
        exif: vec![ExifSummary {
            camera: Some("Canon EOS R5".to_string()),
            iso: Some(3200),
            ..ExifSummary::default()
        }],
        tile_hashes: vec!["t0".to_string()],
    }
}

fn offline_gateway() -> CloudAiGateway {
    let clock = clock();
    let client = Arc::new(
        ProviderClient::new(
            Arc::new(AnthropicProvider::default()),
            Arc::new(OfflineTransport) as Arc<dyn Transport>,
            Arc::new(RecordingSleeper::default()),
        )
        .with_policy(RetryPolicy {
            attempts: 1,
            base_ms: 0,
            max_backoff_ms: 0,
            timeout: std::time::Duration::from_millis(10),
        }),
    );
    CloudAiGateway::new(
        client,
        Arc::new(MemoryKeyStore::new()) as Arc<dyn KeyStore>,
        Arc::new(MemoryCache::new()) as Arc<dyn ResponseCache>,
        Arc::new(MemoryAudit::new()) as Arc<dyn AuditSink>,
        Arc::new(CostGovernor::new(
            Arc::new(MemoryBudget::new()) as Arc<dyn BudgetStore>
        )),
        Arc::new(AlwaysConsent) as Arc<dyn ConsentGate>,
        clock,
        CloudPolicy::enabled(),
    )
}

#[test]
fn building_a_contact_sheet_stays_inside_its_budget() {
    let clock = clock();
    let timer = StageTimer::start("cloud_contact_sheet_build", clock.as_ref());
    let built = sheet(12);
    let measurement = timer.finish(12);

    assert!(!built.bytes.is_empty());
    assert_timing(&measurement);

    // And the thing that leaves the machine stays inside the size ceiling, which
    // is a privacy budget kept in the same file so it cannot be raised quietly.
    if let Err(breach) = budgets().check_size("cloud_payload", built.bytes.len() as u64) {
        panic!("{breach}");
    }
}

#[test]
fn the_gateway_overhead_stays_inside_its_budget() {
    // Section 11: "gateway overhead excluding provider latency <= 15 ms per
    // call". Measured on the offline transport, so the only work in the number
    // is policy, payload inspection, the cache lookup, the cost estimate, the
    // local fallback and the audit row.
    let gateway = offline_gateway();
    let task = SegmentNaming::for_sheet(sheet(12));
    let project = ProjectId::new();
    let cancel = CancelToken::new();
    let clock = clock();

    // One warm call first: the first one pays for a lazily built schema parse,
    // and a budget measured on a cold path is measuring the wrong thing.
    drop(gateway.run(
        &task,
        &segment("warm"),
        &CallContext {
            project: &project,
            decision_ref: None,
            cancel: &cancel,
        },
    ));

    const CALLS: u64 = 50;
    let timer = StageTimer::start("cloud_gateway_overhead", clock.as_ref());
    for index in 0..CALLS {
        let result = gateway.run(
            &task,
            &segment(&format!("seg-{index}")),
            &CallContext {
                project: &project,
                decision_ref: None,
                cancel: &cancel,
            },
        );
        assert!(result.is_ok(), "every call completes offline");
    }
    let measurement = timer.finish(CALLS);

    println!(
        "gateway overhead: {} ms for {CALLS} calls ({:.2} ms each)",
        measurement.elapsed_ms,
        measurement.elapsed_ms as f64 / CALLS as f64
    );
    assert_timing(&Measurement {
        stage: "cloud_gateway_overhead".to_string(),
        elapsed_ms: measurement.elapsed_ms / CALLS,
        units: 1,
    });
}

#[test]
fn a_wedding_makes_no_more_calls_than_the_policy_allows() {
    // Section 6.1's rule and section 11's budget are the same arithmetic, so
    // asserting it here catches a later phase that starts calling per image.
    let calls = u64::from(WEDDING_IMAGES.div_ceil(IMAGES_PER_CALL));
    println!("{WEDDING_IMAGES} images at one call per {IMAGES_PER_CALL} is {calls} calls");
    if let Err(breach) = budgets().check_count("cloud_calls_per_3000_images", calls) {
        panic!("{breach}");
    }
}

#[test]
fn a_wedding_costs_no_more_than_the_budget_at_default_settings() {
    // The estimate the cost governor itself would make, multiplied by the call
    // count. If a price table changes and this fails, the answer is a cheaper
    // default tier or a smaller sheet, not a bigger budget.
    let config = ProviderConfig {
        kind: ProviderKind::Anthropic,
        endpoint: anthropic::DEFAULT_ENDPOINT.to_string(),
        aliases: anthropic::default_aliases(),
        pinned_host: None,
    };
    let alias = config.alias(Tier::Balanced).expect("a balanced model");
    let prompt = PromptSpec::new(
        aura_cloud::tasks::SEGMENT_NAMING_SYSTEM,
        "segment: seg-1\nlocal classifier top guesses: dancing 0.48\n",
    )
    .with_images(vec![sheet(12)])
    .with_max_tokens(512);

    let tokens_in = estimate_tokens_in(&prompt, alias);
    let per_call = alias.price(tokens_in, prompt.max_tokens);
    let calls = f64::from(WEDDING_IMAGES.div_ceil(IMAGES_PER_CALL));
    let total = per_call * calls;

    println!(
        "{tokens_in} prompt tokens, ${per_call:.4} per call, ${total:.2} for {calls:.0} calls"
    );
    if let Err(breach) = budgets().check_cost("cloud_per_3000_images", total) {
        panic!("{breach}");
    }
}

#[test]
fn a_total_outage_costs_almost_nothing_in_wall_clock() {
    // Section 11: "failure impact on total pipeline time <= 3 % when all cloud
    // calls fail".
    //
    // The denominator has to be the *pipeline*, not the gateway. Comparing a
    // failing gateway against a switched-off one measures a few milliseconds
    // against zero and produces a meaningless percentage. The honest baseline is
    // what a wedding actually costs before any cloud work happens at all, and
    // the cheapest defensible figure for that is phase 02's tier 1 preview
    // budget - 180 s for 4,000 files, so 135 s for 3,000 - which is a floor
    // rather than an estimate, because a real run also decodes proxies and runs
    // local models on top of it.
    let pipeline_floor_ms = budgets()
        .stage
        .get("tier1_embedded_4000")
        .map(|budget| budget.max_elapsed_ms * u64::from(WEDDING_IMAGES) / 4_000)
        .unwrap_or(135_000);

    let clock = clock();
    let task = SegmentNaming::for_sheet(sheet(12));
    let project = ProjectId::new();
    let cancel = CancelToken::new();
    let calls = u64::from(WEDDING_IMAGES.div_ceil(IMAGES_PER_CALL));

    let failing = offline_gateway();
    let outage = StageTimer::start("cloud_outage_run", clock.as_ref());
    for index in 0..calls {
        let result = failing.run(
            &task,
            &segment(&format!("out-{index}")),
            &CallContext {
                project: &project,
                decision_ref: None,
                cancel: &cancel,
            },
        );
        assert!(result.is_ok(), "an outage must not stop the pipeline");
    }
    let spent_on_failing_calls = outage.finish(calls).elapsed_ms;

    // Integer arithmetic so a 0 % result is really 0 % and not a rounded float.
    let overhead_percent = (spent_on_failing_calls * 100) / pipeline_floor_ms.max(1);
    println!(
        "{calls} failed calls cost {spent_on_failing_calls} ms against a \
         {pipeline_floor_ms} ms pipeline floor ({overhead_percent}%)"
    );
    if cfg!(debug_assertions) {
        println!("not asserted in a debug build; run with --release");
        return;
    }
    if let Err(breach) = budgets().check_count("cloud_outage_overhead_percent", overhead_percent) {
        panic!("{breach}");
    }
}
