//! Distinguish smooth local illumination from a shadow edge around a spot.
// Samples are a bounded ring with normalized x/y positions and encoded luminance.
#![allow(clippy::indexing_slicing)]

const RING_SAMPLES: usize = 16;
// A contour discontinuity must not be explained away as clean skin. The ring
// must agree with a local plane to within 4% on at least three quarters of it.
const MAX_RELATIVE_RESIDUAL: f32 = 0.04;

fn solve(mut system: [[f32; 4]; 3]) -> Option<[f32; 3]> {
    for col in 0..3 {
        let pivot =
            (col..3).max_by(|&a, &b| system[a][col].abs().total_cmp(&system[b][col].abs()))?;
        system.swap(col, pivot);
        let divisor = system[col][col];
        if divisor.abs() < 1e-6 {
            return None;
        }
        for value in system[col].iter_mut().skip(col) {
            *value /= divisor;
        }
        let pivot_row = system[col];
        for (row, values) in system.iter_mut().enumerate() {
            if row == col {
                continue;
            }
            let factor = values[col];
            for (value, pivot_value) in values.iter_mut().zip(pivot_row).skip(col) {
                *value -= factor * pivot_value;
            }
        }
    }
    Some([system[0][3], system[1][3], system[2][3]])
}

#[cfg(test)]
pub(super) fn consistent(samples: &[[f32; 3]; RING_SAMPLES], centre: f32) -> bool {
    reference_at(samples, centre, RING_SAMPLES * 3 / 4).is_some()
}

pub(super) fn reference(
    samples: &[[f32; 3]; RING_SAMPLES],
    clean: &[bool; RING_SAMPLES],
    centre: f32,
    inflamed: bool,
) -> Option<f32> {
    if inflamed {
        clean_reference(samples, clean, centre)
    } else {
        reference_at(samples, centre, RING_SAMPLES * 3 / 4)
    }
}

/// Strongly red neighboring lesions are not evidence of a lighting edge.
/// Retain at least ten healthy samples and two in each quadrant. The remaining
/// samples must support the same plane to within 4% on 90% of their support.
#[cfg(test)]
pub(super) fn consistent_clean(
    samples: &[[f32; 3]; RING_SAMPLES],
    clean: &[bool; RING_SAMPLES],
    centre: f32,
) -> bool {
    clean_reference(samples, clean, centre).is_some()
}

fn clean_reference(
    samples: &[[f32; 3]; RING_SAMPLES],
    clean: &[bool; RING_SAMPLES],
    centre: f32,
) -> Option<f32> {
    let count = clean.iter().filter(|v| **v).count();
    if count < 10
        || clean
            .chunks(4)
            .any(|q| q.iter().filter(|v| **v).count() < 2)
    {
        return None;
    }
    let mut system = [[0.0; 4]; 3];
    for ([x, y, luma], valid) in samples.iter().zip(clean) {
        if !valid {
            continue;
        }
        let basis = [1.0, *x, *y];
        for row in 0..3 {
            for col in 0..3 {
                system[row][col] += basis[row] * basis[col];
            }
            system[row][3] += basis[row] * luma;
        }
    }
    let plane = solve(system)?;
    let mut residuals: Vec<_> = samples
        .iter()
        .zip(clean)
        .filter(|(_, valid)| **valid)
        .map(|([x, y, luma], _)| (luma - plane[0] - plane[1] * x - plane[2] * y).abs())
        .collect();
    residuals.sort_by(f32::total_cmp);
    (plane[0] >= centre * 0.8
        && plane[0] > plane[1].hypot(plane[2])
        && residuals[count * 9 / 10] <= plane[0] * MAX_RELATIVE_RESIDUAL)
        .then_some(plane[0])
}

