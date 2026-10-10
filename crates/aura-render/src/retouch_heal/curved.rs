//! Versioned quadratic RGB illumination fitting for curved skin surfaces.
#![allow(clippy::indexing_slicing)]
pub(super) fn basis([_, x, y]: [f32; 3]) -> [f32; 6] {
    [1.0, x, y, x * x, x * y, y * y]
}

pub(super) fn solve(mut a: [[f32; 7]; 6]) -> Option<[f32; 6]> {
    for col in 0..6 {
        let pivot = (col..6).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        a.swap(col, pivot);
        let divisor = a[col][col];
        if divisor.abs() < 1e-6 {
            return None;
        }
        for value in a[col].iter_mut().skip(col) {
            *value /= divisor;
        }
        let pivot_row = a[col];
        for (row, values) in a.iter_mut().enumerate() {
            if row == col {
                continue;
            }
            let factor = values[col];
            for (value, pivot_value) in values.iter_mut().zip(pivot_row).skip(col) {
                *value -= factor * pivot_value;
            }
        }
    }
    Some(std::array::from_fn(|i| a[i][6]))
}

pub(super) fn fit(samples: &[([f32; 3], [f32; 3])]) -> Option<[[f32; 6]; 3]> {
    let mut weights = vec![1.0_f32; samples.len()];
    let mut plane = [[0.0; 6]; 3];
    for _ in 0..8 {
        for c in 0..3 {
            let mut system = [[0.0; 7]; 6];
            for ((uv, rgb), weight) in samples.iter().zip(&weights) {
                let b = basis(*uv);
                for row in 0..6 {
                    for col in 0..6 {
                        system[row][col] += weight * b[row] * b[col];
                    }
                    system[row][6] += weight * b[row] * rgb[c];
                }
            }
            plane[c] = solve(system)?;
        }
        let residuals: Vec<_> = samples
            .iter()
            .map(|(uv, rgb)| {
                let b = basis(*uv);
                (0..3)
                    .map(|c| (rgb[c] - (0..6).map(|j| plane[c][j] * b[j]).sum::<f32>()).powi(2))
                    .sum::<f32>()
                    .sqrt()
            })
            .collect();
        let mut sorted = residuals.clone();
        sorted.sort_by(f32::total_cmp);
        let cutoff = sorted[sorted.len() * 3 / 5].max(0.002);
        for (weight, residual) in weights.iter_mut().zip(residuals) {
            *weight = if residual <= cutoff { 1.0 } else { 0.0 };
        }
    }
    Some(plane)
}
