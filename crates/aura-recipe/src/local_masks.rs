//! Local adjustments drawn in the Studio: masks built from AI selections, gradients and brush
//! strokes, each with its own exposure, colour and detail sliders. ADR-0102.
//!
//! A mask is a list of components combined in order - added, subtracted or intersected, each
//! optionally inverted - so "the subject without the face" or "the sky, only where it is
//! bright" are one mask, as in Lightroom. The sliders reuse the frozen [`MaskParams`] block, so
//! absent still means "not touched here".
//!
//! Carried by the recipe's extension map rather than `Recipe::masks`: that list is frozen with no
//! place for a gradient's placement, a brush stroke or a combination, and widening a frozen
//! contract needs an ADR of its own. The AI selections are stored as mattes beside the masks -
//! a selection measured once on the photograph, never re-derived differently by a later build.
use crate::{
    errors::recipe_invalid,
    retouch_tools::{BrushStroke, LuminanceRange, Matte},
    MaskParams, Recipe,
};
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The masks, in the order they apply.
pub const KEY: &str = "studio_masks_v1";
/// The AI selections the masks refer to, by id.
pub const MATTE_KEY: &str = "studio_masks_mattes_v1";
pub const MAX_MASKS: usize = 32;
pub const MAX_COMPONENTS: usize = 16;
pub const MAX_MATTES: usize = 48;
pub const MAX_STROKES: usize = 128;
pub const MAX_POINTS: usize = 8192;

/// How a component joins the mask built so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Union: the larger of the two.
    Add,
    /// What is built so far, less this component.
    Subtract,
    /// Only where both are.
    Intersect,
}

/// What one component selects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    /// An AI selection stored under [`MATTE_KEY`]: subject, sky, a person's hair...
    Matte {
        matte: String,
        /// What it selects, for the panel: `subject`, `sky`, `face_skin`...
        what: String,
    },
    /// A portrait region the renderer measures from the photograph: `eyes`, `lips`, `teeth`.
    Region { region: String },
    /// Full effect on the `start` side, fading to none at `end`. Normalized frame coordinates.
    Linear { start: [f32; 2], end: [f32; 2] },
    /// An ellipse: full effect inside, fading over `feather` of its radius.
    Radial {
        centre: [f32; 2],
        /// Normalized to the frame's width and height.
        radii: [f32; 2],
        /// Degrees, clockwise.
        angle: f32,
        feather: f32,
    },
    /// Painted by hand. `feather` softens every stroke's edge.
    Brush {
        strokes: Vec<BrushStroke>,
        feather: f32,
    },
    /// A range of brightness, in stops around 18 % grey: usually intersected with another
    /// component, as Lightroom's luminance range.
    Luminance { range: LuminanceRange },
}

/// One component of a mask.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Component {
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub invert: bool,
    pub source: Source,
}

fn default_true() -> bool {
    true
}

fn default_amount() -> f32 {
    1.0
}

/// One local adjustment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LocalMask {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// How much of the sliders applies, `0..=1`.
    #[serde(default = "default_amount")]
    pub amount: f32,
    pub components: Vec<Component>,
    pub params: MaskParams,
}

fn unit(v: f32) -> bool {
    v.is_finite() && (0.0..=1.0).contains(&v)
}

fn params_valid(p: &MaskParams) -> bool {
    let tone = |v: Option<i16>| v.is_none_or(|v| (-100..=100).contains(&v));
    p.exposure
        .is_none_or(|v| v.is_finite() && (-5.0..=5.0).contains(&v))
        && tone(p.contrast)
        && tone(p.highlights)
        && tone(p.shadows)
        && tone(p.whites)
        && tone(p.blacks)
        && tone(p.clarity)
        && tone(p.texture)
        && tone(p.saturation)
        && tone(p.tint)
        && p.temperature.is_none_or(|v| (-5000..=5000).contains(&v))
}

