//! "My style": a personal edit profile learned from a photographer's own Lightroom work.
//! ADR-0104.
//!
//! Imagen's personal profile learns how somebody edits from their Lightroom catalogue. This does
//! the same, in two halves that are kept apart on purpose:
//!
//! * **What depends on the photograph** - how much exposure it needs, its white balance, how
//!   much to recover a sky - is measured on every photograph by AURA's automatic correction, as
//!   it already is. A median of somebody's exposure slider over five hundred photographs says
//!   how dark their camera was that day, not what they like.
//! * **What is the photographer's taste** - their contrast, highlight and shadow handling, black
//!   point, clarity and texture, colour, HSL, tone curve, colour grading, calibration, grain,
//!   vignette and sharpening - is read off every edited photograph in the catalogue and reduced
//!   to its median. A preset synchronised across a shoot and a hundred hand-tuned edits both
//!   come out as what the photographer actually does.
//!
//! The result is an ordinary edit profile (`origin: "personal"`), so it appears in the Studio's
//! presets and in a whole-folder automatic edit, applied on top of each photograph's measured
//! correction and protected by the same scene guards and manual-edit protection as every other
//! profile. It is stored in the catalogue's `setting` table.
//!
//! When the original photographs are still where the catalogue says, up to four hundred of them
//! are opened read-only and measured exactly as a whole-folder edit would measure them. That
//! splits each tonal slider precisely into what AURA's own correction already does and what is
//! the photographer's, and fits how the photographer's part follows the photograph
//! ([`AdaptiveModel`]), kept only for sliders where it beats one fixed value on photographs the
//! fit never saw. Nothing is copied and nothing is written next to the photographs.
// Small numeric code over bounded arrays: indices are in range by construction and the casts
// are between slider units and floats whose range is clamped first.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::indexing_slicing,
    clippy::needless_range_loop,
    clippy::too_many_lines
)]
use std::collections::BTreeMap;

use aura_core::{AuraError, AuraResult};
use aura_recipe::{
    Calibration, ChannelCurves, ColourGrade, Curve, GradeWheel, Grain, HslShift, ParametricCurve,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::IpcResult;
use crate::edit_profiles::{
    AdaptiveModel, AutoCorrection, EditProfile, ProfileAdjust, ProfileSource, SceneStats,
    ADAPTIVE_AXES, ADAPTIVE_FEATURES,
};
use crate::lightroom::{self, CatalogPhoto};
use crate::AppState;

/// The key every personal profile is filed under in the `setting` table.
const SETTING_KEY: &str = "personal_profiles_v1";
/// At most this many personal profiles.
const MAX_PROFILES: usize = 24;
/// Fewer edited photographs than this teach nothing worth calling a style.
pub const MIN_PHOTOS: usize = 10;

fn invalid(message: impl Into<String>) -> AuraError {
    let message = message.into();
    let mut error = aura_core::errors::render::recipe_invalid("personal style", &message);
    error.user_message = message;
    error
}

/// What learning produced.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnedStyle {
    pub profile: EditProfile,
    /// Photographs learned from.
    pub photos: usize,
    /// Edited photographs in the catalogue, before any were set aside.
    pub edited: usize,
    /// What the photographer consistently does, a sentence each.
    pub findings: Vec<String>,
}

/// Input to [`learn_from_lightroom`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearnInput {
    /// The `.lrcat` file.
    pub catalog: String,
    /// What to call the look.
    pub name: String,
}

/// One numeric develop setting across the photographs, with the value Lightroom uses when the
/// key is absent.
fn values(photos: &[&CatalogPhoto], key: &str, default: f64) -> Vec<f64> {
    photos
        .iter()
        .map(|p| {
            p.settings
                .get(key)
                .and_then(Value::as_f64)
                .unwrap_or(default)
        })
        .collect()
}

fn median(mut v: Vec<f64>) -> f64 {
    v.retain(|x| x.is_finite());
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        f64::midpoint(v[n / 2 - 1], v[n / 2])
    }
}

/// Share of photographs within `tolerance` of `centre`.
fn agreement(v: &[f64], centre: f64, tolerance: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter()
        .filter(|x| (*x - centre).abs() <= tolerance)
        .count() as f64
        / v.len() as f64
}

fn slider(v: f64, low: f64, high: f64) -> i16 {
    v.round().clamp(low, high) as i16
}

/// The median of a setting, and how consistently it is used.
struct Learned {
    value: f64,
    agreement: f64,
}

fn learn(photos: &[&CatalogPhoto], key: &str, default: f64, tolerance: f64) -> Learned {
    let v = values(photos, key, default);
    let value = median(v.clone());
    Learned {
        value,
        agreement: agreement(&v, value, tolerance),
    }
}

/// A point curve stored as `x0, y0, x1, y1, ...`, sampled at `xs`.
fn sample_curve(points: &[f64], xs: &[f64]) -> Option<Vec<f64>> {
    let pairs: Vec<(f64, f64)> = points.chunks_exact(2).map(|p| (p[0], p[1])).collect();
    if pairs.len() < 2 {
        return None;
    }
    Some(
        xs.iter()
            .map(|x| {
                let i = pairs
                    .iter()
                    .position(|p| p.0 >= *x)
                    .unwrap_or(pairs.len() - 1);
                if i == 0 {
                    return pairs[0].1;
                }
                let (a, b) = (pairs[i - 1], pairs[i]);
                if (b.0 - a.0).abs() < 1e-9 {
                    b.1
                } else {
                    a.1 + (b.1 - a.1) * (x - a.0) / (b.0 - a.0)
                }
            })
            .collect(),
    )
}

