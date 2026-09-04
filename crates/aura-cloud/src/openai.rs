//! The OpenAI Chat Completions wire format, and the shape everyone else copied.
//!
//! This module serves two providers. `openai` is the official endpoint;
//! [`crate::compat`] is the same wire format at somebody else's host - Ollama,
//! LM Studio, llama.cpp, vLLM, LiteLLM, an enterprise gateway. They differ in
//! exactly two ways, and both are captured by [`Dialect`]:
//!
//! * `response_format: {"type":"json_object"}` is understood by the official
//!   endpoint and rejected outright by several local servers, which return a 400
//!   rather than ignoring it. It is therefore opt-in.
//! * `max_completion_tokens` replaced `max_tokens` on newer OpenAI models, while
//!   most compatible servers only understand the old name.
//!
//! Getting either of those wrong turns into `AURA-CLOUD-6010` on the first call
//! against a local server, which is a bad first experience for the one deployment
//! this build can actually reach today.

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
pub const DEFAULT_ENDPOINT: &str = "https://api.openai.com";

/// Which flavour of the Chat Completions shape to speak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// The official endpoint: `max_completion_tokens`, JSON mode available.
    Official,
    /// A compatible server: `max_tokens`, no JSON mode, no organisation header.
    Compatible,
    /// A hosted vendor that copied the shape and implements JSON mode:
    /// OpenRouter, Groq, Mistral, DeepSeek, xAI, Together, Fireworks and the
    /// rest. `max_tokens`, because none of them took the newer field name.
    CompatibleJson,
    /// Azure OpenAI: the deployment name is in the path, the key travels in
    /// `api-key` rather than in `Authorization`, and the API version is a query
    /// parameter that is not optional.
    Azure,
}

impl Dialect {
    /// True when this dialect uses the newer `max_completion_tokens` field.
    const fn newer_token_field(self) -> bool {
        matches!(self, Self::Official)
    }

    /// True when `response_format: {"type":"json_object"}` is understood.
    ///
    /// Opt-in rather than opt-out: several local servers answer a field they do
    /// not implement with a 400 rather than by ignoring it, which turns the first
    /// call a photographer ever makes into an error about a field they did not
    /// set.
    const fn json_mode(self) -> bool {
        matches!(self, Self::Official | Self::CompatibleJson | Self::Azure)
    }
}

/// The Azure API version this build speaks.
///
/// Azure refuses a request without one, and the value is a date rather than a
/// number. It is a constant rather than a setting because a tenancy that needs a
/// different one needs a different request body as well.
pub const AZURE_API_VERSION: &str = "2024-10-21";

/// OpenAI, or anything that speaks its dialect.
#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    config: ProviderConfig,
    dialect: Dialect,
}

impl OpenAiProvider {
    /// The official endpoint with the default alias table.
    #[must_use]
    pub fn new(endpoint: &str) -> Self {
        Self {
            config: ProviderConfig {
                kind: ProviderKind::OpenAi,
                endpoint: endpoint.trim_end_matches('/').to_string(),
                aliases: default_aliases(),
                pinned_host: None,
            },
            dialect: Dialect::Official,
        }
    }

    /// A provider with a caller-supplied configuration and dialect.
    #[must_use]
    pub const fn with_config(config: ProviderConfig, dialect: Dialect) -> Self {
        Self { config, dialect }
    }

    /// Which dialect this instance speaks.
    #[must_use]
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }
}

impl Default for OpenAiProvider {
    fn default() -> Self {
        Self::new(DEFAULT_ENDPOINT)
    }
}

/// The shipped alias table for the official endpoint.
#[must_use]
pub fn default_aliases() -> BTreeMap<Tier, ModelAlias> {
    crate::catalog::spec(ProviderKind::OpenAi).aliases()
}

