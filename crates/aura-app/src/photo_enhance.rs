//! A one-click local correction that works without optional model packs.

use crate::{
    commands::IpcResult,
    contract::ipc::{DevelopImageInput, RecipeDto},
    AppState,
};
#[cfg(test)]
use aura_recipe::{schema, EditSource};

/// Measure the original and save a restrained, repeatable, protected edit as undoable steps:
/// light and colour, sky, skin, blemishes, eyes, teeth and shine. See [`crate::smart_edit`].
///
/// # Errors
/// Returns a typed error if the original cannot be decoded or the edit cannot be saved.
pub fn enhance_photo(state: &AppState, input: &DevelopImageInput) -> IpcResult<RecipeDto> {
    crate::smart_edit::run(state, input, true)
}

/// Detect and retouch portraits without changing global exposure or a selected look.
/// # Errors
/// Returns a typed analysis, decode or recipe storage error.
pub fn enhance_portrait(state: &AppState, input: &DevelopImageInput) -> IpcResult<RecipeDto> {
    crate::smart_edit::run(state, input, false)
}

#[cfg(test)]
fn saved_recipe_changed(
    base: &aura_recipe::Recipe,
    merged: &aura_recipe::Recipe,
) -> aura_core::AuraResult<bool> {
    Ok(aura_recipe::recipe_hash(base)? != aura_recipe::recipe_hash(merged)?)
}

// The rounded integer controls are clamped to at most 25 in magnitude before conversion.
#[allow(clippy::cast_possible_truncation)]
/// The measured exposure, highlight, shadow and contrast correction for an sRGB preview.
///
/// # Errors
/// `AURA-RAW-2002` for an empty or truncated buffer.
pub fn correction(rgb: &[u8]) -> aura_core::AuraResult<(f32, i16, i16, i16)> {
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
    let measured = (0.18 / linear).log2().clamp(-limit, limit);
    // A bright background or white clothing is not evidence that a face is
    // overexposed. Preserve the normal display-lightness band instead of forcing
    // every photograph towards middle grey. Without subject detection, keep any
    // global darkening small; the separate highlight control handles bright detail.
    let mut exposure = if (0.45..=0.78).contains(&median) {
        0.0
    } else {
        measured.max(-0.25)
    };
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
    // The long literal is the point: it does not survive canonical rounding.
    #[allow(clippy::excessive_precision, clippy::unreadable_literal)]
    fn rounded_portrait_coordinates_do_not_create_another_saved_step() {
        let mut proposal =
            aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "test");
        proposal.extra.insert(
            "portrait_test".into(),
            serde_json::json!({"point": 0.123456789_f32}),
        );
        let base = serde_json::from_str(&aura_recipe::canonical(&proposal).unwrap()).unwrap();
        assert!(!schema::merge(&base, &proposal, EditSource::Ai)
            .unwrap()
            .1
            .changed
            .is_empty());
        assert!(!saved_recipe_changed(&base, &proposal).unwrap());
        proposal.global.exposure = 0.3;
        assert!(saved_recipe_changed(&base, &proposal).unwrap());
    }
    #[test]
    fn normal_portrait_brightness_is_preserved_and_darkening_is_restrained() {
        for brightness in [120, 150, 175, 195] {
            let (exposure, _, _, _) =
                correction(&[brightness; 300]).expect("valid portrait brightness fixture");
            assert!(exposure.abs() < f32::EPSILON);
        }
        let mut bright_background = vec![90; 120];
        bright_background.extend([230; 180]);
        let (exposure, highlights, _, _) =
            correction(&bright_background).expect("valid high-key portrait fixture");
        assert!(exposure >= -0.25);
        assert!(highlights < 0);
    }
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
