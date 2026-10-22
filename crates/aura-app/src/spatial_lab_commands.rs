//! One explicit spatial stack save with membership, revision, and mask checks.
use crate::{commands::IpcResult, contract::ipc::RecipeDto, AppState};
use aura_core::{PhotoId, ProjectId};
use aura_recipe::{
    schema,
    spatial_lab::{self, Operation},
    EditSource,
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub project_id: String,
    pub photo_id: String,
    pub recipe_hash: String,
    pub operations: Vec<Operation>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub recipe: RecipeDto,
}
fn invalid(message: &str) -> aura_core::AuraError {
    let mut error = aura_recipe::errors::recipe_invalid(spatial_lab::KEY, message);
    message.clone_into(&mut error.user_message);
    error
}
/// # Errors
/// Refuses invalid selections, stale revisions, foreign photos, or failed atomic history saves.
pub fn save(state: &AppState, input: &Input) -> IpcResult<Output> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    spatial_lab::validate(&input.operations)?;
    let project =
        ProjectId::from_db(&input.project_id).map_err(|_| invalid("Invalid collection"))?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo"))?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    if aura_recipe::recipe_hash(&base)? != input.recipe_hash {
        return Err(invalid("Photo changed. Reload spatial tools.").into());
    }
    let masks = aura_recipe::local_masks::read(&base)?;
    if input
        .operations
        .iter()
        .any(|op| !masks.iter().any(|m| m.id == op.mask_id))
    {
        return Err(invalid("Select an existing mask for every spatial operation.").into());
    }
    // Compare decoded controls: canonical storage and f32 re-encoding may use
    // different JSON number spellings for the same user settings.
    if spatial_lab::read(&base)? == input.operations {
        return Ok(Output {
            recipe: crate::develop_commands::recipe_dto(&input.photo_id, &base),
        });
    }
    let mut proposal = base.clone();
    spatial_lab::write(&mut proposal, &input.operations)?;
    if aura_recipe::recipe_hash(&proposal)? == input.recipe_hash {
        return Ok(Output {
            recipe: crate::develop_commands::recipe_dto(&input.photo_id, &base),
        });
    }
    // Extension arrays are atomic leaves in the central merge, which also
    // records their user protection and avoids duplicate changed paths.
    let (merged, changes) = schema::merge(&base, &proposal, EditSource::User)?;
    schema::Validation::check(&merged)?;
    if !changes.changed.is_empty() {
        state.recipe_store().save_if_current(
            &project,
            &photo,
            &merged,
            &changes.changed,
            "Spatial repair tools",
            &input.recipe_hash,
        )?;
    }
    Ok(Output {
        recipe: crate::develop_commands::recipe_dto(&input.photo_id, &merged),
    })
}