fn reference_at(
    samples: &[[f32; 3]; RING_SAMPLES],
    centre: f32,
    residual_index: usize,
) -> Option<f32> {
    let mut weights = [1.0_f32; RING_SAMPLES];
    let mut residuals = [0.0_f32; RING_SAMPLES];
    let mut plane = [0.0_f32; 3];
    // Joint support retains gradients, with bounded influence from a neighboring
    // blemish. Abrupt contour edges cannot fit the same plane around the ring.
    for _ in 0..4 {
        let mut system = [[0.0_f32; 4]; 3];
        for ([x, y, luma], weight) in samples.iter().zip(weights) {
            let basis = [1.0, *x, *y];
            for row in 0..3 {
                for col in 0..3 {
                    system[row][col] += weight * basis[row] * basis[col];
                }
                system[row][3] += weight * basis[row] * luma;
            }
        }
        let fitted = solve(system)?;
        plane = fitted;
        for (residual, [x, y, luma]) in residuals.iter_mut().zip(samples) {
            *residual = (luma - plane[0] - plane[1] * x - plane[2] * y).abs();
        }
        let mut sorted = residuals;
        sorted.sort_by(f32::total_cmp);
        let cutoff = (sorted[RING_SAMPLES / 2] * 2.5).max(0.001);
        for (weight, residual) in weights.iter_mut().zip(residuals) {
            *weight = (cutoff / residual.max(cutoff)).powi(2);
        }
    }
    residuals.sort_by(f32::total_cmp);
    let gradient = plane[1].hypot(plane[2]);
    (plane[0] >= centre * 0.8
        && plane[0] > gradient
        && residuals[residual_index] <= plane[0] * MAX_RELATIVE_RESIDUAL)
        .then_some(plane[0])
}

/// Neutral small marks need context wider than their own pore-sized ring.
/// Read the original illumination even outside the selected repair disk; a
/// feature mask must not hide the neighboring dark side of a nose contour.
pub(super) fn context_consistent(
    luma: &[f32],
    width: usize,
    height: usize,
    point: [f32; 2],
    radius: f32,
    centre: f32,
) -> bool {
    let mut ring = [[0.0; 3]; RING_SAMPLES];
    for (n, sample) in ring.iter_mut().enumerate() {
        let angle = std::f32::consts::TAU * n as f32 / RING_SAMPLES as f32;
        let x = (point[0] + angle.cos() * radius).round();
        let y = (point[1] + angle.sin() * radius).round();
        if x < 0.0 || y < 0.0 || x >= width as f32 || y >= height as f32 {
            return false;
        }
        *sample = [
            (x - point[0]) / radius,
            (y - point[1]) / radius,
            luma[y as usize * width + x as usize],
        ];
    }
    // The wider contour check may ignore one neighboring mark, but not several
    // samples along a shadow boundary that a local repair ring would tolerate.
    reference_at(&ring, centre, RING_SAMPLES * 9 / 10).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(f: impl Fn(f32, f32) -> f32) -> [[f32; 3]; RING_SAMPLES] {
        std::array::from_fn(|n| {
            let angle = std::f32::consts::TAU * n as f32 / RING_SAMPLES as f32;
            let (y, x) = angle.sin_cos();
            [x, y, f(x, y)]
        })
    }

    #[test]
    fn smooth_light_and_one_neighboring_mark_are_compatible() {
        let mut samples = ring(|x, y| 0.4 + x * 0.15 + y * 0.03);
        assert!(consistent(&samples, 0.3));
        samples[2][2] *= 0.6;
        assert!(consistent(&samples, 0.3));
    }

    #[test]
    fn sharp_and_curved_shadow_edges_are_rejected() {
        assert!(!consistent(
            &ring(|x, _| if x > 0.0 { 0.5 } else { 0.2 }),
            0.3
        ));
        assert!(!consistent(
            &ring(|x, y| if x + y * y * 0.5 > 0.0 { 0.5 } else { 0.2 }),
            0.3
        ));
        assert!(!consistent(&ring(|_, _| 0.2), 0.5));
    }
    #[test]
    fn clean_support_handles_neighboring_acne_but_not_one_sided_or_shadow_support() {
        let mut samples = ring(|x, y| 0.4 + x * 0.05 + y * 0.02);
        let mut clean = [true; 16];
        for i in [0, 1, 7, 8, 9, 15] {
            samples[i][2] *= 0.5;
            clean[i] = false;
        }
        assert!(!consistent(&samples, 0.3));
        assert!(consistent_clean(&samples, &clean, 0.3));
        clean[2] = false;
        assert!(!consistent_clean(&samples, &clean, 0.3));
        assert!(!consistent_clean(
            &ring(|x, _| if x > 0.0 { 0.5 } else { 0.2 }),
            &[true; 16],
            0.3
        ));
    }
}