/// The median curve over the photographs, or `None` when it is the identity.
fn median_curve(photos: &[&CatalogPhoto], key: &str) -> Option<Vec<[u16; 2]>> {
    let xs: Vec<f64> = (0..=8).map(|i| f64::from(i) * 255.0 / 8.0).collect();
    let samples: Vec<Vec<f64>> = photos
        .iter()
        .filter_map(|p| {
            let list: Vec<f64> = p
                .settings
                .get(key)?
                .as_array()?
                .iter()
                .filter_map(Value::as_f64)
                .collect();
            sample_curve(&list, &xs)
        })
        .collect();
    if samples.len() * 2 < photos.len() {
        return None;
    }
    let mut points: Vec<[u16; 2]> = xs
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let y = median(samples.iter().map(|s| s[i]).collect());
            [x.round() as u16, y.round().clamp(0.0, 255.0) as u16]
        })
        .collect();
    // Never invert tones.
    for i in 1..points.len() {
        if points[i][1] < points[i - 1][1] {
            points[i][1] = points[i - 1][1];
        }
    }
    let identity = points
        .iter()
        .all(|p| (i32::from(p[0]) - i32::from(p[1])).abs() <= 2);
    (!identity).then_some(points)
}

const BANDS: [(&str, &str); 8] = [
    ("red", "Red"),
    ("orange", "Orange"),
    ("yellow", "Yellow"),
    ("green", "Green"),
    ("aqua", "Aqua"),
    ("blue", "Blue"),
    ("purple", "Purple"),
    ("magenta", "Magenta"),
];

fn wheel(photos: &[&CatalogPhoto], region: &str) -> GradeWheel {
    let saturation = learn(photos, &format!("ColorGrade{region}Sat"), 0.0, 3.0).value;
    // A hue means something only where the wheel was used.
    let used: Vec<&CatalogPhoto> = photos
        .iter()
        .copied()
        .filter(|p| {
            p.settings
                .get(&format!("ColorGrade{region}Sat"))
                .and_then(Value::as_f64)
                .is_some_and(|s| s > 0.0)
        })
        .collect();
    let hue = if used.is_empty() {
        0.0
    } else {
        median(values(&used, &format!("ColorGrade{region}Hue"), 0.0))
    };
    GradeWheel {
        hue: (hue.round() as i16).rem_euclid(360),
        saturation: slider(saturation, 0.0, 100.0),
        luminance: slider(
            learn(photos, &format!("ColorGrade{region}Lum"), 0.0, 3.0).value,
            -100.0,
            100.0,
        ),
    }
}

