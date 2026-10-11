//! Data-only credential-file import and capability-aware internal routes.
//!
//! Secrets go directly to the OS store. Serialized profiles contain no keys.
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use aura_core::errors::cloud::{payload_refused, provider_error};
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};

use crate::keys::{KeyStore, SecretKey};
use crate::provider::{
    CloudRequest, CloudResponse, HttpRequest, HttpResponse, Provider, ProviderClient,
    ProviderConfig, ProviderKind, RetryPolicy, ThreadSleeper, Transport,
};

/// Safe metadata; never includes an API key or its fingerprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_profile: Option<String>,
    pub endpoint: String,
    pub model: String,
    pub vision: bool,
    pub input_usd_per_mtok: Option<f64>,
    pub output_usd_per_mtok: Option<f64>,
    pub status: String,
}

impl Profile {
    /// Separate accounts even for profiles with the same endpoint.
    #[must_use]
    pub fn account(&self) -> String {
        format!(
            "api-key:internal:{}:{}",
            self.key_profile.as_deref().unwrap_or(&self.id),
            blake3::hash(self.endpoint.as_bytes()).to_hex()
        )
    }

    #[must_use]
    pub fn available(&self) -> bool {
        self.status == "catalogue_ready"
            && self
                .input_usd_per_mtok
                .is_some_and(|v| v.is_finite() && (0.0..=1000.0).contains(&v))
            && self
                .output_usd_per_mtok
                .is_some_and(|v| v.is_finite() && (0.0..=1000.0).contains(&v))
    }

    /// Quality priors are routing preferences, not a benchmark claim.
    #[must_use]
    pub fn priority(&self) -> f64 {
        let name = self.model.to_ascii_lowercase();
        if name.contains("opus") || name.contains("gpt-6") {
            40.0
        } else if name.contains("gemini") {
            35.0
        } else if name.contains("medium") || name.contains("qwen") {
            30.0
        } else {
            25.0
        }
    }

    #[must_use]
    pub fn client(&self, transport: Arc<dyn Transport>) -> Arc<ProviderClient> {
        let mut aliases = crate::compat::aliases_for(&self.model);
        for alias in aliases.values_mut() {
            alias.input_per_mtok_usd = self.input_usd_per_mtok.unwrap_or(f64::MAX);
            alias.output_per_mtok_usd = self.output_usd_per_mtok.unwrap_or(f64::MAX);
        }
        let provider = FileProvider(crate::compat::provider_with_aliases(
            &self.endpoint,
            aliases,
        ));
        Arc::new(
            ProviderClient::new(
                Arc::new(provider),
                Arc::new(SafeTransport(transport)),
                Arc::new(ThreadSleeper),
            )
            .with_policy(RetryPolicy {
                attempts: 1,
                ..RetryPolicy::default()
            }),
        )
    }
}

/// Read assignment lines as data. Markdown, shell expressions and duplicates
/// are rejected. Error strings never include the input line or value.
///
/// # Errors
/// Refuses malformed or oversized configuration before storing any key.
pub fn parse(text: &str) -> AuraResult<Vec<(Profile, SecretKey)>> {
    if text.len() > 64 * 1024 {
        return Err(payload_refused("AI configuration is too large"));
    }
    let mut groups: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| payload_refused("AI configuration requires assignment lines"))?;
        let (id, field) = [
            "INPUT_USD_PER_MTOK",
            "OUTPUT_USD_PER_MTOK",
            "BASE_URL",
            "API_KEY",
            "MODEL",
        ]
        .into_iter()
        .find_map(|field| {
            name.trim()
                .strip_suffix(&format!("_{field}"))
                .map(|id| (id, field))
        })
        .ok_or_else(|| payload_refused("Unknown AI configuration field"))?;
        if id.is_empty()
            || id.len() > 48
            || !id
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(payload_refused("Invalid AI profile identifier"));
        }
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() || value.len() > 2048 || value.chars().any(char::is_control) {
            return Err(payload_refused("Invalid AI configuration value"));
        }
        if groups
            .entry(id.to_string())
            .or_default()
            .insert(field.to_string(), value.to_string())
            .is_some()
        {
            return Err(payload_refused("Duplicate AI configuration field"));
        }
    }
    if groups.is_empty() || groups.len() > 32 {
        return Err(payload_refused("AI configuration needs one to 32 profiles"));
    }
    groups
        .into_iter()
        .map(|(id, mut fields)| {
            let mut take = |field| {
                fields
                    .remove(field)
                    .ok_or_else(|| payload_refused("Incomplete AI profile"))
            };
            let endpoint = take("BASE_URL")?.trim_end_matches('/').to_string();
            let model = take("MODEL")?;
            let secret = SecretKey::new(take("API_KEY")?);
            validate_endpoint(&endpoint)?;
            if model.len() > 200
                || credential_model_name(&model)
                || (secret.len() > 7 && model.contains(secret.expose()))
                || !model
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
            {
                return Err(payload_refused("Invalid AI model identifier"));
            }
            let rate = |field: &str| -> AuraResult<Option<f64>> {
                fields
                    .get(field)
                    .map(|value| {
                        value
                            .parse::<f64>()
                            .ok()
                            .filter(|v| v.is_finite() && *v >= 0.0 && *v <= 1000.0)
                            .ok_or_else(|| payload_refused("Invalid AI token price"))
                    })
                    .transpose()
            };
            Ok((
                Profile {
                    id,
                    key_profile: None,
                    endpoint,
                    model,
                    vision: false,
                    input_usd_per_mtok: rate("INPUT_USD_PER_MTOK")?,
                    output_usd_per_mtok: rate("OUTPUT_USD_PER_MTOK")?,
                    status: "not_checked".to_string(),
                },
                secret,
            ))
        })
        .collect()
}

