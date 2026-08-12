//! The Anthropic Messages wire format.
//!
//! Three details in here are worth reading rather than skimming.
//!
//! **Images come before text.** Anthropic's own guidance is that a prompt whose
//! images precede its instructions performs measurably better on multi-image
//! questions, and every task in this product is a multi-image question.
//!
//! **The key travels in `x-api-key`, never in the URL.** A key in a query string
//! ends up in proxy logs, in browser history and in crash reports. Every provider
//! module in this crate puts the key in a header for that reason, even the ones
//! whose vendor documents a query parameter.
//!
//! **The repair turn is two extra messages, not an edited first one.** The model
//! sees its own bad answer as an assistant turn and the validator's complaint as
//! the next user turn, which is the shape it was trained on. Rewriting the
//! original user turn instead would change the prompt hash and therefore the
//! cache key, and the repaired answer would be filed under a prompt that was
//! never sent.

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

/// The API version header Anthropic requires on every call.
pub const API_VERSION: &str = "2023-06-01";

/// The default public endpoint.
pub const DEFAULT_ENDPOINT: &str = "https://api.anthropic.com";

/// Anthropic.
#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    config: ProviderConfig,
}

impl AnthropicProvider {
    /// A provider pointed at an endpoint, with the default alias table.
    #[must_use]
    pub fn new(endpoint: &str) -> Self {
        Self {
            config: ProviderConfig {
                kind: ProviderKind::Anthropic,
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

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new(DEFAULT_ENDPOINT)
    }
}

/// The shipped alias table.
///
/// See [`crate::budget::PRICE_TABLE_VERSION`] for how these numbers are kept
/// honest. They are defaults, overridable per installation, and the audit row
/// records the table version every call was priced under.
#[must_use]
pub fn default_aliases() -> std::collections::BTreeMap<Tier, ModelAlias> {
    let mut aliases = std::collections::BTreeMap::new();
    aliases.insert(
        Tier::Reasoning,
        ModelAlias {
            model: "claude-opus-4-5".to_string(),
            input_per_mtok_usd: 5.00,
            output_per_mtok_usd: 25.00,
            image_tokens_per_mpixel: 1_400,
            max_output_tokens: 8_192,
        },
    );
    aliases.insert(
        Tier::Balanced,
        ModelAlias {
            model: "claude-sonnet-4-5".to_string(),
            input_per_mtok_usd: 3.00,
            output_per_mtok_usd: 15.00,
            image_tokens_per_mpixel: 1_400,
            max_output_tokens: 8_192,
        },
    );
    aliases.insert(
        Tier::Cheap,
        ModelAlias {
            model: "claude-haiku-4-5".to_string(),
            input_per_mtok_usd: 1.00,
            output_per_mtok_usd: 5.00,
            image_tokens_per_mpixel: 1_400,
            max_output_tokens: 4_096,
        },
    );
    aliases
}

impl Provider for AnthropicProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn build(&self, request: &CloudRequest, key: &SecretKey) -> AuraResult<HttpRequest> {
        let mut content: Vec<Block> = Vec::with_capacity(request.prompt.images.len() + 1);
        for image in &request.prompt.images {
            content.push(Block::Image {
                source: ImageSource {
                    kind: "base64",
                    media_type: image.media_type.clone(),
                    data: base64_encode(&image.bytes),
                },
            });
        }
        content.push(Block::Text {
            text: request.prompt.user.clone(),
        });

        let mut messages = vec![Message {
            role: "user",
            content,
        }];
        if let Some(repair) = &request.repair {
            messages.push(Message {
                role: "assistant",
                content: vec![Block::Text {
                    text: repair.previous.clone(),
                }],
            });
            messages.push(Message {
                role: "user",
                content: vec![Block::Text {
                    text: crate::repair::repair_instruction(&repair.complaint),
                }],
            });
        }

        let body = Body {
            model: request.model.clone(),
            max_tokens: request.prompt.max_tokens,
            temperature: request.prompt.temperature,
            system: request.prompt.system.clone(),
            messages,
        };
        let bytes = serde_json::to_vec(&body).map_err(|err| {
            payload_refused(format!("could not serialise the Anthropic request: {err}"))
        })?;

        Ok(HttpRequest {
            method: "POST".to_string(),
            url: format!("{}/v1/messages", self.config.endpoint),
            headers: vec![
                ("content-type".to_string(), "application/json".to_string()),
                ("accept".to_string(), "application/json".to_string()),
                ("anthropic-version".to_string(), API_VERSION.to_string()),
                ("x-api-key".to_string(), key.expose().to_string()),
            ],
            body: bytes,
        })
    }

    fn parse(&self, response: &HttpResponse) -> AuraResult<CloudResponse> {
        let parsed: Reply = serde_json::from_slice(&response.body).map_err(|err| {
            provider_error(
                ProviderKind::Anthropic.as_str(),
                response.status,
                format!(
                    "unreadable Anthropic response: {err}; body began {}",
                    crate::redact::scrub(&crate::provider::truncate(&response.text(), 200))
                ),
            )
        })?;

        let text: String = parsed
            .content
            .iter()
            .filter(|block| block.kind == "text")
            .map(|block| block.text.as_str())
            .collect();

        Ok(CloudResponse {
            text,
            tokens_in: parsed.usage.input_tokens,
            tokens_out: parsed.usage.output_tokens,
            model: parsed.model,
            stop_reason: parsed.stop_reason.unwrap_or_else(|| "unknown".to_string()),
        })
    }
}

#[derive(Debug, Serialize)]
struct Body {
    model: String,
    max_tokens: u32,
    temperature: f32,
    system: String,
    messages: Vec<Message>,
}

#[derive(Debug, Serialize)]
struct Message {
    role: &'static str,
    content: Vec<Block>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Block {
    Text { text: String },
    Image { source: ImageSource },
}

#[derive(Debug, Serialize)]
struct ImageSource {
    #[serde(rename = "type")]
    kind: &'static str,
    media_type: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct Reply {
    model: String,
    #[serde(default)]
    content: Vec<ReplyBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    usage: Usage,
}

#[derive(Debug, Deserialize)]
struct ReplyBlock {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Default, Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}
