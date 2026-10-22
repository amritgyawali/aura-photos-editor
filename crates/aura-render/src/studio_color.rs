//! Independent colour operators. Stored control points operate in the existing curve domain;
//! the working buffer remains linear Rec.2020 and receives no output transfer here.
#![allow(clippy::indexing_slicing)]
use crate::{
    cpu::Frame,
    tonemap::{curve_domain_decode as decode, curve_domain_encode as encode},
};
use aura_recipe::studio_color::{LevelChannel, Levels, SelectiveColor};
use aura_recipe::Recipe;
use rayon::prelude::*;

fn luma(p: [f32; 3]) -> f32 {
    p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593
}
fn channel(v: f32, c: &LevelChannel) -> f32 {
    if *c == LevelChannel::default() {
        return v;
    }
    let x = ((encode(v) - c.input_black) / (c.input_white - c.input_black))
        .clamp(0.0, 1.0)
        .powf(1.0 / c.gamma);
    decode(c.output_black + x * (c.output_white - c.output_black))
}
/// Levels in RGB or hue-preserving luminance mode, followed by individual channels.
#[must_use]
pub fn levels(rgb: [f32; 3], settings: &Levels) -> [f32; 3] {
    let base = if settings.rgb == LevelChannel::default() {
        rgb
    } else if settings.luminosity {
        crate::colour::set_luma(rgb, channel(luma(rgb), &settings.rgb))
    } else {
        rgb.map(|v| channel(v, &settings.rgb))
    };
    [
        channel(base[0], &settings.red),
        channel(base[1], &settings.green),
        channel(base[2], &settings.blue),
    ]
}
/// Soft overlapping six hue ranges plus whites, neutrals and blacks.
fn range_weight(name: &str, rgb: [f32; 3]) -> f32 {
    let encoded = rgb.map(|v| encode(v).clamp(0.0, 1.0));
    let (h, s, v) = crate::tonemap::to_hsv(encoded);
    match name {
        "whites" => ((v - 0.5) * 2.0).clamp(0.0, 1.0) * (1.0 - s),
        "blacks" => (1.0 - v * 2.0).clamp(0.0, 1.0),
        "neutrals" => (1.0 - (v * 2.0 - 1.0).abs()) * (1.0 - s),
        _ => {
            let centre = match name {
                "reds" => 0.0,
                "yellows" => 1.0 / 6.0,
                "greens" => 2.0 / 6.0,
                "cyans" => 0.5,
                "blues" => 4.0 / 6.0,
                "magentas" => 5.0 / 6.0,
                _ => return 0.0,
            };
            let d = (h - centre).abs();
            (1.0 - d.min(1.0 - d) * 6.0).max(0.0) * s
        }
    }
}
/// Relative CMYK scales the existing ink component; absolute mode adds ink directly.
#[must_use]
pub fn selective(rgb: [f32; 3], settings: &SelectiveColor) -> [f32; 3] {
    if settings.ranges.is_empty() {
        return rgb;
    }
    let mut delta = [0.0; 3];
    let encoded = rgb.map(|v| encode(v).clamp(0.0, 1.0));
    for (name, c) in &settings.ranges {
        let k = range_weight(name, rgb) / 100.0;
        for (i, ink) in [c.cyan, c.magenta, c.yellow].iter().enumerate() {
            let scale = if settings.absolute {
                1.0
            } else {
                1.0 - encoded[i]
            };
            let black_scale = if settings.absolute {
                1.0
            } else {
                1.0 - encoded.iter().copied().fold(0.0_f32, f32::max)
            };
            delta[i] -= k * (ink * scale + c.black * black_scale);
        }
    }
    // Preserve highlights above the nominal white rather than clamping an unrelated range.
    [0, 1, 2].map(|i| {
        if delta[i] == 0.0 {
            rgb[i]
        } else {
            decode((encode(rgb[i]) + delta[i]).max(0.0))
        }
    })
}

