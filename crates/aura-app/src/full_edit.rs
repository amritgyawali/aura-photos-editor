//! The full automatic edit: the same ten stages, in the same order, for every photograph.
//! ADR-0187.
//!
//! | Group | Stage | Where the work is done |
//! |---|---|---|
//! | Edit | 1 Basic edit | exposure, contrast, highlights, shadows, whites, blacks, levels, clarity, dehaze, noise ([`advanced_retouch`] foundation) |
//! | Edit | 2 Colour correction | white balance, tint, vibrance, colour fringing |
//! | Edit | 3 Calibration | **this module**: shadow tint and the red and blue primaries, measured |
//! | Edit | 4 Colour grading | **this module**: a restrained split tone from the frame's own light |
//! | Edit | 5 Crop & straighten | a horizon the photograph's straight lines measure, cropped so no corner is empty |
//! | Retouch | 6 Blemishes | spots healed from nearby skin; moles and freckles kept |
//! | Retouch | 7 Skin smoothing | texture-keeping smoothing, pores, lines softened, tone evened |
//! | Retouch | 8 Face | eyes, under-eyes, teeth, lips, dodge and burn, sculpting light |
//! | Retouch | 9 Body | neck, shoulders and arms that match the person's own face skin |
//! | Advanced | 10 Advanced edit | background, hair, clothing, jewellery, background toning, grain, output sharpening, quality control |
//!
//! Every stage is reported, whatever it did: **applied**, **nothing needed**, **does not apply**
//! (no person in the frame) or **kept your setting** (a value a person set by hand, or a look
//! the photographer chose). A stage that changed something is its own history step.
//!
//! Stages 3 and 4 are decided on the rendered result of stages 1, 2 and 5 to 10, and each is
//! checked again on a render of what it proposes: a calibration that did not reduce what it
//! measured, or a grade that moved a face's colour, is reduced or withdrawn. Neither compares
//! anybody's skin with an ideal colour.
// Pixel statistics convert between f32 and integer sliders on purpose.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    // Pixel loops use the conventional r, g, b, w, h, n; a stage is built beside the stages.
    clippy::many_single_char_names,
    clippy::similar_names
)]

use crate::{
    advanced_retouch::{self, AdvancedRetouchInput, Outcome, Stage},
    commands::IpcResult,
    contract::ipc::RecipeDto,
    portrait_auto::{PREFIX, SCENE_PREFIX},
    AppState,
};
use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_preview::contract::service::{PreviewService, Priority};
use aura_recipe::{retouch_tools, schema, Calibration, ColourGrade, EditSource, Recipe};
use aura_render::RenderService;
use serde::{Deserialize, Serialize};

/// Where the run's report is stored in the recipe.
pub const KEY: &str = "full_edit_v1";
/// Written into the report; bump on any behavioural change.
pub const VERSION: &str = "full-edit-v1";
/// Neutral shadows this much greener (or more magenta) than neutral midtones get a shadow tint.
pub const SHADOW_TINT_FLOOR: f32 = 0.02;
/// Share of the frame with one primary clipped and the other two not before calibration acts.
pub const PRIMARY_CLIP_FLOOR: f32 = 0.003;
/// A pixel is neutral when its channels differ by at most this share of the brightest, in light.
pub const NEUTRAL_SPREAD: f32 = 0.25;
/// Largest face colour move a grade or a calibration may cause, in rg chromaticity.
pub const FACE_SHIFT_CEILING: f32 = 0.010;

/// What the photographer chose as the look of the photographs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Look {
    /// No look: every stage, the grade included, is measured from this photograph.
    #[default]
    None,
    /// A reference fit follows this edit and decides the colour: no grade here.
    Coming,
    /// An edit profile or a provider already set light and colour: they are kept, and the
    /// calibration and grade are left to it when it set them.
    Applied,
}

/// A request to run the full automatic edit on one photograph.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FullEditInput {
    pub project_id: String,
    pub photo_id: String,
    #[serde(default)]
    pub look: Look,
    /// Someone is watching: send a picture of the photograph before the edit and after every
    /// step that changed it (ADR-0187). Off, nothing extra is rendered.
    #[serde(default)]
    pub preview: bool,
}

/// The ten stages, in the photographer's own words and order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FullStageKey {
    Basic,
    ColourCorrection,
    Calibration,
    ColourGrading,
    CropStraighten,
    Blemishes,
    SkinSmoothing,
    Face,
    Body,
    Advanced,
}

impl FullStageKey {
    pub const ALL: [Self; 10] = [
        Self::Basic,
        Self::ColourCorrection,
        Self::Calibration,
        Self::ColourGrading,
        Self::CropStraighten,
        Self::Blemishes,
        Self::SkinSmoothing,
        Self::Face,
        Self::Body,
        Self::Advanced,
    ];

    #[must_use]
    pub fn number(self) -> u8 {
        Self::ALL
            .iter()
            .position(|s| *s == self)
            .map_or(0, |i| u8::try_from(i + 1).unwrap_or(0))
    }

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Basic => "Basic edit",
            Self::ColourCorrection => "Colour correction",
            Self::Calibration => "Calibration",
            Self::ColourGrading => "Colour grading",
            Self::CropStraighten => "Crop & straighten",
            Self::Blemishes => "Blemishes",
            Self::SkinSmoothing => "Skin smoothing",
            Self::Face => "Face",
            Self::Body => "Body",
            Self::Advanced => "Advanced edit",
        }
    }

    #[must_use]
    pub const fn group(self) -> &'static str {
        match self {
            Self::Basic
            | Self::ColourCorrection
            | Self::Calibration
            | Self::ColourGrading
            | Self::CropStraighten => "Edit",
            Self::Blemishes | Self::SkinSmoothing | Self::Face | Self::Body => "Retouch",
            Self::Advanced => "Advanced",
        }
    }

    /// The stage that does the work of one of the advanced retouch's eighteen.
    #[must_use]
    pub const fn of_advanced(stage: Stage) -> Self {
        match stage {
            Stage::Foundation => Self::Basic,
            Stage::ColourGrade => Self::ColourCorrection,
            Stage::LensPerspective => Self::CropStraighten,
            Stage::SkinCleanup => Self::Blemishes,
            Stage::FrequencySeparation | Stage::SkinColour => Self::SkinSmoothing,
            Stage::MicroDodgeBurn
            | Stage::MediumDodgeBurn
            | Stage::GlobalDodgeBurn
            | Stage::EyesLipsTeeth => Self::Face,
            Stage::Background
            | Stage::Hair
            | Stage::Clothing
            | Stage::Jewellery
            | Stage::BackgroundToning
            | Stage::Grain
            | Stage::OutputSharpening
            | Stage::QualityControl => Self::Advanced,
        }
    }
}

/// One stage of the full edit, as reported to the photographer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullStage {
    pub number: u8,
    pub key: FullStageKey,
    pub title: String,
    pub group: String,
    pub outcome: Outcome,
    /// What was changed.
    pub changes: Vec<String>,
    /// What was measured or why nothing was changed.
    pub checks: Vec<String>,
}

impl FullStage {
    fn new(key: FullStageKey) -> Self {
        Self {
            number: key.number(),
            key,
            title: key.title().into(),
            group: key.group().into(),
            outcome: Outcome::Unchanged,
            changes: Vec::new(),
            checks: Vec::new(),
        }
    }
}

/// The finished recipe and the stage-by-stage report.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullEditDto {
    pub photo_id: String,
    pub recipe: RecipeDto,
    pub stages: Vec<FullStage>,
    pub faces: usize,
    /// Quality control's verdict at QC resolution.
    pub quality_passed: bool,
    pub summary: String,
    /// Wall-clock milliseconds the run took.
    pub millis: u64,
}

/// Progress of one photograph, stage by stage.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullProgress {
    pub photo_id: String,
    pub number: u8,
    pub total: u8,
    pub title: String,
    /// `running`, `done`, or `picture` when it carries [`StagePicture`].
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<StagePicture>,
}

/// The photograph as it stands after one step of the edit, for someone watching it happen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagePicture {
    /// What had just been done: `Original`, or the stage and the step within it.
    pub step: String,
    /// A `data:image/jpeg;base64,` URL.
    pub image: String,
    pub width: u32,
    pub height: u32,
}

