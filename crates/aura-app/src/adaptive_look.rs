//! The finishing look a photographer adds after the corrections, decided for each photograph
//! from that photograph alone.
//!
//! [`crate::smart_edit`] repairs what is wrong: exposure, a cast, a veil, noise. A finished
//! JPEG with nothing wrong in it comes out of that pass almost untouched, and a wedding of a
//! thousand such frames came out of it with the same small shadow lift and vibrance on every
//! one. This pass is the other half of an edit: depth, colour that holds together, and a light
//! split tone. Every amount is measured here, so two frames get the same look only when they
//! are the same picture:
//!
//! - **Depth.** A parametric S-curve sized by how far the frame's own tonal spread falls
//!   short of what its kind of scene usually has, and an opening of the darks when a hard
//!   frame has too much. Low-light frames are lifted in their deepest tones instead, where the
//!   shadows slider does not reach, and lose any contrast that would darken them again.
//! - **Colour.** Each hue band that covers enough of the frame and is dull is enriched; a band
//!   already at the edge of the gamut (stage LEDs) is calmed. A band that skin falls in is
//!   left alone in proportion to how much of the skin it holds.
//! - **Split tone.** Warm highlights and cool shadows, weighted by the frame's own warmth: a
//!   tungsten room is not warmed further, a cool stage gets warmth back in its highlights.
//! - **Skin is measured, not trusted.** The colour half of the look is run through the
//!   renderer's own HSL and grade on this frame's own skin samples, in the working space, and
//!   scaled back until skin moves less than [`SKIN_DE_LIMIT`]. A look that would change
//!   somebody's skin is a look that is withdrawn, not one that is shipped with a note.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::too_many_lines
)]

use std::collections::BTreeMap;

use aura_recipe::{ColourGrade, GradeWheel, HslShift, ParametricCurve};
use aura_render::creative::GradePlan;
use aura_render::output::display_srgb_to_working;
use aura_render::tonemap::{band_weights, hsl, to_hsv, vibrance_saturation};
use aura_vision::portrait::PortraitFace;

use crate::portrait_features::Pixels;
use crate::smart_edit::SceneKind;

/// Largest mean colour difference (CIE76 in the working space) the look may make on this
/// frame's own skin. Under about 2 a difference is not visible side by side.
pub const SKIN_DE_LIMIT: f32 = 1.5;
/// Largest mean hue rotation, in degrees, the look may make on skin.
pub const SKIN_HUE_LIMIT: f32 = 2.0;
/// Most any one band's saturation is moved.
const BAND_LIMIT: f32 = 20.0;
/// Most the S-curve moves the lights or darks.
const DEPTH_LIMIT: f32 = 22.0;
/// Most vibrance the look adds on top of the correction's own.
const VIBRANCE_LIMIT: f32 = 12.0;

/// What the pass decided.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Look {
    pub parametric: ParametricCurve,
    pub hsl: BTreeMap<String, HslShift>,
    pub grade: ColourGrade,
    /// Vibrance added to what the corrections set, from how colourful the frame already is.
    /// The caller folds it into the corrections' own vibrance before either is written.
    pub vibrance: i16,
    /// A replacement for the correction's contrast: low-light frames lose the part that
    /// would darken their deepest tones.
    pub contrast: Option<i16>,
    /// The colour shift the look makes on this frame's skin, as measured, when it has people.
    pub skin_shift: Option<f32>,
    pub decisions: Vec<String>,
}

impl Look {
    /// True when the look changes nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parametric.is_flat()
            && self.hsl.values().all(HslShift::is_neutral)
            && self.grade.is_neutral()
            && self.vibrance == 0
            && self.contrast.is_none()
    }
}

/// Off only for comparisons: `AURA_ADAPTIVE_LOOK=0` leaves every photograph as the
/// corrections left it.
#[must_use]
pub fn enabled() -> bool {
    std::env::var("AURA_ADAPTIVE_LOOK").map_or(true, |v| v.trim() != "0")
}

/// Measurements the look is decided from, on display-encoded pixels.
#[derive(Debug, Clone, Default)]
struct Reading {
    /// Spread and placement of display lightness outside the faces.
    spread: f32,
    median: f32,
    bright_share: f32,
    dark_share: f32,
    /// Mean of red minus blue over midtones outside the faces: the frame's warmth.
    warmth: f32,
    /// Per band: share of the coloured pixels outside faces, and their mean saturation in the
    /// working space (the space the renderer's bands are defined in).
    band_share: [f32; 8],
    band_sat: [f32; 8],
    coloured: usize,
    /// Mean display saturation outside the faces.
    saturation: f32,
    /// Skin samples inside the faces, display-encoded.
    skin: Vec<[f32; 3]>,
}

