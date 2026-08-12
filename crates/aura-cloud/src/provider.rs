//! Providers, the transport port, model aliasing, retries and the breaker.
//!
//! Four vendors, one shape. A task never names a vendor or a model; it names a
//! [`Tier`], and the provider's alias table turns that into a model string and a
//! price. That indirection is what lets the same `SegmentNaming` task run against
//! Anthropic, OpenAI, Google or a local OpenAI-compatible server and produce the
//! same typed answer - which is acceptance criterion 7 of section 10.1, and the
//! reason the alias table is data rather than code.
//!
//! ## The transport port
//!
//! [`Transport`] is the only place in the entire product where bytes go to a
//! socket, and it is a port rather than a concrete client so that CI can run the
//! whole gateway with no network stack at all. Three implementations ship:
//! [`crate::cassette::CassetteTransport`] replays recorded responses,
//! [`OfflineTransport`] refuses everything, and [`crate::http::HttpTransport`]
//! is a real HTTP/1.1 client. See `docs/adr/ADR-0009-cloud-ai-policy.md` for why
//! that last one does not speak TLS in this build.
//!
//! ## Retries, and why they are bounded so tightly
//!
//! A photo pipeline must never be held hostage by a provider. Three attempts,
//! exponential backoff, and then the local fallback answers - because a 4,000
//! image run that retried generously would turn a five-minute provider blip into
//! an hour of a photographer watching a progress bar. The breaker exists for the
//! same reason: once a key is rejected, all 75 calls in the run would be rejected
//! identically, so the first rejection stops the rest from being attempted.
//!
//! Backoff jitter is derived from the request digest rather than from a random
//! number generator. Two machines retrying the same request at the same moment
//! still spread out, and determinism invariant 4 survives.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use aura_core::errors::cloud::{key_rejected, provider_error, rate_limited, unreachable};
use aura_core::AuraResult;

use crate::contract::cloud::{PromptSpec, Tier};
use crate::keys::SecretKey;

/// Which vendor's wire format a provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProviderKind {
    /// Anthropic Messages API.
    Anthropic,
    /// OpenAI Chat Completions API.
    OpenAi,
    /// Google Generative Language `generateContent`.
    Google,
    /// Anything speaking the OpenAI Chat Completions shape at another host:
    /// Ollama, LM Studio, llama.cpp, vLLM, LiteLLM, an enterprise gateway.
    Compat,
}

impl ProviderKind {
    /// Stable text for the settings panel, the audit row and telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::Google => "google",
            Self::Compat => "compat",
        }
    }

    /// Parse the identifier the UI sends. Unknown text is treated as `compat`,
    /// because a self-hosted endpoint is the case a user is most likely to name
    /// something we have never heard of.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text {
            "anthropic" => Self::Anthropic,
            "openai" => Self::OpenAi,
            "google" => Self::Google,
            _ => Self::Compat,
        }
    }

    /// The keychain account name this provider's key is filed under.
    #[must_use]
    pub fn account(self) -> String {
        format!("api-key:{}", self.as_str())
    }
}

/// One model, with everything the cost governor needs to price it.
///
/// Prices are per million tokens because that is how every vendor publishes
/// them, and keeping the published unit means a price change is a one-line edit
/// that a non-engineer can check.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelAlias {
    /// The vendor's own model identifier, sent on the wire.
    pub model: String,
    /// US dollars per million input tokens.
    pub input_per_mtok_usd: f64,
    /// US dollars per million output tokens.
    pub output_per_mtok_usd: f64,
    /// Tokens one megapixel of image costs on this model.
    ///
    /// Vendors differ by a factor of two here, and an image-heavy task priced
    /// with the wrong constant is exactly how a budget gets blown.
    pub image_tokens_per_mpixel: u32,
    /// The largest completion this model will produce.
    pub max_output_tokens: u32,
}

impl ModelAlias {
    /// What one call at these token counts costs, in US dollars.
    #[must_use]
    pub fn price(&self, tokens_in: u32, tokens_out: u32) -> f64 {
        (f64::from(tokens_in) / 1_000_000.0) * self.input_per_mtok_usd
            + (f64::from(tokens_out) / 1_000_000.0) * self.output_per_mtok_usd
    }
}

/// Roughly how many characters make one token in English prose.
///
/// Four is the usual published figure. It is an estimate used only to *refuse*
/// calls before they are made; the audit row records the tokens the provider
/// actually billed, so the spend meter is never an estimate.
pub const CHARS_PER_TOKEN: usize = 4;