/// Learn the photographer's taste from the photographs' settings.
fn adjust_from(photos: &[&CatalogPhoto], findings: &mut Vec<String>) -> ProfileAdjust {
    let mut say = |what: &str, l: &Learned, unit: &str| {
        if l.value.abs() >= 1.0 {
            findings.push(format!(
                "{what} {:+.0}{unit} ({:.0} % of photographs within a few points of it).",
                l.value,
                l.agreement * 100.0
            ));
        }
    };
    let contrast = learn(photos, "Contrast2012", 0.0, 5.0);
    let highlights = learn(photos, "Highlights2012", 0.0, 8.0);
    let shadows = learn(photos, "Shadows2012", 0.0, 8.0);
    let whites = learn(photos, "Whites2012", 0.0, 8.0);
    let blacks = learn(photos, "Blacks2012", 0.0, 8.0);
    let clarity = learn(photos, "Clarity2012", 0.0, 5.0);
    let texture = learn(photos, "Texture", 0.0, 5.0);
    let dehaze = learn(photos, "Dehaze", 0.0, 5.0);
    let vibrance = learn(photos, "Vibrance", 0.0, 5.0);
    let saturation = learn(photos, "Saturation", 0.0, 5.0);
    for (what, l) in [
        ("Contrast", &contrast),
        ("Highlights", &highlights),
        ("Shadows", &shadows),
        ("Whites", &whites),
        ("Blacks", &blacks),
        ("Clarity", &clarity),
        ("Texture", &texture),
        ("Dehaze", &dehaze),
        ("Vibrance", &vibrance),
        ("Saturation", &saturation),
    ] {
        say(what, l, "");
    }
    // Exposure is half taste and half the day's light. A photographer who brightens nearly
    // every photograph by the same amount likes them brighter; half of that is kept, on top of
    // the measured correction, and the rest is left to the measurement.
    let exposure = learn(photos, "Exposure2012", 0.0, 0.15);
    let exposure_taste = if exposure.agreement >= 0.6 {
        findings.push(format!(
            "You brighten by {:+.2} stops in {:.0} % of photographs; half of it is kept as taste, the rest is measured per photograph.",
            exposure.value,
            exposure.agreement * 100.0
        ));
        (exposure.value * 0.5).clamp(-1.0, 1.0)
    } else {
        0.0
    };

    let mut hsl = BTreeMap::new();
    for (band, name) in BANDS {
        let shift = HslShift {
            h: slider(
                learn(photos, &format!("HueAdjustment{name}"), 0.0, 3.0).value,
                -100.0,
                100.0,
            ),
            s: slider(
                learn(photos, &format!("SaturationAdjustment{name}"), 0.0, 3.0).value,
                -100.0,
                100.0,
            ),
            l: slider(
                learn(photos, &format!("LuminanceAdjustment{name}"), 0.0, 3.0).value,
                -100.0,
                100.0,
            ),
        };
        if !shift.is_neutral() {
            findings.push(format!(
                "{name}: hue {:+}, saturation {:+}, luminance {:+}.",
                shift.h, shift.s, shift.l
            ));
            hsl.insert(band.to_string(), shift);
        }
    }

    let curve = median_curve(photos, "ToneCurvePV2012").unwrap_or_default();
    if !curve.is_empty() {
        findings.push("A tone curve of your own, reproduced from its median shape.".into());
    }
    let channel = |key: &str| median_curve(photos, key).map(|points| Curve { points });
    let channel_curves = match (
        channel("ToneCurvePV2012Red"),
        channel("ToneCurvePV2012Green"),
        channel("ToneCurvePV2012Blue"),
    ) {
        (None, None, None) => None,
        (r, g, b) => {
            findings.push("Colour curves on the red, green or blue channel.".into());
            Some(ChannelCurves {
                red: r.unwrap_or_else(Curve::identity),
                green: g.unwrap_or_else(Curve::identity),
                blue: b.unwrap_or_else(Curve::identity),
            })
        }
    };
    let parametric = {
        let p = ParametricCurve {
            highlights: slider(
                learn(photos, "ParametricHighlights", 0.0, 3.0).value,
                -100.0,
                100.0,
            ),
            lights: slider(
                learn(photos, "ParametricLights", 0.0, 3.0).value,
                -100.0,
                100.0,
            ),
            darks: slider(
                learn(photos, "ParametricDarks", 0.0, 3.0).value,
                -100.0,
                100.0,
            ),
            shadows: slider(
                learn(photos, "ParametricShadows", 0.0, 3.0).value,
                -100.0,
                100.0,
            ),
            shadow_split: slider(
                learn(photos, "ParametricShadowSplit", 25.0, 3.0).value,
                10.0,
                30.0,
            ),
            midtone_split: slider(
                learn(photos, "ParametricMidtoneSplit", 50.0, 3.0).value,
                35.0,
                65.0,
            ),
            highlight_split: slider(
                learn(photos, "ParametricHighlightSplit", 75.0, 3.0).value,
                70.0,
                90.0,
            ),
        };
        (p.highlights != 0 || p.lights != 0 || p.darks != 0 || p.shadows != 0).then_some(p)
    };
    // Colour grading: capped on the midtones and highlights, which colour every face.
    let mut grade = ColourGrade {
        shadows: wheel(photos, "Shadow"),
        midtones: wheel(photos, "Midtone"),
        highlights: wheel(photos, "Highlight"),
        global: wheel(photos, "Global"),
        blending: slider(
            learn(photos, "ColorGradeBlending", 50.0, 5.0).value,
            0.0,
            100.0,
        ),
        balance: slider(
            learn(photos, "SplitToningBalance", 0.0, 5.0).value,
            -100.0,
            100.0,
        ),
    };
    grade.midtones.saturation = grade.midtones.saturation.min(20);
    grade.highlights.saturation = grade.highlights.saturation.min(30);
    let colour_grade = [
        grade.shadows,
        grade.midtones,
        grade.highlights,
        grade.global,
    ]
    .iter()
    .any(|w| !w.is_neutral())
    .then(|| {
        findings.push("Colour grading on the shadows, midtones or highlights.".into());
        grade
    });
    let calibration = {
        let c = Calibration {
            shadows_tint: slider(learn(photos, "ShadowTint", 0.0, 3.0).value, -100.0, 100.0),
            red_hue: slider(learn(photos, "RedHue", 0.0, 3.0).value, -100.0, 100.0),
            red_saturation: slider(
                learn(photos, "RedSaturation", 0.0, 3.0).value,
                -100.0,
                100.0,
            ),
            green_hue: slider(learn(photos, "GreenHue", 0.0, 3.0).value, -100.0, 100.0),
            green_saturation: slider(
                learn(photos, "GreenSaturation", 0.0, 3.0).value,
                -100.0,
                100.0,
            ),
            blue_hue: slider(learn(photos, "BlueHue", 0.0, 3.0).value, -100.0, 100.0),
            blue_saturation: slider(
                learn(photos, "BlueSaturation", 0.0, 3.0).value,
                -100.0,
                100.0,
            ),
        };
        (c != Calibration::default()).then(|| {
            findings.push("Camera calibration adjustments.".into());
            c
        })
    };
    let grain = {
        let amount = learn(photos, "GrainAmount", 0.0, 3.0).value;
        (amount >= 1.0).then(|| {
            findings.push(format!("Film grain {amount:.0}."));
            Grain {
                amount: slider(amount, 0.0, 100.0),
                size: slider(learn(photos, "GrainSize", 25.0, 5.0).value, 0.0, 100.0),
                roughness: slider(learn(photos, "GrainFrequency", 50.0, 5.0).value, 0.0, 100.0),
            }
        })
    };
    let vignette = {
        let amount = learn(photos, "PostCropVignetteAmount", 0.0, 5.0).value;
        if amount <= -1.0 {
            findings.push(format!("A post-crop vignette of {amount:.0}."));
        }
        if amount < 0.0 {
            (-amount / 100.0).min(1.0) as f32
        } else {
            0.0
        }
    };
    let grey = photos
        .iter()
        .filter(|p| p.settings.get("ConvertToGrayscale") == Some(&Value::Bool(true)))
        .count();
    let bw = (grey * 2 > photos.len()).then(|| {
        findings.push("Black and white, with your own channel mix.".into());
        BANDS
            .iter()
            .map(|(band, name)| {
                (
                    (*band).to_string(),
                    slider(
                        learn(photos, &format!("GrayMixer{name}"), 0.0, 3.0).value,
                        -100.0,
                        100.0,
                    ),
                )
            })
            .collect()
    });

    ProfileAdjust {
        exposure: exposure_taste as f32,
        // Contrast, highlights and shadows are partly what this photograph needed, which AURA
        // measures itself and adds underneath. Without the originals to tell the two apart,
        // half is kept as taste; with them, [`fit_tonal`] replaces these with the exact split.
        contrast: slider(contrast.value * 0.5, -100.0, 100.0),
        highlights: slider(highlights.value * 0.5, -100.0, 100.0),
        shadows: slider(shadows.value * 0.5, -100.0, 100.0),
        whites: slider(whites.value, -100.0, 100.0),
        blacks: slider(blacks.value, -100.0, 100.0),
        temperature: 0,
        tint: 0,
        clarity: slider(clarity.value, -100.0, 100.0),
        texture: slider(texture.value, -100.0, 100.0),
        dehaze: slider(dehaze.value, -100.0, 100.0),
        vibrance: slider(vibrance.value, -100.0, 100.0),
        saturation: slider(saturation.value, -100.0, 100.0),
        curve,
        hsl,
        sharpen: slider(learn(photos, "Sharpness", 40.0, 5.0).value, 0.0, 150.0),
        noise: slider(
            learn(photos, "LuminanceSmoothing", 0.0, 5.0).value,
            0.0,
            100.0,
        ),
        vignette,
        parametric,
        channel_curves,
        colour_grade,
        calibration,
        grain,
        bw,
        // A JPEG was already developed by the camera: the taste applies at half strength.
        developed_strength: Some(0.5),
    }
}

