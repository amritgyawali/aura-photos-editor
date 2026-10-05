//! Adaptive retouch: one set of fine controls per face, decided from that face. ADR-0086.
//!
//! Every correction the planner makes is already measured, but the *settings* that scale those
//! corrections were one fixed set for every photograph in a batch. A retoucher does not work
//! that way: the same preset is turned down on a face forty pixels wide, on a noisy reception
//! frame and on a face in hard light, and turned up on rough skin in a close-up. This module measures
//! what such a decision rests on and turns the chosen settings into this face's settings.
//!
//! Two rules bound it. The chosen settings are the style: every decision is a *factor* on them,
//! so a preset still means what it says and a control at zero stays off. And nothing here
//! compares anybody with an ideal: each reading is of this face against itself (its own cheek,
//! its own median, its own light), or of the frame (resolution, noise, how many people).
use super::{add, line_energy, luma, robust_spread, Capsule, Geometry, Pixels, SkinReference};
use crate::retouch_settings::Settings;
use aura_vision::portrait::PortraitFace;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Bump on any change to a reading or a rule.
pub const VERSION: &str = "expert-adaptive-v1";

/// What was measured about one face before deciding its settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    /// Pixels between the eyes at analysis resolution: how much detail there is to protect.
    pub eye_px: f32,
    /// Mid-scale luminance irregularity of the cheeks and forehead, relative to their own mean.
    pub roughness: f32,
    /// How uneven the skin's redness is from patch to patch.
    pub blotch: f32,
    /// Stops between the brightest and darkest skin patches: how directional the light is.
    pub light_stops: f32,
    /// Share of the skin that is a specular highlight.
    pub shine: f32,
    /// Compact marks on the cheeks and forehead that are redder or darker than the skin
    /// around them. A count, never a diagnosis.
    pub marks: f32,
    /// Line texture beside the eyes relative to the same person's cheek.
    pub lines: f32,
    /// Linear luminance of this person's skin, used only to recognise a face in deep shadow.
    pub skin_luma: f32,
}

/// What the whole frame contributes to a decision about one face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Context {
    /// Faces large enough to be retouched in this frame.
    pub faces: usize,
    /// Display-referred noise estimate for the frame (see `smart_edit`).
    pub noise: f32,
}

/// One face's settings, with the reasons.
#[derive(Debug, Clone, PartialEq)]
pub struct Tuned {
    pub settings: Settings,
    /// Multiplies the chosen overall strength.
    pub intensity: f32,
    pub notes: Vec<String>,
}

/// What is stored with the face's assessment so the decision can be read back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub version: String,
    pub condition: Condition,
    /// The overall strength this face was retouched at.
    pub intensity: f32,
    /// Only the controls that differ from the chosen ones: name, then chosen and used values.
    pub adjusted: BTreeMap<String, [f32; 2]>,
    pub notes: Vec<String>,
}

fn redness(p: [f32; 3]) -> f32 {
    (p[0] - (p[1] + p[2]) * 0.5) / (p[0] + p[1] + p[2]).max(1e-4)
}

