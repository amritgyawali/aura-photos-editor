//! Separate a patch's smooth lighting from the texture used to choose retouch strength.

/// Residual range after a least-squares plane over a centred 5 by 5 patch.
/// The coordinate axes have zero mean, so they cannot change the patch's brightness.
pub(super) fn texture(values: &[[f32; 5]; 5], mean: f32) -> f32 {
    // Sum of squared coordinates (-2..=2), repeated along the other axis.
    const AXIS_ENERGY: f32 = 50.0;
    let mut slopes = [0.0; 2];
    for (y, row) in (-2_i16..=2).zip(values) {
        for (x, value) in (-2_i16..=2).zip(row) {
            slopes[0] += f32::from(x) * value;
            slopes[1] += f32::from(y) * value;
        }
    }
    let [horizontal, vertical] = slopes.map(|sum| sum / AXIS_ENERGY);
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for (y, row) in (-2_i16..=2).zip(values) {
        for (x, value) in (-2_i16..=2).zip(row) {
            let residual = value - mean - horizontal * f32::from(x) - vertical * f32::from(y);
            low = low.min(residual);
            high = high.max(residual);
        }
    }
    high - low
}

#[cfg(test)]
mod tests {
    use super::super::sample_quality;

    fn patch(base: u8, slope: i16, texture: i16) -> Vec<u8> {
        (0_i16..9)
            .flat_map(|y| {
                (0_i16..9).flat_map(move |x| {
                    let detail = if (x + y) % 2 == 0 { texture } else { -texture };
                    let value = u8::try_from(i16::from(base) + slope * (x - 4) + detail).unwrap();
                    [value; 3]
                })
            })
            .collect()
    }

    #[test]
    fn directional_light_does_not_increase_measured_skin_texture() {
        for base in [60, 120, 200] {
            let flat = sample_quality(&patch(base, 0, 0), 9, 9, [4.0, 4.0]).unwrap();
            let shaded = sample_quality(&patch(base, 8, 0), 9, 9, [4.0, 4.0]).unwrap();
            assert!((shaded.mean - flat.mean).abs() < 1e-6);
            assert!(
                shaded.variation < 1e-6,
                "light is not texture: {}",
                shaded.variation
            );
            assert_eq!(shaded.chroma, flat.chroma);
        }
    }

    #[test]
    fn actual_texture_survives_removing_the_lighting_gradient() {
        for base in [60, 120, 200] {
            let flat = sample_quality(&patch(base, 0, 5), 9, 9, [4.0, 4.0]).unwrap();
            let shaded = sample_quality(&patch(base, 6, 5), 9, 9, [4.0, 4.0]).unwrap();
            assert!(shaded.variation > 0.035);
            assert!((shaded.variation - flat.variation).abs() < 1e-6);
        }
    }

    #[test]
    fn feature_boundaries_and_invalid_samples_are_still_rejected() {
        let boundary: Vec<u8> = (0..81)
            .flat_map(|i| if i % 9 < 4 { [40; 3] } else { [210; 3] })
            .collect();
        assert!(sample_quality(&boundary, 9, 9, [4.0, 4.0]).is_none());
        assert!(sample_quality(&[255; 243], 9, 9, [4.0, 4.0]).is_none());
        assert!(sample_quality(&[0; 243], 9, 9, [4.0, 4.0]).is_none());
        assert!(sample_quality(&[100; 3], 9, 9, [4.0, 4.0]).is_none());
        assert!(sample_quality(&[100; 243], 9, 9, [f32::NAN, 4.0]).is_none());
    }
}
