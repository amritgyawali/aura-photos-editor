//! Fixtures shared by the cloud test files.
//!
//! Everything here is synthetic and deterministic. No test in this crate touches
//! a network, a real keychain, or a photograph.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use aura_catalog::consent::{AlwaysConsent, NeverConsent};
use aura_cloud::anthropic::AnthropicProvider;
use aura_cloud::audit::MemoryAudit;
use aura_cloud::budget::{CostGovernor, MemoryBudget};
use aura_cloud::cache::MemoryCache;
use aura_cloud::cassette::CassetteTransport;
use aura_cloud::contract::cloud::ImagePart;
use aura_cloud::keys::MemoryKeyStore;
use aura_cloud::payload::{self, PayloadPolicy, SourceImage};
use aura_cloud::provider::{Provider, ProviderClient, RecordingSleeper, RetryPolicy, Transport};
use aura_cloud::redact::ExifSummary;
use aura_cloud::tasks::{SceneGuess, SegmentInput, SegmentNaming};
use aura_cloud::{CloudAiGateway, CloudPolicy};
use aura_core::clock::{Clock, FixedClock};
use aura_core::progress::CancelToken;
use aura_core::ProjectId;
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};
use time::OffsetDateTime;

/// Where the recorded provider responses live.
pub fn cassette_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/cloud/cassettes")
}

/// A frozen clock, so every latency in a test is a number the test chose.
pub fn clock() -> Arc<FixedClock> {
    FixedClock::at(OffsetDateTime::UNIX_EPOCH)
}

/// The project every test uses.
///
/// A fixed UUID rather than `ProjectId::new()`: a v7 id carries the wall clock,
/// and an id that changed between runs would put a different string in every
/// audit row a snapshot test looks at.
pub fn project() -> ProjectId {
    ProjectId::from_uuid(uuid::Uuid::from_bytes([
        0x01, 0x94, 0xa0, 0x0d, 0x00, 0x04, 0x70, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x01,
    ]))
}

