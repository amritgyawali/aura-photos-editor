//! Versioned local retouch authoring, carried by the recipe extension map. ADR-0073.
use crate::{errors::recipe_invalid, Recipe};
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const KEY: &str = "studio_retouch_v1";
/// Segmentation mattes that retouch operations refer to by id. ADR-0082.
pub const MATTE_KEY: &str = "studio_retouch_mattes_v1";
pub const MAX_EDITS: usize = 256;
pub const MAX_MATTES: usize = 64;
/// At most this many cells per matte; the renderer refines its edges at full resolution.
pub const MAX_MATTE_CELLS: usize = 256 * 256;
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
    /// Only change skin connected to the sample without crossing a strong edge, so a
    /// skin-coloured wall, table or backdrop that is not touching the person is never
    /// selected. Absent (false) in recipes written before it existed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub connected: bool,
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
            connected: false,
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
    /// Patch-heal donor footprint relative to the target (1 keeps the original scale).
    /// Smaller clean donors can cover dense blemishes; only used with an explicit source.
    #[serde(default = "default_source_scale")]
    pub source_scale: f32,
    pub amount: f32,
    /// Feather fraction: 0 is a hard edge, 1 is fully feathered.
    pub feather: f32,
    /// Frequency radius as a fraction of the image's shorter dimension.
    pub radius: f32,
    /// High-band gain. 1 preserves the original high band.
    pub texture: f32,
    /// Separate fine pores from larger uneven texture during frequency separation.
    /// Off for old recipes; on for the automatic deep skin finish.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub preserve_microtexture: bool,
    /// Transfer real donor texture over a robust local lighting fit. Old heals stay unchanged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub texture_heal: bool,
    pub tone: f32,
    pub warmth: f32,
    pub tint: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<BrushMask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<SkinSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Selection>,
    /// A segmentation matte from [`MATTE_KEY`] that limits the operation to, for example,
    /// one person's face skin. Multiplies the region, brush and selection coverage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matte: Option<String>,
}

/// A soft selection measured by the skin segmenter, stored compactly. ADR-0082.
///
/// `data` is base64 of a run-length code: each byte is `(level << 4) | (run - 1)`, where
/// `level` is coverage in fifteenths (0..15, decoded as `level * 17`) and `run` is 1..16 cells
/// in row-major order. Sixteen levels are enough because the renderer re-derives the soft
/// edge from the photograph itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Matte {
    /// Normalized left, top, right, bottom of the grid.
    pub bounds: [f32; 4],
    pub width: u32,
    pub height: u32,
    pub data: String,
    /// Re-detect fine image edges when upsampling. Disable for an already protected
    /// soft surface selection, so blemishes cannot cut holes in their own correction.
    #[serde(default = "default_refine_edges")]
    pub refine_edges: bool,
}

fn default_refine_edges() -> bool {
    true
}

