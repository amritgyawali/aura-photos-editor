//! Edit profiles: named, adaptive looks a photographer can apply in one click.
//!
//! A profile is **not** a fixed preset. Lightroom presets write absolute numbers, so a preset
//! built on a bright beach frame over-brightens every indoor frame it touches. A profile here is
//! a *residual on top of a measured correction*:
//!
//! ```text
//! neutral develop
//!   -> the measured local correction (photo_enhance::correction)    what this frame needs
//!   -> + the profile's creative adjustments, scaled by strength     what the look adds
//!   -> scene guards, each of which can only make the look gentler   what this frame tolerates
//!   -> schema::merge, which refuses every field a person set         what the photographer owns
//! ```
//!
//! Every application starts again from the neutral develop, so applying a profile twice, or
//! switching between profiles, never compounds. Manual settings survive every application
//! because the only writer is [`aura_recipe::schema::merge`].
//!
//! The profiles live in `config/edit_profiles.json`. Two kinds ship:
//!
//! * **researched** profiles, whose numbers come from published before/after walkthroughs of
//!   well-known looks (light and airy, dark and moody, teal and orange, Portra 400 and others).
//!   Each one names its sources, and the numbers are expressed in AURA's own recipe units.
//! * **learned** profiles, whose numbers were *measured*: `tools/profile-fit` renders the
//!   camera RAW of a FiveK dataset pair through AURA's own renderer, recovers the settings
//!   that reproduce the professional retoucher's finished photograph by coordinate descent, and
//!   takes the median over many pairs of what the retoucher did *beyond* AURA's own correction.
//!   Each learned profile carries its held-out measurement in `evidence`.
//!
//! The recipe schema has no split-toning, grain or colour-grading wheels, so a look that needs
//! them is expressed through temperature, tint, the point curve and the eight HSL bands - which
//! is how most published walkthroughs build teal-and-orange before colour grading existed.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use aura_core::{AuraError, AuraResult, PhotoId, ProjectId};
use aura_preview::contract::service::{PreviewService, Priority};
use aura_recipe::{schema, Bw, Curve, EditSource, HslShift, Mask, MaskKind, MaskParams, Recipe};
use aura_render::{
    OutputSpec, RenderLevel, RenderPurpose, RenderRequest, RenderService, RenderedData,
};
use serde::{Deserialize, Serialize};

use crate::{commands::IpcResult, AppState};

/// The shipped profile table. Editable data rather than code, checked by the tests below.
const PROFILES_JSON: &str = include_str!("../config/edit_profiles.json");

/// Masks a profile writes carry this id prefix, so a re-application removes exactly its own.
pub const PROFILE_MASK_PREFIX: &str = "profile_";

/// The eight hue bands, in the recipe's own order.
const BANDS: [&str; 8] = aura_recipe::HSL_BANDS;

/// The strongest a profile may be applied. Above one the look is extrapolated, which is useful
/// for a subtle profile and bounded so a strong one cannot be pushed into posterisation.
pub const MAX_STRENGTH: f32 = 1.5;

/// A source a researched profile was built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileSource {
    /// What the page is called.
    pub title: String,
    /// Where it is.
    pub url: String,
}

/// What a learned profile was measured against, on pairs the fit never saw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileEvidence {
    /// The dataset the before/after pairs came from.
    pub dataset: String,
    /// Pairs the profile was learned from.
    pub training_pairs: u32,
    /// Pairs held out for the measurement below.
    pub held_out_pairs: u32,
    /// Mean dE00 between AURA's automatic correction and the retoucher's final.
    pub auto_de00: f32,
    /// Mean dE00 between automatic correction plus this profile and the retoucher's final.
    pub profile_de00: f32,
}

/// The creative half of a profile, in recipe units. Every field defaults to "change nothing".
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ProfileAdjust {
    /// Stops, added to the measured exposure correction.
    pub exposure: f32,
    /// Added to the measured contrast correction.
    pub contrast: i16,
    /// Added to the measured highlight correction.
    pub highlights: i16,
    /// Added to the measured shadow correction.
    pub shadows: i16,
    /// White point.
    pub whites: i16,
    /// Black point.
    pub blacks: i16,
    /// Kelvin offset from the neutral 5,500 K reference; positive renders warmer.
    pub temperature: i32,
    /// Green-magenta offset; positive renders more magenta.
    pub tint: i16,
    /// Coarse local contrast.
    pub clarity: i16,
    /// Fine local contrast.
    pub texture: i16,
    /// Haze removal.
    pub dehaze: i16,
    /// Saturation weighted toward muted colours.
    pub vibrance: i16,
    /// Flat saturation.
    pub saturation: i16,
    /// An absolute point curve in 0-255 units, blended toward identity by strength.
    pub curve: Vec<[u16; 2]>,
    /// Per-band shifts, keys from the eight recipe bands.
    pub hsl: BTreeMap<String, HslShift>,
    /// Output sharpening amount, `0..=150`.
    pub sharpen: i16,
    /// Luminance noise reduction, `0..=100`.
    pub noise: i16,
    /// Edge darkening in stops, `0..=1`, drawn with a radial mask.
    pub vignette: f32,
    /// A black-and-white mix, keys from the eight bands. `None` keeps colour.
    pub bw: Option<BTreeMap<String, i16>>,
    /// Strength multiplier on a photograph that is already developed - a JPEG or PNG, or a
    /// RAW whose mosaic could not be decoded - rather than AURA's own RAW render. `None` is 1.
    ///
    /// A profile learned from RAW before-and-afters includes the retoucher lifting contrast and
    /// saturation out of a flat sensor render. A camera JPEG has already had that done to it by
    /// the camera, and applying it a second time is how a phone photo ends up neon.
    pub developed_strength: Option<f32>,
}

