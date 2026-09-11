//! Which providers a photographer may point AURA at, and what each one costs.
//!
//! Phase 04 shipped four providers written out by hand in four modules, and the
//! desktop shell offered those four in a dropdown. That was enough while the
//! answer to "which model" was an engineering decision. It stops being enough the
//! moment it becomes a photographer's decision: somebody who already pays for
//! Groq, or whose studio has a Mistral contract, or who runs Ollama on the
//! machine under the desk, should not have to be told that AURA supports three
//! vendors and a text field.
//!
//! So the vendor list is **data** rather than code. One table, one row per
//! provider, and everything the rest of the product needs to reach it:
//!
//! * the wire format, because there are only three in use and everyone else
//!   copied OpenAI's;
//! * the endpoint, and whether a photographer may change it - a self-hosted
//!   gateway may, `api.anthropic.com` may not be quietly repointed;
//! * whether a key is needed at all, because a local server does not have one;
//! * three model names with three prices, because [`crate::budget`] prices a call
//!   before it makes it and a provider with no price table cannot be governed;
//! * whether the provider can see a photograph, because every task in this
//!   product sends a thumbnail and a text-only model would fail every one of
//!   them with a message about an unsupported content part.
//!
//! ## About the prices
//!
//! They are the vendors' own published list prices at the time this table was
//! written, and they are **defaults rather than measurements**. Nothing in this
//! repository has ever billed a provider. Two things keep that honest: the price
//! is only ever used to *refuse* a call before it is made, and the audit row
//! records the tokens the provider said it actually billed, so the spend meter a
//! photographer reads is never an estimate. A studio whose contract prices differ
//! edits the model, and [`crate::budget::PRICE_TABLE_VERSION`] is what says which
//! table a stored row was priced under.
//!
//! ## About the model names
//!
//! Same shape, weaker claim. A vendor renames a model roughly every quarter, and
//! a name that has moved produces one clean 404 from the provider rather than a
//! wrong answer. Every one of these is overridable per tier from Settings, which
//! is also the escape hatch for the photographer who wants a model this table has
//! never heard of.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::contract::cloud::Tier;
use crate::openai::Dialect;
use crate::provider::{ModelAlias, Provider, ProviderConfig, ProviderKind};

/// Which vendor's request shape a provider expects.
///
/// Three real formats and three variations on the third. Anthropic and Google
/// each publish their own; everybody else implements OpenAI's Chat Completions,
/// and the variations are about two fields rather than about the protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wire {
    /// Anthropic's Messages API.
    Anthropic,
    /// Google's `generateContent`.
    Google,
    /// OpenAI's own endpoint: `max_completion_tokens`, JSON mode understood.
    OpenAi,
    /// Chat Completions elsewhere, at a host that understands JSON mode.
    OpenAiJson,
    /// Chat Completions elsewhere, at a host that rejects JSON mode outright.
    ///
    /// Several local servers answer a `response_format` they do not implement
    /// with a 400 rather than by ignoring it, which turns the first call a
    /// photographer ever makes into an error about a field they did not set.
    OpenAiPlain,
    /// Azure OpenAI: the deployment is in the path, the key is in `api-key`, and
    /// the API version is a query parameter.
    Azure,
}

impl Wire {
    /// Stable text for the settings panel and the audit row.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::Google => "google",
            Self::OpenAi => "openai",
            Self::OpenAiJson => "openai-compatible",
            Self::OpenAiPlain => "openai-compatible-plain",
            Self::Azure => "azure-openai",
        }
    }

    /// The OpenAI dialect this wire maps onto, for the wires that have one.
    #[must_use]
    pub const fn dialect(self) -> Option<Dialect> {
        match self {
            Self::OpenAi => Some(Dialect::Official),
            Self::OpenAiJson => Some(Dialect::CompatibleJson),
            Self::OpenAiPlain => Some(Dialect::Compatible),
            Self::Azure => Some(Dialect::Azure),
            Self::Anthropic | Self::Google => None,
        }
    }
}