/// Measure one face. `None` when the landmarks or the skin cannot be read.
#[must_use]
pub fn assess(face: &PortraitFace, px: &Pixels<'_>) -> Option<Condition> {
    let g = Geometry::new(face, px)?;
    let skin = SkinReference::measure(&g, px)?;
    let d = g.d;
    let areas = g.skin_areas();
    let exclusions = g.exclusions();
    // Everything is read on a grid over the three skin areas (two cheeks and the forehead).
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, 0.0_f32, 0.0_f32);
    for area in areas.iter().take(3) {
        x0 = x0.min(area.a[0] - area.r);
        y0 = y0.min(area.a[1] - area.r);
        x1 = x1.max(area.a[0] + area.r);
        y1 = y1.max(area.a[1] + area.r);
    }
    let (x0, y0) = (x0.floor().max(0.0) as usize, y0.floor().max(0.0) as usize);
    let (x1, y1) = (
        (x1.ceil().max(0.0) as usize).min(px.width),
        (y1.ceil().max(0.0) as usize).min(px.height),
    );
    if x1 <= x0 + 8 || y1 <= y0 + 8 {
        return None;
    }
    let (w, h) = (x1 - x0, y1 - y0);
    // Patches about an eighth of the eye distance: larger than a pore or a spot, smaller
    // than the shape of a cheek.
    let cell = (0.12 * d).max(3.0);
    // A high-pass at a thirtieth of the eye distance: pores, fine lines and grain.
    let k = ((0.03 * d).round() as usize).max(1);
    let mut inside = vec![false; w * h];
    let mut affine = vec![false; w * h];
    let mut lum = vec![0.0_f32; w * h];
    let mut red = vec![0.0_f32; w * h];
    let (mut total, mut specular, mut affine_count) = (0_usize, 0_usize, 0_usize);
    for y in 0..h {
        for x in 0..w {
            let point = [(x0 + x) as f32 + 0.5, (y0 + y) as f32 + 0.5];
            if !areas.iter().take(3).any(|area| area.contains(point))
                || exclusions.iter().any(|e| e.contains(point))
            {
                continue;
            }
            let p = px.linear(x0 + x, y0 + y);
            let i = y * w + x;
            total += 1;
            let max = p[0].max(p[1]).max(p[2]).max(1e-4);
            let min = p[0].min(p[1]).min(p[2]);
            if luma(p) > (skin.luma * 1.8).max(0.5) && (max - min) / max < 0.25 {
                specular += 1;
            }
            let is_skin = skin.affine(p);
            affine_count += usize::from(is_skin);
            if let (Some(a), Some(b), Some(l), Some(r)) = (
                inside.get_mut(i),
                affine.get_mut(i),
                lum.get_mut(i),
                red.get_mut(i),
            ) {
                *a = true;
                *b = is_skin;
                *l = luma(p);
                *r = redness(p);
            }
        }
    }
    if total < 40 || affine_count < 40 {
        return None;
    }
    let at = |v: &[f32], x: usize, y: usize| v.get(y * w + x).copied().unwrap_or(0.0);
    let flag = |v: &[bool], x: usize, y: usize| v.get(y * w + x).copied().unwrap_or(false);
    // Per patch: skin luminance, redness and fine contrast.
    let mut cells: BTreeMap<(usize, usize), ([f32; 3], f32, f32)> = BTreeMap::new();
    for y in 0..h {
        for x in 0..w {
            if !flag(&affine, x, y) {
                continue;
            }
            let key = ((x as f32 / cell) as usize, (y as f32 / cell) as usize);
            let slot = cells.entry(key).or_insert(([0.0; 3], 0.0, 0.0));
            let l = at(&lum, x, y);
            slot.0[0] += l;
            slot.0[1] += at(&red, x, y);
            slot.1 += 1.0;
            if x >= k && y >= k && x + k < w && y + k < h {
                let around = [(x - k, y), (x + k, y), (x, y - k), (x, y + k)];
                if around.iter().all(|(xx, yy)| flag(&affine, *xx, *yy)) {
                    let mean = around
                        .iter()
                        .map(|(xx, yy)| at(&lum, *xx, *yy))
                        .sum::<f32>()
                        * 0.25;
                    slot.0[2] += (l - mean).abs() / mean.max(1e-4);
                    slot.2 += 1.0;
                }
            }
        }
    }
    let full = cell * cell * 0.5;
    let means: BTreeMap<(usize, usize), [f32; 3]> = cells
        .into_iter()
        .filter(|(_, (_, n, textured))| *n >= full && *textured >= full * 0.5)
        .map(|(key, (sum, n, textured))| (key, [sum[0] / n, sum[1] / n, sum[2] / textured]))
        .collect();
    if means.len() < 4 {
        return None;
    }
    let sorted = |index: usize| {
        let mut values: Vec<f32> = means.values().map(|m| m[index]).collect();
        values.sort_by(f32::total_cmp);
        values
    };
    let lums = sorted(0);
    let light_stops = match (lums.get(lums.len() / 10), lums.get(lums.len() * 9 / 10)) {
        (Some(lo), Some(hi)) => (hi.max(1e-4) / lo.max(1e-4)).log2().max(0.0),
        _ => 0.0,
    };
    let blotch = robust_spread(means.values().map(|m| m[1]).collect());
    // The median patch: a spectacle rim, a strand of hair or a crease raises a few patches
    // and leaves this reading where the skin itself puts it.
    let textures = sorted(2);
    let roughness = textures.get(textures.len() / 2).copied().unwrap_or(0.0);
    // Marks: compact groups of pixels that are redder or darker than their own patch. A
    // crease, a strand of hair and a spectacle rim are long, so they are not counted.
    let mut departs = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            if !flag(&inside, x, y) {
                continue;
            }
            let key = ((x as f32 / cell) as usize, (y as f32 / cell) as usize);
            let Some(mean) = means.get(&key) else {
                continue;
            };
            let level = at(&lum, x, y) / mean[0].max(1e-4);
            let redder = at(&red, x, y) - mean[1] > 0.03 && (0.45..=1.5).contains(&level);
            if redder || (0.3..0.8).contains(&level) {
                if let Some(slot) = departs.get_mut(y * w + x) {
                    *slot = true;
                }
            }
        }
    }
    let largest = (0.11 * d).max(4.0);
    let smallest = (0.012 * d).max(1.0).powi(2);
    let marks = super::deep_blemish::components(&departs, w, h)
        .into_iter()
        .filter(|group| {
            let (mut gx0, mut gy0, mut gx1, mut gy1) = (w, h, 0, 0);
            for i in group {
                gx0 = gx0.min(i % w);
                gx1 = gx1.max(i % w);
                gy0 = gy0.min(i / w);
                gy1 = gy1.max(i / w);
            }
            let (bw, bh) = ((gx1 - gx0 + 1) as f32, (gy1 - gy0 + 1) as f32);
            let n = group.len() as f32;
            n >= smallest
                && bw.max(bh) <= largest
                && bw.max(bh) <= bw.min(bh) * 2.2
                && n >= bw * bh * 0.45
        })
        .count();
    let cheek = areas
        .iter()
        .take(2)
        .filter_map(|zone| line_energy(*zone, &g, &skin, px))
        .reduce(f32::min);
    let corners: Vec<f32> = [(g.eyes[0], -1.0), (g.eyes[1], 1.0)]
        .into_iter()
        .filter_map(|(eye, side)| {
            line_energy(
                Capsule::disk(add(eye, g.u, side * 0.45 * d), 0.13 * d),
                &g,
                &skin,
                px,
            )
        })
        .collect();
    // The quieter corner: hair at one temple or a spectacle arm is not a line.
    let lines = match (cheek, corners.iter().copied().reduce(f32::min)) {
        (Some(cheek), Some(corner)) if corners.len() == 2 => corner / cheek.max(1e-4),
        _ => 1.0,
    };
    Some(Condition {
        eye_px: d,
        roughness,
        blotch,
        light_stops,
        shine: specular as f32 / total as f32,
        marks: marks as f32,
        lines,
        skin_luma: skin.luma,
    })
}

