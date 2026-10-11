//! The Studio's finishing tools: face and body shape, liquify, background replacement, and
//! feature colour and makeup. ADR-0108.
//!
//! Two commands, one to read what a photograph carries and one to store the panel's state as a
//! single undoable manual edit. Nothing here runs automatically and no automatic pass writes this
//! extension, so a reshape only ever exists because a photographer made it.
use aura_core::{AuraError, PhotoId, ProjectId};
use aura_recipe::studio_finish::{self, StudioFinish};
use aura_recipe::{schema, EditSource};
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
