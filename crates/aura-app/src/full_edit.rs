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

// ---------------------------------------------------------------------------------------------
// White balance and whites, measured on the render (stage 2 and stage 1)
// ---------------------------------------------------------------------------------------------

/// A pixel more saturated than this is a colour, not a neutral under a coloured light.
pub const NEUTRAL_SATURATION: f32 = 0.30;
/// Neutral references must cover this share of the frame (faces aside) to be measured.
pub const NEUTRAL_MIN_SHARE: f32 = 0.004;
/// The share of a warm light's warmth kept after white balance: a room lit by tungsten still
/// reads as a warm room, with its whites white. The photographer's finished photographs keep
/// about a fifth at the same neutral references (650 pairs, ADR-0191).
pub const WARMTH_KEPT: f32 = 0.2;
/// The share of a cool light's coolness kept: the photographer leaves shade, daylight and a
/// blue hour cool and hardly warms them at all (they keep about seven tenths, ADR-0191).
pub const COOLNESS_KEPT: f32 = 0.7;
/// The share of a warm light's warmth kept when there are no people in the frame: without skin
/// to judge it by, warm light is most often the picture - a golden hour, dust in low sun, a
/// lamp-lit room.
pub const SCENE_WARMTH_KEPT: f32 = 0.5;
/// The share of a magenta or green cast kept: half, as the photographer does, because a pink
/// canopy, a green lawn and coloured decor fool every neutral estimate the same way.
pub const TINT_KEPT: f32 = 0.5;
/// A white balance this close to the one in place (log chroma) is left as it is.
pub const BALANCE_TOLERANCE: f32 = 0.03;
/// The magenta or green cast read is counted up to this, in log chroma, before [`TINT_KEPT`]:
/// at most 0.075 comes off in one edit (about 25 tint).
pub const TINT_CEILING: f32 = 0.15;
/// The most white balance may turn the faces' hue, in degrees.
pub const FACE_HUE_CEILING: f32 = 5.0;
/// The largest white-balance correction, in log chroma, made in one edit.
pub const BALANCE_CEILING: f32 = 0.6;

/// A bright pixel whose channels are this close is a rebuilt clipped highlight, not a witness.
pub const RECONSTRUCTED_SPREAD: u8 = 2;

/// What a measured step adds to a stage the advanced retouch already reported.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Notes {
    pub applied: bool,
    pub protected: bool,
    pub changes: Vec<String>,
    pub checks: Vec<String>,
}

impl Notes {
    fn merge(&mut self, other: Self) {
        self.applied |= other.applied;
        self.protected |= other.protected;
        self.changes.extend(other.changes);
        self.checks.extend(other.checks);
    }

    fn into_stage(self, stage: &mut FullStage) {
        if self.applied {
            stage.outcome = Outcome::Applied;
        } else if self.protected && stage.outcome != Outcome::Applied {
            stage.outcome = Outcome::Protected;
        } else if stage.outcome == Outcome::NotApplicable && !self.checks.is_empty() {
            stage.outcome = Outcome::Unchanged;
        }
        stage.changes.extend(self.changes);
        stage.checks.extend(self.checks);
    }
}

/// The neutral references of a rendered frame: whites, greys and low-saturation cloth, outside
/// the faces, and the colour cast they read.
#[derive(Debug, Clone, PartialEq)]
pub struct Neutrals {
    /// `(ln r/g, ln b/g)` of the references in linear light; `(0, 0)` is neutral.
    pub cast: (f32, f32),
    /// The share of the measured frame the references cover.
    pub share: f32,
    /// Byte offsets of the references, so a second render is read at the same places.
    pub at: Vec<usize>,
    /// One vote per part of the frame (a 3x3 grid) that holds enough references.
    pub votes: usize,
    /// The share of those votes within [`VOTE_AGREEMENT`] of the cast: below
    /// [`MIXED_LIGHT_SHARE`] the parts of the frame are under different light.
    pub agreement: f32,
    /// Byte offsets of highlights the camera clipped (rebuilt as exact grey): a white balance
    /// tints them, so they are read again on the checked render.
    pub blown: Vec<usize>,
    /// The share of the measured frame they cover.
    pub blown_share: f32,
}

/// Blown highlights covering this share of the frame (a sky, a window) must stay white.
pub const BLOWN_SHARE: f32 = 0.02;
/// How far a white balance may tint blown highlights, in log chroma.
pub const BLOWN_CEILING: f32 = 0.07;

/// The colour blown highlights read on a render, `(ln r/g, ln b/g)`.
#[must_use]
pub fn blown_cast(rgb: &[u8], at: &[usize]) -> Option<(f32, f32)> {
    let samples: Vec<(f32, f32, f32)> = at
        .iter()
        .filter_map(|&i| {
            let p = rgb.get(i..i + 3)?;
            let (lr, lg, lb) = (lin(*p.first()?), lin(*p.get(1)?), lin(*p.get(2)?));
            (lg > 0.01 && lr > 0.01 && lb > 0.01).then(|| ((lr / lg).ln(), (lb / lg).ln(), 1.0))
        })
        .collect();
    if samples.len() < 20 {
        return None;
    }
    median_cast(&samples)
}

/// Two parts of a frame agree about the light within this, in log chroma.
pub const VOTE_AGREEMENT: f32 = 0.10;
/// Fewer agreeing votes than this share is mixed light: the correction is halved.
pub const MIXED_LIGHT_SHARE: f32 = 0.6;

fn in_boxes(faces: &[[f32; 4]], x: f32, y: f32) -> bool {
    faces.iter().any(|&[left, top, right, bottom]| {
        let (gw, gh) = ((right - left) * 0.2, (bottom - top) * 0.2);
        x >= left - gw && x <= right + gw && y >= top - gh && y <= bottom + gh
    })
}

/// The weighted median of `(value, weight)` pairs.
fn weighted_median(values: &mut [(f32, f32)]) -> Option<f32> {
    values.sort_by(|a, b| a.0.total_cmp(&b.0));
    let total: f32 = values.iter().map(|v| v.1).sum();
    if total <= 0.0 {
        return None;
    }
    let mut run = 0.0;
    for (value, weight) in values.iter() {
        run += weight;
        if run >= total * 0.5 {
            return Some(*value);
        }
    }
    values.last().map(|v| v.0)
}