/// Map `v` from `lo..hi` onto `a..b`, clamped at both ends.
fn ramp(v: f32, lo: f32, hi: f32, a: f32, b: f32) -> f32 {
    if !v.is_finite() {
        return a.midpoint(b);
    }
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    a + (b - a) * t
}

fn scale(value: &mut f32, factor: f32) {
    *value = (*value * factor).clamp(0.0, 1.0);
}

/// Turn the chosen settings into this face's settings.
///
/// Readings in the middle of each range leave the chosen value alone; the thresholds were set
/// on the photographs in the real-photo harness and are recorded in ADR-0086.
#[must_use]
pub fn tune(chosen: &Settings, c: &Condition, ctx: &Context) -> Tuned {
    let mut s = *chosen;
    let mut notes = Vec::new();
    let mut intensity = 1.0_f32;

    // 1. How much skin work the skin itself asks for. Smooth skin is left nearly alone; rough
    //    skin gets more, with its pores kept.
    let rough = ramp(c.roughness, 0.012, 0.045, 0.65, 1.45);
    scale(&mut s.smoothing, rough);
    scale(
        &mut s.micro_dodge_burn,
        ramp(c.roughness, 0.012, 0.045, 0.7, 1.4),
    );
    if rough < 0.85 {
        notes.push(format!(
            "Skin is already even (texture reading {:.1}%): smoothing turned down to {:.0}% so it is not over-worked.",
            c.roughness * 100.0,
            s.smoothing * 100.0
        ));
    } else if rough > 1.15 {
        s.texture = s.texture.max(0.6);
        notes.push(format!(
            "Skin texture is uneven (reading {:.1}%): smoothing raised to {:.0}% with pore detail kept at {:.0}%.",
            c.roughness * 100.0,
            s.smoothing * 100.0,
            s.texture * 100.0
        ));
    }

    // 2. Colour evenness follows how blotchy this face's own colour is.
    let blotch = ramp(c.blotch, 0.02, 0.06, 0.85, 1.3);
    scale(&mut s.tone_evenness, blotch);
    scale(&mut s.redness, ramp(c.blotch, 0.02, 0.06, 0.9, 1.25));
    if blotch > 1.12 {
        notes.push(format!(
            "Skin colour is patchy (redness varies by {:.1}%): tone evening raised to {:.0}%.",
            c.blotch * 100.0,
            s.tone_evenness * 100.0
        ));
    } else if blotch < 0.9 {
        notes.push(format!(
            "Skin colour is even already: tone evening held at {:.0}%.",
            s.tone_evenness * 100.0
        ));
    }

    // 3. Directional light is the photograph. Evening it flattens the face, so the harder
    //    the light, the less the light is touched.
    let light = ramp(c.light_stops, 0.7, 1.8, 1.0, 0.45);
    if light < 0.9 {
        scale(&mut s.light_evenness, light);
        scale(&mut s.face_light, light);
        notes.push(format!(
            "Directional light ({:.1} stops across the skin): light evening reduced to {:.0}% to keep the face's shape.",
            c.light_stops,
            s.light_evenness * 100.0
        ));
    }

    // 4. Shine.
    let shine = ramp(c.shine, 0.01, 0.08, 0.8, 1.5);
    scale(&mut s.shine, shine);
    if shine > 1.15 {
        notes.push(format!(
            "Shiny skin ({:.1}% specular): shine control raised to {:.0}%.",
            c.shine * 100.0,
            s.shine * 100.0
        ));
    }

    // 5. Many marks: search the whole face rather than four patches. Dark marks stay
    //    protected - choosing to remove a mole is the photographer's decision, never this one.
    if c.marks >= 26.0 && c.eye_px >= 100.0 && !s.deep_blemish_cleanup {
        s.deep_blemish_cleanup = true;
        s.max_spots = ramp(c.marks, 26.0, 70.0, 60.0, 120.0).round() as u8;
        s.blemish_sensitivity = s.blemish_sensitivity.max(0.65);
        s.texture = s.texture.max(0.7);
        notes.push(format!(
            "Many small marks ({:.0} compact marks counted on the cheeks and forehead): whole-face cleanup of up to {} spots, dark marks protected{}.",
            c.marks,
            s.max_spots,
            if s.keep_freckles { ", freckle fields kept" } else { "" }
        ));
    } else if c.marks < 2.0 {
        scale(&mut s.blemish_sensitivity, 0.85);
    }

    // 6. Viewing size. A small face has no pores to keep and shows smoothing as smear; a
    //    close-up shows every pore, so more of them are kept and unevenness is treated broader.
    if c.eye_px < 45.0 {
        let k = ramp(c.eye_px, 20.0, 45.0, 0.45, 0.8);
        scale(&mut s.smoothing, k);
        scale(&mut s.micro_dodge_burn, 0.5);
        scale(&mut s.iris_detail, 0.5);
        scale(&mut s.lash_definition, 0.5);
        s.pore_refine = 0.0;
        notes.push(format!(
            "Small face ({:.0} px between the eyes): skin work reduced to {:.0}% and fine eye detail halved.",
            c.eye_px,
            s.smoothing * 100.0
        ));
    } else if c.eye_px > 170.0 {
        s.texture = s.texture.max(0.7);
        s.smoothing_size = (s.smoothing_size * 1.25).min(1.0);
        notes.push(format!(
            "Close-up ({:.0} px between the eyes): pore detail kept at {:.0}% and unevenness treated at a broader size.",
            c.eye_px,
            s.texture * 100.0
        ));
    }

    // 7. Noise. Clean skin on a noisy frame reads as plastic, and eye sharpening sharpens
    //    grain, so both are held back.
    if ctx.noise > 0.012 {
        let k = ramp(ctx.noise, 0.012, 0.03, 0.9, 0.65);
        scale(&mut s.smoothing, k);
        s.texture = (s.texture + 0.15).min(1.0);
        scale(&mut s.iris_detail, k);
        scale(&mut s.lash_definition, k);
        scale(&mut s.brow_definition, k);
        scale(&mut s.hair_detail, k);
        notes.push(format!(
            "Noisy frame ({:.1}% of full scale): smoothing held to {:.0}% and detail sharpening reduced so the skin matches the grain around it.",
            ctx.noise * 100.0,
            s.smoothing * 100.0
        ));
    }

    // 8. Lines that are much stronger than the cheek are the person's face, not a flaw: they
    //    are softened less, so nobody is made to look like somebody else.
    if c.lines > 3.0 {
        let k = ramp(c.lines, 3.0, 5.0, 0.85, 0.65);
        for line in [
            &mut s.forehead_lines,
            &mut s.crows_feet,
            &mut s.smile_lines,
            &mut s.under_eye_lines,
            &mut s.neck_lines,
        ] {
            scale(line, k);
        }
        s.texture = s.texture.max(0.6);
        notes.push(format!(
            "Established expression lines ({:.1}x the cheek's texture): line softening reduced to {:.0}% of the chosen amount to keep character.",
            c.lines,
            k * 100.0
        ));
    }

    // 9. A face in deep shadow carries the frame's noise; evening it lifts that noise.
    if c.skin_luma < 0.045 {
        scale(&mut s.micro_dodge_burn, 0.6);
        scale(&mut s.smoothing, 0.85);
        notes.push(
            "Face in deep shadow: micro dodge and burn reduced so shadow noise is not lifted."
                .into(),
        );
    }

    // 10. Groups are retouched lighter than a portrait: everyone must still match everyone.
    if ctx.faces >= 3 {
        intensity = if ctx.faces >= 6 { 0.75 } else { 0.85 };
        notes.push(format!(
            "Group of {}: overall strength {:.0}% so every face is finished to the same light touch.",
            ctx.faces,
            intensity * 100.0
        ));
    }

    if notes.is_empty() {
        notes.push("Measured within the normal range on every reading: the chosen settings were used as they are.".into());
    }
    Tuned {
        settings: s.sanitised(),
        intensity,
        notes,
    }
}