/// Long edge of a [`StagePicture`]: large enough to see skin and colour change on a screen,
/// small enough that a render per step costs a fraction of the step.
pub const PICTURE_EDGE: u32 = 960;

/// Render `recipe` as the photographer will see it - crop, effects and sharpening included,
/// nothing skipped - and encode it for the screen.
///
/// # Errors
/// Rendering or encoding failures.
pub fn picture_of(
    state: &AppState,
    photo: PhotoId,
    recipe: &Recipe,
    step: String,
) -> AuraResult<StagePicture> {
    let result = state.render()?.render(aura_render::RenderRequest {
        image_id: photo,
        recipe: recipe.clone(),
        level: aura_render::RenderLevel::Screen(PICTURE_EDGE, PICTURE_EDGE),
        // A slider preview may skip heavy retouch; this picture must show it.
        purpose: aura_render::RenderPurpose::Analysis,
        output: aura_render::OutputSpec {
            colour_space: aura_render::OutputColour::Srgb,
            bit_depth: 8,
            icc: None,
        },
    })?;
    let bytes = match &result.data {
        aura_render::RenderedData::Eight(v) => v.clone(),
        aura_render::RenderedData::Sixteen(v) => v.iter().map(|x| (x >> 8) as u8).collect(),
    };
    let (width, height) = (result.width, result.height);
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 84)
        .encode(&bytes, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|e| aura_core::errors::render::recipe_invalid("picture", &e.to_string()))?;
    Ok(StagePicture {
        step,
        image: format!(
            "data:image/jpeg;base64,{}",
            crate::develop_commands::base64(&jpeg)
        ),
        width,
        height,
    })
}

/// A request for a [`StagePicture`] of a photograph's saved edit as it stands.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PictureInput {
    pub project_id: String,
    pub photo_id: String,
    /// What the picture shows, as the viewer will label it.
    pub step: String,
}

/// The saved edit of a photograph as a [`StagePicture`]: the last picture of a watched edit whose
/// look is matched after the ten stages.
///
/// # Errors
/// Membership, recipe, rendering or encoding failures.
pub fn picture(state: &AppState, input: &PictureInput) -> IpcResult<StagePicture> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let photo = PhotoId::from_db(&input.photo_id).map_err(|_| {
        aura_core::errors::render::recipe_invalid("photo", "Invalid photo identifier")
    })?;
    let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
    Ok(picture_of(state, photo, &recipe, input.step.clone())?)
}

/// The report stored in the recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    version: String,
    look: Look,
    stages: Vec<FullStage>,
}

// ---------------------------------------------------------------------------------------------
// measurements on a rendered sRGB frame
// ---------------------------------------------------------------------------------------------

fn smoothstep(x: f32, a: f32, b: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lin(v: u8) -> f32 {
    aura_raw::colour::curve::srgb_byte_to_linear(v)
}

/// What calibration reads from a frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ColourReadings {
    /// Green excess of near-neutral shadows over near-neutral midtones (positive: greener).
    /// `None` when either has too few pixels to say.
    pub shadow_green: Option<f32>,
    /// The mean weight calibration's shadow tint carries on those shadow pixels.
    pub shadow_weight: f32,
    /// Share of the frame whose red is clipped while green and blue are not.
    pub red_clip: f32,
    /// Share of the frame whose blue is clipped while red and green are not.
    pub blue_clip: f32,
}

/// Median of a sample, `None` when empty.
fn median(mut v: Vec<f32>) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    let mid = v.len() / 2;
    v.select_nth_unstable_by(mid, f32::total_cmp);
    v.get(mid).copied()
}

/// Read a frame for calibration.
///
/// The shadow tint is the **median** green excess of strictly neutral shadow pixels against
/// the same median of neutral midtones, so a dark green garland or a dark red sari - coloured
/// things that happen to be dark - cannot pass for a tint of the light.
#[must_use]
pub fn read_colour(rgb: &[u8]) -> ColourReadings {
    let mut shadow = Vec::new();
    let mut mid = Vec::new();
    let mut weight = 0.0_f64;
    let (mut red, mut blue, mut total) = (0_usize, 0_usize, 0_usize);
    for p in rgb.chunks_exact(3) {
        let &[r, g, b] = p else { continue };
        total += 1;
        if r >= 250 && g < 200 && b < 200 {
            red += 1;
        }
        if b >= 250 && r < 200 && g < 200 {
            blue += 1;
        }
        // Neutral in light, not in bytes: in the shadows a few bytes are a large share of the
        // light, and a byte tolerance lets plainly coloured pixels through.
        let (lr, lg, lb) = (lin(r), lin(g), lin(b));
        let lmax = lr.max(lg).max(lb);
        let lmin = lr.min(lg).min(lb);
        if lmax - lmin > NEUTRAL_SPREAD * lmax {
            continue;
        }
        let rb = f32::midpoint(lr, lb);
        if rb <= 1e-5 {
            continue;
        }
        let excess = lg / rb - 1.0;
        let luma = 0.2126 * lr + 0.7152 * lg + 0.0722 * lb;
        let encoded = aura_raw::colour::curve::srgb_encode(luma);
        if (0.05..0.30).contains(&encoded) {
            shadow.push(excess);
            weight += f64::from(1.0 - smoothstep(encoded, 0.0, 0.5));
        } else if (0.35..0.80).contains(&encoded) {
            mid.push(excess);
        }
    }
    let total_f = total.max(1) as f32;
    let enough = |n: usize| n as f32 >= 0.01 * total_f && n >= 50;
    let ns = shadow.len();
    let shadow_green = if enough(ns) && enough(mid.len()) {
        median(shadow).zip(median(mid)).map(|(s, m)| s - m)
    } else {
        None
    };
    ColourReadings {
        shadow_green,
        shadow_weight: if ns > 0 {
            (weight / ns as f64) as f32
        } else {
            0.0
        },
        red_clip: red as f32 / total_f,
        blue_clip: blue as f32 / total_f,
    }
}

/// What a calibration decided, and why.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CalibrationDecision {
    pub calibration: Calibration,
    pub changes: Vec<String>,
    pub checks: Vec<String>,
}

fn primary_cut(share: f32) -> i16 {
    (-(6.0 + share * 600.0)).round().clamp(-18.0, -6.0) as i16
}

/// Calibration from what a frame measures, starting from neutral so a repeat run decides the
/// same thing. Corrections only: a residual shadow tint, and primaries clipping on their own.
#[must_use]
pub fn decide_calibration(r: &ColourReadings) -> CalibrationDecision {
    let mut d = CalibrationDecision::default();
    match r.shadow_green {
        Some(e) if e.abs() >= SHADOW_TINT_FLOOR => {
            let tint = (400.0 * e / r.shadow_weight.max(0.35))
                .round()
                .clamp(-12.0, 12.0) as i16;
            d.calibration.shadows_tint = tint;
            d.changes.push(format!(
                "Shadows tint {tint:+}: neutral shadows were {:.1}% {} than neutral midtones.",
                e.abs() * 100.0,
                if e > 0.0 { "greener" } else { "more magenta" }
            ));
        }
        Some(e) => d.checks.push(format!(
            "Neutral shadows match the neutral midtones within {:.1}%: no shadow tint needed.",
            e.abs() * 100.0
        )),
        None => d.checks.push(
            "Too few neutral shadows or midtones to measure a shadow tint; none applied.".into(),
        ),
    }
    if r.red_clip >= PRIMARY_CLIP_FLOOR {
        let cut = primary_cut(r.red_clip);
        d.calibration.red_saturation = cut;
        d.changes.push(format!(
            "Red primary saturation {cut}: {:.1}% of the frame had red clipped while green and blue were not, so saturated reds lost their detail.",
            r.red_clip * 100.0
        ));
    } else {
        d.checks.push(format!(
            "Saturated reds keep their detail ({:.2}% clipped on red alone).",
            r.red_clip * 100.0
        ));
    }
    if r.blue_clip >= PRIMARY_CLIP_FLOOR {
        let cut = primary_cut(r.blue_clip);
        d.calibration.blue_saturation = cut;
        d.changes.push(format!(
            "Blue primary saturation {cut}: {:.1}% of the frame had blue clipped while red and green were not.",
            r.blue_clip * 100.0
        ));
    } else {
        d.checks.push(format!(
            "Saturated blues keep their detail ({:.2}% clipped on blue alone).",
            r.blue_clip * 100.0
        ));
    }
    d
}

