//! Automatic internal AI configuration. Credentials never enter the catalogue.
use crate::state::AppState;
use aura_cloud::internal_ai::Profile;
use aura_core::errors::cloud::payload_refused;
use aura_core::errors::db::statement_failed;
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const KEY: &str = "ai.internal_profiles.v1";
static IMPORT: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// Retouch one photo with bounded vision guidance when no saved user settings
/// exist. Saved preferences and manual operations always retain precedence.
/// # Errors
/// Membership, decoding or saving failures. Cloud outages use local retouch.
pub fn retouch(
    state: &AppState,
    input: &crate::contract::ipc::DevelopImageInput,
) -> crate::commands::IpcResult<crate::contract::ipc::RecipeDto> {
    retouch_with_cancel(state, input, &aura_core::progress::CancelToken::new())
}

pub(crate) fn retouch_with_cancel(
    state: &AppState,
    input: &crate::contract::ipc::DevelopImageInput,
    cancel: &aura_core::progress::CancelToken,
) -> crate::commands::IpcResult<crate::contract::ipc::RecipeDto> {
    use aura_cloud::contract::cloud::Source;
    use aura_preview::contract::service::{PreviewService, Priority};
    let photo = aura_core::PhotoId::from_db(&input.photo_id)
        .map_err(|_| payload_refused("Invalid retouch photo identifier"))?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    let cloud = state.cloud()?;
    if has_custom_skin_preferences(&base) {
        return crate::smart_edit::run(state, input, false);
    }
    if !cloud.has_internal_routes()
        || cloud.policy().refusal().is_some()
        || cloud.policy().blur_faces
    {
        return local_skin_retouch(state, input);
    }
    let project = state.project_of(photo)?;
    authorize_new_project(state, &project.to_string())?;
    let buffer = state.previews(&project.to_string())?.get(
        photo,
        aura_raw::PixelLevel::Thumb(512),
        Priority::Interactive,
    )?;
    let Some(rgb) = buffer.as_srgb8() else {
        return local_skin_retouch(state, input);
    };
    let image = aura_cloud::payload::crop(
        &aura_cloud::payload::SourceImage::new(&buffer),
        aura_cloud::payload::PayloadPolicy::default(),
    )?;
    let readings =
        aura_cloud::photo_adjustment::PhotoReadings::measure(rgb, image.content_hash.clone())?;
    let answer = cloud.run(
        &aura_cloud::skin_advice::SkinAdvice { image },
        &readings,
        &aura_cloud::gateway::CallContext {
            project: &project,
            decision_ref: Some(&input.photo_id),
            cancel,
        },
    );
    let answer = answer?;
    if cancel.is_cancelled() {
        return Err(aura_core::errors::cloud::cancelled("AI retouch stopped before saving").into());
    }
    if answer.source == Source::LocalFallback
        || answer.confidence < 0.6
        || !answer.value.visible_skin
    {
        return local_skin_retouch(state, input);
    }
    // A manual setting may have been saved while the provider was answering.
    let fresh = crate::develop_commands::load_or_neutral(state, photo)?;
    if aura_recipe::recipe_hash(&fresh)? != aura_recipe::recipe_hash(&base)? {
        return crate::smart_edit::run(state, input, false);
    }
    let options = fresh_skin_options(Some(&answer.value));
    crate::smart_edit::auto_retouch(
        state,
        &crate::smart_edit::AutoRetouchInput {
            project_id: project.to_string(),
            photo_id: input.photo_id.clone(),
            global: false,
            options,
        },
    )
}

// Both online and offline automation must enable visible body healing. Keep
// protection and texture defaults together so the two paths cannot drift.
pub(crate) fn fresh_skin_options(
    advice: Option<&aura_cloud::skin_advice::Advice>,
) -> crate::portrait_features::Options {
    let mut settings = crate::retouch_settings::Settings {
        deep_blemish_cleanup: true,
        smoothing: 0.15,
        body_smoothing: 0.15,
        body_blemishes: 0.75,
        texture: 0.85,
        frequency_heal: 0.75,
        max_spots: 512,
        keep_freckles: true,
        remove_dark_marks: false,
        ..crate::retouch_settings::Settings::default()
    };
    if let Some(advice) = advice {
        settings.cleanup_passes = advice.cleanup_passes;
        settings.blemish_sensitivity = advice.blemish_sensitivity;
        settings.frequency_heal = advice.frequency_heal;
        settings.pore_refine = advice.pore_refine;
    }
    crate::portrait_features::Options {
        scope: crate::portrait_features::Scope::FaceAndBody,
        eyes: false,
        teeth: false,
        refine: false,
        settings,
        ..crate::portrait_features::Options::default()
    }
}

