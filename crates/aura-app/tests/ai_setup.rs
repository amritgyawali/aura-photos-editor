//! The provider catalogue, the first-run screen, and what survives a restart.
//!
//! Three things are checked here and they fail in three different ways.
//!
//! **Shape.** The four new DTOs must exist in `ui/src/ipc/types.ts` with the same
//! fields, for the reason `ipc_contract.rs` gives: a field on one side and not the
//! other is a runtime `undefined` in a web view, which is the one class of bug a
//! typed boundary is supposed to make impossible.
//!
//! **Persistence.** Phase 04 kept the provider choice in memory. A photographer
//! picked Google, pasted a key, closed the application and reopened it pointed at
//! Anthropic with no key - which reads as the key having been lost. The tests that
//! matter here open the catalog **twice**, because a single-process test cannot
//! tell a stored choice from a cached one.
//!
//! **Refusal.** Declining the screen must record no provider, and a public
//! vendor's address must not be repointable by anything the panel can send.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_app::contract::ipc::{
    AiModelDto, AiProviderDto, AiSetupStatusDto, SaveAiSetupInput, SetAiKeyInput,
};
use aura_app::state::AppState;
use aura_cloud::keys::{KeyStore, MemoryKeyStore};

fn types_ts() -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/ipc/types.ts");
    std::fs::read_to_string(path).expect("ui/src/ipc/types.ts")
}

fn assert_keys_declared<T: serde::Serialize>(value: &T, type_name: &str) {
    let json = serde_json::to_value(value).expect("serialise");
    let object = json.as_object().expect("a struct serialises to an object");
    let ts = types_ts();

    let start = ts
        .find(&format!("export type {type_name} = {{"))
        .unwrap_or_else(|| panic!("{type_name} is missing from types.ts"));
    let end = ts[start..]
        .find("};")
        .map(|offset| start + offset)
        .unwrap_or(ts.len());
    let block = &ts[start..end];

    for key in object.keys() {
        assert!(
            block.contains(&format!("{key}:")),
            "{type_name}.{key} is missing from ui/src/ipc/types.ts"
        );
    }
}

/// A catalog in a temporary directory, with an in-memory credential store.
///
/// The key store is in memory in every test in this file, for the reason phase 04
/// gave it one: no test may write to the developer's real keychain.
fn open(dir: &Path, keys: Arc<dyn KeyStore>) -> AppState {
    AppState::open(&dir.join("catalog.aura"))
        .expect("a catalog")
        .with_key_store(keys)
}

#[test]
fn the_four_new_shapes_exist_on_both_sides_of_the_boundary() {
    assert_keys_declared(
        &AiModelDto {
            tier: "cheap".to_string(),
            model: "claude-haiku-4-5".to_string(),
            input_per_mtok_usd: 1.0,
            output_per_mtok_usd: 5.0,
        },
        "AiModelDto",
    );
    assert_keys_declared(
        &AiProviderDto {
            id: "anthropic".to_string(),
            label: "Anthropic (Claude)".to_string(),
            blurb: "...".to_string(),
            wire: "anthropic".to_string(),
            endpoint: "https://api.anthropic.com".to_string(),
            endpoint_editable: false,
            requires_key: true,
            key_hint: "sk-ant-...".to_string(),
            keys_url: "https://console.anthropic.com/settings/keys".to_string(),
            images: true,
            models: Vec::new(),
        },
        "AiProviderDto",
    );
    assert_keys_declared(
        &AiSetupStatusDto {
            completed: false,
            skipped: false,
            provider: "anthropic".to_string(),
            endpoint: None,
            models: Vec::new(),
            keyed_providers: Vec::new(),
            schemes: Vec::new(),
            offline_studio_mode: false,
        },
        "AiSetupStatusDto",
    );
    assert_keys_declared(
        &SaveAiSetupInput {
            provider: "groq".to_string(),
            endpoint: None,
            cheap_model: None,
            balanced_model: None,
            reasoning_model: None,
            completed: true,
            skipped: false,
        },
        "SaveAiSetupInput",
    );
}

#[test]
fn the_catalogue_offers_more_than_ten_providers_and_prices_every_tier() {
    let dir = tempfile::tempdir().expect("a directory");
    let state = open(dir.path(), Arc::new(MemoryKeyStore::new()));

    let providers = aura_app::list_ai_providers(&state).expect("the catalogue");
    assert!(
        providers.len() >= 10,
        "only {} providers are offered",
        providers.len()
    );

    for provider in &providers {
        assert!(!provider.label.is_empty(), "{} has no label", provider.id);
        assert!(!provider.blurb.is_empty(), "{} has no blurb", provider.id);
        assert!(
            !provider.endpoint.is_empty(),
            "{} has no endpoint",
            provider.id
        );
        assert_eq!(
            provider.models.len(),
            3,
            "{} is missing a tier",
            provider.id
        );
        for model in &provider.models {
            assert!(!model.model.is_empty(), "{} has a blank model", provider.id);
            assert!(model.input_per_mtok_usd >= 0.0);
        }
    }
}

