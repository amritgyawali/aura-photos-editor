//! One photograph's automatic edit, using the configured governed AI gateway.

use aura_cloud::contract::cloud::Validate;
use aura_cloud::gateway::CallContext;
use aura_cloud::payload::{crop, PayloadPolicy, SourceImage};
use aura_cloud::photo_adjustment::{PhotoAutoEdit, PhotoReadings};
use aura_core::progress::CancelToken;
use aura_core::{PhotoId, ProjectId};
use aura_preview::contract::service::{PreviewService, Priority};
use aura_recipe::{schema, EditSource};

use crate::commands::IpcResult;
use crate::contract::ipc::{PhotoAutoEditDto, PhotoAutoEditInput};
use crate::develop_commands::{load_or_neutral, recipe_dto};
use crate::AppState;

/// Analyse original pixels and save a reversible, manually protected edit.
///
/// # Errors
/// Returns a typed error for missing images, undecodable pixels or a failed save.
pub fn photo_auto_edit(
    state: &AppState,
    input: &PhotoAutoEditInput,
) -> IpcResult<PhotoAutoEditDto> {
    let cancel = CancelToken::new();
    state.register_job(&input.job_id, cancel.clone());
    let result = edit(state, input, &cancel);
    state.finish_job(&input.job_id);
    result
}

fn edit(
    state: &AppState,
    input: &PhotoAutoEditInput,
    cancel: &CancelToken,
) -> IpcResult<PhotoAutoEditDto> {
    let invalid =
        |field: &str| aura_core::errors::render::recipe_invalid(field, "invalid identifier");
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("photo"))?;
    let project = ProjectId::from_db(&input.project_id).map_err(|_| invalid("project"))?;
    let belongs = state.catalog().read(|conn| {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM photo WHERE photo_id=?1 AND project_id=?2)",
            rusqlite::params![input.photo_id, input.project_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|e| aura_core::errors::db::statement_failed("auto-edit photograph", &e))
    })?;
    if !belongs {
        return Err(invalid("photo does not belong to this project").into());
    }
    let buffer = state.previews(&input.project_id)?.get(
        photo,
        aura_raw::PixelLevel::Thumb(512),
        Priority::Interactive,
    )?;
    let aura_raw::PixelData::Srgb8(rgb) = &buffer.data else {
        return Err(aura_core::errors::raw::corrupt("auto-edit requires an sRGB preview").into());
    };
    let policy = state.cloud_policy();
    // When face blur is requested but no detector has supplied regions, stay local.
    let image = crop(&SourceImage::new(&buffer), PayloadPolicy::default())?;
    let readings = PhotoReadings::measure(rgb, image.content_hash.clone())?;
    let task = PhotoAutoEdit { image };
    let answer = if policy.blur_faces {
        use aura_cloud::contract::cloud::{CloudResult, CloudTask};
        let value = task.local_fallback(&readings)?;
        CloudResult::local(value, 0.35, uuid::Uuid::new_v4())
    } else {
        state.cloud()?.run(
            &task,
            &readings,
            &CallContext {
                project: &project,
                decision_ref: Some(&input.photo_id),
                cancel,
            },
        )?
    };
    if cancel.is_cancelled() {
        return Err(aura_core::errors::cloud::cancelled("auto-edit stopped before saving").into());
    }
    answer
        .value
        .validate()
        .map_err(|e| aura_core::errors::render::recipe_invalid("auto-edit", &e))?;
    // Reload after the provider responds so edits made during the request stay protected.
    let base = load_or_neutral(state, photo)?;
    let mut proposal = base.clone();
    proposal.global.exposure = answer.value.exposure;
    proposal.global.contrast = answer.value.contrast;
    proposal.global.highlights = answer.value.highlights;
    proposal.global.shadows = answer.value.shadows;
    proposal.global.vibrance = answer.value.vibrance;
    proposal.provenance.confidence = answer.confidence;
    proposal.provenance.source = EditSource::Ai;
    proposal.provenance.decision_id = Some(answer.call_id.to_string());
    let (merged, report) = schema::merge(&base, &proposal, EditSource::Ai)?;
    schema::Validation::check(&merged)?;
    let source = answer.source.as_str().to_string();
    let label = format!(
        "Auto edit ({source}, {}): {}",
        answer.model,
        answer.value.reasons.join(" ")
    );
    state
        .recipe_store()
        .save(&project, &photo, &merged, &report.changed, &label)?;
    Ok(PhotoAutoEditDto {
        recipe: recipe_dto(&input.photo_id, &merged),
        source,
        model: answer.model,
        reasons: answer.value.reasons,
    })
}
