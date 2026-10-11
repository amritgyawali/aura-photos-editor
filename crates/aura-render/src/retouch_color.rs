//! Explicit selected color effects. No smoothing or geometric change is performed.
// The buffers and 3x3 color matrix are dimension-checked by the render entry point.
#![allow(clippy::indexing_slicing)]
use aura_raw::colour::matrix::{mul, REC2020_TO_XYZ_D65, SRGB_TO_XYZ_D65};

/// Convert a display sRGB choice once per operation, before the pixel loop.
pub(crate) fn to_working(target: [f32; 3]) -> [f32; 3] {
    let linear = target.map(|v| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    let matrix = mul(
        aura_raw::colour::working_space::xyz_d65_to_rec2020(),
        SRGB_TO_XYZ_D65,
    );
    std::array::from_fn(|i| (0..3).map(|j| matrix[i][j] as f32 * linear[j]).sum())
}

/// Source, target and output are linear Rec.2020.
pub(crate) fn transform(old: [f32; 3], chosen: [f32; 3], preserve_luma: bool) -> [f32; 3] {
    if !preserve_luma {
        return chosen;
    }
    let luma = |rgb: [f32; 3]| {
        (0..3)
            .map(|c| REC2020_TO_XYZ_D65[1][c] as f32 * rgb[c])
            .sum::<f32>()
    };
    let lum = luma(old).max(0.0);
    let chosen_lum = luma(chosen);
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
        assert_eq!(transform([0.1, 0.4, 0.7], [0.0; 3], false), [0.0; 3]);
        let white = transform([0.1, 0.4, 0.7], [1.0; 3], false);
        let display = aura_raw::colour::working_space::working_to_linear_srgb(white.map(f64::from));
        assert!(
            display.iter().all(|v| (*v - 1.0).abs() < 0.000_001),
            "{display:?}"
        );
        assert_eq!(
            transform([0.1; 3], [1.0, 0.0, 0.0], false),
            transform([0.8; 3], [1.0, 0.0, 0.0], false)
        );
    }
}