/// Every provider that needs a key names one, and every one that does not is a
/// local server. A row that asked for a key and could not say what one looks like
/// would be a row nobody can complete.
#[test]
fn every_provider_that_needs_a_key_says_what_a_key_looks_like() {
    let dir = tempfile::tempdir().expect("a directory");
    let state = open(dir.path(), Arc::new(MemoryKeyStore::new()));

    for provider in aura_app::list_ai_providers(&state).expect("the catalogue") {
        if provider.requires_key {
            assert!(
                !provider.key_hint.is_empty() && !provider.keys_url.is_empty(),
                "{} needs a key and says nowhere to get one",
                provider.id
            );
        } else {
            assert!(
                provider.endpoint_editable,
                "{} needs no key, so it must be a server the photographer can point at",
                provider.id
            );
        }
    }
}

#[test]
fn a_fresh_catalog_has_not_answered_the_question() {
    let dir = tempfile::tempdir().expect("a directory");
    let state = open(dir.path(), Arc::new(MemoryKeyStore::new()));

    let status = aura_app::ai_setup_status(&state).expect("a status");
    assert!(!status.completed);
    assert!(!status.skipped);
    assert!(status.keyed_providers.is_empty());
}

/// The test that phase 04 did not have. Two `AppState`s over one catalog: the
/// second is a restart, and a choice that only lived in the first one's memory
/// would be gone by the time it looks.
#[test]
fn a_choice_survives_a_restart() {
    let dir = tempfile::tempdir().expect("a directory");
    let keys: Arc<dyn KeyStore> = Arc::new(MemoryKeyStore::new());

    {
        let state = open(dir.path(), Arc::clone(&keys));
        aura_app::save_ai_setup(
            &state,
            &SaveAiSetupInput {
                provider: "groq".to_string(),
                endpoint: None,
                cheap_model: None,
                balanced_model: Some("llama-4-scout".to_string()),
                reasoning_model: None,
                completed: true,
                skipped: false,
            },
        )
        .expect("a saved choice");
    }

    let reopened = open(dir.path(), Arc::clone(&keys));
    let status = aura_app::ai_setup_status(&reopened).expect("a status");
    assert_eq!(status.provider, "groq");
    assert!(status.completed);
    assert!(!status.skipped);
    assert_eq!(
        status.models.get(1).map(String::as_str),
        Some("llama-4-scout")
    );

    // And the gateway is actually pointed there, rather than the panel merely
    // saying so: the model the balanced tier resolves to is the chosen one.
    let cloud = aura_app::cloud_status(&reopened).expect("a cloud status");
    assert_eq!(cloud.provider, "groq");
    assert!(
        cloud.tier_models.contains(&"llama-4-scout".to_string()),
        "{:?}",
        cloud.tier_models
    );
}

/// Declining is an answer, and it is not a decision about a provider.
#[test]
fn skipping_answers_the_question_without_choosing_anybody() {
    let dir = tempfile::tempdir().expect("a directory");
    let keys: Arc<dyn KeyStore> = Arc::new(MemoryKeyStore::new());

    {
        let state = open(dir.path(), Arc::clone(&keys));
        let status = aura_app::skip_ai_setup(&state).expect("a skip");
        assert!(status.completed);
        assert!(status.skipped);
    }

    let reopened = open(dir.path(), keys);
    let status = aura_app::ai_setup_status(&reopened).expect("a status");
    assert!(status.completed, "nobody should be asked twice");
    assert!(
        status.skipped,
        "and the product must remember it was declined"
    );
    assert!(status.keyed_providers.is_empty(), "no key was stored");
}

/// Storing a key is choosing a provider, and the choice outlives the process.
#[test]
fn a_stored_key_names_its_provider_and_the_name_survives_a_restart() {
    let dir = tempfile::tempdir().expect("a directory");
    let keys: Arc<dyn KeyStore> = Arc::new(MemoryKeyStore::new());

    {
        let state = open(dir.path(), Arc::clone(&keys));
        aura_app::set_ai_key(
            &state,
            &SetAiKeyInput {
                provider: "openai".to_string(),
                key: "sk-test-0123456789abcdef".to_string(),
                endpoint: None,
            },
        )
        .expect("a stored key");
    }

    let reopened = open(dir.path(), keys);
    let status = aura_app::ai_setup_status(&reopened).expect("a status");
    assert_eq!(status.provider, "openai");
    assert!(status.completed);
    assert!(status.keyed_providers.contains(&"openai".to_string()));
}

