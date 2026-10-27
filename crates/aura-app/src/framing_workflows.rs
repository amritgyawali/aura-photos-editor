//! Read-only framing evidence and a temporary colour-only view. No recipe is saved here.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use crate::{
    commands::IpcResult,
    contract::ipc::{RenderDto, RenderNoteDto},
    AppState,
};
use aura_core::{AuraError, PhotoId};
use aura_render::{FrameSource, OutputColour, RenderLevel};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub project_id: String,
    pub photo_id: String,
    pub recipe_hash: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Face {
    pub bounds: [f32; 4],
    pub eyes: [[f32; 2]; 2],
    pub eyes_measured: u8,
    pub confidence: f32,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub width: u32,
    pub height: u32,
    pub recipe_hash: String,
    pub faces: Vec<Face>,
    pub subject_bounds: Option<[f32; 4]>,
    pub subject_axis: Option<f32>,
    pub reasons: Vec<String>,
}
fn invalid(message: &str) -> AuraError {
    let mut error = aura_core::errors::render::recipe_invalid("framing", message);
    message.clone_into(&mut error.user_message);
    error
}
fn checked(state: &AppState, input: &Input) -> Result<(PhotoId, aura_recipe::Recipe), AuraError> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo."))?;
    let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
    if aura_recipe::hash::recipe_hash(&recipe)? != input.recipe_hash {
        return Err(invalid(
            "The photo changed. Reload before measuring framing.",
        ));
    }
    Ok((photo, recipe))
}

/// Measure source dimensions, face landmarks and the subject silhouette for explicit framing.
/// # Errors
/// Invalid collection/photo/hash, missing original, or unavailable decoded pixels.
pub fn context(state: &AppState, input: &Input) -> IpcResult<Context> {
    let (photo, recipe) = checked(state, input)?;
    let frame = crate::photo_frames::CatalogFrames::new(state.clone())
        .frame(&photo, RenderLevel::Proxy2048)?;
    let parse = aura_render::portrait::parse(&frame, &aura_render::portrait::hints(&recipe));
    let faces = parse.as_ref().map_or_else(Vec::new, |map| {
        map.faces
            .iter()
            .map(|face| {
                let sx = map.width as f32;
                let sy = map.height as f32;
                Face {
                    bounds: [
                        (face.center[0] - face.size / 2.0) / sx,
                        (face.center[1] - face.size / 2.0) / sy,
                        (face.center[0] + face.size / 2.0) / sx,
                        (face.center[1] + face.size / 2.0) / sy,
                    ]
                    .map(|v| v.clamp(0.0, 1.0)),
                    eyes: [face.left_eye, face.right_eye].map(|p| [p[0] / sx, p[1] / sy]),
                    eyes_measured: face.eyes_measured,
                    confidence: face.confidence,
                }
            })
            .collect()
    });
    let matrix = aura_render::output::working_to_output(OutputColour::Srgb);
    let bytes: Vec<u8> = frame
        .rgb
        .chunks_exact(3)
        .flat_map(|pixel| {
            let rgb = [
                pixel.first().copied().unwrap_or(0.0),
                pixel.get(1).copied().unwrap_or(0.0),
                pixel.get(2).copied().unwrap_or(0.0),
            ];
            aura_render::output::map_one(rgb, matrix, OutputColour::Srgb)
                .map(|v| (v * 255.0).round() as u8)
        })
        .collect();
    let mut reasons = vec![
        "Framing changes are proposed from source coordinates. Save only the framing you choose."
            .into(),
    ];
    let (subject_bounds, subject_axis) =
        match aura_vision::ai::subject(&bytes, frame.width, frame.height) {
            Ok(matte) => silhouette(&matte, frame.width as f32 / frame.height as f32),
            Err(reason) => {
                reasons.push(format!("Subject framing unavailable: {reason}"));
                (None, None)
            }
        };
    if faces.is_empty() {
        reasons.push("No face found. Use free crop or draw a face in Portrait retouch before measuring again.".into());
    }
    // Full source dimensions are needed for exact pixel crops; proxy dimensions must never
    // be presented as original dimensions. Decode the full source only on this explicit action.
    let full =
        crate::photo_frames::CatalogFrames::new(state.clone()).frame(&photo, RenderLevel::Full)?;
    checked(state, input)?;
    Ok(Context {
        width: full.width,
        height: full.height,
        recipe_hash: input.recipe_hash.clone(),
        faces,
        subject_bounds,
        subject_axis,
        reasons,
    })
}

