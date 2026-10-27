//! Explicit, stale-write-protected colour authoring. Measurements propose drafts only.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::indexing_slicing
)]
use crate::{commands::IpcResult, contract::ipc::RecipeDto, AppState};
use aura_core::{AuraError, PhotoId, ProjectId};
use aura_recipe::{
    schema,
    studio_color::{self, StudioColor},
    EditSource,
};
use aura_render::{CpuEngine, FrameSource, RenderLevel, RenderPurpose};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
fn invalid(message: &str) -> AuraError {
    let mut e = aura_core::errors::render::recipe_invalid("studio color", message);
    message.clone_into(&mut e.user_message);
    e
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveStudioColorInput {
    pub project_id: String,
    pub photo_id: String,
    pub recipe_hash: String,
    pub color: StudioColor,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioColorDto {
    pub color: StudioColor,
    pub recipe: RecipeDto,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeasureStudioColorInput {
    pub project_id: String,
    pub photo_id: String,
    pub recipe_hash: String,
    pub kind: String,
    pub channel: String,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioColorMeasurement {
    pub color: StudioColor,
    pub recipe_hash: String,
    pub description: String,
    pub hsl_band: Option<String>,
}
fn base(
    state: &AppState,
    project: &str,
    photo: &str,
    expected: &str,
) -> IpcResult<(ProjectId, PhotoId, aura_recipe::Recipe)> {
    crate::studio_tools::require_member(state, project, photo)?;
    let p = ProjectId::from_db(project).map_err(|_| invalid("Invalid collection"))?;
    let id = PhotoId::from_db(photo).map_err(|_| invalid("Invalid photo"))?;
    let recipe = crate::develop_commands::load_or_neutral(state, id)?;
    if aura_recipe::recipe_hash(&recipe)? != expected {
        return Err(invalid("Photo changed. Reload before editing colour.").into());
    }
    Ok((p, id, recipe))
}
/// Save all explicit colour controls in one undoable history step.
/// # Errors
/// Refuses invalid controls, membership, stale recipes or failed atomic storage.
pub fn save(state: &AppState, input: &SaveStudioColorInput) -> IpcResult<StudioColorDto> {
    let (project, photo, base) = base(
        state,
        &input.project_id,
        &input.photo_id,
        &input.recipe_hash,
    )?;
    studio_color::validate(&input.color)?;
    // Canonical JSON rounds f32 controls. Compare decoded settings before the
    // leaf merge so reapplying them cannot manufacture another Undo entry.
    if studio_color::read(&base)? == input.color {
        return Ok(StudioColorDto {
            color: studio_color::read(&base)?,
            recipe: crate::develop_commands::recipe_dto(&input.photo_id, &base),
        });
    }
    let mut proposal = base.clone();
    studio_color::write(&mut proposal, &input.color)?;
    if aura_recipe::recipe_hash(&proposal)? == input.recipe_hash {
        return Ok(StudioColorDto {
            color: studio_color::read(&base)?,
            recipe: crate::develop_commands::recipe_dto(&input.photo_id, &base),
        });
    }
    // A complete panel snapshot replaces its map. Generic leaf merge retains removed keys.
    let (mut merged, mut changes) = schema::merge(&base, &proposal, EditSource::User)?;
    if base.extra.get(studio_color::KEY) != proposal.extra.get(studio_color::KEY) {
        if let Some(value) = proposal.extra.get(studio_color::KEY) {
            merged.extra.insert(studio_color::KEY.into(), value.clone());
        }
        changes.changed.push(studio_color::KEY.into());
    }
    schema::Validation::check(&merged)?;
    if !changes.changed.is_empty() {
        state.recipe_store().save_if_current(
            &project,
            &photo,
            &merged,
            &changes.changed,
            "Levels and selective colour",
            &input.recipe_hash,
        )?;
    }
    Ok(StudioColorDto {
        color: studio_color::read(&merged)?,
        recipe: crate::develop_commands::recipe_dto(&input.photo_id, &merged),
    })
}
fn sample(rgb: &[f32], width: usize, height: usize, x: f32, y: f32) -> [f32; 3] {
    let px = (x * width as f32).floor().min((width - 1) as f32) as usize;
    let py = (y * height as f32).floor().min((height - 1) as f32) as usize;
    let mut sum = [0.0; 3];
    let mut count = 0;
    for row in py.saturating_sub(3)..=(py + 3).min(height - 1) {
        for col in px.saturating_sub(3)..=(px + 3).min(width - 1) {
            let offset = (row * width + col) * 3;
            for c in 0..3 {
                sum[c] += rgb[offset + c];
            }
            count += 1;
        }
    }
    sum.map(|v| v / count as f32)
}
fn percentile(values: &mut [f32], q: f32) -> f32 {
    values.sort_by(f32::total_cmp);
    values[((values.len() - 1) as f32 * q).round() as usize]
}
fn validate_measurement(input: &MeasureStudioColorInput) -> Result<(), AuraError> {
    if !["auto", "black", "white", "hsl"].contains(&input.kind.as_str())
        || !["rgb", "red", "green", "blue"].contains(&input.channel.as_str())
        || !input.x.is_finite()
        || !input.y.is_finite()
        || !(0.0..=1.0).contains(&input.x)
        || !(0.0..=1.0).contains(&input.y)
    {
        return Err(invalid("Choose a valid point and Levels channel"));
    }
    Ok(())
}
/// Propose measured 0.5/99.5 percentile Levels or a sampled black/white point.
/// Coordinates address the uncropped, oriented edited image. No measurement writes history.
/// # Errors
/// Invalid coordinates, stale recipe, unknown channel, missing pixels or collapsed range.
pub fn measure(
    state: &AppState,
    input: &MeasureStudioColorInput,
) -> IpcResult<StudioColorMeasurement> {
    let (_, photo, original) = base(
        state,
        &input.project_id,
        &input.photo_id,
        &input.recipe_hash,
    )?;
    validate_measurement(input)?;
    let source = Arc::new(crate::photo_frames::CatalogFrames::new(state.clone()));
    let frame = source.frame(&photo, RenderLevel::Proxy2048)?;
    let mut prepared = original.clone();
    prepared.extra.remove(studio_color::KEY);
    prepared.geometry = aura_recipe::Geometry::default();
    prepared.global.sharpen.amount = 0;
    prepared.global.effects = aura_recipe::Effects::default();
    let engine = CpuEngine::new(source, Arc::clone(state.clock()));
    let plan = aura_render::graph::plan(
        &prepared,
        RenderPurpose::Export,
        frame.kind,
        aura_render::Capabilities {
            mask_generators: true,
            retouch_operators: true,
            geometry_models: true,
            ..Default::default()
        },
    );
    let (mut rgb, w, h, _) =
        engine.working_buffer(&frame, &prepared, &plan, RenderLevel::Proxy2048, None);
    if w == 0 || h == 0 || rgb.len() != w as usize * h as usize * 3 {
        return Err(invalid("Photo has no usable pixels").into());
    }
    let scalar = |p: &[f32]| {
        aura_render::tonemap::curve_domain_encode(match input.channel.as_str() {
            "red" => p[0],
            "green" => p[1],
            "blue" => p[2],
            _ => p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593,
        })
        .clamp(0.0, 1.0)
    };
    let mut color = studio_color::read(&original)?;
    if input.kind == "hsl" {
        for pixel in rgb.chunks_exact_mut(3) {
            let adjusted = aura_render::studio_color::selective(
                aura_render::studio_color::levels([pixel[0], pixel[1], pixel[2]], &color.levels),
                &color.selective,
            );
            pixel.copy_from_slice(&adjusted);
        }
        let point = sample(&rgb, w as usize, h as usize, input.x, input.y);
        let band = dominant_hsl_band(point)?;
        base(
            state,
            &input.project_id,
            &input.photo_id,
            &input.recipe_hash,
        )?;
        return Ok(StudioColorMeasurement {
            color,
            recipe_hash: input.recipe_hash.clone(),
            description: format!("Selected {band} from the edited sample before local colour"),
            hsl_band: Some(band.to_owned()),
        });
    }
    let channel = match input.channel.as_str() {
        "red" => &mut color.levels.red,
        "green" => &mut color.levels.green,
        "blue" => &mut color.levels.blue,
        _ => &mut color.levels.rgb,
    };
    if input.kind == "auto" {
        let mut values: Vec<f32> = rgb.chunks_exact(3).map(scalar).collect();
        channel.input_black = percentile(&mut values, 0.005);
        channel.input_white = percentile(&mut values, 0.995);
    } else {
        let point = sample(&rgb, w as usize, h as usize, input.x, input.y);
        let value = scalar(&point);
        if input.kind == "black" {
            channel.input_black = value;
        } else {
            channel.input_white = value;
        }
    }
    studio_color::validate(&color)?;
    // Reject measurements that finished after another editor changed the photo.
    base(
        state,
        &input.project_id,
        &input.photo_id,
        &input.recipe_hash,
    )?;
    Ok(StudioColorMeasurement {
        color,
        recipe_hash: input.recipe_hash.clone(),
        description: if input.kind == "auto" {
            "Measured 0.5% and 99.5% percentiles before Levels on the uncropped edited proxy".into()
        } else {
            "Sampled a 7 × 7 patch before Levels on the uncropped edited proxy".into()
        },
        hsl_band: None,
    })
}

fn dominant_hsl_band(rgb: [f32; 3]) -> Result<&'static str, AuraError> {
    let (h, s, v) = aura_render::tonemap::to_hsv(rgb);
    if s < 0.03 || v < 0.001 {
        return Err(invalid(
            "Choose a coloured sample; this patch has no reliable hue",
        ));
    }
    let weights = aura_render::tonemap::band_weights(h);
    let index = weights
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(i, _)| i)
        .ok_or_else(|| invalid("No hue band available"))?;
    aura_recipe::HSL_BANDS
        .get(index)
        .copied()
        .ok_or_else(|| invalid("No hue band available"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hsl_selector_finds_coloured_band_and_refuses_grey() {
        assert_eq!(dominant_hsl_band([0.6, 0.05, 0.05]).unwrap(), "red");
        assert_eq!(dominant_hsl_band([0.01, 0.05, 0.7]).unwrap(), "blue");
        assert!(dominant_hsl_band([0.2, 0.2, 0.2]).is_err());
    }
    #[test]
    fn percentile_rejects_outliers_by_rank() {
        let mut values = (0..1000).map(|v| v as f32 / 999.0).collect::<Vec<_>>();
        assert!(percentile(&mut values, 0.005) > 0.0);
        assert!(percentile(&mut values, 0.995) < 1.0);
    }
    #[test]
    fn patch_uses_correct_coordinates_and_clamps_edges() {
        let rgb = vec![0.1; 8 * 8 * 3];
        assert_eq!(
            sample(&rgb, 8, 8, 1.0, 1.0).map(|v| (v * 100.0).round() as i32),
            [10, 10, 10]
        );
    }
}