/// `(ln r/g, ln b/g, luminance)` of one sRGB pixel, when it can witness the light: unclipped,
/// not dark and not strongly coloured.
fn neutral_sample(p: &[u8]) -> Option<(f32, f32, f32)> {
    let (&r, &g, &b) = (p.first()?, p.get(1)?, p.get(2)?);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    if max >= 245 || min < 12 {
        return None;
    }
    if f32::from(max - min) / f32::from(max) > NEUTRAL_SATURATION {
        return None;
    }
    // A highlight the camera clipped is rebuilt as an exact grey: it shows the reconstruction,
    // not the light. A white shirt blown in the camera file reads (238, 238, 238) at any balance.
    if max - min <= RECONSTRUCTED_SPREAD && max >= 140 {
        return None;
    }
    let (lr, lg, lb) = (lin(r), lin(g), lin(b));
    let y = 0.2126 * lr + 0.7152 * lg + 0.0722 * lb;
    (y >= 0.03).then(|| ((lr / lg).ln(), (lb / lg).ln(), y))
}

fn median_cast(samples: &[(f32, f32, f32)]) -> Option<(f32, f32)> {
    // Brighter references weigh more: whites take the colour of the light and little else.
    let mut red: Vec<(f32, f32)> = samples.iter().map(|s| (s.0, s.2)).collect();
    let mut blue: Vec<(f32, f32)> = samples.iter().map(|s| (s.1, s.2)).collect();
    Some((weighted_median(&mut red)?, weighted_median(&mut blue)?))
}

/// Find the neutral references of a rendered sRGB frame. The search starts from its brightest
/// near-neutral tones and narrows onto the cluster around them, so a red sari, gold work or skin
/// do not pull it; `None` when too little of the frame is near-neutral to measure.
#[must_use]
pub fn read_neutrals(rgb: &[u8], w: usize, h: usize, faces: &[[f32; 4]]) -> Option<Neutrals> {
    if w < 8 || h < 8 {
        return None;
    }
    let step = ((w * h) as f32 / 120_000.0).sqrt().ceil().max(1.0) as usize;
    let mut candidates = Vec::new();
    let mut blown = Vec::new();
    let mut total = 0_usize;
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            if in_boxes(
                faces,
                (x as f32 + 0.5) / w as f32,
                (y as f32 + 0.5) / h as f32,
            ) {
                continue;
            }
            total += 1;
            let i = (y * w + x) * 3;
            let Some(p) = rgb.get(i..i + 3) else {
                continue;
            };
            if let Some(sample) = neutral_sample(p) {
                candidates.push((i, sample));
            } else if let (Some(&r), Some(&g), Some(&b)) = (p.first(), p.get(1), p.get(2)) {
                if r.max(g).max(b) >= 140
                    && r.max(g).max(b) - r.min(g).min(b) <= RECONSTRUCTED_SPREAD
                {
                    blown.push(i);
                }
            }
        }
    }
    if total == 0 || candidates.len() < 40 {
        return None;
    }
    let mut lumas: Vec<f32> = candidates.iter().map(|c| c.1 .2).collect();
    lumas.sort_by(f32::total_cmp);
    let bright = lumas.get(lumas.len() * 4 / 5).copied().unwrap_or(0.0);
    let brightest: Vec<_> = candidates
        .iter()
        .filter(|c| c.1 .2 >= bright)
        .map(|c| c.1)
        .collect();
    let mut centre = median_cast(&brightest)?;
    let mut chosen: Vec<&(usize, (f32, f32, f32))> = Vec::new();
    for radius in [0.2_f32, 0.12, 0.08] {
        chosen = candidates
            .iter()
            .filter(|c| (c.1 .0 - centre.0).hypot(c.1 .1 - centre.1) <= radius)
            .collect();
        if chosen.len() < 20 {
            return None;
        }
        let samples: Vec<_> = chosen.iter().map(|c| c.1).collect();
        centre = median_cast(&samples)?;
    }
    let share = chosen.len() as f32 / total as f32;
    if share < NEUTRAL_MIN_SHARE {
        return None;
    }
    // Equal votes from the parts of the frame: a large canopy or wall must not outvote the
    // rest by its area, and parts that disagree are under different light.
    let mut parts: [Vec<(f32, f32, f32)>; 9] = std::array::from_fn(|_| Vec::new());
    for c in &chosen {
        let pixel = c.0 / 3;
        let (x, y) = (pixel % w, pixel / w);
        if let Some(part) = parts.get_mut((y * 3 / h).min(2) * 3 + (x * 3 / w).min(2)) {
            part.push(c.1);
        }
    }
    let votes: Vec<(f32, f32)> = parts
        .iter()
        .filter(|p| p.len() >= 15)
        .filter_map(|p| median_cast(p))
        .collect();
    let mut cast = centre;
    let mut agreement = 1.0;
    if votes.len() >= 3 {
        let mut red: Vec<(f32, f32)> = votes.iter().map(|v| (v.0, 1.0)).collect();
        let mut blue: Vec<(f32, f32)> = votes.iter().map(|v| (v.1, 1.0)).collect();
        cast = (weighted_median(&mut red)?, weighted_median(&mut blue)?);
        agreement = votes
            .iter()
            .filter(|v| (v.0 - cast.0).hypot(v.1 - cast.1) <= VOTE_AGREEMENT)
            .count() as f32
            / votes.len() as f32;
    }
    Some(Neutrals {
        cast,
        share,
        at: chosen.iter().map(|c| c.0).collect(),
        votes: votes.len(),
        agreement,
        blown_share: blown.len() as f32 / total as f32,
        blown,
    })
}

/// The cast the same references read on another render of the same size.
#[must_use]
pub fn cast_at(rgb: &[u8], at: &[usize]) -> Option<(f32, f32)> {
    let samples: Vec<_> = at
        .iter()
        .filter_map(|&i| rgb.get(i..i + 3).and_then(neutral_sample))
        .collect();
    if samples.len() < 20 {
        return None;
    }
    median_cast(&samples)
}

/// `(ln r, ln b)` of the white-balance multipliers a temperature and tint imply, green at one.
fn balance_log(kelvin: u32, tint: i16) -> (f32, f32) {
    let [r, _, b] = aura_render::colour::white_balance(kelvin as f32, f32::from(tint));
    (r.max(1e-6).ln(), b.max(1e-6).ln())
}

