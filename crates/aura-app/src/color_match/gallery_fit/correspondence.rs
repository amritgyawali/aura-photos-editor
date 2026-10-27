//! Sparse native colour structure distinguishes fine patterns lost by thumbnails.
use aura_core::{progress::CancelToken, AuraResult};
use aura_raw::codec::Rgb8;
use aura_raw::colour::curve::srgb_byte_to_linear;
use aura_recipe::calibrated_look::{CalibratedLook, TERMS};
use aura_render::reference_grade::oklab_from_linear_srgb;
use serde::{Deserialize, Serialize};

const LIMIT: usize = 2048;
const MIN_POINTS: usize = 256;

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct SpatialEvidence {
    width: u32,
    height: u32,
    rgb: Vec<[u8; 3]>,
}

#[derive(Debug)]
struct Point {
    xy: [f64; 2],
    reference: [f64; 3],
    actual: [f64; 3],
}

fn held_out(point: &Point) -> bool {
    let tile = (point.xy[0] * 16.0) as usize + 16 * (point.xy[1] * 16.0) as usize;
    tile.is_multiple_of(5)
}

fn validation_error(points: &[Point], model: &CalibratedLook) -> (f32, f32, usize) {
    let (mut before, mut after, mut count) = (0.0_f32, 0.0_f32, 0_usize);
    for point in points.iter().filter(|point| held_out(point)) {
        let actual = point.actual.map(|value| value as f32 / 255.0);
        let reference = point.reference.map(|value| value as f32 / 255.0);
        before += super::super::calibration::pixel_error(actual, reference);
        after += super::super::calibration::pixel_error(model.encoded_srgb(actual), reference);
        count += 1;
    }
    (before, after, count)
}

impl SpatialEvidence {
    pub(super) fn new(image: &Rgb8) -> Option<Self> {
        let count = (image.width as usize).checked_mul(image.height as usize)?;
        if !(MIN_POINTS..=200_000_000).contains(&count) || count.checked_mul(3)? != image.data.len()
        {
            return None;
        }
        let rgb = super::super::calibration::sample_indices(count, LIMIT)
            .filter_map(|i| {
                image
                    .data
                    .get(i * 3..i * 3 + 3)
                    .and_then(|pixel| pixel.try_into().ok())
            })
            .collect();
        Some(Self {
            width: image.width,
            height: image.height,
            rgb,
        })
    }

    pub(super) fn valid(&self) -> bool {
        let count = (self.width as usize)
            .checked_mul(self.height as usize)
            .unwrap_or(0);
        self.width > 0
            && self.height > 0
            && (MIN_POINTS..=200_000_000).contains(&count)
            && self.rgb.len() == count.min(LIMIT)
    }

    fn points(&self, image: &Rgb8) -> Option<Vec<Point>> {
        if !self.valid() || image.width == 0 || image.height == 0 {
            return None;
        }
        let count = self.width as usize * self.height as usize;
        super::super::calibration::sample_indices(count, LIMIT)
            .zip(&self.rgb)
            .map(|(i, reference)| {
                let source_x = (i % self.width as usize) as f64 + 0.5;
                let source_y = (i / self.width as usize) as f64 + 0.5;
                let xy = [
                    source_x / f64::from(self.width),
                    source_y / f64::from(self.height),
                ];
                let column = (xy[0] * f64::from(image.width)).floor() as usize;
                let row = (xy[1] * f64::from(image.height)).floor() as usize;
                let at = row
                    .checked_mul(image.width as usize)?
                    .checked_add(column)?
                    .checked_mul(3)?;
                let &[r, g, b] = image.data.get(at..at + 3)? else {
                    return None;
                };
                Some(Point {
                    xy,
                    reference: reference.map(f64::from),
                    actual: [r, g, b].map(f64::from),
                })
            })
            .collect()
    }

    pub(super) fn corresponding(&self, image: &Rgb8) -> bool {
        let Some(points) = self.points(image) else {
            return false;
        };
        let (reference, actual) = residuals(&points);
        let (score, energy) = correlation(&reference, &actual);
        if score < 0.985 || energy < 27.0 * points.len() as f64 {
            return false;
        }
        let (mut supported, mut matching) = (0, 0);
        for tile in 0..16 {
            let selected: Vec<_> = points
                .iter()
                .enumerate()
                .filter(|(_, point)| {
                    let x = (point.xy[0] * 4.0) as usize;
                    let y = (point.xy[1] * 4.0) as usize;
                    y * 4 + x == tile
                })
                .collect();
            if selected.len() < 8 {
                continue;
            }
            let a: Vec<_> = selected
                .iter()
                .filter_map(|(i, _)| reference.get(*i).copied())
                .collect();
            let b: Vec<_> = selected
                .iter()
                .filter_map(|(i, _)| actual.get(*i).copied())
                .collect();
            let a = centred(&a);
            let b = centred(&b);
            let (score, energy) = correlation(&a, &b);
            if energy >= 27.0 * selected.len() as f64 {
                supported += 1;
                matching += usize::from(score >= 0.95);
            }
        }
        supported >= 6 && matching * 5 >= supported * 4
    }

