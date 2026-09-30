//! Versioned local retouch authoring, carried by the recipe extension map. ADR-0068.
use crate::{errors::recipe_invalid, Recipe};
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};

pub const KEY: &str = "studio_retouch_v1";
pub const MAX_EDITS: usize = 256;
pub const MAX_STROKES: usize = 128;
pub const MAX_POINTS: usize = 8192;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BrushMask {
    pub strokes: Vec<BrushStroke>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BrushStroke {
    pub erase: bool,
    /// Radius relative to the shorter image edge, before pressure.
    pub radius: f32,
    pub opacity: f32,
    /// Normalized x, y, pressure. Pressure changes radius, not repeated opacity.
    pub points: Vec<[f32; 3]>,
}

/// Sample-relative selection; it does not classify people or infer a skin tone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SkinSettings {
    pub tolerance: f32,
    pub edge_protection: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Gradient {
    pub start: [f32; 2],
    pub end: [f32; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LuminanceRange {
    /// Stops relative to 18% linear working luminance, before this operation.
    pub low: f32,
    pub high: f32,
    /// Smooth falloff outside each end of the selected interval, in stops.
    pub softness: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Selection {
    #[serde(default)]
    pub inverted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Gradient>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub luminance: Option<LuminanceRange>,
}

impl Default for SkinSettings {
    fn default() -> Self {
        Self {
            tolerance: 0.08,
            edge_protection: 0.8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Heal,
    Clone,
    AutoBlemish,
    Frequency,
    MicroDodgeBurn,
    Dodge,
    Burn,
    SkinColor,
    ColorMatch,
    Mattify,
    UnderEye,
    Wrinkle,
    Teeth,
    EyeClean,
    EyeDetail,
    RedEye,
    Fabric,
    Backdrop,
    Glare,
    Makeup,
    SkinSmooth,
    SkinUniformity,
    PortraitDodgeBurn,
    PatchHeal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Edit {
    pub id: String,
    pub tool: Tool,
    pub enabled: bool,
    /// Center x/y and ellipse radii x/y in normalized full-frame coordinates.
    pub region: [f32; 4],
    /// Normalized donor/reference point; mandatory for clone and color matching.
    pub source: Option<[f32; 2]>,
    pub amount: f32,
    /// Feather fraction: 0 is a hard edge, 1 is fully feathered.
    pub feather: f32,
    /// Frequency radius as a fraction of the image's shorter dimension.
    pub radius: f32,
    /// High-band gain. 1 preserves the original high band.
    pub texture: f32,
    pub tone: f32,
    pub warmth: f32,
    pub tint: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<BrushMask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<SkinSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Selection>,
}

/// Validate retouch parameters and aggregate authoring limits.
///
/// # Errors
/// Returns a recipe error for invalid parameters, missing required sources,
/// duplicate identifiers or excessive operations, strokes or points.
pub fn validate(edits: &[Edit]) -> AuraResult<()> {
    if edits.len() > MAX_EDITS {
        return Err(recipe_invalid(KEY, "too many retouch operations"));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut total_points = 0;
    for edit in edits {
        let unit = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
        if edit.id.is_empty()
            || edit.id.len() > 100
            || !ids.insert(&edit.id)
            || !edit.region.iter().all(|v| unit(*v))
            || edit.region[2] < 0.001
            || edit.region[3] < 0.001
            || !unit(edit.amount)
            || !unit(edit.feather)
            || !unit(edit.tone)
            || !edit.radius.is_finite()
            || !(0.0005..=0.05).contains(&edit.radius)
            || !edit.texture.is_finite()
            || !(0.0..=2.0).contains(&edit.texture)
            || !edit.warmth.is_finite()
            || !(-1.0..=1.0).contains(&edit.warmth)
            || !edit.tint.is_finite()
            || !(-1.0..=1.0).contains(&edit.tint)
            || edit.source.is_some_and(|p| !p.iter().all(|v| unit(*v)))
            || (matches!(
                edit.tool,
                Tool::Clone
                    | Tool::ColorMatch
                    | Tool::SkinSmooth
                    | Tool::SkinUniformity
                    | Tool::PortraitDodgeBurn
            ) && edit.source.is_none())
        {
            return Err(recipe_invalid(
                KEY,
                "invalid retouch parameters, duplicate ID, or missing source",
            ));
        }
        if edit.skin.is_some_and(|s| {
            !s.tolerance.is_finite()
                || !(0.015..=0.3).contains(&s.tolerance)
                || !unit(s.edge_protection)
        }) {
            return Err(recipe_invalid(KEY, "invalid skin range or edge protection"));
        }
        if edit.tool == Tool::PatchHeal
            && edit.source.is_none()
            && (edit.mask.is_some()
                || edit.region[2] > 0.1
                || edit.region[3] > 0.1
                || edit
                    .selection
                    .as_ref()
                    .is_some_and(|s| s.inverted || s.gradient.is_some()))
        {
            return Err(recipe_invalid(
                KEY,
                "choose a source for painted or large patch repairs",
            ));
        }
        validate_selection(edit)?;
        if let Some(mask) = &edit.mask {
            if mask.strokes.len() > MAX_STROKES {
                return Err(recipe_invalid(KEY, "too many mask strokes"));
            }
            for stroke in &mask.strokes {
                total_points += stroke.points.len();
                if stroke.points.is_empty()
                    || total_points > MAX_POINTS
                    || !stroke.radius.is_finite()
                    || !(0.0005..=0.25).contains(&stroke.radius)
                    || !unit(stroke.opacity)
                    || !stroke.points.iter().flatten().all(|v| unit(*v))
                {
                    return Err(recipe_invalid(KEY, "invalid or oversized brush mask"));
                }
            }
        }
    }
    Ok(())
}

/// Read and validate the optional native retouch extension.
///
/// # Errors
/// Returns a recipe error if the stored extension is malformed or fails validation.
pub fn read(recipe: &Recipe) -> AuraResult<Vec<Edit>> {
    let Some(value) = recipe.extra.get(KEY) else {
        return Ok(Vec::new());
    };
    let edits: Vec<Edit> = serde_json::from_value(value.clone())
        .map_err(|_| recipe_invalid(KEY, "invalid retouch operation format"))?;
    validate(&edits)?;
    Ok(edits)
}

/// Store validated native operations without modifying other recipe fields.
///
/// # Errors
/// Returns a recipe error if validation or serialization fails.
pub fn write(recipe: &mut Recipe, edits: &[Edit]) -> AuraResult<()> {
    validate(edits)?;
    // Keep an empty array after clearing so merge records the deletion as a user edit.
    recipe.extra.insert(
        KEY.into(),
        serde_json::to_value(edits)
            .map_err(|_| recipe_invalid(KEY, "cannot serialize retouch operations"))?,
    );
    Ok(())
}

fn validate_selection(edit: &Edit) -> AuraResult<()> {
    let unit = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
    if let Some(selection) = &edit.selection {
        if let Some(g) = &selection.gradient {
            if edit.mask.is_some()
                || !g.start.iter().chain(&g.end).all(|v| unit(*v))
                || (g.start[0] - g.end[0]).hypot(g.start[1] - g.end[1]) < 0.001
            {
                return Err(recipe_invalid(
                    KEY,
                    "invalid gradient or conflicting painted mask",
                ));
            }
        }
        if selection.luminance.as_ref().is_some_and(|r| {
            !r.low.is_finite()
                || !r.high.is_finite()
                || !r.softness.is_finite()
                || !(-16.0..=16.0).contains(&r.low)
                || !(-16.0..=16.0).contains(&r.high)
                || r.low > r.high
                || !(0.0..=4.0).contains(&r.softness)
        }) {
            return Err(recipe_invalid(KEY, "invalid luminance range"));
        }
        if edit.tool == Tool::Heal
            && edit.source.is_none()
            && (selection.inverted || selection.gradient.is_some())
        {
            return Err(recipe_invalid(
                KEY,
                "choose a source for gradient or inverted healing",
            ));
        }
    }
    Ok(())
}
