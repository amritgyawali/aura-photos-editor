//! How strongly each kind of automatic retouch is applied, set after the automatic pass. ADR-0130.
//!
//! The automatic pass measures every face and writes one operation per mark, line or region,
//! each with an `amount` it chose. A photographer who thinks the skin is a little too smooth
//! should not have to find the twelve smoothing operations and move each one: they move one
//! slider. The slider is a factor per [`Category`], stored beside the operations rather than
//! inside them, so:
//!
//! - the operations keep the amounts the pass measured, and re-running the pass leaves the
//!   photographer's factors where they were;
//! - 100% is always exactly what the pass decided, and moving a slider back to 100% restores it;
//! - only the renderer reads the scaled amounts ([`effective`]), so nothing that plans or edits
//!   operations ever mistakes a slider position for a measurement.
//!
//! Operations a person authored (`manual-…`, or anything not written by the automatic pass)
//! are never scaled: they have their own strength control.
use crate::errors::recipe_invalid;
use crate::retouch_tools::{self, Edit, Tool};
use crate::Recipe;
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const KEY: &str = "studio_retouch_intensity_v1";
/// The largest factor a slider may reach. Each operation is still capped at full strength, so
/// how far above 100% a category can actually go depends on the amounts the pass chose.
pub const MAX_FACTOR: f32 = 2.0;
/// Operation ids the automatic passes write start with this; nothing else is scaled.
pub const AUTOMATIC_PREFIX: &str = "auto-";

/// One slider in the panel. The names follow the automatic retouch settings they correspond to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Acne, spots and marks on the face.
    Blemishes,
    /// Skin smoothing and pore refinement.
    Smoothing,
    /// Texture put back from clean skin.
    Texture,
    /// Evening of blotchy tone.
    ToneEvenness,
    /// Redness and skin colour correction.
    Redness,
    /// Evening of uneven light across the skin (dodge and burn).
    LightEvenness,
    /// Forehead lines, crow's feet, smile lines, neck lines.
    Wrinkles,
    /// Dark circles and eye bags.
    UnderEye,
    /// Eye whites, iris and lashes.
    Eyes,
    Teeth,
    /// Lip colour, lip definition and makeup.
    Lips,
    /// Shine and oily highlights.
    Shine,
    /// Face light, glow, contour and highlight.
    Sculpt,
    /// Smoothing, tone, shine and redness on body skin.
    BodySkin,
    /// Spots and marks on body skin.
    BodyBlemishes,
    StretchMarks,
    /// Hair detail, shine and stray hairs.
    Hair,
    /// Clothing wrinkles, lint and marks.
    Clothing,
    /// Backdrop cleanup, dust and background tone.
    Background,
    /// Scene-wide automatic edits such as the sky.
    Scene,
    /// Any other automatic operation, so that every one of them has a slider.
    Other,
}

impl Category {
    pub const ALL: [Self; 21] = [
        Self::Blemishes,
        Self::Smoothing,
        Self::Texture,
        Self::ToneEvenness,
        Self::Redness,
        Self::LightEvenness,
        Self::Wrinkles,
        Self::UnderEye,
        Self::Eyes,
        Self::Teeth,
        Self::Lips,
        Self::Shine,
        Self::Sculpt,
        Self::BodySkin,
        Self::BodyBlemishes,
        Self::StretchMarks,
        Self::Hair,
        Self::Clothing,
        Self::Background,
        Self::Scene,
        Self::Other,
    ];
}