/// What colour grading reads from a frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ToneReadings {
    /// Mean encoded luminance, `0..1`.
    pub mean_luma: f32,
    /// Mean HSV saturation.
    pub mean_saturation: f32,
    /// Share of pixels with HSV saturation above 0.6 and value above 0.2.
    pub colourful_share: f32,
    /// Mean encoded colour of the highlights (encoded luminance above 0.7), when 2% or more.
    pub highlights: Option<[f32; 3]>,
    /// Mean encoded colour of the shadows (encoded luminance under 0.3), when 2% or more.
    pub shadows: Option<[f32; 3]>,
}

/// HSV hue in degrees and saturation of an encoded colour.
#[must_use]
pub fn hue_saturation(c: [f32; 3]) -> (f32, f32) {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let delta = max - min;
    if max <= 1e-6 || delta <= 1e-6 {
        return (0.0, 0.0);
    }
    let hue = if (max - c[0]).abs() < f32::EPSILON {
        60.0 * ((c[1] - c[2]) / delta).rem_euclid(6.0)
    } else if (max - c[1]).abs() < f32::EPSILON {
        60.0 * ((c[2] - c[0]) / delta + 2.0)
    } else {
        60.0 * ((c[0] - c[1]) / delta + 4.0)
    };
    (hue.rem_euclid(360.0), delta / max)
}

/// Read a frame for colour grading.
#[must_use]
pub fn read_tones(rgb: &[u8]) -> ToneReadings {
    let (mut luma, mut sat, mut colourful, mut n) = (0.0_f64, 0.0_f64, 0_usize, 0_usize);
    let (mut hi, mut lo) = ([0.0_f64; 3], [0.0_f64; 3]);
    let (mut nh, mut nl) = (0_usize, 0_usize);
    for p in rgb.chunks_exact(3) {
        let &[r, g, b] = p else { continue };
        let c = [
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
        ];
        let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let (_, s) = hue_saturation(c);
        let v = c[0].max(c[1]).max(c[2]);
        luma += f64::from(y);
        sat += f64::from(s);
        if s > 0.6 && v > 0.2 {
            colourful += 1;
        }
        if y > 0.7 {
            for (a, x) in hi.iter_mut().zip(c) {
                *a += f64::from(x);
            }
            nh += 1;
        } else if y < 0.3 {
            for (a, x) in lo.iter_mut().zip(c) {
                *a += f64::from(x);
            }
            nl += 1;
        }
        n += 1;
    }
    let nf = n.max(1) as f64;
    let mean = |a: [f64; 3], k: usize| {
        (k as f64 >= 0.02 * nf && k > 0).then(|| a.map(|v| (v / k as f64) as f32))
    };
    ToneReadings {
        mean_luma: (luma / nf) as f32,
        mean_saturation: (sat / nf) as f32,
        colourful_share: colourful as f32 / nf as f32,
        highlights: mean(hi, nh),
        shadows: mean(lo, nl),
    }
}

/// What a grade decided, and why.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GradeDecision {
    pub grade: ColourGrade,
    pub changes: Vec<String>,
    pub checks: Vec<String>,
}

/// A restrained split tone in the frame's own light, starting from neutral.
///
/// The highlights take the colour of the frame's own highlights when that is a warm light,
/// and a warm light's colour otherwise; the shadows take its complement, except with people
/// in the frame, where the shadows stay untinted so no complexion turns cool. A black and
/// white frame, a frame already full of strong colour and a night scene keep their colour.
#[must_use]
pub fn decide_grade(t: &ToneReadings, people: bool) -> GradeDecision {
    let mut d = GradeDecision::default();
    if t.mean_saturation < 0.03 {
        d.checks
            .push("Black and white or nearly so: a colour grade would only tint it.".into());
        return d;
    }
    if t.colourful_share > 0.35 {
        d.checks.push(format!(
            "{:.0}% of the frame is already strongly coloured: that colour is the scene's own and is not graded.",
            t.colourful_share * 100.0
        ));
        return d;
    }
    if t.mean_luma < 0.12 {
        d.checks
            .push("A night or low-key frame keeps the colour of its own light.".into());
        return d;
    }
    let mut hue = 40.0_f32;
    match t.highlights.map(hue_saturation) {
        Some((_, s)) if s > 0.15 => d.checks.push(format!(
            "Highlights already carry colour ({:.0}% saturation): left as they are.",
            s * 100.0
        )),
        Some((h, s)) => {
            if (15.0..=75.0).contains(&h) && s >= 0.02 {
                hue = h;
            }
            let amount: i16 = if people { 5 } else { 6 };
            d.grade.highlights.hue = hue.round() as i16 % 360;
            d.grade.highlights.saturation = amount;
            d.changes.push(format!(
                "Highlights warmed toward {} light: hue {:.0}°, {amount}%.",
                if (hue - 40.0).abs() > f32::EPSILON {
                    "the frame's own"
                } else {
                    "a warm"
                },
                hue
            ));
        }
        None => d
            .checks
            .push("Too few highlights to grade; highlights left neutral.".into()),
    }
    if people {
        d.checks
            .push("People in the frame: shadows left untinted so no complexion turns cool.".into());
    } else {
        match t.shadows.map(hue_saturation) {
            Some((_, s)) if s > 0.15 => d.checks.push(format!(
                "Shadows already carry colour ({:.0}% saturation): left as they are.",
                s * 100.0
            )),
            Some(_) => {
                let shade = ((hue + 180.0).round() as i16).rem_euclid(360);
                d.grade.shadows.hue = shade;
                d.grade.shadows.saturation = 4;
                d.changes.push(format!(
                    "Shadows given the complementary cool: hue {shade}°, 4%."
                ));
            }
            None => d
                .checks
                .push("Too few shadows to grade; shadows left neutral.".into()),
        }
    }
    d
}

/// Mean rg chromaticity inside the central part of each face box.
#[must_use]
pub fn face_chroma(rgb: &[u8], w: usize, h: usize, faces: &[[f32; 4]]) -> Option<[f32; 2]> {
    let (mut r, mut g, mut n) = (0.0_f64, 0.0_f64, 0_usize);
    for &[left, top, right, bottom] in faces {
        let (cx, cy) = (f32::midpoint(left, right), f32::midpoint(top, bottom));
        let (hw, hh) = ((right - left) * 0.3, (bottom - top) * 0.3);
        let x0 = ((cx - hw).clamp(0.0, 1.0) * w as f32) as usize;
        let x1 = ((cx + hw).clamp(0.0, 1.0) * w as f32) as usize;
        let y0 = ((cy - hh).clamp(0.0, 1.0) * h as f32) as usize;
        let y1 = ((cy + hh).clamp(0.0, 1.0) * h as f32) as usize;
        for y in y0..y1.min(h) {
            for x in x0..x1.min(w) {
                let i = (y * w + x) * 3;
                let Some(&[pr, pg, pb]) = rgb.get(i..i + 3) else {
                    continue;
                };
                let sum = u32::from(pr) + u32::from(pg) + u32::from(pb);
                if sum < 30 {
                    continue;
                }
                r += f64::from(pr) / f64::from(sum);
                g += f64::from(pg) / f64::from(sum);
                n += 1;
            }
        }
    }
    (n >= 20).then(|| [(r / n as f64) as f32, (g / n as f64) as f32])
}

fn chroma_shift(a: Option<[f32; 2]>, b: Option<[f32; 2]>) -> Option<f32> {
    let (a, b) = a.zip(b)?;
    Some((a[0] - b[0]).hypot(a[1] - b[1]))
}

// ---------------------------------------------------------------------------------------------
// classifying what the retouch saved
// ---------------------------------------------------------------------------------------------

