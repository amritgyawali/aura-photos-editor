//! A one-click local correction that works without optional model packs.

use crate::{
    commands::IpcResult,
    contract::ipc::{DevelopImageInput, RecipeDto},
    AppState,
};
use aura_core::{PhotoId, ProjectId};
use aura_preview::contract::service::{PreviewService, Priority};
use aura_recipe::{schema, EditSource};

/// Measure the original and save a restrained, repeatable, protected correction.
///
/// # Errors
/// Returns a typed error if the original cannot be decoded or the edit cannot be saved.
pub fn enhance_photo(state: &AppState, input: &DevelopImageInput) -> IpcResult<RecipeDto> {
    let invalid = |message: &str| aura_core::errors::render::recipe_invalid("photo", message);
    let photo =
        PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo identifier"))?;
    let project_id: String = state.catalog().read(|conn| {
        conn.query_row(
            "SELECT project_id FROM photo WHERE photo_id=?1",
            [&input.photo_id],
            |row| row.get(0),
        )
        .map_err(|e| aura_core::errors::db::statement_failed("enhance photo", &e))
    })?;
    let project =
        ProjectId::from_db(&project_id).map_err(|_| invalid("Invalid project identifier"))?;
    let pixels = state.previews(&project_id)?.get(
        photo,
        aura_raw::PixelLevel::Thumb(512),
        Priority::Interactive,
    )?;
    let Some(rgb) = pixels.as_srgb8() else {
        return Err(invalid("An sRGB preview is required").into());
    };
    let (exposure, highlights, shadows, contrast) = correction(rgb)?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    let mut proposal = base.clone();
    proposal.global.exposure = exposure;
    proposal.global.highlights = highlights;
    proposal.global.shadows = shadows;
    proposal.global.contrast = contrast;
    proposal.provenance.source = EditSource::Ai;
    proposal.provenance.confidence = 0.35;
    let (merged, report) = schema::merge(&base, &proposal, EditSource::Ai)?;
    schema::Validation::check(&merged)?;
    state.recipe_store().save(
        &project,
        &photo,
        &merged,
        &report.changed,
        "Local auto enhancement: measured brightness and contrast; no trained model used",
    )?;
    Ok(crate::develop_commands::recipe_dto(
        &input.photo_id,
        &merged,
    ))
}

// The rounded integer controls are clamped to at most 25 in magnitude before conversion.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn correction(rgb: &[u8]) -> aura_core::AuraResult<(f32, i16, i16, i16)> {
    if rgb.is_empty() || !rgb.len().is_multiple_of(3) {
        return Err(aura_core::errors::raw::corrupt(
            "Incomplete enhancement pixels",
        ));
    }
    // Measure linear luminance, then bin display lightness. A histogram keeps
    // memory constant and avoids sorting every pixel of every imported photo.
    let mut histogram = [0_usize; 256];
    for pixel in rgb.chunks_exact(3) {
        let linear: f32 = pixel
            .iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(v, weight)| aura_raw::colour::curve::srgb_decode(f32::from(*v) / 255.0) * weight)
            .sum();
        let bin =
            aura_raw::colour::curve::quantise_u8(aura_raw::colour::curve::srgb_encode(linear));
        if let Some(count) = histogram.get_mut(usize::from(bin)) {
            *count += 1;
        }
    }
    let percentile = |n| {
        let target = (rgb.len() / 3 - 1) * n / 100;
        let mut count = 0;
        for (bin, amount) in (0_u16..256).zip(histogram.iter()) {
            count += amount;
            if count > target {
                return f32::from(bin) / 255.0;
            }
        }
        1.0
    };
    let low = percentile(10);
    let median = percentile(50);
    let high = percentile(95);
    // Empty black/white frames contain no recoverable detail. Do not invent a
    // correction or add contrast to a uniformly colored photograph.
    if high < 0.02 || low > 0.98 {
        return Ok((0.0, 0, 0, 0));
    }
    // Histograms cannot distinguish silhouettes from underexposure. Keep the
    // correction especially small in uniformly dark or bright photographs.
    let limit = if high < 0.43 || low > 0.63 {
        0.25
    } else {
        0.75
    };
    let linear = aura_raw::colour::curve::srgb_decode(median).max(0.005);
    let mut exposure = (0.18 / linear).log2().clamp(-limit, limit);
    // Lift backlit shadows locally instead of blowing out an already bright sky.
    if exposure > 0.0 {
        let headroom = (0.98 / aura_raw::colour::curve::srgb_decode(high).max(0.005))
            .log2()
            .max(0.0);
        exposure = exposure.min(headroom);
    }
    let range = high - low;
    Ok((
        exposure,
        -(((high - 0.82) / 0.18).clamp(0.0, 1.0) * 25.0).round() as i16,
        if high > 0.4 {
            (((0.24 - low) / 0.24).clamp(0.0, 1.0) * 20.0).round() as i16
        } else {
            0
        },
        if range > 0.08 {
            (((0.55 - range) / 0.55).clamp(0.0, 1.0) * 10.0).round() as i16
        } else {
            0
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn correction_is_bounded_and_responds_to_actual_brightness() {
        assert!(
            correction(&[30; 300])
                .expect("valid photo fixture and successful operation")
                .0
                > 0.0
        );
        assert!(
            correction(&[230; 300])
                .expect("valid photo fixture and successful operation")
                .0
                < 0.0
        );
        assert!(
            correction(&[0; 300])
                .expect("valid photo fixture and successful operation")
                .0
                <= 0.25
        );
        assert!(correction(&[]).is_err());
        assert!(correction(&[1, 2]).is_err());
    }

    #[test]
    fn protects_backlit_highlights_and_keeps_blank_frames_neutral() {
        let mut backlit = vec![40; 270];
        backlit.extend([250; 30]);
        let (exposure, highlights, shadows, _) =
            correction(&backlit).expect("valid photo fixture and successful operation");
        assert!(exposure < 0.1);
        assert!(highlights < 0);
        assert!(shadows > 0);
        assert_eq!(
            correction(&[0; 300]).expect("valid photo fixture and successful operation"),
            (0.0, 0, 0, 0)
        );
        assert_eq!(
            correction(&[255; 300]).expect("valid photo fixture and successful operation"),
            (0.0, 0, 0, 0)
        );
        assert_eq!(
            correction(&[110; 300])
                .expect("valid photo fixture and successful operation")
                .3,
            0
        );
    }
}
