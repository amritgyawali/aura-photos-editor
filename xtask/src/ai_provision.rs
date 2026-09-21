//! Provision this machine's AI provider from the developer settings file.
//!
//! `cargo xtask ai-provision` reads `.claude/settings.json` at the repository
//! root - the file that already carries this machine's provider and key - and
//! writes the same choice into AURA through the product's *own* commands: the
//! key through `set_ai_key` (which puts it in the OS credential store, not in
//! any file AURA owns), and the provider/endpoint/model through
//! `save_ai_setup` (the catalog row the first-run screen and the AI panel read).
//! There is no second storage shape here to drift from the first.
//!
//! It is idempotent, and it is an xtask rather than startup code on purpose: a
//! shipped build must still show the panel (ADR-0063), but this machine was
//! asked to be provisioned silently, and that is a developer action with a
//! visible command behind it.
//!
//! The key is not trusted prose. Before anything is stored the task probes the
//! endpoint the way the gateway will use it - one text call and one vision call
//! through the real `HttpTransport`, over TLS - because `PhotoAutoEdit` sends an
//! image and a text-only model would fail every call while still falling back
//! locally without a word. A key that cannot answer a vision call is refused at
//! provisioning time, where somebody is looking, rather than quietly for a month.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use aura_app::contract::ipc::{SaveAiSetupInput, SetAiKeyInput};
use aura_app::{ai_setup_status, check_ai_key, save_ai_setup, set_ai_key, AppState};
use aura_cloud::http::HttpTransport;
use aura_cloud::provider::{HttpRequest, Transport};

/// The settings file, relative to the repository root xtask is run from.
const SETTINGS_PATH: &str = ".claude/settings.json";

/// The OpenAI-shape compatible server, which is what this endpoint speaks.
const PROVIDER: &str = "compat";

/// A 32x32 solid-blue PNG. The endpoint rejects images of one pixel per side
/// with "must be larger than 10" - the kind of fact only a live probe learns.
const PROBE_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAIAAAD8GO2jAAAAJ0lEQVR4nO3NMQkAAAwDsPo33anoMQjkT5KOCQQCgUAgEAgEgr4IDjpL/C4b7k3TAAAAAElFTkSuQmCC";

pub fn run(_args: &[String]) -> ExitCode {
    match provision() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("ai-provision: refused - {message}");
            ExitCode::FAILURE
        }
    }
}

