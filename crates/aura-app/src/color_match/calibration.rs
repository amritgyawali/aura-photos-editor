//! Supervised global colour calibration, independent of target-scene colour proportions.
//!
//! An edited reference alone confounds object colours and edits. An aligned before/after
//! pair supplies corresponding RGBs. Fit a regularised cubic residual with robust reweighting,
//! and qualify it on spatially held-out pixels through the renderer's actual lookup evaluator.
use aura_core::{progress::CancelToken, AuraResult};
use aura_raw::{codec::Rgb8, colour::curve::srgb_decode};
use aura_recipe::calibrated_look::{basis, CalibratedLook, TERMS};
use aura_render::{calibrated_look::Evaluator, reference_grade::oklab_from_linear_srgb};
use serde::{Deserialize, Serialize};

use super::{check_cancel, content, invalid};

const MAX_SAMPLES: usize = 65_536;
const MIN_SAMPLES: usize = 512;
const ROUNDS: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationReport {
    pub training_samples: usize,
    pub validation_samples: usize,
    /// Mean and p95 Euclidean `OKLab` error * 100, not an accuracy percentage.
    pub mean_error: f32,
    pub p95_error: f32,
    pub occupied_color_cells: usize,
}

#[derive(Clone)]
struct Pair {
    input: [f32; 3],
    output: [f32; 3],
    cell: usize,
    validation: bool,
}

fn lab(rgb: [f32; 3]) -> [f32; 3] {
    oklab_from_linear_srgb(rgb.map(srgb_decode))
}

pub(super) fn pixel_error(one: [f32; 3], other: [f32; 3]) -> f32 {
    let (a, b) = (lab(one), lab(other));
    100.0
        * a.into_iter()
            .zip(b)
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f32>()
            .sqrt()
}

pub(super) fn sample_indices(count: usize, limit: usize) -> impl Iterator<Item = usize> {
    let budget = count.min(limit);
    (0..budget).map(move |k| {
        let begin = k * count / budget;
        let end = (k + 1) * count / budget;
        let mut hash = (k as u64).wrapping_add(0x9e37_79b9_7f4a_7c15);
        hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        hash ^= hash >> 31;
        begin + hash as usize % (end - begin).max(1)
    })
}

fn pairs(original: &Rgb8, edited: &Rgb8) -> AuraResult<Vec<Pair>> {
    if original.width != edited.width || original.height != edited.height {
        return Err(invalid(
            "The original and edited reference must have identical dimensions and framing.",
        ));
    }
    let count = original.width as usize * original.height as usize;
    if original.data.len() != count * 3 || edited.data.len() != count * 3 || count < MIN_SAMPLES {
        return Err(invalid(
            "The reference pair needs at least 512 valid corresponding pixels.",
        ));
    }
    let budget = count.min(MAX_SAMPLES);
    let mut out = Vec::with_capacity(budget);
    // One reproducible, non-periodic pixel in each stratum, including narrow photographs.
    for i in sample_indices(count, budget) {
        let read = |image: &Rgb8| {
            std::array::from_fn(|c| {
                f32::from(image.data.get(3 * i + c).copied().unwrap_or(0)) / 255.0
            })
        };
        let input: [f32; 3] = read(original);
        let bins = input.map(|v| (v * 4.0).floor().min(3.0) as usize);
        let x = i % original.width as usize;
        let y = i / original.width as usize;
        let tile_x = x * 8 / original.width as usize;
        let tile_y = y * 8 / original.height as usize;
        out.push(Pair {
            input,
            output: read(edited),
            cell: (bins[0] * 4 + bins[1]) * 4 + bins[2],
            // Hold out entire spatial tiles, rather than neighbours from every patch.
            validation: (tile_x + 3 * tile_y).is_multiple_of(5),
        });
    }
    Ok(out)
}

/// Pivoted elimination in f64. Positive ridge terms keep poorly supported colours close to
/// the identity prior rather than producing enormous coefficients in an unobserved direction.
fn solve(
    mut matrix: [[f64; TERMS]; TERMS],
    mut rhs: [[f64; 3]; TERMS],
) -> AuraResult<Vec<[f32; 3]>> {
    for column in 0..TERMS {
        let (pivot, divisor) = matrix
            .iter()
            .enumerate()
            .skip(column)
            .filter_map(|(index, row)| row.get(column).map(|value| (index, *value)))
            .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
            .ok_or_else(|| invalid("The reference pair could not be calibrated."))?;
        if divisor.abs() < 1e-14 {
            return Err(invalid(
                "The reference pair has insufficient colour information.",
            ));
        }
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);
        let pivot_row = matrix
            .get_mut(column)
            .ok_or_else(|| invalid("Invalid calibration matrix."))?;
        for value in pivot_row.iter_mut().skip(column) {
            *value /= divisor;
        }
        let pivot_rhs = rhs
            .get_mut(column)
            .ok_or_else(|| invalid("Invalid calibration vector."))?;
        for value in &mut *pivot_rhs {
            *value /= divisor;
        }
        let (pivot_row, pivot_rhs) = (*pivot_row, *pivot_rhs);
        for (index, (row, output)) in matrix.iter_mut().zip(rhs.iter_mut()).enumerate() {
            if index == column {
                continue;
            }
            let factor = row.get(column).copied().unwrap_or(0.0);
            for (value, pivot_value) in row.iter_mut().zip(pivot_row).skip(column) {
                *value -= factor * pivot_value;
            }
            for (value, pivot_value) in output.iter_mut().zip(pivot_rhs) {
                *value -= factor * pivot_value;
            }
        }
    }
    Ok(rhs.into_iter().map(|v| v.map(|x| x as f32)).collect())
}