/// One named look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditProfile {
    /// Stable identifier, used on the wire and in the edit history.
    pub id: String,
    /// What the photographer reads.
    pub name: String,
    /// A grouping for the gallery, e.g. `Film` or `Portrait`.
    pub category: String,
    /// One line under the name.
    pub tagline: String,
    /// A paragraph on what the look does.
    pub description: String,
    /// The kinds of photograph it suits.
    pub best_for: Vec<String>,
    /// The editing technique, step by step, as a retoucher would describe it.
    pub technique: Vec<String>,
    /// `researched` or `learned`.
    pub origin: String,
    /// Where the numbers came from.
    #[serde(default)]
    pub sources: Vec<ProfileSource>,
    /// For a learned profile, its held-out measurement.
    #[serde(default)]
    pub evidence: Option<ProfileEvidence>,
    /// Colours that stand in for the look before any photograph exists.
    pub swatch: Vec<String>,
    /// The creative adjustments.
    pub adjust: ProfileAdjust,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileTable {
    version: u16,
    profiles: Vec<EditProfile>,
}

/// Input to [`apply_edit_profile`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyProfileInput {
    /// The photograph.
    pub photo_id: String,
    /// The profile, or `auto` for the measured correction alone.
    pub profile_id: String,
    /// `0..=1.5`; 1 is the profile as designed.
    pub strength: f32,
}

/// What an application did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyProfileReport {
    /// The profile that was applied.
    pub profile_id: String,
    /// How many recipe leaves moved.
    pub changed: usize,
    /// Fields a person set, which the application did not touch.
    pub protected_fields: Vec<String>,
    /// Each scene guard that softened the look, in a sentence.
    pub adaptations: Vec<String>,
}

/// Input to [`preview_edit_profile`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProfileInput {
    /// The profile.
    pub profile_id: String,
    /// A photograph to preview on, or `None` for the built-in sample scene.
    #[serde(default)]
    pub photo_id: Option<String>,
    /// `0..=1.5`.
    pub strength: f32,
    /// Long edge of the preview in pixels, clamped to `64..=640`.
    #[serde(default)]
    pub size: Option<u32>,
}

/// A before and after, both rendered by the export renderer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePreview {
    /// The profile.
    pub profile_id: String,
    /// JPEG data URL of the neutral develop.
    pub before: String,
    /// JPEG data URL with the profile applied.
    pub after: String,
    /// Each scene guard that softened the look.
    pub adaptations: Vec<String>,
}

fn refused(message: impl Into<String>) -> AuraError {
    let message = message.into();
    let mut error = aura_core::errors::render::recipe_invalid("profile", &message);
    error.user_message = message;
    error
}

/// Every shipped profile, parsed and validated once.
///
/// # Errors
/// Only if the shipped table is malformed, which the tests below rule out.
pub fn profiles() -> AuraResult<&'static [EditProfile]> {
    static TABLE: OnceLock<Result<Vec<EditProfile>, String>> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let parsed: ProfileTable =
            serde_json::from_str(PROFILES_JSON).map_err(|e| e.to_string())?;
        if parsed.version != 1 {
            return Err(format!(
                "unsupported profile table version {}",
                parsed.version
            ));
        }
        for profile in &parsed.profiles {
            validate(profile)?;
        }
        Ok(parsed.profiles)
    });
    match table {
        Ok(profiles) => Ok(profiles.as_slice()),
        Err(why) => Err(refused(format!("The edit profile table is invalid: {why}"))),
    }
}

/// The profile with this id.
///
/// # Errors
/// `AURA-RENDER-8002` when no profile has that id.
pub fn profile(id: &str) -> AuraResult<&'static EditProfile> {
    profiles()?
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| refused(format!("There is no edit profile called {id}.")))
}

