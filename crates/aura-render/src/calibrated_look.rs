//! Smooth global colour transfer from a measured original/edited reference pair.
use aura_raw::colour::curve::{srgb_decode, srgb_encode};
use aura_recipe::{
    calibrated_look::{tetrahedral_stencil, CalibratedLook},
    Recipe,
};

const LEGACY_EDGE: usize = 17;
const ADAPTIVE_EDGE: usize = 33;

/// The model is sampled once per render. Version 1 retains its original 17-cubed trilinear
/// path. Adaptive models use 33-cubed tetrahedral interpolation, including the neutral diagonal.
/// Both interpolate bounded colours and cannot overshoot their surrounding sampled values.
#[derive(Debug)]
pub struct Evaluator {
    values: Vec<[f32; 3]>,
    edge: usize,
    tetrahedral: bool,
}

impl Evaluator {
    #[must_use]
    pub fn new(model: &CalibratedLook) -> Self {
        let tetrahedral = model.residual_lut.is_some();
        let edge = if tetrahedral {
            ADAPTIVE_EDGE
        } else {
            LEGACY_EDGE
        };
        let values = (0..edge * edge * edge)
            .map(|i| {
                model.encoded_srgb([
                    (i / (edge * edge)) as f32 / (edge - 1) as f32,
                    (i / edge % edge) as f32 / (edge - 1) as f32,
                    (i % edge) as f32 / (edge - 1) as f32,
                ])
            })
            .collect();
        Self {
            values,
            edge,
            tetrahedral,
        }
    }

    #[must_use]
    pub fn encoded_srgb(&self, rgb: [f32; 3]) -> [f32; 3] {
        if self.tetrahedral {
            let mut result = [0.0; 3];
            for (index, weight) in tetrahedral_stencil(rgb, self.edge) {
                if let Some(value) = self.values.get(index) {
                    for (out, &v) in result.iter_mut().zip(value) {
                        *out += weight * v;
                    }
                }
            }
            return result;
        }
        let position = rgb.map(|v| v.clamp(0.0, 1.0) * (self.edge - 1) as f32);
        let index = position.map(|v| (v as usize).min(self.edge - 2));
        let fraction = std::array::from_fn::<_, 3, _>(|i| {
            position.get(i).copied().unwrap_or(0.0) - index.get(i).copied().unwrap_or(0) as f32
        });
        let mut out = [0.0; 3];
        for r in 0..2 {
            for g in 0..2 {
                for b in 0..2 {
                    let choices = [r, g, b];
                    let weight: f32 = choices
                        .iter()
                        .zip(fraction)
                        .map(|(side, t)| if *side == 0 { 1.0 - t } else { t })
                        .product();
                    let offset =
                        ((index[0] + r) * self.edge + index[1] + g) * self.edge + index[2] + b;
                    if let Some(value) = self.values.get(offset) {
                        for (slot, v) in out.iter_mut().zip(value) {
                            *slot += weight * v;
                        }
                    }
                }
            }
        }
        out
    }

    /// An upper bound on the infinity-norm RGB noise gain of this actual lookup table.
    /// Tetrahedral cells are affine: each tetrahedron's row-sum Jacobian norm is exact.
    /// Trilinear cells use the sum of directional edge maxima, a conservative bound.
    #[must_use]
    pub fn max_noise_gain(&self) -> f32 {
        let mut maximum = 0.0_f32;
        for r in 0..self.edge - 1 {
            for g in 0..self.edge - 1 {
                for b in 0..self.edge - 1 {
                    let corners = std::array::from_fn::<_, 8, _>(|corner| {
                        let index = ((r + (corner >> 2 & 1)) * self.edge + g + (corner >> 1 & 1))
                            * self.edge
                            + b
                            + (corner & 1);
                        self.values.get(index).copied().unwrap_or([f32::NAN; 3])
                    });
                    let gain = cube_noise_gain(&corners, self.tetrahedral) * (self.edge - 1) as f32;
                    if !gain.is_finite() {
                        return f32::INFINITY;
                    }
                    maximum = maximum.max(gain);
                }
            }
        }
        maximum
    }
}

fn cube_noise_gain(corners: &[[f32; 3]; 8], tetrahedral: bool) -> f32 {
    if corners.iter().flatten().any(|v| !v.is_finite()) {
        return f32::INFINITY;
    }
    let at = |i| corners.get(i).copied().unwrap_or([0.0; 3]);
    if tetrahedral {
        let mut maximum = 0.0_f32;
        for path in [
            [0, 4, 6, 7],
            [0, 4, 5, 7],
            [0, 2, 6, 7],
            [0, 2, 3, 7],
            [0, 1, 5, 7],
            [0, 1, 3, 7],
        ] {
            let mut sum = [0.0; 3];
            for pair in path.windows(2) {
                if let &[a, b] = pair {
                    for (out, (a, b)) in sum.iter_mut().zip(at(a).into_iter().zip(at(b))) {
                        *out += (a - b).abs();
                    }
                }
            }
            maximum = sum.into_iter().fold(maximum, f32::max);
        }
        maximum
    } else {
        let mut sum = [0.0; 3];
        for stride in [4, 2, 1] {
            let mut axis = [0.0_f32; 3];
            for i in 0..8 {
                if i & stride == 0 {
                    for (out, (a, b)) in axis.iter_mut().zip(at(i).into_iter().zip(at(i + stride)))
                    {
                        *out = out.max((a - b).abs());
                    }
                }
            }
            for (out, gain) in sum.iter_mut().zip(axis) {
                *out += gain;
            }
        }
        sum.into_iter().fold(0.0, f32::max)
    }
}

