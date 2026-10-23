//! Smooth global colour transfer from a measured original/edited reference pair.
use aura_raw::colour::curve::{srgb_decode, srgb_encode};
use aura_recipe::{calibrated_look::CalibratedLook, Recipe};

const EDGE: usize = 17;

/// The cubic is sampled once per render. Trilinear interpolation avoids polynomial ringing
/// between LUT nodes and keeps each output inside the gamut of its eight surrounding values.
#[derive(Debug)]
pub struct Evaluator {
    values: Vec<[f32; 3]>,
}

impl Evaluator {
    #[must_use]
    pub fn new(model: &CalibratedLook) -> Self {
        let values = (0..EDGE * EDGE * EDGE)
            .map(|i| {
                model.encoded_srgb([
                    (i / (EDGE * EDGE)) as f32 / (EDGE - 1) as f32,
                    (i / EDGE % EDGE) as f32 / (EDGE - 1) as f32,
                    (i % EDGE) as f32 / (EDGE - 1) as f32,
                ])
            })
            .collect();
        Self { values }
    }

    #[must_use]
    pub fn encoded_srgb(&self, rgb: [f32; 3]) -> [f32; 3] {
        let position = rgb.map(|v| v.clamp(0.0, 1.0) * (EDGE - 1) as f32);
        let index = position.map(|v| (v as usize).min(EDGE - 2));
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
                    let offset = ((index[0] + r) * EDGE + index[1] + g) * EDGE + index[2] + b;
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
}

pub(crate) fn apply(rgb: &mut [f32], recipe: &Recipe) {
    let Ok(Some(model)) = aura_recipe::calibrated_look::read(recipe) else {
        return;
    };
    if model.amount == 0.0 || model.coefficients.iter().flatten().all(|v| *v == 0.0) {
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
        assert!(crate::graph::engine_string().contains("+calibrated.1"));
        let model = CalibratedLook {
            version: 1,
            coefficients: vec![[0.1; 3]; TERMS],
            amount: 0.0,
        };
        write(&mut recipe, &model).unwrap();
        apply(&mut rgb, &recipe);
        assert_eq!(rgb, before);
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
        };
        model.coefficients[0] = [0.025, 0.01, -0.025];
        model.coefficients[1] = [-0.02, 0.0, 0.0];
        model.coefficients[3] = [0.0, 0.0, -0.02];
        let evaluator = Evaluator::new(&model);
        let mut recipe = aura_recipe::fixtures::reference();
        write(&mut recipe, &model).unwrap();
        let matrix = crate::output::working_to_output(OutputColour::Srgb);
        let inputs = [[0.12, 0.3, 0.6], [0.8, 0.5, 0.2], [0.95; 3]];
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