/// A short, stable id from a name.
fn slug(name: &str) -> String {
    let mut out: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let out = out.trim_matches('-').chars().take(40).collect::<String>();
    format!(
        "personal-{}",
        if out.is_empty() { "style".into() } else { out }
    )
}

/// What AURA measured on one original photograph: the automatic correction it would make and
/// what the scene guards read, exactly as a whole-folder edit measures it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
    pub auto: AutoCorrection,
    pub stats: SceneStats,
}

/// Fewer measured originals than this, and a per-photograph model is a guess.
pub const MIN_ADAPTIVE: usize = 30;
/// At most this many originals are opened, spread evenly through the catalogue.
const MAX_MEASURED: usize = 400;
/// A slider adapts per photograph only when, on photographs held out from the fit, the model
/// comes this much closer to the photographer's own value than one fixed value does.
const MIN_GAIN: f64 = 0.05;

/// Ridge regression on standardised features. Returns the slopes; the intercept is the mean.
fn ridge(
    x: &[[f32; ADAPTIVE_FEATURES]],
    y: &[f64],
    mean: &[f64],
    spread: &[f64],
    lambda: f64,
) -> Vec<f64> {
    const D: usize = ADAPTIVE_FEATURES;
    let ym = y.iter().sum::<f64>() / y.len().max(1) as f64;
    let z: Vec<[f64; D]> = x
        .iter()
        .map(|r| {
            std::array::from_fn(|j| ((f64::from(r[j]) - mean[j]) / spread[j]).clamp(-3.0, 3.0))
        })
        .collect();
    // (Z'Z + lambda I) beta = Z'(y - ym), solved by Gauss-Jordan with partial pivoting.
    let mut a = [[0.0_f64; D + 1]; D];
    for i in 0..D {
        for j in 0..D {
            a[i][j] =
                z.iter().map(|r| r[i] * r[j]).sum::<f64>() + if i == j { lambda } else { 0.0 };
        }
        a[i][D] = z.iter().zip(y).map(|(r, v)| r[i] * (v - ym)).sum::<f64>();
    }
    for c in 0..D {
        let p = (c..D)
            .max_by(|i, j| a[*i][c].abs().total_cmp(&a[*j][c].abs()))
            .unwrap_or(c);
        a.swap(c, p);
        let pivot = a[c][c];
        if pivot.abs() < 1e-12 {
            return vec![0.0; D];
        }
        for k in c..=D {
            a[c][k] /= pivot;
        }
        for r in 0..D {
            if r != c {
                let f = a[r][c];
                for k in c..=D {
                    a[r][k] -= f * a[c][k];
                }
            }
        }
    }
    (0..D).map(|i| a[i][D]).collect()
}