/// One model on one provider, at the price its vendor publishes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TierModel {
    /// Which tier a task asking for this gets.
    pub tier: Tier,
    /// The vendor's own identifier, sent on the wire.
    pub model: &'static str,
    /// US dollars per million input tokens.
    pub input_per_mtok_usd: f64,
    /// US dollars per million output tokens.
    pub output_per_mtok_usd: f64,
    /// The largest completion this model will produce.
    pub max_output_tokens: u32,
}

/// Everything the product needs to know about one provider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProviderSpec {
    /// The identifier this provider's key is filed under.
    pub kind: ProviderKind,
    /// What a photographer reads in the picker.
    pub label: &'static str,
    /// One sentence on why somebody would choose this one.
    pub blurb: &'static str,
    /// Which request shape to build.
    pub wire: Wire,
    /// Where it lives, without a trailing slash.
    pub endpoint: &'static str,
    /// True when the address is the photographer's to set.
    ///
    /// A self-hosted server, an enterprise gateway and an Azure resource all have
    /// an address only the user knows. A public vendor's does not move, and a
    /// field that lets somebody retype `api.anthropic.com` is a field that lets
    /// them send a wedding to a typo.
    pub endpoint_editable: bool,
    /// True when a key must be stored before a call can be made.
    pub requires_key: bool,
    /// What the key looks like, so a paste into the wrong provider is visible.
    pub key_hint: &'static str,
    /// Where the vendor issues keys. Shown as text; the app opens no browser.
    pub keys_url: &'static str,
    /// True when the default models here can see a photograph.
    ///
    /// Every task in this product sends a thumbnail. A text-only provider is not
    /// refused - it is *reported*, because the tasks degrade to their local
    /// fallback rather than failing, and a photographer who wants text-only
    /// reasoning may still have a reason to choose one.
    pub images: bool,
    /// Tokens one megapixel of image costs here. Vendors differ by a factor of
    /// three, and an image-heavy task priced with the wrong constant is exactly
    /// how a budget gets blown.
    pub image_tokens_per_mpixel: u32,
    /// One model per tier: cheap, balanced, reasoning.
    pub tiers: [TierModel; 3],
}

impl ProviderSpec {
    /// The alias table the governor prices against.
    #[must_use]
    pub fn aliases(&self) -> BTreeMap<Tier, ModelAlias> {
        self.aliases_with(&ModelChoice::default())
    }

    /// The same table with the photographer's own model names substituted.
    ///
    /// A name they chose replaces the model and keeps the price, because the
    /// price is a property of their contract rather than of the string. A studio
    /// that is billed differently edits the price table; a photographer trying a
    /// newer model of the same family is not asked to.
    #[must_use]
    pub fn aliases_with(&self, chosen: &ModelChoice) -> BTreeMap<Tier, ModelAlias> {
        self.tiers
            .iter()
            .map(|tier| {
                (
                    tier.tier,
                    ModelAlias {
                        model: chosen.for_tier(tier.tier).unwrap_or(tier.model).to_string(),
                        input_per_mtok_usd: tier.input_per_mtok_usd,
                        output_per_mtok_usd: tier.output_per_mtok_usd,
                        image_tokens_per_mpixel: self.image_tokens_per_mpixel,
                        max_output_tokens: tier.max_output_tokens,
                    },
                )
            })
            .collect()
    }

    /// The model this provider uses for one tier, before any override.
    #[must_use]
    pub fn model_for(&self, tier: Tier) -> &'static str {
        self.tiers
            .iter()
            .find(|entry| entry.tier == tier)
            .map_or("", |entry| entry.model)
    }
}

/// The model names a photographer chose, per tier.
///
/// Empty is the ordinary state and means "use whatever the table says". A blank
/// string is treated as absent rather than as a model called nothing, because a
/// cleared text field and an untouched one are the same intention.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelChoice {
    /// The cheap tier's model, when the photographer named one.
    pub cheap: Option<String>,
    /// The balanced tier's model, when the photographer named one.
    pub balanced: Option<String>,
    /// The reasoning tier's model, when the photographer named one.
    pub reasoning: Option<String>,
}