fn validate(profile: &EditProfile) -> Result<(), String> {
    let a = &profile.adjust;
    let bad = |what: &str| Err(format!("{}: {what}", profile.id));
    if profile.id.trim().is_empty() || profile.id == "auto" {
        return bad("reserved or empty id");
    }
    if !matches!(profile.origin.as_str(), "researched" | "learned") {
        return bad("origin must be researched or learned");
    }
    if profile.origin == "researched" && profile.sources.is_empty() {
        return bad("a researched profile must name its sources");
    }
    if profile.origin == "learned" && profile.evidence.is_none() {
        return bad("a learned profile must carry its measurement");
    }
    if !(-2.0..=2.0).contains(&a.exposure) || !(0.0..=1.0).contains(&a.vignette) {
        return bad("exposure or vignette out of range");
    }
    if !(-3000..=3000).contains(&a.temperature) {
        return bad("temperature offset out of range");
    }
    let scalars = [
        a.contrast,
        a.highlights,
        a.shadows,
        a.whites,
        a.blacks,
        a.tint,
        a.clarity,
        a.texture,
        a.dehaze,
        a.vibrance,
        a.saturation,
    ];
    if scalars.iter().any(|v| !(-100..=100).contains(v)) {
        return bad("a slider is outside -100..100");
    }
    if a.developed_strength
        .is_some_and(|k| !(0.0..=1.0).contains(&k))
    {
        return bad("developed strength must be 0..1");
    }
    if !(0..=150).contains(&a.sharpen) || !(0..=100).contains(&a.noise) {
        return bad("sharpen or noise out of range");
    }
    if !a.curve.is_empty() {
        let curve = Curve {
            points: a.curve.clone(),
        };
        let mut probe = aura_recipe::fixtures::neutral(&"0".repeat(64), "");
        probe.global.curve = curve;
        schema::Validation::check(&probe).map_err(|e| format!("{}: {}", profile.id, e.detail))?;
        if a.curve
            .windows(2)
            .any(|w| matches!(w, [p, q] if q[1] < p[1]))
        {
            return bad("the curve must not invert tones");
        }
    }
    let bands = a.hsl.keys().chain(a.bw.iter().flat_map(BTreeMap::keys));
    for band in bands {
        if !BANDS.contains(&band.as_str()) {
            return bad("unknown hue band");
        }
    }
    let shifts = a.hsl.values().flat_map(|s| [s.h, s.s, s.l]);
    if shifts
        .chain(a.bw.iter().flat_map(|m| m.values().copied()))
        .any(|v| !(-100..=100).contains(&v))
    {
        return bad("a band shift is outside -100..100");
    }
    if profile.swatch.is_empty()
        || !profile.swatch.iter().all(|c| {
            c.len() == 7 && c.starts_with('#') && c.chars().skip(1).all(|h| h.is_ascii_hexdigit())
        })
    {
        return bad("swatch must be #rrggbb colours");
    }
    Ok(())
}

/// What the scene guards read off a photograph's preview. Display-referred, `0..=1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneStats {
    /// 10th percentile of display lightness.
    pub low: f32,
    /// Median display lightness.
    pub median: f32,
    /// 99th percentile of display lightness.
    pub high: f32,
    /// Warm-cool lean of the midtones: mean `(r - b) / (r + g + b)` in linear light.
    pub warmth: f32,
    /// 90th percentile of per-pixel chroma, `max - min` of the display channels.
    pub chroma_p90: f32,
    /// True when these pixels were already developed by a camera or another editor rather than
    /// rendered by AURA from a RAW mosaic. [`SceneStats::measure`] cannot tell and says false.
    pub developed: bool,
}

impl SceneStats {
    /// Measure an sRGB 8-bit buffer.
    ///
    /// # Errors
    /// `AURA-RAW-2002` for an empty or truncated buffer.
    pub fn measure(rgb: &[u8]) -> AuraResult<Self> {
        if rgb.is_empty() || !rgb.len().is_multiple_of(3) {
            return Err(aura_core::errors::raw::corrupt("Incomplete profile pixels"));
        }
        let mut light = [0_u32; 256];
        let mut chroma = [0_u32; 256];
        let (mut warm_sum, mut warm_count) = (0.0_f64, 0_u32);
        for pixel in rgb.chunks_exact(3) {
            let [r, g, b] = [0, 1, 2].map(|i| pixel.get(i).copied().unwrap_or(0));
            let lum = (u32::from(r) * 54 + u32::from(g) * 183 + u32::from(b) * 19) >> 8;
            if let Some(slot) = light.get_mut(lum as usize) {
                *slot += 1;
            }
            let spread = r.max(g).max(b) - r.min(g).min(b);
            if let Some(slot) = chroma.get_mut(usize::from(spread)) {
                *slot += 1;
            }
            // Midtones only: a clipped window or a black shadow says nothing about the light.
            if (40..=215).contains(&lum) {
                let [lr, lg, lb] =
                    [r, g, b].map(|v| aura_raw::colour::curve::srgb_decode(f32::from(v) / 255.0));
                let sum = lr + lg + lb;
                if sum > 1e-4 {
                    warm_sum += f64::from((lr - lb) / sum);
                    warm_count += 1;
                }
            }
        }
        let total = u32::try_from(rgb.len() / 3).unwrap_or(u32::MAX);
        let quantile = |hist: &[u32; 256], q: u32| {
            let target = total.saturating_sub(1).saturating_mul(q) / 100;
            let mut seen = 0_u32;
            for (bin, count) in (0_u16..256).zip(hist.iter()) {
                seen = seen.saturating_add(*count);
                if seen > target {
                    return f32::from(bin) / 255.0;
                }
            }
            1.0
        };
        #[allow(clippy::cast_possible_truncation)]
        let warmth = if warm_count > 0 {
            (warm_sum / f64::from(warm_count)) as f32
        } else {
            0.0
        };
        Ok(Self {
            low: quantile(&light, 10),
            median: quantile(&light, 50),
            high: quantile(&light, 99),
            warmth,
            chroma_p90: quantile(&chroma, 90),
            developed: false,
        })
    }
}

/// The measured correction, as [`crate::photo_enhance::correction`] returns it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AutoCorrection {
    /// Stops.
    pub exposure: f32,
    /// Highlight recovery.
    pub highlights: i16,
    /// Shadow lift.
    pub shadows: i16,
    /// Contrast.
    pub contrast: i16,
}

impl AutoCorrection {
    /// Measure the correction a preview needs.
    ///
    /// # Errors
    /// As [`crate::photo_enhance::correction`].
    pub fn measure(rgb: &[u8]) -> AuraResult<Self> {
        let (exposure, highlights, shadows, contrast) = crate::photo_enhance::correction(rgb)?;
        Ok(Self {
            exposure,
            highlights,
            shadows,
            contrast,
        })
    }
}