fn default_source_scale() -> f32 {
    1.0
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk.first().copied().unwrap_or(0),
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (k, shift) in [18_u32, 12, 6, 0].into_iter().enumerate() {
            if k <= chunk.len() {
                out.push(char::from(
                    BASE64
                        .get(((n >> shift) & 63) as usize)
                        .copied()
                        .unwrap_or(b'A'),
                ));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let value = |c: u8| -> Option<u32> {
        BASE64
            .iter()
            .position(|v| *v == c)
            .and_then(|p| u32::try_from(p).ok())
    };
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().rev().take_while(|c| **c == b'=').count();
        if pad > 2 {
            return None;
        }
        let mut n = 0_u32;
        for (k, c) in chunk.iter().enumerate() {
            let v = if k >= 4 - pad { 0 } else { value(*c)? };
            n = (n << 6) | v;
        }
        let decoded = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(decoded.get(..3 - pad)?);
    }
    Some(out)
}

impl Matte {
    /// Encode coverage values (0..=255, row-major, `width * height` of them).
    #[must_use]
    pub fn encode(bounds: [f32; 4], width: u32, height: u32, alpha: &[u8]) -> Self {
        let mut runs: Vec<u8> = Vec::new();
        let mut current: Option<(u8, u8)> = None;
        for value in alpha {
            let level = ((u16::from(*value) * 15 + 127) / 255) as u8;
            current = match current {
                Some((l, run)) if l == level && run < 16 => Some((l, run + 1)),
                Some((l, run)) => {
                    runs.push((l << 4) | (run - 1));
                    Some((level, 1))
                }
                None => Some((level, 1)),
            };
        }
        if let Some((l, run)) = current {
            runs.push((l << 4) | (run - 1));
        }
        Self {
            bounds,
            width,
            height,
            data: base64_encode(&runs),
            refine_edges: true,
        }
    }

    /// Coverage values 0..=255, row-major, or `None` when the data does not decode to exactly
    /// `width * height` cells.
    #[must_use]
    pub fn decode(&self) -> Option<Vec<u8>> {
        let cells = (self.width as usize).checked_mul(self.height as usize)?;
        if cells == 0 || cells > MAX_MATTE_CELLS {
            return None;
        }
        let mut out = Vec::with_capacity(cells);
        for byte in base64_decode(&self.data)? {
            let level = (byte >> 4) * 17;
            for _ in 0..=(byte & 15) {
                out.push(level);
            }
            if out.len() > cells {
                return None;
            }
        }
        (out.len() == cells).then_some(out)
    }

    fn valid(&self) -> bool {
        let [l, t, r, b] = self.bounds;
        self.bounds
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && r > l
            && b > t
            && self.decode().is_some()
    }
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
            || !edit.source_scale.is_finite()
            || !(0.2..=1.0).contains(&edit.source_scale)
            || (edit.source_scale < 1.0 && edit.tool == Tool::PatchHeal && edit.source.is_none())
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
    read_mattes(recipe)?;
    let Some(value) = recipe.extra.get(KEY) else {
        return Ok(Vec::new());
    };
    let edits: Vec<Edit> = serde_json::from_value(value.clone())
        .map_err(|_| recipe_invalid(KEY, "invalid retouch operation format"))?;
    validate(&edits)?;
    Ok(edits)
}

/// Read and validate the segmentation mattes retouch operations refer to.
///
/// # Errors
/// Returns a recipe error if the stored mattes are malformed or too many.
pub fn read_mattes(recipe: &Recipe) -> AuraResult<BTreeMap<String, Matte>> {
    let Some(value) = recipe.extra.get(MATTE_KEY) else {
        return Ok(BTreeMap::new());
    };
    let mattes: BTreeMap<String, Matte> = serde_json::from_value(value.clone())
        .map_err(|_| recipe_invalid(MATTE_KEY, "invalid matte format"))?;
    if mattes.len() > MAX_MATTES
        || mattes
            .iter()
            .any(|(id, m)| id.is_empty() || id.len() > 100 || !m.valid())
    {
        return Err(recipe_invalid(MATTE_KEY, "invalid or too many mattes"));
    }
    Ok(mattes)
}

/// Store operations together with the mattes they use. Mattes no operation refers to are
/// dropped, so removing an automatic operation never leaves its selection behind.
///
/// # Errors
/// Returns a recipe error if validation or serialization fails, or an operation refers to
/// a matte that is not given.
pub fn write_with_mattes(
    recipe: &mut Recipe,
    edits: &[Edit],
    mattes: &BTreeMap<String, Matte>,
) -> AuraResult<()> {
    validate(edits)?;
    let used: BTreeMap<String, Matte> = mattes
        .iter()
        .filter(|(id, _)| {
            edits
                .iter()
                .any(|e| e.matte.as_deref() == Some(id.as_str()))
        })
        .map(|(id, m)| (id.clone(), m.clone()))
        .collect();
    if edits
        .iter()
        .filter_map(|e| e.matte.as_deref())
        .any(|id| !used.contains_key(id))
    {
        return Err(recipe_invalid(
            MATTE_KEY,
            "an operation refers to a missing matte",
        ));
    }
    if used.len() > MAX_MATTES || used.values().any(|m| !m.valid()) {
        return Err(recipe_invalid(MATTE_KEY, "invalid or too many mattes"));
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
    // Keep an empty array after clearing so merge records the deletion as a user edit.
    recipe.extra.insert(
        KEY.into(),
        serde_json::to_value(edits)
            .map_err(|_| recipe_invalid(KEY, "cannot serialize retouch operations"))?,
    );
    Ok(())
}

/// Store validated native operations without modifying other recipe fields.
///
/// # Errors
/// Returns a recipe error if validation or serialization fails.
pub fn write(recipe: &mut Recipe, edits: &[Edit]) -> AuraResult<()> {
    validate(edits)?;
    // Losing a skin matte must never turn a skin-only operation into a broad brush edit.
    let mattes = read_mattes(recipe)?;
    write_with_mattes(recipe, edits, &mattes)
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod matte_tests {
    use super::*;

    #[test]
    fn writing_a_missing_or_corrupt_matte_never_expands_the_selection() {
        let mut recipe = crate::fixtures::neutral(crate::fixtures::FIXTURE_HASH, "test");
        let edit: Edit = serde_json::from_value(serde_json::json!({
            "id": "skin", "tool": "dodge", "enabled": true,
            "region": [0.5, 0.5, 1.0, 1.0], "source": null,
            "amount": 0.5, "feather": 0.0, "radius": 0.01,
            "texture": 1.0, "tone": 0.5, "warmth": 0.0, "tint": 0.0,
            "matte": "missing-skin"
        }))
        .unwrap();
        let before = recipe.clone();
        assert!(write(&mut recipe, &[edit]).is_err());
        assert_eq!(recipe.extra, before.extra);
        recipe
            .extra
            .insert(MATTE_KEY.into(), serde_json::json!({"broken": 5}));
        assert!(write(&mut recipe, &[]).is_err());
    }

    #[test]
    fn mattes_round_trip_through_sixteen_levels_and_refuse_bad_data() {
        let alpha: Vec<u8> = (0..37 * 23)
            .map(|i| {
                if i % 37 < 10 {
                    0
                } else if i % 37 < 30 {
                    255
                } else {
                    (i * 7 % 256) as u8
                }
            })
            .collect();
        let matte = Matte::encode([0.1, 0.2, 0.6, 0.9], 37, 23, &alpha);
        let decoded = matte.decode().unwrap();
        assert_eq!(decoded.len(), alpha.len());
        for (a, b) in alpha.iter().zip(&decoded) {
            assert!(a.abs_diff(*b) <= 9, "{a} {b}");
        }
        assert!(matte.valid());
        // Long uniform runs compress to a byte per sixteen cells.
        let flat = Matte::encode([0.0, 0.0, 1.0, 1.0], 256, 256, &vec![255; 256 * 256]);
        assert!(flat.data.len() < 6000, "{}", flat.data.len());
        assert_eq!(flat.decode().unwrap().len(), 256 * 256);
        let mut short = matte.clone();
        short.width += 1;
        assert!(short.decode().is_none());
        let mut garbage = matte.clone();
        garbage.data = "!!!!".into();
        assert!(garbage.decode().is_none());
        let mut inverted = matte;
        inverted.bounds = [0.6, 0.2, 0.1, 0.9];
        assert!(!inverted.valid());
    }
}