/// The record stored with a face's assessment.
#[must_use]
pub fn summary(chosen: &Settings, tuned: &Tuned, condition: Condition, intensity: f32) -> Summary {
    let numbers = |s: &Settings| -> BTreeMap<String, f32> {
        match serde_json::to_value(s) {
            Ok(serde_json::Value::Object(map)) => map
                .into_iter()
                .filter_map(|(k, v)| {
                    let n = match v {
                        serde_json::Value::Bool(b) => f32::from(u8::from(b)),
                        other => other.as_f64()? as f32,
                    };
                    Some((k, n))
                })
                .collect(),
            _ => BTreeMap::new(),
        }
    };
    let before = numbers(chosen);
    let adjusted = numbers(&tuned.settings)
        .into_iter()
        .filter_map(|(k, after)| {
            let was = before.get(&k).copied()?;
            ((was - after).abs() > 0.005).then_some((k, [was, after]))
        })
        .collect();
    Summary {
        version: VERSION.into(),
        condition,
        intensity,
        adjusted,
        notes: tuned.notes.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn typical() -> Condition {
        Condition {
            eye_px: 110.0,
            roughness: 0.026,
            blotch: 0.035,
            light_stops: 0.5,
            shine: 0.03,
            marks: 6.0,
            lines: 1.4,
            skin_luma: 0.3,
        }
    }

    const CALM: Context = Context {
        faces: 1,
        noise: 0.004,
    };

    #[test]
    fn a_typical_face_keeps_the_chosen_settings_within_a_small_margin() {
        let chosen = Settings::default();
        let t = tune(&chosen, &typical(), &CALM);
        assert!((t.settings.smoothing - chosen.smoothing).abs() < 0.06);
        assert!((t.settings.tone_evenness - chosen.tone_evenness).abs() < 0.06);
        assert_eq!(t.settings.light_evenness, chosen.light_evenness);
        assert!(!t.settings.deep_blemish_cleanup);
        assert_eq!(t.intensity, 1.0);
    }

    #[test]
    fn two_different_faces_get_two_different_sets_of_settings() {
        let chosen = Settings::default();
        let smooth = Condition {
            roughness: 0.01,
            blotch: 0.01,
            ..typical()
        };
        let rough = Condition {
            roughness: 0.05,
            blotch: 0.07,
            shine: 0.09,
            ..typical()
        };
        let (a, b) = (tune(&chosen, &smooth, &CALM), tune(&chosen, &rough, &CALM));
        assert!(a.settings.smoothing < chosen.smoothing && chosen.smoothing < b.settings.smoothing);
        assert!(a.settings.tone_evenness < b.settings.tone_evenness);
        assert!(b.settings.shine > chosen.shine);
        assert!(b.settings.texture >= 0.6, "rough skin keeps its pores");
        assert_ne!(a.notes, b.notes);
    }

    #[test]
    fn context_restrains_the_retouch() {
        let chosen = Settings::default();
        let base = tune(&chosen, &typical(), &CALM).settings;
        let small = tune(
            &chosen,
            &Condition {
                eye_px: 30.0,
                ..typical()
            },
            &CALM,
        )
        .settings;
        assert!(small.smoothing < base.smoothing && small.iris_detail < base.iris_detail);
        let noisy = tune(
            &chosen,
            &typical(),
            &Context {
                faces: 1,
                noise: 0.025,
            },
        )
        .settings;
        assert!(noisy.smoothing < base.smoothing && noisy.texture > base.texture);
        let hard = tune(
            &chosen,
            &Condition {
                light_stops: 2.0,
                ..typical()
            },
            &CALM,
        )
        .settings;
        assert!(hard.light_evenness < base.light_evenness * 0.6);
        let lined = tune(
            &chosen,
            &Condition {
                lines: 4.5,
                ..typical()
            },
            &CALM,
        )
        .settings;
        assert!(lined.crows_feet < base.crows_feet && lined.forehead_lines < base.forehead_lines);
        let group = tune(
            &chosen,
            &typical(),
            &Context {
                faces: 7,
                noise: 0.004,
            },
        );
        assert_eq!(group.intensity, 0.75);
    }

    #[test]
    fn dense_marks_search_the_whole_face_and_never_choose_to_remove_dark_marks() {
        let chosen = Settings::default();
        let acne = Condition {
            marks: 40.0,
            ..typical()
        };
        let t = tune(&chosen, &acne, &CALM).settings;
        assert!(t.deep_blemish_cleanup && t.max_spots >= 60 && t.max_spots <= 120);
        assert!(!t.remove_dark_marks && t.keep_freckles);
        // Too small to tell a spot from a pore: the sparse search stays.
        let far = Condition {
            eye_px: 60.0,
            ..acne
        };
        assert!(!tune(&chosen, &far, &CALM).settings.deep_blemish_cleanup);
    }

    #[test]
    fn a_control_at_zero_stays_off_and_every_value_stays_in_range() {
        let chosen = Settings {
            smoothing: 0.0,
            shine: 0.0,
            tone_evenness: 1.0,
            ..Settings::default()
        };
        let wild = Condition {
            eye_px: 400.0,
            roughness: 9.0,
            blotch: 9.0,
            light_stops: 9.0,
            shine: 1.0,
            marks: 500.0,
            lines: f32::NAN,
            skin_luma: 0.0,
        };
        let t = tune(
            &chosen,
            &wild,
            &Context {
                faces: 40,
                noise: 1.0,
            },
        );
        assert_eq!(t.settings.smoothing, 0.0);
        assert_eq!(t.settings.shine, 0.0);
        assert_eq!(t.settings, t.settings.sanitised());
        assert!(t.settings.tone_evenness <= 1.0 && t.intensity >= 0.75);
        let s = summary(&chosen, &t, wild, 0.75);
        assert!(s.adjusted.contains_key("deepBlemishCleanup"));
        assert!(!s.adjusted.contains_key("smoothing"));
    }

    #[test]
    fn a_painted_face_is_measured_and_a_rougher_one_reads_rougher() {
        let size = 400_usize;
        let face = PortraitFace {
            bounds: [0.2, 0.15, 0.8, 0.9],
            landmarks: [
                [0.38, 0.42],
                [0.62, 0.42],
                [0.5, 0.55],
                [0.42, 0.68],
                [0.58, 0.68],
            ],
            confidence: 0.95,
        };
        let paint = |grain: i32, red_spots: bool| -> Vec<u8> {
            let mut rgb = Vec::with_capacity(size * size * 3);
            for y in 0..size {
                for x in 0..size {
                    // A deterministic blocky texture, four pixels to a block.
                    let h = ((x / 4) * 7 + (y / 4) * 13) % 5;
                    let v = (h as i32 - 2) * grain;
                    let spot = red_spots && (x / 4 + y / 4) % 9 == 0;
                    let c = |base: i32, extra: i32| (base + v + extra).clamp(0, 255) as u8;
                    rgb.extend([
                        c(200, if spot { 20 } else { 0 }),
                        c(150, if spot { -30 } else { 0 }),
                        c(125, if spot { -30 } else { 0 }),
                    ]);
                }
            }
            rgb
        };
        let (calm, rough, spotty) = (paint(1, false), paint(9, false), paint(1, true));
        let read = |rgb: &[u8]| {
            let px = Pixels::new(rgb, size as u32, size as u32).unwrap();
            assess(&face, &px).unwrap()
        };
        let (a, b, c) = (read(&calm), read(&rough), read(&spotty));
        assert!((a.eye_px - 96.0).abs() < 1.0);
        assert!(
            b.roughness > a.roughness * 3.0,
            "{} {}",
            a.roughness,
            b.roughness
        );
        assert!(c.marks > a.marks + 20.0, "{} {}", a.marks, c.marks);
        assert!(a.shine < 0.01);
        assert_eq!(read(&calm), a, "deterministic");
    }
}