fn standardise(x: &[[f32; ADAPTIVE_FEATURES]]) -> (Vec<f64>, Vec<f64>) {
    let n = x.len().max(1) as f64;
    let mean: Vec<f64> = (0..ADAPTIVE_FEATURES)
        .map(|j| x.iter().map(|r| f64::from(r[j])).sum::<f64>() / n)
        .collect();
    let spread = (0..ADAPTIVE_FEATURES)
        .map(|j| {
            (x.iter()
                .map(|r| (f64::from(r[j]) - mean[j]).powi(2))
                .sum::<f64>()
                / n)
                .sqrt()
                .max(1e-4)
        })
        .collect();
    (mean, spread)
}

fn predict(slopes: &[f64], mean: &[f64], spread: &[f64], f: &[f32; ADAPTIVE_FEATURES]) -> f64 {
    slopes
        .iter()
        .enumerate()
        .map(|(j, b)| b * ((f64::from(f[j]) - mean[j]) / spread[j]).clamp(-3.0, 3.0))
        .sum()
}

/// With the originals measured, split every tonal slider exactly into what AURA's own
/// correction already does and what is the photographer's, and fit how the photographer's part
/// follows the photograph. Five-fold held-out: a slider adapts only where that beats one value.
#[allow(clippy::cast_possible_truncation)]
fn fit_tonal(
    pairs: &[(&CatalogPhoto, Measured)],
    adjust: &mut ProfileAdjust,
    findings: &mut Vec<String>,
) -> Option<AdaptiveModel> {
    if pairs.len() < MIN_ADAPTIVE {
        return None;
    }
    let x: Vec<[f32; ADAPTIVE_FEATURES]> = pairs
        .iter()
        .map(|(_, m)| crate::edit_profiles::adaptive_features(m.auto, m.stats))
        .collect();
    let (mean, spread) = standardise(&x);
    let lambda = 0.4 * pairs.len() as f64;
    let mut weights = BTreeMap::new();
    let mut gains = BTreeMap::new();
    for axis in ADAPTIVE_AXES {
        let (key, default) = match axis {
            "exposure" => ("Exposure2012", 0.0),
            "contrast" => ("Contrast2012", 0.0),
            "highlights" => ("Highlights2012", 0.0),
            "shadows" => ("Shadows2012", 0.0),
            "whites" => ("Whites2012", 0.0),
            _ => ("Blacks2012", 0.0),
        };
        // The photographer's own part: their slider less what AURA's correction already does.
        let y: Vec<f64> = pairs
            .iter()
            .map(|(p, m)| {
                let set = p
                    .settings
                    .get(key)
                    .and_then(Value::as_f64)
                    .unwrap_or(default);
                set - match axis {
                    "exposure" => f64::from(m.auto.exposure),
                    "contrast" => f64::from(m.auto.contrast),
                    "highlights" => f64::from(m.auto.highlights),
                    "shadows" => f64::from(m.auto.shadows),
                    _ => 0.0,
                }
            })
            .collect();
        let usual = median(y.clone());
        // The renderers differ in their starting brightness, so half of a constant exposure
        // offset is kept - the same rule as without the originals.
        if axis == "exposure" {
            adjust.exposure = ((usual * 0.5).clamp(-1.0, 1.0)) as f32;
        } else {
            let v = slider(usual, -100.0, 100.0);
            match axis {
                "contrast" => adjust.contrast = v,
                "highlights" => adjust.highlights = v,
                "shadows" => adjust.shadows = v,
                "whites" => adjust.whites = v,
                _ => adjust.blacks = v,
            }
        }
        // Five folds, assigned by position: deterministic for one catalogue.
        let (mut fixed_err, mut model_err) = (0.0_f64, 0.0_f64);
        for fold in 0..5 {
            let train: Vec<usize> = (0..pairs.len()).filter(|i| i % 5 != fold).collect();
            let test: Vec<usize> = (0..pairs.len()).filter(|i| i % 5 == fold).collect();
            let tx: Vec<[f32; ADAPTIVE_FEATURES]> = train.iter().map(|i| x[*i]).collect();
            let ty: Vec<f64> = train.iter().map(|i| y[*i]).collect();
            let (m, s) = standardise(&tx);
            let slopes = ridge(&tx, &ty, &m, &s, 0.4 * train.len() as f64);
            let centre = median(ty.clone());
            for i in test {
                fixed_err += (y[i] - centre).abs();
                model_err += (y[i] - centre - predict(&slopes, &m, &s, &x[i])).abs();
            }
        }
        let gain = if fixed_err > 1e-9 {
            1.0 - model_err / fixed_err
        } else {
            0.0
        };
        if gain >= MIN_GAIN {
            let slopes = ridge(&x, &y, &mean, &spread, lambda);
            weights.insert(
                axis.to_string(),
                slopes.iter().map(|b| *b as f32).collect::<Vec<f32>>(),
            );
            gains.insert(axis.to_string(), gain as f32);
            findings.push(format!(
                "{} adapts to each photograph as you did: {:.0} % closer to your own setting than one fixed value, on photographs the fit never saw.",
                axis[..1].to_uppercase() + &axis[1..],
                gain * 100.0
            ));
        }
    }
    findings.push(format!(
        "Measured {} of your original photographs, so your tonal sliders are split exactly from what AURA's own correction does.",
        pairs.len()
    ));
    (!weights.is_empty()).then(|| AdaptiveModel {
        mean: mean.iter().map(|v| *v as f32).collect(),
        spread: spread.iter().map(|v| *v as f32).collect(),
        weights,
        photos: u32::try_from(pairs.len()).unwrap_or(u32::MAX),
        held_out_gain: gains,
    })
}

