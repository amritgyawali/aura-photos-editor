//! Independently implemented Levels, selective CMYK colour, and extended local colour.
use crate::{errors::recipe_invalid, ColourGrade, Curve, HslShift, Recipe};
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const KEY: &str = "studio_color_v1";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct LevelChannel {
    pub input_black: f32,
    pub input_white: f32,
    pub gamma: f32,
    pub output_black: f32,
    pub output_white: f32,
}
impl Default for LevelChannel {
    fn default() -> Self {
        Self {
            input_black: 0.0,
            input_white: 1.0,
            gamma: 1.0,
            output_black: 0.0,
            output_white: 1.0,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Levels {
    pub rgb: LevelChannel,
    pub red: LevelChannel,
    pub green: LevelChannel,
    pub blue: LevelChannel,
    pub luminosity: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Cmyk {
    pub cyan: f32,
    pub magenta: f32,
    pub yellow: f32,
    pub black: f32,
}
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct SelectiveColor {
    pub absolute: bool,
    pub ranges: BTreeMap<String, Cmyk>,
}
pub const SELECTIVE_RANGES: [&str; 9] = [
    "reds", "yellows", "greens", "cyans", "blues", "magentas", "whites", "neutrals", "blacks",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct LocalColor {
    pub curve: Curve,
    pub hsl: BTreeMap<String, HslShift>,
    pub grade: ColourGrade,
    pub sharpen: f32,
    pub dehaze: f32,
    pub vibrance: f32,
}
impl Default for LocalColor {
    fn default() -> Self {
        Self {
            curve: Curve::identity(),
            hsl: BTreeMap::new(),
            grade: ColourGrade::default(),
            sharpen: 0.0,
            dehaze: 0.0,
            vibrance: 0.0,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct StudioColor {
    pub levels: Levels,
    pub selective: SelectiveColor,
    pub local: BTreeMap<String, LocalColor>,
}

fn bounded(v: f32, lo: f32, hi: f32) -> bool {
    v.is_finite() && (lo..=hi).contains(&v)
}
/// Validate every value before storage or rendering.
/// # Errors
/// Refuses nonfinite values, inverted endpoints, oversized curves and unknown colour bands.
pub fn validate(value: &StudioColor) -> AuraResult<()> {
    let channel = |c: &LevelChannel| {
        bounded(c.input_black, 0.0, 1.0)
            && bounded(c.input_white, 0.0, 1.0)
            && c.input_white - c.input_black >= 0.001
            && bounded(c.gamma, 0.1, 10.0)
            && bounded(c.output_black, 0.0, 1.0)
            && bounded(c.output_white, 0.0, 1.0)
            && c.output_black <= c.output_white
    };
    let levels = &value.levels;
    let mut valid = [&levels.rgb, &levels.red, &levels.green, &levels.blue]
        .iter()
        .all(|c| channel(c));
    valid &= value.selective.ranges.len() <= 9
        && value.selective.ranges.iter().all(|(name, c)| {
            SELECTIVE_RANGES.contains(&name.as_str())
                && [c.cyan, c.magenta, c.yellow, c.black]
                    .iter()
                    .all(|v| bounded(*v, -100.0, 100.0))
        });
    valid &= value.local.len() <= 32
        && value.local.iter().all(|(id, c)| {
            let curve = &c.curve.points;
            let wheel = |w: &crate::GradeWheel| {
                (0..=359).contains(&w.hue)
                    && (0..=100).contains(&w.saturation)
                    && (-100..=100).contains(&w.luminance)
            };
            !id.is_empty()
                && id.len() <= 100
                && (2..=32).contains(&curve.len())
                && curve.iter().flatten().all(|v| *v <= 255)
                && curve.windows(2).all(|w| {
                    w.first()
                        .zip(w.get(1))
                        .is_some_and(|(a, b)| a.first() < b.first())
                })
                && c.hsl.len() <= 8
                && c.hsl.iter().all(|(band, v)| {
                    crate::HSL_BANDS.contains(&band.as_str())
                        && [v.h, v.s, v.l].iter().all(|v| (-100..=100).contains(v))
                })
                && [
                    &c.grade.shadows,
                    &c.grade.midtones,
                    &c.grade.highlights,
                    &c.grade.global,
                ]
                .iter()
                .all(|w| wheel(w))
                && (0..=100).contains(&c.grade.blending)
                && (-100..=100).contains(&c.grade.balance)
                && bounded(c.sharpen, 0.0, 150.0)
                && bounded(c.dehaze, -100.0, 100.0)
                && bounded(c.vibrance, -100.0, 100.0)
        });
    if valid {
        Ok(())
    } else {
        Err(recipe_invalid(
            KEY,
            "Invalid Levels, selective colour or local colour controls",
        ))
    }
}
/// Read default-neutral settings from an old or current recipe.
/// # Errors
/// Refuses malformed or invalid stored settings.
pub fn read(recipe: &Recipe) -> AuraResult<StudioColor> {
    let value = recipe.extra.get(KEY).map_or_else(
        || Ok(StudioColor::default()),
        |v| {
            serde_json::from_value(v.clone())
                .map_err(|_| recipe_invalid(KEY, "Invalid colour controls"))
        },
    )?;
    validate(&value)?;
    Ok(value)
}
/// Store validated settings without altering any other recipe field.
/// # Errors
/// Refuses invalid settings or serialization failure.
pub fn write(recipe: &mut Recipe, value: &StudioColor) -> AuraResult<()> {
    validate(value)?;
    recipe.extra.insert(
        KEY.into(),
        serde_json::to_value(value)
            .map_err(|_| recipe_invalid(KEY, "Cannot encode colour controls"))?,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_collapsed_levels_and_nonfinite_local() {
        let mut c = StudioColor::default();
        c.levels.rgb.input_white = 0.0;
        assert!(validate(&c).is_err());
        c = StudioColor::default();
        c.local.insert(
            "mask".into(),
            LocalColor {
                vibrance: f32::NAN,
                ..LocalColor::default()
            },
        );
        assert!(validate(&c).is_err());
    }
    #[test]
    fn validates_curve_order_and_known_ranges() {
        let mut c = StudioColor::default();
        c.selective.ranges.insert("skin".into(), Cmyk::default());
        assert!(validate(&c).is_err());
        c = StudioColor::default();
        c.local.insert(
            "mask".into(),
            LocalColor {
                curve: Curve {
                    points: vec![[0, 0], [128, 150], [127, 200], [255, 255]],
                },
                ..LocalColor::default()
            },
        );
        assert!(validate(&c).is_err());
    }
}