fn validate_endpoint(endpoint: &str) -> AuraResult<()> {
    let tail = endpoint
        .strip_prefix("https://")
        .ok_or_else(|| payload_refused("Internal AI endpoints require HTTPS"))?;
    let host = tail.split('/').next().unwrap_or_default();
    if host.is_empty()
        || !host.contains('.')
        || tail.contains(['@', '?', '#', '\\'])
        || tail.chars().any(char::is_whitespace)
        || !tail
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_/:%".contains(&b))
        || tail.contains("..")
    {
        return Err(payload_refused("Invalid internal AI endpoint"));
    }
    Ok(())
}

// Long uppercase vendor model IDs are not credentials. The general log scrubber
// intentionally errs on redaction and is unsuitable as a model-name validator.
fn credential_model_name(model: &str) -> bool {
    model.split('/').any(|part| {
        [
            "sk-",
            "sk_",
            "AIza",
            "gsk_",
            "xai-",
            "ghp_",
            "github_pat_",
            "nvapi-",
        ]
        .iter()
        .any(|prefix| part.starts_with(prefix))
    })
}

fn canonical_model<'a>(endpoint: &str, model: &'a str) -> &'a str {
    if endpoint == "https://generativelanguage.googleapis.com/v1beta/openai" {
        model.strip_prefix("models/").unwrap_or(model)
    } else {
        model
    }
}