fn in_face(faces: &[PortraitFace], x: f32, y: f32) -> bool {
    faces.iter().any(|f| {
        let [l, t, r, b] = f.bounds;
        let (w, h) = (r - l, b - t);
        x >= l - w * 0.3 && x <= r + w * 0.3 && y >= t - h * 0.3 && y <= b + h * 0.8
    })
}

fn in_face_core(faces: &[PortraitFace], x: f32, y: f32) -> bool {
    faces.iter().any(|f| {
        let [l, t, r, b] = f.bounds;
        let (w, h) = (r - l, b - t);
        x >= l + w * 0.2 && x <= r - w * 0.2 && y >= t + h * 0.25 && y <= b - h * 0.1
    })
}

fn read(px: &Pixels<'_>, faces: &[PortraitFace]) -> Reading {
    let (w, h) = (px.width, px.height);
    let step = ((w * h) as f32 / 60_000.0).sqrt().ceil().max(1.0) as usize;
    let mut lum = Vec::new();
    let (mut warm, mut warm_n) = (0.0_f32, 0_usize);
    let mut share = [0.0_f32; 8];
    let mut sat = [0.0_f32; 8];
    let mut coloured = 0_usize;
    let (mut sat_sum, mut sat_n) = (0.0_f32, 0_usize);
    let mut skin = Vec::new();
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            let fx = (x as f32 + 0.5) / w as f32;
            let fy = (y as f32 + 0.5) / h as f32;
            let p = px.encoded(x, y);
            let l = p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722;
            if in_face_core(faces, fx, fy) {
                // Skin: warmer than it is green, green above blue, neither dark nor clipped.
                if p[0] > p[1] && p[1] > p[2] && l > 0.12 && p[0] < 0.98 && skin.len() < 4000 {
                    skin.push(p);
                }
                continue;
            }
            if in_face(faces, fx, fy) {
                continue;
            }
            lum.push(l);
            if (0.25..0.8).contains(&l) {
                warm += p[0] - p[2];
                warm_n += 1;
            }
            let max = p[0].max(p[1]).max(p[2]);
            let min = p[0].min(p[1]).min(p[2]);
            if max > 0.05 {
                sat_sum += (max - min) / max;
                sat_n += 1;
            }
            // Dark and near-white pixels carry little reliable hue.
            if max > 0.1 && max < 0.97 && (max - min) / max > 0.12 {
                let (hue, s, _) = to_hsv(display_srgb_to_working(p));
                for (i, wgt) in band_weights(hue).iter().enumerate() {
                    if let (Some(a), Some(b)) = (share.get_mut(i), sat.get_mut(i)) {
                        *a += wgt;
                        *b += wgt * s;
                    }
                }
                coloured += 1;
            }
        }
    }
    let n = lum.len().max(1) as f32;
    let mean = lum.iter().sum::<f32>() / n;
    let spread = (lum.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n).sqrt();
    let bright = lum.iter().filter(|v| **v > 0.95).count() as f32 / n;
    let dark = lum.iter().filter(|v| **v < 0.2).count() as f32 / n;
    lum.sort_by(f32::total_cmp);
    let median = lum.get(lum.len() / 2).copied().unwrap_or(0.5);
    let mut band_sat = [0.0_f32; 8];
    for i in 0..8 {
        if let (Some(out), Some(s), Some(w)) = (band_sat.get_mut(i), sat.get(i), share.get(i)) {
            *out = if *w > 1e-6 { s / w } else { 0.0 };
        }
    }
    let total = coloured.max(1) as f32;
    for v in &mut share {
        *v /= total;
    }
    Reading {
        spread,
        median,
        bright_share: bright,
        dark_share: dark,
        warmth: if warm_n == 0 {
            0.0
        } else {
            warm / warm_n as f32
        },
        band_share: share,
        band_sat,
        coloured,
        saturation: if sat_n == 0 {
            0.0
        } else {
            sat_sum / sat_n as f32
        },
        skin,
    }
}

