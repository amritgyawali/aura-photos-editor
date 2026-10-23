//! A global RGB transform learned from an aligned original/edited reference pair.
//!
//! Coefficients describe a cubic residual in encoded display sRGB, optionally supplemented by
//! a bounded local residual lattice. The renderer changes coordinates into that domain and
//! back; recipes and the working buffer remain scene linear. Version 1 keeps its old encoding.
use aura_core::{errors::render::recipe_invalid, AuraResult};
use serde::{Deserialize, Serialize};

use crate::Recipe;

pub const KEY: &str = "calibrated_look_v1";
pub const TERMS: usize = 20;

mod lattice;
pub use lattice::{tetrahedral_stencil, ResidualLut, RESIDUAL_EDGE, RESIDUAL_NODES};

/// Total-degree cubic basis, centred to keep the normal equations well conditioned.
#[must_use]
pub fn basis(rgb: [f32; 3]) -> [f64; TERMS] {
    let [r, g, b] = rgb.map(|v| 2.0 * f64::from(v) - 1.0);
    [
        1.0,
        r,
        g,
        b,
        r * r,
        r * g,
        r * b,
        g * g,
        g * b,
        b * b,
        r * r * r,
        r * r * g,
        r * r * b,
        r * g * g,
        r * g * b,
        r * b * b,
        g * g * g,
        g * g * b,
        g * b * b,
        b * b * b,
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CalibratedLook {
    pub version: u32,
    pub coefficients: Vec<[f32; 3]>,
    pub amount: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub residual_lut: Option<ResidualLut>,
}

impl CalibratedLook {
    /// # Errors
    /// Rejects unsupported, oversized, non-finite or unbounded transforms.
    pub fn validate(&self) -> AuraResult<()> {
        if !matches!(
            (self.version, self.residual_lut.is_some()),
            (1, false) | (2, true)
        ) || self.coefficients.len() != TERMS
            || !self.amount.is_finite()
            || !(0.0..=1.0).contains(&self.amount)
            || self
                .coefficients
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 8.0)
        {
            return Err(recipe_invalid(
                KEY,
                "Invalid calibrated reference transform",
            ));
        }
        if let Some(lut) = &self.residual_lut {
            lut.validate()?;
        }
        Ok(())
    }

    /// Apply the shared mapping without changing the input image's geometry or detail.
    #[must_use]
    pub fn encoded_srgb(&self, rgb: [f32; 3]) -> [f32; 3] {
        if self.amount == 0.0 {
            return rgb;
        }
        let correction = self.correction(rgb);
        std::array::from_fn(|channel| {
            let input = rgb.get(channel).copied().unwrap_or(0.0);
            let full = (input + correction.get(channel).copied().unwrap_or(0.0)).clamp(0.0, 1.0);
            input + self.amount * (full - input)
        })
    }

    /// Full-strength RGB residual before gamut clamping, shared by fitting and rendering.
    #[must_use]
    pub fn correction(&self, rgb: [f32; 3]) -> [f32; 3] {
        let terms = basis(rgb);
        let local = self
            .residual_lut
            .as_ref()
            .map_or([0.0; 3], |lut| lut.delta(rgb));
        std::array::from_fn(|channel| {
            let delta: f64 = terms
                .iter()
                .zip(&self.coefficients)
                .map(|(term, coefficient)| {
                    term * f64::from(coefficient.get(channel).copied().unwrap_or(0.0))
                })
                .sum();
            delta as f32 + local.get(channel).copied().unwrap_or(0.0)
        })
    }

    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.coefficients.iter().flatten().all(|v| *v == 0.0)
            && self
                .residual_lut
                .as_ref()
                .is_none_or(|lut| lut.values.iter().flatten().all(|v| *v == 0.0))
    }
}

/// # Errors
/// Rejects a malformed stored transform.
pub fn read(recipe: &Recipe) -> AuraResult<Option<CalibratedLook>> {
    recipe
        .extra
        .get(KEY)
        .map(|value| {
            let model: CalibratedLook = serde_json::from_value(value.clone())
                .map_err(|_| recipe_invalid(KEY, "Cannot decode calibrated reference"))?;
            model.validate()?;
            Ok(model)
        })
        .transpose()
}

