//! The Studio's finishing tools: face and body shape, liquify, background replacement, and
//! feature colour and makeup. ADR-0108.
//!
//! Two commands, one to read what a photograph carries and one to store the panel's state as a
//! single undoable manual edit. Nothing here runs automatically and no automatic pass writes this
//! extension, so a reshape only ever exists because a photographer made it.
//!
//! A third, `suggest_portrait_light`, measures how each face is lit and proposes a portrait light
//! (ADR-0127). It stores nothing: the panel shows the proposal and its reasons, and the
//! photographer saves it - or does not - like any other value in the panel.
use aura_core::{AuraError, PhotoId, ProjectId};
use aura_recipe::studio_finish::{self, Light, StudioFinish};
use aura_recipe::{schema, EditSource};
use aura_render::{FrameSource, RenderLevel};
use serde::{Deserialize, Serialize};

use crate::commands::IpcResult;
use crate::contract::ipc::RecipeDto;
use crate::AppState;

fn invalid(message: &str) -> AuraError {
    let mut error = aura_core::errors::render::recipe_invalid("studio finish", message);
    message.clone_into(&mut error.user_message);
    error
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StudioFinishInput {
    pub project_id: String,
    pub photo_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveStudioFinishInput {
    pub project_id: String,
    pub photo_id: String,
    pub finish: StudioFinish,
    /// A short label for the history row.
    #[serde(default)]
    pub label: Option<String>,
}

/// What both commands return: the finish as stored, and the recipe it is in.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioFinishDto {
    pub finish: StudioFinish,
    pub recipe: RecipeDto,
}

fn ids(project: &str, photo: &str) -> Result<(ProjectId, PhotoId), AuraError> {
    let photo = PhotoId::from_db(photo).map_err(|_| invalid("Invalid photo identifier"))?;
    let project =
        ProjectId::from_db(project).map_err(|_| invalid("Invalid collection identifier"))?;
    Ok((project, photo))
}

/// The finishing tools one photograph carries.
/// # Errors
/// An invalid photograph or recipe.
pub fn studio_finish(state: &AppState, input: &StudioFinishInput) -> IpcResult<StudioFinishDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let (_, photo) = ids(&input.project_id, &input.photo_id)?;
    let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
    Ok(StudioFinishDto {
        finish: studio_finish::read(&recipe)?,
        recipe: crate::develop_commands::recipe_dto(&input.photo_id, &recipe),
    })
}

/// Store the panel's finishing tools as one manual edit.
/// # Errors
/// An invalid photograph, a value out of range, or a failed save.
pub fn save_studio_finish(
    state: &AppState,
    input: &SaveStudioFinishInput,
) -> IpcResult<StudioFinishDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let (project, photo) = ids(&input.project_id, &input.photo_id)?;
    studio_finish::validate(&input.finish)?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    let mut proposal = base.clone();
    studio_finish::write(&mut proposal, &input.finish)?;
    let (merged, changes) = schema::merge(&base, &proposal, EditSource::User)?;
    schema::Validation::check(&merged)?;
    let label = input
        .label
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map_or_else(
            || "Portrait finish".to_string(),
            |l| l.chars().take(80).collect(),
        );
    state
        .recipe_store()
        .save(&project, &photo, &merged, &changes.changed, &label)?;
    Ok(StudioFinishDto {
        finish: studio_finish::read(&merged)?,
        recipe: crate::develop_commands::recipe_dto(&input.photo_id, &merged),
    })
}

/// What Auto light proposes. ADR-0127.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightSuggestionDto {
    /// The proposed portrait light. Glow is never proposed: it is a look, not a correction.
    pub light: Light,
    /// Faces the measurement found.
    pub faces: usize,
    /// How many stops brighter the main face's lit side is than its shadow side.
    pub contrast: Option<f32>,
    /// The decisions, in words.
    pub reasons: Vec<String>,
}

/// Measure how each face in a photograph is lit and propose a portrait light: a fill for harsh
/// side light, a soft key for flat light, catchlights for eyes without them. Stores nothing.
/// ADR-0127.
/// # Errors
/// An invalid photograph, or a preview that cannot be read.
pub fn suggest_portrait_light(
    state: &AppState,
    input: &StudioFinishInput,
) -> IpcResult<LightSuggestionDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let (_, photo) = ids(&input.project_id, &input.photo_id)?;
    let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
    let frame = crate::photo_frames::CatalogFrames::new(state.clone())
        .frame(&photo, RenderLevel::Proxy2048)?;
    let map = aura_render::portrait::parse(&frame, &aura_render::portrait::hints(&recipe));
    let suggestion = map.as_deref().map_or_else(
        || aura_render::portrait_light::LightSuggestion {
            light: Light::default(),
            faces: 0,
            contrast: None,
            reasons: vec!["The photograph could not be read for faces.".to_owned()],
        },
        |map| aura_render::portrait_light::suggest(&frame.rgb, frame.width, frame.height, map),
    );
    Ok(LightSuggestionDto {
        light: suggestion.light,
        faces: suggestion.faces,
        contrast: suggestion.contrast,
        reasons: suggestion.reasons,
    })
}
