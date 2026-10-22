//! Explicit material stack authoring with revision protection and atomic history.
use crate::{commands::IpcResult, contract::ipc::RecipeDto, AppState};
use aura_core::{AuraError, PhotoId, ProjectId};
use aura_recipe::{
    material_lab::{self, Operation},
    schema, EditSource,
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
fn invalid(message: &str) -> AuraError {
    let mut error = aura_core::errors::render::recipe_invalid(material_lab::KEY, message);
    message.clone_into(&mut error.user_message);
    error
}
/// Save a complete user-authored stack in one history step. Removing entries is explicit.
/// # Errors
/// Refuses invalid/missing masks, excessive operations, foreign photos and stale writes.
pub fn save(state: &AppState, input: &Input) -> IpcResult<Output> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    material_lab::validate(&input.operations)?;
    let project =
        ProjectId::from_db(&input.project_id).map_err(|_| invalid("Invalid collection"))?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo"))?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    if aura_recipe::recipe_hash(&base)? != input.recipe_hash {
        return Err(invalid("Photo changed. Reload before applying materials.").into());
    }
    let masks = aura_recipe::local_masks::read(&base)?;
    if input
        .operations
        .iter()
        .any(|op| !masks.iter().any(|m| m.id == op.mask_id))
    {
        return Err(invalid("Choose an existing mask for each material tool.").into());
    }
    if material_lab::read(&base)? == input.operations {
        return Ok(Output {
            recipe: crate::develop_commands::recipe_dto(&input.photo_id, &base),
        });
    }
    let mut proposal = base.clone();
    material_lab::write(&mut proposal, &input.operations)?;
    if aura_recipe::recipe_hash(&proposal)? == input.recipe_hash {
        return Ok(Output {
            recipe: crate::develop_commands::recipe_dto(&input.photo_id, &base),
        });
    }
    let (merged, changes) = schema::merge(&base, &proposal, EditSource::User)?;
    schema::Validation::check(&merged)?;
    if !changes.changed.is_empty() {
        state.recipe_store().save_if_current(
            &project,
            &photo,
            &merged,
            &changes.changed,
            "Material and texture tools",
            &input.recipe_hash,
        )?;
    }
    Ok(Output {
        recipe: crate::develop_commands::recipe_dto(&input.photo_id, &merged),
    })
}
