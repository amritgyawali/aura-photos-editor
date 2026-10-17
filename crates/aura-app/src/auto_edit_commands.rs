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

mod colour_guard;

/// Read actual pixels even when optional learned analysis models are unavailable.
///
/// # Errors
/// Refuses a photograph outside the project, and one whose preview cannot be decoded.
pub fn photo_analysis(
    state: &AppState,
    input: &PhotoAutoEditInput,
) -> IpcResult<crate::contract::ipc::PhotoAnalysisDto> {
    let photo = PhotoId::from_db(&input.photo_id)
        .map_err(|_| aura_core::errors::render::recipe_invalid("photo", "invalid identifier"))?;
    let belongs = state.catalog().read(|conn| {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM photo WHERE photo_id=?1 AND project_id=?2)",
            rusqlite::params![input.photo_id, input.project_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|e| aura_core::errors::db::statement_failed("analyze photograph", &e))
    })?;
    if !belongs {
        return Err(aura_core::errors::render::recipe_invalid(
            "photo",
            "photo does not belong to this project",
        )
        .into());
    }
    let buffer = state.previews(&input.project_id)?.get(
        photo,
        aura_raw::PixelLevel::Thumb(512),
        Priority::Interactive,
    )?;
    let aura_raw::PixelData::Srgb8(rgb) = &buffer.data else {
        return Err(aura_core::errors::raw::corrupt("analysis requires an sRGB preview").into());
    };
    let readings = PhotoReadings::measure(rgb, String::new())?;
    let recommendation = aura_cloud::photo_adjustment::local_adjustment(&readings);
    Ok(crate::contract::ipc::PhotoAnalysisDto {
        photo_id: input.photo_id.clone(),
        readings,
        recommendation,
    })
}

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

/// How far a model may move exposure beyond what the photograph's own measurement supports.
const UNCONFIRMED_STEP: f32 = 0.15;

/// The exposure a vision model's proposal ends at: AURA's own measurement decides, and the
/// model may move it by one small step either way.
///
/// Measured on twenty-two photographs and the same frames two thirds of a stop over, the
/// models asked to darken four of the good ones and five of the overexposed ones, and one
/// answered -0.5 almost whatever it was shown; the measurement, which only moves exposure on
/// evidence, darkened four of four overexposed copies it saw and none of five good ones. So a
/// model that says "leave it" does not undo what the measurement found, and a model that says
/// "darken" does not darken a photograph nothing says is bright.
fn confirmed_exposure(asked: f32, own: f32) -> f32 {
    own + (asked - own).clamp(-UNCONFIRMED_STEP, UNCONFIRMED_STEP)
}

