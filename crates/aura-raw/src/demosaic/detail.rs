//! Gradient-corrected Bayer reconstruction, Malvar, He and Cutler (ICASSP 2004), Fig. 2.
//! https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/Demosaicing_ICASSP04.pdf
//! The analysis proxy stays unchanged; these filters improve full-size reconstruction.
use super::{put, CfaPattern, Levels, RgbF32};

/// Reconstruct a full Bayer frame with cross-channel gradient correction.
/// X-Trans and unknown layouts retain their own neighbourhood interpolation.
#[must_use]
pub fn full_malvar(mosaic: &crate::cfa::Mosaic, cfa: CfaPattern, levels: Levels) -> RgbF32 {
    full_malvar_region(mosaic, cfa, levels, 0, 0, mosaic.width, mosaic.height)
}

/// The same reconstruction over a region, using the original's global CFA phase.
/// The two-pixel sensor boundary uses the existing bounded bilinear interpolation.
#[must_use]
pub fn full_malvar_region(
    mosaic: &crate::cfa::Mosaic,
    cfa: CfaPattern,
    levels: Levels,
    origin_x: u32,
    origin_y: u32,
    width: u32,
    height: u32,
) -> RgbF32 {
    if !matches!(
        cfa,
        CfaPattern::Rggb | CfaPattern::Bggr | CfaPattern::Grbg | CfaPattern::Gbrg
    ) || mosaic.width < 5
        || mosaic.height < 5
    {
        return super::full_bilinear_region(mosaic, cfa, levels, origin_x, origin_y, width, height);
    }
    let mut out = RgbF32::black(width, height);
    let ceiling = levels
        .white_balance
        .iter()
        .copied()
        .filter(|v| *v > 0.0)
        .fold(f32::INFINITY, f32::min);
    let ceiling = if ceiling.is_finite() { ceiling } else { 1.0 };
    out.fill_rows(|y, row| {
        let sy = origin_y + y;
        for x in 0..width {
            let sx = origin_x + x;
            if sx < 2 || sy < 2 || sx + 2 >= mosaic.width || sy + 2 >= mosaic.height {
                let border = super::full_bilinear_region(mosaic, cfa, levels, sx, sy, 1, 1);
                put(row, x as usize, border.pixel(0, 0));
                continue;
            }
            let sample = |dx: i64, dy: i64| {
                let px = (i64::from(sx) + dx) as u32;
                let py = (i64::from(sy) + dy) as u32;
                levels.normalise(mosaic.at(px, py), cfa.colour_at(px, py))
            };
            let own = cfa.colour_at(sx, sy);
            let centre = sample(0, 0);
            let horizontal = sample(-1, 0) + sample(1, 0);
            let vertical = sample(0, -1) + sample(0, 1);
            let far_horizontal = sample(-2, 0) + sample(2, 0);
            let far_vertical = sample(0, -2) + sample(0, 2);
            let diagonal = sample(-1, -1) + sample(1, -1) + sample(-1, 1) + sample(1, 1);
            let mut pixel = [centre; 3];
            if own == 1 {
                let along_row = (5.0 * centre + 4.0 * horizontal - far_horizontal - diagonal
                    + 0.5 * far_vertical)
                    * 0.125;
                let along_column = (5.0 * centre + 4.0 * vertical - far_vertical - diagonal
                    + 0.5 * far_horizontal)
                    * 0.125;
                let red_in_row = cfa.colour_at(sx - 1, sy) == 0;
                pixel[0] = if red_in_row { along_row } else { along_column };
                pixel[2] = if red_in_row { along_column } else { along_row };
            } else {
                pixel[1] =
                    (4.0 * centre + 2.0 * (horizontal + vertical) - far_horizontal - far_vertical)
                        * 0.125;
                let opposite =
                    (6.0 * centre + 2.0 * diagonal - 1.5 * (far_horizontal + far_vertical)) * 0.125;
                if own == 0 {
                    pixel[2] = opposite;
                } else {
                    pixel[0] = opposite;
                }
            }
            put(row, x as usize, pixel.map(|v| v.clamp(0.0, ceiling)));
        }
    });
    out
}