    pub(super) fn distance(&self, image: &Rgb8) -> f32 {
        let Some(points) = self.points(image) else {
            return f32::INFINITY;
        };
        let mut total = 0.0;
        for point in &points {
            let lab =
                |rgb: [f64; 3]| oklab_from_linear_srgb(rgb.map(|v| srgb_byte_to_linear(v as u8)));
            let (a, b) = (lab(point.reference), lab(point.actual));
            total += a
                .iter()
                .zip(b)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f32>()
                .sqrt();
        }
        100.0 * total / points.len() as f32
    }

    /// Fit a monotone per-channel operation only after native registration has qualified.
    /// Hold out whole spatial tiles; local edits and unsupported operations must not be
    /// mistaken for a reusable global RGB transform. Identity is the regularisation prior.
    pub(super) fn affine(
        &self,
        image: &Rgb8,
        cancel: &CancelToken,
    ) -> AuraResult<Option<CalibratedLook>> {
        let Some(points) = self.points(image) else {
            return Ok(None);
        };
        let mut model = CalibratedLook {
            version: 1,
            coefficients: vec![[0.0; 3]; TERMS],
            amount: 1.0,
            residual_lut: None,
        };
        for component in 0..3 {
            super::super::check_cancel(cancel)?;
            let pairs: Vec<_> = points
                .iter()
                .filter(|point| !held_out(point))
                .filter_map(|point| {
                    let input = channel(&point.actual, component) / 255.0;
                    let output = channel(&point.reference, component) / 255.0;
                    ((0.02..0.98).contains(&input) && (0.005..0.995).contains(&output))
                        .then_some((input, output))
                })
                .collect();
            if pairs.len() < 128 {
                return Ok(None);
            }
            let (mut gain, mut offset) = (1.0_f64, 0.0_f64);
            for round in 0..3 {
                let weights: Vec<_> = pairs
                    .iter()
                    .map(|(input, output)| {
                        if round == 0 {
                            1.0
                        } else {
                            let residual = (gain * input + offset - output) / 0.02;
                            1.0 / (1.0 + residual * residual)
                        }
                    })
                    .collect();
                let mass = weights.iter().sum::<f64>();
                if mass < 32.0 {
                    return Ok(None);
                }
                let mean_input = pairs
                    .iter()
                    .zip(&weights)
                    .map(|((input, _), weight)| input * weight)
                    .sum::<f64>()
                    / mass;
                let mean_output = pairs
                    .iter()
                    .zip(&weights)
                    .map(|((_, output), weight)| output * weight)
                    .sum::<f64>()
                    / mass;
                let variance = pairs
                    .iter()
                    .zip(&weights)
                    .map(|((input, _), weight)| weight * (input - mean_input).powi(2))
                    .sum::<f64>();
                if variance < 1e-4 * mass {
                    return Ok(None);
                }
                let covariance = pairs
                    .iter()
                    .zip(&weights)
                    .map(|((input, output), weight)| {
                        weight * (input - mean_input) * (output - mean_output)
                    })
                    .sum::<f64>();
                let prior = 0.001 * mass;
                gain = ((covariance + prior) / (variance + prior)).clamp(0.5, 2.0);
                offset = (mean_output - gain * mean_input).clamp(-0.25, 0.25);
            }
            if let Some(constant) = model
                .coefficients
                .first_mut()
                .and_then(|row| row.get_mut(component))
            {
                *constant = ((gain - 1.0) * 0.5 + offset) as f32;
            }
            if let Some(linear) = model
                .coefficients
                .get_mut(component + 1)
                .and_then(|row| row.get_mut(component))
            {
                *linear = ((gain - 1.0) * 0.5) as f32;
            }
        }
        model.validate()?;
        let (before, after, count) = validation_error(&points, &model);
        super::super::check_cancel(cancel)?;
        Ok((count >= 64 && after < 0.9 * before && after / count as f32 <= 1.5).then_some(model))
    }
}

fn centred(values: &[[f64; 3]]) -> Vec<[f64; 3]> {
    let mean = values
        .iter()
        .fold([0.0; 3], |mut sum, value| {
            for (sum, value) in sum.iter_mut().zip(value) {
                *sum += value;
            }
            sum
        })
        .map(|v| v / values.len().max(1) as f64);
    values
        .iter()
        .map(|value| std::array::from_fn(|c| channel(value, c) - channel(&mean, c)))
        .collect()
}

fn channel(value: &[f64; 3], index: usize) -> f64 {
    value.get(index).copied().unwrap_or(0.0)
}

