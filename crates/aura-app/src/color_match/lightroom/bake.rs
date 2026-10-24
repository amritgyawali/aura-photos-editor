//! Sample the native pointwise colour pipeline, including white balance.
//! Numerical slider values from two different raw processors are not a colour transform.
use aura_core::AuraResult;
use aura_recipe::Recipe;
use aura_render::{Frame, OutputSpec, RenderLevel, RenderPurpose, RenderedData};

use super::super::{invalid, AppState};

pub(super) fn colour_recipe(recipe: &Recipe) -> Recipe {
    let neutral = aura_recipe::fixtures::neutral(&recipe.image.content_hash, &recipe.image.camera);
    let mut colour = recipe.clone();
    colour.geometry = neutral.geometry;
    colour.lens = neutral.lens;
    colour.masks.clear();
    colour.retouch.clear();
    colour.cleanup.clear();
    colour.restoration = neutral.restoration;
    colour.global.clarity = 0;
    colour.global.texture = 0;
    colour.global.dehaze = 0;
    // These use an edge-aware, image-dependent base, not a pointwise function.
    colour.global.highlights = 0;
    colour.global.shadows = 0;
    colour.global.sharpen = neutral.global.sharpen;
    colour.global.noise = neutral.global.noise;
    colour.global.effects = neutral.global.effects;
    colour
        .extra
        .retain(|key, _| key == aura_recipe::reference_grade::KEY);
    colour
}

/// Native 16-bit sRGB samples; the input is display sRGB, never synthetic camera RAW.
pub(super) fn colours(
    state: &AppState,
    recipe: &Recipe,
    inputs: &[[f32; 3]],
) -> AuraResult<Vec<[f32; 3]>> {
    let width = 128;
    if inputs.is_empty() || !inputs.len().is_multiple_of(width) {
        return Err(invalid(
            "The Lightroom colour sampler has invalid dimensions.",
        ));
    }
    let working = inputs
        .iter()
        .flat_map(|rgb| aura_render::output::display_srgb_to_working(*rgb))
        .collect();
    let frame = Frame::working(
        working,
        width as u32,
        (inputs.len() / width) as u32,
        "AURA colour sampler",
    );
    let rendered = state.render()?.render_frame(
        &frame,
        recipe,
        RenderLevel::Full,
        RenderPurpose::Export,
        &OutputSpec {
            bit_depth: 16,
            ..OutputSpec::default()
        },
    )?;
    let RenderedData::Sixteen(words) = rendered.data else {
        return Err(invalid(
            "The Lightroom colour sampler requires 16-bit output.",
        ));
    };
    if words.len() != inputs.len() * 3 {
        return Err(invalid(
            "The Lightroom colour sampler returned incomplete output.",
        ));
    }
    Ok(words
        .chunks_exact(3)
        .map(|rgb| {
            [
                f32::from(rgb.first().copied().unwrap_or(0)) / 65535.0,
                f32::from(rgb.get(1).copied().unwrap_or(0)) / 65535.0,
                f32::from(rgb.get(2).copied().unwrap_or(0)) / 65535.0,
            ]
        })
        .collect())
}

/// Colour is already in the embedded profile. Keep transferable spatial/detail
/// controls separate and preserve the recipient camera's as-shot white balance.
pub(super) fn portable_settings(recipe: &Recipe) -> Recipe {
    let mut portable = recipe.clone();
    let source = &recipe.global;
    portable.global =
        aura_recipe::fixtures::neutral(&recipe.image.content_hash, &recipe.image.camera).global;
    portable.global.clarity = source.clarity;
    portable.global.texture = source.texture;
    portable.global.dehaze = source.dehaze;
    portable.global.highlights = source.highlights;
    portable.global.shadows = source.shadows;
    portable.global.sharpen = source.sharpen;
    portable.global.noise = source.noise.clone();
    portable.global.effects = source.effects;
    portable.bw = None; // The native monochrome mixer, if any, is also baked into RGB.
    portable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_and_spatial_settings_are_not_applied_twice() {
        let mut recipe = aura_recipe::fixtures::neutral("preset", "camera");
        recipe.global.temperature = 7800;
        recipe.global.tint = 14;
        recipe.global.exposure = 0.8;
        recipe.global.saturation = -20;
        recipe.global.highlights = -40;
        recipe.global.shadows = 25;
        recipe.global.clarity = 12;
        recipe.geometry.crop = [0.1, 0.2, 0.8, 0.9];
        let colour = colour_recipe(&recipe);
        assert_eq!(colour.global.temperature, 7800);
        assert_eq!(colour.global.tint, 14);
        assert!((colour.global.exposure - 0.8).abs() < 1e-6);
        assert_eq!(colour.global.saturation, -20);
        assert_eq!(colour.global.highlights, 0);
        assert_eq!(colour.global.shadows, 0);
        assert_eq!(colour.global.clarity, 0);
        assert!(colour.geometry.is_identity());
        let portable = portable_settings(&recipe);
        assert_eq!(portable.global.temperature, 5500);
        assert_eq!(portable.global.tint, 0);
        assert!(portable.global.exposure.abs() < 1e-6);
        assert_eq!(portable.global.saturation, 0);
        assert_eq!(portable.global.highlights, -40);
        assert_eq!(portable.global.shadows, 25);
        assert_eq!(portable.global.clarity, 12);
    }

    #[test]
    fn native_sampler_preserves_neutral_and_bakes_white_balance_and_exposure() {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::open(&directory.path().join("catalog.aura")).unwrap();
        let neutral = aura_recipe::fixtures::neutral("preset", "camera");
        let input = vec![[0.4, 0.5, 0.6]; 128];
        let unchanged = colours(&state, &colour_recipe(&neutral), &input).unwrap();
        assert!(unchanged
            .iter()
            .zip(&input)
            .all(|(a, b)| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.001)));
        let mut edited = neutral.clone();
        edited.global.temperature = 8000;
        edited.global.tint = 20;
        edited.global.exposure = 0.7;
        let output = colours(&state, &colour_recipe(&edited), &input).unwrap();
        assert!(output.iter().zip(&unchanged).all(|(a, b)| a
            .iter()
            .zip(b)
            .map(|(a, b)| (a - b).abs())
            .sum::<f32>()
            > 0.1));
        assert!(colours(&state, &neutral, &input[..127]).is_err());
    }
}