impl ModelChoice {
    /// One model for all three tiers. What a local server usually wants.
    #[must_use]
    pub fn uniform(model: &str) -> Self {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return Self::default();
        }
        Self {
            cheap: Some(trimmed.to_string()),
            balanced: Some(trimmed.to_string()),
            reasoning: Some(trimmed.to_string()),
        }
    }

    /// True when nothing was chosen and the table's own names apply.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cheap.is_none() && self.balanced.is_none() && self.reasoning.is_none()
    }

    /// The chosen name for one tier, if there is one.
    #[must_use]
    pub fn for_tier(&self, tier: Tier) -> Option<&str> {
        let chosen = match tier {
            Tier::Cheap => self.cheap.as_deref(),
            Tier::Balanced => self.balanced.as_deref(),
            Tier::Reasoning => self.reasoning.as_deref(),
        };
        chosen.map(str::trim).filter(|text| !text.is_empty())
    }

    /// Replace one tier, treating blank text as "clear this".
    pub fn set(&mut self, tier: Tier, model: Option<&str>) {
        let value = model
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToString::to_string);
        match tier {
            Tier::Cheap => self.cheap = value,
            Tier::Balanced => self.balanced = value,
            Tier::Reasoning => self.reasoning = value,
        }
    }
}

/// Every provider AURA knows how to reach.
#[must_use]
pub fn all() -> &'static [ProviderSpec] {
    &CATALOG
}

/// The row for one provider. An identifier this build has never heard of resolves
/// to the compatible-server row, which is the one that can be pointed anywhere.
#[must_use]
pub fn spec(kind: ProviderKind) -> &'static ProviderSpec {
    CATALOG
        .iter()
        .find(|entry| entry.kind == kind)
        .unwrap_or(&COMPAT)
}

/// The endpoint, alias table and pin for one provider, ready for a client.
///
/// `endpoint` is the photographer's when they gave one and the provider allows
/// it; a public vendor's address is used as published whatever is passed, because
/// [`ProviderSpec::endpoint_editable`] is a statement about where a wedding may
/// be sent rather than a hint for the panel.
#[must_use]
pub fn config(kind: ProviderKind, endpoint: Option<&str>, models: &ModelChoice) -> ProviderConfig {
    let found = spec(kind);
    let chosen = endpoint
        .map(str::trim)
        .filter(|text| !text.is_empty() && found.endpoint_editable)
        .unwrap_or(found.endpoint);
    ProviderConfig {
        kind,
        endpoint: chosen.trim_end_matches('/').to_string(),
        aliases: found.aliases_with(models),
        pinned_host: None,
    }
}

/// A provider ready to build requests, for any row in the table.
///
/// This is the one place in the product that turns a photographer's choice into
/// something that can speak to a vendor. `aura-app` calls it and does no matching
/// of its own, so adding the twentieth provider is a row in [`CATALOG`] rather
/// than an edit in three crates.
#[must_use]
pub fn build(
    kind: ProviderKind,
    endpoint: Option<&str>,
    models: &ModelChoice,
) -> Arc<dyn Provider> {
    let found = spec(kind);
    let built = config(kind, endpoint, models);
    match found.wire {
        Wire::Anthropic => Arc::new(crate::anthropic::AnthropicProvider::with_config(built)),
        Wire::Google => Arc::new(crate::google::GoogleProvider::with_config(built)),
        wire => Arc::new(crate::openai::OpenAiProvider::with_config(
            built,
            wire.dialect().unwrap_or(Dialect::Compatible),
        )),
    }
}

/// One tier row, written out so the table below stays readable.
const fn tier(
    tier: Tier,
    model: &'static str,
    input: f64,
    output: f64,
    max_output_tokens: u32,
) -> TierModel {
    TierModel {
        tier,
        model,
        input_per_mtok_usd: input,
        output_per_mtok_usd: output,
        max_output_tokens,
    }
}

/// The compatible-server row, and the fallback for an unknown identifier.
const COMPAT: ProviderSpec = ProviderSpec {
    kind: ProviderKind::Compat,
    label: "My own server (OpenAI-compatible)",
    blurb:
        "Any endpoint that speaks OpenAI's chat format: vLLM, llama.cpp, LiteLLM, a studio gateway.",
    wire: Wire::OpenAiPlain,
    endpoint: "http://127.0.0.1:8000",
    endpoint_editable: true,
    requires_key: false,
    key_hint: "only if your server asks for one",
    keys_url: "",
    images: true,
    image_tokens_per_mpixel: 1_500,
    tiers: [
        tier(Tier::Cheap, "local-model", 0.0, 0.0, 4_096),
        tier(Tier::Balanced, "local-model", 0.0, 0.0, 4_096),
        tier(Tier::Reasoning, "local-model", 0.0, 0.0, 4_096),
    ],
};

