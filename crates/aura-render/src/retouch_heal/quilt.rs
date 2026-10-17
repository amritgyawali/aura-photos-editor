//! Deterministic overlapping transfers from independently checked clean donors.
//! Empty saved donor lists continue through the original reflection algorithm.
use super::{difference, Donor, Image};

pub(super) fn sources(image: Image<'_>, source: Donor, points: &[[f32; 2]]) -> Vec<Donor> {
    points
        .iter()
        .map(|p| Donor {
            offset: [
                p[0] * image.w as f32 - source.centre[0],
                p[1] * image.h as f32 - source.centre[1],
            ],
            ..source
        })
        .collect()
}

pub(super) fn detail(
    image: Image<'_>,
    source: Donor,
    x: f32,
    y: f32,
    radius: f32,
) -> Option<[f32; 3]> {
    let donor = source.texture_at(image, x, y)?;
    let mut low = [0.0; 3];
    let mut count = 0.0;
    for dy in -2..=2 {
        for dx in -2..=2 {
            if let Some(p) = source.texture_at(
                image,
                x + dx as f32 * radius * 0.5,
                y + dy as f32 * radius * 0.5,
            ) {
                for c in 0..3 {
                    low[c] += p[c];
                }
                count += 1.0;
            }
        }
    }
    Some(difference(donor, low.map(|v| v / count)))
}

fn hash(x: i32, y: i32) -> u32 {
    let mut value = (x as u32).wrapping_mul(0x9e37_79b9) ^ (y as u32).wrapping_mul(0x85eb_ca6b);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

pub(super) fn blend(
    image: Image<'_>,
    source: Donor,
    donors: &[Donor],
    x: f32,
    y: f32,
    radius: f32,
) -> Option<[f32; 3]> {
    // The unit is proportional to the measured donor footprint, so the pattern
    // is identical across preview/export sizes. Never stretch pores or read beyond
    // that footprint. Rotations preserve native texture scale.
    let step = source.half_extent[0].min(source.half_extent[1]) * 0.8;
    let q = [(x - source.centre[0]) / step, (y - source.centre[1]) / step];
    let tile = [q[0].floor() as i32, q[1].floor() as i32];
    let fade = |t: f32| t * t * (3.0 - 2.0 * t);
    let f = [fade(q[0] - tile[0] as f32), fade(q[1] - tile[1] as f32)];
    let mut sum = [0.0; 3];
    let mut energy = 0.0;
    for dy in 0..=1 {
        for dx in 0..=1 {
            let seed = hash(tile[0] + dx, tile[1] + dy);
            let donor = *donors.get(seed as usize % donors.len())?;
            let offset = [
                (q[0] - (tile[0] + dx) as f32) * step,
                (q[1] - (tile[1] + dy) as f32) * step,
            ];
            let rotated = match seed % 4 {
                0 => offset,
                1 => [-offset[1], offset[0]],
                2 => [-offset[0], -offset[1]],
                _ => [offset[1], -offset[0]],
            };
            let weight = (if dx == 0 { 1.0 - f[0] } else { f[0] })
                * (if dy == 0 { 1.0 - f[1] } else { f[1] });
            let high = detail(
                image,
                donor,
                source.centre[0] + rotated[0],
                source.centre[1] + rotated[1],
                radius,
            )?;
            for c in 0..3 {
                sum[c] += high[c] * weight;
            }
            energy += weight * weight;
        }
    }
    // Overlap must not flatten fine texture. Normalize the measured high band's
    // blend energy; do not alter target illumination or generate random pixels.
    Some(sum.map(|v| v / energy.sqrt().max(0.5)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    #[test]
    fn independent_donors_break_mirrored_repetition_keep_texture_and_join_continuously() {
        let w = 128;
        let mut rgb = vec![0.4; w * w * 3];
        let centres = [[28.0, 32.0], [48.0, 32.0], [88.0, 32.0], [28.0, 92.0]];
        for (n, centre) in centres.iter().enumerate() {
            for y in centre[1] as usize - 8..=centre[1] as usize + 8 {
                for x in centre[0] as usize - 8..=centre[0] as usize + 8 {
                    let detail = ((x as f32 - centre[0]) * 0.75 + n as f32 * 0.8).cos() * 0.018
                        + ((y as f32 - centre[1]) * 0.9 + n as f32).cos() * 0.009;
                    for c in 0..3 {
                        rgb[(y * w + x) * 3 + c] = 0.4 + detail;
                    }
                }
            }
        }
        let source = Donor {
            centre: [64.0; 2],
            offset: [-36.0, -32.0],
            scale: 0.25,
            half_extent: [6.0; 2],
        };
        let donors: Vec<_> = centres
            .iter()
            .map(|p| Donor {
                offset: [p[0] - 64.0, p[1] - 64.0],
                ..source
            })
            .collect();
        let image = Image { rgb: &rgb, w, h: w };
        let mut legacy = 0.0;
        let mut repaired = 0.0;
        let mut energy = 0.0;
        for n in 0..21 {
            let t = n as f32 * 0.25;
            let a = detail(image, source, 52.0 + t, 64.0, 2.0).unwrap()[0];
            let b = detail(image, source, 76.0 - t, 64.0, 2.0).unwrap()[0];
            legacy += (a - b).abs();
            let a = blend(image, source, &donors, 52.0 + t, 64.0, 2.0).unwrap()[0];
            let b = blend(image, source, &donors, 76.0 - t, 64.0, 2.0).unwrap()[0];
            repaired += (a - b).abs();
            energy += a * a + b * b;
        }
        assert!(
            legacy < 0.0001,
            "reflection negative control lacks the repeated pore pattern"
        );
        assert!(
            repaired > 0.01,
            "independent donor texture still repeats the mirrored pattern"
        );
        assert!(energy > 0.0001, "blending flattened away real fine texture");
        for n in -4..=4 {
            let x = 64.0 + n as f32 * 4.8;
            let a = blend(image, source, &donors, x - 0.001, 65.0, 2.0).unwrap();
            let b = blend(image, source, &donors, x + 0.001, 65.0, 2.0).unwrap();
            for c in 0..3 {
                assert!((a[c] - b[c]).abs() < 0.0002, "visible texture seam");
            }
        }
        for exposure in [0.35, 1.8] {
            let scaled: Vec<_> = rgb.iter().map(|v| v * exposure).collect();
            let scaled_image = Image {
                rgb: &scaled,
                w,
                h: w,
            };
            let a = blend(image, source, &donors, 67.25, 63.25, 2.0).unwrap();
            let b = blend(scaled_image, source, &donors, 67.25, 63.25, 2.0).unwrap();
            for c in 0..3 {
                assert!((a[c] * exposure - b[c]).abs() < 0.000_001);
            }
        }
    }
}
