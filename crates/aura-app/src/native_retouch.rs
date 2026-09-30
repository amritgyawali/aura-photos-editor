//! Native retouch commands; typed recipe extension, original-preserving history. ADR-0068.
use crate::{
    commands::IpcResult,
    contract::ipc::{RenderDto, RenderNoteDto},
    AppState,
};
use aura_core::{PhotoId, ProjectId};
pub use aura_recipe::retouch_tools::Edit;
use aura_recipe::{retouch_tools, schema, EditSource};
use aura_render::RenderService;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetouchInput {
    pub project_id: String,
    pub photo_id: String,
    pub action: String,
    #[serde(default)]
    pub edits: Vec<Edit>,
    pub id: Option<String>,
}

fn invalid(message: &str) -> aura_core::AuraError {
    let mut e = aura_core::errors::render::recipe_invalid("native retouch", message);
    e.user_message = message.to_owned();
    e
}

/// Read or modify the active photograph's local operation stack.
/// # Errors
/// Invalid membership, parameters, missing operations or failed history storage.
pub fn edit(state: &AppState, input: &RetouchInput) -> IpcResult<Vec<Edit>> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo"))?;
    let project =
        ProjectId::from_db(&input.project_id).map_err(|_| invalid("Invalid collection"))?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    let mut edits = retouch_tools::read(&base)?;
    match input.action.as_str() {
        "list" => return Ok(edits),
        "append" => {
            if input.edits.is_empty() {
                return Err(invalid("Choose a retouch operation").into());
            }
            for mut e in input.edits.clone() {
                e.id = uuid::Uuid::new_v4().to_string();
                edits.push(e);
            }
        }
        "update" => {
            if input.edits.len() != 1 {
                return Err(invalid("Choose one operation to update").into());
            }
            let change = &input.edits[0];
            let saved = edits
                .iter_mut()
                .find(|e| e.id == change.id)
                .ok_or_else(|| invalid("Retouch operation no longer exists"))?;
            *saved = change.clone();
        }
        "remove" => {
            let id = input
                .id
                .as_deref()
                .ok_or_else(|| invalid("Choose an operation to remove"))?;
            let n = edits.len();
            edits.retain(|e| e.id != id);
            if edits.len() == n {
                return Err(invalid("Retouch operation no longer exists").into());
            }
        }
        "clear" => edits.clear(),
        _ => return Err(invalid("Unknown retouch action").into()),
    }
    retouch_tools::validate(&edits)?;
    let mut proposal = base.clone();
    retouch_tools::write(&mut proposal, &edits)?;
    let (merged, changes) = schema::merge(&base, &proposal, EditSource::User)?;
    schema::Validation::check(&merged)?;
    state.recipe_store().save(
        &project,
        &photo,
        &merged,
        &changes.changed,
        "Native retouch",
    )?;
    Ok(edits)
}

/// Full-frame editing preview. Final crop/perspective is applied in Develop and export.
/// # Errors
/// Missing photograph, invalid recipe or failed rendering.
pub fn preview(state: &AppState, project: &str, photo: &str, before: bool) -> IpcResult<RenderDto> {
    crate::studio_tools::require_member(state, project, photo)?;
    let image_id = PhotoId::from_db(photo).map_err(|_| invalid("Invalid photo"))?;
    let mut recipe = crate::develop_commands::load_or_neutral(state, image_id)?;
    recipe.geometry = aura_recipe::Geometry::default();
    // Post-crop decoration is reviewed in Develop, not baked into a full-frame retouch view.
    recipe.global.effects = aura_recipe::Effects::default();
    if before {
        recipe.extra.remove(retouch_tools::KEY);
    }
    let result = state.render()?.render(aura_render::RenderRequest {
        image_id,
        recipe,
        level: aura_render::RenderLevel::Screen(1600, 1200),
        purpose: aura_render::RenderPurpose::Interactive,
        output: aura_render::OutputSpec {
            colour_space: aura_render::OutputColour::Srgb,
            bit_depth: 8,
            icc: None,
        },
    })?;
    let bytes = match &result.data {
        aura_render::RenderedData::Eight(v) => v.clone(),
        aura_render::RenderedData::Sixteen(v) => v.iter().map(|x| (x >> 8) as u8).collect(),
    };
    Ok(RenderDto {
        width: result.width,
        height: result.height,
        rgb_base64: crate::develop_commands::base64(&bytes),
        colour_space: result.colour_space.as_str().into(),
        icc: aura_render::output::icc_name(result.colour_space).into(),
        render_hash: result.render_hash,
        backend: result.backend,
        stages_run: result.stages_run,
        notes: result
            .notes
            .iter()
            .map(|n| RenderNoteDto {
                stage: n.stage.clone(),
                reason: n.reason.as_str().into(),
                detail: n.detail.clone(),
                is_caveat: n.reason.is_a_caveat(),
            })
            .collect(),
        ms: result.ms,
    })
}