/// The temperature and tint whose multipliers are closest to `target` (`(ln r, ln b)`).
#[must_use]
pub fn solve_balance(target: (f32, f32)) -> (u32, i16) {
    let score = |kelvin: u32| {
        let (ar, ab) = balance_log(kelvin, 0);
        // Tint scales green down by (1 + t/300), lifting red and blue together.
        let u = ((target.0 - ar) + (target.1 - ab)) * 0.5;
        let tint = ((u.exp() - 1.0) * 300.0).round().clamp(-150.0, 150.0) as i16;
        let (r, b) = balance_log(kelvin, tint);
        ((r - target.0).powi(2) + (b - target.1).powi(2), tint)
    };
    let mut best = (f32::INFINITY, 5500_u32, 0_i16);
    for kelvin in (2000..=50_000).step_by(50) {
        let (error, tint) = score(kelvin);
        if error < best.0 {
            best = (error, kelvin, tint);
        }
    }
    for kelvin in best.1.saturating_sub(50).max(2000)..=(best.1 + 50).min(50_000) {
        let (error, tint) = score(kelvin);
        if error < best.0 {
            best = (error, kelvin, tint);
        }
    }
    (best.1, best.2)
}

/// The hue of the faces' mean colour, in degrees (reds past zero read as negative).
#[must_use]
pub fn face_hue(rgb: &[u8], w: usize, h: usize, faces: &[[f32; 4]]) -> Option<f32> {
    let mut sum = [0.0_f64; 3];
    let mut n = 0_usize;
    for &[left, top, right, bottom] in faces {
        let (cx, cy) = (f32::midpoint(left, right), f32::midpoint(top, bottom));
        let (hw, hh) = ((right - left) * 0.3, (bottom - top) * 0.3);
        let x0 = ((cx - hw).max(0.0) * w as f32) as usize;
        let x1 = ((cx + hw).min(1.0) * w as f32) as usize;
        let y0 = ((cy - hh).max(0.0) * h as f32) as usize;
        let y1 = ((cy + hh).min(1.0) * h as f32) as usize;
        for y in y0..y1.min(h) {
            for x in x0..x1.min(w) {
                let i = (y * w + x) * 3;
                if let Some(p) = rgb.get(i..i + 3) {
                    for (s, v) in sum.iter_mut().zip(p) {
                        *s += f64::from(*v);
                    }
                    n += 1;
                }
            }
        }
    }
    if n < 50 {
        return None;
    }
    let [r, g, b] = sum.map(|s| (s / n as f64) as f32);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    if max - min < 1.0 {
        return Some(0.0);
    }
    let hue = if (max - r).abs() < f32::EPSILON {
        60.0 * ((g - b) / (max - min))
    } else if (max - g).abs() < f32::EPSILON {
        60.0 * ((b - r) / (max - min) + 2.0)
    } else {
        60.0 * ((r - g) / (max - min) + 4.0)
    };
    Some(if hue > 180.0 { hue - 360.0 } else { hue })
}

/// Skin of every complexion sits between a rosy red and a golden yellow.
#[must_use]
pub fn skin_hue(hue: f32) -> bool {
    (-10.0..=50.0).contains(&hue)
}

/// The decided correction for a cast read at the camera's own balance: the multipliers wanted,
/// as `(ln r, ln b)`, and the output cast they aim for. Removes the cast, keeping
/// [`WARMTH_KEPT`] of a warm one ([`SCENE_WARMTH_KEPT`] without people) and [`COOLNESS_KEPT`]
/// of a cool one, and moves at most
/// [`BALANCE_CEILING`].
/// A cast splits into warm-cool, `(ln r/g - ln b/g) / 2`, and magenta-green,
/// `(ln r/g + ln b/g) / 2`; the magenta-green part comes off at most this far in one edit,
/// because a pink canopy, a green lawn or coloured decor fool every neutral estimate the same way.
#[must_use]
pub fn balance_goal(cast: (f32, f32), people: bool) -> ((f32, f32), (f32, f32)) {
    let warmth = (cast.0 - cast.1) * 0.5;
    let magenta = (cast.0 + cast.1) * 0.5;
    let keep = if warmth > 0.0 && people {
        WARMTH_KEPT
    } else if warmth > 0.0 {
        SCENE_WARMTH_KEPT
    } else {
        COOLNESS_KEPT
    };
    let warmth_left = warmth * keep;
    let magenta_left = magenta - magenta.clamp(-TINT_CEILING, TINT_CEILING) * (1.0 - TINT_KEPT);
    let mut goal = (magenta_left + warmth_left, magenta_left - warmth_left);
    let (dr, db) = (goal.0 - cast.0, goal.1 - cast.1);
    let size = dr.hypot(db);
    if size > BALANCE_CEILING {
        let k = BALANCE_CEILING / size;
        goal = (cast.0 + dr * k, cast.1 + db * k);
    }
    // At the camera's own balance the multipliers are one, so the output cast moves with them.
    ((goal.0 - cast.0, goal.1 - cast.1), goal)
}