pub(crate) fn apply(rgb: &mut [f32], recipe: &Recipe) {
    let Ok(Some(model)) = aura_recipe::calibrated_look::read(recipe) else {
        return;
    };
    if model.amount == 0.0 || model.is_identity() {
        return;
    }
    let evaluator = Evaluator::new(&model);
    crate::reference_grade::map_display(rgb, |linear| {
        // An invertible coordinate change inside the stage; the output transform still owns
        // the only baked transfer function. This also matches the calibration pixel domain.
        evaluator
            .encoded_srgb(linear.map(srgb_encode))
            .map(srgb_decode)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_recipe::calibrated_look::{write, TERMS};

    #[test]
    fn lookup_reproduces_an_affine_grade_and_interpolates_strength() {
        let mut model = CalibratedLook {
            version: 1,
            coefficients: vec![[0.0; 3]; TERMS],
            amount: 0.5,
            residual_lut: None,
        };
        model.coefficients[0] = [0.04, 0.02, -0.04];
        let evaluator = Evaluator::new(&model);
        let rgb = [0.32, 0.43, 0.71];
        let actual = evaluator.encoded_srgb(rgb);
        let expected = model.encoded_srgb(rgb);
        for (a, b) in actual.into_iter().zip(expected) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn absent_and_zero_transforms_keep_exact_working_pixels() {
        let mut recipe = aura_recipe::fixtures::reference();
        let mut rgb = vec![0.2, 0.4, 0.6, 8.0, 6.0, 4.0];
        let before = rgb.clone();
        apply(&mut rgb, &recipe);
        assert_eq!(rgb, before);
        assert_eq!(
            crate::graph::stage_for("calibrated_look_v1.amount"),
            Some(crate::graph::Stage::Sharpen)
        );
        assert!(crate::graph::engine_string().contains("+calibrated.2"));
        let model = CalibratedLook {
            version: 1,
            coefficients: vec![[0.1; 3]; TERMS],
            amount: 0.0,
            residual_lut: None,
        };
        write(&mut recipe, &model).unwrap();
        apply(&mut rgb, &recipe);
        assert_eq!(rgb, before);
    }

    #[test]
    fn adaptive_residuals_preserve_explicit_strength_and_neutral_colour() {
        use aura_recipe::calibrated_look::{ResidualLut, RESIDUAL_EDGE, RESIDUAL_NODES};
        let mut model = CalibratedLook {
            version: 2,
            coefficients: vec![[0.0; 3]; TERMS],
            amount: 1.0,
            residual_lut: Some(ResidualLut {
                values: (0..RESIDUAL_NODES)
                    .map(|i| {
                        let [r, g, b] = [
                            i / (RESIDUAL_EDGE * RESIDUAL_EDGE),
                            i / RESIDUAL_EDGE % RESIDUAL_EDGE,
                            i % RESIDUAL_EDGE,
                        ]
                        .map(|n| n as f32 / (RESIDUAL_EDGE - 1) as f32);
                        [0.05 * (r - g), 0.03 * (g - b), 0.04 * (b - r)]
                    })
                    .collect(),
            }),
        };
        let full = Evaluator::new(&model);
        assert!((full.max_noise_gain() - 1.1).abs() < 1e-5);
        model.amount = 0.5;
        let half = Evaluator::new(&model);
        for rgb in [[0.22, 0.43, 0.71], [0.4; 3], [0.91, 0.72, 0.25]] {
            let expected = model.encoded_srgb(rgb);
            for ((a, b), (input, full)) in half
                .encoded_srgb(rgb)
                .into_iter()
                .zip(expected)
                .zip(rgb.into_iter().zip(full.encoded_srgb(rgb)))
            {
                assert!((a - b).abs() < 1e-6);
                assert!((a - (input + 0.5 * (full - input))).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn working_stage_and_display_evaluator_agree_including_highlights() {
        use crate::contract::render::OutputColour;
        use aura_raw::colour::{
            curve::{filmic_lite, filmic_lite_inverse},
            working_space::linear_srgb_to_working_f32,
        };
        let mut model = CalibratedLook {
            version: 1,
            coefficients: vec![[0.0; 3]; TERMS],
            amount: 1.0,
            residual_lut: None,
        };
        model.coefficients[0] = [0.025, 0.01, -0.025];
        model.coefficients[1] = [-0.02, 0.0, 0.0];
        model.coefficients[3] = [0.0, 0.0, -0.02];
        let mut adaptive = model.clone();
        adaptive.version = 2;
        adaptive.residual_lut = Some(aura_recipe::calibrated_look::ResidualLut {
            values: vec![[0.01, -0.005, 0.015]; aura_recipe::calibrated_look::RESIDUAL_NODES],
        });
        let matrix = crate::output::working_to_output(OutputColour::Srgb);
        let inputs = [[0.12, 0.3, 0.6], [0.8, 0.5, 0.2], [0.95; 3]];
        for selected in [model, adaptive] {
            let evaluator = Evaluator::new(&selected);
            let mut recipe = aura_recipe::fixtures::reference();
            write(&mut recipe, &selected).unwrap();
            for display in inputs {
                let working = linear_srgb_to_working_f32(display).map(filmic_lite_inverse);
                let mut buffer = working.to_vec();
                apply(&mut buffer, &recipe);
                let actual = crate::colour::apply_f32(
                    matrix,
                    [buffer[0], buffer[1], buffer[2]].map(filmic_lite),
                );
                let expected = evaluator
                    .encoded_srgb(display.map(srgb_encode))
                    .map(srgb_decode);
                for (a, b) in actual.into_iter().zip(expected) {
                    assert!((a - b).abs() < 3e-4, "{actual:?} / {expected:?}");
                }
            }
        }
    }
}