/// Validate the masks against the mattes they refer to.
///
/// # Errors
/// A recipe error for an invalid parameter, a duplicate id, a missing matte, or too many
/// masks, components, strokes or points.
pub fn validate(masks: &[LocalMask], mattes: &BTreeMap<String, Matte>) -> AuraResult<()> {
    if masks.len() > MAX_MASKS {
        return Err(recipe_invalid(KEY, "too many masks"));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut points = 0;
    for mask in masks {
        if mask.id.is_empty()
            || mask.id.len() > 100
            || mask.name.len() > 100
            || !ids.insert(&mask.id)
            || !unit(mask.amount)
            || mask.components.is_empty()
            || mask.components.len() > MAX_COMPONENTS
            || !params_valid(&mask.params)
        {
            return Err(recipe_invalid(
                KEY,
                "invalid mask, duplicate id, or too many components",
            ));
        }
        for component in &mask.components {
            let point = |p: [f32; 2]| p.iter().all(|v| v.is_finite() && (-1.0..=2.0).contains(v));
            let ok = match &component.source {
                Source::Matte { matte, what } => mattes.contains_key(matte) && what.len() <= 40,
                Source::Region { region } => !region.is_empty() && region.len() <= 40,
                Source::Linear { start, end } => {
                    point(*start)
                        && point(*end)
                        && (start[0] - end[0]).hypot(start[1] - end[1]) > 1e-4
                }
                Source::Radial {
                    centre,
                    radii,
                    angle,
                    feather,
                } => {
                    point(*centre)
                        && radii
                            .iter()
                            .all(|r| r.is_finite() && (0.001..=2.0).contains(r))
                        && angle.is_finite()
                        && unit(*feather)
                }
                Source::Brush { strokes, feather } => {
                    points += strokes.iter().map(|s| s.points.len()).sum::<usize>();
                    unit(*feather)
                        && strokes.len() <= MAX_STROKES
                        && points <= MAX_POINTS
                        && strokes.iter().all(|s| {
                            !s.points.is_empty()
                                && s.radius.is_finite()
                                && (0.0005..=0.25).contains(&s.radius)
                                && unit(s.opacity)
                                && s.points.iter().flatten().all(|v| unit(*v))
                        })
                }
                Source::Luminance { range } => {
                    range.low.is_finite()
                        && range.high.is_finite()
                        && range.low <= range.high
                        && range.softness.is_finite()
                        && (0.0..=8.0).contains(&range.softness)
                }
            };
            if !ok {
                return Err(recipe_invalid(KEY, "invalid mask component"));
            }
        }
    }
    Ok(())
}

/// Read the stored AI selections.
///
/// # Errors
/// A recipe error when they are malformed or too many.
pub fn read_mattes(recipe: &Recipe) -> AuraResult<BTreeMap<String, Matte>> {
    let Some(value) = recipe.extra.get(MATTE_KEY) else {
        return Ok(BTreeMap::new());
    };
    let mattes: BTreeMap<String, Matte> = serde_json::from_value(value.clone())
        .map_err(|_| recipe_invalid(MATTE_KEY, "invalid matte format"))?;
    if mattes.len() > MAX_MATTES
        || mattes
            .iter()
            .any(|(id, m)| id.is_empty() || id.len() > 100 || m.decode().is_none())
    {
        return Err(recipe_invalid(MATTE_KEY, "invalid or too many mattes"));
    }
    Ok(mattes)
}

/// Read and validate the masks.
///
/// # Errors
/// A recipe error when they are malformed or fail [`validate`].
pub fn read(recipe: &Recipe) -> AuraResult<Vec<LocalMask>> {
    let mattes = read_mattes(recipe)?;
    let Some(value) = recipe.extra.get(KEY) else {
        return Ok(Vec::new());
    };
    let masks: Vec<LocalMask> = serde_json::from_value(value.clone())
        .map_err(|_| recipe_invalid(KEY, "invalid mask format"))?;
    validate(&masks, &mattes)?;
    Ok(masks)
}

/// Store the masks with the mattes they use; a matte no mask refers to is dropped.
///
/// # Errors
/// A recipe error when validation or serialization fails.
pub fn write(
    recipe: &mut Recipe,
    masks: &[LocalMask],
    mattes: &BTreeMap<String, Matte>,
) -> AuraResult<()> {
    let used: BTreeMap<String, Matte> = mattes
        .iter()
        .filter(|(id, _)| {
            masks.iter().any(|m| {
                m.components.iter().any(
                    |c| matches!(&c.source, Source::Matte { matte, .. } if matte == id.as_str()),
                )
            })
        })
        .map(|(id, m)| (id.clone(), m.clone()))
        .collect();
    validate(masks, &used)?;
    if used.len() > MAX_MATTES {
        return Err(recipe_invalid(MATTE_KEY, "too many mattes"));
    }
    if used.is_empty() {
        recipe.extra.remove(MATTE_KEY);
    } else {
        recipe.extra.insert(
            MATTE_KEY.into(),
            serde_json::to_value(&used)
                .map_err(|_| recipe_invalid(MATTE_KEY, "cannot serialize mattes"))?,
        );
    }
    // An empty list is kept after the last mask goes, so merge records it as an edit.
    recipe.extra.insert(
        KEY.into(),
        serde_json::to_value(masks).map_err(|_| recipe_invalid(KEY, "cannot serialize masks"))?,
    );
    Ok(())
}

/// True when the recipe carries at least one mask that changes something.
#[must_use]
pub fn any_active(recipe: &Recipe) -> bool {
    read(recipe).is_ok_and(|masks| {
        masks
            .iter()
            .any(|m| m.enabled && m.amount > 0.0 && !m.params.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(source: Source) -> LocalMask {
        LocalMask {
            id: "m1".into(),
            name: "Mask 1".into(),
            enabled: true,
            amount: 1.0,
            components: vec![Component {
                mode: Mode::Add,
                invert: false,
                source,
            }],
            params: MaskParams {
                exposure: Some(0.5),
                ..MaskParams::default()
            },
        }
    }

    #[test]
    fn masks_round_trip_and_unused_mattes_are_dropped() {
        let mut recipe = crate::fixtures::reference();
        let mut mattes = BTreeMap::new();
        mattes.insert(
            "subject".to_string(),
            Matte::encode([0.0, 0.0, 1.0, 1.0], 2, 2, &[255, 0, 255, 0]),
        );
        mattes.insert(
            "unused".to_string(),
            Matte::encode([0.0, 0.0, 1.0, 1.0], 2, 2, &[0; 4]),
        );
        let masks = vec![mask(Source::Matte {
            matte: "subject".into(),
            what: "subject".into(),
        })];
        write(&mut recipe, &masks, &mattes).unwrap();
        assert_eq!(read(&recipe).unwrap(), masks);
        assert_eq!(read_mattes(&recipe).unwrap().len(), 1);
        assert!(any_active(&recipe));
    }

    #[test]
    fn a_missing_matte_or_a_degenerate_gradient_is_refused() {
        let mut recipe = crate::fixtures::reference();
        let missing = vec![mask(Source::Matte {
            matte: "nowhere".into(),
            what: "subject".into(),
        })];
        assert!(write(&mut recipe, &missing, &BTreeMap::new()).is_err());
        let flat = vec![mask(Source::Linear {
            start: [0.5, 0.5],
            end: [0.5, 0.5],
        })];
        assert!(write(&mut recipe, &flat, &BTreeMap::new()).is_err());
        let wild = LocalMask {
            params: MaskParams {
                exposure: Some(40.0),
                ..MaskParams::default()
            },
            ..mask(Source::Linear {
                start: [0.5, 0.0],
                end: [0.5, 1.0],
            })
        };
        assert!(write(&mut recipe, &[wild], &BTreeMap::new()).is_err());
    }
}