/// Stage 2's white balance, for every photograph: measured on a render at the camera's own
/// balance, so a second run decides the same thing, and checked on a render of the result.
/// Returns what it did and, when it saved a balance, the render with it.
fn balance_white(
    state: &AppState,
    ids: (&ProjectId, PhotoId),
    current: &mut Recipe,
    base: &mut Recipe,
    shown: &Frame,
    faces: &[[f32; 4]],
    look: Look,
) -> (Notes, Option<Frame>) {
    let (project, photo) = ids;
    let mut notes = Notes::default();
    if look == Look::Applied {
        notes.protected = true;
        notes
            .checks
            .push("White balance: your chosen look sets it; kept.".into());
        return (notes, None);
    }
    let reference = aura_render::colour::REFERENCE_KELVIN as u32;
    let mut as_shot = base.clone();
    as_shot.global.temperature = reference;
    as_shot.global.tint = 0;
    let rendered_as_shot;
    let frame = if base.global.temperature == reference && base.global.tint == 0 {
        shown
    } else {
        match measure(state, photo, &as_shot) {
            Ok(f) => {
                rendered_as_shot = f;
                &rendered_as_shot
            }
            Err(error) => {
                notes.checks.push(format!(
                    "White balance left as it was: the photograph could not be rendered at the camera's own balance to measure it ({}).",
                    error.code
                ));
                return (notes, None);
            }
        }
    };
    let Some(refs) = read_neutrals(&frame.0, frame.1, frame.2, faces) else {
        notes.checks.push(format!(
            "White balance kept as shot: less than {:.1}% of the frame is white, grey or near-neutral, so there is nothing true to measure the light on.",
            NEUTRAL_MIN_SHARE * 100.0
        ));
        return (notes, None);
    };
    let size = refs.cast.0.hypot(refs.cast.1);
    let (mut target, mut goal) = balance_goal(refs.cast, !faces.is_empty());
    let mixed = refs.agreement < MIXED_LIGHT_SHARE;
    if mixed {
        // Mixed light: no one balance is right everywhere, so take half the step.
        target = (target.0 * 0.5, target.1 * 0.5);
        goal = (refs.cast.0 + target.0, refs.cast.1 + target.1);
        notes.checks.push(format!(
            "Mixed light: only {:.0}% of the {} parts of the frame with neutral references agree about its colour, so half the correction was made.",
            refs.agreement * 100.0,
            refs.votes
        ));
    }
    let now = balance_log(base.global.temperature, base.global.tint);
    if (target.0 - now.0).hypot(target.1 - now.1) < BALANCE_TOLERANCE {
        notes.checks.push(format!(
            "White balance measured on whites, greys and near-neutral cloth over {:.1}% of the frame (faces left out): {}",
            refs.share * 100.0,
            if size < BALANCE_TOLERANCE {
                format!("already neutral (cast {size:.3}).")
            } else {
                format!("cast {size:.3}, already corrected the way this edit corrects it.")
            }
        ));
        return (notes, None);
    }
    let face_before = face_hue(&frame.0, frame.1, frame.2, faces);
    // Checked on the render: the same references must move to the goal, and faces stay skin.
    let mut best: Option<((u32, i16), Frame, (f32, f32))> = None;
    let mut halved = false;
    for _ in 0..3 {
        let (kelvin, tint) = solve_balance(target);
        let mut proposal = base.clone();
        proposal.global.temperature = kelvin;
        proposal.global.tint = tint;
        let Ok(rendered) = measure(state, photo, &proposal) else {
            break;
        };
        let Some(after) = cast_at(&rendered.0, &refs.at) else {
            break;
        };
        let face_after = face_hue(&rendered.0, rendered.1, rendered.2, faces);
        let turned = match (face_before, face_after) {
            (Some(a), Some(b)) => (b - a).abs(),
            _ => 0.0,
        };
        let lost_skin = face_before.is_some_and(skin_hue) && !face_after.is_some_and(skin_hue);
        if lost_skin || turned > FACE_HUE_CEILING {
            if halved {
                best = None;
                notes.checks.push(format!(
                    "Even a reduced white balance turned the faces {turned:.0}° in hue on the render (limit {FACE_HUE_CEILING:.0}°); the camera's balance was kept."
                ));
                break;
            }
            halved = true;
            // Scale the step back to what the faces allow, measured, and check again.
            let k = if lost_skin {
                0.5
            } else {
                (FACE_HUE_CEILING / turned * 0.9).clamp(0.3, 0.9)
            };
            // Towards the camera's own balance, where it was measured, so a repeat run decides the same.
            target = (target.0 * k, target.1 * k);
            notes.checks.push(format!(
                "The full correction turned the faces {turned:.0}° in hue (limit {FACE_HUE_CEILING:.0}°); reduced to {:.0}% and checked again.",
                k * 100.0
            ));
            continue;
        }
        // A sky or window the camera clipped must stay white, not turn the colour opposite the cast.
        if refs.blown_share >= BLOWN_SHARE && !halved {
            if let Some(tinted) = blown_cast(&rendered.0, &refs.blown) {
                let tint_size = tinted.0.hypot(tinted.1);
                if tint_size > BLOWN_CEILING {
                    halved = true;
                    let k = (BLOWN_CEILING / tint_size).clamp(0.3, 0.9);
                    // Towards the camera's own balance, where it was measured, so a repeat run decides the same.
                    target = (target.0 * k, target.1 * k);
                    goal = (refs.cast.0 + target.0, refs.cast.1 + target.1);
                    notes.checks.push(format!(
                        "Blown highlights over {:.0}% of the frame (a sky or window) would have turned {tint_size:.2} off white; the correction was reduced to {:.0}% so they stay white.",
                        refs.blown_share * 100.0,
                        k * 100.0
                    ));
                    continue;
                }
            }
        }
        let error = (after.0 - goal.0, after.1 - goal.1);
        let closer = best
            .as_ref()
            .is_none_or(|b| error.0.hypot(error.1) < (b.2 .0 - goal.0).hypot(b.2 .1 - goal.1));
        if closer {
            best = Some(((kelvin, tint), rendered, after));
        }
        if error.0.hypot(error.1) < 0.025 || halved {
            break;
        }
        // The tone curve and the camera matrix bend the result a little: step by what is left.
        target = (target.0 - error.0, target.1 - error.1);
    }
    let Some(((kelvin, tint), rendered, after)) = best else {
        if notes.checks.is_empty() {
            notes.checks.push(
                "White balance left as shot: the correction could not be checked on a render."
                    .into(),
            );
        }
        return (notes, None);
    };
    let left = after.0.hypot(after.1);
    if left >= size * 0.85 {
        notes.checks.push(format!(
            "A white balance of {kelvin} K / tint {tint:+} did not bring the whites closer to neutral on the render (cast {size:.3} to {left:.3}); kept as shot."
        ));
        return (notes, None);
    }
    let warm = refs.cast.0 - refs.cast.1 > 0.0;
    let mut proposal = current.clone();
    proposal.global.temperature = kelvin;
    proposal.global.tint = tint;
    let change = format!(
        "White balance {kelvin} K / tint {tint:+}: whites, greys and near-neutral cloth over {:.1}% of the frame (faces left out) read a {} cast of {size:.3}; now {left:.3}{}.",
        refs.share * 100.0,
        if warm { "warm" } else { "cool or green" },
        if warm {
            ", keeping a little of the light's warmth"
        } else {
            ""
        }
    );
    let label = format!("Full auto edit 2/10 · White balance {kelvin} K / tint {tint:+}");
    match save_step(state, project, &photo, current, proposal, &label) {
        Ok((true, _)) => {
            notes.applied = true;
            notes.changes.push(change);
            notes.checks.push(format!(
                "Checked on the render at the same places{}.",
                match (
                    face_before,
                    face_hue(&rendered.0, rendered.1, rendered.2, faces)
                ) {
                    (Some(a), Some(b)) =>
                        format!("; faces stayed skin-coloured (hue {a:.0}° to {b:.0}°)"),
                    _ => String::new(),
                }
            ));
            base.global.temperature = kelvin;
            base.global.tint = tint;
            (notes, Some(rendered))
        }
        Ok((false, refused)) if !refused.is_empty() => {
            notes.protected = true;
            notes.checks.push(format!(
                "White balance: kept your own setting ({}).",
                refused.join(", ")
            ));
            (notes, None)
        }
        Ok((false, _)) => (notes, None),
        Err(error) => {
            notes.checks.push(format!(
                "The white balance could not be saved ({}); left as it was.",
                error.code
            ));
            (notes, None)
        }
    }
}