/// A provider, its endpoint, and the three models it exposes.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Which wire format.
    pub kind: ProviderKind,
    /// Base URL, without a trailing slash.
    pub endpoint: String,
    /// One model per tier. A provider need not offer all three.
    pub aliases: BTreeMap<Tier, ModelAlias>,
    /// When set, the gateway refuses to send anywhere but this host.
    ///
    /// Region pinning: a studio bound by a data-residency clause sets this to
    /// their regional endpoint and any misconfiguration becomes a refusal rather
    /// than a breach.
    pub pinned_host: Option<String>,
}

impl ProviderConfig {
    /// The model for a tier, or the cheapest one below it that exists.
    #[must_use]
    pub fn alias(&self, tier: Tier) -> Option<&ModelAlias> {
        let mut wanted = Some(tier);
        while let Some(current) = wanted {
            if let Some(found) = self.aliases.get(&current) {
                return Some(found);
            }
            wanted = current.cheaper();
        }
        None
    }

    /// The host part of the endpoint, for the pin check and for the `Host` header.
    #[must_use]
    pub fn host(&self) -> String {
        self.endpoint
            .split("://")
            .nth(1)
            .unwrap_or(&self.endpoint)
            .split('/')
            .next()
            .unwrap_or_default()
            .to_string()
    }

    /// True when this endpoint is allowed by the region pin.
    #[must_use]
    pub fn host_is_permitted(&self) -> bool {
        match &self.pinned_host {
            None => true,
            Some(pinned) => &self.host() == pinned,
        }
    }
}

/// One question, in the gateway's own vocabulary rather than a vendor's.
#[derive(Debug, Clone)]
pub struct CloudRequest {
    /// The vendor model identifier chosen by the governor.
    pub model: String,
    /// System prompt, user turn and images.
    pub prompt: PromptSpec,
    /// Set on the repair attempt: the model's first answer and what was wrong
    /// with it, which the provider modules append as two extra turns.
    pub repair: Option<RepairTurn>,
}

/// The single repair round trip: what came back, and why it was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairTurn {
    /// The model's previous answer, verbatim.
    pub previous: String,
    /// The validator's complaint, verbatim. Written for a reader, not a log.
    pub complaint: String,
}

/// One answer, in the gateway's own vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudResponse {
    /// The completion text, which must be JSON matching the task's schema.
    pub text: String,
    /// Prompt tokens the provider says it billed.
    pub tokens_in: u32,
    /// Completion tokens the provider says it billed.
    pub tokens_out: u32,
    /// The model that actually answered, which may differ from the one asked for.
    pub model: String,
    /// `end_turn`, `max_tokens`, `stop_sequence` - vendor text, normalised.
    ///
    /// `max_tokens` here is the single most useful diagnostic in the crate: it
    /// means the JSON was truncated, which is a token ceiling problem and not a
    /// model problem.
    pub stop_reason: String,
}

/// An HTTP request, built by a provider and carried by a transport.
///
/// `Debug` prints the headers with their values elided, because one of them is
/// always the user's key.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    /// Always `POST` in this crate.
    pub method: String,
    /// Absolute URL.
    pub url: String,
    /// Header name and value pairs, in the order they will be written.
    pub headers: Vec<(String, String)>,
    /// The body, already serialised.
    pub body: Vec<u8>,
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &names)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

impl HttpRequest {
    /// A stable digest of the request, used as the cassette key and as the seed
    /// for the retry jitter.
    ///
    /// Header **values** are excluded on purpose: one of them is the key, and a
    /// cassette keyed on the key would be a cassette that leaked it.
    #[must_use]
    pub fn digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.method.as_bytes());
        hasher.update(self.url.as_bytes());
        for (name, _) in &self.headers {
            hasher.update(name.as_bytes());
        }
        hasher.update(&self.body);
        hasher.finalize().to_hex().to_string()
    }
}

/// What came back over the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// Status code.
    pub status: u16,
    /// Response headers, lower-cased names.
    pub headers: Vec<(String, String)>,
    /// Response body.
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Read one header, case-insensitively.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The body as text, lossily. Bodies are JSON or an error page; either way
    /// invalid UTF-8 is a symptom, not something to fail on.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    /// `Retry-After` in milliseconds, when the provider sent one.
    #[must_use]
    pub fn retry_after_ms(&self) -> Option<u64> {
        self.header("retry-after")
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(|seconds| seconds.saturating_mul(1000))
    }
}