#[allow(clippy::cast_possible_truncation)]
fn scaled(value: i16, strength: f32) -> i16 {
    (f32::from(value) * strength).round().clamp(-150.0, 150.0) as i16
}

fn add(base: i16, delta: i16) -> i16 {
    base.saturating_add(delta).clamp(-100, 100)
}

/// Build the recipe a profile produces over `current`, without saving it.
///
/// `current` is whatever the photograph carries now; the profile is written over a *neutral*
/// develop of the same image and then merged back, so nothing it owns compounds and nothing a
/// person set is touched. Returns the merged recipe, the merge report and the guard notes.
///
/// # Errors
/// `AURA-RENDER-8002` when the merged recipe does not validate.
#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn build(
    current: &Recipe,
    profile: Option<&EditProfile>,
    strength: f32,
    auto: AutoCorrection,
    stats: SceneStats,
) -> AuraResult<(Recipe, schema::MergeReport, Vec<String>)> {
    if !strength.is_finite() || !(0.0..=MAX_STRENGTH).contains(&strength) {
        return Err(refused(
            "Profile strength must be between 0 and 150 percent.",
        ));
    }
    let neutral =
        aura_recipe::fixtures::neutral(&current.image.content_hash, &current.image.camera);
    let mut proposal = current.clone();
    proposal.global = neutral.global.clone();
    for band in BANDS {
        proposal
            .global
            .hsl
            .insert(band.to_string(), HslShift::default());
    }
    proposal
        .masks
        .retain(|mask| !mask.id.starts_with(PROFILE_MASK_PREFIX));
    proposal.bw = None;

    let g = &mut proposal.global;
    g.exposure = auto.exposure;
    g.highlights = auto.highlights;
    g.shadows = auto.shadows;
    g.contrast = auto.contrast;
    let mut notes = Vec::new();

    if let Some(profile) = profile {
        let a = &profile.adjust;
        let mut s = strength;

        // Guard 0 - a look learned from RAW files on a photograph that is already developed.
        if let Some(scale) = a.developed_strength.filter(|_| stats.developed) {
            let scale = scale.clamp(0.0, 1.0);
            if scale < 1.0 {
                s *= scale;
                notes.push(format!(
                    "Applied at {:.0}% because this look was learned from RAW files and this                      photo was already developed by the camera.",
                    scale * 100.0
                ));
            }
        }

        // Guard 1 - exposure headroom. A brightening look on a frame whose top percentile is
        // already near white would clip the sky; only the part that fits is applied.
        let mut exposure = a.exposure * s;
        if exposure > 0.0 {
            let top = aura_raw::colour::curve::srgb_decode(stats.high).max(0.005);
            let headroom = ((0.97 / top).log2() - auto.exposure.max(0.0)).max(0.0);
            if exposure > headroom + 0.02 {
                notes.push(format!(
                    "Brightening held to {headroom:+.2} EV to keep the brightest areas from clipping."
                ));
                exposure = headroom;
            }
        }
        // Guard 2 - a darkening look on an already low-key frame. Night and candle-lit frames
        // are dark on purpose; darkening them again crushes the subject into the shadows.
        if exposure < 0.0 && stats.median < 0.22 {
            exposure *= 0.4;
            notes.push("The photograph is already dark, so the look darkens it less.".to_string());
        }
        g.exposure = (g.exposure + exposure).clamp(-5.0, 5.0);

        // Guard 3 - the colour lean. A warming look on a frame already lit by tungsten turns
        // skin orange; a cooling look on a blue-hour frame turns it grey. Scale the offset by
        // how much room the frame's own lean leaves in that direction.
        let lean = stats.warmth;
        let mut temperature = a.temperature as f32 * s;
        if temperature > 0.0 && lean > 0.06 {
            let keep = (1.0 - (lean - 0.06) / 0.12).clamp(0.2, 1.0);
            temperature *= keep;
            notes.push(format!(
                "Warmth reduced to {:.0}% because the light is already warm.",
                keep * 100.0
            ));
        } else if temperature < 0.0 && lean < -0.04 {
            let keep = (1.0 - (-lean - 0.04) / 0.12).clamp(0.2, 1.0);
            temperature *= keep;
            notes.push(format!(
                "Cooling reduced to {:.0}% because the light is already cool.",
                keep * 100.0
            ));
        }
        g.temperature = (5500.0 + temperature).round().clamp(2000.0, 50_000.0) as u32;
        g.tint = scaled(a.tint, s).clamp(-150, 150);

        // Guard 4 - saturation. A vivid look on a frame that is already saturated (stage
        // lighting, a neon sign) pushes colours out of gamut; its boosts are halved.
        let saturated = stats.chroma_p90 > 0.62;
        let boost = |v: i16| {
            if saturated && v > 0 {
                scaled(v, s * 0.5)
            } else {
                scaled(v, s)
            }
        };
        if saturated && (a.vibrance > 0 || a.saturation > 0) {
            notes.push("Colour boost halved because the photograph is already vivid.".to_string());
        }
        g.vibrance = boost(a.vibrance).clamp(-100, 100);
        g.saturation = boost(a.saturation).clamp(-100, 100);

        g.contrast = add(g.contrast, scaled(a.contrast, s));
        g.highlights = add(g.highlights, scaled(a.highlights, s));
        g.shadows = add(g.shadows, scaled(a.shadows, s));
        g.whites = scaled(a.whites, s).clamp(-100, 100);
        g.blacks = scaled(a.blacks, s).clamp(-100, 100);
        g.clarity = scaled(a.clarity, s).clamp(-100, 100);
        g.texture = scaled(a.texture, s).clamp(-100, 100);
        g.dehaze = scaled(a.dehaze, s).clamp(-100, 100);

        if !a.curve.is_empty() {
            let blend = s.min(1.0);
            let points: Vec<[u16; 2]> = a
                .curve
                .iter()
                .map(|[x, y]| {
                    let (x, y) = (f32::from(*x), f32::from(*y));
                    [
                        x as u16,
                        (x + (y - x) * blend).round().clamp(0.0, 255.0) as u16,
                    ]
                })
                .collect();
            g.curve = Curve { points };
        }

        // Guard 5 - skin. Orange is the band human skin of every tone sits in. A look may
        // lighten or mute it a little, but a hue rotation or a heavy saturation change there
        // is how a gallery ends up with orange or grey people, so both are bounded.
        let mut skin_limited = false;
        for (band, shift) in &a.hsl {
            let mut h = scaled(shift.h, s);
            let mut sat = scaled(shift.s, s);
            if band == "orange" && a.bw.is_none() {
                let (bounded_h, bounded_s) = (h.clamp(-6, 6), sat.clamp(-20, 15));
                skin_limited |= bounded_h != h || bounded_s != sat;
                h = bounded_h;
                sat = bounded_s;
            }
            g.hsl.insert(
                band.clone(),
                HslShift {
                    h: h.clamp(-100, 100),
                    s: sat.clamp(-100, 100),
                    l: scaled(shift.l, s).clamp(-100, 100),
                },
            );
        }
        if skin_limited {
            notes.push("Skin-tone shift limited so people keep their natural colour.".to_string());
        }

        if a.sharpen > 0 {
            g.sharpen.amount = scaled(a.sharpen, s.min(1.0)).clamp(0, 150);
            g.sharpen.masking = 40;
        }
        if a.noise > 0 {
            g.noise.luminance = scaled(a.noise, s.min(1.0)).clamp(0, 100);
            g.noise.colour = g.noise.luminance;
        }

        if a.vignette > 0.0 {
            // The renderer's radial mask is one at the centre and falls to zero at a third of
            // the frame, so the edge darkening is a global move with the centre lifted back.
            let v = (a.vignette * s).min(1.0);
            g.exposure = (g.exposure - v).clamp(-5.0, 5.0);
            proposal.masks.push(Mask {
                id: format!("{PROFILE_MASK_PREFIX}vignette"),
                kind: MaskKind::Radial,
                target: None,
                invert_of: None,
                feather: 1.0,
                params: MaskParams {
                    exposure: Some(v * 1.1),
                    ..MaskParams::default()
                },
            });
        }

        if let Some(mix) = &a.bw {
            // A monochrome look is a decision, not a degree: any strength converts, and the
            // strength scales the mix and the tonal moves instead.
            proposal.bw = Some(Bw {
                mix: mix
                    .iter()
                    .map(|(band, v)| (band.clone(), scaled(*v, s.min(1.0)).clamp(-100, 100)))
                    .collect(),
                grade: None,
            });
        }
        s = s.min(1.0);
        proposal.provenance.style_profile = Some(format!("{}@{:.0}", profile.id, s * 100.0));
    } else {
        proposal.provenance.style_profile = None;
    }
    proposal.provenance.source = EditSource::Ai;
    proposal.provenance.confidence = 0.45;
    let (merged, report) = schema::merge(current, &proposal, EditSource::Ai)?;
    schema::Validation::check(&merged)?;
    Ok((merged, report, notes))
}