pub(crate) fn has_custom_skin_preferences(base: &aura_recipe::Recipe) -> bool {
    let report = base.extra.get(crate::portrait_auto::KEY);
    let explicitly_chosen = report
        .and_then(|value| value.get("explicitOptions"))
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    explicitly_chosen
        || base.provenance.user_edited_fields.iter().any(|path| {
            path == aura_recipe::retouch_tools::KEY
                || path.starts_with(&format!("{}.", aura_recipe::retouch_tools::KEY))
        })
        || report
            .and_then(|value| value.get("options"))
            .is_some_and(|value| {
                let saved: Option<crate::portrait_features::Options> =
                    serde_json::from_value(value.clone()).ok();
                saved.is_none_or(|saved| {
                    let encoded = serde_json::to_value(saved).ok();
                    encoded
                        != serde_json::to_value(crate::portrait_features::Options::default()).ok()
                        && encoded != serde_json::to_value(fresh_skin_options(None)).ok()
                })
            })
}

// Fresh/default photos still evaluate all visible skin when a service is unavailable.
// Saved custom choices take the separate branch above, including explicit face-only work.
fn local_skin_retouch(
    state: &AppState,
    input: &crate::contract::ipc::DevelopImageInput,
) -> crate::commands::IpcResult<crate::contract::ipc::RecipeDto> {
    let photo = aura_core::PhotoId::from_db(&input.photo_id)
        .map_err(|_| payload_refused("Invalid retouch photo identifier"))?;
    let project = state.project_of(photo)?;
    crate::smart_edit::auto_retouch(
        state,
        &crate::smart_edit::AutoRetouchInput {
            project_id: project.to_string(),
            photo_id: input.photo_id.clone(),
            global: false,
            options: fresh_skin_options(None),
        },
    )
}

/// Nonsecret configuration and discovery report.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub profiles: Vec<Profile>,
    pub checked_day: String,
    pub source_available: bool,
}

/// Read safe metadata without touching a key or opening a connection.
/// # Errors
/// Catalogue read failures.
pub fn status(state: &AppState) -> AuraResult<Status> {
    state.catalog().read(|conn| {
        use rusqlite::OptionalExtension;
        let encoded: Option<String> = conn
            .query_row(
                "SELECT value_json FROM setting WHERE key=?1",
                [KEY],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| statement_failed("Could not read internal AI configuration", &e))?;
        encoded
            .map(|value| {
                serde_json::from_str(&value)
                    .map_err(|_| payload_refused("Stored internal AI configuration is invalid"))
            })
            .transpose()
            .map(Option::unwrap_or_default)
    })
}

/// Import the supplied local file on startup and refresh discovery once daily.
/// The environment override is useful for another machine or an isolated test.
/// # Errors
/// Invalid configuration, credential-store or catalogue failure. No secret data
/// is included in these errors.
pub fn initialize(state: &AppState, refresh: bool) -> AuraResult<Status> {
    let _import = IMPORT.lock();
    let previous = status(state)?;
    // Offline mode suppresses discovery as well as image requests. Restoring
    // policy does not load any credential from the OS store.
    if state.cloud()?.policy().offline_studio_mode {
        return Ok(previous);
    }
    let day = state.clock().now_utc().date().to_string();
    if !refresh && previous.source_available && previous.checked_day == day {
        return Ok(previous);
    }
    let path = std::env::var_os("AURA_AI_CONFIG_FILE").map_or_else(
        || std::path::PathBuf::from(r"D:\AI-Company\HERMES-AI-CONFIG.md"),
        std::path::PathBuf::from,
    );
    if !path.is_file() {
        return Ok(previous);
    }
    let bytes = std::fs::metadata(&path)
        .map_err(|_| payload_refused("Could not inspect AI configuration file"))?
        .len();
    if bytes > 64 * 1024 {
        return Err(payload_refused("AI configuration is too large"));
    }
    let text = aura_cloud::keys::SecretKey::new(
        std::fs::read_to_string(&path)
            .map_err(|_| payload_refused("Could not read AI configuration file"))?,
    );
    let keys = state.key_store()?;
    let transport = aura_cloud::http::HttpTransport::new();
    let mut profiles = aura_cloud::internal_ai::import(text.expose(), keys.as_ref(), &transport)?;
    profiles.sort_by(|a, b| b.priority().total_cmp(&a.priority()).then(a.id.cmp(&b.id)));
    let next = Status {
        profiles,
        checked_day: day,
        source_available: true,
    };
    let encoded = serde_json::to_string(&next)
        .map_err(|_| payload_refused("Could not encode internal AI metadata"))?;
    let now = aura_catalog::rfc3339(state.clock().now_utc());
    state.catalog().writer().with(move |conn| {
        conn.execute("INSERT INTO setting(key,value_json,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at", rusqlite::params![KEY, encoded, now])
            .map_err(|e| statement_failed("Could not store internal AI metadata", &e))?;
        Ok(())
    })?;
    state.reload_internal_ai();
    let privacy_chosen = state.catalog().read(|conn| {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM setting WHERE key='ai.privacy.v1')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|e| statement_failed("Could not inspect AI privacy setting", &e))
    })?;
    if !previous.source_available && !privacy_chosen && next.profiles.iter().any(Profile::available)
    {
        // The supplied configuration activates internal routing once. Later
        // refreshes never re-enable a user's offline/privacy choice.
        let policy = state.cloud()?.policy();
        if !policy.offline_studio_mode {
            state.set_cloud_policy(aura_cloud::CloudPolicy {
                project_enabled: true,
                ..policy
            })?;
        }
    }
    // Configuration completes setup without copying a credential to the legacy
    // provider slot. A user can still switch privacy off independently.
    let mut setup = state.ai_setup()?;
    if !setup.completed && next.profiles.iter().any(Profile::available) {
        setup.completed = true;
        setup.skipped = false;
        state.save_ai_setup(&setup)?;
    }
    Ok(next)
}