/// # Errors
/// Validates before changing the recipe. Zero strength removes the transform.
pub fn write(recipe: &mut Recipe, model: &CalibratedLook) -> AuraResult<()> {
    model.validate()?;
    if model.amount == 0.0 || model.is_identity() {
        recipe.extra.remove(KEY);
    } else {
        let value = serde_json::to_value(model)
            .map_err(|_| recipe_invalid(KEY, "Cannot encode calibrated reference"))?;
        recipe.extra.insert(KEY.into(), value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_models_cannot_change_a_recipe_and_zero_removes_the_look() {
        let mut recipe = crate::fixtures::reference();
        let mut model = CalibratedLook {
            version: 1,
            coefficients: vec![[0.0; 3]; TERMS],
            amount: 1.0,
            residual_lut: None,
        };
        model.coefficients[0] = [0.02, 0.0, -0.03];
        write(&mut recipe, &model).unwrap();
        assert_eq!(read(&recipe).unwrap(), Some(model.clone()));
        let saved = recipe.clone();
        model.coefficients[2][1] = f32::NAN;
        assert!(write(&mut recipe, &model).is_err());
        assert_eq!(recipe, saved);
        model.coefficients[2][1] = 0.0;
        model.amount = 0.0;
        assert_eq!(
            model.encoded_srgb([0.9, 0.2, 0.1]).map(f32::to_bits),
            [0.9_f32, 0.2, 0.1].map(f32::to_bits)
        );
        write(&mut recipe, &model).unwrap();
        assert!(read(&recipe).unwrap().is_none());
    }

    #[test]
    fn automated_merge_can_remove_the_look_but_preserves_a_protected_look() {
        let mut current = crate::fixtures::reference();
        let mut model = CalibratedLook {
            version: 1,
            coefficients: vec![[0.0; 3]; TERMS],
            amount: 1.0,
            residual_lut: None,
        };
        model.coefficients[0] = [0.02; 3];
        write(&mut current, &model).unwrap();
        let mut proposal = current.clone();
        proposal.extra.remove(KEY);
        let (cleared, _) =
            crate::schema::merge(&current, &proposal, crate::EditSource::Ai).unwrap();
        assert!(read(&cleared).unwrap().is_none());
        current.provenance.user_edited_fields.push(KEY.into());
        let (kept, report) =
            crate::schema::merge(&current, &proposal, crate::EditSource::Ai).unwrap();
        assert_eq!(read(&kept).unwrap(), Some(model));
        assert!(!report.refused.is_empty());
    }

    #[test]
    fn residual_only_models_round_trip_and_old_models_keep_their_encoding() {
        let old = CalibratedLook {
            version: 1,
            coefficients: vec![[0.0; 3]; TERMS],
            amount: 1.0,
            residual_lut: None,
        };
        let json = serde_json::to_string(&old).unwrap();
        assert!(!json.contains("residualLut"));
        assert_eq!(serde_json::from_str::<CalibratedLook>(&json).unwrap(), old);
        let mut model = old.clone();
        model.version = 2;
        model.residual_lut = Some(ResidualLut {
            values: vec![[0.02, -0.01, 0.03]; RESIDUAL_NODES],
        });
        let mut recipe = crate::fixtures::reference();
        write(&mut recipe, &model).unwrap();
        assert_eq!(read(&recipe).unwrap(), Some(model.clone()));
        let result = model.encoded_srgb([0.3, 0.4, 0.5]);
        for (a, b) in result.into_iter().zip([0.32, 0.39, 0.53]) {
            assert!((a - b).abs() < 1e-6);
        }
        model.residual_lut.as_mut().unwrap().values[2][1] = f32::NAN;
        assert!(write(&mut recipe, &model).is_err());
        assert!(read(&recipe).unwrap().is_some());
    }
}