/// Import all credentials and inspect model catalogues. Catalogue access is not
/// proof of a working completion; the first governed task verifies that.
///
/// # Errors
/// Invalid configuration or OS credential-storage failure. Provider failures
/// are safe per-profile status values and never expose response bodies.
pub fn import(
    text: &str,
    keys: &dyn KeyStore,
    transport: &dyn Transport,
) -> AuraResult<Vec<Profile>> {
    let parsed = parse(text)?;
    // All input validation finishes before the first mutation.
    for (profile, secret) in &parsed {
        keys.store(&profile.account(), secret)?;
    }
    let mut result = Vec::new();
    for (mut profile, secret) in parsed {
        let request = HttpRequest {
            method: "GET".to_string(),
            url: format!("{}/models", profile.endpoint),
            headers: vec![(
                "authorization".to_string(),
                format!("Bearer {}", secret.expose()),
            )],
            body: Vec::new(),
        };
        match transport.send(&request, Duration::from_secs(10)) {
            Ok(response) if response.status == 200 => {
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&response.body) {
                    apply_catalogue(&mut profile, &value);
                    // A configured text model remains available for text tasks.
                    // A separately labelled vision route can use the same key,
                    // with its own model, capability and published prices.
                    if !profile.vision
                        || profile.endpoint
                            == "https://generativelanguage.googleapis.com/v1beta/openai"
                    {
                        if let Some(rows) = value.get("data").and_then(serde_json::Value::as_array)
                        {
                            let mut candidates: Vec<_> = rows
                                .iter()
                                .filter_map(|row| {
                                    let name = canonical_model(
                                        &profile.endpoint,
                                        row.get("id")?.as_str()?,
                                    );
                                    if name == canonical_model(&profile.endpoint, &profile.model)
                                        || name.len() > 200
                                        || credential_model_name(name)
                                        || (secret.len() > 7 && name.contains(secret.expose()))
                                        || !name.bytes().all(|b| {
                                            b.is_ascii_alphanumeric() || b"._:/-".contains(&b)
                                        })
                                    {
                                        return None;
                                    }
                                    let mut candidate = profile.clone();
                                    candidate.id = format!("{}:vision", profile.id);
                                    candidate.key_profile = Some(profile.id.clone());
                                    candidate.model = name.to_string();
                                    candidate.input_usd_per_mtok = None;
                                    candidate.output_usd_per_mtok = None;
                                    apply_model(&mut candidate, row);
                                    (candidate.vision
                                        && candidate.available()
                                        && candidate.input_usd_per_mtok? <= 5.0
                                        && candidate.output_usd_per_mtok? <= 25.0)
                                        .then_some(candidate)
                                })
                                .collect();
                            candidates.sort_by(|a, b| {
                                // A configured free model authorizes a free
                                // vision alternative, not an automatic paid upgrade.
                                b.priority()
                                    .total_cmp(&a.priority())
                                    .then(
                                        a.input_usd_per_mtok
                                            .unwrap_or(1000.0)
                                            .total_cmp(&b.input_usd_per_mtok.unwrap_or(1000.0)),
                                    )
                                    .then(a.model.cmp(&b.model))
                            });
                            if profile.model.ends_with(":free") || profile.model.ends_with("/free")
                            {
                                candidates.retain(|candidate| {
                                    candidate
                                        .input_usd_per_mtok
                                        .is_some_and(|v| v.abs() < f64::EPSILON)
                                        && candidate
                                            .output_usd_per_mtok
                                            .is_some_and(|v| v.abs() < f64::EPSILON)
                                });
                            }
                            if let Some(candidate) = candidates.into_iter().next() {
                                result.push(candidate);
                            }
                        }
                    }
                } else {
                    profile.status = "unreadable_catalogue".to_string();
                }
            }
            Ok(response) => {
                profile.status = format!("http_{}", response.status);
            }
            Err(_) => {
                profile.status = "unreachable".to_string();
            }
        }
        result.push(profile);
    }
    Ok(result)
}

fn apply_catalogue(profile: &mut Profile, catalogue: &serde_json::Value) {
    let model = catalogue
        .get("data")
        .and_then(serde_json::Value::as_array)
        .and_then(|rows| {
            rows.iter().find(|row| {
                row.get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| {
                        canonical_model(&profile.endpoint, id)
                            == canonical_model(&profile.endpoint, &profile.model)
                    })
            })
        });
    let Some(model) = model else {
        profile.status = "model_not_listed".to_string();
        return;
    };
    apply_model(profile, model);
}

fn apply_model(profile: &mut Profile, model: &serde_json::Value) {
    profile.vision = model.pointer("/capabilities/vision").and_then(serde_json::Value::as_bool) == Some(true)
        || model.pointer("/architecture/input_modalities").and_then(serde_json::Value::as_array)
            .is_some_and(|modes| modes.iter().any(|mode| mode.as_str() == Some("image")))
        // Google's OpenAI model catalogue omits capabilities; this endpoint and
        // model family are documented as multimodal in the Gemini API docs.
        || (profile.endpoint == "https://generativelanguage.googleapis.com/v1beta/openai" && matches!(canonical_model(&profile.endpoint, &profile.model), "gemini-3.8-flash" | "gemini-3.6-flash"))
        // NVIDIA's catalogue omits modalities for this documented vision API.
        || (profile.endpoint == "https://integrate.api.nvidia.com/v1" && profile.model == "meta/llama-3.2-90b-vision-instruct");
    for (field, target) in [
        ("prompt", &mut profile.input_usd_per_mtok),
        ("completion", &mut profile.output_usd_per_mtok),
    ] {
        if target.is_none() {
            *target = model
                .get("pricing")
                .and_then(|p| p.get(field))
                .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
                .filter(|v| v.is_finite() && *v >= 0.0 && *v <= 0.001)
                .map(|v| v * 1_000_000.0);
        }
    }
    if profile.endpoint == "https://generativelanguage.googleapis.com/v1beta/openai"
        && matches!(
            canonical_model(&profile.endpoint, &profile.model),
            "gemini-3.8-flash" | "gemini-3.6-flash"
        )
    {
        profile.input_usd_per_mtok.get_or_insert(1.5); // Regular price; introductory discount is not assumed.
        profile.output_usd_per_mtok.get_or_insert(7.5);
    }
    if profile.endpoint == "https://api.mistral.ai/v1" && profile.model == "mistral-medium-latest" {
        profile.input_usd_per_mtok.get_or_insert(1.5);
        profile.output_usd_per_mtok.get_or_insert(7.5);
    }
    if profile.endpoint == "https://api.groq.com/openai/v1" {
        let price = match profile.model.as_str() {
            "openai/gpt-oss-120b" => Some((0.15, 0.60)),
            "qwen/qwen3.8-27b" => Some((0.80, 4.0)),
            _ => None,
        };
        if let Some((input, output)) = price {
            profile.input_usd_per_mtok.get_or_insert(input);
            profile.output_usd_per_mtok.get_or_insert(output);
        }
    }
    // NVIDIA's hosted development endpoint and Kilo's explicit free route are
    // free inference services with quotas. This does not price other deployments.
    if profile.endpoint == "https://integrate.api.nvidia.com/v1"
        || (profile.endpoint == "https://api.kilo.ai/api/gateway"
            && profile.model == "kilo-auto/free")
    {
        profile.input_usd_per_mtok.get_or_insert(0.0);
        profile.output_usd_per_mtok.get_or_insert(0.0);
    }
    profile.status = "catalogue_ready".to_string();
}

