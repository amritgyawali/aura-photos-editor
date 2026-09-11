//! Live end-to-end: the real transport, the real gateway, a real vision model.
//!
//! Run with:
//!   AURA_LIVE_AI_KEY=sk-... RUSTUP_TOOLCHAIN=1.97.1-x86_64-pc-windows-gnu \
//!     cargo test -p aura-cloud --test live_provider -- --ignored --nocapture
//!
//! Sends a real JPEG through `HttpTransport` -> `OpenAiProvider` -> `CloudAiGateway`
//! exactly as `photo_auto_edit` does, and asserts a validated adjustment comes back.
//! Skipped everywhere else; this test exists so "does my provider work" is one command
//! rather than a guess. No cassette, no offline transport: the point is the network.

use std::sync::Arc;

use aura_catalog::consent::AlwaysConsent;
use aura_cloud::compat;
use aura_cloud::gateway::{CallContext, CloudAiGateway, CloudPolicy};
use aura_cloud::keys::{KeyStore, MemoryKeyStore, SecretKey};
use aura_cloud::openai::{Dialect, OpenAiProvider};
use aura_cloud::payload::{crop, PayloadPolicy, SourceImage};
use aura_cloud::photo_adjustment::{PhotoAutoEdit, PhotoReadings};
use aura_cloud::provider::{
    CloudRequest, ModelAlias, ProviderClient, ProviderConfig, ProviderKind, ThreadSleeper,
};
use aura_cloud::http::HttpTransport;
use aura_cloud::contract::cloud::{PromptSpec, Tier, Validate};
use aura_cloud::Provider;
use aura_cloud::audit::MemoryAudit;
use aura_cloud::budget::{CostGovernor, MemoryBudget};
use aura_cloud::cache::MemoryCache;
use aura_core::clock::SystemClock;
use aura_core::progress::CancelToken;
use aura_core::ProjectId;

/// A 64x48 gradient with a dark left half and a bright right half.
fn fixture_rgb() -> aura_raw::codec::Rgb8 {
    let (width, height) = (64u8, 48u8);
    let data = (0..u32::from(width) * u32::from(height))
        .flat_map(|i| {
            let x = (i % u32::from(width)) as u8;
            let y = (i / u32::from(width)) as u8;
            // Left side dark, right side bright, slight vertical variation: an
            // under-exposed frame is the one thing every vision model should agree on.
            let base = if x < width / 2 { 40 + y / 4 } else { 200 - y / 4 };
            [base, base.saturating_add(6), base.saturating_add(12)]
        })
        .collect();
    aura_raw::codec::Rgb8 {
        width: u32::from(width),
        height: u32::from(height),
        data,
    }
}