/// The only door to a socket in the product.
pub trait Transport: Send + Sync + fmt::Debug {
    /// Send one request and wait for the whole response.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6003` for anything that stopped the bytes arriving: DNS, the
    /// connect, the deadline, a truncated body. A non-2xx status is **not** an
    /// error here - it is a response, and mapping it is the provider's job.
    fn send(&self, request: &HttpRequest, timeout: Duration) -> AuraResult<HttpResponse>;

    /// Stable name for the audit row: `cassette`, `offline`, `http`.
    fn name(&self) -> &'static str;
}

/// A transport that refuses everything. Offline studio mode, and the offline test.
#[derive(Debug, Default)]
pub struct OfflineTransport;

impl Transport for OfflineTransport {
    fn send(&self, _request: &HttpRequest, _timeout: Duration) -> AuraResult<HttpResponse> {
        Err(unreachable(
            "offline",
            "the offline transport refuses every request by design",
        ))
    }

    fn name(&self) -> &'static str {
        "offline"
    }
}

/// Turning the gateway's vocabulary into a vendor's, and back.
pub trait Provider: Send + Sync + fmt::Debug {
    /// Which wire format this speaks.
    fn kind(&self) -> ProviderKind;

    /// The endpoint, models and pin.
    fn config(&self) -> &ProviderConfig;

    /// Build the HTTP request, including the key header.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6009` when the request cannot be serialised, which in practice
    /// means an image part that is not valid.
    fn build(&self, request: &CloudRequest, key: &SecretKey) -> AuraResult<HttpRequest>;

    /// Read the vendor's answer, or map its error onto ours.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6002` on 401 and 403, `AURA-CLOUD-6004` on 429 and 5xx,
    /// `AURA-CLOUD-6010` for anything else that is not a well-formed answer.
    fn parse(&self, response: &HttpResponse) -> AuraResult<CloudResponse>;

    /// A cheap request that proves the key works, for the Check button.
    ///
    /// # Errors
    ///
    /// As [`Provider::build`].
    fn probe(&self, key: &SecretKey) -> AuraResult<HttpRequest> {
        let alias = self
            .config()
            .alias(Tier::Cheap)
            .map_or_else(|| "unknown".to_string(), |alias| alias.model.clone());
        self.build(
            &CloudRequest {
                model: alias,
                prompt: PromptSpec::new(
                    "Reply with the single word OK.",
                    "Reply with the single word OK.",
                )
                .with_max_tokens(8),
                repair: None,
            },
            key,
        )
    }
}

/// Map a status code every vendor shares onto our taxonomy.
///
/// Kept in one function rather than repeated in four provider modules, because
/// four copies of a status table is four chances for one of them to treat a 401
/// as retryable and hammer a provider with a bad key.
///
/// # Errors
///
/// Always, when called - it exists to produce the error.
#[must_use]
pub fn map_status(kind: ProviderKind, response: &HttpResponse) -> aura_core::AuraError {
    let name = kind.as_str();
    let body = crate::redact::scrub(&truncate(&response.text(), 200));
    match response.status {
        401 | 403 => key_rejected(name, format!("{} {}: {body}", response.status, name)),
        408 | 409 | 429 => rate_limited(name, response.retry_after_ms().unwrap_or(1_000)),
        500..=599 => rate_limited(name, response.retry_after_ms().unwrap_or(2_000)),
        status => provider_error(name, status, format!("{status} {name}: {body}")),
    }
}

/// First `limit` characters, with an ellipsis. Never a whole response body in a
/// log line.
#[must_use]
pub fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let head: String = text.chars().take(limit).collect();
    format!("{head}...")
}

/// How many times, how long between, and how long to wait for each attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total attempts including the first. Three.
    pub attempts: u32,
    /// The first backoff, doubled each time.
    pub base_ms: u64,
    /// The ceiling on one backoff.
    pub max_backoff_ms: u64,
    /// Per-attempt deadline handed to the transport.
    pub timeout: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            attempts: 3,
            base_ms: 250,
            max_backoff_ms: 4_000,
            timeout: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    /// How long to wait before attempt `attempt`, counting from one.
    ///
    /// Exponential, capped, with jitter of up to a quarter of the delay derived
    /// from `seed` - the request digest - rather than from a random generator.
    /// Two machines retrying the same request still spread out, and the same run
    /// replayed produces the same schedule.
    #[must_use]
    pub fn backoff_ms(&self, attempt: u32, seed: &str, retry_after_ms: Option<u64>) -> u64 {
        if let Some(requested) = retry_after_ms {
            return requested.min(self.max_backoff_ms);
        }
        let exponential = self
            .base_ms
            .saturating_mul(1u64 << attempt.min(16).saturating_sub(1))
            .min(self.max_backoff_ms);
        let jitter_span = exponential / 4;
        if jitter_span == 0 {
            return exponential;
        }
        let digest = blake3::hash(format!("{seed}:{attempt}").as_bytes());
        let bytes = digest.as_bytes();
        let sample = u64::from(bytes.first().copied().unwrap_or(0))
            | (u64::from(bytes.get(1).copied().unwrap_or(0)) << 8);
        exponential - jitter_span + (sample % (jitter_span * 2 + 1))
    }
}

