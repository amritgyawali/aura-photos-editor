//! Explicit selected color effects. No smoothing or geometric change is performed.
// The buffers and 3x3 color matrix are dimension-checked by the render entry point.
#![allow(clippy::indexing_slicing)]
use aura_raw::colour::working_space::working_luminance_f32;

/// Convert a display sRGB choice once per operation, before the pixel loop. The value is the
/// one the output transform renders back to that colour, as for any display-referred pixel.
pub(crate) fn to_working(target: [f32; 3]) -> [f32; 3] {
    crate::output::display_srgb_to_working(target)
}

/// Source, target and output are linear Rec.2020.
pub(crate) fn transform(old: [f32; 3], chosen: [f32; 3], preserve_luma: bool) -> [f32; 3] {
    if !preserve_luma {
        return chosen;
    }
    let lum = working_luminance_f32(old).max(0.0);
    let chosen_lum = working_luminance_f32(chosen);
    if chosen_lum < 0.000_001 {
        return [lum; 3];
    }
    let chroma = chosen.map(|v| v / chosen_lum - 1.0);
    // Compress chroma together at highlights so no channel clipping loses strand detail.
    // HDR source highlights are retained, not silently reduced to SDR white.
    let ceiling = old.iter().copied().fold(1.0_f32, f32::max);
    let mut saturation = lum;
    for delta in chroma {
        if delta > 0.0 {
            saturation = saturation.min((ceiling - lum).max(0.0) / delta);
        }
        if delta < 0.0 {
            saturation = saturation.min(lum / -delta);
        }
    }
    chroma.map(|v| lum + saturation * v)
}

#[cfg(test)]
// Exact zero and source-independent fill are determinism assertions, not approximate color tests.
#[allow(clippy::float_cmp)]
mod tests {
    fn transform(old: [f32; 3], target: [f32; 3], preserve_luma: bool) -> [f32; 3] {
        super::transform(old, super::to_working(target), preserve_luma)
    }
    fn luma(rgb: [f32; 3]) -> f32 {
        rgb[0] * 0.2627 + rgb[1] * 0.6780 + rgb[2] * 0.0593
    }

    #[test]
    fn recoloring_preserves_luminance_and_detail_through_the_highlights() {
        for target in [[1.0, 0.0, 0.0], [0.1, 0.3, 0.7], [0.0; 3], [1.0; 3]] {
            let mut previous = 0.0;
            for level in [0.001, 0.03, 0.1, 0.35, 0.9, 1.0, 2.0] {
                let old = [level * 0.9, level, level * 1.1];
                let out = transform(old, target, true);
                assert!((luma(old) - luma(out)).abs() < 0.0001);
                assert!(out.iter().all(|v| v.is_finite() && *v >= 0.0));
                assert!(luma(out) > previous);
                previous = luma(out);
            }
        }
    }

    #[test]
    fn solid_fill_converts_srgb_and_does_not_depend_on_source_texture() {
        use crate::contract::render::OutputColour::Srgb;
        assert_eq!(transform([0.1, 0.4, 0.7], [0.0; 3], false), [0.0; 3]);
        // A chosen colour is the colour that is delivered: through the output transform, not
        // only up to it, which is where a bright choice used to come out duller than chosen.
        for chosen in [[1.0; 3], [0.84, 0.9, 0.92], [0.5, 0.2, 0.7]] {
            let working = transform([0.1, 0.4, 0.7], chosen, false);
            let display =
                crate::output::map_one(working, crate::output::working_to_output(Srgb), Srgb);
            assert!(
                display
                    .iter()
                    .zip(chosen)
                    .all(|(out, want)| (out - want).abs() < 0.001),
                "{chosen:?} was delivered as {display:?}"
            );
        }
        assert_eq!(
            transform([0.1; 3], [1.0, 0.0, 0.0], false),
            transform([0.8; 3], [1.0, 0.0, 0.0], false)
        );
    }
}