fn photo_project(state: &AppState, photo_id: &str) -> AuraResult<(PhotoId, String, ProjectId)> {
    let photo = PhotoId::from_db(photo_id).map_err(|_| refused("Invalid photo identifier"))?;
    let key = photo_id.to_string();
    let project_key: String = state.catalog().read(move |conn| {
        conn.query_row(
            "SELECT project_id FROM photo WHERE photo_id=?1",
            [&key],
            |row| row.get(0),
        )
        .map_err(|e| aura_core::errors::db::statement_failed("edit profile photo", &e))
    })?;
    let project = ProjectId::from_db(&project_key).map_err(|_| refused("Invalid collection"))?;
    Ok((photo, project_key, project))
}

fn measure_photo(
    state: &AppState,
    photo: PhotoId,
    project_key: &str,
) -> AuraResult<(AutoCorrection, SceneStats)> {
    let pixels = state.previews(project_key)?.get(
        photo,
        aura_raw::PixelLevel::Thumb(512),
        Priority::Interactive,
    )?;
    let rgb = pixels
        .as_srgb8()
        .ok_or_else(|| refused("An sRGB preview is required"))?;
    let stats = SceneStats {
        developed: pixels.source != aura_raw::PixelSource::Demosaiced,
        ..SceneStats::measure(rgb)?
    };
    Ok((AutoCorrection::measure(rgb)?, stats))
}

fn resolve(id: &str) -> AuraResult<Option<&'static EditProfile>> {
    if id == "auto" {
        Ok(None)
    } else {
        profile(id).map(Some)
    }
}

