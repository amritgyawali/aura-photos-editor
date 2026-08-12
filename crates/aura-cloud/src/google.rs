//! The Google Generative Language `generateContent` wire format.
//!
//! Google documents `?key=` as the way to authenticate. This module does not use
//! it. A key in a query string is a key in every proxy log, every access log and
//! every crash report between here and the datacentre, and `x-goog-api-key`
//! carries the same credential in a place that is not logged by default. The URL
//! this module builds contains the model name and nothing secret, which is also
//! what makes the cassette key - a digest over the URL and the body - safe to
//! commit to the repository.
//!
//! The other Google-specific detail is `responseMimeType: application/json`,
//! which is the only one of the three vendors' JSON modes that materially changes
//! the failure rate on this product's schemas. Without it, Gemini wraps JSON in a
//! markdown fence often enough to burn a repair round trip on perhaps one call in
//! twenty.

use std::collections::BTreeMap;

use aura_core::errors::cloud::{payload_refused, provider_error};
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};

use crate::contract::cloud::Tier;
use crate::keys::SecretKey;
use crate::payload::base64_encode;
use crate::provider::{
    CloudRequest, CloudResponse, HttpRequest, HttpResponse, ModelAlias, Provider, ProviderConfig,
    ProviderKind,
};

/// The default public endpoint.
pub const DEFAULT_ENDPOINT: &str = "https://generativelanguage.googleapis.com";

/// Google.
#[derive(Debug, Clone)]
pub struct GoogleProvider {
    config: ProviderConfig,
}

impl GoogleProvider {
    /// A provider pointed at an endpoint, with the default alias table.
    #[must_use]
    pub fn new(endpoint: &str) -> Self {
        Self {
            config: ProviderConfig {
                kind: ProviderKind::Google,
                endpoint: endpoint.trim_end_matches('/').to_string(),
                aliases: default_aliases(),
                pinned_host: None,
            },
        }
    }

    /// A provider with a caller-supplied configuration.
    #[must_use]
    pub const fn with_config(config: ProviderConfig) -> Self {
        Self { config }
    }
}

impl Default for GoogleProvider {
    fn default() -> Self {
        Self::new(DEFAULT_ENDPOINT)
    }
}

/// The shipped alias table.
#[must_use]
pub fn default_aliases() -> BTreeMap<Tier, ModelAlias> {
    let mut aliases = BTreeMap::new();
    aliases.insert(
        Tier::Reasoning,
        ModelAlias {
            model: "gemini-2.5-pro".to_string(),
            input_per_mtok_usd: 1.25,
            output_per_mtok_usd: 10.00,
            image_tokens_per_mpixel: 500,
            max_output_tokens: 8_192,
        },
    );
    aliases.insert(
        Tier::Balanced,
        ModelAlias {
            model: "gemini-2.5-flash".to_string(),
            input_per_mtok_usd: 0.30,
            output_per_mtok_usd: 2.50,
            image_tokens_per_mpixel: 500,
            max_output_tokens: 8_192,
        },
    );
    aliases.insert(
        Tier::Cheap,
        ModelAlias {
            model: "gemini-2.5-flash-lite".to_string(),
            input_per_mtok_usd: 0.10,
            output_per_mtok_usd: 0.40,
            image_tokens_per_mpixel: 500,
            max_output_tokens: 4_096,
        },
    );
    aliases
}

impl Provider for GoogleProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Google
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn build(&self, request: &CloudRequest, key: &SecretKey) -> AuraResult<HttpRequest> {
        let mut parts: Vec<Part> = Vec::with_capacity(request.prompt.images.len() + 1);
        for image in &request.prompt.images {
            parts.push(Part::Inline {
                inline_data: InlineData {
                    mime_type: image.media_type.clone(),
                    data: base64_encode(&image.bytes),
                },
            });
        }
        parts.push(Part::Text {
            text: request.prompt.user.clone(),
        });

        let mut contents = vec![Content {
            role: "user",
            parts,
        }];
        if let Some(repair) = &request.repair {
            contents.push(Content {
                role: "model",
                parts: vec![Part::Text {
                    text: repair.previous.clone(),
                }],
            });
            contents.push(Content {
                role: "user",
                parts: vec![Part::Text {
                    text: crate::repair::repair_instruction(&repair.complaint),
                }],
            });
        }

        let body = Body {
            system_instruction: Some(Content {
                role: "system",
                parts: vec![Part::Text {
                    text: request.prompt.system.clone(),
                }],
            }),
            contents,
            generation_config: GenerationConfig {
                temperature: request.prompt.temperature,
                max_output_tokens: request.prompt.max_tokens,
                response_mime_type: "application/json",
            },
        };
        let bytes = serde_json::to_vec(&body).map_err(|err| {
            payload_refused(format!("could not serialise the Google request: {err}"))
        })?;

        Ok(HttpRequest {
            method: "POST".to_string(),
            url: format!(
                "{}/v1beta/models/{}:generateContent",
                self.config.endpoint, request.model
            ),
            headers: vec![
                ("content-type".to_string(), "application/json".to_string()),
                ("accept".to_string(), "application/json".to_string()),
                ("x-goog-api-key".to_string(), key.expose().to_string()),
            ],
            body: bytes,
        })
    }

    fn parse(&self, response: &HttpResponse) -> AuraResult<CloudResponse> {
        let parsed: Reply = serde_json::from_slice(&response.body).map_err(|err| {
            provider_error(
                ProviderKind::Google.as_str(),
                response.status,
                format!(
                    "unreadable Google response: {err}; body began {}",
                    crate::redact::scrub(&crate::provider::truncate(&response.text(), 200))
                ),
            )
        })?;

        let candidate = parsed.candidates.first().ok_or_else(|| {
            provider_error(
                ProviderKind::Google.as_str(),
                response.status,
                "the response carried no candidates",
            )
        })?;

        let text: String = candidate
            .content
            .parts
            .iter()
            .filter_map(|part| part.text.as_deref())
            .collect();

        Ok(CloudResponse {
            text,
            tokens_in: parsed.usage_metadata.prompt_token_count,
            tokens_out: parsed.usage_metadata.candidates_token_count,
            model: parsed.model_version.unwrap_or_default(),
            stop_reason: candidate
                .finish_reason
                .clone()
                .unwrap_or_else(|| "unknown".to_string())
                .to_ascii_lowercase(),
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Body {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<Content>,
    contents: Vec<Content>,
    generation_config: GenerationConfig,
}

#[derive(Debug, Serialize)]
struct Content {
    role: &'static str,
    parts: Vec<Part>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Part {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(untagged)]
    Inline { inline_data: InlineData },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct InlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    temperature: f32,
    max_output_tokens: u32,
    response_mime_type: &'static str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reply {
    #[serde(default)]
    candidates: Vec<Candidate>,
    #[serde(default)]
    usage_metadata: UsageMetadata,
    #[serde(default)]
    model_version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    #[serde(default)]
    content: ReplyContent,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ReplyContent {
    #[serde(default)]
    parts: Vec<ReplyPart>,
}

#[derive(Debug, Deserialize)]
struct ReplyPart {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageMetadata {
    #[serde(default)]
    prompt_token_count: u32,
    #[serde(default)]
    candidates_token_count: u32,
}
