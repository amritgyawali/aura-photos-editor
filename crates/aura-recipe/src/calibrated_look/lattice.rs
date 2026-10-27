//! Bounded local residuals supplementing the global polynomial colour model.
use aura_core::{errors::render::recipe_invalid, AuraResult};
use serde::{Deserialize, Serialize};

use super::KEY;

pub const RESIDUAL_EDGE: usize = 9;
pub const RESIDUAL_NODES: usize = RESIDUAL_EDGE * RESIDUAL_EDGE * RESIDUAL_EDGE;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResidualLut {
    /// RGB residuals on an evenly spaced 9-cubed lattice, blue varying fastest.
    pub values: Vec<[f32; 3]>,
}

/// Four barycentric weights inside a tetrahedron of a regular RGB lattice. Equal fractions
/// use a deterministic axis order; the neutral diagonal uses only its two diagonal endpoints.
/// Edge sizes are clamped to 2..=65 to keep indices bounded for public callers.
#[must_use]
pub fn tetrahedral_stencil(rgb: [f32; 3], edge: usize) -> [(usize, f32); 4] {
    let edge = edge.clamp(2, 65);
    let p = rgb.map(|v| v.clamp(0.0, 1.0) * (edge - 1) as f32);
    let base = p.map(|v| (v as usize).min(edge - 2));
    let fraction = [
        p[0] - base[0] as f32,
        p[1] - base[1] as f32,
        p[2] - base[2] as f32,
    ];
    let mut order = [(fraction[0], 0_usize), (fraction[1], 1), (fraction[2], 2)];
    order.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let [(hi, first_axis), (mid, second_axis), (lo, _)] = order;
    let mut first = base;
    if let Some(v) = first.get_mut(first_axis) {
        *v += 1;
    }
    let mut second = first;
    if let Some(v) = second.get_mut(second_axis) {
        *v += 1;
    }
    let index = |[r, g, b]: [usize; 3]| (r * edge + g) * edge + b;
    [
        (index(base), 1.0 - hi),
        (index(first), hi - mid),
        (index(second), mid - lo),
        (index(base.map(|v| v + 1)), lo),
    ]
}

impl ResidualLut {
    /// # Errors
    /// Rejects wrong dimensions, non-finite corrections and excessively large residuals.
    pub fn validate(&self) -> AuraResult<()> {
        if self.values.len() != RESIDUAL_NODES
            || self
                .values
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 0.35)
        {
            return Err(recipe_invalid(
                KEY,
                "Invalid calibrated reference residual lattice",
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn delta(&self, rgb: [f32; 3]) -> [f32; 3] {
        let mut result = [0.0; 3];
        for (index, weight) in tetrahedral_stencil(rgb, RESIDUAL_EDGE) {
            if let Some(value) = self.values.get(index) {
                for (out, &v) in result.iter_mut().zip(value) {
                    *out += weight * v;
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_reproduces_affine_colours_in_all_six_orderings_and_at_boundaries() {
        let operation =
            |[r, g, b]: [f32; 3]| [0.2 + 0.4 * r + 0.1 * g, 0.1 + 0.2 * g + 0.3 * b, 0.5 * b];
        for rgb in [
            [0.27, 0.21, 0.16],
            [0.27, 0.16, 0.21],
            [0.21, 0.27, 0.16],
            [0.16, 0.27, 0.21],
            [0.21, 0.16, 0.27],
            [0.16, 0.21, 0.27],
            [0.0; 3],
            [1.0; 3],
            [0.0, 1.0, 0.4],
        ] {
            let mut output = [0.0; 3];
            let mut mass = 0.0;
            for (index, weight) in tetrahedral_stencil(rgb, RESIDUAL_EDGE) {
                assert!(weight >= 0.0 && index < RESIDUAL_NODES);
                mass += weight;
                let node = [index / 81, index / 9 % 9, index % 9].map(|i| i as f32 / 8.0);
                for (out, v) in output.iter_mut().zip(operation(node)) {
                    *out += weight * v;
                }
            }
            assert!((mass - 1.0).abs() < 1e-6);
            for (a, b) in output.into_iter().zip(operation(rgb)) {
                assert!((a - b).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn neutral_diagonal_never_picks_up_unrelated_surrounding_colours() {
        let mut lut = ResidualLut {
            values: vec![[0.3, -0.2, 0.1]; RESIDUAL_NODES],
        };
        for i in 0..RESIDUAL_EDGE {
            lut.values[(i * RESIDUAL_EDGE + i) * RESIDUAL_EDGE + i] =
                [0.02 * (i as f32 / 8.0).powi(2); 3];
        }
        let expected = 0.02
            * (0.25_f32.powi(2) + (0.31 - 0.25) / 0.125 * (0.375_f32.powi(2) - 0.25_f32.powi(2)));
        for value in lut.delta([0.31; 3]) {
            assert!((value - expected).abs() < 1e-7);
        }
    }
}