/// The category an operation's slider belongs to, or `None` when no slider scales it.
#[must_use]
pub fn category_of(edit: &Edit) -> Option<Category> {
    let id = edit.id.as_str();
    if !id.starts_with(AUTOMATIC_PREFIX) {
        return None;
    }
    let has = |part: &str| id.contains(part);
    let heal = matches!(
        edit.tool,
        Tool::AcneClear
            | Tool::FrequencyHeal
            | Tool::AutoBlemish
            | Tool::PatchHeal
            | Tool::Heal
            | Tool::Clone
    );
    // Ids decide before tools: one tool serves several jobs (a dodge lifts an eye bag, a
    // sky and a cheekbone), and the pass names each operation for the job it does.
    let category = if id.starts_with("auto-scene-") {
        Category::Scene
    } else if has("-backdrop")
        || has("-background")
        || has("-dust-")
        || matches!(edit.tool, Tool::Backdrop | Tool::BackgroundColor)
    {
        Category::Background
    } else if has("-stray-") || has("-hair-") {
        Category::Hair
    } else if has("-fabric") || has("-cloth") || edit.tool == Tool::Fabric {
        Category::Clothing
    } else if edit.tool == Tool::StretchMarks || has("-stretch") {
        Category::StretchMarks
    } else if has("-body") {
        if heal || has("-body-spots") || has("-body-marks") {
            Category::BodyBlemishes
        } else {
            Category::BodySkin
        }
    } else if heal || has("-spot-") || has("-clear") || has("-blemish") {
        Category::Blemishes
    } else if edit.tool == Tool::Wrinkle || has("-lines-") || has("-fold-") {
        Category::Wrinkles
    } else if edit.tool == Tool::UnderEye || has("-undereye") {
        Category::UnderEye
    } else if has("-eye-") || matches!(edit.tool, Tool::EyeClean | Tool::EyeDetail | Tool::RedEye) {
        Category::Eyes
    } else if edit.tool == Tool::Teeth || has("-teeth") {
        Category::Teeth
    } else if edit.tool == Tool::Makeup || has("-lips-") || has("-makeup-") {
        Category::Lips
    } else if edit.tool == Tool::Mattify || has("-shine") {
        Category::Shine
    } else if edit.tool == Tool::SkinColor || has("-redness") {
        Category::Redness
    } else if edit.tool == Tool::TextureGraft || has("-texture-graft") {
        Category::Texture
    } else if matches!(edit.tool, Tool::SkinSmooth | Tool::Frequency) {
        Category::Smoothing
    } else if edit.tool == Tool::SkinUniformity {
        Category::ToneEvenness
    } else if matches!(edit.tool, Tool::PortraitDodgeBurn | Tool::MicroDodgeBurn) {
        Category::LightEvenness
    } else if matches!(edit.tool, Tool::Dodge | Tool::Burn) || has("-sculpt") {
        Category::Sculpt
    } else {
        Category::Other
    };
    Some(category)
}

fn valid(factor: f32) -> bool {
    factor.is_finite() && (0.0..=MAX_FACTOR).contains(&factor)
}

/// The stored factors. A category that is absent is at 100%.
///
/// # Errors
/// Returns a recipe error when the stored value is malformed or a factor is out of range.
pub fn read(recipe: &Recipe) -> AuraResult<BTreeMap<Category, f32>> {
    let Some(value) = recipe.extra.get(KEY) else {
        return Ok(BTreeMap::new());
    };
    let factors: BTreeMap<Category, f32> = serde_json::from_value(value.clone())
        .map_err(|_| recipe_invalid(KEY, "invalid retouch intensity format"))?;
    if factors.values().any(|f| !valid(*f)) {
        return Err(recipe_invalid(
            KEY,
            "retouch intensity must be between 0% and 200%",
        ));
    }
    Ok(factors)
}

/// Store the factors. Every category given is written explicitly, 100% included, because a
/// merge only carries the values a proposal contains: removing a key would leave the old
/// factor in place.
///
/// # Errors
/// Returns a recipe error when a factor is out of range.
pub fn write(recipe: &mut Recipe, factors: &BTreeMap<Category, f32>) -> AuraResult<()> {
    if factors.values().any(|f| !valid(*f)) {
        return Err(recipe_invalid(
            KEY,
            "retouch intensity must be between 0% and 200%",
        ));
    }
    let value = serde_json::to_value(factors)
        .map_err(|_| recipe_invalid(KEY, "cannot serialize retouch intensity"))?;
    recipe.extra.insert(KEY.into(), value);
    Ok(())
}

/// Scale the automatic operations by their category's factor. Each amount stays within 0..=1.
#[must_use]
pub fn scaled(mut edits: Vec<Edit>, factors: &BTreeMap<Category, f32>) -> Vec<Edit> {
    if factors.is_empty() {
        return edits;
    }
    for edit in &mut edits {
        if let Some(factor) = category_of(edit).and_then(|c| factors.get(&c)) {
            edit.amount = (edit.amount * factor).clamp(0.0, 1.0);
        }
    }
    edits
}

/// The stored stack with every automatic operation scaled by its slider, every operation kept -
/// for a caller that finds operations by id or position.
///
/// # Errors
/// Returns a recipe error when the operations or the factors are malformed.
pub fn scaled_stack(recipe: &Recipe) -> AuraResult<Vec<Edit>> {
    let edits = retouch_tools::read(recipe)?;
    Ok(scaled(edits, &read(recipe)?))
}

/// The operations as the renderer applies them: [`scaled_stack`] without the automatic
/// operations a slider has set to 0%. Those do nothing, and leaving them out is what makes every
/// slider at 0% render exactly the photo without retouch: a stack of operations that do nothing
/// is rendered through the retouch path, which is not bit-identical to no stack at all.
///
/// # Errors
/// Returns a recipe error when the operations or the factors are malformed.
pub fn effective(recipe: &Recipe) -> AuraResult<Vec<Edit>> {
    let factors = read(recipe)?;
    let mut edits = scaled(retouch_tools::read(recipe)?, &factors);
    edits.retain(|e| {
        category_of(e)
            .and_then(|c| factors.get(&c))
            .is_none_or(|f| *f > 0.0)
    });
    Ok(edits)
}

