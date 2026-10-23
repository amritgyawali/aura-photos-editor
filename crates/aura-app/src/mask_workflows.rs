//! Explicit selection editing, using the native renderer's composed coverage.
//! Frozen selections occupy the entire uncropped frame, on a bounded 512-cell grid.
use crate::{commands::IpcResult, local_mask_commands::LocalMasksDto, AppState};
use aura_core::{progress::CancelToken, AuraError, AuraResult, PhotoId, ProjectId};
use aura_recipe::{
    local_masks::{self, Component, Mode, Source},
    schema, EditSource, Recipe,
};
use serde::Deserialize;

mod plane;
#[cfg(test)]
mod tests;

// Product limits keep authored selection processing bounded on the stored matte grid.
const MAX_RADIUS_CELLS: u32 = 32;
const MAX_EDGE_CONTRAST: u32 = 100;
const MAX_COMPONENT_AREA: u32 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Duplicate,
    Invert,
    Union,
    Intersection,
    Subtraction,
    Freeze,
    Feather,
    Expand,
    Contract,
    EdgeContrast,
    RemoveIslands,
    FillHoles,
}
impl Action {
    fn label(self) -> &'static str {
        match self {
            Self::Duplicate => "Duplicate mask",
            Self::Invert => "Invert whole selection",
            Self::Union => "Union mask selections",
            Self::Intersection => "Intersect mask selections",
            Self::Subtraction => "Subtract mask selection",
            Self::Freeze => "Freeze composed selection",
            Self::Feather => "Feather selection",
            Self::Expand => "Expand selection",
            Self::Contract => "Contract selection",
            Self::EdgeContrast => "Refine selection contrast",
            Self::RemoveIslands => "Remove selection islands",
            Self::FillHoles => "Fill selection holes",
        }
    }
    fn combines(self) -> bool {
        matches!(self, Self::Union | Self::Intersection | Self::Subtraction)
    }
    fn parameterized(self) -> bool {
        matches!(
            self,
            Self::Feather
                | Self::Expand
                | Self::Contract
                | Self::EdgeContrast
                | Self::RemoveIslands
                | Self::FillHoles
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub project_id: String,
    pub photo_id: String,
    pub mask_id: String,
    pub recipe_hash: String,
    pub cancel_id: String,
    pub action: Action,
    #[serde(default)]
    pub other_mask_id: Option<String>,
    #[serde(default)]
    pub value: u32,
}

fn invalid(message: &str) -> AuraError {
    let mut error = aura_core::errors::render::recipe_invalid("mask workflow", message);
    error.user_message = message.into();
    error
}
fn check(cancel: &CancelToken) -> AuraResult<()> {
    if cancel.is_cancelled() {
        Err(invalid("Selection editing stopped. No edit was saved."))
    } else {
        Ok(())
    }
}
fn validate(input: &Input) -> AuraResult<()> {
    let max = match input.action {
        Action::Feather | Action::Expand | Action::Contract => MAX_RADIUS_CELLS,
        Action::EdgeContrast => MAX_EDGE_CONTRAST,
        Action::RemoveIslands | Action::FillHoles => MAX_COMPONENT_AREA,
        _ => 0,
    };
    if input.value > max
        || (input.action.combines()
            && input
                .other_mask_id
                .as_deref()
                .is_none_or(|id| id == input.mask_id))
    {
        return Err(invalid(
            "Choose a different second mask and a value within the displayed range.",
        ));
    }
    Ok(())
}

/// Apply one explicit operation, preserving local sliders and unrelated manual edits.
/// # Errors
/// Invalid membership, stale recipe, selection failure, cancellation, or invalid bounds.
pub fn apply(state: &AppState, input: &Input) -> IpcResult<LocalMasksDto> {
    apply_with_cancel(state, input, CancelToken::new())
}

/// Use the token registered by the desktop adapter before queueing its worker.
/// An already-cancelled token remains cancelled when the worker starts.
/// # Errors
/// The same validation, rendering and stale-recipe errors as [`apply`].
pub fn apply_with_cancel(
    state: &AppState,
    input: &Input,
    cancel: CancelToken,
) -> IpcResult<LocalMasksDto> {
    let active = cancel.clone();
    state.register_job(&input.cancel_id, cancel);
    let result = (|| -> AuraResult<LocalMasksDto> {
        validate(input)?;
        uuid::Uuid::parse_str(&input.cancel_id)
            .map_err(|_| invalid("Invalid selection editing job."))?;
        check(&active)?;
        crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
        let project =
            ProjectId::from_db(&input.project_id).map_err(|_| invalid("Invalid collection."))?;
        let photo =
            PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photograph."))?;
        run(state, input, project, photo, &active)
    })();
    state.finish_job(&input.cancel_id);
    Ok(result?)
}
fn run(
    state: &AppState,
    input: &Input,
    project: ProjectId,
    photo: PhotoId,
    cancel: &CancelToken,
) -> AuraResult<LocalMasksDto> {
    check(cancel)?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    if aura_recipe::hash::recipe_hash(&base)? != input.recipe_hash {
        return Err(invalid(
            "The photograph changed. Reload the masks before editing its selection.",
        ));
    }
    let mut masks = local_masks::read(&base)?;
    let original = masks
        .iter()
        .find(|mask| mask.id == input.mask_id)
        .cloned()
        .ok_or_else(|| invalid("That mask no longer exists."))?;
    let mut changed = original.clone();
    let mut mattes = local_masks::read_mattes(&base)?;
    if input.action == Action::Duplicate {
        if masks.len() >= local_masks::MAX_MASKS {
            return Err(invalid(
                "This photograph already has the maximum number of masks.",
            ));
        }
        changed.id = format!("mask-{}", uuid::Uuid::new_v4().simple());
        changed.name = format!(
            "{} copy",
            original.name.chars().take(20).collect::<String>()
        );
        masks.push(changed.clone());
    } else if !input.action.parameterized() || input.value > 0 {
        let selection = measured(state, input, photo, &base, cancel)?;
        let matte = selection.matte();
        let encoded =
            serde_json::to_vec(&matte).map_err(|_| invalid("Cannot save edited selection."))?;
        let id = format!("selection-{}", blake3::hash(&encoded).to_hex());
        mattes.insert(id.clone(), matte);
        changed.components = vec![Component {
            mode: Mode::Add,
            invert: false,
            source: Source::Matte {
                matte: id,
                what: "edited selection".into(),
            },
        }];
        if let Some(mask) = masks.iter_mut().find(|mask| mask.id == input.mask_id) {
            *mask = changed.clone();
        }
    }
    let mut proposal = base.clone();
    local_masks::write(&mut proposal, &masks, &mattes)?;
    if input.action == Action::Duplicate {
        let mut color = aura_recipe::studio_color::read(&base)?;
        if let Some(local) = color.local.get(&input.mask_id).cloned() {
            color.local.insert(changed.id.clone(), local);
            aura_recipe::studio_color::write(&mut proposal, &color)?;
        }
    }
    let saved = save(state, input, (project, photo), &base, &proposal, cancel)?;
    Ok(LocalMasksDto {
        masks,
        recipe: crate::develop_commands::recipe_dto(&input.photo_id, &saved),
        mask_id: Some(changed.id),
        message: None,
    })
}

fn measured(
    state: &AppState,
    input: &Input,
    photo: PhotoId,
    base: &Recipe,
    cancel: &CancelToken,
) -> AuraResult<plane::Plane> {
    let render = state.render()?;
    check(cancel)?;
    let (rgb, width, height) = render.local_mask_coverage(&photo, base, &input.mask_id)?;
    check(cancel)?;
    let mut selection = plane::Plane::from_rgb(&rgb, width, height)?;
    let second = if input.action.combines() {
        let id = input
            .other_mask_id
            .as_deref()
            .ok_or_else(|| invalid("Choose a second mask."))?;
        let (rgb, width, height) = render.local_mask_coverage(&photo, base, id)?;
        check(cancel)?;
        Some(plane::Plane::from_rgb(&rgb, width, height)?)
    } else {
        None
    };
    selection.edit(input.action, input.value, second.as_ref(), cancel)?;
    Ok(selection)
}

fn save(
    state: &AppState,
    input: &Input,
    ids: (ProjectId, PhotoId),
    base: &Recipe,
    proposal: &Recipe,
    cancel: &CancelToken,
) -> AuraResult<Recipe> {
    let (project, photo) = ids;
    // Stored extension numbers use canonical precision. Re-encoding a matte may spell
    // 0 as 0.0 in its temporary JSON without changing the authored recipe at all.
    if aura_recipe::hash::recipe_hash(base)? == aura_recipe::hash::recipe_hash(proposal)? {
        check(cancel)?;
        return Ok(base.clone());
    }
    let (mut merged, mut changes) = schema::merge(base, proposal, EditSource::User)?;
    // Generic leaf merging cannot delete obsolete dynamic matte IDs. The explicit
    // selection edit owns just these two blocks, retaining every other manual field.
    for key in [local_masks::KEY, local_masks::MATTE_KEY] {
        if merged.extra.get(key) != proposal.extra.get(key) {
            if let Some(value) = proposal.extra.get(key) {
                merged.extra.insert(key.into(), value.clone());
            } else {
                merged.extra.remove(key);
            }
            if !changes.changed.iter().any(|path| path == key) {
                changes.changed.push(key.into());
            }
            if !merged
                .provenance
                .user_edited_fields
                .iter()
                .any(|path| path == key)
            {
                merged.provenance.user_edited_fields.push(key.into());
            }
        }
    }
    schema::Validation::check(&merged)?;
    check(cancel)?;
    if !changes.changed.is_empty() {
        state.recipe_store().save_if_current(
            &project,
            &photo,
            &merged,
            &changes.changed,
            input.action.label(),
            &input.recipe_hash,
        )?;
    }
    Ok(merged)
}
