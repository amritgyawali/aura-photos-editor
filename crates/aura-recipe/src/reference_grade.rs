//! A colour and tone transfer fitted from an edited reference photograph (ADR-0160).
//!
//! The recipe carries the fitted transform and never the reference: a tone curve over OKLab
//! lightness, the neutral axis this photograph had and the one it should have at nine
//! lightnesses (white balance and split toning in one shape), a chroma curve, twelve hue
//! sectors of saturation, hue and lightness, and five lightness zones of chroma. Every field is
//! a measurement-sized number list, so the four values a delivered file is re-created from
//! still decide every pixel. `aura-render` executes it as one stage; this module only holds,
//! bounds and stores it.
//!
//! The transform is defined on *display* colour, because that is where an edited reference was
//! judged and measured. The renderer maps each working-space pixel there through the output
//! transform's own functions and back again inside the stage.
use crate::{errors::recipe_invalid, Recipe};
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};

pub const KEY: &str = "reference_grade_v1";
/// Tone curve nodes, evenly spaced over input OKLab lightness `0..=1`.
pub const TONE_NODES: usize = 33;
/// Neutral-axis nodes, evenly spaced over OKLab lightness `0..=1`.
pub const AXIS_NODES: usize = 9;
/// Chroma curve nodes, evenly spaced over `0..=CHROMA_SPAN` of input chroma.
pub const CHROMA_NODES: usize = 17;
/// Hue sectors, centred every thirty degrees of OKLab hue starting at zero.
pub const SECTORS: usize = 12;
/// Chroma-gain zones, evenly spaced over output OKLab lightness `0..=1`.
pub const ZONES: usize = 5;
/// OKLab chroma covered by the chroma curve. The most saturated sRGB primary is about 0.32;
/// beyond the span the curve continues with its last slope.
pub const CHROMA_SPAN: f32 = 0.4;
/// The largest neutral-axis offset either side may carry, in OKLab `a`/`b` units. A strong
/// tungsten cast measures about 0.05; twice that is a deliberate colour wash.
pub const MAX_CAST: f32 = 0.12;
/// The largest per-sector hue rotation, in radians (about 34 degrees).
pub const MAX_HUE_SHIFT: f32 = 0.6;
/// The largest per-sector lightness offset, in OKLab `L` units.
pub const MAX_HUE_LIGHT: f32 = 0.12;
/// Chroma multipliers per sector and per zone are bounded to this range. Zero is allowed so
/// a reference that removes one colour entirely can be followed.
pub const MAX_GAIN: f32 = 4.0;
/// The chroma curve may not exceed this multiple of the span at its last node.
pub const MAX_CHROMA_OUT: f32 = 3.0 * CHROMA_SPAN;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReferenceGrade {
    /// Output lightness at input lightness `i / (TONE_NODES - 1)`; non-decreasing.
    pub tone: Vec<f32>,
    /// The photograph's own neutral axis `[a, b]` at input lightness `i / (AXIS_NODES - 1)`.
    pub cast_in: Vec<[f32; 2]>,
    /// The neutral axis the result should have at output lightness `i / (AXIS_NODES - 1)`.
    pub cast_out: Vec<[f32; 2]>,
    /// Output chroma at input chroma `CHROMA_SPAN * i / (CHROMA_NODES - 1)`, measured from the
    /// neutral axis; starts at zero and never decreases.
    pub chroma: Vec<f32>,
    /// Chroma multiplier per hue sector.
    pub hue_gain: Vec<f32>,
    /// Hue rotation per sector, radians.
    pub hue_shift: Vec<f32>,
    /// Lightness offset per sector for coloured pixels.
    pub hue_light: Vec<f32>,
    /// Chroma multiplier at output lightness `i / (ZONES - 1)`.
    pub zone_gain: Vec<f32>,
    /// How strongly skin-coloured pixels keep their own hue and colourfulness, `0..=1`.
    pub skin: f32,
    /// Additional per-pixel skin tone/colour displacement protection for palette transfer.
    /// Absent in older grades; zero preserves their rendering exactly.
    #[serde(default, skip_serializing_if = "no_fidelity")]
    pub skin_fidelity: f32,
}

// Serde's skip_serializing_if callback receives a borrowed field.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn no_fidelity(value: &f32) -> bool {
    *value <= 0.0
}

impl ReferenceGrade {
    /// The transform that changes nothing.
    #[must_use]
    pub fn identity() -> Self {
        Self {
            tone: (0..TONE_NODES)
                .map(|i| i as f32 / (TONE_NODES - 1) as f32)
                .collect(),
            cast_in: vec![[0.0; 2]; AXIS_NODES],
            cast_out: vec![[0.0; 2]; AXIS_NODES],
            chroma: (0..CHROMA_NODES)
                .map(|i| CHROMA_SPAN * i as f32 / (CHROMA_NODES - 1) as f32)
                .collect(),
            hue_gain: vec![1.0; SECTORS],
            hue_shift: vec![0.0; SECTORS],
            hue_light: vec![0.0; SECTORS],
            zone_gain: vec![1.0; ZONES],
            skin: 0.0,
            skin_fidelity: 0.0,
        }
    }