/// The tone sliders, in the order [`crate::tone_target::Sliders`] solves them.
const TONE_PATHS: [&str; 6] = [
    "global.exposure",
    "global.contrast",
    "global.highlights",
    "global.shadows",
    "global.whites",
    "global.blacks",
];

/// Whether a person set this field (or the block it is in) themselves.
fn owned(recipe: &Recipe, path: &str) -> bool {
    recipe.provenance.user_edited_fields.iter().any(|f| {
        f == path
            || path
                .strip_prefix(f.as_str())
                .is_some_and(|rest| rest.starts_with('.'))
    })
}

/// A slider a person set keeps their value; the rest come from the solve.
fn keep_fixed(
    solved: crate::tone_target::Sliders,
    kept: crate::tone_target::Sliders,
    fixed: [bool; 6],
) -> crate::tone_target::Sliders {
    let mut out = solved;
    for (i, f) in fixed.iter().enumerate() {
        if *f {
            match i {
                0 => out.exposure = kept.exposure,
                1 => out.contrast = kept.contrast,
                2 => out.highlights = kept.highlights,
                3 => out.shadows = kept.shadows,
                4 => out.whites = kept.whites,
                _ => out.blacks = kept.blacks,
            }
        }
    }
    out
}

/// Stage 1's tone, ADR-0191: the light is read on a render with every tone slider at zero, the
/// lightness the photographer's own finished photographs give that light is predicted (skin
/// first when there are people), and exposure, contrast, highlights, shadows, whites and blacks
/// are solved to it in a model of the renderer, then checked on the render, which corrects the
/// model where it was wrong. Read as shot, so a second run decides the same thing.
#[allow(clippy::too_many_arguments)]
fn shape_tone(
    state: &AppState,
    ids: (&ProjectId, PhotoId),
    current: &mut Recipe,
    base: &mut Recipe,
    shown: &Frame,
    faces: &[[f32; 4]],
    look: Look,
    incumbent: Option<crate::tone_target::Sliders>,
    read_as_shot: &mut Option<crate::tone_target::Reading>,
) -> (Notes, Option<Frame>) {
    use crate::tone_target::{self as tt, Sliders};
    let (project, photo) = ids;
    let mut notes = Notes::default();
    if look == Look::Applied {
        notes
            .checks
            .push("Tone: your chosen look sets its own tone; it was kept.".into());
        return (notes, None);
    }
    let fixed = TONE_PATHS.map(|p| owned(current, p));
    if fixed.iter().all(|f| *f) {
        notes.protected = true;
        notes
            .checks
            .push("Tone: you set exposure and every tone slider yourself; all kept.".into());
        return (notes, None);
    }
    // The light as shot: every free tone slider at zero, and no automatic levels curve (whites
    // and blacks set the ends of the range here; a curve on top would set them twice).
    let mut neutral = base.clone();
    let kept = Sliders::of(&base.global);
    keep_fixed(Sliders::ZERO, kept, fixed).write(&mut neutral.global);
    if !owned(current, "global.curve") {
        neutral.global.curve = aura_recipe::Curve::identity();
    }
    let frame = match measure(state, photo, &neutral) {
        Ok(frame) => frame,
        Err(error) => {
            notes.checks.push(format!(
                "Tone: the photograph could not be rendered as shot ({}); left as it was.",
                error.code
            ));
            return (notes, None);
        }
    };
    let before = tt::read_against(&shown.0, shown.1, shown.2, faces, &frame.0);
    let shaped = tt::shape(&frame, faces, fixed, incumbent, |sliders| {
        let mut proposal = neutral.clone();
        keep_fixed(sliders, kept, fixed).write(&mut proposal.global);
        measure(state, photo, &proposal).ok()
    });
    let Some(tt::Shaped {
        shot,
        goal,
        sliders,
        frame: rendered,
        reading,
        clip_allowed,
    }) = shaped
    else {
        notes
            .checks
            .push("Tone: the solved tone could not be checked on a render; left as it was.".into());
        return (notes, None);
    };
    *read_as_shot = Some(shot);
    let distance = |r: &tt::Reading| tt::distance(r, &goal, clip_allowed);
    // Kept only when the render is closer to the photographer's tone than what was there.
    if let Some(b) = before
        .as_ref()
        .filter(|b| distance(b) <= distance(&reading))
    {
        notes.checks.push(format!(
            "Tone: read as {}; the tone in place was already as close to the photographer's as the solve came ({}).",
            goal.intent.label(),
            describe_tone(&shot, &goal, b)
        ));
        return (notes, None);
    }
    let sliders = keep_fixed(sliders, kept, fixed);
    let mut proposal = current.clone();
    sliders.write(&mut proposal.global);
    proposal.global.curve = neutral.global.curve.clone();
    let what = format!(
        "exposure {:+.2} EV, highlights {:+}, shadows {:+}, whites {:+}, blacks {:+}, contrast {:+}",
        sliders.exposure,
        sliders.highlights,
        sliders.shadows,
        sliders.whites,
        sliders.blacks,
        sliders.contrast
    );
    let label = format!("Full auto edit 1/10 · Tone: {what}");
    match save_step(state, project, &photo, current, proposal, &label) {
        Ok((saved, refused)) => {
            if saved {
                notes.applied = true;
                notes.changes.push(format!(
                    "Tone: {what} - {} (checked on the render, {:.2}% clipped).",
                    describe_tone(&shot, &goal, &reading),
                    reading.clipped * 100.0
                ));
                notes.checks.push(format!(
                    "Tone: read as {}; the targets are the lightness the photographer's own finished photographs give this light (learned from 651 of them, eight cameras, day and night), solved in a model of the renderer and corrected on the render.",
                    goal.intent.label()
                ));
                sliders.write(&mut base.global);
                base.global.curve = neutral.global.curve.clone();
            }
            if !refused.is_empty() {
                notes.protected = true;
                notes.checks.push(format!(
                    "Tone: kept your own setting ({}).",
                    refused.join(", ")
                ));
            }
            (notes, saved.then_some(rendered))
        }
        Err(error) => {
            notes.checks.push(format!(
                "The tone could not be saved ({}); left as it was.",
                error.code
            ));
            (notes, None)
        }
    }
}