fn correlation(a: &[[f64; 3]], b: &[[f64; 3]]) -> (f64, f64) {
    let (mut dot, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (&x, &y) in a.iter().flatten().zip(b.iter().flatten()) {
        dot += x * y;
        aa += x * x;
        bb += y * y;
    }
    let norm = (aa * bb).sqrt();
    (if norm > 1e-9 { dot / norm } else { 0.0 }, aa.min(bb))
}

fn residuals(points: &[Point]) -> (Vec<[f64; 3]>, Vec<[f64; 3]>) {
    let n = points.len() as f64;
    let mean_xy = points
        .iter()
        .fold([0.0; 2], |sum, p| [sum[0] + p.xy[0], sum[1] + p.xy[1]])
        .map(|v| v / n);
    let xx = points
        .iter()
        .map(|p| (p.xy[0] - mean_xy[0]).powi(2))
        .sum::<f64>();
    let yy = points
        .iter()
        .map(|p| (p.xy[1] - mean_xy[1]).powi(2))
        .sum::<f64>();
    let xy = points
        .iter()
        .map(|p| (p.xy[0] - mean_xy[0]) * (p.xy[1] - mean_xy[1]))
        .sum::<f64>();
    let remove = |reference: bool| {
        let colours: Vec<_> = points
            .iter()
            .map(|p| if reference { p.reference } else { p.actual })
            .collect();
        let values = centred(&colours);
        let moment = |axis: usize| {
            values.iter().zip(points).fold([0.0; 3], |mut sum, (v, p)| {
                let offset = p.xy.get(axis).copied().unwrap_or(0.0)
                    - mean_xy.get(axis).copied().unwrap_or(0.0);
                for (sum, &v) in sum.iter_mut().zip(v) {
                    *sum += v * offset;
                }
                sum
            })
        };
        let (mx, my) = (moment(0), moment(1));
        let det = (xx * yy - xy * xy).max(1e-9);
        values
            .iter()
            .zip(points)
            .map(|(value, p)| {
                std::array::from_fn(|c| {
                    channel(value, c)
                        - (channel(&mx, c) * yy - channel(&my, c) * xy) / det
                            * (p.xy[0] - mean_xy[0])
                        - (channel(&my, c) * xx - channel(&mx, c) * xy) / det
                            * (p.xy[1] - mean_xy[1])
                })
            })
            .collect()
    };
    (remove(true), remove(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_affine_grade_generalises_and_refuses_local_relighting_and_cancellation() {
        let original = Rgb8 {
            width: 96,
            height: 64,
            data: (0..96 * 64)
                .flat_map(|i| {
                    let value = 30 + (i % 96) as u8;
                    [value + 35, value + 10, value]
                })
                .collect(),
        };
        let mut edited = original.clone();
        edited.data.iter_mut().for_each(|value| *value += 20);
        let evidence = SpatialEvidence::new(&edited).unwrap();
        let model = evidence
            .affine(&original, &CancelToken::new())
            .unwrap()
            .unwrap();
        for (pixel, expected) in original
            .data
            .chunks_exact(3)
            .zip(edited.data.chunks_exact(3))
        {
            let input = [pixel[0], pixel[1], pixel[2]].map(|value| f32::from(value) / 255.0);
            for (actual, expected) in model.encoded_srgb(input).iter().zip(expected) {
                assert!((actual * 255.0 - f32::from(*expected)).abs() < 0.05);
            }
        }
        let mut local = original.clone();
        for value in local.data.iter_mut().take(96 * 32 * 3) {
            *value += 60;
        }
        assert!(SpatialEvidence::new(&local)
            .unwrap()
            .affine(&original, &CancelToken::new())
            .unwrap()
            .is_none());
        let cancel = CancelToken::new();
        cancel.cancel();
        assert!(evidence.affine(&original, &cancel).is_err());
    }

    #[test]
    fn sparse_native_structure_recognises_fabric_but_not_smooth_ramps_or_other_patterns() {
        let image = |offset: u8, reverse: bool, fabric: bool| Rgb8 {
            width: 768,
            height: 256,
            data: (0..768_usize * 256)
                .flat_map(|i| {
                    let x = i % 768;
                    let value = 25 + (x * 100 / 768) as u8 + offset;
                    if fabric {
                        if x.is_multiple_of(2) == reverse {
                            [value, value, value + 60]
                        } else {
                            [value + 60, value, value]
                        }
                    } else {
                        [value; 3]
                    }
                })
                .collect(),
        };
        let reference = SpatialEvidence::new(&image(10, false, true)).unwrap();
        assert!(reference.corresponding(&image(0, false, true)));
        assert!(!reference.corresponding(&image(0, true, true)));
        assert!(reference.distance(&image(10, false, true)) < 1e-6);
        assert!(!SpatialEvidence::new(&image(10, false, false))
            .unwrap()
            .corresponding(&image(0, false, false)));
    }
}
