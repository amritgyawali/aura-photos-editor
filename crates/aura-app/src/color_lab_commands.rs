//! Atomic revision-protected selected colour method authoring.
use crate::{commands::IpcResult, contract::ipc::RecipeDto, AppState};
use aura_core::{AuraError, PhotoId, ProjectId};
use aura_recipe::{
    color_lab::{self, Operation},
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
    let mut error = aura_core::errors::render::recipe_invalid(color_lab::KEY, message);
    message.clone_into(&mut error.user_message);
    error
}
/// Persist an explicit stack atomically with one history step.
/// # Errors
/// Refuses invalid controls, foreign photos, missing masks and stale revisions.
pub fn save(state: &AppState, input: &Input) -> IpcResult<Output> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    color_lab::validate(&input.operations)?;
    let project =
        ProjectId::from_db(&input.project_id).map_err(|_| invalid("Invalid collection"))?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo"))?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    if aura_recipe::recipe_hash(&base)? != input.recipe_hash {
        return Err(invalid("Photo changed. Reload before applying colour methods.").into());
    }
    let masks = aura_recipe::local_masks::read(&base)?;
    if input
        .operations
        .iter()
        .any(|op| !masks.iter().any(|mask| mask.id == op.mask_id))
    {
        return Err(invalid("Choose an existing mask for each colour method.").into());
    }
    // Stored canonical JSON rounds numeric values; compare decoded controls so
    // re-encoding the same f32 values cannot manufacture another history step.
    if color_lab::read(&base)? == input.operations {
        return Ok(Output {
            recipe: crate::develop_commands::recipe_dto(&input.photo_id, &base),
        });
    }
    let mut proposal = base.clone();
    color_lab::write(&mut proposal, &input.operations)?;
    if aura_recipe::recipe_hash(&proposal)? == input.recipe_hash {
        return Ok(Output {
            recipe: crate::develop_commands::recipe_dto(&input.photo_id, &base),
        });
    }
    // Extensions are flattened recipe leaves. The central merge owns changed
    // paths and user protection; manually appending the key duplicates history.
    let (merged, changes) = schema::merge(&base, &proposal, EditSource::User)?;
    schema::Validation::check(&merged)?;
    if !changes.changed.is_empty() {
        state.recipe_store().save_if_current(
            &project,
            &photo,
            &merged,
            &changes.changed,
            "Selected colour and tone methods",
            &input.recipe_hash,
        )?;
    }
    Ok(Output {
        recipe: crate::develop_commands::recipe_dto(&input.photo_id, &merged),
    })
}