/// The highest factor at which no operation in the category would be capped: `1 / max amount`,
/// at most [`MAX_FACTOR`] and never below 1.
#[must_use]
pub fn headroom(edits: &[Edit], category: Category) -> f32 {
    let top = edits
        .iter()
        .filter(|e| e.enabled && category_of(e) == Some(category))
        .map(|e| e.amount)
        .fold(0.0_f32, f32::max);
    if top <= 0.0 {
        1.0
    } else {
        (1.0 / top).clamp(1.0, MAX_FACTOR)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp, clippy::too_many_lines)]
mod tests {
    use super::*;

    fn op(id: &str, tool: Tool, amount: f32) -> Edit {
        Edit {
            id: id.into(),
            tool,
            enabled: true,
            repair_passes: None,
            region: [0.5, 0.5, 0.1, 0.1],
            source: None,
            source_scale: 1.0,
            amount,
            feather: 0.5,
            radius: 0.01,
            texture: 1.0,
            preserve_microtexture: false,
            texture_heal: false,
            clean_ring_fit: false,
            curved_heal: false,
            heal_samples: Vec::new(),
            texture_sources: Vec::new(),
            sensitivity: None,
            keep_dark_marks: false,
            mole_edge: None,
            tone: 0.5,
            warmth: 0.0,
            tint: 0.0,
            target_color: None,
            mask: None,
            skin: None,
            selection: None,
            matte: None,
        }
    }

    #[test]
    fn automatic_operations_fall_into_the_slider_a_photographer_would_look_for() {
        let cases = [
            (
                "auto-portrait-v1-0-spot-3",
                Tool::AcneClear,
                Category::Blemishes,
            ),
            (
                "auto-portrait-v1-0-clear",
                Tool::FrequencyHeal,
                Category::Blemishes,
            ),
            (
                "auto-portrait-v1-0-texture",
                Tool::SkinSmooth,
                Category::Smoothing,
            ),
            (
                "auto-portrait-v1-0-pores",
                Tool::Frequency,
                Category::Smoothing,
            ),
            (
                "auto-portrait-v1-0-tone",
                Tool::SkinUniformity,
                Category::ToneEvenness,
            ),
            (
                "auto-portrait-v1-0-light",
                Tool::PortraitDodgeBurn,
                Category::LightEvenness,
            ),
            (
                "auto-portrait-v1-0-microdb",
                Tool::MicroDodgeBurn,
                Category::LightEvenness,
            ),
            (
                "auto-portrait-v1-0-texture-graft",
                Tool::TextureGraft,
                Category::Texture,
            ),
            (
                "auto-portrait-v1-0-redness-a",
                Tool::SkinColor,
                Category::Redness,
            ),
            (
                "auto-portrait-v1-0-lines-forehead",
                Tool::Wrinkle,
                Category::Wrinkles,
            ),
            (
                "auto-portrait-v1-0-fold-a",
                Tool::SkinSmooth,
                Category::Wrinkles,
            ),
            (
                "auto-portrait-v1-0-undereye-a",
                Tool::UnderEye,
                Category::UnderEye,
            ),
            (
                "auto-portrait-v1-0-undereye-bag-a",
                Tool::Dodge,
                Category::UnderEye,
            ),
            (
                "auto-portrait-v1-0-eye-a-white",
                Tool::EyeClean,
                Category::Eyes,
            ),
            ("auto-portrait-v1-0-teeth", Tool::Teeth, Category::Teeth),
            (
                "auto-portrait-v1-0-lips-detail",
                Tool::Frequency,
                Category::Lips,
            ),
            ("auto-portrait-v1-0-shine", Tool::Mattify, Category::Shine),
            (
                "auto-portrait-v1-0-facelight",
                Tool::Dodge,
                Category::Sculpt,
            ),
            (
                "auto-portrait-v1-0-sculpt-contour",
                Tool::Burn,
                Category::Sculpt,
            ),
            (
                "auto-portrait-v1-0-body-texture",
                Tool::SkinSmooth,
                Category::BodySkin,
            ),
            (
                "auto-portrait-v1-0-body-shine",
                Tool::Mattify,
                Category::BodySkin,
            ),
            (
                "auto-portrait-v1-0-body-spots",
                Tool::FrequencyHeal,
                Category::BodyBlemishes,
            ),
            (
                "auto-portrait-v1-0-stretch",
                Tool::StretchMarks,
                Category::StretchMarks,
            ),
            (
                "auto-portrait-v1-0-hair-detail",
                Tool::Frequency,
                Category::Hair,
            ),
            ("auto-portrait-v1-stray-2", Tool::Heal, Category::Hair),
            (
                "auto-portrait-v1-0-fabric",
                Tool::Fabric,
                Category::Clothing,
            ),
            ("auto-portrait-v1-1-cloth-0", Tool::Heal, Category::Clothing),
            (
                "auto-portrait-v1-backdrop",
                Tool::Backdrop,
                Category::Background,
            ),
            ("auto-portrait-v1-dust-3", Tool::Heal, Category::Background),
            ("auto-scene-v1-sky", Tool::Dodge, Category::Scene),
        ];
        for (id, tool, expected) in cases {
            assert_eq!(category_of(&op(id, tool, 0.5)), Some(expected), "{id}");
        }
    }