pub(super) fn fit(
    original: &Rgb8,
    edited: &Rgb8,
    cancel: &CancelToken,
) -> AuraResult<(CalibratedLook, CalibrationReport)> {
    fit_pairs(&[(original, edited)], cancel)
}

/// One look learned from several original/edited pairs at once. Each pair shows the edit on the
/// colours of its own scene; pooled, they show it on more of the colours a batch will contain, so
/// less of the batch depends on how the cubic extrapolates. Every pair must correspond.
pub(super) fn fit_pairs(
    pairs_in: &[(&Rgb8, &Rgb8)],
    cancel: &CancelToken,
) -> AuraResult<(CalibratedLook, CalibrationReport)> {
    check_cancel(cancel)?;
    let mut rows = Vec::new();
    for (original, edited) in pairs_in {
        let these = pairs(original, edited)?;
        if !content::similarity(&content::thumbnail(edited), original).is_some_and(|s| s >= 0.9) {
            return Err(invalid("Choose the original of this exact edited photo, with the same framing and orientation."));
        }
        rows.extend(these);
    }
    let mut cells = [0_usize; 64];
    for row in rows.iter().filter(|row| !row.validation) {
        if let Some(cell) = cells.get_mut(row.cell) {
            *cell += 1;
        }
    }
    let training_samples = cells.iter().sum::<usize>();
    let validation_samples = rows.len() - training_samples;
    if training_samples < MIN_SAMPLES / 2 || validation_samples < 64 {
        return Err(invalid(
            "The reference pair has too few independent validation pixels.",
        ));
    }
    let model = train(&rows, &cells, cancel)?;
    check_cancel(cancel)?;
    let evaluator = Evaluator::new(&model);
    let mut errors: Vec<_> = rows
        .iter()
        .filter(|row| row.validation)
        .map(|row| pixel_error(evaluator.encoded_srgb(row.input), row.output))
        .collect();
    errors.sort_by(f32::total_cmp);
    let mean_error = errors.iter().sum::<f32>() / errors.len().max(1) as f32;
    let p95_error = errors
        .get((errors.len().saturating_sub(1) as f32 * 0.95).round() as usize)
        .copied()
        .unwrap_or(f32::INFINITY);
    if !mean_error.is_finite() || mean_error > 3.0 || p95_error > 8.0 {
        return Err(invalid(format!("This pair cannot be explained reliably by a global colour grade (validation error {mean_error:.2}, p95 {p95_error:.2}). Use the same framing without local edits, or match the edited reference alone.")));
    }
    // A photographic grade may soften or flatten tone; it must not reverse the neutral ramp.
    let mut last = -1.0_f32;
    for i in 0..=256 {
        let mapped = evaluator.encoded_srgb([i as f32 / 256.0; 3]);
        let light = lab(mapped)[0];
        if light + 0.002 < last {
            return Err(invalid("The reference pair implies an unstable tone reversal. Choose a better-exposed original/edited pair."));
        }
        last = light;
    }
    Ok((
        model,
        CalibrationReport {
            training_samples,
            validation_samples,
            mean_error,
            p95_error,
            occupied_color_cells: cells.iter().filter(|&&n| n >= 8).count(),
        },
    ))
}

fn train(rows: &[Pair], cells: &[usize; 64], cancel: &CancelToken) -> AuraResult<CalibratedLook> {
    let mut model = CalibratedLook {
        version: 1,
        coefficients: vec![[0.0; 3]; TERMS],
        amount: 1.0,
    };
    for round in 0..ROUNDS {
        check_cancel(cancel)?;
        let mut matrix = [[0.0_f64; TERMS]; TERMS];
        let mut rhs = [[0.0_f64; 3]; TERMS];
        let mut total = 0.0;
        for (index, row) in rows.iter().filter(|row| !row.validation).enumerate() {
            if index.is_multiple_of(4096) {
                check_cancel(cancel)?;
            }
            let terms = basis(row.input);
            let predicted = model.encoded_srgb(row.input);
            let residual = predicted
                .into_iter()
                .zip(row.output)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f32>()
                .sqrt();
            let robust = if round == 0 {
                1.0
            } else {
                (0.035 / residual.max(0.035)).max(0.02)
            };
            let weight = f64::from(robust)
                / (cells.get(row.cell).copied().unwrap_or(1).max(1) as f64).sqrt();
            total += weight;
            for (i, ((matrix_row, output), &term)) in matrix
                .iter_mut()
                .zip(rhs.iter_mut())
                .zip(&terms)
                .enumerate()
            {
                for (value, &other) in matrix_row.iter_mut().zip(&terms).take(i + 1) {
                    *value += weight * term * other;
                }
                for (value, (out, input)) in
                    output.iter_mut().zip(row.output.into_iter().zip(row.input))
                {
                    *value += weight * term * f64::from(out - input);
                }
            }
        }
        for i in 0..TERMS {
            let source = matrix
                .get(i)
                .copied()
                .ok_or_else(|| invalid("Invalid calibration matrix."))?;
            for (j, row) in matrix.iter_mut().enumerate().take(i) {
                if let Some(value) = row.get_mut(i) {
                    *value = source.get(j).copied().unwrap_or(0.0);
                }
            }
            let ridge = if i == 0 {
                1e-7
            } else if i < 4 {
                1e-6
            } else if i < 10 {
                2e-5
            } else {
                8e-5
            };
            if let Some(value) = matrix.get_mut(i).and_then(|row| row.get_mut(i)) {
                *value += total * ridge;
            }
        }
        model.coefficients = solve(matrix, rhs)?;
        model.validate()?;
    }
    Ok(model)
}

#[cfg(test)]
mod tests;
