//! Robust local tone reconstruction with real, unscaled donor high frequencies.
//! Used only by versioned opt-in heals; never synthesizes a face or random pores.
use super::{curved, quilt, Donor, Image};

#[derive(Debug)]
enum Fit {
    Affine([[f32; 3]; 3]),
    Curved([[f32; 6]; 3]),
}

#[derive(Debug)]
pub(super) struct TextureHeal {
    centre: [f32; 2],
    radius: [f32; 2],
    plane: Fit,
    texture_radius: f32,
    donors: Vec<Donor>,
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
    pub(super) fn new(
        image: Image<'_>,
        source: Donor,
        clean_ring_fit: bool,
        curved_heal: bool,
        heal_samples: &[[f32; 2]],
        texture_sources: &[[f32; 2]],
    ) -> Option<Self> {
        let radius = source.half_extent.map(|r| r / source.scale);
        let donors = quilt::sources(image, source, texture_sources);
        let mut samples = Vec::new();
        let positions: Vec<_> = if heal_samples.is_empty() {
            [1.05, 1.2, 1.35]
                .into_iter()
                .flat_map(|ring| {
                    (0..32).map(move |n| {
                        let angle = std::f32::consts::TAU * n as f32 / 32.0;
                        [angle.cos() * ring, angle.sin() * ring]
                    })
                })
                .collect()
        } else {
            heal_samples.to_vec()
        };
        for uv in positions {
            if let Some(rgb) = image.at(
                source.centre[0] + uv[0] * radius[0],
                source.centre[1] + uv[1] * radius[1],
            ) {
                samples.push(([1.0, uv[0], uv[1]], rgb));
            }
        }
        if samples.len() < 24 {
            return None;
        }
        if curved_heal {
            return Some(Self {
                centre: source.centre,
                radius,
                plane: Fit::Curved(curved::fit(&samples)?),
                donors,
                texture_radius: (source.half_extent[0].min(source.half_extent[1]) * 0.4)
                    .clamp(1.0, 4.0),
            });
        }
        let mut weights = vec![1.0_f32; samples.len()];
        let mut plane = [[0.0_f32; 3]; 3];
        // Joint RGB residual rejects neighbouring dark marks and highlights without
        // independently selecting channels and introducing a coloured patch.
        for _ in 0..if clean_ring_fit { 8 } else { 5 } {
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
            let cutoff = if clean_ring_fit {
                // Joint RGB least-trimmed fitting retains the healthy majority.
                // A median residual times 2.5 can give every neighboring acne
                // sample full weight when a third of the ring is contaminated.
                sorted[sorted.len() * 3 / 5].max(0.002)
            } else {
                (sorted[sorted.len() / 2] * 2.5).max(0.002)
            };
            for (weight, residual) in weights.iter_mut().zip(residuals) {
                *weight = if clean_ring_fit {
                    if residual <= cutoff {
                        1.0
                    } else {
                        0.0
                    }
                } else if residual <= cutoff {
                    1.0
                } else {
                    (cutoff / residual).powi(2)
                };
            }
        }
        Some(Self {
            centre: source.centre,
            radius,
            plane: Fit::Affine(plane),
            donors,
            texture_radius: (source.half_extent[0].min(source.half_extent[1]) * 0.4)
                .clamp(1.0, 4.0),
        })
    }

    pub(super) fn at(&self, image: Image<'_>, source: Donor, x: f32, y: f32) -> Option<[f32; 3]> {
        let high = if self.donors.is_empty() {
            quilt::detail(image, source, x, y, self.texture_radius)?
        } else {
            quilt::blend(image, source, &self.donors, x, y, self.texture_radius)?
        };
        let uv = [
            1.0,
            (x - self.centre[0]) / self.radius[0],
            (y - self.centre[1]) / self.radius[1],
        ];
        Some(std::array::from_fn(|c| {
            let light = match &self.plane {
                Fit::Affine(plane) => (0..3).map(|j| plane[c][j] * uv[j]).sum::<f32>(),
                Fit::Curved(plane) => {
                    let b = curved::basis(uv);
                    (0..6).map(|j| plane[c][j] * b[j]).sum::<f32>()
                }
            };
            (light + high[c]).max(0.0)
        }))
    }
}