/// Stage 1's colour, ADR-0191: the frame's colour strength (median CIE chroma) is brought to
/// what the photographer's finished photographs have for this light, with vibrance first and
/// saturation only when vibrance is not enough, never past the skin ceiling or into new
/// clipping, and checked on the render. Runs after the tone, because chroma follows lightness.
#[allow(clippy::too_many_arguments)]
fn enrich_colour(
    state: &AppState,
    ids: (&ProjectId, PhotoId),
    current: &mut Recipe,
    base: &mut Recipe,
    shown: &Frame,
    faces: &[[f32; 4]],
    look: Look,
    shot: &crate::tone_target::Reading,
) -> (Notes, Option<Frame>) {
    use crate::tone_target as tt;
    let (project, photo) = ids;
    let mut notes = Notes::default();
    if look != Look::None {
        return (notes, None);
    }
    let (own_v, own_s) = (
        owned(current, "global.vibrance"),
        owned(current, "global.saturation"),
    );
    if own_v && own_s {
        notes.protected = true;
        notes
            .checks
            .push("Colour: you set vibrance and saturation yourself; kept.".into());
        return (notes, None);
    }
    let samples = tt::samples(&shown.0, shown.1, shown.2, faces);
    let Some((now, _, _)) = tt::simulate_colour(&samples, 0.0, 0.0) else {
        return (notes, None);
    };
    let Some(target) = tt::chroma_target(shot, now) else {
        notes.checks.push(
            "Colour: the photograph has next to no colour (black and white or one tone); none was added."
                .into(),
        );
        return (notes, None);
    };
    if (now - target).abs() < 1.0 {
        notes.checks.push(format!(
            "Colour: chroma {now:.0} is already the photographer's for this light ({target:.0})."
        ));
        return (notes, None);
    }
    let (v0, s0) = (base.global.vibrance, base.global.saturation);
    let room = if own_v { 0 } else { 60 - v0 };
    let Some((mut dv, mut ds, _)) = tt::solve_colour(&samples, target, room) else {
        return (notes, None);
    };
    if own_v {
        dv = 0;
    }
    if own_s {
        ds = 0;
    }
    let mut best: Option<(i16, i16, Frame, f32)> = None;
    for _ in 0..2 {
        if dv == 0 && ds == 0 {
            break;
        }
        let mut proposal = base.clone();
        proposal.global.vibrance = (v0 + dv).clamp(-100, 100);
        proposal.global.saturation = (s0 + ds).clamp(-100, 100);
        let Ok(rendered) = measure(state, photo, &proposal) else {
            break;
        };
        let Some(reading) = tt::read(&rendered.0, rendered.1, rendered.2, faces) else {
            break;
        };
        let got = reading.chroma;
        let closer = best
            .as_ref()
            .is_none_or(|b| (got - target).abs() < (b.3 - target).abs());
        if closer {
            best = Some((
                proposal.global.vibrance,
                proposal.global.saturation,
                rendered,
                got,
            ));
        }
        if (got - target).abs() < 1.5 || (got - now).abs() < 0.3 {
            break;
        }
        // The render's response is the truth: scale the move by it once.
        let scale = ((target - now) / (got - now)).clamp(0.3, 2.0);
        dv = (f32::from(dv) * scale).round() as i16;
        ds = (f32::from(ds) * scale).round() as i16;
        dv = dv.min(room.max(0));
    }
    let Some((vibrance, saturation, rendered, got)) = best else {
        return (notes, None);
    };
    if (got - target).abs() >= (now - target).abs() {
        notes.checks.push(format!(
            "Colour: chroma {now:.0} against the photographer's {target:.0}; the render did not come closer, left as it was."
        ));
        return (notes, None);
    }
    let mut proposal = current.clone();
    proposal.global.vibrance = vibrance;
    proposal.global.saturation = saturation;
    let what = format!("vibrance {vibrance:+}, saturation {saturation:+}");
    match save_step(
        state,
        project,
        &photo,
        current,
        proposal,
        &format!("Full auto edit 1/10 · Colour: {what}"),
    ) {
        Ok((true, refused)) => {
            notes.applied = true;
            notes.changes.push(format!(
                "Colour: {what} - chroma {now:.0} -> {got:.0}, the photographer's for this light {target:.0} (vibrance first, skin kept below chroma {:.0}; checked on the render).",
                crate::tone_target::SKIN_CHROMA_CEILING
            ));
            if !refused.is_empty() {
                notes.protected = true;
            }
            base.global.vibrance = vibrance;
            base.global.saturation = saturation;
            (notes, Some(rendered))
        }
        Ok((false, refused)) => {
            if !refused.is_empty() {
                notes.protected = true;
                notes.checks.push(format!(
                    "Colour: kept your own setting ({}).",
                    refused.join(", ")
                ));
            }
            (notes, None)
        }
        Err(error) => {
            notes.checks.push(format!(
                "The colour could not be saved ({}); left as it was.",
                error.code
            ));
            (notes, None)
        }
    }
}

/// The faces the measured stages read, as boxes `[left, top, right, bottom]` (0 to 1): those
/// on the thumbnail, and the small ones of a crowd from the proxy, as the advanced retouch
/// finds them.
fn faces_of(state: &AppState, project_id: &str, photo: PhotoId) -> Vec<[f32; 4]> {
    let Ok(previews) = state.previews(project_id) else {
        return Vec::new();
    };
    let Ok(thumb) = previews.get(
        photo,
        aura_raw::PixelLevel::Thumb(512),
        Priority::Interactive,
    ) else {
        return Vec::new();
    };
    let Some(found) = thumb
        .as_srgb8()
        .and_then(|rgb| aura_vision::portrait::detect(rgb, thumb.width, thumb.height).ok())
    else {
        return Vec::new();
    };
    let proxy = previews
        .get(
            photo,
            aura_raw::PixelLevel::Proxy2048,
            Priority::Interactive,
        )
        .ok();
    let all = proxy
        .as_ref()
        .and_then(|p| {
            p.as_srgb8().and_then(|pixels| {
                aura_vision::portrait::detect_small_faces(pixels, p.width, p.height, &found).ok()
            })
        })
        .unwrap_or(found);
    all.into_iter().map(|f| f.bounds).collect()
}