    /// Whether this transform leaves every colour where it was, to within storage precision.
    /// The skin protection is a limit on the other fields and changes nothing by itself.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        let identity = Self::identity();
        let near = |a: &[f32], b: &[f32]| {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 1e-5)
        };
        let casts_equal = self.cast_in.len() == self.cast_out.len()
            && self
                .cast_in
                .iter()
                .zip(&self.cast_out)
                .all(|(a, b)| (a[0] - b[0]).abs() <= 1e-5 && (a[1] - b[1]).abs() <= 1e-5);
        near(&self.tone, &identity.tone)
            && casts_equal
            && near(&self.chroma, &identity.chroma)
            && near(&self.hue_gain, &identity.hue_gain)
            && near(&self.hue_shift, &identity.hue_shift)
            && near(&self.hue_light, &identity.hue_light)
            && near(&self.zone_gain, &identity.zone_gain)
    }

    /// # Errors
    /// Wrong node counts, non-finite values, a decreasing curve or a value outside its bound.
    pub fn validate(&self) -> AuraResult<()> {
        let refuse = |why: &str| Err(recipe_invalid(KEY, why));
        let lengths = self.tone.len() == TONE_NODES
            && self.cast_in.len() == AXIS_NODES
            && self.cast_out.len() == AXIS_NODES
            && self.chroma.len() == CHROMA_NODES
            && self.hue_gain.len() == SECTORS
            && self.hue_shift.len() == SECTORS
            && self.hue_light.len() == SECTORS
            && self.zone_gain.len() == ZONES;
        if !lengths {
            return refuse("reference grade has the wrong number of nodes");
        }
        let all = self
            .tone
            .iter()
            .chain(self.cast_in.iter().flatten())
            .chain(self.cast_out.iter().flatten())
            .chain(&self.chroma)
            .chain(&self.hue_gain)
            .chain(&self.hue_shift)
            .chain(&self.hue_light)
            .chain(&self.zone_gain)
            .chain([&self.skin, &self.skin_fidelity]);
        if !all.into_iter().all(|v| v.is_finite()) {
            return refuse("reference grade holds a non-finite value");
        }
        let rising = |values: &[f32]| {
            values
                .iter()
                .zip(values.iter().skip(1))
                .all(|(a, b)| b + 1e-6 >= *a)
        };
        if !rising(&self.tone) || self.tone.iter().any(|v| !(0.0..=1.0).contains(v)) {
            return refuse("reference grade tone curve must rise within 0..1");
        }
        if !rising(&self.chroma)
            || self.chroma.first().is_some_and(|v| v.abs() > 1e-6)
            || self
                .chroma
                .iter()
                .any(|v| !(0.0..=MAX_CHROMA_OUT).contains(v))
        {
            return refuse("reference grade chroma curve must start at zero and rise");
        }
        if self
            .cast_in
            .iter()
            .chain(&self.cast_out)
            .flatten()
            .any(|v| v.abs() > MAX_CAST + 1e-6)
        {
            return refuse("reference grade colour cast is out of range");
        }
        if self
            .hue_gain
            .iter()
            .chain(&self.zone_gain)
            .any(|v| !(0.0..=MAX_GAIN).contains(v))
            || self
                .hue_shift
                .iter()
                .any(|v| v.abs() > MAX_HUE_SHIFT + 1e-6)
            || self
                .hue_light
                .iter()
                .any(|v| v.abs() > MAX_HUE_LIGHT + 1e-6)
            || !(0.0..=1.0).contains(&self.skin)
            || !(0.0..=1.0).contains(&self.skin_fidelity)
        {
            return refuse("reference grade hue or zone adjustment is out of range");
        }
        Ok(())
    }

    /// Clamp every field into its bound and restore the curves' monotonicity. Used after
    /// strengths above 100% extrapolate a fitted transform.
    #[must_use]
    pub fn bounded(mut self) -> Self {
        let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        let identity = Self::identity();
        let mut floor = 0.0_f32;
        for (v, i) in self.tone.iter_mut().zip(&identity.tone) {
            *v = finite(*v, *i).clamp(floor, 1.0);
            floor = *v;
        }
        let mut floor = 0.0_f32;
        for (index, (v, i)) in self.chroma.iter_mut().zip(&identity.chroma).enumerate() {
            *v = if index == 0 {
                0.0
            } else {
                finite(*v, *i).clamp(floor, MAX_CHROMA_OUT)
            };
            floor = *v;
        }
        for v in self.cast_in.iter_mut().chain(&mut self.cast_out).flatten() {
            *v = finite(*v, 0.0).clamp(-MAX_CAST, MAX_CAST);
        }
        for v in self.hue_gain.iter_mut().chain(&mut self.zone_gain) {
            *v = finite(*v, 1.0).clamp(0.0, MAX_GAIN);
        }
        for v in &mut self.hue_shift {
            *v = finite(*v, 0.0).clamp(-MAX_HUE_SHIFT, MAX_HUE_SHIFT);
        }
        for v in &mut self.hue_light {
            *v = finite(*v, 0.0).clamp(-MAX_HUE_LIGHT, MAX_HUE_LIGHT);
        }
        self.skin = finite(self.skin, 0.0).clamp(0.0, 1.0);
        self.skin_fidelity = finite(self.skin_fidelity, 0.0).clamp(0.0, 1.0);
        self
    }
}