/// The recipe an answer proposes for one photograph, and why, before it is merged.
fn proposed(
    state: &AppState,
    photo: PhotoId,
    project_id: &str,
    base: &aura_recipe::Recipe,
    answer: &aura_cloud::contract::cloud::CloudResult<
        aura_cloud::photo_adjustment::PhotoAdjustment,
    >,
) -> aura_core::AuraResult<(aura_recipe::Recipe, Vec<String>)> {
    let seen = crate::smart_edit::Seen::of(state, photo, project_id)?;
    if answer.source == aura_cloud::contract::cloud::Source::LocalFallback {
        // No model saw the photograph. The stand-in the task carries reads one histogram and
        // aims its median at middle grey, which darkens every bright studio frame by most of
        // a stop. AURA's own measured correction knows about faces and intent; use that.
        let (measured, decisions) = seen.measured_light(state, photo, base)?;
        let mut reasons =
            vec!["No AI provider answered, so this is AURA's own measured correction.".to_owned()];
        reasons.extend(decisions);
        return Ok((measured, reasons));
    }
    // Cloud proposes; deterministic code decides. The proposal is held to what the
    // photograph shows by the same guards AURA's own measurement goes through.
    let mut reasons = answer.value.reasons.clone();
    let mut tone = (
        answer.value.exposure,
        answer.value.highlights,
        answer.value.shadows,
        answer.value.contrast,
    );
    if let Some(note) = seen.guard(&mut tone) {
        reasons.push(format!(
            "AURA adjusted the proposal. {}",
            note.replace("the histogram asked for", "the AI asked for")
        ));
    }
    // Always measure the original colour, so repeating a cached proposal cannot walk
    // white balance away from the photograph. Manual values are protected at merge.
    let mut measurement_base = base.clone();
    measurement_base.global.temperature = 5500;
    measurement_base.global.tint = 0;
    let (measured, _) = seen.measured_light(state, photo, &measurement_base)?;
    let own = measured.global.exposure;
    let confirmed = confirmed_exposure(tone.0, own);
    if (confirmed - tone.0).abs() > 1e-3 {
        reasons.push(format!(
            "Exposure {confirmed:+.2} EV (the AI asked for {:+.2}): AURA's own measurement of this photograph is {own:+.2} EV, and the AI may move it by at most {UNCONFIRMED_STEP:.2}.",
            tone.0
        ));
        tone.0 = confirmed;
    }
    let mut proposal = base.clone();
    proposal.global.exposure = tone.0;
    proposal.global.highlights = tone.1;
    proposal.global.shadows = tone.2;
    proposal.global.contrast = tone.3;
    reasons.extend(colour_guard::apply(
        &answer.value,
        &measured.global,
        &mut proposal.global,
    ));
    // The finishing look is measured from the photograph, not proposed by the model; it rides
    // along with whatever light and colour the model's answer settled on. ADR-0123.
    proposal.global.parametric = measured.global.parametric;
    proposal.global.hsl.clone_from(&measured.global.hsl);
    proposal.global.colour_grade = measured.global.colour_grade;
    Ok((proposal, reasons))
}

fn edit(
    state: &AppState,
    input: &PhotoAutoEditInput,
    cancel: &CancelToken,
) -> IpcResult<PhotoAutoEditDto> {
    let _busy = crate::activity::begin(
        state,
        "ai",
        format!(
            "Asking the AI provider about {}",
            crate::activity::photo_name(state, &input.photo_id)
        ),
    );
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
    crate::internal_ai::authorize_new_project(state, &input.project_id)?;
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
    let (mut proposal, reasons) = proposed(state, photo, &input.project_id, &base, &answer)?;
    proposal.provenance.confidence = answer.confidence;
    proposal.provenance.source = EditSource::Ai;
    proposal.provenance.decision_id = Some(answer.call_id.to_string());
    let (merged, report) = schema::merge(&base, &proposal, EditSource::Ai)?;
    schema::Validation::check(&merged)?;
    let source = answer.source.as_str().to_string();
    let label = format!(
        "Auto edit ({source}, {}, {}): {}",
        answer.model,
        answer.value.preset,
        reasons.join(" ")
    );
    // A repeated/cached recommendation can change only audit provenance.
    // Keep the existing recipe and history when no editable parameter changed.
    let saved = if report.changed.is_empty() {
        base
    } else {
        state
            .recipe_store()
            .save(&project, &photo, &merged, &report.changed, &label)?;
        merged
    };
    Ok(PhotoAutoEditDto {
        recipe: recipe_dto(&input.photo_id, &saved),
        source,
        model: answer.model,
        reasons,
    })
}

#[cfg(test)]
mod tests {
    use super::confirmed_exposure;

    #[test]
    fn the_measurement_decides_exposure_and_a_model_refines_it_by_a_step() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        // The measurement found nothing: the model gets a small step either way.
        assert!(close(confirmed_exposure(-0.5, 0.0), -0.15));
        assert!(close(confirmed_exposure(0.9, 0.0), 0.15));
        assert!(close(confirmed_exposure(0.1, 0.0), 0.1));
        // The measurement found something: the model refines it by at most the step.
        assert!(close(confirmed_exposure(1.2, 0.6), 0.75));
        assert!(close(confirmed_exposure(0.4, 0.6), 0.45));
        assert!(close(confirmed_exposure(-0.9, -0.3), -0.45));
        // A model that says "leave it" does not undo an overexposure the measurement found.
        assert!(close(confirmed_exposure(0.0, -0.5), -0.35));
        assert!(close(confirmed_exposure(-0.6, 0.5), 0.35));
    }
}