/// What the measured white balance, tone and colour did, and the render they leave.
struct Basics {
    balance: Notes,
    finish: Notes,
    shown: AuraResult<Frame>,
}

/// Stage 2's white balance, then stage 1's tone and colour, each measured and checked on the
/// render, before calibration and grade read it. `show` pictures each step that changed the
/// photograph.
#[allow(clippy::too_many_arguments)]
fn measured_basics(
    state: &AppState,
    ids: (&ProjectId, PhotoId),
    current: &mut Recipe,
    base: &mut Recipe,
    mut shown: AuraResult<Frame>,
    faces: &[[f32; 4]],
    look: Look,
    incumbent: Option<crate::tone_target::Sliders>,
    show: &dyn Fn(Option<FullStageKey>, String, &Recipe),
) -> Basics {
    let mut balance = Notes::default();
    let mut finish = Notes::default();
    if let Ok(frame) = &shown {
        let (notes, balanced) = balance_white(state, ids, current, base, frame, faces, look);
        balance = notes;
        if let Some(frame) = balanced {
            show(
                Some(FullStageKey::ColourCorrection),
                format!("{} · White balance", FullStageKey::ColourCorrection.title()),
                current,
            );
            shown = Ok(frame);
        }
    }
    if let Ok(frame) = &shown {
        let mut as_shot = None;
        let (notes, toned) = shape_tone(
            state,
            ids,
            current,
            base,
            frame,
            faces,
            look,
            incumbent,
            &mut as_shot,
        );
        finish = notes;
        if let Some(frame) = toned {
            show(
                Some(FullStageKey::Basic),
                format!("{} · Tone", FullStageKey::Basic.title()),
                current,
            );
            shown = Ok(frame);
        }
        // Colour strength follows the tone: read after it, on the toned render.
        if let (Some(shot), Ok(frame)) = (as_shot, &shown) {
            let (notes, enriched) =
                enrich_colour(state, ids, current, base, frame, faces, look, &shot);
            finish.merge(notes);
            if let Some(frame) = enriched {
                show(
                    Some(FullStageKey::Basic),
                    format!("{} · Colour", FullStageKey::Basic.title()),
                    current,
                );
                shown = Ok(frame);
            }
        }
    }
    Basics {
        balance,
        finish,
        shown,
    }
}

/// White balance, tone and colour alone, on a photograph as it is - the measured basics of the
/// full edit without the retouch, calibration or grade. For measuring them against the
/// photographer's own photographs quickly (`tests/photographer_pairs.rs`); not a command.
///
/// # Errors
///
/// When the identifiers are invalid or the photograph's recipe cannot be loaded.
pub fn basics_only(state: &AppState, project_id: &str, photo_id: &str) -> IpcResult<Vec<String>> {
    let invalid = |message: &str| aura_core::errors::render::recipe_invalid("photo", message);
    let photo = PhotoId::from_db(photo_id).map_err(|_| invalid("Invalid photo identifier"))?;
    let project =
        ProjectId::from_db(project_id).map_err(|_| invalid("Invalid project identifier"))?;
    let faces = faces_of(state, project_id, photo);
    let mut current = crate::develop_commands::load_or_neutral(state, photo)?;
    let mut base = current.clone();
    let shown = measure(state, photo, &base);
    let basics = measured_basics(
        state,
        (&project, photo),
        &mut current,
        &mut base,
        shown,
        &faces,
        Look::None,
        None,
        &|_, _, _| {},
    );
    Ok(basics
        .balance
        .changes
        .into_iter()
        .chain(basics.balance.checks)
        .chain(basics.finish.changes)
        .chain(basics.finish.checks)
        .collect())
}

