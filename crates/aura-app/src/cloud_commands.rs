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

use crate::commands::IpcResult;
use crate::contract::ipc::{
    CloudCacheStatsDto, CloudCallDto, CloudSpendDto, CloudStatusDto, KeyCheckDto, SetAiKeyInput,
    SetCloudBudgetInput, SetCloudPrivacyInput,
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
    state.set_cloud_provider(kind, input.endpoint.as_deref())?;
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
    let account = cloud.client().provider().config().kind.account();

    let Some(secret) = state.key_store()?.load(&account)? else {
        return Ok(KeyCheckDto {
            ok: false,
            model: String::new(),
            message: "No key is stored for this provider yet.".to_string(),
        });
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