/// Waiting between attempts, behind a port so tests do not.
pub trait Sleeper: Send + Sync + fmt::Debug {
    /// Block for this long.
    fn sleep(&self, duration: Duration);
}

/// The real one.
#[derive(Debug, Default)]
pub struct ThreadSleeper;

impl Sleeper for ThreadSleeper {
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// A sleeper that records instead of sleeping. Tests assert the schedule.
#[derive(Debug, Default)]
pub struct RecordingSleeper {
    slept_ms: parking_lot::Mutex<Vec<u64>>,
}

impl RecordingSleeper {
    /// Every wait that was asked for, in order.
    #[must_use]
    pub fn slept_ms(&self) -> Vec<u64> {
        self.slept_ms.lock().clone()
    }
}

impl Sleeper for RecordingSleeper {
    fn sleep(&self, duration: Duration) {
        self.slept_ms
            .lock()
            .push(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX));
    }
}

/// Stops a provider being asked again once it has clearly stopped working.
///
/// Two thresholds, because two failures mean different things. A rejected key
/// will be rejected every time, so one is enough. A network failure might be a
/// blip, so it takes three in a row.
#[derive(Debug)]
pub struct CircuitBreaker {
    open: AtomicBool,
    consecutive: AtomicU32,
    reason: parking_lot::Mutex<Option<String>>,
    threshold: u32,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            open: AtomicBool::new(false),
            consecutive: AtomicU32::new(0),
            reason: parking_lot::Mutex::new(None),
            threshold: 3,
        }
    }
}

impl CircuitBreaker {
    /// A breaker that opens after `threshold` consecutive soft failures.
    #[must_use]
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold: threshold.max(1),
            ..Self::default()
        }
    }

    /// True when no further attempts should be made this session.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    /// Why it opened, for the settings panel.
    #[must_use]
    pub fn reason(&self) -> Option<String> {
        self.reason.lock().clone()
    }

    /// A call succeeded. Clears the run of failures but not an open breaker -
    /// only [`CircuitBreaker::reset`] does that, and only the user's Check
    /// button calls it.
    pub fn record_success(&self) {
        self.consecutive.store(0, Ordering::Release);
    }

    /// A call failed. Opens immediately for a rejected key.
    pub fn record_failure(&self, code: &str, detail: &str) {
        if code == "AURA-CLOUD-6002" {
            self.trip(detail);
            return;
        }
        let count = self.consecutive.fetch_add(1, Ordering::AcqRel) + 1;
        if count >= self.threshold {
            self.trip(detail);
        }
    }

    fn trip(&self, detail: &str) {
        if !self.open.swap(true, Ordering::AcqRel) {
            *self.reason.lock() = Some(crate::redact::scrub(detail));
            tracing::warn!(
                target: "cloud.breaker",
                "the provider stopped answering; the rest of this run uses local models"
            );
        }
    }

    /// Close the breaker. Only a deliberate user action does this.
    pub fn reset(&self) {
        self.open.store(false, Ordering::Release);
        self.consecutive.store(0, Ordering::Release);
        *self.reason.lock() = None;
    }
}

/// A provider, a transport, and the policy that connects them.
#[derive(Debug)]
pub struct ProviderClient {
    provider: Arc<dyn Provider>,
    transport: Arc<dyn Transport>,
    sleeper: Arc<dyn Sleeper>,
    policy: RetryPolicy,
    breaker: Arc<CircuitBreaker>,
}

impl ProviderClient {
    /// Assemble a client from its parts.
    #[must_use]
    pub fn new(
        provider: Arc<dyn Provider>,
        transport: Arc<dyn Transport>,
        sleeper: Arc<dyn Sleeper>,
    ) -> Self {
        Self {
            provider,
            transport,
            sleeper,
            policy: RetryPolicy::default(),
            breaker: Arc::new(CircuitBreaker::default()),
        }
    }