fn env_key() -> Option<String> {
    std::env::var("AURA_LIVE_AI_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn endpoint() -> String {
    std::env::var("AURA_LIVE_AI_ENDPOINT").unwrap_or_else(|_| "https://api.b.ai/v1".to_string())
}

fn model() -> String {
    std::env::var("AURA_LIVE_AI_MODEL").unwrap_or_else(|_| "glm-5.3-flash".to_string())
}

fn aliases_for(model: &str) -> std::collections::BTreeMap<Tier, ModelAlias> {
    compat::aliases_for(model)
}

#[test]
#[ignore = "live network; needs AURA_LIVE_AI_KEY"]
fn the_configured_provider_answers_a_real_auto_edit_task() {
    let Some(key) = env_key() else {
        panic!("set AURA_LIVE_AI_KEY to run the live check");
    };

    // 1. The transport reaches https, as the setup screen reports.
    let transport = HttpTransport::new();
    let schemes = transport.schemes();
    assert!(
        schemes.contains(&"https"),
        "this build must speak https for the live check; got {schemes:?}"
    );

    // 2. The provider is the one the UI builds for "my own server" (compat wire).
    let provider = OpenAiProvider::with_config(
        ProviderConfig {
            kind: ProviderKind::Compat,
            endpoint: endpoint(),
            aliases: aliases_for(&model()),
            pinned_host: None,
        },
        Dialect::Compatible,
    );
    assert_eq!(
        provider
            .config()
            .alias(Tier::Balanced)
            .map(|alias| alias.model.clone()),
        Some(model()),
        "the alias table must carry the chosen model on every tier"
    );

    // 3. A probe: one tiny text call that proves the key and the wire shape.
    let secret = SecretKey::new(key.clone());
    let client = ProviderClient::new(
        std::sync::Arc::new(provider.clone()),
        std::sync::Arc::new(HttpTransport::new()),
        std::sync::Arc::new(ThreadSleeper),
    );
    let probe = client
        .call(
            &CloudRequest {
                model: model(),
                prompt: PromptSpec::new(
                    "Reply with the single word OK.",
                    "Reply with the single word OK.",
                )
                .with_max_tokens(3000),
                repair: None,
            },
            &secret,
        )
        .expect("probe call must reach the provider");
    assert!(
        !probe.text.trim().is_empty(),
        "the probe must answer something; got stop_reason {:?}",
        probe.stop_reason
    );
    println!(
        "probe ok: model={} tokens={}->{} text={:?}",
        probe.model,
        probe.tokens_in,
        probe.tokens_out,
        probe.text.chars().take(80).collect::<String>()
    );

    // 4. The real task through the real gateway: vision in, validated JSON out.
    let rgb = fixture_rgb();
    let buffer = aura_raw::PixelBuffer {
        width: rgb.width,
        height: rgb.height,
        data: aura_raw::PixelData::Srgb8(rgb.data.clone()),
        colour_space: aura_raw::ColourSpace::Srgb,
        source: aura_raw::PixelSource::Demosaiced,
        decode_ms: 0,
    };
    let image = SourceImage::new(&buffer);
    let policy = PayloadPolicy::default();
    let part = crop(&image, policy).expect("payload builds");
    assert_eq!(part.media_type, "image/jpeg");
    let readings = PhotoReadings::measure(&rgb.data, part.content_hash.clone()).expect("readings");
    let task = PhotoAutoEdit { image: part };

    let keys = MemoryKeyStore::default();
    keys.store(&ProviderKind::Compat.account(), &secret)
        .expect("store key");
    let key_store: std::sync::Arc<dyn KeyStore> = std::sync::Arc::new(keys);

    let gateway = CloudAiGateway::new(
        std::sync::Arc::new(ProviderClient::new(
            std::sync::Arc::new(provider),
            std::sync::Arc::new(transport),
            std::sync::Arc::new(ThreadSleeper),
        )),
        key_store,
        std::sync::Arc::new(MemoryCache::default()),
        std::sync::Arc::new(MemoryAudit::default()),
        std::sync::Arc::new(CostGovernor::new(std::sync::Arc::new(
            MemoryBudget::default(),
        ))),
        std::sync::Arc::new(AlwaysConsent),
        std::sync::Arc::new(SystemClock::default()),
        CloudPolicy::enabled(),
    );

    let project = ProjectId::new();
    let cancel = CancelToken::new();
    let answer = gateway
        .run(
            &task,
            &readings,
            &CallContext {
                project: &project,
                decision_ref: None,
                cancel: &cancel,
            },
        )
        .expect("the gateway must return a validated answer");
    assert_eq!(
        answer.source.as_str(),
        "provider",
        "a live provider must answer, not the local fallback; reasons were {:?}",
        answer.value.reasons
    );
    answer
        .value
        .validate()
        .expect("the provider's answer must validate");
    println!(
        "auto-edit ok: exposure={:+} contrast={} highlights={} shadows={} vibrance={} confidence={:.2}",
        answer.value.exposure,
        answer.value.contrast,
        answer.value.highlights,
        answer.value.shadows,
        answer.value.vibrance,
        answer.confidence
    );
    println!("reasons: {:?}", answer.value.reasons);
}