/// # Errors
/// Rejects a malformed stored transform; an absent one is `None`.
pub fn read(recipe: &Recipe) -> AuraResult<Option<ReferenceGrade>> {
    let Some(value) = recipe.extra.get(KEY) else {
        return Ok(None);
    };
    let grade: ReferenceGrade = serde_json::from_value(value.clone())
        .map_err(|_| recipe_invalid(KEY, "Invalid reference grade"))?;
    grade.validate()?;
    Ok(Some(grade))
}

/// Store a transform, or remove the key when it changes nothing.
/// # Errors
/// Refuses an invalid transform without changing the recipe.
pub fn write(recipe: &mut Recipe, grade: &ReferenceGrade) -> AuraResult<()> {
    grade.validate()?;
    if grade.is_identity() {
        recipe.extra.remove(KEY);
        return Ok(());
    }
    recipe.extra.insert(
        KEY.into(),
        serde_json::to_value(grade)
            .map_err(|_| recipe_invalid(KEY, "Cannot encode reference grade"))?,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_valid_neutral_and_not_stored() {
        let identity = ReferenceGrade::identity();
        identity.validate().unwrap();
        assert!(identity.is_identity());
        let mut recipe = crate::fixtures::reference();
        let before = recipe.clone();
        write(&mut recipe, &identity).unwrap();
        assert_eq!(recipe, before);
        assert_eq!(read(&recipe).unwrap(), None);
    }

    #[test]
    fn old_grades_keep_zero_skin_fidelity_and_new_fidelity_is_bounded() {
        let identity = ReferenceGrade::identity();
        let old = serde_json::to_value(&identity).unwrap();
        assert!(old.get("skinFidelity").is_none());
        let restored: ReferenceGrade = serde_json::from_value(old).unwrap();
        assert_eq!(restored, identity);
        let mut invalid = identity;
        invalid.skin_fidelity = 1.5;
        assert!(invalid.validate().is_err());
        assert!((invalid.bounded().skin_fidelity - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_fitted_grade_round_trips_and_an_invalid_one_changes_nothing() {
        let mut grade = ReferenceGrade::identity();
        grade.tone[16] = 0.52;
        grade.cast_out[2] = [-0.01, -0.02];
        grade.hue_gain[3] = 0.6;
        grade.skin = 0.5;
        let mut recipe = crate::fixtures::reference();
        write(&mut recipe, &grade).unwrap();
        assert!(!grade.is_identity());
        assert_eq!(read(&recipe).unwrap(), Some(grade.clone()));
        let stored = recipe.clone();
        for broken in 0..5 {
            let mut bad = grade.clone();
            match broken {
                0 => bad.tone[3] = 0.9,
                1 => bad.chroma[0] = 0.1,
                2 => bad.cast_in[0][1] = 0.5,
                3 => {
                    bad.hue_shift.pop();
                }
                _ => bad.zone_gain[1] = f32::NAN,
            }
            assert!(write(&mut recipe, &bad).is_err(), "case {broken}");
            assert_eq!(recipe, stored);
        }
    }

    #[test]
    fn bounding_restores_every_invariant() {
        let mut grade = ReferenceGrade::identity();
        grade.tone[5] = 2.0;
        grade.tone[6] = -1.0;
        grade.chroma[0] = 0.3;
        grade.chroma[4] = f32::INFINITY;
        grade.cast_out[1] = [1.0, -1.0];
        grade.hue_gain[0] = -2.0;
        grade.hue_shift[1] = 9.0;
        grade.skin = 3.0;
        let bounded = grade.bounded();
        bounded.validate().unwrap();
        assert_eq!(
            bounded.tone[6].to_bits(),
            1.0_f32.to_bits(),
            "a later node cannot fall below an earlier one"
        );
    }
}