/// Learn a profile from a catalogue's photographs. `measured` is aligned with `photos`; an
/// empty slice means no original could be opened.
///
/// # Errors
/// Too few edited photographs to call it a style.
pub fn learn_from(
    photos: &[CatalogPhoto],
    measured: &[Option<Measured>],
    name: &str,
    source: &str,
) -> AuraResult<LearnedStyle> {
    // Only photographs edited in the 2012 process or later carry these sliders.
    let edited: Vec<usize> = (0..photos.len())
        .filter(|i| {
            let s = &photos[*i].settings;
            s.contains_key("Exposure2012") || s.contains_key("Contrast2012")
        })
        .collect();
    // RAW edits are the photographer's taste over a flat sensor render; a camera JPEG has
    // already had contrast and colour added. Learn from RAW when there is enough of it.
    let raw: Vec<usize> = edited
        .iter()
        .copied()
        .filter(|i| matches!(photos[*i].format.as_str(), "RAW" | "DNG"))
        .collect();
    let chosen_ix = if raw.len() >= MIN_PHOTOS {
        raw
    } else {
        edited.clone()
    };
    if chosen_ix.len() < MIN_PHOTOS {
        return Err(invalid(format!(
            "Only {} edited photographs in that catalogue; AURA needs at least {MIN_PHOTOS} to learn a style.",
            chosen_ix.len()
        )));
    }
    let chosen: Vec<&CatalogPhoto> = chosen_ix.iter().map(|i| &photos[*i]).collect();
    let mut findings = Vec::new();
    let mut adjust = adjust_from(&chosen, &mut findings);
    let pairs: Vec<(&CatalogPhoto, Measured)> = chosen_ix
        .iter()
        .filter_map(|i| {
            measured
                .get(*i)
                .copied()
                .flatten()
                .map(|m| (&photos[*i], m))
        })
        .collect();
    let adaptive = if pairs.len() >= MIN_ADAPTIVE {
        fit_tonal(&pairs, &mut adjust, &mut findings)
    } else {
        findings.push(
            "The original photographs were not found, so half of your highlight, shadow and contrast moves is kept as taste and AURA measures the rest on each photograph. Learn again with the originals connected for a style that adapts per photograph."
                .into(),
        );
        None
    };
    let name = if name.trim().is_empty() {
        "My style"
    } else {
        name.trim()
    };
    let profile = EditProfile {
        id: slug(name),
        name: name.to_string(),
        category: "Personal".into(),
        tagline: format!("Learned from {} of your edited photographs", chosen.len()),
        description: "Your own look, learned from your Lightroom edits: AURA measures each photograph's exposure and white balance itself, then adds the contrast, tone, colour and finishing you consistently use.".into(),
        best_for: vec!["Your own photographs".into()],
        technique: findings.clone(),
        origin: "personal".into(),
        sources: vec![ProfileSource {
            title: format!("{} photographs from {source}", chosen.len()),
            url: String::new(),
        }],
        evidence: None,
        swatch: vec!["#7a6a8c".into(), "#d8c4ee".into()],
        adjust,
        adaptive,
    };
    crate::edit_profiles::check(&profile).map_err(invalid)?;
    Ok(LearnedStyle {
        profile,
        photos: chosen.len(),
        edited: edited.len(),
        findings,
    })
}

/// Every stored personal profile.
///
/// # Errors
/// `AURA-DB-3006` when the catalogue cannot be read.
pub fn stored(conn: &Connection) -> AuraResult<Vec<EditProfile>> {
    let text: Option<String> = conn
        .query_row(
            "SELECT value_json FROM setting WHERE key = ?1",
            [SETTING_KEY],
            |row| row.get(0),
        )
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
        .map_err(|e| {
            aura_core::errors::db::statement_failed("could not read personal profiles", &e)
        })?;
    Ok(text
        .as_deref()
        .and_then(|t| serde_json::from_str::<Vec<EditProfile>>(t).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|p| crate::edit_profiles::check(p).is_ok())
        .collect())
}

fn store(conn: &Connection, profiles: &[EditProfile], now: &str) -> AuraResult<()> {
    let encoded = serde_json::to_string(profiles).map_err(|e| invalid(e.to_string()))?;
    conn.execute(
        "INSERT INTO setting (key, value_json, updated_at) VALUES (?1, ?2, ?3)
           ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                                          updated_at = excluded.updated_at",
        rusqlite::params![SETTING_KEY, encoded, now],
    )
    .map_err(|e| {
        aura_core::errors::db::statement_failed("could not store personal profiles", &e)
    })?;
    Ok(())
}

/// The personal profiles this catalogue holds.
///
/// # Errors
/// `AURA-DB-3006` when the catalogue cannot be read.
pub fn list(state: &AppState) -> AuraResult<Vec<EditProfile>> {
    state.catalog().read(stored)
}