fn provision() -> Result<(), String> {
    let (key, base_url, model) = read_settings()?;
    if key.trim().is_empty() || base_url.trim().is_empty() {
        return Err(format!(
            "{SETTINGS_PATH} env needs ANTHROPIC_AUTH_TOKEN and ANTHROPIC_BASE_URL"
        ));
    }
    // The compatible provider appends `/v1/chat/completions` itself; a base
    // carrying `/v1` would double it, which is exactly the 403 this repository
    // hit once. Normalise to the host here so the stored shape is right.
    let base_url = base_url
        .trim_end_matches('/')
        .trim_end_matches("/v1")
        .to_string();
    let model = model.ok_or("settings.json env has no ANTHROPIC_MODEL to verify")?;
    if model.trim().is_empty() {
        return Err("ANTHROPIC_MODEL is empty".to_string());
    }
    let model = model.trim().to_string();
    println!(
        "ai-provision: probing {base_url} with model {model} (key {} chars)",
        key.len()
    );

    // 1. The text call the gateway's cheapest task would make.
    let url = format!("{base_url}/v1/chat/completions");
    let transport = HttpTransport::new();
    if !transport.schemes().iter().any(|scheme| *scheme == "https") {
        return Err(
            "this build's transport cannot reach https; rebuild with the tls feature".to_string(),
        );
    }
    let text_body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "Reply with the single word OK"}],
        "max_tokens": 200,
    });
    let text = post(&transport, &url, &key, &text_body)?;
    println!(
        "ai-provision: text ok - {:?}",
        text.chars().take(24).collect::<String>()
    );

    // 2. The vision call `PhotoAutoEdit` actually makes. This is the one that
    //    decides whether the model is usable by this product at all.
    let vision_body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": [
            {"type": "text", "text": "What single colour is this image? Answer one word."},
            {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{PROBE_PNG}")}}
        ]}],
        "max_tokens": 600,
    });
    let vision = post(&transport, &url, &key, &vision_body)?;
    if vision.trim().is_empty() {
        return Err("the vision call came back empty".to_string());
    }
    println!("ai-provision: vision ok - {vision:?}");

    // 3. Store through the product's own commands, at the catalog the shell
    //    itself will open.
    let path = catalog_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("cannot create {}: {err}", parent.display()))?;
    }
    let state = AppState::open(&path)
        .map_err(|err| format!("cannot open catalog: {}", err.user_message))?;

    set_ai_key(
        &state,
        &SetAiKeyInput {
            provider: PROVIDER.to_string(),
            key: key.clone(),
            endpoint: Some(base_url.clone()),
        },
    )
    .map_err(|err| format!("set_ai_key refused: {} - {}", err.code, err.message))?;
    println!("ai-provision: key written to the credential store");

    save_ai_setup(
        &state,
        &SaveAiSetupInput {
            provider: PROVIDER.to_string(),
            endpoint: Some(base_url.clone()),
            cheap_model: Some(model.clone()),
            balanced_model: Some(model.clone()),
            reasoning_model: Some(model.clone()),
            completed: true,
            skipped: false,
        },
    )
    .map_err(|err| format!("save_ai_setup refused: {} - {}", err.code, err.message))?;

    // 4. The Check button's own path: load the key back out of the credential
    //    store and one more call through the gateway. This is what proves the
    //    DPAPI round trip, not just the write.
    let check = check_ai_key(&state)
        .map_err(|err| format!("check_ai_key refused: {} - {}", err.code, err.message))?;
    if !check.ok {
        return Err(format!(
            "the key did not pass the app's own check: {}",
            check.message
        ));
    }
    println!("ai-provision: check ok - {}", check.message);

    let status =
        ai_setup_status(&state).map_err(|err| format!("read-back refused: {}", err.message))?;
    println!(
        "ai-provision: done - provider {} at {:?}, tiers {:?}, keyed {:?}, schemes {:?}",
        status.provider, status.endpoint, status.models, status.keyed_providers, status.schemes,
    );
    if !status.completed || status.provider != PROVIDER {
        return Err("the read-back does not match what was written".to_string());
    }
    Ok(())
}

fn read_settings() -> Result<(String, String, Option<String>), String> {
    let text = std::fs::read_to_string(SETTINGS_PATH)
        .map_err(|err| format!("read {SETTINGS_PATH}: {err} (run from the repository root)"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|err| format!("parse {SETTINGS_PATH}: {err}"))?;
    let get = |name: &str| -> Option<String> {
        value
            .get("env")
            .and_then(|env| env.get(name))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    Ok((
        get("ANTHROPIC_AUTH_TOKEN").unwrap_or_default(),
        get("ANTHROPIC_BASE_URL").unwrap_or_default(),
        get("ANTHROPIC_MODEL"),
    ))
}

/// One POST, through the same transport the gateway uses. A non-2xx is an error
/// here because every caller of this task treats the provider as either working
/// or not; the gateway's status taxonomy is not wanted in a provisioning probe.
fn post(
    transport: &HttpTransport,
    url: &str,
    key: &str,
    body: &serde_json::Value,
) -> Result<String, String> {
    let request = HttpRequest {
        method: "POST".to_string(),
        url: url.to_string(),
        headers: vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("accept".to_string(), "application/json".to_string()),
            ("authorization".to_string(), format!("Bearer {key}")),
        ],
        body: body.to_string().into_bytes(),
    };
    let response = transport
        .send(&request, Duration::from_secs(120))
        .map_err(|err| format!("{}", err.user_message))?;
    if !(200..300).contains(&response.status) {
        let snippet = String::from_utf8_lossy(&response.body);
        return Err(format!(
            "HTTP {} {}",
            response.status,
            snippet.chars().take(200).collect::<String>()
        ));
    }
    let parsed: serde_json::Value = serde_json::from_slice(&response.body)
        .map_err(|err| format!("unparseable reply: {err}"))?;
    parsed
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "the reply carried no message content".to_string())
}

/// The location the shell's own `catalog_path()` resolves to.
fn catalog_path() -> PathBuf {
    aura_core::paths::AppPaths::resolve()
        .map(|paths| paths.data_dir.join("catalogs").join("default.sqlite"))
        .unwrap_or_else(|_| PathBuf::from("catalogs/default.sqlite"))
}