/// The stage an automatic retouch operation belongs to; `None` for a person's own operation.
#[must_use]
pub fn category_of(id: &str) -> Option<FullStageKey> {
    if id.starts_with(SCENE_PREFIX) {
        return Some(FullStageKey::Advanced);
    }
    let rest = id.strip_prefix(PREFIX)?;
    let name = rest
        .split_once('-')
        .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        .map_or(rest, |(_, n)| n);
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| name.starts_with(p));
    Some(if name.starts_with("body-") {
        FullStageKey::Body
    } else if matches!(name, "clear" | "heal") || name.starts_with("spot") {
        FullStageKey::Blemishes
    } else if matches!(
        name,
        "texture" | "pores" | "surface-finish" | "texture-graft" | "tone" | "colour"
    ) || starts(&["lines-", "redness-"])
    {
        FullStageKey::SkinSmoothing
    } else if matches!(
        name,
        "microdb" | "light" | "shine" | "brightness" | "facelight" | "glow" | "teeth"
    ) || starts(&["fold-", "undereye", "sculpt-", "eye-", "lips-", "makeup-"])
    {
        FullStageKey::Face
    } else {
        FullStageKey::Advanced
    })
}

/// A readable name for an automatic operation: `auto-portrait-v1-0-eye-left` is `eye left`.
fn op_name(id: &str) -> String {
    let rest = id
        .strip_prefix(PREFIX)
        .or_else(|| id.strip_prefix(SCENE_PREFIX))
        .unwrap_or(id);
    let name = rest
        .split_once('-')
        .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        .map_or(rest, |(_, n)| n);
    let base: String = name
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == '-')
        .replace('-', " ");
    if base.is_empty() {
        name.replace('-', " ")
    } else {
        base
    }
}