/// The file already supplies a full API base path. The legacy compatible
/// provider appends `/v1`; remove that extra segment only for these routes.
#[derive(Debug)]
struct FileProvider(crate::openai::OpenAiProvider);
impl Provider for FileProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Compat
    }
    fn config(&self) -> &ProviderConfig {
        self.0.config()
    }
    fn build(&self, request: &CloudRequest, key: &SecretKey) -> AuraResult<HttpRequest> {
        let mut built = self.0.build(request, key)?;
        built.url = format!("{}/chat/completions", self.config().endpoint);
        let mut body: serde_json::Value = serde_json::from_slice(&built.body)
            .map_err(|_| payload_refused("Invalid internal AI request"))?;
        if let Some(fields) = body.as_object_mut() {
            // New reasoning models reject temperature and use a completion
            // budget that includes reasoning. Do not retry the same bad shape.
            let model = request.model.to_ascii_lowercase();
            if model.contains("gpt-5") || model.contains("gpt-6") || model.contains("gpt-oss") {
                fields.remove("temperature");
                if let Some(limit) = fields.remove("max_tokens") {
                    fields.insert("max_completion_tokens".to_string(), limit);
                }
                fields.insert(
                    "reasoning_effort".to_string(),
                    serde_json::Value::String("low".to_string()),
                );
            }
            if self.config().endpoint == "https://generativelanguage.googleapis.com/v1beta/openai" {
                fields.insert(
                    "reasoning_effort".to_string(),
                    serde_json::Value::String("low".to_string()),
                );
            }
        }
        built.body = serde_json::to_vec(&body)
            .map_err(|_| payload_refused("Invalid internal AI request"))?;
        Ok(built)
    }
    fn parse(&self, response: &HttpResponse) -> AuraResult<CloudResponse> {
        self.0.parse(response).map_err(|_| {
            provider_error(
                "internal_ai",
                response.status,
                "Invalid AI completion response",
            )
        })
    }
}

/// An error response might echo a supplied secret. No error body escapes this
/// boundary into the existing status mapping or circuit-breaker diagnostics.
#[derive(Debug)]
struct SafeTransport(Arc<dyn Transport>);
impl Transport for SafeTransport {
    fn send(&self, request: &HttpRequest, timeout: Duration) -> AuraResult<HttpResponse> {
        let mut response = self.0.send(request, timeout)?;
        if request
            .headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
            .any(|(_, value)| {
                value.strip_prefix("Bearer ").is_some_and(|secret| {
                    !secret.is_empty()
                        && response
                            .body
                            .windows(secret.len())
                            .any(|part| part == secret.as_bytes())
                })
            })
        {
            return Err(provider_error(
                "internal_ai",
                response.status,
                "Provider response echoed a credential; response discarded",
            ));
        }
        if !(200..300).contains(&response.status) {
            response.body.clear();
        }
        Ok(response)
    }
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn schemes(&self) -> Vec<&'static str> {
        self.0.schemes()
    }
}

#[cfg(test)]
#[path = "internal_ai/tests/unit.rs"]
mod tests;