pub(crate) fn apply(
    rgb: &mut [f32],
    width: u32,
    height: u32,
    recipe: &Recipe,
    frame: Option<&Frame>,
) {
    let Ok(settings) = aura_recipe::studio_color::read(recipe) else {
        return;
    };
    if settings == aura_recipe::studio_color::StudioColor::default() {
        return;
    }
    rgb.par_chunks_mut(3).for_each(|p| {
        let out = selective(
            levels([p[0], p[1], p[2]], &settings.levels),
            &settings.selective,
        );
        p.copy_from_slice(&out);
    });
    let Ok(masks) = aura_recipe::local_masks::read(recipe) else {
        return;
    };
    for mask in masks.iter().filter(|m| m.enabled && m.amount > 0.0) {
        let Some(local) = settings.local.get(&mask.id) else {
            continue;
        };
        if *local == aura_recipe::studio_color::LocalColor::default() {
            continue;
        }
        let Some(mut weight) =
            crate::local_masks::coverage(rgb, width, height, recipe, frame, &mask.id)
        else {
            continue;
        };
        for value in &mut weight {
            *value *= mask.amount;
        }
        let lut = crate::tonemap::CurveLut::build(&local.curve);
        let grade = crate::creative::GradePlan::new(&local.grade);
        let mut adjusted = rgb.to_vec();
        adjusted.par_chunks_mut(3).for_each(|p| {
            let p0 = [p[0], p[1], p[2]];
            let p1 = crate::tonemap::apply_curve(p0, &lut);
            let p2 = crate::tonemap::hsl(p1, &local.hsl);
            let p3 = crate::tonemap::vibrance_saturation(p2, local.vibrance / 100.0, 0.0);
            p.copy_from_slice(&grade.apply(p3));
        });
        if local.sharpen > 0.0 || local.dehaze != 0.0 {
            let stats = crate::spatial::measure(&adjusted, width as usize, height as usize);
            crate::spatial::unsharp(
                &mut adjusted,
                width as usize,
                height as usize,
                1.0,
                local.sharpen / 100.0,
                0.5,
                0.0,
                &stats,
            );
            crate::spatial::dehaze(
                &mut adjusted,
                width as usize,
                height as usize,
                local.dehaze / 100.0,
                &stats,
            );
        }
        rgb.par_chunks_mut(3)
            .zip(adjusted.par_chunks(3))
            .zip(weight.par_iter())
            .for_each(|((p, q), k)| {
                if *k > 0.0 {
                    for i in 0..3 {
                        p[i] += (q[i] - p[i]) * k;
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn levels_are_identity_by_default_and_luminosity_preserves_ratios() {
        let p = [0.12, 0.3, 1.4];
        assert_eq!(
            levels(p, &Levels::default()).map(f32::to_bits),
            p.map(f32::to_bits)
        );
        let mut c = Levels::default();
        c.rgb.gamma = 1.4;
        c.luminosity = true;
        let q = levels(p, &c);
        assert!((q[0] / q[1] - p[0] / p[1]).abs() < 1e-5);
        assert_ne!(p.map(f32::to_bits), q.map(f32::to_bits));
    }
    #[test]
    fn selective_ranges_and_modes_are_real_and_distinct() {
        let p = [0.5, 0.1, 0.08];
        let mut s = SelectiveColor::default();
        s.ranges.insert(
            "reds".into(),
            aura_recipe::studio_color::Cmyk {
                cyan: 30.0,
                ..Default::default()
            },
        );
        let relative = selective(p, &s);
        s.absolute = true;
        let absolute = selective(p, &s);
        assert!(absolute[0] < relative[0] && relative[0] < p[0]);
        assert_eq!(absolute[1].to_bits(), p[1].to_bits());
        let blue = [0.01, 0.01, 0.8];
        assert_eq!(
            selective(blue, &s).map(f32::to_bits),
            blue.map(f32::to_bits)
        );
    }
    #[test]
    fn local_colour_preserves_pixels_outside_mask_and_zero_amount() {
        let mut recipe = aura_recipe::fixtures::reference();
        recipe.extra.clear();
        let mask = serde_json::json!({"id":"left","name":"Left","enabled":true,"amount":1.0,"components":[{"mode":"add","source":{"type":"linear","start":[0.0,0.0],"end":[0.5,0.0]}}],"params":{}});
        recipe.extra.insert(
            aura_recipe::local_masks::KEY.into(),
            serde_json::json!([mask]),
        );
        let mut settings = aura_recipe::studio_color::StudioColor::default();
        settings.local.insert(
            "left".into(),
            aura_recipe::studio_color::LocalColor {
                curve: aura_recipe::Curve {
                    points: vec![[0, 0], [128, 190], [255, 255]],
                },
                vibrance: 80.0,
                ..Default::default()
            },
        );
        aura_recipe::studio_color::write(&mut recipe, &settings).unwrap();
        let source = [0.12, 0.3, 0.4].repeat(16);
        let mut actual = source.clone();
        apply(&mut actual, 8, 2, &recipe, None);
        assert_ne!(&actual[..3], &source[..3]);
        assert_eq!(&actual[15..24], &source[15..24]);
        recipe.extra.get_mut(aura_recipe::local_masks::KEY).unwrap()[0]["amount"] =
            serde_json::json!(0.0);
        actual = source.clone();
        apply(&mut actual, 8, 2, &recipe, None);
        assert_eq!(actual, source);
    }
    #[test]
    fn every_extended_local_operator_changes_selected_pixels_without_leaking() {
        let mut recipe =
            aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "fixture");
        recipe.extra.insert(aura_recipe::local_masks::KEY.into(),serde_json::json!([{
            "id":"left","name":"Left","enabled":true,"amount":1.0,
            "components":[{"mode":"add","source":{"type":"linear","start":[0.0,0.0],"end":[0.5,0.0]}}],"params":{}
        }]));
        let source: Vec<f32> = (0..32 * 16)
            .flat_map(|i| {
                let edge = if (i % 32) % 4 < 2 { 0.08 } else { 0.0 };
                [0.38 + edge, 0.14 + edge, 0.1 + edge]
            })
            .collect();
        for name in ["curve", "hsl", "grade", "sharpen", "dehaze", "vibrance"] {
            let mut local = aura_recipe::studio_color::LocalColor::default();
            match name {
                "curve" => local.curve.points = vec![[0, 0], [128, 180], [255, 255]],
                "hsl" => {
                    local.hsl.insert(
                        "red".into(),
                        aura_recipe::HslShift {
                            h: 20,
                            s: -50,
                            l: 30,
                        },
                    );
                }
                "grade" => {
                    local.grade.shadows.hue = 230;
                    local.grade.shadows.saturation = 60;
                }
                "sharpen" => local.sharpen = 100.0,
                "dehaze" => local.dehaze = 60.0,
                _ => local.vibrance = 80.0,
            }
            let mut settings = aura_recipe::studio_color::StudioColor::default();
            settings.local.insert("left".into(), local);
            aura_recipe::studio_color::write(&mut recipe, &settings).unwrap();
            let mut actual = source.clone();
            apply(&mut actual, 32, 16, &recipe, None);
            let changed = (0..16).any(|y| {
                (0..10 * 3).any(|x| (actual[y * 32 * 3 + x] - source[y * 32 * 3 + x]).abs() > 1e-5)
            });
            assert!(changed, "{name} must affect selected pixels");
            for y in 0..16 {
                assert_eq!(
                    &actual[(y * 32 + 20) * 3..(y * 32 + 32) * 3],
                    &source[(y * 32 + 20) * 3..(y * 32 + 32) * 3],
                    "{name} leaks outside mask"
                );
            }
        }
    }
}