fn silhouette(matte: &aura_vision::skin::Matte, aspect: f32) -> (Option<[f32; 4]>, Option<f32>) {
    let mut samples = Vec::new();
    let mut bounds = [1.0_f32, 1.0, 0.0, 0.0];
    for (i, alpha) in matte.alpha.iter().enumerate() {
        if *alpha < 128 {
            continue;
        }
        let x = matte.bounds[0]
            + (matte.bounds[2] - matte.bounds[0]) * (i % matte.width) as f32 / matte.width as f32;
        let y = matte.bounds[1]
            + (matte.bounds[3] - matte.bounds[1]) * (i / matte.width) as f32 / matte.height as f32;
        bounds[0] = bounds[0].min(x);
        bounds[1] = bounds[1].min(y);
        bounds[2] = bounds[2].max(x + 1.0 / matte.width as f32);
        bounds[3] = bounds[3].max(y + 1.0 / matte.height as f32);
        samples.push([x * aspect, y]);
    }
    if samples.len() < 32 {
        return (None, None);
    }
    let n = samples.len() as f32;
    let mean = samples
        .iter()
        .fold([0.0; 2], |s, p| [s[0] + p[0] / n, s[1] + p[1] / n]);
    let covariance = samples.iter().fold([0.0; 3], |s, p| {
        let x = p[0] - mean[0];
        let y = p[1] - mean[1];
        [s[0] + x * x / n, s[1] + y * y / n, s[2] + x * y / n]
    });
    let [xx, yy, xy] = covariance;
    let anisotropy = ((xx - yy).powi(2) + 4.0 * xy * xy).sqrt() / (xx + yy).max(1e-6);
    // A nearly round/flat subject has no stable long axis; do not invent one.
    let axis =
        (anisotropy > 0.2 && yy > xx).then(|| (0.5 * (2.0 * xy).atan2(xx - yy)).to_degrees());
    (Some(bounds.map(|v| v.clamp(0.0, 1.0))), axis)
}

/// Preview colour adjustments while omitting retouch, restoration, geometry warps and cleanup.
/// Geometry crop/rotation is retained so the compared composition stays the same.
/// # Errors
/// Stale/missing source or rendering failure. This never writes the saved recipe.
pub fn color_only(state: &AppState, input: &Input) -> IpcResult<RenderDto> {
    let (photo, mut recipe) = checked(state, input)?;
    recipe.retouch.clear();
    recipe.cleanup.clear();
    recipe.restoration = aura_recipe::Restoration::default();
    recipe.global.sharpen = aura_recipe::Sharpen::default();
    recipe.global.noise = aura_recipe::Noise::default();
    recipe.global.effects = aura_recipe::Effects::default();
    recipe.global.texture = 0;
    recipe.global.clarity = 0;
    for mask in &mut recipe.masks {
        mask.params.clarity = None;
        mask.params.texture = None;
    }
    for key in [
        aura_recipe::retouch_tools::KEY,
        aura_recipe::retouch_tools::MATTE_KEY,
        aura_recipe::studio_finish::KEY,
        aura_recipe::material_lab::KEY,
        aura_recipe::spatial_lab::KEY,
    ] {
        recipe.extra.remove(key);
    }
    let mattes = aura_recipe::local_masks::read_mattes(&recipe)?;
    let mut masks = aura_recipe::local_masks::read(&recipe)?;
    for mask in &mut masks {
        mask.params.clarity = None;
        mask.params.texture = None;
    }
    if !masks.is_empty() {
        aura_recipe::local_masks::write(&mut recipe, &masks, &mattes)?;
    }
    let mut color = aura_recipe::studio_color::read(&recipe)?;
    for local in color.local.values_mut() {
        local.sharpen = 0.0;
    }
    aura_recipe::studio_color::write(&mut recipe, &color)?;
    let result = crate::preview_render::render(
        state,
        photo,
        recipe,
        RenderLevel::Full,
        OutputColour::Srgb,
        false,
    )?;
    checked(state, input)?;
    let bytes = match &result.data {
        aura_render::RenderedData::Eight(bytes) => bytes.clone(),
        aura_render::RenderedData::Sixteen(words) => words.iter().map(|v| (v >> 8) as u8).collect(),
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
        ms: result.ms,
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
    })
}

#[cfg(test)]
mod tests;