/// Measure one original exactly as a whole-folder edit would: its first-tier preview, the
/// automatic correction, and the scene statistics.
fn measure_original(
    path: &std::path::Path,
    clock: &dyn aura_core::clock::Clock,
) -> Option<Measured> {
    let bytes = std::fs::read(path).ok()?;
    let meta = aura_raw::read_meta(&bytes, path).ok()?;
    let limits = aura_raw::DecodeLimits::tier1();
    let one = aura_raw::thumb::tier1(&bytes, &meta, 512, limits, clock, path).ok()?;
    let decoded = aura_raw::codec::decode_jpeg(&one.jpeg, limits).ok()?;
    let stats = SceneStats {
        developed: one.buffer.source != aura_raw::PixelSource::Demosaiced,
        ..SceneStats::measure(&decoded.data).ok()?
    };
    Some(Measured {
        auto: AutoCorrection::measure(&decoded.data).ok()?,
        stats,
    })
}

/// Measure up to [`MAX_MEASURED`] originals that are still on disk, spread evenly through the
/// catalogue. Aligned with `photos`; a photograph not measured is `None`.
fn measure_originals(
    photos: &[CatalogPhoto],
    clock: &dyn aura_core::clock::Clock,
) -> Vec<Option<Measured>> {
    use rayon::prelude::*;
    let present: Vec<usize> = (0..photos.len())
        .filter(|i| photos[*i].path.is_file())
        .collect();
    let step = present.len().div_ceil(MAX_MEASURED).max(1);
    let wanted: Vec<usize> = present.into_iter().step_by(step).collect();
    let found: Vec<(usize, Option<Measured>)> = wanted
        .par_iter()
        .map(|i| (*i, measure_original(&photos[*i].path, clock)))
        .collect();
    let mut out = vec![None; photos.len()];
    for (i, m) in found {
        if let Some(slot) = out.get_mut(i) {
            *slot = m;
        }
    }
    out
}

/// Learn a style from a Lightroom catalogue and keep it, replacing one of the same name.
///
/// # Errors
/// An unreadable catalogue, too few edited photographs, or a storage failure.
pub fn learn_from_lightroom(state: &AppState, input: &LearnInput) -> IpcResult<LearnedStyle> {
    let path = std::path::PathBuf::from(&input.catalog);
    let catalog = lightroom::read(&path)?;
    let source = path.file_name().map_or_else(
        || "your catalogue".into(),
        |n| n.to_string_lossy().into_owned(),
    );
    let measured = measure_originals(&catalog.photos, state.clock().as_ref());
    let learned = learn_from(
        &catalog.photos,
        &measured,
        &input.name,
        &format!("the Lightroom catalogue {source}"),
    )?;
    let profile = learned.profile.clone();
    let now = aura_catalog::rfc3339(state.clock().now_utc());
    state.catalog().writer().with(move |conn| {
        let mut all = stored(conn)?;
        all.retain(|p| p.id != profile.id);
        if all.len() >= MAX_PROFILES {
            return Err(invalid("Delete a personal style before learning another."));
        }
        all.push(profile);
        store(conn, &all, &now)
    })?;
    Ok(learned)
}

/// Delete a personal profile.
///
/// # Errors
/// A storage failure.
pub fn delete(state: &AppState, id: &str) -> IpcResult<()> {
    let id = id.to_string();
    let now = aura_catalog::rfc3339(state.clock().now_utc());
    state.catalog().writer().with(move |conn| {
        let mut all = stored(conn)?;
        all.retain(|p| p.id != id);
        store(conn, &all, &now)
    })?;
    Ok(())
}

