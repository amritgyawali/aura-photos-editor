//! The cloud half of the command surface.
//!
//! Three rules shape these commands.
//!
//! **The key goes one way.** [`set_ai_key`] carries it from the text field to the
//! operating system's credential store and nothing carries it back. There is no
//! `get_ai_key`, and [`cloud_status`] returns four characters from each end and a
//! store name. A panel that could display the key is a panel that puts it in a
//! screenshot in a support ticket.
//!
//! **Nothing here blocks for long.** Every command reads state that is already in
//! memory or one indexed row. [`check_ai_key`] is the exception, and it is
//! explicitly the command that spends a round trip in front of the user, with a
//! fifteen-second ceiling and no retries - a Check button that took ninety
//! seconds to fail three times would be worse than useless.
//!
//! **The panel tells the truth about what is off.** Offline studio mode, a
//! project switch that is off, a cap that has stopped calls and an open circuit
//! breaker are all reported with their reasons rather than presented as a working
//! system that happens to make no calls.

use aura_cloud::budget::MONTH_SCOPE;
use aura_cloud::contract::cloud::Tier;
use aura_cloud::keys::SecretKey;
use aura_cloud::provider::ProviderKind;

use crate::ai_settings::AiSetup;
use crate::commands::IpcResult;
use crate::contract::ipc::{
    AiModelDto, AiProviderDto, AiSetupStatusDto, CloudCacheStatsDto, CloudCallDto, CloudSpendDto,
    CloudStatusDto, KeyCheckDto, SaveAiSetupInput, SetAiKeyInput, SetCloudBudgetInput,
    SetCloudPrivacyInput,
};
use crate::state::AppState;

/// What Settings > AI Keys shows.
///
/// # Errors
///
/// `AURA-CLOUD-6012` when the credential store cannot be reached at all. A
/// *missing* key is not an error - it is the default state of a fresh install and
/// is reported as `keyPresent: false`.
pub fn cloud_status(state: &AppState) -> IpcResult<CloudStatusDto> {
    let cloud = state.cloud()?;
    let config = cloud.client().provider().config().clone();
    let account = config.kind.account();

    let stored = state.key_store()?.load(&account)?;
    let policy = cloud.policy();

    Ok(CloudStatusDto {
        provider: config.kind.as_str().to_string(),
        endpoint: config.endpoint.clone(),
        key_present: stored.is_some(),
        key_fingerprint: stored
            .as_ref()
            .map(SecretKey::fingerprint)
            .unwrap_or_default(),
        key_store: state.key_store_name().to_string(),
        offline_studio_mode: policy.offline_studio_mode,
        project_enabled: policy.project_enabled,
        blur_faces: policy.blur_faces,
        transport: cloud.client().transport_name().to_string(),
        breaker_reason: cloud.client().breaker().reason(),
        tier_models: [Tier::Cheap, Tier::Balanced, Tier::Reasoning]
            .iter()
            .filter_map(|tier| config.alias(*tier).map(|alias| alias.model.clone()))
            .collect(),
    })
}

/// Store a key in the operating system's credential store.
///
/// # Errors
///
/// `AURA-CLOUD-6012` when the credential store refuses. Nothing is written
/// anywhere else when it does.
pub fn set_ai_key(state: &AppState, input: &SetAiKeyInput) -> IpcResult<CloudStatusDto> {
    let kind = ProviderKind::parse(&input.provider);
    let secret = SecretKey::new(input.key.clone());
    state.key_store()?.store(&kind.account(), &secret)?;

    // Storing a key *is* choosing a provider, and the choice is written down
    // rather than left in memory. Phase 04 left it in memory: a photographer
    // picked Google, pasted a key, closed the application, and reopened it
    // pointed at Anthropic with no key - which reads as the key having been lost.
    let mut setup = state.ai_setup()?;
    setup.provider = kind.as_str().to_string();
    setup.endpoint = input.endpoint.clone();
    setup.completed = true;
    setup.skipped = false;
    state.save_ai_setup(&setup)?;

    tracing::info!(
        target: "cloud.key_stored",
        provider = kind.as_str(),
        "an API key was stored; the key itself is not logged anywhere"
    );
    cloud_status(state)
}

/// Forget the key for one provider.
///
/// # Errors
///
/// `AURA-CLOUD-6012` when the credential store refuses. Deleting a key that is
/// not there is success.
pub fn clear_ai_key(state: &AppState, provider: &str) -> IpcResult<CloudStatusDto> {
    let kind = ProviderKind::parse(provider);
    state.key_store()?.delete(&kind.account())?;
    cloud_status(state)
}