/// A synthetic sRGB frame with a deterministic pattern.
///
/// The pattern varies with `seed` so two frames are genuinely different bytes,
/// which is what makes the cache-key tests meaningful.
pub fn frame(width: u32, height: u32, seed: u8) -> PixelBuffer {
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

/// A contact sheet built from `count` synthetic frames.
pub fn sheet(count: usize) -> ImagePart {
    let frames: Vec<PixelBuffer> = (0..count)
        .map(|index| frame(320, 240, u8::try_from(index).unwrap_or(0)))
        .collect();
    let sources: Vec<SourceImage<'_>> = frames.iter().map(SourceImage::new).collect();
    match payload::contact_sheet(&sources, PayloadPolicy::default()) {
        Ok(part) => part,
        Err(err) => panic!("the fixture contact sheet must build: {err}"),
    }
}

/// A segment input the cassettes match on.
pub fn segment(id: &str, best: &str, score_milli: u16, tiles: usize) -> SegmentInput {
    SegmentInput {
        segment_id: id.to_string(),
        start_offset_s: 7_200,
        end_offset_s: 9_000,
        frame_count: 148,
        local_guesses: vec![
            SceneGuess::new(best, score_milli),
            SceneGuess::new("candid", 210),
            SceneGuess::new("details", 90),
        ],
        exif: (0..tiles)
            .map(|index| ExifSummary {
                camera: Some("Canon EOS R5".to_string()),
                lens: Some("RF 35mm F1.8".to_string()),
                focal_mm: Some(35),
                aperture_x100: Some(180),
                shutter_denominator: Some(250),
                iso: Some(3200),
                flash: Some(false),
                offset_seconds: Some(u32::try_from(index).unwrap_or(0) * 30),
                orientation: Some(1),
                ..ExifSummary::default()
            })
            .collect(),
        tile_hashes: (0..tiles).map(|index| format!("tile-{index}")).collect(),
    }
}

/// Everything a gateway is made of, kept so a test can inspect the parts.
pub struct Harness {
    pub gateway: Arc<CloudAiGateway>,
    pub transport: Arc<CassetteTransport>,
    pub audit: Arc<MemoryAudit>,
    pub cache: Arc<MemoryCache>,
    pub budget: Arc<MemoryBudget>,
    pub keys: Arc<MemoryKeyStore>,
    pub clock: Arc<FixedClock>,
    pub cancel: CancelToken,
    pub project: ProjectId,
}

impl Harness {
    /// The default: cassettes loaded, a key present, cloud AI on, consent given.
    pub fn new() -> Self {
        Self::builder().build()
    }

    /// A builder for the cases that differ.
    pub fn builder() -> HarnessBuilder {
        HarnessBuilder {
            provider: Arc::new(AnthropicProvider::new("https://api.anthropic.com")),
            transport: None,
            key: Some("sk-ant-api03-TESTKEY000000000000000000000000000000000AA".to_string()),
            policy: CloudPolicy::enabled(),
            consent: true,
            cap_usd: 1.50,
        }
    }

    /// The one segment task every gateway test drives.
    pub fn task(&self, tiles: usize) -> SegmentNaming {
        SegmentNaming::for_sheet(sheet(tiles))
    }
}

/// Assembles a [`Harness`] with one thing changed at a time.
pub struct HarnessBuilder {
    provider: Arc<dyn Provider>,
    transport: Option<Arc<dyn Transport>>,
    key: Option<String>,
    policy: CloudPolicy,
    consent: bool,
    cap_usd: f64,
}

impl HarnessBuilder {
    /// Speak to a different vendor.
    pub fn provider(mut self, provider: Arc<dyn Provider>) -> Self {
        self.provider = provider;
        self
    }

    /// Use a different transport - the offline one, usually.
    pub fn transport(mut self, transport: Arc<dyn Transport>) -> Self {
        self.transport = Some(transport);
        self
    }

    /// Store no key at all.
    pub fn without_key(mut self) -> Self {
        self.key = None;
        self
    }

    /// Change the switches.
    pub fn policy(mut self, policy: CloudPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Withhold consent.
    pub fn without_consent(mut self) -> Self {
        self.consent = false;
        self
    }

    /// Set the project cap.
    pub fn cap_usd(mut self, cap_usd: f64) -> Self {
        self.cap_usd = cap_usd;
        self
    }

    /// Build it.
    pub fn build(self) -> Harness {
        let clock = clock();
        let project = project();

        let cassettes = match CassetteTransport::from_dir(&cassette_dir()) {
            Ok(loaded) => Arc::new(loaded),
            Err(err) => panic!("the cassettes must load: {err}"),
        };
        let transport: Arc<dyn Transport> = self
            .transport
            .unwrap_or_else(|| Arc::clone(&cassettes) as Arc<dyn Transport>);

        // Every provider reads the same test key, so a provider-swap test does
        // not have to know which account name each vendor files under.
        let keys = Arc::new(MemoryKeyStore::new());
        if let Some(secret) = &self.key {
            let secret = aura_cloud::keys::SecretKey::new(secret.clone());
            for account in [
                "api-key:anthropic",
                "api-key:openai",
                "api-key:google",
                "api-key:compat",
            ] {
                if let Err(err) = aura_cloud::keys::KeyStore::store(&*keys, account, &secret) {
                    panic!("the in-memory key store must accept a key: {err}");
                }
            }
        }

        let audit = Arc::new(MemoryAudit::new());
        let cache = Arc::new(MemoryCache::new());
        let budget = Arc::new(MemoryBudget::with_cap(&project.to_string(), self.cap_usd));

        let client = Arc::new(
            ProviderClient::new(
                self.provider,
                Arc::clone(&transport),
                Arc::new(RecordingSleeper::default()),
            )
            // The tests must not actually wait, and a `RecordingSleeper` makes
            // the schedule assertable instead.
            .with_policy(RetryPolicy {
                attempts: 3,
                base_ms: 1,
                max_backoff_ms: 4,
                timeout: std::time::Duration::from_millis(50),
            }),
        );

        let gateway = Arc::new(CloudAiGateway::new(
            client,
            Arc::clone(&keys) as Arc<dyn aura_cloud::keys::KeyStore>,
            Arc::clone(&cache) as Arc<dyn aura_cloud::cache::ResponseCache>,
            Arc::clone(&audit) as Arc<dyn aura_cloud::audit::AuditSink>,
            Arc::new(CostGovernor::new(
                Arc::clone(&budget) as Arc<dyn aura_cloud::budget::BudgetStore>
            )),
            if self.consent {
                Arc::new(AlwaysConsent) as Arc<dyn aura_core::contract::consent::ConsentGate>
            } else {
                Arc::new(NeverConsent) as Arc<dyn aura_core::contract::consent::ConsentGate>
            },
            Arc::clone(&clock) as Arc<dyn Clock>,
            self.policy,
        ));

        Harness {
            gateway,
            transport: cassettes,
            audit,
            cache,
            budget,
            keys,
            clock,
            cancel: CancelToken::new(),
            project,
        }
    }
}