    #[test]
    fn a_person_s_own_operation_is_never_scaled() {
        assert_eq!(
            category_of(&op(
                "manual-auto-portrait-v1-0-texture",
                Tool::SkinSmooth,
                0.5
            )),
            None
        );
        assert_eq!(
            category_of(&op("5b1f0c2e-0000", Tool::SkinSmooth, 0.5)),
            None
        );
        let factors = BTreeMap::from([(Category::Smoothing, 0.0)]);
        let out = scaled(vec![op("5b1f", Tool::SkinSmooth, 0.4)], &factors);
        assert_eq!(out[0].amount, 0.4);
    }

    #[test]
    fn a_factor_scales_only_its_own_category_and_never_past_full_strength() {
        let edits = vec![
            op("auto-portrait-v1-0-texture", Tool::SkinSmooth, 0.6),
            op("auto-portrait-v1-0-spot-0", Tool::AcneClear, 0.8),
        ];
        let factors = BTreeMap::from([(Category::Smoothing, 0.5), (Category::Blemishes, 2.0)]);
        let out = scaled(edits, &factors);
        assert!((out[0].amount - 0.3).abs() < 1e-6);
        assert_eq!(out[1].amount, 1.0);
    }

    #[test]
    fn a_slider_at_zero_takes_its_operations_out_of_the_rendered_stack() {
        let mut recipe = crate::fixtures::reference();
        let edits = vec![
            op("auto-portrait-v1-0-pores", Tool::Frequency, 0.6),
            op("auto-portrait-v1-0-spot-0", Tool::AcneClear, 0.8),
            op("hand-1", Tool::Frequency, 0.5),
        ];
        retouch_tools::write(&mut recipe, &edits).unwrap();
        write(&mut recipe, &BTreeMap::from([(Category::Smoothing, 0.0)])).unwrap();
        let rendered: Vec<String> = effective(&recipe)
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(rendered, vec!["auto-portrait-v1-0-spot-0", "hand-1"]);
        let kept = scaled_stack(&recipe).unwrap();
        assert_eq!(kept.len(), 3);
        assert_eq!(kept[0].amount, 0.0);
    }

    #[test]
    fn headroom_is_how_far_a_slider_can_go_before_an_operation_is_capped() {
        let edits = vec![
            op("auto-portrait-v1-0-texture", Tool::SkinSmooth, 0.5),
            op("auto-portrait-v1-1-texture", Tool::SkinSmooth, 0.8),
            op("auto-portrait-v1-0-spot-0", Tool::AcneClear, 1.0),
        ];
        assert!((headroom(&edits, Category::Smoothing) - 1.25).abs() < 1e-6);
        assert_eq!(headroom(&edits, Category::Blemishes), 1.0);
        assert_eq!(headroom(&edits, Category::Teeth), 1.0);
    }

    #[test]
    fn factors_round_trip_and_out_of_range_is_refused() {
        let mut recipe = crate::fixtures::reference();
        assert!(read(&recipe).unwrap().is_empty());
        let factors = BTreeMap::from([(Category::Blemishes, 0.7), (Category::Teeth, 1.0)]);
        write(&mut recipe, &factors).unwrap();
        assert_eq!(read(&recipe).unwrap(), factors);
        assert!(write(&mut recipe, &BTreeMap::from([(Category::Teeth, 2.5)])).is_err());
        assert!(write(&mut recipe, &BTreeMap::from([(Category::Teeth, f32::NAN)])).is_err());
        recipe
            .extra
            .insert(KEY.into(), serde_json::json!({"teeth": 3.0}));
        assert!(read(&recipe).is_err());
        recipe
            .extra
            .insert(KEY.into(), serde_json::json!({"not_a_slider": 1.0}));
        assert!(read(&recipe).is_err());
    }

    #[test]
    fn every_category_serialises_to_a_distinct_name() {
        let names: std::collections::BTreeSet<String> = Category::ALL
            .iter()
            .map(|c| serde_json::to_string(c).unwrap())
            .collect();
        assert_eq!(names.len(), Category::ALL.len());
    }
}
