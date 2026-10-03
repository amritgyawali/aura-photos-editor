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
use serde::{Deserialize, Serialize};

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
    message.clone_into(&mut e.user_message);
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
    let mut mattes = retouch_tools::read_mattes(&base)?;
    match input.action.as_str() {
        "list" => return Ok(edits),
        "append" => {
            if input.edits.is_empty() {
                return Err(invalid("Choose a retouch operation").into());
            }
            for mut e in input.edits.clone() {
                e.id = uuid::Uuid::new_v4().to_string();
                preserve_manual_matte(&mut e, &mut mattes)?;
                edits.push(e);
            }
        }
        "update" => {
            if input.edits.len() != 1 {
                return Err(invalid("Choose one operation to update").into());
            }
            let change = input
                .edits
                .first()
                .ok_or_else(|| invalid("Choose one operation to update"))?;
            let saved = edits
                .iter_mut()
                .find(|e| e.id == change.id)
                .ok_or_else(|| invalid("Retouch operation no longer exists"))?;
            *saved = change.clone();
            if crate::portrait_auto::group_of(&saved.id).is_some() {
                saved.id = format!("manual-{}", saved.id);
            }
            preserve_manual_matte(saved, &mut mattes)?;
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
        "duplicate" | "earlier" | "later" => {
            let index = edits
                .iter()
                .position(|e| Some(e.id.as_str()) == input.id.as_deref())
                .ok_or_else(|| invalid("Retouch operation no longer exists"))?;
            match input.action.as_str() {
                "duplicate" => {
                    let mut copy = edits
                        .get(index)
                        .ok_or_else(|| invalid("Retouch operation no longer exists"))?
                        .clone();
                    copy.id = uuid::Uuid::new_v4().to_string();
                    preserve_manual_matte(&mut copy, &mut mattes)?;
                    edits.insert(index + 1, copy);
                }
                "earlier" if index > 0 => edits.swap(index, index - 1),
                "later" if index + 1 < edits.len() => edits.swap(index, index + 1),
                _ => return Ok(edits),
            }
        }
        "clear" => edits.clear(),
        _ => return Err(invalid("Unknown retouch action").into()),
    }
    retouch_tools::validate(&edits)?;
    let mut proposal = base.clone();
    retouch_tools::write_with_mattes(&mut proposal, &edits, &mattes)?;
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

// An automatic pass reuses its mask IDs. Snapshot a manually chosen mask so a later
// detection cannot silently move an existing manual edit to another person's skin.
fn preserve_manual_matte(
    edit: &mut Edit,
    mattes: &mut std::collections::BTreeMap<String, retouch_tools::Matte>,
) -> aura_core::AuraResult<()> {
    let Some(id) = edit.matte.as_ref() else {
        return Ok(());
    };
    let matte = mattes
        .get(id)
        .ok_or_else(|| invalid("The skin selection is missing. Detect skin again."))?;
    if id.starts_with(crate::portrait_auto::PREFIX) {
        let snapshot = matte.clone();
        let bytes = serde_json::to_vec(&snapshot)
            .map_err(|_| invalid("Could not preserve the skin selection."))?;
        // Several manual tools can share one immutable snapshot without consuming the
        // recipe's matte budget once per tool.
        let id = format!("manual-{}", blake3::hash(&bytes).to_hex());
        mattes.insert(id.clone(), snapshot);
        edit.matte = Some(id);
    }
    Ok(())
}

/// Full-frame editing preview. Final crop/perspective is applied in Develop and export.
/// # Errors
/// Missing photograph, invalid recipe or failed rendering.
pub fn preview(state: &AppState, project: &str, photo: &str, before: bool) -> IpcResult<RenderDto> {
    crate::studio_tools::require_member(state, project, photo)?;
    let image_id = PhotoId::from_db(photo).map_err(|_| invalid("Invalid photo"))?;
    let mut recipe = crate::develop_commands::load_or_neutral(state, image_id)?;
    if before {
        recipe.extra.remove(retouch_tools::KEY);
    }
    render_preview(state, image_id, recipe)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DraftInput {
    pub project_id: String,
    pub photo_id: String,
    pub edit: Edit,
    pub replace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionPreview {
    pub width: u32,
    pub height: u32,
    pub rgb_base64: String,
}

/// Preview selection coverage without changing the recipe or history.
/// # Errors
/// Invalid collection membership, draft, replacement ID, or failed rendering.
pub fn selection_preview(state: &AppState, input: &DraftInput) -> IpcResult<SelectionPreview> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo"))?;
    let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
    let (rgb, width, height) = state.render()?.retouch_selection(
        &photo,
        &recipe,
        &input.edit,
        input.replace_id.as_deref(),
    )?;
    Ok(SelectionPreview {
        width,
        height,
        rgb_base64: crate::develop_commands::base64(&rgb),
    })
}

/// Render a proposed edit without storing a recipe or history entry.
/// # Errors
/// Invalid targets, missing selected operation, invalid draft or render failure.
pub fn draft_preview(state: &AppState, input: &DraftInput) -> IpcResult<RenderDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let image_id = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo"))?;
    let mut recipe = crate::develop_commands::load_or_neutral(state, image_id)?;
    let mut edits = retouch_tools::read(&recipe)?;
    let mut draft = input.edit.clone();
    if let Some(id) = &input.replace_id {
        let current = edits
            .iter_mut()
            .find(|e| &e.id == id)
            .ok_or_else(|| invalid("Retouch operation no longer exists"))?;
        draft.id.clone_from(id);
        *current = draft;
    } else {
        // Stable unused ID keeps repeated draft renders cacheable without colliding with saved IDs.
        let mut sequence = 0;
        draft.id = format!("unsaved-preview-{sequence}");
        while edits.iter().any(|e| e.id == draft.id) {
            sequence += 1;
            draft.id = format!("unsaved-preview-{sequence}");
        }
        edits.push(draft);
    }
    retouch_tools::write(&mut recipe, &edits)?;
    render_preview(state, image_id, recipe)
}

fn render_preview(
    state: &AppState,
    image_id: PhotoId,
    mut recipe: aura_recipe::Recipe,
) -> IpcResult<RenderDto> {
    recipe.geometry = aura_recipe::Geometry::default();
    // Post-crop decoration is reviewed in Develop, not baked into a full-frame retouch view.
    recipe.global.effects = aura_recipe::Effects::default();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_selection_survives_replacement_of_the_automatic_matte() {
        let id = "auto-portrait-v1-0-face";
        let original = retouch_tools::Matte::encode([0.1, 0.1, 0.8, 0.8], 2, 2, &[255; 4]);
        let mut mattes = std::collections::BTreeMap::from([(id.into(), original.clone())]);
        let mut edit: Edit = serde_json::from_value(serde_json::json!({
            "id": "manual-edit", "tool": "dodge", "enabled": true,
            "region": [0.5, 0.5, 1.0, 1.0], "source": null,
            "amount": 0.5, "feather": 0.0, "radius": 0.01,
            "texture": 1.0, "tone": 0.5, "warmth": 0.0, "tint": 0.0,
            "matte": id
        }))
        .unwrap();
        preserve_manual_matte(&mut edit, &mut mattes).unwrap();
        let saved = edit.matte.clone().unwrap();
        mattes.insert(
            id.into(),
            retouch_tools::Matte::encode([0.0, 0.0, 1.0, 1.0], 2, 2, &[0; 4]),
        );
        assert_eq!(mattes.get(&saved), Some(&original));
        preserve_manual_matte(&mut edit, &mut mattes).unwrap();
        assert_eq!(edit.matte.as_ref(), Some(&saved));
        mattes.remove(&saved);
        assert!(preserve_manual_matte(&mut edit, &mut mattes).is_err());
    }
}