    /// Override the retry policy.
    #[must_use]
    pub const fn with_policy(mut self, policy: RetryPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// The provider behind this client.
    #[must_use]
    pub fn provider(&self) -> &Arc<dyn Provider> {
        &self.provider
    }

    /// The transport behind this client, named in every audit row.
    #[must_use]
    pub fn transport_name(&self) -> &'static str {
        self.transport.name()
    }

    /// The breaker, so Settings can show why calls stopped and reset it.
    #[must_use]
    pub fn breaker(&self) -> &Arc<CircuitBreaker> {
        &self.breaker
    }

    /// Send one request with no retries and no breaker.
    ///
    /// Used by the Check button and by nothing else. A key validation must give
    /// the user an answer in one round trip rather than three, and it must not be
    /// blocked by a breaker that the same button exists to reset.
    ///
    /// # Errors
    ///
    /// Whatever the transport raised.
    pub fn transport_send(
        &self,
        request: &HttpRequest,
        timeout: Duration,
    ) -> AuraResult<HttpResponse> {
        self.transport.send(request, timeout)
    }

    /// Send one request, retrying what is worth retrying.
    ///
    /// # Errors
    ///
    /// The last error seen. `AURA-CLOUD-6003` when the breaker is already open,
    /// so a caller can tell "we did not try" from "we tried and failed".
    pub fn call(&self, request: &CloudRequest, key: &SecretKey) -> AuraResult<CloudResponse> {
        if self.breaker.is_open() {
            return Err(unreachable(
                self.provider.kind().as_str(),
                format!(
                    "not attempted: {}",
                    self.breaker
                        .reason()
                        .unwrap_or_else(|| "the provider stopped answering".to_string())
                ),
            ));
        }

        let config = self.provider.config();
        if !config.host_is_permitted() {
            return Err(aura_core::errors::cloud::disabled("region_pinned")
                .with_context("host", config.host()));
        }

        let http = self.provider.build(request, key)?;
        let seed = http.digest();
        let mut last = unreachable(
            self.provider.kind().as_str(),
            "no attempt was made".to_string(),
        );

        for attempt in 1..=self.policy.attempts {
            if attempt > 1 {
                let wait = self
                    .policy
                    .backoff_ms(attempt - 1, &seed, retry_after(&last));
                self.sleeper.sleep(Duration::from_millis(wait));
            }

            let outcome = match self.transport.send(&http, self.policy.timeout) {
                Ok(response) if (200..300).contains(&response.status) => {
                    match self.provider.parse(&response) {
                        Ok(parsed) => {
                            self.breaker.record_success();
                            return Ok(parsed);
                        }
                        Err(err) => err,
                    }
                }
                Ok(response) => map_status(self.provider.kind(), &response),
                Err(err) => err,
            };

            tracing::debug!(
                target: "cloud.attempt",
                provider = self.provider.kind().as_str(),
                transport = self.transport.name(),
                attempt,
                code = outcome.code.0,
                "cloud attempt failed"
            );

            let retryable = outcome.is_retryable();
            self.breaker.record_failure(outcome.code.0, &outcome.detail);
            last = outcome;
            if !retryable {
                break;
            }
        }

        Err(last)
    }
}

/// The pause a `Retry-After` asked for, when the last error carried one.
fn retry_after(error: &aura_core::AuraError) -> Option<u64> {
    error
        .context
        .iter()
        .find(|(key, _)| key == "retry_after_ms")
        .and_then(|(_, value)| value.parse().ok())
}

/// Estimate the prompt tokens of a rendered prompt.
///
/// Text by characters, images by area against the model's own constant. It is
/// deliberately a slight over-estimate: refusing a call that would have been
/// affordable costs the user a slightly worse decision, while allowing one that
/// is not costs them money.
#[must_use]
pub fn estimate_tokens_in(prompt: &PromptSpec, alias: &ModelAlias) -> u32 {
    let text_chars = prompt.system.chars().count() + prompt.user.chars().count();
    let text_tokens = text_chars.div_ceil(CHARS_PER_TOKEN);
    let image_tokens: u64 = prompt
        .images
        .iter()
        .map(|image| {
            let megapixels = f64::from(image.width) * f64::from(image.height) / 1_000_000.0;
            (megapixels * f64::from(alias.image_tokens_per_mpixel)).ceil() as u64
        })
        .sum();
    u32::try_from(text_tokens as u64 + image_tokens).unwrap_or(u32::MAX)
}
