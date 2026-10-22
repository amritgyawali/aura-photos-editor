//! Explicit selection-bound material processing. No inferred anatomy or private models.
use crate::{errors::recipe_invalid, Recipe};
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const KEY: &str = "material_lab_v1";
pub const MAX_OPERATIONS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    DirectionalSmooth,
    SilverHair,
    FabricBands,
    LumaMoire,
    Deband,
    ChromaMedian,
    ImpulseRepair,
    LineScratch,
    Destripe,
    Checkerboard,
    Ringing,
    SpecularChroma,
    ChromaMottle,
    IlluminationPlane,
    ChannelDetail,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    pub id: String,
    pub tool: Tool,
    pub mask_id: String,
    pub enabled: bool,
    /// Blend fraction; zero is exactly neutral.
    pub amount: f32,
    /// Spatial reach as a fraction of the shorter image edge.
    pub radius: f32,
    /// Direction in image-space degrees. Destripe uses the nearest image axis.
    pub angle: f32,
    /// Linear-light residual gate; controls protection of strong structure.
    pub threshold: f32,
    /// Fine-band retention for fabric; replacement detail gain for channel reconstruction.
    pub detail: f32,
    /// Coarse-band retention, independent of fine weave. One leaves that band unchanged.
    pub coarse: f32,
}

/// Validate before persistence and at render admission.
/// # Errors
/// Rejects ambiguous IDs, excessive work, unknown fields and invalid numeric controls.
pub fn validate(operations: &[Operation]) -> AuraResult<()> {
    let bounded = |v: f32, lo: f32, hi: f32| v.is_finite() && (lo..=hi).contains(&v);
    let mut ids = BTreeSet::new();
    if operations.len() > MAX_OPERATIONS
        || operations.iter().any(|op| {
            op.id.is_empty()
                || op.id.len() > 100
                || !ids.insert(&op.id)
                || op.mask_id.is_empty()
                || op.mask_id.len() > 100
                || !bounded(op.amount, 0.0, 1.0)
                || !bounded(op.radius, 0.001, 0.1)
                || !bounded(op.angle, -180.0, 180.0)
                || !bounded(op.threshold, 0.0001, 1.0)
                || !bounded(op.detail, 0.0, 2.0)
                || !bounded(op.coarse, 0.0, 2.0)
        })
    {
        return Err(recipe_invalid(
            KEY,
            "Invalid material operation or selection",
        ));
    }
    Ok(())
}

/// # Errors
/// Refuses malformed or invalid stored operations.
pub fn read(recipe: &Recipe) -> AuraResult<Vec<Operation>> {
    let operations = recipe.extra.get(KEY).map_or_else(
        || Ok(Vec::new()),
        |value| {
            serde_json::from_value(value.clone())
                .map_err(|_| recipe_invalid(KEY, "Invalid material stack"))
        },
    )?;
    validate(&operations)?;
    Ok(operations)
}

/// # Errors
/// Refuses invalid operations or serialization failure without altering the recipe.
pub fn write(recipe: &mut Recipe, operations: &[Operation]) -> AuraResult<()> {
    validate(operations)?;
    let value = serde_json::to_value(operations)
        .map_err(|_| recipe_invalid(KEY, "Cannot encode material stack"))?;
    recipe.extra.insert(KEY.into(), value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn op() -> Operation {
        Operation {
            id: "one".into(),
            mask_id: "mask".into(),
            tool: Tool::Deband,
            enabled: true,
            amount: 0.5,
            radius: 0.01,
            angle: 0.0,
            threshold: 0.04,
            detail: 1.0,
            coarse: 0.25,
        }
    }
    #[test]
    fn refuses_duplicates_nonfinite_and_unbounded_work() {
        assert!(validate(&[op(), op()]).is_err());
        let mut value = op();
        value.amount = f32::NAN;
        assert!(validate(&[value]).is_err());
        let mut value = op();
        value.mask_id.clear();
        assert!(validate(&[value]).is_err());
        let mut value = op();
        value.radius = 1.0;
        assert!(validate(&[value]).is_err());
        assert!(validate(&[op()]).is_ok());
    }
    #[test]
    fn old_recipe_neutral_and_roundtrip_preserves_unrelated_fields() {
        let mut recipe = crate::fixtures::reference();
        assert!(read(&recipe).unwrap().is_empty());
        let original = recipe.clone();
        write(&mut recipe, &[op()]).unwrap();
        assert_eq!(read(&recipe).unwrap(), vec![op()]);
        recipe.extra.remove(KEY);
        assert_eq!(recipe, original);
    }
}