/// Every profile, for the gallery.
///
/// # Errors
/// Only if the shipped table is malformed.
pub fn list_edit_profiles() -> IpcResult<Vec<EditProfile>> {
    Ok(profiles()?.to_vec())
}

/// The recipe a profile produces for one photograph, built but not saved.
///
/// # Errors
/// Unknown profile, missing photograph, preview decode, or an invalid merge.
pub fn profile_recipe(
    state: &AppState,
    photo_id: &str,
    profile_id: &str,
    strength: f32,
) -> AuraResult<(Recipe, schema::MergeReport, Vec<String>)> {
    let chosen = resolve(profile_id)?;
    let (photo, project_key, _) = photo_project(state, photo_id)?;
    let (auto, stats) = measure_photo(state, photo, &project_key)?;
    let current = crate::develop_commands::load_or_neutral(state, photo)?;
    build(&current, chosen, strength, auto, stats)
}

/// Apply a profile to one photograph and save it, protecting every manual setting.
///
/// # Errors
/// Unknown profile, strength out of range, missing photograph, decode or storage failure.
pub fn apply_edit_profile(
    state: &AppState,
    input: &ApplyProfileInput,
) -> IpcResult<ApplyProfileReport> {
    let (photo, _, project) = photo_project(state, &input.photo_id)?;
    let (merged, report, adaptations) =
        profile_recipe(state, &input.photo_id, &input.profile_id, input.strength)?;
    let name = resolve(&input.profile_id)?.map_or("Auto", |p| p.name.as_str());
    let mut message = format!(
        "Edit profile: {name} at {:.0}% over the measured correction.",
        input.strength * 100.0
    );
    for note in &adaptations {
        message.push(' ');
        message.push_str(note);
    }
    state
        .recipe_store()
        .save(&project, &photo, &merged, &report.changed, &message)?;
    Ok(ApplyProfileReport {
        profile_id: input.profile_id.clone(),
        changed: report.changed.len(),
        protected_fields: report.refused,
        adaptations,
    })
}

fn data_url(width: u32, height: u32, data: &RenderedData) -> AuraResult<String> {
    let bytes = match data {
        RenderedData::Eight(bytes) => bytes.clone(),
        RenderedData::Sixteen(words) => words
            .iter()
            .map(|w| u8::try_from(w >> 8).unwrap_or(u8::MAX))
            .collect(),
    };
    let jpeg = aura_raw::codec::encode_jpeg(
        &aura_raw::codec::Rgb8 {
            width,
            height,
            data: bytes,
        },
        88,
    )?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        crate::preview_commands::base64(&jpeg)
    ))
}

/// Render a before and after of a profile through the export renderer, without saving.
///
/// On a photograph when one is given, otherwise on a built-in sample scene with sky, skin,
/// foliage, a white dress and deep shadow - the five things every look has to get right.
///
/// # Errors
/// Unknown profile, missing photograph, decode or render failure.
pub fn preview_edit_profile(
    state: &AppState,
    input: &PreviewProfileInput,
) -> IpcResult<ProfilePreview> {
    let chosen = resolve(&input.profile_id)?;
    let edge = input.size.unwrap_or(360).clamp(64, 640);
    let engine = state.render()?;
    let output = OutputSpec::default();
    let (before, after, adaptations) = if let Some(photo_id) = &input.photo_id {
        let (photo, project_key, _) = photo_project(state, photo_id)?;
        let (auto, stats) = measure_photo(state, photo, &project_key)?;
        let current = crate::develop_commands::load_or_neutral(state, photo)?;
        let neutral =
            aura_recipe::fixtures::neutral(&current.image.content_hash, &current.image.camera);
        let (recipe, _, notes) = build(&neutral, chosen, input.strength, auto, stats)?;
        let render = |recipe: Recipe| {
            engine.render(RenderRequest {
                image_id: photo,
                recipe,
                level: RenderLevel::Screen(edge, edge),
                output: output.clone(),
                purpose: RenderPurpose::Interactive,
            })
        };
        let before = render(neutral)?;
        let after = render(recipe)?;
        (
            data_url(before.width, before.height, &before.data)?,
            data_url(after.width, after.height, &after.data)?,
            notes,
        )
    } else {
        let (width, height) = (edge, edge * 2 / 3);
        let scene = sample_scene(width, height);
        let frame = aura_render::Frame::working(
            crate::photo_frames::srgb8_to_working(&scene),
            width,
            height,
            "",
        );
        let neutral = aura_recipe::fixtures::neutral(&"0".repeat(64), "");
        let auto = AutoCorrection::measure(&scene)?;
        let stats = SceneStats::measure(&scene)?;
        let (recipe, _, notes) = build(&neutral, chosen, input.strength, auto, stats)?;
        let render = |recipe: &Recipe| {
            engine.render_frame(
                &frame,
                recipe,
                RenderLevel::Screen(width, height),
                RenderPurpose::Analysis,
                &output,
            )
        };
        let before = render(&neutral)?;
        let after = render(&recipe)?;
        (
            data_url(before.width, before.height, &before.data)?,
            data_url(after.width, after.height, &after.data)?,
            notes,
        )
    };
    Ok(ProfilePreview {
        profile_id: input.profile_id.clone(),
        before,
        after,
        adaptations,
    })
}

