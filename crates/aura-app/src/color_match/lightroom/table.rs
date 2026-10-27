//! Adobe DNG SDK RGB-table v1 interchange: blue-fastest RGB16 deltas,
//! little-endian header/trailer, zlib and Adobe's XMP-safe base85 alphabet.
//! Protocol reference: Adobe DNG SDK 1.7.1, `dng_big_table.{h,cpp}`.
use std::io::Write;

use aura_core::AuraResult;
use aura_raw::colour::curve::srgb_decode;
use aura_render::reference_grade::oklab_from_linear_srgb;
use flate2::{write::ZlibEncoder, Compression};

pub(super) const EDGE: usize = 32; // Compatible with the older SDK's serialized 3D limit.
const ALPHABET: &[u8] =
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?`'|()[]{}@%$#";

#[derive(Debug)]
pub(super) struct Table {
    pub fingerprint: String,
    pub encoded: String,
    pub mean_error: f32,
    pub p95_error: f32,
    pub max_error: f32,
}

fn point(grid: &[[u16; 3]], position: [usize; 3]) -> [f32; 3] {
    let [red, green, blue] = position;
    let index = (red * EDGE + green) * EDGE + blue;
    grid.get(index)
        .copied()
        .unwrap_or([0; 3])
        .map(|v| f32::from(v) / 65535.0)
}

/// Tetrahedral interpolation, including all six channel-order cases.
fn interpolate(grid: &[[u16; 3]], rgb: [f32; 3]) -> [f32; 3] {
    let scaled = rgb.map(|v| v.clamp(0.0, 1.0) * (EDGE - 1) as f32);
    let base = scaled.map(|v| (v as usize).min(EDGE - 2));
    let fraction = std::array::from_fn::<_, 3, _>(|i| {
        scaled.get(i).copied().unwrap_or(0.0) - base.get(i).copied().unwrap_or(0) as f32
    });
    let mut order = [0, 1, 2];
    order.sort_by(|a, b| {
        fraction
            .get(*b)
            .copied()
            .unwrap_or(0.0)
            .total_cmp(&fraction.get(*a).copied().unwrap_or(0.0))
    });
    let mut position = base;
    let mut previous = point(grid, position);
    let mut result = previous;
    for axis in order {
        if let Some(value) = position.get_mut(axis) {
            *value += 1;
        }
        let next = point(grid, position);
        for (channel, out) in result.iter_mut().enumerate() {
            *out += (next.get(channel).copied().unwrap_or(0.0)
                - previous.get(channel).copied().unwrap_or(0.0))
                * fraction.get(axis).copied().unwrap_or(0.0);
        }
        previous = next;
    }
    result
}

fn halton(mut index: usize, base: usize) -> f32 {
    let (mut scale, mut value) = (1.0, 0.0);
    while index > 0 {
        scale /= base as f32;
        value += scale * (index % base) as f32;
        index /= base;
    }
    value
}

fn error(left: [f32; 3], right: [f32; 3]) -> f32 {
    let a = oklab_from_linear_srgb(left.map(srgb_decode));
    let b = oklab_from_linear_srgb(right.map(srgb_decode));
    a.iter()
        .zip(b)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        .sqrt()
        * 100.0
}

fn wire(grid: &[[u16; 3]]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(EDGE.pow(3) * 6 + 44);
    for value in [1_u32, 1, 3, EDGE as u32] {
        payload.extend(value.to_le_bytes());
    }
    for (index, rgb) in grid.iter().enumerate() {
        let coordinates = [index / (EDGE * EDGE), index / EDGE % EDGE, index % EDGE];
        for (&value, coordinate) in rgb.iter().zip(coordinates) {
            let identity = ((coordinate * 65535 + EDGE / 2) / (EDGE - 1)) as u16;
            payload.extend(value.wrapping_sub(identity).to_le_bytes());
        }
    }
    // sRGB primaries, sRGB transfer function, gamut clip, amount range 0..1.
    for value in [0_u32, 1, 0] {
        payload.extend(value.to_le_bytes());
    }
    for value in [0_f64, 1.0] {
        payload.extend(value.to_le_bytes());
    }
    payload
}

fn base85(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 5 / 4 + 5);
    for chunk in bytes.chunks(4) {
        let mut group = [0_u8; 4];
        for (target, value) in group.iter_mut().zip(chunk) {
            *target = *value;
        }
        let mut value = u32::from_le_bytes(group);
        for _ in 0..=chunk.len() {
            result.push(char::from(
                ALPHABET.get((value % 85) as usize).copied().unwrap_or(b'0'),
            ));
            value /= 85;
        }
    }
    result
}

pub(super) fn build_with(
    evaluate: &mut impl FnMut(&[[f32; 3]]) -> AuraResult<Vec<[f32; 3]>>,
    checkpoint: &mut impl FnMut() -> AuraResult<()>,
) -> AuraResult<Table> {
    let mut inputs = Vec::with_capacity(EDGE.pow(3) + 4352);
    for red in 0..EDGE {
        checkpoint()?;
        for green in 0..EDGE {
            for blue in 0..EDGE {
                let rgb = [red, green, blue].map(|v| v as f32 / (EDGE - 1) as f32);
                inputs.push(rgb);
            }
        }
    }
    // Off-grid held-out colours and a dense neutral ramp. This measures table
    // approximation of AURA's grade, not Lightroom's complete raw renderer.
    for index in 1..=4096 {
        inputs.push([2, 3, 5].map(|base| halton(index, base)));
    }
    for index in 0..256 {
        inputs.push([index as f32 / 255.0; 3]);
    }
    let output = evaluate(&inputs)?;
    checkpoint()?;
    if output.len() != inputs.len()
        || output
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(super::super::invalid(
            "The Lightroom colour sampler returned invalid colours.",
        ));
    }
    let grid: Vec<_> = output
        .iter()
        .take(EDGE.pow(3))
        .map(|rgb| rgb.map(|v| (v * 65535.0).round() as u16))
        .collect();
    let mut errors = Vec::with_capacity(4352);
    for (&rgb, &expected) in inputs.iter().zip(&output).skip(EDGE.pow(3)) {
        errors.push(error(expected, interpolate(&grid, rgb)));
    }
    errors.sort_by(f32::total_cmp);
    let mean_error = errors.iter().sum::<f32>() / errors.len() as f32;
    let p95_error = errors.get(errors.len() * 95 / 100).copied().unwrap_or(0.0);
    let max_error = errors.last().copied().unwrap_or(0.0);
    let payload = wire(&grid);
    let fingerprint = format!("{:x}", md5::compute(&payload)).to_uppercase();
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&payload)
        .map_err(|_| super::super::invalid("Could not encode the Lightroom colour table."))?;
    let compressed = encoder
        .finish()
        .map_err(|_| super::super::invalid("Could not finish the Lightroom colour table."))?;
    let mut packet = (payload.len() as u32).to_le_bytes().to_vec();
    packet.extend(compressed);
    Ok(Table {
        fingerprint,
        encoded: base85(&packet),
        mean_error,
        p95_error,
        max_error,
    })
}

#[cfg(test)]
pub(super) fn build(
    grade: &aura_recipe::reference_grade::ReferenceGrade,
    checkpoint: &mut impl FnMut() -> AuraResult<()>,
) -> AuraResult<Table> {
    grade.validate()?;
    let evaluator = aura_render::reference_grade::Evaluator::new(grade);
    build_with(
        &mut |inputs| {
            Ok(inputs
                .iter()
                .map(|rgb| {
                    evaluator
                        .linear_srgb(rgb.map(srgb_decode))
                        .map(aura_render::output::encode_srgb)
                })
                .collect())
        },
        checkpoint,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_recipe::reference_grade::ReferenceGrade;

    #[test]
    fn wire_identity_has_adobe_header_zero_deltas_and_declared_colour_space() {
        let grid: Vec<_> = (0..EDGE.pow(3))
            .map(|index| {
                [index / (EDGE * EDGE), index / EDGE % EDGE, index % EDGE]
                    .map(|v| ((v * 65535 + EDGE / 2) / (EDGE - 1)) as u16)
            })
            .collect();
        let payload = wire(&grid);
        assert_eq!(payload.len(), 196_652);
        assert_eq!(
            &payload[..16],
            &[1, 0, 0, 0, 1, 0, 0, 0, 3, 0, 0, 0, 32, 0, 0, 0]
        );
        assert!(payload[16..16 + EDGE.pow(3) * 6].iter().all(|v| *v == 0));
        assert_eq!(
            &payload[payload.len() - 28..payload.len() - 16],
            &[0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn base85_known_groups_use_little_endian_and_correct_partial_lengths() {
        assert_eq!(base85(&[0; 4]), "00000");
        assert_eq!(base85(&[1, 0, 0, 0]), "10000");
        assert_eq!(base85(&[85]), "01");
        assert_eq!(base85(&[0, 0]), "000");
        assert_eq!(base85(&[0, 0, 0]), "0000");
        assert_eq!(ALPHABET.len(), 85);
    }

    #[test]
    fn identity_and_affine_colours_interpolate_in_every_tetrahedral_order() {
        let grid: Vec<_> = (0..EDGE.pow(3))
            .map(|index| {
                [index / (EDGE * EDGE), index / EDGE % EDGE, index % EDGE]
                    .map(|v| ((0.2 + 0.6 * v as f32 / (EDGE - 1) as f32) * 65535.0).round() as u16)
            })
            .collect();
        for index in 1..=256 {
            let rgb = [2, 3, 5].map(|b| halton(index, b));
            for (actual, source) in interpolate(&grid, rgb).into_iter().zip(rgb) {
                assert!((actual - (0.2 + 0.6 * source)).abs() < 2.0 / 65535.0);
            }
        }
        let table = build(&ReferenceGrade::identity(), &mut || Ok(())).unwrap();
        assert!(table.max_error < 0.01, "identity error {}", table.max_error);
    }
}
