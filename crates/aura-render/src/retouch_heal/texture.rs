//! Robust local tone reconstruction with real, unscaled donor high frequencies.
//! Used only by versioned opt-in heals; never synthesizes a face or random pores.
use super::{difference, Donor, Image};

#[derive(Debug)]
pub(super) struct TextureHeal {
    centre: [f32; 2],
    radius: [f32; 2],
    plane: [[f32; 3]; 3],
    texture_radius: f32,
}

fn solve(mut a: [[f32; 4]; 3]) -> Option<[f32; 3]> {
    for col in 0..3 {
        let pivot = (col..3).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
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
            if row != col {
                let factor = values[col];
                for (value, pivot_value) in values.iter_mut().zip(pivot_row).skip(col) {
                    *value -= factor * pivot_value;
                }
            }
        }
    }
    Some([a[0][3], a[1][3], a[2][3]])
}

impl TextureHeal {
    pub(super) fn new(image: Image<'_>, source: Donor) -> Option<Self> {
        let radius = source.half_extent.map(|r| r / source.scale);
        let mut samples = Vec::new();
        for ring in [1.05, 1.2, 1.35] {
            for n in 0..32 {
                let angle = std::f32::consts::TAU * n as f32 / 32.0;
                let uv = [angle.cos() * ring, angle.sin() * ring];
                if let Some(rgb) = image.at(
                    source.centre[0] + uv[0] * radius[0],
                    source.centre[1] + uv[1] * radius[1],
                ) {
                    samples.push(([1.0, uv[0], uv[1]], rgb));
                }
            }
        }
        if samples.len() < 24 {
            return None;
        }
        let mut weights = vec![1.0_f32; samples.len()];
        let mut plane = [[0.0_f32; 3]; 3];
        // Joint RGB residual rejects neighbouring dark marks and highlights without
        // independently selecting channels and introducing a coloured patch.
        for _ in 0..5 {
            for c in 0..3 {
                let mut system = [[0.0; 4]; 3];
                for ((uv, rgb), weight) in samples.iter().zip(&weights) {
                    for row in 0..3 {
                        for col in 0..3 {
                            system[row][col] += weight * uv[row] * uv[col];
                        }
                        system[row][3] += weight * uv[row] * rgb[c];
                    }
                }
                plane[c] = solve(system)?;
            }
            let residuals: Vec<f32> = samples
                .iter()
                .map(|(uv, rgb)| {
                    (0..3)
                        .map(|c| {
                            (rgb[c] - (0..3).map(|j| plane[c][j] * uv[j]).sum::<f32>()).powi(2)
                        })
                        .sum::<f32>()
                        .sqrt()
                })
                .collect();
            let mut sorted = residuals.clone();
            sorted.sort_by(f32::total_cmp);
            let cutoff = (sorted[sorted.len() / 2] * 2.5).max(0.002);
            for (weight, residual) in weights.iter_mut().zip(residuals) {
                *weight = if residual <= cutoff {
                    1.0
                } else {
                    (cutoff / residual).powi(2)
                };
            }
        }
        Some(Self {
            centre: source.centre,
            radius,
            plane,
            texture_radius: (source.half_extent[0].min(source.half_extent[1]) * 0.4)
                .clamp(1.0, 4.0),
        })
    }

    pub(super) fn at(&self, image: Image<'_>, source: Donor, x: f32, y: f32) -> Option<[f32; 3]> {
        let donor = source.texture_at(image, x, y)?;
        let mut low = [0.0; 3];
        let mut count = 0.0;
        for dy in -2..=2 {
            for dx in -2..=2 {
                // Filtering also stays inside the clean donor footprint. Sampling
                // beyond it could reintroduce a nearby defect into the high band.
                if let Some(p) = source.texture_at(
                    image,
                    x + dx as f32 * self.texture_radius * 0.5,
                    y + dy as f32 * self.texture_radius * 0.5,
                ) {
                    for c in 0..3 {
                        low[c] += p[c];
                    }
                    count += 1.0;
                }
            }
        }
        let high = difference(donor, low.map(|v| v / count));
        let uv = [
            1.0,
            (x - self.centre[0]) / self.radius[0],
            (y - self.centre[1]) / self.radius[1],
        ];
        Some(std::array::from_fn(|c| {
            ((0..3).map(|j| self.plane[c][j] * uv[j]).sum::<f32>() + high[c]).max(0.0)
        }))
    }
}