/// Store a user quality judgement for a real successful AI call. The record is
/// local and contains no image, filename, prompt or key. Daily routing reads it.
/// # Errors
/// Unknown call, wrong project, or a result that came from a local fallback.
pub fn feedback(state: &AppState, project: &str, call_id: &str, accepted: bool) -> AuraResult<()> {
    let id: uuid::Uuid = call_id
        .parse()
        .map_err(|_| payload_refused("Invalid AI call identifier"))?;
    let cloud = state.cloud()?;
    let rows = cloud.audit().recent(project, 512)?;
    let original = rows
        .into_iter()
        .find(|row| {
            row.id == id
                && row.source == aura_cloud::contract::cloud::Source::Cloud
                && matches!(row.status.as_str(), "ok" | "repaired")
        })
        .ok_or_else(|| payload_refused("AI feedback needs a successful call in this project"))?;
    let feedback_id = rating_id(project, call_id);
    let mut record = aura_cloud::audit::CallRecord::local(
        feedback_id,
        "internal_ai_feedback",
        1,
        "user_feedback",
    )
    .in_project(project)
    .for_decision(call_id);
    record.provider = original.provider;
    record.model = original.model;
    record.status = if accepted { "accepted" } else { "rejected" }.to_string();
    cloud.audit().record(&record);
    Ok(())
}

fn rating_id(project: &str, call_id: &str) -> uuid::Uuid {
    let digest = blake3::hash(format!("internal-feedback:{project}:{call_id}").as_bytes());
    let mut bytes = [0_u8; 16];
    for (out, value) in bytes.iter_mut().zip(digest.as_bytes()) {
        *out = *value;
    }
    uuid::Uuid::from_bytes(bytes)
}

/// Explicitly enabling cloud AI for a project records derived-image consent.
/// Full-resolution imagery and shared model training remain separate choices.
/// # Errors
/// Invalid project or consent ledger failure.
pub fn enable_project(state: &AppState, project: &str, enabled: bool) -> AuraResult<()> {
    let project = aura_core::ProjectId::from_db(project)
        .map_err(|_| payload_refused("Invalid AI project identifier"))?;
    aura_catalog::consent::CatalogConsent::new(
        Arc::clone(state.catalog()),
        Arc::clone(state.clock()),
    )
    .grant(&project, enabled, enabled, "AURA AI project setting")
}

/// A requested AI edit may activate derived-image consent for an untouched
/// project when the user supplied the internal configuration. A previous
/// explicit privacy choice, revocation or expiry is never overridden.
/// # Errors
/// Catalogue or consent ledger failures.
pub fn authorize_new_project(state: &AppState, project: &str) -> AuraResult<()> {
    if !status(state)?.source_available
        || !state.cloud()?.policy().project_enabled
        || state.cloud_policy().offline_studio_mode
    {
        return Ok(());
    }
    let untouched = state.catalog().read(|conn| {
        conn.query_row("SELECT granted_at IS NULL AND revoked_at IS NULL AND expires_at IS NULL FROM project_consent WHERE project_id=?1", [project], |row| row.get::<_,bool>(0))
            .map_err(|e| statement_failed("Could not read project AI permissions", &e))
    })?;
    if untouched {
        enable_project(state, project, true)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_ratings_replace_one_record_and_remain_project_scoped() {
        assert_eq!(
            rating_id("project-one", "call-one"),
            rating_id("project-one", "call-one")
        );
        assert_ne!(
            rating_id("project-one", "call-one"),
            rating_id("project-two", "call-one")
        );
        assert_ne!(
            rating_id("project-one", "call-one"),
            rating_id("project-one", "call-two")
        );
    }
}