/// Several keys at once, because switching provider must not mean pasting again.
#[test]
fn three_providers_can_be_set_up_and_switched_between() {
    let dir = tempfile::tempdir().expect("a directory");
    let keys: Arc<dyn KeyStore> = Arc::new(MemoryKeyStore::new());
    let state = open(dir.path(), Arc::clone(&keys));

    for provider in ["anthropic", "openai", "groq"] {
        aura_app::set_ai_key(
            &state,
            &SetAiKeyInput {
                provider: provider.to_string(),
                key: format!("sk-{provider}-0123456789abcdef"),
                endpoint: None,
            },
        )
        .expect("a stored key");
    }

    let status = aura_app::ai_setup_status(&state).expect("a status");
    for provider in ["anthropic", "openai", "groq"] {
        assert!(
            status.keyed_providers.contains(&provider.to_string()),
            "{provider} is not reported as set up"
        );
    }

    // Switching back asks for nothing: the key is already in the store.
    aura_app::save_ai_setup(
        &state,
        &SaveAiSetupInput {
            provider: "anthropic".to_string(),
            endpoint: None,
            cheap_model: None,
            balanced_model: None,
            reasoning_model: None,
            completed: true,
            skipped: false,
        },
    )
    .expect("a switch");
    assert!(
        aura_app::cloud_status(&state)
            .expect("a status")
            .key_present
    );
}

/// A photographer's own server may be pointed anywhere; a public vendor may not.
#[test]
fn only_a_provider_whose_address_is_the_photographers_can_be_repointed() {
    let dir = tempfile::tempdir().expect("a directory");
    let state = open(dir.path(), Arc::new(MemoryKeyStore::new()));

    aura_app::save_ai_setup(
        &state,
        &SaveAiSetupInput {
            provider: "anthropic".to_string(),
            endpoint: Some("http://somewhere.example".to_string()),
            cheap_model: None,
            balanced_model: None,
            reasoning_model: None,
            completed: true,
            skipped: false,
        },
    )
    .expect("a saved choice");
    assert_eq!(
        aura_app::cloud_status(&state).expect("a status").endpoint,
        "https://api.anthropic.com"
    );

    aura_app::save_ai_setup(
        &state,
        &SaveAiSetupInput {
            provider: "ollama".to_string(),
            endpoint: Some("http://192.168.1.9:11434".to_string()),
            cheap_model: None,
            balanced_model: None,
            reasoning_model: None,
            completed: true,
            skipped: false,
        },
    )
    .expect("a saved choice");
    assert_eq!(
        aura_app::cloud_status(&state).expect("a status").endpoint,
        "http://192.168.1.9:11434"
    );
}

/// A vendor this build has never heard of resolves to the one row that can be
/// pointed anywhere, rather than to whichever provider happened to be first.
#[test]
fn an_unknown_provider_becomes_the_compatible_server() {
    let dir = tempfile::tempdir().expect("a directory");
    let state = open(dir.path(), Arc::new(MemoryKeyStore::new()));

    let status = aura_app::save_ai_setup(
        &state,
        &SaveAiSetupInput {
            provider: "a-vendor-invented-next-year".to_string(),
            endpoint: Some("http://127.0.0.1:9999".to_string()),
            cheap_model: Some("whatever-i-loaded".to_string()),
            balanced_model: None,
            reasoning_model: None,
            completed: true,
            skipped: false,
        },
    )
    .expect("a saved choice");
    assert_eq!(status.provider, "compat");

    let cloud = aura_app::cloud_status(&state).expect("a status");
    assert_eq!(cloud.endpoint, "http://127.0.0.1:9999");
    assert!(
        cloud.tier_models.contains(&"whatever-i-loaded".to_string()),
        "{:?}",
        cloud.tier_models
    );
}

/// What the setup screen needs to warn about a build that cannot reach a vendor.
#[test]
fn the_status_says_which_schemes_this_build_can_reach() {
    let dir = tempfile::tempdir().expect("a directory");
    let state = open(dir.path(), Arc::new(MemoryKeyStore::new()));

    let schemes = aura_app::ai_setup_status(&state).expect("a status").schemes;
    assert!(schemes.contains(&"http".to_string()), "{schemes:?}");
    // The default build carries `aura-cloud`'s `tls` feature, which is what makes
    // sixteen of the nineteen rows in the catalogue reachable at all. A build
    // without it reports only `http` and the setup screen warns on every HTTPS
    // provider - see ADR-0063 section 5.
    assert!(schemes.contains(&"https".to_string()), "{schemes:?}");
}