/// Build a settings map from `(key, value)` pairs, for tests.
#[cfg(test)]
fn settings(pairs: &[(&str, Value)]) -> serde_json::Map<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn photo(i: usize, format: &str) -> CatalogPhoto {
        CatalogPhoto {
            path: std::path::PathBuf::from(format!("C:/shoot/{i}.CR2")),
            format: format.into(),
            settings: settings(&[
                ("Exposure2012", json!(0.4 + (i % 3) as f64 * 0.02)),
                ("Contrast2012", json!(12)),
                ("Highlights2012", json!(-60 - (i % 5) as i64)),
                ("Blacks2012", json!(-20)),
                ("Clarity2012", json!(-10)),
                ("HueAdjustmentOrange", json!(-6)),
                ("LuminanceAdjustmentOrange", json!(10)),
                ("ColorGradeMidtoneHue", json!(40)),
                ("ColorGradeMidtoneSat", json!(45)),
                ("ToneCurvePV2012", json!([0, 20, 128, 128, 255, 240])),
                ("Temperature", json!(4100 + i as i64 * 37)),
                ("WhiteBalance", json!("Custom")),
            ]),
            iso: Some(400.0),
        }
    }

    #[test]
    fn a_style_is_the_median_taste_and_leaves_the_light_to_the_measurement() {
        let photos: Vec<CatalogPhoto> = (0..30).map(|i| photo(i, "RAW")).collect();
        let learned = learn_from(&photos, &[], "Warm matte", "a test").unwrap();
        let a = &learned.profile.adjust;
        assert_eq!((a.blacks, a.clarity), (-20, -10));
        // Without the originals, half of what AURA's own correction also measures is taste.
        assert_eq!((a.contrast, a.highlights), (6, -31));
        // White balance is the day's light, never learned.
        assert_eq!((a.temperature, a.tint), (0, 0));
        // A consistent brightening is half kept as taste.
        assert!((a.exposure - 0.21).abs() < 0.02, "{}", a.exposure);
        assert_eq!(a.hsl["orange"], HslShift { h: -6, s: 0, l: 10 });
        // The midtone wheel colours faces: capped.
        let grade = a.colour_grade.unwrap();
        assert_eq!((grade.midtones.hue, grade.midtones.saturation), (40, 20));
        // A faded black point in the curve.
        assert!(a.curve.first().is_some_and(|p| p[1] >= 15));
        assert_eq!(learned.profile.id, "personal-warm-matte");
        assert!(learned.profile.adaptive.is_none());
        assert!(crate::edit_profiles::check(&learned.profile).is_ok());
        assert!(!learned.findings.is_empty());
    }

    #[test]
    fn too_few_photographs_is_refused_and_raw_is_preferred() {
        let few: Vec<CatalogPhoto> = (0..4).map(|i| photo(i, "RAW")).collect();
        assert!(learn_from(&few, &[], "x", "t").is_err());
        let mut mixed: Vec<CatalogPhoto> = (0..12).map(|i| photo(i, "RAW")).collect();
        for i in 0..40 {
            let mut p = photo(i, "JPG");
            p.settings.insert("Contrast2012".into(), json!(60));
            mixed.push(p);
        }
        assert_eq!(
            learn_from(&mixed, &[], "x", "t")
                .unwrap()
                .profile
                .adjust
                .contrast,
            6
        );
    }

    /// A photographer who recovers highlights harder the brighter the frame is: with the
    /// originals measured, the split from AURA's own correction is exact and the dependence is
    /// learned, and a slider that follows nothing stays one value.
    #[test]
    fn with_the_originals_the_tonal_sliders_adapt_per_photograph() {
        let mut photos = Vec::new();
        let mut measured = Vec::new();
        for i in 0..80_usize {
            let high = 0.70 + 0.29 * ((i * 37) % 80) as f32 / 80.0;
            let auto = AutoCorrection {
                exposure: 0.1,
                highlights: -20,
                shadows: 10,
                contrast: 5,
            };
            let stats = SceneStats {
                low: 0.08 + 0.001 * (i % 7) as f32,
                median: 0.4 + 0.002 * (i % 11) as f32,
                high,
                warmth: 0.02 * ((i % 5) as f32 - 2.0),
                chroma_p90: 0.3 + 0.01 * (i % 3) as f32,
                developed: false,
            };
            let mut p = photo(i, "RAW");
            // Their own part: -10 on a dull frame, about -60 on a bright one.
            let own = -10.0 - 170.0 * f64::from(high - 0.70);
            p.settings
                .insert("Highlights2012".into(), json!(-20.0 + own));
            p.settings.insert("Contrast2012".into(), json!(5 + 15));
            photos.push(p);
            measured.push(Some(Measured { auto, stats }));
        }
        let learned = learn_from(&photos, &measured, "Adaptive", "t").unwrap();
        let a = &learned.profile.adjust;
        // The exact split: their contrast is 20, of which AURA already does 5.
        assert_eq!(a.contrast, 15);
        let model = learned
            .profile
            .adaptive
            .clone()
            .expect("a per-photograph model");
        assert!(model.weights.contains_key("highlights"), "{model:?}");
        assert!(!model.weights.contains_key("contrast"));
        assert!(model.held_out_gain["highlights"] > 0.5);
        assert!(crate::edit_profiles::check(&learned.profile).is_ok());
        // Applied: a bright frame gets more recovery than a dull one.
        let auto = AutoCorrection {
            exposure: 0.1,
            highlights: -20,
            shadows: 10,
            contrast: 5,
        };
        let at = |high: f32| {
            let stats = SceneStats {
                low: 0.08,
                median: 0.41,
                high,
                warmth: 0.0,
                chroma_p90: 0.31,
                developed: false,
            };
            model.deviation("highlights", auto, stats)
        };
        assert!(at(0.98) < at(0.72) - 20.0, "{} {}", at(0.98), at(0.72));
        // Bounded, however far outside the training range a photograph is.
        assert!(at(5.0).abs() <= 35.0);
    }

    #[test]
    #[ignore = "needs AURA_LRCAT: a Lightroom Classic catalogue; prints what was learned"]
    fn learns_from_a_real_catalogue() {
        let path = std::env::var("AURA_LRCAT").unwrap();
        let catalog = lightroom::read(std::path::Path::new(&path)).unwrap();
        println!(
            "{} photographs, {} unreadable",
            catalog.photos.len(),
            catalog.unreadable
        );
        let clock = aura_core::clock::SystemClock::default();
        let measured = measure_originals(&catalog.photos, &clock);
        println!("{} originals measured", measured.iter().flatten().count());
        let learned = learn_from(&catalog.photos, &measured, "My style", "test").unwrap();
        println!(
            "learned from {} of {} edited",
            learned.photos, learned.edited
        );
        for f in &learned.findings {
            println!("  {f}");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&learned.profile.adjust).unwrap()
        );
    }
}