/// CIE L*a*b* of a working-space colour.
fn lab(working: [f32; 3]) -> [f32; 3] {
    let xyz = aura_raw::colour::working_space::working_to_xyz(working.map(f64::from));
    let white = [0.950_47_f64, 1.0, 1.088_83];
    let f = |t: f64| {
        if t > 0.008_856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let [fx, fy, fz] = [
        f(xyz[0] / white[0]),
        f(xyz[1] / white[1]),
        f(xyz[2] / white[2]),
    ];
    [
        (116.0 * fy - 16.0) as f32,
        (500.0 * (fx - fy)) as f32,
        (200.0 * (fy - fz)) as f32,
    ]
}

/// Mean colour difference and mean hue rotation (degrees) the colour half of a look makes on
/// the given skin, through the renderer's own band and grade functions.
fn skin_shift(
    skin: &[[f32; 3]],
    bands: &BTreeMap<String, HslShift>,
    grade: &ColourGrade,
    vibrance: i16,
) -> (f32, f32) {
    if skin.is_empty() {
        return (0.0, 0.0);
    }
    let plan = GradePlan::new(grade);
    let (mut de, mut dh) = (0.0_f32, 0.0_f32);
    for p in skin {
        let before = display_srgb_to_working(*p);
        let after = plan.apply(vibrance_saturation(
            hsl(before, bands),
            f32::from(vibrance) / 100.0,
            0.0,
        ));
        let (a, b) = (lab(before), lab(after));
        de += ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        let (ha, hb) = (a[2].atan2(a[1]), b[2].atan2(b[1]));
        let mut d = (hb - ha).to_degrees().abs();
        if d > 180.0 {
            d = 360.0 - d;
        }
        dh += d;
    }
    let n = skin.len() as f32;
    (de / n, dh / n)
}

fn scaled(
    bands: &BTreeMap<String, HslShift>,
    grade: &ColourGrade,
    k: f32,
) -> (BTreeMap<String, HslShift>, ColourGrade) {
    let s = |v: i16| (f32::from(v) * k).round() as i16;
    let bands = bands
        .iter()
        .map(|(name, shift)| {
            (
                name.clone(),
                HslShift {
                    h: s(shift.h),
                    s: s(shift.s),
                    l: s(shift.l),
                },
            )
        })
        .filter(|(_, shift)| !shift.is_neutral())
        .collect();
    let wheel = |w: GradeWheel| GradeWheel {
        hue: w.hue,
        saturation: s(w.saturation),
        luminance: s(w.luminance),
    };
    let grade = ColourGrade {
        shadows: wheel(grade.shadows),
        midtones: wheel(grade.midtones),
        highlights: wheel(grade.highlights),
        global: wheel(grade.global),
        ..*grade
    };
    (bands, grade)
}

/// The usual tonal spread (standard deviation of display lightness) of a finished frame of
/// each kind. A frame that falls short is given depth; one far beyond it is opened.
const fn spread_target(kind: SceneKind) -> Option<f32> {
    match kind {
        SceneKind::Portrait | SceneKind::Group => Some(0.23),
        SceneKind::Landscape => Some(0.26),
        SceneKind::General => Some(0.25),
        SceneKind::HighKey => Some(0.17),
        SceneKind::LowLight | SceneKind::Night => None,
    }
}

/// Decide the look for one photograph. `contrast` is what the corrections already set.
#[must_use]
pub fn plan(px: &Pixels<'_>, faces: &[PortraitFace], kind: SceneKind, contrast: i16) -> Look {
    let r = read(px, faces);
    let people = !faces.is_empty();
    let mut look = Look::default();

    // ---- depth -------------------------------------------------------------------------
    let low_key = matches!(kind, SceneKind::LowLight | SceneKind::Night)
        || (r.dark_share > 0.55 && r.median < 0.2);
    if low_key {
        // The shadows slider's band starts above the deepest tones; the parametric curve's
        // shadows region reaches them. Contrast steepens about mid grey and so darkens them.
        let lift = ((0.2 - r.median) * 110.0).clamp(0.0, 20.0)
            * if kind == SceneKind::Night { 0.6 } else { 1.0 };
        if contrast > 0 {
            look.contrast = Some(0);
            look.decisions.push(format!(
                "Contrast {contrast:+} withdrawn: on a frame this dark it would deepen the tones the lift is for."
            ));
        }
        if lift >= 3.0 {
            look.parametric.shadows = lift.round() as i16;
            look.parametric.darks = (lift * 0.4).round() as i16;
            look.decisions.push(format!(
                "Deepest tones opened (curve shadows +{}, darks +{}): {:.0}% of the frame is dark and its median sits at {:.0}%; the light sources keep their glow.",
                look.parametric.shadows,
                look.parametric.darks,
                r.dark_share * 100.0,
                r.median * 100.0
            ));
        }
    } else if let Some(target) = spread_target(kind) {
        let gap = target - r.spread;
        let gentle = if people { 0.7 } else { 1.0 };
        if gap > 0.015 {
            let s = (gap * 320.0 * gentle).clamp(0.0, DEPTH_LIMIT);
            // Little room at the top: do not push what is already near white, or a saturated
            // colour whose brightest channel is already near the top.
            let lights = if r.bright_share > 0.03
                || crate::smart_edit::saturated_top_share(px)
                    > crate::smart_edit::SATURATED_TOP_SHARE
            {
                s.min(3.0)
            } else {
                s
            };
            look.parametric.lights = lights.round() as i16;
            look.parametric.darks = -(s * 0.8).round() as i16;
            look.decisions.push(format!(
                "Depth: S-curve lights {:+}, darks {:+}; the frame's tonal spread is {:.2} where a finished {} is about {target:.2}.",
                look.parametric.lights,
                look.parametric.darks,
                r.spread,
                kind.label()
            ));
        } else if r.spread > target + 0.06 {
            let s = ((r.spread - target - 0.06) * 150.0).clamp(0.0, 10.0);
            look.parametric.darks = s.round() as i16;
            look.parametric.lights = -(s * 0.5).round() as i16;
            look.decisions.push(format!(
                "Hard light opened: darks {:+}, lights {:+}; tonal spread {:.2} against about {target:.2}.",
                look.parametric.darks,
                look.parametric.lights,
                r.spread
            ));
        } else {
            look.decisions.push(format!(
                "Depth kept: tonal spread {:.2} is already right for a {}.",
                r.spread,
                kind.label()
            ));
        }
    }

    // ---- colour ------------------------------------------------------------------------
    // How much of each band the skin holds: a band that is mostly skin is not the dress.
    let mut skin_in = [0.0_f32; 8];
    for p in &r.skin {
        let (hue, _, _) = to_hsv(display_srgb_to_working(*p));
        for (slot, w) in skin_in.iter_mut().zip(band_weights(hue)) {
            *slot += w;
        }
    }
    let skin_samples = r.skin.len().max(1) as f32;
    let names = aura_recipe::HSL_BANDS;
    let mut bands: BTreeMap<String, HslShift> = BTreeMap::new();
    let mut touched = Vec::new();
    if r.coloured >= 200 {
        for (i, name) in names.iter().enumerate() {
            let share = r.band_share.get(i).copied().unwrap_or(0.0);
            let sat = r.band_sat.get(i).copied().unwrap_or(0.0);
            let skin_part = if people {
                skin_in.get(i).copied().unwrap_or(0.0) / skin_samples
            } else {
                0.0
            };
            // Orange is where skin of every tone lives in this space; it is never pushed.
            if *name == "orange" || share < 0.05 {
                continue;
            }
            let room = (1.0 - 2.5 * skin_part).max(0.0);
            let confidence = (share / 0.15).min(1.0);
            let s = if sat < 0.65 {
                ((0.7 - sat) * 50.0 * confidence * room).clamp(0.0, BAND_LIMIT)
            } else if sat > 0.85 {
                -((sat - 0.85) * 80.0 * confidence).clamp(3.0, 12.0)
            } else {
                0.0
            };
            let s = if matches!(kind, SceneKind::Portrait | SceneKind::Group) {
                s * 0.75
            } else {
                s
            };
            let mut shift = HslShift {
                s: s.round() as i16,
                ..HslShift::default()
            };
            // An open blue sky gains depth by darkening a little, the way a polariser does.
            if *name == "blue" && kind == SceneKind::Landscape && share > 0.15 {
                shift.l = -((share * 30.0).clamp(3.0, 10.0)).round() as i16;
            }
            if !shift.is_neutral() {
                touched.push(format!(
                    "{name} {:+}{} ({:.0}% of the colour, saturation {:.0}%)",
                    shift.s,
                    if shift.l == 0 {
                        String::new()
                    } else {
                        format!(", luminance {:+}", shift.l)
                    },
                    share * 100.0,
                    sat * 100.0
                ));
                bands.insert((*name).to_string(), shift);
            }
        }
    }

    // A split tone weighted by the frame's own warmth (red minus blue in its midtones).
    let w = r.warmth;
    let (highlights, shadows): (f32, f32) = if low_key {
        (0.0, 4.0)
    } else if kind == SceneKind::HighKey {
        (2.0, 0.0)
    } else if w > 0.12 {
        // A warm room is warm enough: only separate its shadows.
        (0.0, 3.0)
    } else if w < -0.02 {
        // A cool stage or blue hour: give warmth back to the highlights.
        (((-w) * 60.0).clamp(3.0, 7.0), 3.0)
    } else {
        // Daylight and neutral rooms: the classic warm-light, cool-shadow depth.
        ((0.12 - w) * 35.0 + 2.0, 3.0)
    };
    let grade = ColourGrade {
        highlights: GradeWheel {
            hue: 42,
            saturation: highlights.round().clamp(0.0, 8.0) as i16,
            luminance: 0,
        },
        shadows: GradeWheel {
            hue: 212,
            saturation: shadows.round().clamp(0.0, 6.0) as i16,
            luminance: 0,
        },
        ..ColourGrade::default()
    };

    // Vibrance on top of the correction's, by how much colour the frame has left to give.
    let vibrance = if low_key {
        (0.35 - r.saturation) * 20.0
    } else {
        (0.45 - r.saturation) * 35.0
    }
    .clamp(0.0, VIBRANCE_LIMIT)
        * if people { 0.6 } else { 1.0 };

    // ---- skin, measured ----------------------------------------------------------------
    let mut k = 1.0_f32;
    let vibrance_at = |k: f32| (vibrance * k).round() as i16;
    let (mut de, mut dh) = skin_shift(&r.skin, &bands, &grade, vibrance_at(1.0));
    while people && !r.skin.is_empty() && (de > SKIN_DE_LIMIT || dh > SKIN_HUE_LIMIT) && k > 0.0 {
        k = (k - 0.25).max(0.0);
        let (b, g) = scaled(&bands, &grade, k);
        (de, dh) = skin_shift(&r.skin, &b, &g, vibrance_at(k));
    }
    let (bands, grade) = scaled(&bands, &grade, k);
    look.vibrance = vibrance_at(k);
    if look.vibrance > 0 {
        look.decisions.push(format!(
            "Vibrance {:+} more: colour outside the faces averages {:.0}% saturation.",
            look.vibrance,
            r.saturation * 100.0
        ));
    }
    if !touched.is_empty() && !bands.is_empty() {
        look.decisions.push(format!(
            "Colour per band: {}{}.",
            touched.join("; "),
            if k < 1.0 {
                format!(", at {:.0}% so skin stays as it is", k * 100.0)
            } else {
                String::new()
            }
        ));
    }
    if !grade.is_neutral() {
        look.decisions.push(format!(
            "Split tone: warm highlights {}, cool shadows {} (frame warmth {w:+.2}){}.",
            grade.highlights.saturation,
            grade.shadows.saturation,
            if k < 1.0 {
                format!(", held to {:.0}% by skin", k * 100.0)
            } else {
                String::new()
            }
        ));
    }
    if people && !r.skin.is_empty() {
        look.skin_shift = Some(de);
        look.decisions.push(format!(
            "Skin measured through the look on {} samples: colour difference {de:.2}, hue {dh:.1}° (limits {SKIN_DE_LIMIT} and {SKIN_HUE_LIMIT}°).",
            r.skin.len()
        ));
    }
    look.hsl = bands;
    look.grade = grade;
    look
}

/// Write the look into a recipe's global block.
///
/// The look owns these fields in an automatic edit, so it sets them rather than adding to
/// them: pressing Auto again gives the same recipe, not twice the look. A value a person set
/// is still theirs, because [`aura_recipe::schema::merge`] refuses to overwrite it.
pub fn apply(look: &Look, g: &mut aura_recipe::Global) {
    let p = &mut g.parametric;
    p.shadows = look.parametric.shadows;
    p.darks = look.parametric.darks;
    p.lights = look.parametric.lights;
    p.highlights = look.parametric.highlights;
    g.hsl.clone_from(&look.hsl);
    g.colour_grade = look.grade;
    if let Some(contrast) = look.contrast {
        g.contrast = contrast;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 3]) -> Vec<u8> {
        let mut v = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&f(x, y));
            }
        }
        v
    }

    fn face() -> PortraitFace {
        PortraitFace {
            bounds: [0.35, 0.2, 0.65, 0.6],
            landmarks: [
                [0.45, 0.35],
                [0.55, 0.35],
                [0.5, 0.45],
                [0.45, 0.52],
                [0.55, 0.52],
            ],
            confidence: 0.9,
        }
    }

    #[test]
    fn a_flat_frame_gets_depth_and_a_contrasty_one_does_not() {
        let flat = frame(120, 80, |x, _| {
            let v = 110 + (x % 30) as u8;
            [v, v, v - 5]
        });
        let look = plan(
            &Pixels::new(&flat, 120, 80).unwrap(),
            &[],
            SceneKind::General,
            0,
        );
        assert!(
            look.parametric.lights > 0 && look.parametric.darks < 0,
            "{look:?}"
        );
        let hard = frame(120, 80, |x, _| {
            if x % 2 == 0 {
                [20, 20, 20]
            } else {
                [230, 230, 225]
            }
        });
        let look = plan(
            &Pixels::new(&hard, 120, 80).unwrap(),
            &[],
            SceneKind::General,
            0,
        );
        assert!(look.parametric.lights <= 0, "{look:?}");
    }

    #[test]
    fn a_dark_frame_is_opened_in_its_deepest_tones_and_loses_darkening_contrast() {
        let dark = frame(120, 80, |x, y| {
            if (x + y) % 37 == 0 {
                [250, 220, 160]
            } else {
                [6, 5, 4]
            }
        });
        let look = plan(
            &Pixels::new(&dark, 120, 80).unwrap(),
            &[],
            SceneKind::LowLight,
            7,
        );
        assert_eq!(look.contrast, Some(0));
        assert!(look.parametric.shadows > 0, "{look:?}");
    }

    #[test]
    fn two_different_frames_get_two_different_looks() {
        // Muted colours, so each frame has a dull band of its own to enrich.
        let warm = frame(120, 80, |x, _| [165, 105 + (x % 20) as u8, 112]);
        let cool = frame(120, 80, |x, _| [104, 118 + (x % 20) as u8, 160]);
        let a = plan(
            &Pixels::new(&warm, 120, 80).unwrap(),
            &[],
            SceneKind::General,
            0,
        );
        let b = plan(
            &Pixels::new(&cool, 120, 80).unwrap(),
            &[],
            SceneKind::General,
            0,
        );
        assert_ne!(a.grade, b.grade);
        assert_ne!(a.hsl, b.hsl);
    }

    #[test]
    fn skin_stays_inside_its_limits() {
        // A dull red dress filling the frame around a face: the red band wants a boost and
        // the skin sits partly in red. Whatever is planned, skin moves less than the limits.
        let f = face();
        let px = frame(160, 120, |x, y| {
            let (fx, fy) = ((x as f32 + 0.5) / 160.0, (y as f32 + 0.5) / 120.0);
            if in_face_core(std::slice::from_ref(&f), fx, fy) {
                [190, 130, 105]
            } else {
                [150 + (x % 20) as u8, 70, 70]
            }
        });
        let look = plan(
            &Pixels::new(&px, 160, 120).unwrap(),
            &[f],
            SceneKind::Portrait,
            0,
        );
        let shift = look.skin_shift.expect("measured");
        assert!(shift <= SKIN_DE_LIMIT + 1e-3, "{look:?}");
    }

    #[test]
    fn applying_twice_is_the_same_as_once() {
        let mut g = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "t").global;
        g.hsl.insert("green".into(), HslShift { h: 0, s: 30, l: 0 });
        let mut look = Look::default();
        look.hsl.insert("red".into(), HslShift { h: 0, s: 4, l: 0 });
        look.grade.highlights.saturation = 3;
        look.parametric.lights = 6;
        apply(&look, &mut g);
        let once = g.clone();
        apply(&look, &mut g);
        assert_eq!(g, once);
        assert_eq!(g.hsl.get("red").map(|s| s.s), Some(4));
        assert!(
            !g.hsl.contains_key("green"),
            "a stale automatic band is not left behind"
        );
    }
}
