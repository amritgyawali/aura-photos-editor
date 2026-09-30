//! Explicit, local authoring tools. See ADR-0067.
use crate::{commands::IpcResult, contract::ipc::RecipeDto, AppState};
use aura_core::{AuraError, AuraResult, PhotoId, ProjectId};
use aura_recipe::{schema, EditSource};
use aura_render::{FrameSource, RenderLevel};

/// A neutral point in the uncropped, oriented original, in normalized coordinates.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhiteBalancePickInput {
    pub project_id: String,
    pub photo_id: String,
    pub x: f32,
    pub y: f32,
}

pub(crate) fn require_member(state: &AppState, project: &str, photo: &str) -> AuraResult<()> {
    let project = project.to_owned();
    let photo = photo.to_owned();
    state.catalog().read(move |conn| {
        let found: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM photo WHERE project_id = ?1 AND photo_id = ?2)",
                [&project, &photo],
                |row| row.get(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("photo collection", &e))?;
        if found {
            Ok(())
        } else {
            Err(invalid("Photo does not belong to this collection"))
        }
    })
}

fn invalid(message: &str) -> AuraError {
    let mut error = aura_core::errors::render::recipe_invalid("studio", message);
    error.user_message = message.to_owned();
    error
}

/// Find the renderer's temperature/tint that neutralizes a linear Rec.2020 patch.
/// The residual is minimized in log ratios, preserving the renderer's green/exposure anchor.
/// # Errors
/// Rejects non-finite, dark or clipped samples with no reliable neutral information.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub fn neutral_white_balance(rgb: [f32; 3]) -> AuraResult<(u32, i16)> {
    if rgb
        .iter()
        .any(|v| !v.is_finite() || *v <= 0.005 || *v >= 0.95)
    {
        return Err(invalid(
            "Choose a midtone neutral area, away from black or clipped highlights",
        ));
    }
    let [red, green, blue] = rgb;
    let score = |kelvin: u32| {
        let [r, _, b] = aura_render::colour::white_balance(kelvin as f32, 0.0);
        let q = (green * green / (red * r * blue * b)).sqrt();
        let tint = ((q - 1.0) * 300.0).round().clamp(-150.0, 150.0) as i16;
        let factor = 1.0 + f32::from(tint) / 300.0;
        let error =
            (red * r * factor / green).ln().powi(2) + (blue * b * factor / green).ln().powi(2);
        (error, tint)
    };
    let mut best = (f32::INFINITY, 5500, 0);
    for kelvin in (2000..=50_000).step_by(100) {
        let (error, tint) = score(kelvin);
        if error < best.0 {
            best = (error, kelvin, tint);
        }
    }
    let low = best.1.saturating_sub(100).max(2000);
    let high = (best.1 + 100).min(50_000);
    for kelvin in low..=high {
        let (error, tint) = score(kelvin);
        if error < best.0 {
            best = (error, kelvin, tint);
        }
    }
    // A strongly coloured object cannot supply a neutral illuminant in the supported range.
    if best.0 > 0.04 {
        return Err(invalid(
            "This area cannot be neutralized; choose a gray or white surface",
        ));
    }
    Ok((best.1, best.2))
}

/// Sample original working pixels and save both controls as one undoable manual edit.
/// # Errors
/// Invalid coordinates, catalog membership, unavailable pixels or a failed save.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn pick_white_balance(state: &AppState, input: &WhiteBalancePickInput) -> IpcResult<RecipeDto> {
    if !input.x.is_finite()
        || !input.y.is_finite()
        || !(0.0..=1.0).contains(&input.x)
        || !(0.0..=1.0).contains(&input.y)
    {
        return Err(invalid("Pick a point inside the original photograph").into());
    }
    let project = ProjectId::from_db(&input.project_id)
        .map_err(|_| invalid("Invalid collection identifier"))?;
    let photo =
        PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo identifier"))?;
    require_member(state, &input.project_id, &input.photo_id)?;
    let frame = crate::photo_frames::CatalogFrames::new(state.clone())
        .frame(&photo, RenderLevel::Proxy2048)?;
    if frame.width == 0 || frame.height == 0 {
        return Err(invalid("Photograph has no pixels").into());
    }
    let x = (input.x * frame.width as f32)
        .floor()
        .min((frame.width - 1) as f32) as u32;
    let y = (input.y * frame.height as f32)
        .floor()
        .min((frame.height - 1) as f32) as u32;
    let mut sum = [0.0_f32; 3];
    let mut count = 0_u32;
    for row in y.saturating_sub(3)..=y.saturating_add(3).min(frame.height - 1) {
        for col in x.saturating_sub(3)..=x.saturating_add(3).min(frame.width - 1) {
            let offset = (row as usize * frame.width as usize + col as usize) * 3;
            if let Some(pixel) = frame.rgb.get(offset..offset + 3) {
                if pixel
                    .iter()
                    .all(|v| v.is_finite() && *v > 0.005 && *v < 0.95)
                {
                    for (total, value) in sum.iter_mut().zip(pixel) {
                        *total += value;
                    }
                    count += 1;
                }
            }
        }
    }
    if count < 4 {
        return Err(invalid("Too few usable pixels; choose a larger midtone neutral area").into());
    }
    let (temperature, tint) = neutral_white_balance(sum.map(|v| v / count as f32))?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    let mut proposal = base.clone();
    proposal.global.temperature = temperature;
    proposal.global.tint = tint;
    let (mut merged, mut changes) = schema::merge(&base, &proposal, EditSource::User)?;
    // Choosing a neutral point explicitly sets both controls, even if one already
    // equals the fitted value. Preserve that intent against subsequent automatic edits.
    for path in ["global.temperature", "global.tint"] {
        if !merged
            .provenance
            .user_edited_fields
            .iter()
            .any(|field| field == path)
        {
            merged.provenance.user_edited_fields.push(path.to_owned());
            if !changes.changed.iter().any(|field| field == path) {
                changes.changed.push(path.to_owned());
            }
        }
    }
    merged.provenance.user_edited_fields.sort_unstable();
    changes.changed.sort_unstable();
    schema::Validation::check(&merged)?;
    state.recipe_store().save(
        &project,
        &photo,
        &merged,
        &changes.changed,
        "White balance picker",
    )?;
    Ok(crate::develop_commands::recipe_dto(
        &input.photo_id,
        &merged,
    ))
}