/// A small synthetic photograph: sky, sun glow, hills, a face, a white dress, a dark suit.
///
/// Deterministic and resolution-independent, so the gallery previews every profile on the same
/// scene before the photographer has imported anything.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn sample_scene(width: u32, height: u32) -> Vec<u8> {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    let mut out = Vec::with_capacity(width as usize * height as usize * 3);
    let mix = |[ar, ag, ab]: [f32; 3], [br, bg, bb]: [f32; 3], t: f32| -> [f32; 3] {
        let t = t.clamp(0.0, 1.0);
        [ar + (br - ar) * t, ag + (bg - ag) * t, ab + (bb - ab) * t]
    };
    for y in 0..height {
        for x in 0..width {
            let (u, v) = (x as f32 / w, y as f32 / h);
            // Sky: deep blue at the top to a pale warm horizon.
            let mut c = mix([70.0, 120.0, 190.0], [235.0, 205.0, 170.0], v / 0.55);
            let sun = ((u - 0.78).powi(2) + (v - 0.36).powi(2)).sqrt();
            c = mix(c, [255.0, 238.0, 205.0], 1.0 - sun / 0.12);
            // Hills: two layers of green, the far one hazier.
            let far = 0.52 + 0.05 * (u * 7.0).sin();
            let near = 0.64 + 0.06 * (u * 4.3 + 1.0).cos();
            if v > far {
                c = mix([120.0, 150.0, 120.0], [95.0, 130.0, 80.0], (v - far) * 6.0);
            }
            if v > near {
                c = mix([70.0, 115.0, 45.0], [35.0, 60.0, 25.0], (v - near) * 3.0);
            }
            // A couple: a white dress, a dark suit, two faces.
            let dress = (u - 0.36).abs() < 0.07 + (v - 0.55) * 0.18 && v > 0.55;
            let suit = (u - 0.52).abs() < 0.06 && v > 0.55;
            if dress {
                c = mix(
                    [246.0, 243.0, 236.0],
                    [200.0, 196.0, 190.0],
                    (u - 0.29) * 4.0,
                );
            }
            if suit {
                c = [28.0, 30.0, 36.0];
            }
            for (fx, tone) in [
                (0.36_f32, [226.0, 176.0, 146.0]),
                (0.52, [150.0, 100.0, 72.0]),
            ] {
                let face = ((u - fx) / 0.035).powi(2) + ((v - 0.49) / 0.06).powi(2);
                if face < 1.0 {
                    let [r, g, b] = tone;
                    c = mix(tone, [r * 0.8, g * 0.78, b * 0.76], face);
                }
            }
            out.extend(c.map(|value| value.round().clamp(0.0, 255.0) as u8));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral() -> Recipe {
        aura_recipe::fixtures::neutral(&"a".repeat(64), "Bench-01")
    }

    fn stats() -> SceneStats {
        SceneStats::measure(&sample_scene(96, 64)).unwrap()
    }

    #[test]
    fn the_table_parses_and_ships_more_than_ten_profiles() {
        let all = profiles().unwrap();
        assert!(all.len() > 10, "only {} profiles", all.len());
        let mut ids: Vec<_> = all.iter().map(|p| p.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "profile ids must be unique");
        assert!(
            all.iter().any(|p| p.adjust.bw.is_some()),
            "a monochrome look ships"
        );
    }

    #[test]
    fn every_profile_builds_a_valid_recipe_at_every_strength() {
        for profile in profiles().unwrap() {
            for strength in [0.0, 0.5, 1.0, MAX_STRENGTH] {
                let (recipe, _, _) = build(
                    &neutral(),
                    Some(profile),
                    strength,
                    AutoCorrection::default(),
                    stats(),
                )
                .unwrap_or_else(|e| panic!("{}: {}", profile.id, e.detail));
                schema::Validation::check(&recipe).unwrap();
            }
        }
    }

    #[test]
    fn applying_twice_or_switching_never_compounds() {
        let all = profiles().unwrap();
        let (first, second) = (&all[0], &all[1]);
        let auto = AutoCorrection::default();
        let (once, _, _) = build(&neutral(), Some(first), 1.0, auto, stats()).unwrap();
        let (twice, _, _) = build(&once, Some(first), 1.0, auto, stats()).unwrap();
        assert_eq!(once.global, twice.global);
        assert_eq!(once.masks, twice.masks);
        let (switched, _, _) = build(&twice, Some(second), 1.0, auto, stats()).unwrap();
        let (direct, _, _) = build(&neutral(), Some(second), 1.0, auto, stats()).unwrap();
        assert_eq!(switched.global, direct.global);
        assert_eq!(switched.bw, direct.bw);
        assert_eq!(switched.masks, direct.masks);
    }

    #[test]
    fn a_manual_setting_is_never_overwritten() {
        let airy = profile("light-airy").unwrap();
        let (mut edited, _) = {
            let mut proposal = neutral();
            proposal.global.exposure = -1.0;
            schema::merge(&neutral(), &proposal, EditSource::User).unwrap()
        };
        edited.provenance.user_edited_fields = vec!["global.exposure".to_string()];
        let (after, report, _) =
            build(&edited, Some(airy), 1.0, AutoCorrection::default(), stats()).unwrap();
        assert!((after.global.exposure + 1.0).abs() < 1e-6);
        assert!(report.refused.iter().any(|p| p == "global.exposure"));
    }

    #[test]
    fn strength_zero_is_the_measured_correction_alone() {
        let moody = profile("dark-moody").unwrap();
        let auto = AutoCorrection {
            exposure: 0.3,
            highlights: -10,
            shadows: 12,
            contrast: 4,
        };
        let (zero, _, _) = build(&neutral(), Some(moody), 0.0, auto, stats()).unwrap();
        let (plain, _, _) = build(&neutral(), None, 1.0, auto, stats()).unwrap();
        assert_eq!(zero.global.exposure, plain.global.exposure);
        assert_eq!(zero.global.contrast, plain.global.contrast);
        assert_eq!(zero.global.temperature, 5500);
    }

    #[test]
    fn guards_soften_a_look_on_a_frame_that_cannot_take_it() {
        let golden = profile("golden-hour").unwrap();
        let warm = SceneStats {
            warmth: 0.2,
            ..stats()
        };
        let (on_warm, _, notes) = build(
            &neutral(),
            Some(golden),
            1.0,
            AutoCorrection::default(),
            warm,
        )
        .unwrap();
        let (on_plain, _, _) = build(
            &neutral(),
            Some(golden),
            1.0,
            AutoCorrection::default(),
            stats(),
        )
        .unwrap();
        assert!(on_warm.global.temperature < on_plain.global.temperature);
        assert!(notes.iter().any(|n| n.contains("already warm")));

        let bright = SceneStats {
            high: 1.0,
            ..stats()
        };
        let airy = profile("light-airy").unwrap();
        let (held, _, notes) = build(
            &neutral(),
            Some(airy),
            1.0,
            AutoCorrection::default(),
            bright,
        )
        .unwrap();
        assert!(held.global.exposure <= 0.01);
        assert!(notes.iter().any(|n| n.contains("clipping")));
    }

    #[test]
    fn skin_is_protected_in_every_colour_profile() {
        for profile in profiles().unwrap() {
            let (recipe, _, _) = build(
                &neutral(),
                Some(profile),
                MAX_STRENGTH,
                AutoCorrection::default(),
                stats(),
            )
            .unwrap();
            if recipe.bw.is_none() {
                let orange = recipe.global.hsl.get("orange").copied().unwrap_or_default();
                assert!(orange.h.abs() <= 6, "{} rotates skin", profile.id);
                assert!(
                    (-20..=15).contains(&orange.s),
                    "{} over-shifts skin",
                    profile.id
                );
            }
        }
    }

    #[test]
    fn every_profile_visibly_changes_the_sample_through_the_real_renderer() {
        let (width, height) = (96, 64);
        let scene = sample_scene(width, height);
        let frame = aura_render::Frame::working(
            crate::photo_frames::srgb8_to_working(&scene),
            width,
            height,
            "",
        );
        let engine = aura_render::CpuEngine::new(
            aura_render::fixtures::StaticSource::shared(frame.clone()),
            aura_core::clock::FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
        );
        let render = |recipe: &Recipe| match engine
            .render_frame(
                &frame,
                recipe,
                RenderLevel::Screen(width, height),
                RenderPurpose::Analysis,
                &OutputSpec::default(),
            )
            .unwrap()
            .data
        {
            RenderedData::Eight(bytes) => bytes,
            RenderedData::Sixteen(_) => panic!("8-bit expected"),
        };
        let base = render(&neutral());
        let mut outputs = Vec::new();
        for profile in profiles().unwrap() {
            let (recipe, _, _) = build(
                &neutral(),
                Some(profile),
                1.0,
                AutoCorrection::default(),
                stats(),
            )
            .unwrap();
            let out = render(&recipe);
            let moved = base
                .iter()
                .zip(&out)
                .map(|(a, b)| f64::from(a.abs_diff(*b)))
                .sum::<f64>()
                / base.len() as f64;
            assert!(
                moved > 2.0,
                "{} barely changes the photo ({moved:.2})",
                profile.id
            );
            outputs.push((profile.id.clone(), out));
        }
        for (i, (a_id, a)) in outputs.iter().enumerate() {
            for (b_id, b) in outputs.iter().skip(i + 1) {
                let apart = a
                    .iter()
                    .zip(b)
                    .map(|(x, y)| f64::from(x.abs_diff(*y)))
                    .sum::<f64>()
                    / a.len() as f64;
                assert!(apart > 1.0, "{a_id} and {b_id} look the same ({apart:.2})");
            }
        }
    }

    #[test]
    fn a_raw_learned_look_is_gentler_on_a_developed_photo() {
        let mut learned = profile("vivid-landscape").unwrap().clone();
        learned.adjust.developed_strength = Some(0.5);
        let raw = stats();
        let jpeg = SceneStats {
            developed: true,
            ..raw
        };
        let (on_raw, _, _) = build(
            &neutral(),
            Some(&learned),
            1.0,
            AutoCorrection::default(),
            raw,
        )
        .unwrap();
        let (on_jpeg, _, notes) = build(
            &neutral(),
            Some(&learned),
            1.0,
            AutoCorrection::default(),
            jpeg,
        )
        .unwrap();
        assert!(on_jpeg.global.vibrance < on_raw.global.vibrance);
        assert!(notes.iter().any(|n| n.contains("learned from RAW")));
    }

    #[test]
    fn scene_stats_read_the_light() {
        let warm: Vec<u8> = [200_u8, 140, 90].repeat(100);
        let cool: Vec<u8> = [90_u8, 140, 200].repeat(100);
        assert!(SceneStats::measure(&warm).unwrap().warmth > 0.1);
        assert!(SceneStats::measure(&cool).unwrap().warmth < -0.1);
        assert!(SceneStats::measure(&[]).is_err());
    }
}