/// "12 spot, 2 texture, eye left" - each distinct name with its count, most first.
fn summarise_ops(names: &[String]) -> String {
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for n in names {
        *counts.entry(n.as_str()).or_default() += 1;
    }
    let mut rows: Vec<(&str, usize)> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    rows.iter()
        .take(8)
        .map(|(n, c)| {
            if *c > 1 {
                format!("{c} × {n}")
            } else {
                (*n).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn auto_ops(recipe: &Recipe, key: FullStageKey) -> AuraResult<Vec<retouch_tools::Edit>> {
    Ok(retouch_tools::read(recipe)?
        .into_iter()
        .filter(|e| e.enabled && category_of(&e.id) == Some(key))
        .collect())
}

fn same_ops(a: &[retouch_tools::Edit], b: &[retouch_tools::Edit]) -> bool {
    match (serde_json::to_string(a), serde_json::to_string(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------
// describing the global changes
// ---------------------------------------------------------------------------------------------

fn int_change(out: &mut Vec<String>, label: &str, a: i16, b: i16) {
    if a != b {
        out.push(format!("{label} {b:+}"));
    }
}

fn basic_changes(before: &Recipe, after: &Recipe) -> Vec<String> {
    let (a, b) = (&before.global, &after.global);
    let mut out = Vec::new();
    if (a.exposure - b.exposure).abs() > 0.004 {
        out.push(format!("Exposure {:+.2} EV", b.exposure));
    }
    int_change(&mut out, "Contrast", a.contrast, b.contrast);
    int_change(&mut out, "Highlights", a.highlights, b.highlights);
    int_change(&mut out, "Shadows", a.shadows, b.shadows);
    int_change(&mut out, "Whites", a.whites, b.whites);
    int_change(&mut out, "Blacks", a.blacks, b.blacks);
    int_change(&mut out, "Clarity", a.clarity, b.clarity);
    int_change(&mut out, "Dehaze", a.dehaze, b.dehaze);
    if a.noise.luminance != b.noise.luminance || a.noise.colour != b.noise.colour {
        out.push(format!(
            "Noise reduction {} luminance, {} colour",
            b.noise.luminance, b.noise.colour
        ));
    }
    let curves = |r: &Recipe| {
        serde_json::to_string(&(
            &r.global.curve,
            &r.global.channel_curves,
            &r.global.parametric,
        ))
        .unwrap_or_default()
    };
    if curves(before) != curves(after) {
        out.push("Black and white points set on every channel (levels)".into());
    }
    out
}

fn colour_changes(before: &Recipe, after: &Recipe) -> Vec<String> {
    let (a, b) = (&before.global, &after.global);
    let mut out = Vec::new();
    if a.temperature != b.temperature {
        out.push(format!("White balance {} K", b.temperature));
    }
    int_change(&mut out, "Tint", a.tint, b.tint);
    int_change(&mut out, "Vibrance", a.vibrance, b.vibrance);
    int_change(&mut out, "Saturation", a.saturation, b.saturation);
    if serde_json::to_string(&a.hsl).ok() != serde_json::to_string(&b.hsl).ok() {
        out.push("Colour bands adjusted (HSL)".into());
    }
    out
}

fn finish_changes(before: &Recipe, after: &Recipe) -> Vec<String> {
    let (a, b) = (&before.global, &after.global);
    let mut out = Vec::new();
    if a.effects.grain.amount != b.effects.grain.amount {
        out.push(format!("Grain {}", b.effects.grain.amount));
    }
    if a.sharpen.amount != b.sharpen.amount || a.sharpen.masking != b.sharpen.masking {
        out.push(format!(
            "Output sharpening {} (masking {})",
            b.sharpen.amount, b.sharpen.masking
        ));
    }
    out
}

fn reports_for(
    report: &advanced_retouch::Report,
    key: FullStageKey,
) -> Vec<&advanced_retouch::StageReport> {
    report
        .stages
        .iter()
        .filter(|r| FullStageKey::of_advanced(r.stage) == key)
        .collect()
}

/// The stage's outcome from the advanced stages that did its work, given whether anything of
/// it changed in the recipe.
fn outcome_from(reports: &[&advanced_retouch::StageReport], changed: bool) -> Outcome {
    if changed {
        Outcome::Applied
    } else if reports.iter().any(|r| r.outcome == Outcome::Protected) {
        Outcome::Protected
    } else if !reports.is_empty() && reports.iter().all(|r| r.outcome == Outcome::NotApplicable) {
        Outcome::NotApplicable
    } else {
        Outcome::Unchanged
    }
}

fn first_lines<'a>(
    reports: &[&'a advanced_retouch::StageReport],
    pick: impl Fn(&'a advanced_retouch::StageReport) -> &'a Vec<String>,
    limit: usize,
) -> Vec<String> {
    reports
        .iter()
        .flat_map(|r| pick(r).iter())
        .filter(|s| !s.trim().is_empty())
        .take(limit)
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------------------------
// the run
// ---------------------------------------------------------------------------------------------

fn neutral_calibration(c: &Calibration) -> bool {
    *c == Calibration::default()
}

fn neutral_grade(g: &ColourGrade) -> bool {
    *g == ColourGrade::default()
}

/// Save `proposal` over `current` as an automatic step; the values a person set are kept.
fn save_step(
    state: &AppState,
    project: &ProjectId,
    photo: &PhotoId,
    current: &mut Recipe,
    mut proposal: Recipe,
    label: &str,
) -> AuraResult<(bool, Vec<String>)> {
    proposal.provenance.source = EditSource::Ai;
    proposal.provenance.confidence = 0.35;
    let (merged, changes) = schema::merge(current, &proposal, EditSource::Ai)?;
    schema::Validation::check(&merged)?;
    if changes.changed.is_empty()
        || aura_recipe::recipe_hash(&merged)? == aura_recipe::recipe_hash(current)?
    {
        return Ok((false, changes.refused));
    }
    state
        .recipe_store()
        .save(project, photo, &merged, &changes.changed, label)?;
    *current = merged;
    Ok((true, changes.refused))
}

/// A rendered frame: sRGB bytes, width, height.
type Frame = (Vec<u8>, usize, usize);

/// Calibration and grade are measured and checked at this size: enough for faces and
/// clipping, and a quarter of the pixels quality control renders.
const MEASURE_LEVEL: aura_render::RenderLevel = aura_render::RenderLevel::Screen(960, 960);

fn measure(state: &AppState, photo: PhotoId, recipe: &Recipe) -> AuraResult<Frame> {
    advanced_retouch::qc_render_at(state, photo, recipe, MEASURE_LEVEL)
}

/// Stage 3. `base` is the photograph with no automatic calibration or grade and `shown` its
/// render. Returns the stage and, when it saved a calibration, the render of the calibrated
/// photograph (still without a grade) for stage 4 to start from.
fn calibrate(
    state: &AppState,
    ids: (&ProjectId, PhotoId),
    current: &mut Recipe,
    base: &Recipe,
    shown: &Frame,
    faces: &[[f32; 4]],
) -> (FullStage, Option<Frame>) {
    let (project, photo) = ids;
    let mut stage = FullStage::new(FullStageKey::Calibration);
    let readings = read_colour(&shown.0);
    let mut decision = decide_calibration(&readings);
    stage.checks.append(&mut decision.checks);
    let mut wanted = decision.calibration;
    let mut checked: Option<(Calibration, Frame)> = None;
    if wanted != Calibration::default() {
        // Checked on the render of what it proposes: each part must reduce what it measured,
        // and the faces must keep their colour.
        let before_face = face_chroma(&shown.0, shown.1, shown.2, faces);
        for attempt in 0..2 {
            let mut proposal = base.clone();
            proposal.global.calibration = wanted;
            let Ok(frame) = measure(state, photo, &proposal) else {
                break;
            };
            let after = read_colour(&frame.0);
            if wanted.shadows_tint != 0
                && !matches!((readings.shadow_green, after.shadow_green), (Some(a), Some(b)) if b.abs() < a.abs())
            {
                stage.checks.push("A shadow tint did not bring the shadows closer to the midtones on the render; withdrawn.".into());
                wanted.shadows_tint = 0;
                decision.changes.retain(|c| !c.starts_with("Shadows tint"));
            }
            if wanted.red_saturation != 0 && after.red_clip >= readings.red_clip * 0.9 {
                stage.checks.push("Lowering the red primary did not recover the clipped reds on the render; withdrawn.".into());
                wanted.red_saturation = 0;
                decision.changes.retain(|c| !c.starts_with("Red primary"));
            }
            if wanted.blue_saturation != 0 && after.blue_clip >= readings.blue_clip * 0.9 {
                stage.checks.push("Lowering the blue primary did not recover the clipped blues on the render; withdrawn.".into());
                wanted.blue_saturation = 0;
                decision.changes.retain(|c| !c.starts_with("Blue primary"));
            }
            let moved = chroma_shift(before_face, face_chroma(&frame.0, frame.1, frame.2, faces));
            checked = Some((proposal.global.calibration, frame));
            match moved {
                Some(m) if m > FACE_SHIFT_CEILING && attempt == 0 => {
                    stage.checks.push(format!(
                        "Faces moved {m:.3} in colour (limit {FACE_SHIFT_CEILING}); calibration halved and checked again."
                    ));
                    wanted.shadows_tint /= 2;
                    wanted.red_saturation /= 2;
                    wanted.blue_saturation /= 2;
                    for c in &mut decision.changes {
                        c.push_str(" Halved to keep the faces' colour.");
                    }
                }
                Some(m) if m > FACE_SHIFT_CEILING => {
                    stage.checks.push(format!(
                        "Faces still moved {m:.3} in colour; calibration withdrawn."
                    ));
                    wanted = Calibration::default();
                    decision.changes.clear();
                    break;
                }
                Some(m) => {
                    stage
                        .checks
                        .push(format!("Faces kept their colour (moved {m:.3})."));
                    break;
                }
                None => break,
            }
        }
    }
    let mut proposal = current.clone();
    proposal.global.calibration = wanted;
    let label = format!(
        "Full auto edit 3/10 · Calibration: {}",
        decision
            .changes
            .first()
            .cloned()
            .unwrap_or_else(|| "neutral".into())
    );
    record(
        &mut stage,
        save_step(state, project, &photo, current, proposal, &label),
        decision.changes,
        "Calibration",
    );
    // The render stage 4 starts from, when it is the calibration that was kept.
    let frame = checked
        .filter(|(c, _)| *c == wanted && wanted != Calibration::default())
        .map(|(_, f)| f);
    (stage, frame)
}

/// What a save meant for the stage's outcome.
fn record(
    stage: &mut FullStage,
    saved: AuraResult<(bool, Vec<String>)>,
    changes: Vec<String>,
    what: &str,
) {
    match saved {
        Ok((true, _)) => {
            stage.outcome = Outcome::Applied;
            stage.changes = changes;
        }
        Ok((false, refused)) if !refused.is_empty() => {
            stage.outcome = Outcome::Protected;
            stage
                .checks
                .push(format!("Kept your own settings: {}.", refused.join(", ")));
        }
        Ok((false, _)) => {
            if !changes.is_empty() {
                stage.changes = changes;
                stage
                    .checks
                    .push("Already in place from an earlier run; nothing new to save.".into());
            }
        }
        Err(error) => stage.checks.push(format!(
            "{what} could not be saved ({}); left as it was.",
            error.code
        )),
    }
}

/// Stage 4. `base` is the calibrated photograph without a grade and `shown` its render.
fn grade(
    state: &AppState,
    ids: (&ProjectId, PhotoId),
    current: &mut Recipe,
    base: &Recipe,
    shown: &Frame,
    faces: &[[f32; 4]],
    people: bool,
) -> FullStage {
    let (project, photo) = ids;
    let mut stage = FullStage::new(FullStageKey::ColourGrading);
    let readings = read_tones(&shown.0);
    let mut decision = decide_grade(&readings, people);
    stage.checks.append(&mut decision.checks);
    let mut wanted = decision.grade;
    if !neutral_grade(&wanted) {
        let before_face = face_chroma(&shown.0, shown.1, shown.2, faces);
        let before_clip = advanced_retouch::measure::clipped(&shown.0);
        for attempt in 0..2 {
            let mut proposal = base.clone();
            proposal.global.colour_grade = wanted;
            let Ok((pixels, w, h)) = measure(state, photo, &proposal) else {
                break;
            };
            let clip = advanced_retouch::measure::clipped(&pixels);
            let moved = chroma_shift(before_face, face_chroma(&pixels, w, h, faces));
            let too_far = moved.is_some_and(|m| m > FACE_SHIFT_CEILING);
            let clipped_more = clip > before_clip + 0.001;
            if !too_far && !clipped_more {
                stage.checks.push(match moved {
                    Some(m) => format!("Checked on the render: faces moved {m:.3} in colour (limit {FACE_SHIFT_CEILING}); no new clipping."),
                    None => "Checked on the render: no new clipping.".into(),
                });
                break;
            }
            if attempt == 0 {
                stage.checks.push(format!(
                    "The grade {}; halved and checked again.",
                    if too_far {
                        format!("moved the faces' colour {:.3}", moved.unwrap_or(0.0))
                    } else {
                        "clipped more of the frame".into()
                    }
                ));
                wanted.highlights.saturation /= 2;
                wanted.shadows.saturation /= 2;
                for c in &mut decision.changes {
                    c.push_str(" Halved after checking the render.");
                }
            } else {
                stage
                    .checks
                    .push("The halved grade still moved the faces or clipped; withdrawn.".into());
                wanted = ColourGrade::default();
                decision.changes.clear();
            }
        }
    }
    let mut proposal = current.clone();
    proposal.global.colour_grade = wanted;
    let label = format!(
        "Full auto edit 4/10 · Colour grading: {}",
        decision
            .changes
            .first()
            .cloned()
            .unwrap_or_else(|| "neutral".into())
    );
    record(
        &mut stage,
        save_step(state, project, &photo, current, proposal, &label),
        decision.changes,
        "The grade",
    );
    stage
}

/// Assemble stages 1, 2 and 5 to 10 from what the advanced retouch saved.
fn retouch_stages(
    before: &Recipe,
    after: &Recipe,
    report: &advanced_retouch::Report,
    look: Look,
    people: bool,
) -> AuraResult<Vec<FullStage>> {
    let mut out = Vec::new();
    for key in [FullStageKey::Basic, FullStageKey::ColourCorrection] {
        let mut stage = FullStage::new(key);
        let reports = reports_for(report, key);
        let mut changes = if key == FullStageKey::Basic {
            basic_changes(before, after)
        } else {
            colour_changes(before, after)
        };
        if key == FullStageKey::ColourCorrection {
            // Colour fringing is corrected with the lens.
            changes.extend(
                reports_for(report, FullStageKey::CropStraighten)
                    .iter()
                    .flat_map(|r| r.changes.iter())
                    .filter(|c| c.to_lowercase().contains("fring"))
                    .cloned(),
            );
        }
        if look == Look::Applied {
            stage.outcome = Outcome::Protected;
            stage
                .checks
                .push("Light and colour kept as your chosen look set them.".into());
        } else {
            stage.outcome = outcome_from(&reports, !changes.is_empty());
            stage.checks = first_lines(&reports, |r| &r.checks, 4);
        }
        stage.changes = changes;
        out.push(stage);
    }
    {
        let mut stage = FullStage::new(FullStageKey::CropStraighten);
        let reports = reports_for(report, FullStageKey::CropStraighten);
        let (a, b) = (&before.geometry, &after.geometry);
        let moved = (a.rotate - b.rotate).abs() > 0.004
            || a.crop
                .iter()
                .zip(b.crop)
                .any(|(x, y)| (x - y).abs() > 0.0005);
        if moved {
            stage.changes.push(format!(
                "Levelled by {:+.2}° and cropped to {:.0}% × {:.0}% of the frame so no corner is empty.",
                b.rotate,
                (b.crop[2] - b.crop[0]) * 100.0,
                (b.crop[3] - b.crop[1]) * 100.0
            ));
        }
        stage.outcome = outcome_from(&reports, moved);
        stage.checks = reports
            .iter()
            .flat_map(|r| r.checks.iter().chain(r.changes.iter()))
            .filter(|c| !c.to_lowercase().contains("fring"))
            .take(4)
            .cloned()
            .collect();
        out.push(stage);
    }
    for key in [
        FullStageKey::Blemishes,
        FullStageKey::SkinSmoothing,
        FullStageKey::Face,
        FullStageKey::Body,
        FullStageKey::Advanced,
    ] {
        let mut stage = FullStage::new(key);
        let reports = reports_for(report, key);
        let now = auto_ops(after, key)?;
        let was = auto_ops(before, key)?;
        let names: Vec<String> = now.iter().map(|e| op_name(&e.id)).collect();
        let ops_changed = !same_ops(&now, &was);
        let mut globals = Vec::new();
        if key == FullStageKey::Advanced {
            globals = finish_changes(before, after);
        }
        let changed = (ops_changed && !now.is_empty()) || !globals.is_empty();
        if !now.is_empty() {
            stage.changes.push(format!(
                "{} operation(s): {}.",
                now.len(),
                summarise_ops(&names)
            ));
        }
        stage.changes.extend(globals);
        if key != FullStageKey::Body {
            stage
                .changes
                .extend(first_lines(&reports, |r| &r.changes, 3));
        }
        stage.outcome = if key == FullStageKey::Body {
            if changed {
                Outcome::Applied
            } else if !people {
                Outcome::NotApplicable
            } else {
                Outcome::Unchanged
            }
        } else {
            outcome_from(&reports, changed)
        };
        if !people && key != FullStageKey::Advanced {
            stage.outcome = if changed {
                Outcome::Applied
            } else {
                Outcome::NotApplicable
            };
            stage
                .checks
                .push("No face was found in the photograph (nobody in it, or faces turned away or too small), so face retouch does not apply.".into());
        } else if key == FullStageKey::Body {
            if now.is_empty() {
                stage.checks.push("No visible body skin below the faces matched the person's own skin closely enough to retouch.".into());
            } else {
                stage.checks.push("Body skin found below each face by matching that person's own face skin; the face itself is excluded.".into());
            }
        } else {
            stage.checks = first_lines(&reports, |r| &r.checks, 4);
        }
        if !changed && !now.is_empty() {
            stage
                .checks
                .push("Already in place from an earlier run; nothing new to save.".into());
        }
        if key == FullStageKey::Advanced {
            stage.checks.push(report.summary.clone());
        }
        out.push(stage);
    }
    Ok(out)
}

/// The retouch the full edit applies when the photographer has chosen none: the product's
/// calibrated automatic finish - the same one Auto enhance applies - with the advanced stages'
/// hair, clothing and backdrop clean-up, which that finish leaves to them.
#[must_use]
pub fn default_options() -> crate::portrait_features::Options {
    let mut options = crate::internal_ai::fresh_skin_options(None);
    let advanced = advanced_retouch::options().settings;
    options.settings.hair_detail = advanced.hair_detail;
    options.settings.hair_shine = advanced.hair_shine;
    options.settings.fabric = advanced.fabric;
    options.settings.backdrop = advanced.backdrop;
    options
}

/// This photo's retouch choices: what the photographer saved for it, which always win, or
/// [`default_options`].
#[must_use]
pub fn options_for(base: &Recipe) -> crate::portrait_features::Options {
    if crate::internal_ai::has_custom_skin_preferences(base) {
        base.extra
            .get(crate::portrait_auto::KEY)
            .and_then(|report| report.get("options"))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_else(default_options)
    } else {
        default_options()
    }
}

/// What an outcome is called in a report.
#[must_use]
pub const fn outcome_label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Applied => "applied",
        Outcome::Unchanged => "nothing needed",
        Outcome::NotApplicable => "does not apply",
        Outcome::Protected => "kept your setting",
    }
}

/// One line per stage, for a run's log: "3 Calibration - applied: Shadows tint +6: ...".
#[must_use]
pub fn report_lines(dto: &FullEditDto) -> Vec<String> {
    let mut lines: Vec<String> = dto
        .stages
        .iter()
        .map(|s| {
            let said = s.changes.first().or(s.checks.first());
            format!(
                "{} {} - {}{}",
                s.number,
                s.title,
                outcome_label(s.outcome),
                said.map(|t| format!(": {t}")).unwrap_or_default()
            )
        })
        .collect();
    lines.push(dto.summary.clone());
    lines
}

/// Run the full automatic edit on one photograph: ten stages, in order, every one reported.
///
/// # Errors
/// Membership, decode, analysis, rendering or storage failures of the retouch stages. The
/// calibration and grade never fail the photograph: what they could not measure they leave.
pub fn run(
    state: &AppState,
    input: &FullEditInput,
    progress: &dyn Fn(FullProgress),
) -> IpcResult<FullEditDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let started = state.clock().monotonic_ms();
    let invalid = |message: &str| aura_core::errors::render::recipe_invalid("photo", message);
    let photo =
        PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo identifier"))?;
    let project =
        ProjectId::from_db(&input.project_id).map_err(|_| invalid("Invalid project identifier"))?;
    let total = u8::try_from(FullStageKey::ALL.len()).unwrap_or(10);
    // The status bar names the photograph and the stage (ADR-0188).
    let busy = crate::activity::begin(
        state,
        "edit",
        format!(
            "Full edit: {}",
            crate::activity::photo_name(state, &input.photo_id)
        ),
    );
    busy.progress(0, u64::from(total));
    let tell = |key: FullStageKey, running: bool| {
        let number = key.number();
        busy.detail(format!(
            "Stage {number} of {total}: {}{}",
            key.title(),
            if running { "" } else { " - done" }
        ));
        busy.progress(
            u64::from(if running {
                number.saturating_sub(1)
            } else {
                number
            }),
            u64::from(total),
        );
        progress(FullProgress {
            photo_id: input.photo_id.clone(),
            number: key.number(),
            total,
            title: key.title().into(),
            state: if running { "running" } else { "done" }.into(),
            picture: None,
        });
    };
    // A picture after every step that changed the photograph, only while someone watches. A
    // picture that cannot be rendered is left out: the edit itself does not depend on it.
    let show = |key: Option<FullStageKey>, step: String, recipe: &Recipe| {
        if !input.preview {
            return;
        }
        let _time = crate::perf::step(state, "full_edit.picture");
        match picture_of(state, photo, recipe, step) {
            Ok(picture) => progress(FullProgress {
                photo_id: input.photo_id.clone(),
                number: key.map_or(0, FullStageKey::number),
                total,
                title: key.map_or("Original", FullStageKey::title).into(),
                state: "picture".into(),
                picture: Some(picture),
            }),
            Err(error) => {
                tracing::warn!(target: "retouch", code = %error.code, "no picture of the full edit step");
            }
        }
    };
    let before = crate::develop_commands::load_or_neutral(state, photo)?;
    show(None, "Original".into(), &before);
    // The applied steps of the stage running now, pictured when the next stage starts.
    let pending = std::cell::RefCell::new(None::<(FullStageKey, Vec<String>)>);
    let flush = || {
        let Some((key, steps)) = pending.borrow_mut().take() else {
            return;
        };
        if let Ok(now) = crate::develop_commands::load_or_neutral(state, photo) {
            let named: Vec<String> = steps.into_iter().filter(|s| s != key.title()).collect();
            let step = if named.is_empty() {
                key.title().to_owned()
            } else {
                format!("{} · {}", key.title(), named.join(", "))
            };
            show(Some(key), step, &now);
        }
    };

    // Stages 1, 2 and 5 to 10: the advanced retouch, face and body.
    let retouching = crate::perf::step(state, "full_edit.advanced");
    let advanced = advanced_retouch::run_keeping(
        state,
        &AdvancedRetouchInput {
            project_id: input.project_id.clone(),
            photo_id: input.photo_id.clone(),
            options: Some(options_for(&before)),
            preset: None,
        },
        input.look == Look::Applied,
        &|p| {
            let key = Stage::ALL
                .get(usize::from(p.number.saturating_sub(1)))
                .map_or(FullStageKey::Advanced, |s| FullStageKey::of_advanced(*s));
            // Steps of one stage that run back to back share one picture: each picture renders
            // the whole retouch again, and three dodge-and-burn passes read as one change.
            if pending.borrow().as_ref().is_some_and(|(k, _)| *k != key) {
                flush();
            }
            tell(key, p.state == "running");
            if p.state == "done" && p.outcome == Some(Outcome::Applied) {
                let mut held = pending.borrow_mut();
                match held.as_mut() {
                    Some((_, steps)) => steps.push(p.title.clone()),
                    None => *held = Some((key, vec![p.title.clone()])),
                }
            }
        },
    )?;
    flush();
    drop(retouching);
    let mut current = crate::develop_commands::load_or_neutral(state, photo)?;
    let after_retouch = current.clone();

    // The faces the calibration and grade check, from the same preview the retouch read.
    let faces: Vec<[f32; 4]> = state
        .previews(&input.project_id)
        .ok()
        .and_then(|previews| {
            previews
                .get(
                    photo,
                    aura_raw::PixelLevel::Thumb(512),
                    Priority::Interactive,
                )
                .ok()
        })
        .and_then(|thumb| {
            thumb
                .as_srgb8()
                .and_then(|rgb| aura_vision::portrait::detect(rgb, thumb.width, thumb.height).ok())
        })
        .unwrap_or_default()
        .into_iter()
        .map(|f| f.bounds)
        .collect();
    let people = advanced.report.faces > 0 || !faces.is_empty();

    // Stages 3 and 4 start from the photograph without any automatic calibration or grade, so
    // a repeat run decides the same thing; a look's own calibration or grade is kept.
    tell(FullStageKey::Calibration, true);
    let calibrating = crate::perf::step(state, "full_edit.calibration");
    let ids = (&project, photo);
    let keep_calibration =
        input.look == Look::Applied && !neutral_calibration(&current.global.calibration);
    let mut base = current.clone();
    if !keep_calibration {
        base.global.calibration = Calibration::default();
    }
    if input.look == Look::None {
        base.global.colour_grade = ColourGrade::default();
    }
    let shown = measure(state, photo, &base);
    let (calibration, calibrated) = if keep_calibration {
        let mut stage = FullStage::new(FullStageKey::Calibration);
        stage.outcome = Outcome::Protected;
        stage
            .checks
            .push("Your chosen look sets its own calibration; it was kept.".into());
        (stage, None)
    } else {
        match &shown {
            Ok(frame) => calibrate(state, ids, &mut current, &base, frame, &faces),
            Err(error) => {
                let mut stage = FullStage::new(FullStageKey::Calibration);
                stage.checks.push(format!(
                    "The photograph could not be rendered to measure its colour ({}); calibration left as it was.",
                    error.code
                ));
                (stage, None)
            }
        }
    };
    drop(calibrating);
    if calibration.outcome == Outcome::Applied {
        show(
            Some(FullStageKey::Calibration),
            FullStageKey::Calibration.title().into(),
            &current,
        );
    }
    tell(FullStageKey::Calibration, false);
    tell(FullStageKey::ColourGrading, true);
    let grading = crate::perf::step(state, "full_edit.grade");
    let graded = if input.look == Look::None {
        let mut graded_base = current.clone();
        graded_base.global.colour_grade = ColourGrade::default();
        match calibrated.map(Ok).unwrap_or(shown) {
            Ok(frame) => grade(
                state,
                ids,
                &mut current,
                &graded_base,
                &frame,
                &faces,
                people,
            ),
            Err(error) => {
                let mut stage = FullStage::new(FullStageKey::ColourGrading);
                stage.checks.push(format!(
                    "The photograph could not be rendered to measure its tones ({}); no grade applied.",
                    error.code
                ));
                stage
            }
        }
    } else {
        let mut stage = FullStage::new(FullStageKey::ColourGrading);
        stage.outcome = Outcome::Protected;
        stage.checks.push(
            if input.look == Look::Coming {
                "Your reference look decides the colour; it is matched after this edit."
            } else {
                "Your chosen look decides the colour grade; it was kept."
            }
            .into(),
        );
        stage
    };
    drop(grading);
    if graded.outcome == Outcome::Applied {
        show(
            Some(FullStageKey::ColourGrading),
            FullStageKey::ColourGrading.title().into(),
            &current,
        );
    }
    tell(FullStageKey::ColourGrading, false);

    let mut stages = retouch_stages(
        &before,
        &after_retouch,
        &advanced.report,
        input.look,
        people,
    )?;
    stages.push(calibration);
    stages.push(graded);
    stages.sort_by_key(|s| s.number);

    let applied = stages
        .iter()
        .filter(|s| s.outcome == Outcome::Applied)
        .count();
    let summary = format!(
        "All 10 stages ran in order: {applied} changed the photograph, the rest were measured and needed nothing, did not apply or kept your settings. {}",
        if advanced.report.quality.passed {
            "Quality control passed at QC resolution."
        } else {
            "Quality control flagged something or could not measure it - see Advanced edit."
        }
    );

    // The report is stored with the photograph; nothing is written when it is unchanged.
    let mut proposal = current.clone();
    proposal.extra.insert(
        KEY.into(),
        serde_json::to_value(Stored {
            version: VERSION.into(),
            look: input.look,
            stages: stages.clone(),
        })
        .map_err(|e| aura_core::errors::render::recipe_invalid(KEY, &e.to_string()))?,
    );
    proposal.provenance.source = EditSource::User;
    let (merged, changes) = schema::merge(&current, &proposal, EditSource::User)?;
    if !changes.changed.is_empty()
        && aura_recipe::recipe_hash(&merged)? != aura_recipe::recipe_hash(&current)?
    {
        schema::Validation::check(&merged)?;
        state.recipe_store().save(
            &project,
            &photo,
            &merged,
            &changes.changed,
            "Full auto edit · report",
        )?;
        current = merged;
    }
    let finished = state.clock().monotonic_ms();
    Ok(FullEditDto {
        photo_id: input.photo_id.clone(),
        recipe: crate::develop_commands::recipe_dto(&input.photo_id, &current),
        stages,
        faces: advanced.report.faces.max(faces.len()),
        quality_passed: advanced.report.quality.passed,
        summary,
        millis: finished.saturating_sub(started),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(pixels: &[([u8; 3], usize)]) -> Vec<u8> {
        pixels
            .iter()
            .flat_map(|(p, n)| std::iter::repeat_n(*p, *n).flatten())
            .collect()
    }

    #[test]
    fn saved_retouch_choices_win_and_the_default_is_nobodys_choice() {
        let mut recipe = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "");
        // The default, once stored by a run, is not mistaken for a photographer's choice.
        let mut report = serde_json::Map::new();
        report.insert(
            "options".into(),
            serde_json::to_value(default_options()).unwrap_or_default(),
        );
        recipe
            .extra
            .insert(crate::portrait_auto::KEY.into(), report.into());
        assert!(!crate::internal_ai::has_custom_skin_preferences(&recipe));
        assert!(default_options().settings.fabric > 0.0);
        // A photographer's own choice is used as it stands.
        let chosen = crate::portrait_features::Options {
            intensity: 0.6,
            teeth: false,
            scope: crate::portrait_features::Scope::Body,
            ..Default::default()
        };
        let mut report = serde_json::Map::new();
        report.insert(
            "options".into(),
            serde_json::to_value(chosen).unwrap_or_default(),
        );
        recipe
            .extra
            .insert(crate::portrait_auto::KEY.into(), report.into());
        let used = options_for(&recipe);
        assert_eq!(used.scope, crate::portrait_features::Scope::Body);
        assert!((used.intensity - 0.6).abs() < 1e-6);
    }

    #[test]
    fn stages_are_the_photographers_ten_in_order() {
        let titles: Vec<_> = FullStageKey::ALL.iter().map(|k| k.title()).collect();
        assert_eq!(
            titles,
            [
                "Basic edit",
                "Colour correction",
                "Calibration",
                "Colour grading",
                "Crop & straighten",
                "Blemishes",
                "Skin smoothing",
                "Face",
                "Body",
                "Advanced edit"
            ]
        );
        for (i, k) in FullStageKey::ALL.iter().enumerate() {
            assert_eq!(usize::from(k.number()), i + 1);
        }
        // Every advanced stage lands in one of them.
        for s in Stage::ALL {
            assert!(FullStageKey::ALL.contains(&FullStageKey::of_advanced(s)));
        }
    }

    #[test]
    fn green_shadows_get_a_magenta_shadow_tint_and_neutral_ones_none() {
        // Neutral midtones and shadows whose green carries about 15% more light than red and blue.
        let green = frame(&[([128, 128, 128], 4000), ([40, 43, 40], 4000)]);
        let r = read_colour(&green);
        let e = r.shadow_green.unwrap_or(0.0);
        assert!(e > 0.05, "measured {e}");
        let d = decide_calibration(&r);
        assert!(d.calibration.shadows_tint > 0, "{d:?}");
        assert!(d.calibration.shadows_tint <= 12);

        let neutral = frame(&[([128, 128, 128], 4000), ([40, 40, 40], 4000)]);
        let d = decide_calibration(&read_colour(&neutral));
        assert_eq!(d.calibration, Calibration::default(), "{d:?}");
    }

    #[test]
    fn a_dark_green_garland_among_neutral_shadows_is_not_a_tint() {
        // Neutral shadows and midtones, and a smaller dark green-ish object that is nearly neutral.
        let frame = frame(&[
            ([128, 128, 128], 4000),
            ([40, 40, 40], 3000),
            ([40, 43, 40], 1500),
        ]);
        let d = decide_calibration(&read_colour(&frame));
        assert_eq!(d.calibration.shadows_tint, 0, "{d:?}");
    }

    #[test]
    fn clipped_reds_lower_the_red_primary_and_unclipped_ones_do_not() {
        let clipped = frame(&[([255, 60, 50], 300), ([120, 110, 100], 9700)]);
        let d = decide_calibration(&read_colour(&clipped));
        assert!(d.calibration.red_saturation < 0, "{d:?}");
        assert!(d.calibration.red_saturation >= -18);
        assert_eq!(d.calibration.blue_saturation, 0);

        let fine = frame(&[([230, 60, 50], 300), ([120, 110, 100], 9700)]);
        let d = decide_calibration(&read_colour(&fine));
        assert_eq!(d.calibration.red_saturation, 0, "{d:?}");
    }

    #[test]
    fn a_frame_without_neutrals_is_not_given_a_shadow_tint() {
        let colourful = frame(&[([200, 40, 40], 5000), ([30, 30, 160], 5000)]);
        let r = read_colour(&colourful);
        assert!(r.shadow_green.is_none());
        assert_eq!(decide_calibration(&r).calibration.shadows_tint, 0);
    }

    #[test]
    fn a_grade_warms_highlights_and_cools_shadows_without_people() {
        let t = read_tones(&frame(&[
            ([235, 225, 205], 3000),
            ([120, 115, 110], 4000),
            ([40, 42, 45], 3000),
        ]));
        let d = decide_grade(&t, false);
        assert!(d.grade.highlights.saturation > 0, "{d:?}");
        assert!((15..=75).contains(&d.grade.highlights.hue), "{d:?}");
        assert!(d.grade.shadows.saturation > 0, "{d:?}");
        assert!((180..=270).contains(&d.grade.shadows.hue), "{d:?}");
    }

    #[test]
    fn with_people_the_shadows_stay_untinted() {
        let t = read_tones(&frame(&[
            ([235, 225, 205], 3000),
            ([120, 115, 110], 4000),
            ([40, 42, 45], 3000),
        ]));
        let d = decide_grade(&t, true);
        assert!(d.grade.highlights.saturation > 0);
        assert_eq!(d.grade.shadows.saturation, 0, "{d:?}");
    }

    #[test]
    fn monochrome_strong_colour_and_night_frames_are_not_graded() {
        let mono = read_tones(&frame(&[([200, 200, 200], 5000), ([60, 60, 60], 5000)]));
        assert_eq!(decide_grade(&mono, false).grade, ColourGrade::default());
        let neon = read_tones(&frame(&[([250, 20, 200], 6000), ([20, 220, 240], 4000)]));
        assert_eq!(decide_grade(&neon, false).grade, ColourGrade::default());
        let night = read_tones(&frame(&[([20, 18, 30], 9000), ([240, 200, 120], 1000)]));
        assert_eq!(decide_grade(&night, false).grade, ColourGrade::default());
    }

    #[test]
    fn retouch_operations_land_in_the_photographers_stages() {
        let id = |s: &str| format!("{PREFIX}{s}");
        assert_eq!(category_of(&id("0-spot-3")), Some(FullStageKey::Blemishes));
        assert_eq!(category_of(&id("0-clear")), Some(FullStageKey::Blemishes));
        assert_eq!(
            category_of(&id("0-texture")),
            Some(FullStageKey::SkinSmoothing)
        );
        assert_eq!(
            category_of(&id("1-lines-forehead")),
            Some(FullStageKey::SkinSmoothing)
        );
        assert_eq!(category_of(&id("0-eye-left")), Some(FullStageKey::Face));
        assert_eq!(category_of(&id("0-teeth")), Some(FullStageKey::Face));
        assert_eq!(category_of(&id("0-sculpt-cheek")), Some(FullStageKey::Face));
        assert_eq!(category_of(&id("0-body-texture")), Some(FullStageKey::Body));
        assert_eq!(category_of(&id("0-body-spots")), Some(FullStageKey::Body));
        assert_eq!(category_of(&id("0-cloth-1")), Some(FullStageKey::Advanced));
        assert_eq!(category_of(&id("backdrop")), Some(FullStageKey::Advanced));
        assert_eq!(
            category_of(crate::smart_edit::DEFRINGE_ID),
            Some(FullStageKey::Advanced)
        );
        assert_eq!(category_of("my-own-heal"), None);
        assert_eq!(op_name(&id("0-eye-left")), "eye left");
        assert_eq!(op_name(&id("2-spot-14")), "spot");
    }

    #[test]
    fn face_colour_shift_is_measured_inside_the_face_boxes() {
        let w = 40;
        let h = 40;
        let warm: Vec<u8> = std::iter::repeat_n([200_u8, 150, 120], w * h)
            .flatten()
            .collect();
        let cool: Vec<u8> = std::iter::repeat_n([180_u8, 150, 140], w * h)
            .flatten()
            .collect();
        let faces = [[0.25, 0.25, 0.75, 0.75]];
        let a = face_chroma(&warm, w, h, &faces);
        let b = face_chroma(&cool, w, h, &faces);
        assert!(chroma_shift(a, b).is_some_and(|m| m > FACE_SHIFT_CEILING));
        assert!(chroma_shift(a, a).is_some_and(|m| m < 1e-6));
        assert!(face_chroma(&warm, w, h, &[]).is_none());
    }
}