/// Prove the stored key works. The Check button.
///
/// Returns a `KeyCheckDto` rather than an error on rejection, because "your
/// provider would not accept this key" is an answer to the question the button
/// asked, not a failure of the command.
///
/// # Errors
///
/// `AURA-CLOUD-6012` when the credential store cannot be read.
pub fn check_ai_key(state: &AppState) -> IpcResult<KeyCheckDto> {
    let cloud = state.cloud()?;
    let kind = cloud.client().provider().config().kind;

    let stored = state.key_store()?.load(&kind.account())?;
    let secret = match stored {
        Some(secret) => secret,
        // A local server has no key, and "check" is the most useful button on the
        // screen for one: the thing most likely to be wrong about Ollama is that it
        // is not running. Probing with an empty secret sends no credential header
        // at all, which is exactly what such a server expects.
        None if !aura_cloud::catalog::spec(kind).requires_key => SecretKey::new(String::new()),
        None => {
            return Ok(KeyCheckDto {
                ok: false,
                model: String::new(),
                message: "No key is stored for this provider yet.".to_string(),
            })
        }
    };

    match cloud.validate_key(&secret) {
        Ok(model) => Ok(KeyCheckDto {
            ok: true,
            model: model.clone(),
            message: format!("The key works. {model} answered."),
        }),
        Err(err) => Ok(KeyCheckDto {
            ok: false,
            model: String::new(),
            message: err.user_message.clone(),
        }),
    }
}

/// Every provider AURA knows how to reach.
///
/// Static, and deliberately says nothing about this machine. Whether a key is
/// stored is in [`ai_setup_status`], because that answer costs a read of the
/// credential store and this one costs nothing - a picker that had to wait for
/// nineteen keychain reads before it could draw would be a picker that feels
/// broken on the one screen a photographer meets first.
///
/// # Errors
///
/// Never. The signature matches every other command so a caller does not have to
/// know which of them can fail.
pub fn list_ai_providers(_state: &AppState) -> IpcResult<Vec<AiProviderDto>> {
    Ok(aura_cloud::catalog::all()
        .iter()
        .map(|spec| AiProviderDto {
            id: spec.kind.as_str().to_string(),
            label: spec.label.to_string(),
            blurb: spec.blurb.to_string(),
            wire: spec.wire.as_str().to_string(),
            endpoint: spec.endpoint.to_string(),
            endpoint_editable: spec.endpoint_editable,
            requires_key: spec.requires_key,
            key_hint: spec.key_hint.to_string(),
            keys_url: spec.keys_url.to_string(),
            images: spec.images,
            models: spec
                .tiers
                .iter()
                .map(|entry| AiModelDto {
                    tier: tier_name(entry.tier).to_string(),
                    model: entry.model.to_string(),
                    input_per_mtok_usd: entry.input_per_mtok_usd,
                    output_per_mtok_usd: entry.output_per_mtok_usd,
                })
                .collect(),
        })
        .collect())
}

/// What the first-run screen and the AI panel both need to know.
///
/// # Errors
///
/// `AURA-CLOUD-6012` when the credential store cannot be reached at all.
pub fn ai_setup_status(state: &AppState) -> IpcResult<AiSetupStatusDto> {
    let stored = state.ai_setup()?;
    let keys = state.key_store()?;

    // A provider whose key cannot be read is reported as *not* keyed rather than
    // failing the whole panel. One broken entry in a credential store must not be
    // able to hide the other eighteen providers from somebody trying to get set
    // up, and the Check button is what turns a wrong answer here into a sentence.
    let keyed = ProviderKind::ALL
        .iter()
        .filter(|kind| keys.has(&kind.account()).unwrap_or(false))
        .map(|kind| kind.as_str().to_string())
        .collect();

    let models = stored.models();
    Ok(AiSetupStatusDto {
        completed: stored.completed,
        skipped: stored.skipped,
        provider: stored.kind().as_str().to_string(),
        endpoint: stored.endpoint().map(ToString::to_string),
        models: [Tier::Cheap, Tier::Balanced, Tier::Reasoning]
            .iter()
            .map(|tier| models.for_tier(*tier).unwrap_or_default().to_string())
            .collect(),
        keyed_providers: keyed,
        schemes: state.cloud()?.client().transport_schemes(),
        offline_studio_mode: state.cloud_policy().offline_studio_mode,
    })
}

/// Record the provider choice, and point this process at it.
///
/// Writes the setting row and rebuilds the gateway, so the next call goes where
/// the panel says it will. The key is not part of this: it travels through
/// [`set_ai_key`] and nothing else.
///
/// # Errors
///
/// `AURA-DB-3006` when the setting row cannot be written.
pub fn save_ai_setup(state: &AppState, input: &SaveAiSetupInput) -> IpcResult<AiSetupStatusDto> {
    let kind = ProviderKind::parse(&input.provider);
    let setup = AiSetup {
        provider: kind.as_str().to_string(),
        endpoint: input.endpoint.clone(),
        cheap_model: input.cheap_model.clone(),
        balanced_model: input.balanced_model.clone(),
        reasoning_model: input.reasoning_model.clone(),
        completed: input.completed,
        skipped: input.skipped,
    };
    state.save_ai_setup(&setup)?;
    tracing::info!(
        target: "cloud.setup",
        provider = kind.as_str(),
        skipped = input.skipped,
        "the AI provider choice was recorded; no key is part of this row"
    );
    ai_setup_status(state)
}