impl Provider for OpenAiProvider {
    fn kind(&self) -> ProviderKind {
        self.config.kind
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn build(&self, request: &CloudRequest, key: &SecretKey) -> AuraResult<HttpRequest> {
        let mut parts: Vec<Part> = Vec::with_capacity(request.prompt.images.len() + 1);
        parts.push(Part::Text {
            text: request.prompt.user.clone(),
        });
        for image in &request.prompt.images {
            parts.push(Part::ImageUrl {
                image_url: ImageUrl {
                    // A data URL rather than a link: nothing about a wedding is
                    // published to a URL a third party could fetch.
                    url: format!(
                        "data:{};base64,{}",
                        image.media_type,
                        base64_encode(&image.bytes)
                    ),
                    detail: "low",
                },
            });
        }

        let mut messages = vec![
            Message::system(&request.prompt.system),
            Message {
                role: "user",
                content: Content::Parts(parts),
            },
        ];
        if let Some(repair) = &request.repair {
            messages.push(Message {
                role: "assistant",
                content: Content::Text(repair.previous.clone()),
            });
            messages.push(Message {
                role: "user",
                content: Content::Text(crate::repair::repair_instruction(&repair.complaint)),
            });
        }

        let newer = self.dialect.newer_token_field();
        let body = Body {
            model: request.model.clone(),
            temperature: request.prompt.temperature,
            max_tokens: (!newer).then_some(request.prompt.max_tokens),
            max_completion_tokens: newer.then_some(request.prompt.max_tokens),
            response_format: self.dialect.json_mode().then_some(ResponseFormat {
                kind: "json_object",
            }),
            stream: false,
            messages,
        };
        let bytes = serde_json::to_vec(&body).map_err(|err| {
            payload_refused(format!("could not serialise the OpenAI request: {err}"))
        })?;

        let url = if self.dialect == Dialect::Azure {
            // The deployment name is the path segment, and it is the name the
            // customer chose in their own resource rather than a model name we
            // could guess - which is why the Azure row in the catalog ships
            // placeholders and the panel asks for the three deployments.
            format!(
                "{}/openai/deployments/{}/chat/completions?api-version={AZURE_API_VERSION}",
                self.config.endpoint, request.model
            )
        } else {
            format!("{}/v1/chat/completions", self.config.endpoint)
        };

        let mut headers = vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("accept".to_string(), "application/json".to_string()),
        ];
        // A local server usually wants no credential at all, and an empty bearer
        // is a header some of them reject rather than ignore. No key means no
        // header; the gateway has already decided whether one was required.
        if !key.is_empty() {
            if self.dialect == Dialect::Azure {
                headers.push(("api-key".to_string(), key.expose().to_string()));
            } else {
                headers.push((
                    "authorization".to_string(),
                    format!("Bearer {}", key.expose()),
                ));
            }
        }

        Ok(HttpRequest {
            method: "POST".to_string(),
            url,
            headers,
            body: bytes,
        })
    }

    fn parse(&self, response: &HttpResponse) -> AuraResult<CloudResponse> {
        let parsed: Reply = serde_json::from_slice(&response.body).map_err(|err| {
            provider_error(
                self.config.kind.as_str(),
                response.status,
                format!(
                    "unreadable response: {err}; body began {}",
                    crate::redact::scrub(&crate::provider::truncate(&response.text(), 200))
                ),
            )
        })?;

        let choice = parsed.choices.first().ok_or_else(|| {
            provider_error(
                self.config.kind.as_str(),
                response.status,
                "the response carried no choices",
            )
        })?;

        Ok(CloudResponse {
            text: choice.message.content.clone().unwrap_or_default(),
            tokens_in: parsed.usage.prompt_tokens,
            tokens_out: parsed.usage.completion_tokens,
            model: parsed.model,
            stop_reason: choice
                .finish_reason
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
        })
    }
}

#[derive(Debug, Serialize)]
struct Body {
    model: String,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    stream: bool,
    messages: Vec<Message>,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Debug, Serialize)]
struct Message {
    role: &'static str,
    content: Content,
}

impl Message {
    fn system(text: &str) -> Self {
        Self {
            role: "system",
            content: Content::Text(text.to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Parts(Vec<Part>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Part {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Serialize)]
struct ImageUrl {
    url: String,
    detail: &'static str,
}

#[derive(Debug, Deserialize)]
struct Reply {
    #[serde(default)]
    model: String,
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Usage,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ReplyMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReplyMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}
