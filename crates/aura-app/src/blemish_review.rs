//! Native remaining-mark review. Uses the saved retouch view and never saves an edit.
use crate::{commands::IpcResult, AppState};
use aura_core::PhotoId;
use aura_raw::colour::{
    curve::srgb_decode,
    matrix::{mul, SRGB_TO_XYZ_D65},
};
use aura_recipe::retouch_tools::{self, Edit};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub project_id: String,
    pub photo_id: String,
    pub recipe_hash: String,
    pub selection: Edit,
    #[serde(default)]
    pub quality: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub recipe_hash: String,
    pub width: u32,
    pub height: u32,
    pub review: aura_render::blemish_review::Review,
}

fn invalid(message: &str) -> aura_core::AuraError {
    let mut error = aura_recipe::errors::recipe_invalid("blemish review", message);
    message.clone_into(&mut error.user_message);
    error
}

/// Find possible residual mark areas inside the chosen selection of the saved photo.
/// # Errors
/// Invalid membership/selection, stale recipe, missing matte or failed rendering.
pub fn inspect(state: &AppState, input: &Input) -> IpcResult<Report> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo"))?;
    let mut recipe = crate::develop_commands::load_or_neutral(state, photo)?;
    let hash = aura_recipe::recipe_hash(&recipe)?;
    if hash != input.recipe_hash {
        return Err(invalid("The saved photo changed. Check remaining marks again.").into());
    }
    let mattes = retouch_tools::read_mattes(&recipe)?;
    if input
        .quality
        .as_deref()
        .is_some_and(|v| !matches!(v, "full" | "fast"))
    {
        return Err(invalid("Choose full resolution or a quick check.").into());
    }
    let probe = aura_render::blemish_review::probe(&input.selection);
    retouch_tools::validate(std::slice::from_ref(&probe))?;
    if probe
        .matte
        .as_ref()
        .is_some_and(|id| !mattes.contains_key(id))
    {
        return Err(invalid("The selected skin mask is no longer available.").into());
    }
    // Retouch coordinates are the uncropped original-frame coordinates, just as in
    // native_retouch::render_preview. This review never copies crop coordinates.
    recipe.geometry = aura_recipe::Geometry::default();
    recipe.global.effects = aura_recipe::Effects::default();
    let frame = crate::preview_render::render(
        state,
        photo,
        recipe,
        crate::preview_render::Quality::parse(input.quality.as_deref()).level(),
        aura_render::OutputColour::Srgb,
        false,
    )?;
    let matrix = aura_render::colour::narrow(mul(
        aura_raw::colour::working_space::xyz_d65_to_rec2020(),
        SRGB_TO_XYZ_D65,
    ));
    let encoded = match frame.data {
        aura_render::RenderedData::Eight(bytes) => bytes,
        aura_render::RenderedData::Sixteen(values) => {
            values.iter().map(|v| (v >> 8) as u8).collect()
        }
    };
    let working: Vec<f32> = encoded
        .chunks_exact(3)
        .flat_map(|pixel| {
            let mut rgb = [0.0; 3];
            for (out, value) in rgb.iter_mut().zip(pixel) {
                *out = srgb_decode(f32::from(*value) / 255.0);
            }
            aura_render::colour::apply_f32(matrix, rgb)
        })
        .collect();
    let review = aura_render::blemish_review::measure(
        &working,
        frame.width as usize,
        frame.height as usize,
        &probe,
        &mattes,
    );
    if aura_recipe::recipe_hash(&crate::develop_commands::load_or_neutral(state, photo)?)? != hash {
        return Err(invalid("The saved photo changed during the check. Check again.").into());
    }
    Ok(Report {
        recipe_hash: hash,
        width: frame.width,
        height: frame.height,
        review,
    })
}