/// Answer the first-run screen by declining it.
///
/// A separate command rather than a flag on [`save_ai_setup`], because declining
/// must not be able to record a provider. Somebody who pressed "not now" has not
/// chosen Anthropic, and a panel that later showed them a configured-looking
/// Anthropic with no key behind it would be reporting a decision nobody made.
///
/// # Errors
///
/// `AURA-DB-3006` when the setting row cannot be written.
pub fn skip_ai_setup(state: &AppState) -> IpcResult<AiSetupStatusDto> {
    let mut setup = state.ai_setup()?;
    setup.completed = true;
    setup.skipped = true;
    state.save_ai_setup(&setup)?;
    ai_setup_status(state)
}

/// `cheap`, `balanced` or `reasoning`, as the wire spells them.
const fn tier_name(tier: Tier) -> &'static str {
    match tier {
        Tier::Cheap => "cheap",
        Tier::Balanced => "balanced",
        Tier::Reasoning => "reasoning",
    }
}

/// Set the per-job and per-month caps.
///
/// # Errors
///
/// `AURA-DB-3006` when the caps cannot be written.
pub fn set_cloud_budget(state: &AppState, input: &SetCloudBudgetInput) -> IpcResult<CloudSpendDto> {
    let cloud = state.cloud()?;
    let store = cloud.governor().store();
    store.set_cap(&input.project_id, input.cap_usd, input.hard_stop)?;
    store.set_cap(MONTH_SCOPE, input.month_cap_usd, input.hard_stop)?;
    cloud_spend(state, &input.project_id)
}

/// Set the privacy switches.
///
/// # Errors
///
/// Whatever building the gateway raised.
pub fn set_cloud_privacy(
    state: &AppState,
    input: &SetCloudPrivacyInput,
) -> IpcResult<CloudStatusDto> {
    state.set_cloud_policy(aura_cloud::CloudPolicy {
        offline_studio_mode: input.offline_studio_mode,
        project_enabled: input.enabled,
        blur_faces: input.blur_faces,
        ..aura_cloud::CloudPolicy::default()
    })?;
    tracing::info!(
        target: "cloud.policy",
        offline = input.offline_studio_mode,
        enabled = input.enabled,
        blur = input.blur_faces,
        "cloud privacy switches changed"
    );
    cloud_status(state)
}

/// The live spend meter.
///
/// # Errors
///
/// `AURA-DB-3006` when the caps or the audit trail cannot be read.
pub fn cloud_spend(state: &AppState, project_id: &str) -> IpcResult<CloudSpendDto> {
    let cloud = state.cloud()?;
    let store = cloud.governor().store();
    let project = store.state(project_id)?;
    let month = store.state(MONTH_SCOPE)?;
    let ledger = cloud.ledger();

    Ok(CloudSpendDto {
        cap_usd: project.cap_usd,
        // The authoritative figure is the sum of what was actually billed, not
        // the running counter: a counter can drift if a write is lost, and the
        // audit rows cannot.
        spent_usd: cloud
            .audit()
            .spent_usd(project_id)
            .unwrap_or(project.spent_usd),
        month_cap_usd: month.cap_usd,
        month_spent_usd: month.spent_usd,
        calls: project.calls,
        downgrades: project.downgrades,
        fallbacks: ledger.fallbacks(),
        cache_hit_rate: ledger.cache_hit_rate(),
        stopped: project.is_stopped() || month.is_stopped(),
    })
}

/// The audit viewer's rows, newest first.
///
/// # Errors
///
/// `AURA-DB-3006` when the trail cannot be read.
pub fn cloud_calls(state: &AppState, project_id: &str, limit: u32) -> IpcResult<Vec<CloudCallDto>> {
    let cloud = state.cloud()?;
    Ok(cloud
        .audit()
        .recent(project_id, limit.clamp(1, 500))?
        .into_iter()
        .map(|row| CloudCallDto {
            id: row.id.to_string(),
            task: row.task,
            task_version: u32::from(row.task_version),
            model: row.model,
            source: row.source.as_str().to_string(),
            fallback_reason: row.fallback_reason,
            tokens_in: row.tokens_in,
            tokens_out: row.tokens_out,
            cost_usd: row.cost_usd,
            latency_ms: row.latency_ms,
            status: row.status,
            retry_count: row.retry_count,
            prompt_hash: row.prompt_hash,
            confidence: row.confidence,
            decision_ref: row.decision_ref,
        })
        .collect())
}

/// What the response cache is holding.
///
/// # Errors
///
/// `AURA-DB-3006` when the cache cannot be read.
pub fn cloud_cache_stats(state: &AppState) -> IpcResult<CloudCacheStatsDto> {
    let held = state.cloud()?.cache().stats()?;
    Ok(CloudCacheStatsDto {
        entries: held.entries,
        bytes: held.bytes,
        hits: held.hits,
    })
}

/// Forget every cached answer for one task version.
///
/// The button a support engineer presses after a prompt change that should have
/// been a version bump and was not.
///
/// # Errors
///
/// `AURA-DB-3006` when the cache cannot be written.
pub fn purge_cloud_cache(state: &AppState, task: &str, task_version: u32) -> IpcResult<u64> {
    Ok(state
        .cloud()?
        .cache()
        .purge_task(task, u16::try_from(task_version).unwrap_or(u16::MAX))?)
}
