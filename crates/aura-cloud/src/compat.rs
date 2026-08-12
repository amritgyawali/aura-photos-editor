//! OpenAI-compatible endpoints: the deployment this build can actually reach.
//!
//! Ollama, LM Studio, llama.cpp's server, vLLM, LiteLLM and most enterprise
//! gateways all speak the Chat Completions shape. They sit on the studio network
//! or on `localhost`, which means plain HTTP - and plain HTTP is exactly what
//! [`crate::http::HttpTransport`] speaks. Until the TLS waiver in
//! `docs/adr/ADR-0009-cloud-ai-policy.md` is discharged, this is the provider a
//! user can point at a real model today.
//!
//! Two things are different about a local endpoint and both are handled here.
//!
//! **Prices are zero.** Electricity is not free, but it is not billed per token,
//! and a cost governor that charged imaginary dollars against a real cap would
//! stop a local model for no reason. A compatible endpoint may still declare
//! prices - a hosted gateway does bill - so the price is configuration rather
//! than a constant.
//!
//! **The model names are the user's.** There is no meaningful default alias
//! table for a server that might be running anything, so [`aliases_for`] takes
//! the three model names from the user and prices them at zero.

use std::collections::BTreeMap;

use crate::contract::cloud::Tier;
use crate::openai::{Dialect, OpenAiProvider};
use crate::provider::{ModelAlias, ProviderConfig, ProviderKind};

/// Where a local server usually listens. Ollama's default port.
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434";

/// Build an alias table from one model name used for all three tiers.
///
/// The common case: a photographer running one local model. Declaring it at all
/// three tiers means a task that asks for `reasoning` gets it rather than being
/// refused, which is the right trade when the alternative is no cloud reasoning
/// at all and the marginal cost is zero.
#[must_use]
pub fn aliases_for(model: &str) -> BTreeMap<Tier, ModelAlias> {
    let mut aliases = BTreeMap::new();
    for tier in [Tier::Cheap, Tier::Balanced, Tier::Reasoning] {
        aliases.insert(
            tier,
            ModelAlias {
                model: model.to_string(),
                input_per_mtok_usd: 0.0,
                output_per_mtok_usd: 0.0,
                // Still counted, even at zero cost: the *token* estimate also
                // drives the truncation warning and the context-window check.
                image_tokens_per_mpixel: 1_500,
                max_output_tokens: 4_096,
            },
        );
    }
    aliases
}

/// A provider pointed at a compatible server running one model.
#[must_use]
pub fn provider(endpoint: &str, model: &str) -> OpenAiProvider {
    OpenAiProvider::with_config(
        ProviderConfig {
            kind: ProviderKind::Compat,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            aliases: aliases_for(model),
            pinned_host: None,
        },
        Dialect::Compatible,
    )
}

/// A provider with an explicit per-tier table, for a hosted gateway that bills.
#[must_use]
pub fn provider_with_aliases(
    endpoint: &str,
    aliases: BTreeMap<Tier, ModelAlias>,
) -> OpenAiProvider {
    OpenAiProvider::with_config(
        ProviderConfig {
            kind: ProviderKind::Compat,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            aliases,
            pinned_host: None,
        },
        Dialect::Compatible,
    )
}