/// "skin L* 41 -> 58 (photographer 59), middle 38 -> 53 (55), ...": as shot, now, and the
/// photographer's target.
fn describe_tone(
    shot: &crate::tone_target::Reading,
    goal: &crate::tone_target::Targets,
    now: &crate::tone_target::Reading,
) -> String {
    let mut parts = Vec::new();
    if let (Some(a), Some(b), Some(t)) = (shot.face, now.face, goal.face) {
        parts.push(format!("skin L* {a:.0} -> {b:.0} (photographer {t:.0})"));
    }
    for (name, i) in [
        ("shadows", 1),
        ("middle", 2),
        ("highlights", 3),
        ("white point", 4),
    ] {
        if let (Some(a), Some(b), Some(t)) = (shot.l.get(i), now.l.get(i), goal.l.get(i)) {
            parts.push(format!("{name} {a:.0} -> {b:.0} ({t:.0})"));
        }
    }
    parts.join(", ")
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
    let faces = faces_of(state, &input.project_id, photo);
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
        // Saturation is set only by stage 1's colour step (the foundation sets vibrance on
        // every run): a repeat run measures white balance and colour without the last one's.
        if !owned(&current, "global.saturation") {
            base.global.saturation = 0;
        }
    }
    let shown = measure(state, photo, &base);
    let Basics {
        mut balance,
        mut finish,
        shown,
    } = measured_basics(
        state,
        ids,
        &mut current,
        &mut base,
        shown,
        &faces,
        input.look,
        // A photograph the full edit finished before keeps its solved tone while it holds.
        before
            .extra
            .contains_key(KEY)
            .then(|| crate::tone_target::Sliders::of(&before.global)),
        &show,
    );
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
    for stage in &mut stages {
        match stage.key {
            FullStageKey::ColourCorrection => std::mem::take(&mut balance).into_stage(stage),
            FullStageKey::Basic => std::mem::take(&mut finish).into_stage(stage),
            _ => {}
        }
    }
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

    /// A 200x100 frame painted by a function of (x, y).
    fn painted(paint: impl Fn(usize, usize) -> [u8; 3]) -> (Vec<u8>, usize, usize) {
        let (w, h) = (200, 100);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                rgb.extend(paint(x, y));
            }
        }
        (rgb, w, h)
    }

    #[test]
    fn white_balance_solves_back_to_the_temperature_and_tint_it_came_from() {
        for (kelvin, tint) in [(5500_u32, 0_i16), (4300, 12), (6800, -20), (3400, 30)] {
            let (k, t) = solve_balance(balance_log(kelvin, tint));
            assert!(k.abs_diff(kelvin) <= 60, "{kelvin} -> {k}");
            assert!((t - tint).abs() <= 2, "{tint} -> {t}");
        }
    }

    #[test]
    fn a_warm_white_shirt_beside_a_red_sari_reads_the_warm_light_not_the_sari() {
        // Left: a cream shirt and its shading (warm light). Right: a saturated red sari.
        let (rgb, w, h) = painted(|x, y| {
            if x < 80 {
                let v = 150 + (y % 40) as u8;
                [v, v.saturating_sub(8), v.saturating_sub(26)]
            } else {
                [200, 30, 45]
            }
        });
        let refs = read_neutrals(&rgb, w, h, &[]).expect("the shirt is a reference");
        assert!(refs.cast.0 > 0.0 && refs.cast.1 < -0.1, "{:?}", refs.cast);
        assert!(refs.share > 0.3 && refs.share < 0.45, "{}", refs.share);
        // The same places read again on another render give the same cast.
        let again = cast_at(&rgb, &refs.at).expect("same places");
        assert!((again.0 - refs.cast.0).abs() < 1e-3 && (again.1 - refs.cast.1).abs() < 1e-3);
    }

    #[test]
    fn a_highlight_the_camera_clipped_is_not_a_witness_of_the_light() {
        // A blown shirt rebuilt as exact grey, and nothing else near-neutral.
        let (rgb, w, h) = painted(|x, _| {
            if x < 100 {
                [238, 238, 238]
            } else {
                [200, 30, 45]
            }
        });
        assert!(read_neutrals(&rgb, w, h, &[]).is_none());
        // Beside real references it is recorded as blown, to be read again on the checked render.
        let (rgb, w, h) = painted(|x, y| match x {
            0..=59 => [238, 238, 238],
            60..=139 => {
                let v = 140 + (y % 30) as u8;
                [v, v.saturating_sub(6), v.saturating_sub(20)]
            }
            _ => [200, 30, 45],
        });
        let refs = read_neutrals(&rgb, w, h, &[]).expect("the cloth is a reference");
        assert!(refs.blown_share > 0.25, "{}", refs.blown_share);
        assert!(
            refs.cast.1 < -0.05,
            "the clipped grey did not pull the cast to zero: {:?}",
            refs.cast
        );
        assert!(blown_cast(&rgb, &refs.blown).is_some_and(|c| c.0.hypot(c.1) < 1e-3));
    }

    #[test]
    fn faces_are_not_references() {
        // The whole frame is a pale warm tone; inside the face box it is skin.
        let face = [0.4_f32, 0.2, 0.6, 0.8];
        let (rgb, w, h) = painted(|x, y| {
            let (fx, fy) = (x as f32 / 200.0, y as f32 / 100.0);
            if in_boxes(&[face], fx, fy) {
                [220, 170, 140]
            } else {
                [180, 177, 172]
            }
        });
        let refs = read_neutrals(&rgb, w, h, &[face]).expect("the background is a reference");
        // The faint warm grey, not the skin: skin would read a cast many times this size.
        assert!(refs.cast.0.hypot(refs.cast.1) < 0.08, "{:?}", refs.cast);
        assert!(face_hue(&rgb, w, h, &[face]).is_some_and(skin_hue));
    }

    #[test]
    fn white_balance_keeps_what_the_photographer_keeps() {
        // Warm: goal is a fifth of the cast, multipliers take the rest off.
        let ((dr, db), goal) = balance_goal((0.2, -0.2), true);
        assert!(
            (goal.0 - 0.04).abs() < 1e-4 && (goal.1 + 0.04).abs() < 1e-4,
            "{goal:?}"
        );
        assert!((dr + 0.16).abs() < 1e-4 && (db - 0.16).abs() < 1e-4);
        // Cool: most of it is kept, as the photographer keeps it.
        let (_, goal) = balance_goal((-0.1, 0.1), true);
        assert!(
            (goal.0 + 0.07).abs() < 1e-4 && (goal.1 - 0.07).abs() < 1e-4,
            "{goal:?}"
        );
        // Magenta 0.2 (a pink canopy reads this way): half of at most 0.15 comes off.
        let (_, goal) = balance_goal((0.2, 0.2), true);
        assert!(
            (goal.0 - 0.125).abs() < 1e-4 && (goal.1 - 0.125).abs() < 1e-4,
            "{goal:?}"
        );
        // Without people, half of a warm light is kept: it is most often the picture.
        let (_, goal) = balance_goal((0.2, -0.2), false);
        assert!((goal.0 - 0.1).abs() < 1e-4, "{goal:?}");
        // Never more than the ceiling in one edit.
        let (step, _) = balance_goal((1.2, -1.2), true);
        assert!(step.0.hypot(step.1) <= BALANCE_CEILING + 1e-4);
    }

    #[test]
    fn every_part_of_a_frame_under_one_light_votes_for_it() {
        let (rgb, w, h) = painted(|x, y| {
            let v = 150 + ((x + y) % 20) as u8;
            [v, v.saturating_sub(10), v.saturating_sub(35)]
        });
        let one = read_neutrals(&rgb, w, h, &[]).expect("one light");
        assert!(
            one.votes == 9 && one.agreement >= MIXED_LIGHT_SHARE,
            "{} {}",
            one.votes,
            one.agreement
        );
        // A small part under another light, close enough to be measured with the rest, loses its vote.
        let (rgb, w, h) = painted(|x, y| {
            let v = 150 + ((x + y) % 20) as u8;
            if x < 70 {
                [v, v.saturating_sub(4), v.saturating_sub(4)]
            } else {
                [v, v.saturating_sub(10), v.saturating_sub(35)]
            }
        });
        let two = read_neutrals(&rgb, w, h, &[]).expect("two lights");
        assert!(
            two.agreement < 1.0 || two.votes < 9,
            "{} {}",
            two.votes,
            two.agreement
        );
    }
}
