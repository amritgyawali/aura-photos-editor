//! A bounded, continuous chromatic residual. Coordinates are absolute display OKLab a/b.
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};

use crate::errors::recipe_invalid;

pub const EDGE: usize = 33;
pub const SPAN: f32 = 0.4;
pub const LIMIT: f32 = 0.3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PaletteMap {
    /// Mean colour displacement, separated so warmth/tint controls remain independent.
    pub offset: [f32; 2],
    /// Row-major residual displacements, a varying fastest, on [-SPAN, SPAN]^2.
    pub deltas: Vec<[f32; 2]>,
}

impl PaletteMap {
    #[must_use]
    pub fn coordinate(index: usize) -> [f32; 2] {
        [index % EDGE, index / EDGE].map(|i| -SPAN + 2.0 * SPAN * i as f32 / (EDGE - 1) as f32)
    }

    /// # Errors
    /// Rejects malformed sizes, non-finite values and excessive colour displacements.
    pub fn validate(&self) -> AuraResult<()> {
        if self.deltas.len() != EDGE * EDGE
            || self
                .offset
                .iter()
                .chain(self.deltas.iter().flatten())
                .any(|v| !v.is_finite() || v.abs() > LIMIT + 1e-6)
        {
            return Err(recipe_invalid(super::KEY, "Invalid reference palette map"));
        }
        Ok(())
    }

    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.offset
            .iter()
            .chain(self.deltas.iter().flatten())
            .all(|v| v.abs() <= 1e-5)
    }

    pub fn bound(&mut self) {
        for v in self
            .offset
            .iter_mut()
            .chain(self.deltas.iter_mut().flatten())
        {
            *v = if v.is_finite() {
                v.clamp(-LIMIT, LIMIT)
            } else {
                0.0
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_and_nonfinite_lattices_are_refused() {
        let mut map = PaletteMap {
            offset: [0.0, 0.04],
            deltas: vec![[0.0; 2]; EDGE * EDGE],
        };
        map.validate().unwrap();
        map.deltas[5][0] = f32::NAN;
        assert!(map.validate().is_err());
        map.bound();
        map.validate().unwrap();
        map.deltas.pop();
        assert!(map.validate().is_err());
        assert!(!map.is_identity());
    }
}
