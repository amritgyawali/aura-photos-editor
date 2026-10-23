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
    let previous = studio_finish::read(&base)?;
    let mut finish = input.finish.clone();
    // A tool that tells the people from their background is given the learned selection the
    // first time one is used, frozen into the recipe (ADR-0150). Kept once made.
    if finish.needs_subject() && finish.subject_matte.is_none() {
        finish.subject_matte = previous.subject_matte.clone().or_else(|| {
            crate::local_mask_commands::subject_matte(state, &input.project_id, photo)
                .ok()
                .flatten()
                .map(|(m, _)| m)
        });
    }
    if input.finish.background_repair > 0.0
        && input.finish.background_repair.to_bits() != previous.background_repair.to_bits()
    {
        let frame = crate::photo_frames::CatalogFrames::new(state.clone())
            .frame(&photo, RenderLevel::Proxy2048)?;
        let map = aura_render::portrait::parse(&frame, &aura_render::portrait::hints(&base));
        let available = map.as_deref().is_some_and(|map| {
            let mask = aura_render::portrait::resolve(
                map,
                aura_portrait::Region::Background,
                frame.width,
                frame.height,
            );
            mask.iter().any(|v| *v < 0.2) && mask.iter().any(|v| *v > 0.8)
        });
        if !available {
            return Err(invalid("Background repair needs a detected subject and background. Add a missed face in Portrait regions, or use the Liquify Restore brush.").into());
        }
    }
    let mut proposal = base.clone();
    studio_finish::write(&mut proposal, &finish)?;
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

/// The white background an ID photograph needs, written into `proposal` over what `base`
/// carries: the photographer's other finishing is kept, and the learned person outline is
/// measured once and frozen into the recipe. Only the ID framing presets call it, on a
/// photographer's press. Returns what separated the person when it was measured now. ADR-0150.
pub(crate) fn white_id_background(
    state: &AppState,
    project_id: &str,
    photo: PhotoId,
    base: &aura_recipe::Recipe,
    proposal: &mut aura_recipe::Recipe,
) -> IpcResult<Option<&'static str>> {
    let mut finish = studio_finish::read(base)?;
    finish.background = Some(studio_finish::Background {
        mode: studio_finish::BackgroundMode::Colour,
        colour: [1.0, 1.0, 1.0],
        colour2: [1.0, 1.0, 1.0],
        amount: 100.0,
        feather: 25.0,
    });
    let mut measured = None;
    if finish.subject_matte.is_none() {
        if let Some((matte, source)) =
            crate::local_mask_commands::subject_matte(state, project_id, photo)?
        {
            finish.subject_matte = Some(matte);
            measured = Some(source);
        }
    }
    studio_finish::write(proposal, &finish)?;
    Ok(measured)
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