/// Every provider, in the order the picker shows them.
///
/// Ordering is deliberate: the three vendors whose wire formats this product
/// implements natively, then Azure because a studio on it has no choice, then the
/// aggregators and the fast hosts, then the two local servers, then the escape
/// hatch. A photographer scanning this list should meet the thing they already
/// pay for before they meet the thing they have never heard of.
static CATALOG: [ProviderSpec; 19] = [
    ProviderSpec {
        kind: ProviderKind::Anthropic,
        label: "Anthropic (Claude)",
        blurb: "Strong at reading a scene and explaining why. The default this product was built against.",
        wire: Wire::Anthropic,
        endpoint: "https://api.anthropic.com",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "sk-ant-...",
        keys_url: "https://console.anthropic.com/settings/keys",
        images: true,
        image_tokens_per_mpixel: 1_400,
        tiers: [
            tier(Tier::Cheap, "claude-haiku-4-5", 1.00, 5.00, 4_096),
            tier(Tier::Balanced, "claude-sonnet-4-5", 3.00, 15.00, 8_192),
            tier(Tier::Reasoning, "claude-opus-4-5", 5.00, 25.00, 8_192),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::OpenAi,
        label: "OpenAI (GPT)",
        blurb: "The widest model range, and the only one with a native JSON mode this product uses.",
        wire: Wire::OpenAi,
        endpoint: "https://api.openai.com",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "sk-...",
        keys_url: "https://platform.openai.com/api-keys",
        images: true,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(Tier::Cheap, "gpt-5-nano", 0.05, 0.40, 4_096),
            tier(Tier::Balanced, "gpt-5-mini", 0.25, 2.00, 8_192),
            tier(Tier::Reasoning, "gpt-5", 1.25, 10.00, 8_192),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::Google,
        label: "Google (Gemini)",
        blurb: "The cheapest way to look at a lot of photographs, and the largest context window here.",
        wire: Wire::Google,
        endpoint: "https://generativelanguage.googleapis.com",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "AIza...",
        keys_url: "https://aistudio.google.com/apikey",
        images: true,
        image_tokens_per_mpixel: 500,
        tiers: [
            tier(Tier::Cheap, "gemini-2.5-flash-lite", 0.10, 0.40, 4_096),
            tier(Tier::Balanced, "gemini-2.5-flash", 0.30, 2.50, 8_192),
            tier(Tier::Reasoning, "gemini-2.5-pro", 1.25, 10.00, 8_192),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::AzureOpenAi,
        label: "Azure OpenAI",
        blurb: "OpenAI's models inside your own Azure tenancy, for a studio with a data-residency clause.",
        wire: Wire::Azure,
        endpoint: "https://YOUR-RESOURCE.openai.azure.com",
        endpoint_editable: true,
        requires_key: true,
        key_hint: "a 32-character resource key",
        keys_url: "https://portal.azure.com",
        images: true,
        image_tokens_per_mpixel: 1_500,
        // On Azure the model name is the *deployment* name you chose, so these
        // three are placeholders that almost every tenancy will override.
        tiers: [
            tier(Tier::Cheap, "gpt-4o-mini", 0.15, 0.60, 4_096),
            tier(Tier::Balanced, "gpt-4o", 2.50, 10.00, 8_192),
            tier(Tier::Reasoning, "gpt-4o", 2.50, 10.00, 8_192),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::OpenRouter,
        label: "OpenRouter",
        blurb: "One key, four hundred models, including every model above. The easiest way to try several.",
        wire: Wire::OpenAiJson,
        endpoint: "https://openrouter.ai/api",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "sk-or-v1-...",
        keys_url: "https://openrouter.ai/keys",
        images: true,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(Tier::Cheap, "google/gemini-2.5-flash-lite", 0.10, 0.40, 4_096),
            tier(Tier::Balanced, "anthropic/claude-sonnet-4.5", 3.00, 15.00, 8_192),
            tier(Tier::Reasoning, "openai/gpt-5", 1.25, 10.00, 8_192),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::Groq,
        label: "Groq",
        blurb: "The fastest answers here by a wide margin, which matters when a run makes seventy calls.",
        wire: Wire::OpenAiJson,
        endpoint: "https://api.groq.com/openai",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "gsk_...",
        keys_url: "https://console.groq.com/keys",
        images: true,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(
                Tier::Cheap,
                "meta-llama/llama-4-scout-17b-16e-instruct",
                0.11,
                0.34,
                4_096,
            ),
            tier(
                Tier::Balanced,
                "meta-llama/llama-4-maverick-17b-128e-instruct",
                0.20,
                0.60,
                8_192,
            ),
            tier(
                Tier::Reasoning,
                "meta-llama/llama-4-maverick-17b-128e-instruct",
                0.20,
                0.60,
                8_192,
            ),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::Mistral,
        label: "Mistral",
        blurb: "European hosting, and Pixtral reads a photograph well for what it costs.",
        wire: Wire::OpenAiJson,
        endpoint: "https://api.mistral.ai",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "a 32-character key",
        keys_url: "https://console.mistral.ai/api-keys",
        images: true,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(Tier::Cheap, "pixtral-12b-2409", 0.15, 0.15, 4_096),
            tier(Tier::Balanced, "mistral-small-latest", 0.20, 0.60, 8_192),
            tier(Tier::Reasoning, "mistral-large-latest", 2.00, 6.00, 8_192),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::DeepSeek,
        label: "DeepSeek",
        blurb: "The cheapest reasoning on this list. Text only, so image tasks fall back to AURA's own models.",
        wire: Wire::OpenAiJson,
        endpoint: "https://api.deepseek.com",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "sk-...",
        keys_url: "https://platform.deepseek.com/api_keys",
        images: false,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(Tier::Cheap, "deepseek-chat", 0.27, 1.10, 4_096),
            tier(Tier::Balanced, "deepseek-chat", 0.27, 1.10, 8_192),
            tier(Tier::Reasoning, "deepseek-reasoner", 0.55, 2.19, 8_192),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::XAi,
        label: "xAI (Grok)",
        blurb: "Grok reads images, and the fast tier is priced against Gemini rather than against GPT.",
        wire: Wire::OpenAiJson,
        endpoint: "https://api.x.ai",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "xai-...",
        keys_url: "https://console.x.ai",
        images: true,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(Tier::Cheap, "grok-4-fast-non-reasoning", 0.20, 0.50, 4_096),
            tier(Tier::Balanced, "grok-4-fast-reasoning", 0.20, 0.50, 8_192),
            tier(Tier::Reasoning, "grok-4", 3.00, 15.00, 8_192),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::Together,
        label: "Together AI",
        blurb: "Open-weight models at hosted speed, with the Llama 4 family for anything that needs to see.",
        wire: Wire::OpenAiJson,
        endpoint: "https://api.together.xyz",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "a 64-character key",
        keys_url: "https://api.together.ai/settings/api-keys",
        images: true,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(
                Tier::Cheap,
                "meta-llama/Llama-4-Scout-17B-16E-Instruct",
                0.18,
                0.59,
                4_096,
            ),
            tier(
                Tier::Balanced,
                "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8",
                0.27,
                0.85,
                8_192,
            ),
            tier(
                Tier::Reasoning,
                "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8",
                0.27,
                0.85,
                8_192,
            ),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::Fireworks,
        label: "Fireworks AI",
        blurb: "Open-weight hosting with a generous rate limit, which suits a four-thousand-frame run.",
        wire: Wire::OpenAiJson,
        endpoint: "https://api.fireworks.ai/inference",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "fw_...",
        keys_url: "https://fireworks.ai/account/api-keys",
        images: true,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(
                Tier::Cheap,
                "accounts/fireworks/models/llama4-scout-instruct-basic",
                0.15,
                0.60,
                4_096,
            ),
            tier(
                Tier::Balanced,
                "accounts/fireworks/models/llama4-maverick-instruct-basic",
                0.22,
                0.88,
                8_192,
            ),
            tier(
                Tier::Reasoning,
                "accounts/fireworks/models/llama4-maverick-instruct-basic",
                0.22,
                0.88,
                8_192,
            ),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::DeepInfra,
        label: "DeepInfra",
        blurb: "The cheapest hosted Llama 4 here, and it reads a photograph.",
        wire: Wire::OpenAiJson,
        endpoint: "https://api.deepinfra.com/v1/openai",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "a 32-character key",
        keys_url: "https://deepinfra.com/dash/api_keys",
        images: true,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(
                Tier::Cheap,
                "meta-llama/Llama-4-Scout-17B-16E-Instruct",
                0.08,
                0.30,
                4_096,
            ),
            tier(
                Tier::Balanced,
                "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8",
                0.15,
                0.60,
                8_192,
            ),
            tier(
                Tier::Reasoning,
                "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8",
                0.15,
                0.60,
                8_192,
            ),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::Cerebras,
        label: "Cerebras",
        blurb: "Very fast text reasoning. No vision, so image tasks fall back to AURA's own models.",
        wire: Wire::OpenAiJson,
        endpoint: "https://api.cerebras.ai",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "csk-...",
        keys_url: "https://cloud.cerebras.ai",
        images: false,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(Tier::Cheap, "llama3.1-8b", 0.10, 0.10, 4_096),
            tier(Tier::Balanced, "llama-3.3-70b", 0.85, 1.20, 8_192),
            tier(
                Tier::Reasoning,
                "qwen-3-235b-a22b-instruct-2507",
                0.60,
                1.20,
                8_192,
            ),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::Moonshot,
        label: "Moonshot (Kimi)",
        blurb: "Long context and a vision line, priced below the western vendors.",
        wire: Wire::OpenAiJson,
        endpoint: "https://api.moonshot.ai",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "sk-...",
        keys_url: "https://platform.moonshot.ai/console/api-keys",
        images: true,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(
                Tier::Cheap,
                "moonshot-v1-8k-vision-preview",
                0.20,
                2.00,
                4_096,
            ),
            tier(
                Tier::Balanced,
                "moonshot-v1-32k-vision-preview",
                0.35,
                2.00,
                8_192,
            ),
            tier(Tier::Reasoning, "kimi-k2-0905-preview", 0.60, 2.50, 8_192),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::Nvidia,
        label: "NVIDIA NIM",
        blurb: "NVIDIA's hosted catalogue. Free credits to begin with, which is why the prices here are zero.",
        wire: Wire::OpenAiJson,
        endpoint: "https://integrate.api.nvidia.com",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "nvapi-...",
        keys_url: "https://build.nvidia.com",
        images: true,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(
                Tier::Cheap,
                "meta/llama-4-scout-17b-16e-instruct",
                0.0,
                0.0,
                4_096,
            ),
            tier(
                Tier::Balanced,
                "meta/llama-4-maverick-17b-128e-instruct",
                0.0,
                0.0,
                8_192,
            ),
            tier(
                Tier::Reasoning,
                "meta/llama-4-maverick-17b-128e-instruct",
                0.0,
                0.0,
                8_192,
            ),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::Perplexity,
        label: "Perplexity",
        blurb: "Answers with citations from the web. Text only, and the one provider here that searches.",
        wire: Wire::OpenAiJson,
        endpoint: "https://api.perplexity.ai",
        endpoint_editable: false,
        requires_key: true,
        key_hint: "pplx-...",
        keys_url: "https://www.perplexity.ai/settings/api",
        images: false,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(Tier::Cheap, "sonar", 1.00, 1.00, 4_096),
            tier(Tier::Balanced, "sonar-pro", 3.00, 15.00, 8_192),
            tier(Tier::Reasoning, "sonar-reasoning-pro", 2.00, 8.00, 8_192),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::Ollama,
        label: "Ollama (on this computer)",
        blurb: "Nothing leaves the machine and nothing is billed. Needs a vision model pulled first.",
        wire: Wire::OpenAiPlain,
        endpoint: "http://127.0.0.1:11434",
        endpoint_editable: true,
        requires_key: false,
        key_hint: "no key needed",
        keys_url: "https://ollama.com/download",
        images: true,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(Tier::Cheap, "llama3.2-vision", 0.0, 0.0, 4_096),
            tier(Tier::Balanced, "llama3.2-vision", 0.0, 0.0, 4_096),
            tier(Tier::Reasoning, "qwen2.5vl", 0.0, 0.0, 4_096),
        ],
    },
    ProviderSpec {
        kind: ProviderKind::LmStudio,
        label: "LM Studio (on this computer)",
        blurb: "Same as Ollama, with a window. Load a model there first and AURA will use it.",
        wire: Wire::OpenAiPlain,
        endpoint: "http://127.0.0.1:1234",
        endpoint_editable: true,
        requires_key: false,
        key_hint: "no key needed",
        keys_url: "https://lmstudio.ai",
        images: true,
        image_tokens_per_mpixel: 1_500,
        tiers: [
            tier(Tier::Cheap, "local-model", 0.0, 0.0, 4_096),
            tier(Tier::Balanced, "local-model", 0.0, 0.0, 4_096),
            tier(Tier::Reasoning, "local-model", 0.0, 0.0, 4_096),
        ],
    },
    COMPAT,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_exactly_one_row() {
        for kind in ProviderKind::ALL {
            let matches = CATALOG.iter().filter(|row| row.kind == *kind).count();
            assert_eq!(matches, 1, "{} has {matches} rows", kind.as_str());
        }
        assert_eq!(CATALOG.len(), ProviderKind::ALL.len());
    }

    #[test]
    fn every_row_prices_all_three_tiers() {
        for row in &CATALOG {
            let aliases = row.aliases();
            assert_eq!(aliases.len(), 3, "{} is missing a tier", row.label);
            for tier in [Tier::Cheap, Tier::Balanced, Tier::Reasoning] {
                let alias = aliases.get(&tier).expect("tier present");
                assert!(!alias.model.is_empty(), "{} has a blank model", row.label);
                assert!(alias.input_per_mtok_usd >= 0.0);
                assert!(alias.output_per_mtok_usd >= 0.0);
            }
        }
    }

    #[test]
    fn a_public_vendors_address_cannot_be_repointed() {
        let moved = config(
            ProviderKind::Anthropic,
            Some("http://somewhere.example"),
            &ModelChoice::default(),
        );
        assert_eq!(moved.endpoint, "https://api.anthropic.com");

        let local = config(
            ProviderKind::Ollama,
            Some("http://192.168.1.9:11434/"),
            &ModelChoice::default(),
        );
        assert_eq!(local.endpoint, "http://192.168.1.9:11434");
    }

    #[test]
    fn a_chosen_model_keeps_the_rows_price() {
        let row = spec(ProviderKind::OpenAi);
        let aliases = row.aliases_with(&ModelChoice::uniform("gpt-6-imaginary"));
        let cheap = aliases.get(&Tier::Cheap).expect("cheap tier");
        assert_eq!(cheap.model, "gpt-6-imaginary");
        assert!((cheap.input_per_mtok_usd - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn a_blank_model_is_not_a_model_called_nothing() {
        let choice = ModelChoice::uniform("   ");
        assert!(choice.is_empty());
        let aliases = spec(ProviderKind::Groq).aliases_with(&choice);
        let cheap = aliases.get(&Tier::Cheap).expect("cheap tier");
        assert_eq!(cheap.model, "meta-llama/llama-4-scout-17b-16e-instruct");
    }

    #[test]
    fn an_unknown_identifier_lands_on_the_compatible_row() {
        let kind = ProviderKind::parse("some-vendor-invented-next-year");
        assert_eq!(kind, ProviderKind::Compat);
        assert!(spec(kind).endpoint_editable);
        assert!(!spec(kind).requires_key);
    }

    #[test]
    fn every_row_builds_a_provider_of_its_own_kind() {
        for row in &CATALOG {
            let built = build(row.kind, None, &ModelChoice::default());
            assert_eq!(built.kind(), row.kind, "{} built the wrong kind", row.label);
        }
    }
}