#[cfg(test)]
// These tests require bit-exact reconstruction and index fixed RGB triples.
#[allow(clippy::float_cmp, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn levels() -> Levels {
        Levels {
            black: 0,
            white: 65535,
            white_balance: [1.0; 3],
        }
    }

    fn mosaic(cfa: CfaPattern, colour: impl Fn(u32, u32) -> [f32; 3]) -> crate::cfa::Mosaic {
        crate::cfa::Mosaic {
            width: 96,
            height: 64,
            data: (0..64)
                .flat_map(|y| (0..96).map(move |x| (x, y)))
                .map(|(x, y)| {
                    (colour(x, y)[usize::from(cfa.colour_at(x, y))] * 65535.0).round() as u16
                })
                .collect(),
        }
    }

    #[test]
    fn preserves_measured_channels_and_constant_colours_in_every_bayer_phase() {
        for cfa in [
            CfaPattern::Rggb,
            CfaPattern::Bggr,
            CfaPattern::Grbg,
            CfaPattern::Gbrg,
        ] {
            let sensor = mosaic(cfa, |_, _| [0.65, 0.3, 0.15]);
            let result = full_malvar(&sensor, cfa, levels());
            for y in 0..sensor.height {
                for x in 0..sensor.width {
                    let pixel = result.pixel(x, y);
                    for (actual, expected) in pixel.iter().zip([0.65, 0.3, 0.15]) {
                        assert!((actual - expected).abs() < 0.00004);
                    }
                    let own = usize::from(cfa.colour_at(x, y));
                    assert_eq!(pixel[own], levels().normalise(sensor.at(x, y), own as u8));
                }
            }
        }
    }

    #[test]
    fn reconstructs_luminance_detail_more_accurately_than_bilinear_without_false_colour() {
        for cfa in [
            CfaPattern::Rggb,
            CfaPattern::Bggr,
            CfaPattern::Grbg,
            CfaPattern::Gbrg,
        ] {
            let truth = |x: u32, y: u32| {
                let wave = ((x as f32 * 0.7 + y as f32 * 0.4).sin() * 0.28 + 0.5).clamp(0.0, 1.0);
                [wave; 3]
            };
            let sensor = mosaic(cfa, truth);
            let actual = full_malvar(&sensor, cfa, levels());
            let baseline = super::super::full_bilinear(&sensor, cfa, levels());
            let (mut error, mut old) = (0.0_f64, 0.0_f64);
            for y in 2..sensor.height - 2 {
                for x in 2..sensor.width - 2 {
                    for ((new, before), expected) in actual
                        .pixel(x, y)
                        .iter()
                        .zip(baseline.pixel(x, y))
                        .zip(truth(x, y))
                    {
                        error += f64::from(new - expected).powi(2);
                        old += f64::from(before - expected).powi(2);
                    }
                }
            }
            assert!(error < old * 0.3, "detail error {error}, bilinear {old}");
        }
    }

    #[test]
    fn independent_regions_match_whole_frame_at_tile_edges() {
        let cfa = CfaPattern::Grbg;
        let sensor = mosaic(cfa, |x, y| {
            [
                (x % 11) as f32 / 11.0,
                (y % 13) as f32 / 13.0,
                ((x + y) % 7) as f32 / 7.0,
            ]
        });
        let whole = full_malvar(&sensor, cfa, levels());
        for (ox, oy, w, h) in [(0, 0, 31, 29), (31, 29, 32, 30), (63, 59, 33, 5)] {
            let tile = full_malvar_region(&sensor, cfa, levels(), ox, oy, w, h);
            for y in 0..h {
                for x in 0..w {
                    assert_eq!(tile.pixel(x, y), whole.pixel(ox + x, oy + y));
                }
            }
        }
    }
}
