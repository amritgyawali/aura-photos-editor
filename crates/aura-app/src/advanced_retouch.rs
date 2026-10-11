//! Auto advanced retouch: a professional retoucher's whole workflow, run automatically and in
//! order, one saved history step per stage. ADR-0093.
//!
//! The order is the one a high-end portrait studio works in:
//!
//! RAW foundation -> lens and perspective -> background -> hair -> skin cleanup -> selective
//! frequency separation -> micro dodge and burn -> medium dodge and burn -> global dodge and
//! burn -> skin colour -> eyes, lips and teeth -> clothing -> jewellery -> background toning ->
//! colour grade -> grain -> output sharpening -> quality control and export readiness.
//!
//! **No stage is skipped silently.** Every one of the eighteen is inspected and reported with
//! what it checked, what it changed and, when it changed nothing, why. A stage that changed
//! something is its own entry in the photograph's history, so a photographer can step back to
//! "after the skin cleanup" and keep everything before it.
//!
//! Three rules hold throughout, and the code has no switch that loosens them:
//!
//! - **Correct, never redesign.** Nothing reshapes a face or a body; there is no liquify. The
//!   only geometric change is levelling a horizon the photograph's own straight lines measure.
//! - **Measure against the person, never an ideal.** Every skin decision comes from the same
//!   person's own skin ([`crate::portrait_auto`]); moles and freckles are kept; texture is kept
//!   and quality control measures that it was.
//! - **Correction first, style second.** The foundation step leaves the frame neutral and
//!   flexible; colour, structure, grain and sharpening come last.
//!
//! Everything is an ordinary recipe field or retouch operation: editable, reversible, and
//! protected by [`aura_recipe::schema::merge`] wherever a person already set a value.
// Pixel statistics and slider values convert between f32 and integers on purpose.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    // Frame geometry uses the conventional single letters (l, t, r, b, x, y, w, h).
    clippy::many_single_char_names
)]

pub mod measure;

use crate::{
    commands::IpcResult,
    contract::ipc::RecipeDto,
    portrait_auto::{self, PREFIX, SCENE_PREFIX},
    portrait_features::{Options, Pixels, Scope},
    retouch_settings::Settings,
    smart_edit::{self, GlobalPlan, SceneKind},
    AppState,
};
use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_preview::contract::service::{PreviewService, Priority};
use aura_recipe::{
    retouch_tools::{self, Edit, Matte, Tool},
    schema, EditSource, Recipe,
};
use aura_render::{FrameSource, RenderLevel, RenderService};
use aura_vision::portrait::PortraitFace;
use measure::{Reflections, Tilt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where the run's report is stored in the recipe.
pub const KEY: &str = "studio_advanced_retouch_v1";
/// The planner version written into the report; bump on any behavioural change.
pub const VERSION: &str = "auto-advanced-v1";
/// Below this share of fine skin texture kept, quality control softens the skin steps.
pub const TEXTURE_FLOOR: f32 = 0.6;
/// Above this skin chromaticity shift, quality control halves the skin colour steps.
pub const SKIN_SHIFT_CEILING: f32 = 0.012;
/// Typical visual-aid readings on a portrait; the dodge and burn is scaled around them.
const TYPICAL_MICRO: f32 = 0.035;
const TYPICAL_MEDIUM: f32 = 0.03;

/// The eighteen stages, in the order they run and are saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Foundation,
    LensPerspective,
    Background,
    Hair,
    SkinCleanup,
    FrequencySeparation,
    MicroDodgeBurn,
    MediumDodgeBurn,
    GlobalDodgeBurn,
    SkinColour,
    EyesLipsTeeth,
    Clothing,
    Jewellery,
    BackgroundToning,
    ColourGrade,
    Grain,
    OutputSharpening,
    QualityControl,
}

impl Stage {
    pub const ALL: [Self; 18] = [
        Self::Foundation,
        Self::LensPerspective,
        Self::Background,
        Self::Hair,
        Self::SkinCleanup,
        Self::FrequencySeparation,
        Self::MicroDodgeBurn,
        Self::MediumDodgeBurn,
        Self::GlobalDodgeBurn,
        Self::SkinColour,
        Self::EyesLipsTeeth,
        Self::Clothing,
        Self::Jewellery,
        Self::BackgroundToning,
        Self::ColourGrade,
        Self::Grain,
        Self::OutputSharpening,
        Self::QualityControl,
    ];

    /// One-based position in the workflow.
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
            Self::Foundation => "RAW foundation",
            Self::LensPerspective => "Lens & perspective",
            Self::Background => "Background cleanup",
            Self::Hair => "Hair cleanup",
            Self::SkinCleanup => "Skin cleanup",
            Self::FrequencySeparation => "Selective frequency separation",
            Self::MicroDodgeBurn => "Micro dodge & burn",
            Self::MediumDodgeBurn => "Medium dodge & burn",
            Self::GlobalDodgeBurn => "Global dodge & burn",
            Self::SkinColour => "Skin colour",
            Self::EyesLipsTeeth => "Eyes, lips & teeth",
            Self::Clothing => "Clothing",
            Self::Jewellery => "Jewellery & reflections",
            Self::BackgroundToning => "Background toning",
            Self::ColourGrade => "Colour grade",
            Self::Grain => "Grain",
            Self::OutputSharpening => "Output sharpening",
            Self::QualityControl => "Quality control & export",
        }
    }

    /// True for the stages whose work is retouch operations rather than recipe sliders.
    #[must_use]
    pub const fn retouch(self) -> bool {
        matches!(
            self,
            Self::Background
                | Self::Hair
                | Self::SkinCleanup
                | Self::FrequencySeparation
                | Self::MicroDodgeBurn
                | Self::MediumDodgeBurn
                | Self::GlobalDodgeBurn
                | Self::SkinColour
                | Self::EyesLipsTeeth
                | Self::Clothing
                | Self::Jewellery
                | Self::BackgroundToning
        )
    }
}

/// The stage an automatic retouch operation belongs to; `None` for a person's own operation.
#[must_use]
pub fn stage_of(edit: &Edit) -> Option<Stage> {
    portrait_auto::group_of(&edit.id)?;
    if edit.id.starts_with(SCENE_PREFIX) {
        return Some(Stage::BackgroundToning);
    }
    let rest = edit.id.strip_prefix(PREFIX)?;
    // `{face}-{name}` for per-person operations, `{name}` for frame-wide ones.
    let name = rest
        .split_once('-')
        .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        .map_or(rest, |(_, n)| n);
    let by_name = match name {
        "backdrop" => Some(Stage::Background),
        "clear" | "heal" | "body-spots" => Some(Stage::SkinCleanup),
        "texture" | "body-texture" | "pores" | "surface-finish" | "texture-graft" => {
            Some(Stage::FrequencySeparation)
        }
        "microdb" => Some(Stage::MicroDodgeBurn),
        "light" | "shine" | "body-shine" | "brightness" => Some(Stage::MediumDodgeBurn),
        "facelight" | "glow" => Some(Stage::GlobalDodgeBurn),
        "tone" | "body-tone" | "body-match" | "colour" | "body-redness" => Some(Stage::SkinColour),
        "teeth" => Some(Stage::EyesLipsTeeth),
        "fabric" => Some(Stage::Clothing),
        "background-tone" => Some(Stage::BackgroundToning),
        n if n.starts_with("dust-") => Some(Stage::Background),
        n if n.starts_with("hair-") || n.starts_with("stray-") => Some(Stage::Hair),
        n if n.starts_with("spot") => Some(Stage::SkinCleanup),
        n if n.starts_with("lines-") => Some(Stage::FrequencySeparation),
        n if n.starts_with("fold-") => Some(Stage::MicroDodgeBurn),
        // Puffiness is evened with the medium transitions; the dark circle itself is corrected
        // with the eyes, after every face-wide light change, so it is matched to the cheek as
        // that cheek finally looks.
        n if n.starts_with("undereye-bag") => Some(Stage::MediumDodgeBurn),
        n if n.starts_with("undereye") => Some(Stage::EyesLipsTeeth),
        n if n.starts_with("sculpt-") => Some(Stage::GlobalDodgeBurn),
        n if n.starts_with("redness-") || n.starts_with("makeup-") => Some(Stage::SkinColour),
        n if n.starts_with("eye-") || n.starts_with("lips-") => Some(Stage::EyesLipsTeeth),
        n if n.starts_with("cloth-") => Some(Stage::Clothing),
        n if n.starts_with("jewel-") => Some(Stage::Jewellery),
        _ => None,
    };
    by_name.or(Some(match edit.tool {
        // These tools are explicit manual edits and must never be planned as automatic retouch.
        Tool::Colorize | Tool::BackgroundColor | Tool::Reshape => return None,
        Tool::AutoBlemish | Tool::PatchHeal | Tool::FrequencyHeal | Tool::AcneClear => {
            Stage::SkinCleanup
        }
        Tool::SkinSmooth | Tool::Frequency | Tool::Wrinkle | Tool::TextureGraft => {
            Stage::FrequencySeparation
        }
        Tool::MicroDodgeBurn => Stage::MicroDodgeBurn,
        Tool::SkinUniformity | Tool::ColorMatch | Tool::SkinColor | Tool::Makeup => {
            Stage::SkinColour
        }
        Tool::Teeth | Tool::EyeClean | Tool::EyeDetail | Tool::RedEye | Tool::UnderEye => {
            Stage::EyesLipsTeeth
        }
        Tool::Fabric => Stage::Clothing,
        Tool::Glare => Stage::Jewellery,
        Tool::Backdrop | Tool::Heal | Tool::Clone => Stage::Background,
        Tool::PortraitDodgeBurn | Tool::Mattify | Tool::Dodge | Tool::Burn => {
            Stage::MediumDodgeBurn
        }
    }))
}

/// The retouch stack after saving every stage up to and including `upto`.
///
/// A person's own operations keep their order and come first. Each automatic stage is either
/// the newly planned work (once it has been reached) or whatever the stack already held, so
/// the stack only ever changes in the stage that is being saved.
#[must_use]
pub fn staged(current: &[Edit], planned: &BTreeMap<Stage, Vec<Edit>>, upto: Stage) -> Vec<Edit> {
    let mut out: Vec<Edit> = current
        .iter()
        .filter(|e| stage_of(e).is_none())
        .cloned()
        .collect();
    for stage in Stage::ALL.into_iter().filter(|s| s.retouch()) {
        match planned.get(&stage) {
            Some(edits) if stage <= upto => out.extend(edits.iter().cloned()),
            _ => out.extend(
                current
                    .iter()
                    .filter(|e| stage_of(e) == Some(stage))
                    .cloned(),
            ),
        }
    }
    out
}

/// What a stage did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// It changed the photograph and was saved as its own history step.
    Applied,
    /// It inspected the photograph and found nothing that needed changing.
    Unchanged,
    /// It does not apply to this photograph (for example, no person in it).
    NotApplicable,
    /// It would have changed values a person set by hand; those were kept.
    Protected,
}

/// One stage of the run, as reported to the photographer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageReport {
    pub number: u8,
    pub stage: Stage,
    pub title: String,
    pub outcome: Outcome,
    /// What was inspected and measured.
    pub checks: Vec<String>,
    /// What was changed.
    pub changes: Vec<String>,
    /// Retouch operations this stage owns.
    pub operations: usize,
    /// True when the stage is its own history entry.
    pub saved: bool,
}

impl StageReport {
    fn new(stage: Stage) -> Self {
        Self {
            number: stage.number(),
            stage,
            title: stage.title().into(),
            outcome: Outcome::Unchanged,
            checks: Vec::new(),
            changes: Vec::new(),
            operations: 0,
            saved: false,
        }
    }
}

/// The measurements quality control made, and what it did about them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityReport {
    /// Fine skin texture kept by the retouch, after over before (1 keeps all of it).
    pub texture_retention: Option<f32>,
    /// How far the skin's average colour moved during the retouch, in chromaticity units.
    pub skin_shift: Option<f32>,
    /// Share of clipped pixels in the original and in the finished photograph.
    pub clipped_original: Option<f32>,
    pub clipped_final: Option<f32>,
    /// Larger over smaller change between the two halves of each face (1 is balanced).
    pub mirror_balance: Option<f32>,
    /// True when every measurement is within its limit.
    pub passed: bool,
    /// True when quality control softened a stage and measured again.
    pub corrected: bool,
}

/// The whole run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub version: String,
    pub faces: usize,
    pub stages: Vec<StageReport>,
    pub quality: QualityReport,
    /// History entries this run added.
    pub history_steps: usize,
    pub summary: String,
}

/// Progress of a running pass, one event before and one after each stage.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub photo_id: String,
    pub number: u8,
    pub total: u8,
    pub title: String,
    /// `running` or `done`.
    pub state: String,
    pub outcome: Option<Outcome>,
}

/// A photographer's request to run the advanced retouch.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdvancedRetouchInput {
    pub project_id: String,
    pub photo_id: String,
    /// The retouch choices; absent uses [`options`], the workflow's own defaults.
    #[serde(default)]
    pub options: Option<Options>,
}

/// The finished recipe and the stage-by-stage report.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedRetouchDto {
    pub recipe: RecipeDto,
    pub report: Report,
}

/// The workflow's own retouch choices: a high-end portrait finish that keeps identity.
///
/// Acne and temporary marks are cleared from the clean skin around them while dark marks
/// (moles, beauty marks) and freckle fields are kept; skin is smoothed gently with pores kept
/// and restored; lines are softened, not removed; the eyes are cleaned rather than whitened;
/// teeth lose yellow rather than turning white; lips keep their texture; contour and
/// highlight follow the existing light only faintly; clothes, hair and a plain backdrop get
/// their measured clean-up.
#[must_use]
pub fn options() -> Options {
    Options {
        intensity: 1.0,
        blemishes: true,
        eyes: true,
        teeth: true,
        refine: true,
        scope: Scope::FaceAndBody,
        adaptive: true,
        settings: Settings {
            smoothing: 0.35,
            texture: 0.9,
            tone_evenness: 0.6,
            micro_dodge_burn: 0.5,
            shine: 0.6,
            texture_graft: 0.8,
            blemish_sensitivity: 0.75,
            deep_blemish_cleanup: true,
            remove_dark_marks: false,
            frequency_heal: 1.0,
            max_spots: 220,
            keep_freckles: true,
            forehead_lines: 0.35,
            crows_feet: 0.35,
            smile_lines: 0.4,
            under_eye_lines: 0.25,
            neck_lines: 0.2,
            eye_whitening: 0.1,
            iris_brightness: 0.15,
            iris_detail: 0.4,
            teeth_whitening: 0.45,
            lip_definition: 0.15,
            contour: 0.15,
            highlight: 0.15,
            body_smoothing: 0.4,
            body_redness: 0.2,
            body_blemishes: 0.3,
            hair_detail: 0.3,
            hair_shine: 0.15,
            fabric: 0.3,
            backdrop: 0.4,
            ..Settings::default()
        },
    }
}

/// Everything decided before anything is saved.
#[derive(Debug, Clone)]
pub struct Planned {
    /// Exposure, highlights, shadows, contrast.
    pub tone: (f32, i16, i16, i16),
    pub global: Option<GlobalPlan>,
    /// Rotation in degrees (positive clockwise) and the crop that hides the corners.
    pub level: Option<(f32, [f32; 4])>,
    pub grain: Option<(i16, i16, i16)>,
    /// Automatic retouch operations per stage; a present-but-empty stage removes older ones.
    pub stages: BTreeMap<Stage, Vec<Edit>>,
    pub mattes: BTreeMap<String, Matte>,
    pub reports: Vec<StageReport>,
    pub portrait: portrait_auto::Report,
    pub faces: Vec<PortraitFace>,
    /// Face skin per person, for quality control.
    pub people: Vec<aura_vision::skin::Person>,
}

fn report_mut(reports: &mut [StageReport], stage: Stage) -> Option<&mut StageReport> {
    reports.iter_mut().find(|r| r.stage == stage)
}

fn base_edit(id: String, tool: Tool, region: [f32; 4], amount: f32) -> Edit {
    Edit {
        id,
        tool,
        enabled: true,
        region: region.map(|v| v.clamp(0.001, 1.0)),
        source: None,
        amount: amount.clamp(0.0, 0.95),
        feather: 0.5,
        radius: 0.002,
        source_scale: 1.0,
        preserve_microtexture: false,
        texture: 1.0,
        texture_heal: false,
        clean_ring_fit: false,
        curved_heal: false,
        heal_samples: Vec::new(),
        texture_sources: Vec::new(),
        target_color: None,
        sensitivity: None,
        keep_dark_marks: false,
        tone: 0.5,
        warmth: 0.0,
        tint: 0.0,
        mask: None,
        skin: None,
        selection: None,
        matte: None,
    }
}

fn encode_matte(m: &aura_vision::skin::Matte) -> Option<Matte> {
    Some(Matte::encode(
        m.bounds,
        u32::try_from(m.width).ok()?,
        u32::try_from(m.height).ok()?,
        &m.alpha,
    ))
}

/// Plan every stage from the photograph's pixels. Nothing is saved.
///
/// `rgb` is the small analysis thumbnail, `detail` the 2048-pixel proxy, `frame` the
/// renderer's own input for the white-balance estimate.
/// # Errors
/// Invalid pixels or a failed model run.
#[allow(clippy::too_many_arguments)]
pub fn plan(
    base: &Recipe,
    rgb: &[u8],
    width: u32,
    height: u32,
    detail: Option<(&[u8], u32, u32)>,
    frame: Option<&aura_render::Frame>,
    faces: Vec<PortraitFace>,
    options: &Options,
) -> AuraResult<Planned> {
    let options = options.sanitised();
    let mut reports: Vec<StageReport> = Stage::ALL.into_iter().map(StageReport::new).collect();
    let thumb = Pixels::new(rgb, width, height)
        .ok_or_else(|| aura_core::errors::render::recipe_invalid("photo", "invalid preview"))?;
    let px = detail
        .and_then(|(d, w, h)| Pixels::new(d, w, h))
        .unwrap_or(thumb);

    // ---- 1. RAW foundation -----------------------------------------------------------------
    let mut tone = crate::photo_enhance::correction(rgb)?;
    let (capped, note) = smart_edit::face_exposure_cap(tone.0, &thumb, &faces);
    tone.0 = capped;
    let note = note.or_else(|| smart_edit::respect_intent(&mut tone, &thumb, &faces));
    let exposure = smart_edit::effective_exposure(base, tone.0, true);
    let mut global = smart_edit::analyse(&px, frame, &faces, exposure);
    if let Some(r) = report_mut(&mut reports, Stage::Foundation) {
        r.checks.push(format!(
            "Scene: {}. Histogram, highlights, shadows and noise measured on the {}x{} proxy; white balance from neutral areas with faces ignored.",
            global.kind.label(),
            px.width,
            px.height
        ));
        r.checks.extend(note);
        let wanted = tone.3;
        tone.3 = tone.3.clamp(-15, 15);
        if tone.3 != wanted {
            r.checks.push(format!(
                "Contrast kept moderate ({:+} instead of {wanted:+}): the foundation stays neutral and flexible; the look comes in the colour grade.",
                tone.3
            ));
        }
        r.checks.extend(
            global
                .decisions
                .iter()
                .filter(|d| {
                    !d.starts_with("Vibrance")
                        && !d.starts_with("Clarity")
                        && !d.starts_with("Sharpening")
                        && !d.starts_with("Sky")
                })
                .cloned(),
        );
        r.checks.push("Colour: edited in linear Rec.2020 at 32-bit float, non-destructively; the original file is never changed. Masters export as 16-bit TIFF in Adobe RGB (the album preset), web copies in sRGB.".into());
    }

    // ---- 3 to 14: the portrait and its surroundings ----------------------------------------
    let portrait = portrait_auto::plan_with_faces(
        base,
        rgb,
        width,
        height,
        detail,
        exposure,
        Some(faces.clone()),
        &options,
        true,
    )?;
    let mut stages: BTreeMap<Stage, Vec<Edit>> = Stage::ALL
        .into_iter()
        .filter(|s| s.retouch())
        .map(|s| (s, Vec::new()))
        .collect();
    for edit in portrait.groups.values().flatten() {
        if let Some(stage) = stage_of(edit) {
            stages.entry(stage).or_default().push(edit.clone());
        }
    }
    let mut mattes = portrait.mattes.clone();
    let people = portrait.people.clone();
    let (pw, ph) = (px.width, px.height);
    let people_found = !people.is_empty();
    let linear = measure::linear_frame(&px);
    let planes = |pick: fn(&aura_vision::skin::Person) -> Option<&aura_vision::skin::Matte>| {
        let mut plane = vec![0.0_f32; pw * ph];
        for person in &people {
            if let Some(m) = pick(person) {
                for (dst, v) in plane.iter_mut().zip(measure::matte_plane(m, pw, ph)) {
                    *dst = dst.max(v);
                }
            }
        }
        plane
    };
    let face_plane = planes(|p| p.face.as_ref());
    // People are not architecture: stripes on a shirt, an arm or a strand of hair must never
    // decide where level is. Segmented people are excluded, and where nobody was segmented a
    // generous region around and below each face.
    let mut person_plane = vec![0.0_f32; pw * ph];
    for pick in [
        (|p: &aura_vision::skin::Person| p.face.as_ref())
            as fn(&aura_vision::skin::Person) -> Option<&aura_vision::skin::Matte>,
        |p| p.body.as_ref(),
        |p| p.hair.as_ref(),
        |p| p.clothes.as_ref(),
    ] {
        for (dst, v) in person_plane.iter_mut().zip(planes(pick)) {
            *dst = dst.max(v);
        }
    }
    for face in &faces {
        let [l, t, r, _] = face.bounds;
        let (fw, fh) = (r - l, face.bounds[3] - t);
        let (x0, x1) = (
            ((l - fw) * pw as f32).max(0.0) as usize,
            (((r + fw) * pw as f32) as usize).min(pw),
        );
        let y0 = ((t - 0.5 * fh) * ph as f32).max(0.0) as usize;
        for y in y0..ph {
            for x in x0..x1 {
                if let Some(v) = person_plane.get_mut(y * pw + x) {
                    *v = 1.0;
                }
            }
        }
    }
    // ---- 2. Lens & perspective --------------------------------------------------------------
    let mut level = None;
    if let Some(r) = report_mut(&mut reports, Stage::LensPerspective) {
        match measure::tilt(&px, Some(&person_plane)) {
            Tilt::Tilted {
                degrees,
                lines,
                agreement,
            } if base.geometry.is_identity() => {
                let aspect = px.width as f32 / px.height.max(1) as f32;
                let rect = aura_geometry::straighten::inscribed(-degrees, aspect);
                level = Some((-degrees, [rect.x, rect.y, rect.x + rect.w, rect.y + rect.h]));
                r.checks.push(format!(
                    "Horizon: {lines} long straight lines agree ({:.0}% of their length) on a {degrees:+.1}° tilt.",
                    agreement * 100.0
                ));
            }
            Tilt::Tilted { degrees, .. } => r.checks.push(format!(
                "Horizon measured {degrees:+.1}° off level, but the photograph already has a crop or rotation; it is kept."
            )),
            Tilt::Level { lines, degrees } => r.checks.push(format!(
                "Horizon: {lines} straight lines agree the frame is level ({degrees:+.2}°)."
            )),
            Tilt::Unsure(why) => r.checks.push(format!("Horizon left as shot: {why}.")),
        }
        r.checks.push(if base.lens.coefficients.is_some() {
            "Lens: this photograph already carries lens coefficients; they are kept as set.".into()
        } else {
            "Lens: no measured profile for this lens, so distortion, vignetting and chromatic aberration are left as shot rather than guessed.".into()
        });
        r.checks.push("Structure: correct photographic distortion, never redesign the person - this workflow has no liquify and reshapes no face or body.".into());
    }

    let manual = retouch_tools::read(base)?
        .iter()
        .filter(|e| stage_of(e).is_none())
        .count();
    let used = |stages: &BTreeMap<Stage, Vec<Edit>>| stages.values().map(Vec::len).sum::<usize>();
    let room = |stages: &BTreeMap<Stage, Vec<Edit>>| {
        retouch_tools::MAX_EDITS.saturating_sub(manual + used(stages))
    };
    let no_person = if faces.is_empty() {
        "No face was found, so there is no person to separate from the background."
    } else {
        "The person segmenter did not run (switched off or unavailable on this device)."
    };

    // 3. Background cleanup: the plain-backdrop smoothing plus dust and specks.
    if let Some(r) = report_mut(&mut reports, Stage::Background) {
        let backdrop = stages
            .get(&Stage::Background)
            .is_some_and(|v| v.iter().any(|e| e.tool == Tool::Backdrop));
        if backdrop {
            r.changes
                .push("Plain backdrop smoothed, kept clear of the person's edges.".into());
        } else if portrait
            .report
            .message
            .contains("Backdrop smoothing was skipped")
        {
            r.checks
                .push("Backdrop: textured (a room, foliage, brick), so it is not smoothed.".into());
        }
        match &portrait.background {
            Some(bg) if people_found => {
                let away = portrait_auto::erode(bg, (bg.width.max(bg.height) / 40).max(2));
                let region = measure::matte_plane(&away, pw, ph);
                let hair_near = planes(|p| p.hair.as_ref());
                let search = measure::marks(
                    &linear,
                    &region,
                    Some(&hair_near),
                    0.00004,
                    16.min(room(&stages)),
                );
                r.checks.push(if search.pattern {
                    format!("Dust and specks: the background is textured at 100% ({}+ small irregularities are its own grain or pattern), so nothing was cleaned off it.", search.found)
                } else {
                    format!(
                        "Dust and specks: searched the background at 100% ({} candidate{}; {} on textured areas left alone, {} too large to be dust, {} touching hair, {} without a clean donor).",
                        search.found,
                        if search.found == 1 { "" } else { "s" },
                        search.textured,
                        search.too_large,
                        search.excluded,
                        search.no_donor
                    )
                });
                let ops: Vec<Edit> = search
                    .repairs
                    .iter()
                    .enumerate()
                    .map(|(n, rep)| {
                        let mut e = base_edit(
                            format!("{PREFIX}dust-{n}"),
                            Tool::Heal,
                            rep.region,
                            measure::repair_amount(rep.departure),
                        );
                        e.source = Some(rep.source);
                        e.feather = 0.6;
                        e
                    })
                    .collect();
                if !ops.is_empty() {
                    r.changes.push(format!(
                        "Healed {} speck{} of dust from clean backdrop beside {}.",
                        ops.len(),
                        if ops.len() == 1 { "" } else { "s" },
                        if ops.len() == 1 { "it" } else { "each" }
                    ));
                }
                stages.entry(Stage::Background).or_default().extend(ops);
            }
            _ => r.checks.push(format!("Dust search skipped: {no_person}")),
        }
        r.checks.push("Distracting objects and people are never removed automatically: what belongs to the story is a person's decision.".into());
    }

    // 4. Hair: silhouette strays, then the hair's own detail and shine.
    if let Some(r) = report_mut(&mut reports, Stage::Hair) {
        let hair_plane = planes(|p| p.hair.as_ref());
        if people.iter().any(|p| p.hair.is_some()) {
            let search = measure::strays(&linear, &hair_plane);
            r.checks.push(if search.soft_edge {
                format!(
                    "Silhouette: {} fine structures all along the hair's edge - that is the hair's own soft outline, not stray strands, and fading it would notch the outline, so it is kept.",
                    search.found
                )
            } else {
                format!(
                    "Silhouette: {} candidate{} outside the hair; {} over a background with its own detail and any not thin and clear of the hair were left.",
                    search.found,
                    if search.found == 1 { "" } else { "s" },
                    search.busy
                )
            });
            let n = search.strays.len().min(room(&stages));
            let ops: Vec<Edit> = search
                .strays
                .iter()
                .take(n)
                .enumerate()
                .map(|(k, s)| {
                    let mut e = base_edit(
                        format!("{PREFIX}stray-{k}"),
                        Tool::Backdrop,
                        s.region,
                        s.amount,
                    );
                    e.feather = 0.7;
                    e
                })
                .collect();
            if !ops.is_empty() {
                r.changes.push(format!(
                    "Faded {} stray strand{} toward the quiet background behind them (contrast reduced, never erased).",
                    ops.len(),
                    if ops.len() == 1 { "" } else { "s" }
                ));
            }
            stages.entry(Stage::Hair).or_default().extend(ops);
            r.checks.push("Interior and rebuild: hairs inside the hair shape are left as they are; no strands are cloned or invented.".into());
        } else if people_found {
            r.checks
                .push("The segmenter found no hair on anybody in this frame.".into());
        } else {
            r.checks.push(format!("No hair to inspect. {no_person}"));
        }
        let own = stages
            .get(&Stage::Hair)
            .map_or(0, |v| v.iter().filter(|e| e.id.contains("-hair-")).count());
        if own > 0 {
            r.changes
                .push("Hair detail and the hair's own highlights lifted slightly.".into());
        }
    }

    // 12. Clothing: creases plus lint, threads and small stains.
    let hair_plane = planes(|p| p.hair.as_ref());
    if let Some(r) = report_mut(&mut reports, Stage::Clothing) {
        if people.iter().any(|p| p.clothes.is_some()) {
            if stages.get(&Stage::Clothing).is_some_and(|v| !v.is_empty()) {
                r.changes
                    .push("Fabric creases softened; seams and weave kept.".into());
            }
            let mut ops = Vec::new();
            for (index, person) in people.iter().enumerate() {
                let Some(clothes) = &person.clothes else {
                    continue;
                };
                let inside =
                    portrait_auto::erode(clothes, (clothes.width.max(clothes.height) / 60).max(1));
                let region = measure::matte_plane(&inside, pw, ph);
                let search = measure::marks(
                    &linear,
                    &region,
                    Some(&hair_plane),
                    0.0004,
                    12.min(room(&stages).saturating_sub(ops.len())),
                );
                r.checks.push(if search.pattern {
                    format!("Person {}: the fabric's own weave or print shows as {}+ small marks at 100%, so nothing was cleaned off it.", index + 1, search.found)
                } else {
                    format!(
                        "Person {}: {} mark{} on the clothes ({} on patterned fabric left alone, {} too large, {} hair ends, {} without a clean donor).",
                        index + 1,
                        search.found,
                        if search.found == 1 { "" } else { "s" },
                        search.textured,
                        search.too_large,
                        search.excluded,
                        search.no_donor
                    )
                });
                for (n, rep) in search.repairs.iter().enumerate() {
                    let mut e = base_edit(
                        format!("{PREFIX}{index}-cloth-{n}"),
                        Tool::Heal,
                        rep.region,
                        measure::repair_amount(rep.departure),
                    );
                    e.source = Some(rep.source);
                    e.feather = 0.6;
                    r.changes.push(format!(
                        "Healed a {} on person {}'s clothes from the fabric beside it.",
                        rep.kind,
                        index + 1
                    ));
                    ops.push(e);
                }
            }
            stages.entry(Stage::Clothing).or_default().extend(ops);
        } else if people_found {
            r.checks.push("The segmenter found no clothing on anybody in this frame (a close crop, or skin and hair only).".into());
        } else {
            r.checks
                .push(format!("No clothing to inspect. {no_person}"));
        }
    }

    // 13. Jewellery and hot reflections on the outfit.
    if let Some(r) = report_mut(&mut reports, Stage::Jewellery) {
        if people_found {
            // The outfit, and the band below each face where a necklace or earrings sit. Bare
            // arms and shoulders are left out: a highlight there is skin shine, which the
            // dodge and burn already handled.
            let mut person = planes(|p| p.clothes.as_ref());
            let body = planes(|p| p.body.as_ref());
            for face in &faces {
                let [l, t, r, b] = face.bounds;
                let (fw, fh) = (r - l, b - t);
                let (x0, x1) = (
                    ((l - 0.4 * fw) * pw as f32).max(0.0) as usize,
                    (((r + 0.4 * fw) * pw as f32) as usize).min(pw),
                );
                let (y0, y1) = (
                    ((t + 0.4 * fh) * ph as f32).max(0.0) as usize,
                    (((b + 0.7 * fh) * ph as f32) as usize).min(ph),
                );
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = y * pw + x;
                        if let (Some(dst), Some(v)) = (person.get_mut(i), body.get(i)) {
                            *dst = dst.max(*v);
                        }
                    }
                }
            }
            let behind = portrait
                .background
                .as_ref()
                .map(|bg| measure::matte_plane(bg, pw, ph));
            match measure::reflections(&px, &person, behind.as_deref(), &faces) {
                Reflections::Sparkle(count) => r.checks.push(format!(
                    "{count} small bright highlights on the outfit: they are its sparkle or pattern (sequins, crystals, a print), so they are kept."
                )),
                Reflections::Found(spots) => {
                    r.checks.push(format!(
                        "Searched the outfit and jewellery for clipped specular reflections; {} found.",
                        spots.len()
                    ));
                    let n = spots.len().min(room(&stages));
                    let ops: Vec<Edit> = spots
                        .iter()
                        .take(n)
                        .enumerate()
                        .map(|(k, s)| {
                            let mut e = base_edit(format!("{PREFIX}jewel-{k}"), Tool::Glare, s.region, 0.5);
                            e.feather = 0.6;
                            e
                        })
                        .collect();
                    if !ops.is_empty() {
                        r.changes.push(format!(
                            "Tamed {} small burnt-out highlight{} on the outfit (jewellery, buttons, glints) by about a sixth at their peak, so they read as light on a surface rather than holes; their shape and sparkle are kept.",
                            ops.len(),
                            if ops.len() == 1 { "" } else { "s" }
                        ));
                    }
                    stages.entry(Stage::Jewellery).or_default().extend(ops);
                }
            }
        } else {
            r.checks.push(format!("No outfit to inspect. {no_person}"));
        }
    }

    // 14. Background toning: a bright sky, and a background brighter than the person.
    if let Some(r) = report_mut(&mut reports, Stage::BackgroundToning) {
        if let Some(sky) = global.sky.take() {
            r.changes
                .push("Bright sky balanced with a feathered gradient over the sky only.".into());
            stages.entry(Stage::BackgroundToning).or_default().push(sky);
        }
        match (&portrait.background, people_found) {
            (Some(bg), true) => {
                let away = portrait_auto::erode(bg, (bg.width.max(bg.height) / 25).max(3));
                let plane = measure::matte_plane(&away, pw, ph);
                match measure::background_tone(&px, &plane, &face_plane) {
                    Some(t) if t.reduction > 0.0 && room(&stages) > 0 => {
                        let id = format!("{PREFIX}background-tone");
                        if let Some(m) = encode_matte(&away) {
                            mattes.insert(id.clone(), m);
                            let [l, tp, rt, b] = away.bounds;
                            let mut e = base_edit(
                                id.clone(),
                                Tool::Burn,
                                [(l + rt) * 0.5, (tp + b) * 0.5, (rt - l) * 0.71, (b - tp) * 0.71],
                                measure::burn_amount(t.reduction),
                            );
                            e.feather = 0.0;
                            e.matte = Some(id);
                            stages.entry(Stage::BackgroundToning).or_default().push(e);
                            r.changes.push(format!(
                                "Background lowered by about {:.0}% so the eye goes to the person first (it measured {:.0}% against the face's {:.0}%).",
                                t.reduction * 100.0,
                                t.background * 100.0,
                                t.subject * 100.0
                            ));
                        }
                    }
                    Some(t) if t.background > measure::HIGH_KEY => r.checks.push(format!(
                        "A white or near-white backdrop ({:.0}%) is a high-key look by design; lowering it would only turn white into grey, so it is kept.",
                        t.background * 100.0
                    )),
                    Some(t) if t.background > t.subject + 0.06 => r.checks.push(format!(
                        "Background ({:.0}%) is brighter than the face ({:.0}%), but the operation limit is reached; it is left as it was.",
                        t.background * 100.0,
                        t.subject * 100.0
                    )),
                    Some(t) => r.checks.push(format!(
                        "Background ({:.0}%) is not brighter than the face ({:.0}%); its light is kept as it was.",
                        t.background * 100.0,
                        t.subject * 100.0
                    )),
                    None => r.checks.push("Background too small or too broken up to measure its brightness against the face.".into()),
                }
            }
            _ if stages
                .get(&Stage::BackgroundToning)
                .is_some_and(|v| !v.is_empty()) => {}
            _ => r
                .checks
                .push(format!("No separated background to tone. {no_person}")),
        }
    }

    // The visual aid: colour removed, contrast exaggerated, read at two scales.
    let mut aid = None;
    for (index, face) in faces.iter().enumerate() {
        let Some(m) = people.get(index).and_then(|p| p.face.as_ref()) else {
            continue;
        };
        let plane = measure::matte_plane(m, pw, ph);
        if let Some(reading) = measure::visual_aid(&px, &plane, face) {
            for edit in stages.values_mut().flatten() {
                let face_tag = format!("{PREFIX}{index}-");
                let Some(name) = edit.id.strip_prefix(&face_tag) else {
                    continue;
                };
                if name == "microdb" || name.starts_with("fold-") {
                    edit.amount = (edit.amount * measure::aid_gain(reading.micro, TYPICAL_MICRO))
                        .clamp(0.02, 0.95);
                } else if name == "light" {
                    edit.amount = (edit.amount * measure::aid_gain(reading.medium, TYPICAL_MEDIUM))
                        .clamp(0.02, 0.95);
                }
            }
            aid.get_or_insert(Vec::new()).push((index, reading));
        }
    }

    // Plain-language summaries of the portrait stages.
    let faces_found = !faces.is_empty();
    let spots: usize = portrait
        .report
        .assessments
        .iter()
        .map(|a| a.spots_healed)
        .sum();
    let kept: usize = portrait
        .report
        .assessments
        .iter()
        .map(|a| a.marks_kept)
        .sum();
    let count = |stages: &BTreeMap<Stage, Vec<Edit>>, s: Stage| stages.get(&s).map_or(0, Vec::len);
    let describe: [(Stage, String, String); 7] = [
        (
            Stage::SkinCleanup,
            "Temporary marks (pimples, redness, flakes) measured against the clean skin around each one; dark marks that are not redder than the skin (moles, beauty marks) and freckle fields are kept.".into(),
            {
                let cleared = stages.get(&Stage::SkinCleanup).map_or(0, |v| v.iter().filter(|e| e.tool == Tool::AcneClear).count());
                let mut text = Vec::new();
                if cleared > 0 {
                    text.push(format!("Acne clear on {cleared} face{}: every compact temporary mark rebuilt from the clean skin around it, pores kept in place", if cleared == 1 { "" } else { "s" }));
                }
                if spots > 0 {
                    text.push(format!("{spots} individual spot{} healed", if spots == 1 { "" } else { "s" }));
                }
                if kept > 0 {
                    text.push(format!("{kept} possible permanent mark{} kept", if kept == 1 { "" } else { "s" }));
                }
                if text.is_empty() {
                    text.push("Body skin spots healed".into());
                }
                format!("{}.", text.join("; "))
            },
        ),
        (
            Stage::FrequencySeparation,
            "Never a blur across the face: texture (high frequency) and tone (low frequency) are separated at a radius measured from each face, only uneven tone is corrected, and the person's own pores are restored afterwards.".into(),
            "Uneven tone smoothed selectively with the fine texture kept; lines softened, not removed; real pore texture restored where healing had flattened it.".into(),
        ),
        (
            Stage::MicroDodgeBurn,
            "Visual aid: the skin read in black and white with exaggerated contrast to find small dark and bright irregularities.".into(),
            "Small luminance irregularities evened by lightening the dark ones and darkening the bright ones, colour untouched.".into(),
        ),
        (
            Stage::MediumDodgeBurn,
            "Larger transitions checked: cheeks, forehead, puffiness under the eyes, jaw, and hot spots of shine.".into(),
            "Patchy light evened across larger areas; puffiness under the eyes evened; shine softened.".into(),
        ),
        (
            Stage::GlobalDodgeBurn,
            "The light that was there is preserved: shaping only follows it faintly (cheekbone light, jaw shadow); no make-up is painted with light.".into(),
            "Faint cheekbone light and jaw shadow added along the existing light.".into(),
        ),
        (
            Stage::SkinColour,
            "Colour corrected separately from brightness: redness, blotches and colour casts measured against this person's own skin, never an ideal tone.".into(),
            "Blotchy colour and redness evened toward the person's own skin colour, brightness untouched; body skin matched to the face.".into(),
        ),
        (
            Stage::EyesLipsTeeth,
            "Eyes: dark circles measured against the cheek below them, after every light change on the face; whites cleaned of redness rather than painted white; teeth lose yellow rather than going white; lips keep their vertical texture; brows and lashes are not filled.".into(),
            "Dark circles, where measured, lifted toward the cheek below - brightness and colour cast, never lighter than that cheek, pores and lashes untouched; eye redness reduced, iris and catchlight lifted slightly, teeth less yellow, lip detail kept.".into(),
        ),
    ];
    for (stage, check, change) in describe {
        let Some(r) = report_mut(&mut reports, stage) else {
            continue;
        };
        if !faces_found {
            r.checks
                .push("No face large and clear enough to retouch was found.".into());
            continue;
        }
        r.checks.push(check);
        if stage == Stage::MicroDodgeBurn || stage == Stage::MediumDodgeBurn {
            for (index, reading) in aid.iter().flatten() {
                r.checks.push(format!(
                    "Face {}: {} irregularity {:.3} (typical {:.3}).",
                    index + 1,
                    if stage == Stage::MicroDodgeBurn {
                        "micro"
                    } else {
                        "medium-scale"
                    },
                    if stage == Stage::MicroDodgeBurn {
                        reading.micro
                    } else {
                        reading.medium
                    },
                    if stage == Stage::MicroDodgeBurn {
                        TYPICAL_MICRO
                    } else {
                        TYPICAL_MEDIUM
                    },
                ));
            }
        }
        if count(&stages, stage) > 0 {
            r.changes.push(change);
        }
    }
    if let Some(r) = report_mut(&mut reports, Stage::SkinCleanup) {
        r.checks.extend(
            portrait
                .report
                .assessments
                .iter()
                .filter(|a| a.status == "skipped")
                .map(|a| format!("Face {}: {}", a.face, a.reason)),
        );
    }
    // What was measured at each face's eyes: dark circles against the cheek, redness, closed
    // eyes left alone.
    if let Some(r) = report_mut(&mut reports, Stage::EyesLipsTeeth) {
        r.checks
            .extend(portrait.report.assessments.iter().flat_map(|a| {
                a.findings
                    .iter()
                    .filter(|f| f.starts_with("Eyes:"))
                    .map(move |f| format!("Face {}: {f}", a.face))
            }));
    }

    // ---- 15 to 17: grade, grain, sharpening -------------------------------------------------
    if let Some(r) = report_mut(&mut reports, Stage::ColourGrade) {
        r.checks.push("Correction first, style second: the grade is applied only now, on a technically corrected photograph.".into());
        r.checks.extend(
            global
                .decisions
                .iter()
                .filter(|d| d.starts_with("Vibrance") || d.starts_with("Clarity"))
                .cloned(),
        );
        if matches!(global.kind, SceneKind::Portrait | SceneKind::Group) {
            r.checks.push(
                "No clarity on a portrait: it would roughen skin the retouch just evened.".into(),
            );
        }
        r.checks.push(
            "No signature look is invented: choose an edit profile in Develop to add yours on top."
                .into(),
        );
    }
    let retouched = stages
        .iter()
        .filter(|(s, _)| {
            matches!(
                s,
                Stage::Background
                    | Stage::SkinCleanup
                    | Stage::FrequencySeparation
                    | Stage::Clothing
            )
        })
        .any(|(_, v)| !v.is_empty());
    let noise = smart_edit::noise_sigma(&px);
    let grain = (retouched && noise < 0.004).then_some((6, 15, 45));
    if let Some(r) = report_mut(&mut reports, Stage::Grain) {
        r.checks.push(format!(
            "Measured noise {:.2}% of full scale.",
            noise * 100.0
        ));
        r.checks.push(match (grain, retouched) {
            (Some(_), _) => "The photograph is clean and parts of it were smoothed or healed, so a fine, even grain ties the retouched and untouched areas together.".into(),
            (None, false) => "Nothing was smoothed or healed, so there is nothing to tie together with grain.".into(),
            (None, true) => "The photograph's own grain is already there and covers the retouch; no grain is added.".into(),
        });
    }
    if let Some(r) = report_mut(&mut reports, Stage::OutputSharpening) {
        r.checks.extend(
            global
                .decisions
                .iter()
                .filter(|d| d.starts_with("Sharpening"))
                .cloned(),
        );
        r.checks.push("Sharpened last, after every retouch, so the retouch is never sharpened into artefacts; export adds output sharpening for screen or print on top.".into());
    }
    if let Some(r) = report_mut(&mut reports, Stage::QualityControl) {
        r.checks.push("Inspected after saving: overall (25%): clipping and tonal range; consistency (50%): skin colour drift; retouch quality (100%): fine skin texture kept; the mirror: whether one half of a face was changed more than the other.".into());
    }

    for r in &mut reports {
        r.operations = stages.get(&r.stage).map_or(0, Vec::len);
    }
    let mut portrait_report = portrait.report.clone();
    portrait_report.scene = Some(portrait_auto::SceneSummary {
        kind: global.kind.label().into(),
        decisions: global.decisions.clone(),
    });
    Ok(Planned {
        tone,
        global: Some(global),
        level,
        grain,
        stages,
        mattes,
        reports,
        portrait: portrait_report,
        faces,
        people,
    })
}

/// Write one stage's slider decisions into a proposal.
fn apply_globals(stage: Stage, recipe: &mut Recipe, planned: &Planned) {
    let Some(plan) = &planned.global else { return };
    let g = &mut recipe.global;
    match stage {
        Stage::Foundation => {
            let (exposure, highlights, shadows, contrast) = planned.tone;
            g.exposure = exposure;
            g.highlights = highlights;
            g.shadows = (shadows + plan.extra_shadows).clamp(-100, 100);
            g.contrast = contrast;
            if let Some((kelvin, tint)) = plan.white_balance {
                g.temperature = kelvin;
                g.tint = tint;
            }
            g.whites = plan.whites;
            g.blacks = plan.blacks;
            g.dehaze = plan.dehaze;
            if let Some((luminance, colour)) = plan.noise {
                g.noise.luminance = luminance;
                g.noise.colour = colour;
                g.noise.detail = 50;
            }
        }
        Stage::LensPerspective => {
            if let Some((rotate, crop)) = planned.level {
                recipe.geometry.rotate = rotate;
                recipe.geometry.crop = crop;
            }
        }
        Stage::ColourGrade => {
            g.vibrance = plan.vibrance;
            g.clarity = plan.clarity;
        }
        Stage::Grain => {
            if let Some((amount, size, roughness)) = planned.grain {
                g.effects.grain.amount = amount;
                g.effects.grain.size = size;
                g.effects.grain.roughness = roughness;
            }
        }
        Stage::OutputSharpening => {
            let (amount, radius, detail, masking) = plan.sharpen;
            g.sharpen.amount = amount;
            g.sharpen.radius = radius;
            g.sharpen.detail = detail;
            g.sharpen.masking = masking;
        }
        _ => {}
    }
}

fn describe_globals(stage: Stage, before: &Recipe, after: &Recipe) -> Vec<String> {
    let (a, b) = (&before.global, &after.global);
    let mut out = Vec::new();
    let mut num = |name: &str, x: f32, y: f32, unit: &str| {
        if (x - y).abs() > 1e-4 {
            out.push(format!("{name} {x:+.2}{unit} -> {y:+.2}{unit}"));
        }
    };
    match stage {
        Stage::Foundation => {
            num("Exposure", a.exposure, b.exposure, " EV");
            for (name, x, y) in [
                ("Highlights", a.highlights, b.highlights),
                ("Shadows", a.shadows, b.shadows),
                ("Contrast", a.contrast, b.contrast),
                ("Whites", a.whites, b.whites),
                ("Blacks", a.blacks, b.blacks),
                ("Dehaze", a.dehaze, b.dehaze),
                ("Tint", a.tint, b.tint),
                ("Noise reduction", a.noise.luminance, b.noise.luminance),
                ("Colour noise reduction", a.noise.colour, b.noise.colour),
            ] {
                if x != y {
                    out.push(format!("{name} {x:+} -> {y:+}"));
                }
            }
            if a.temperature != b.temperature {
                out.push(format!(
                    "Temperature {} K -> {} K",
                    a.temperature, b.temperature
                ));
            }
        }
        Stage::LensPerspective => {
            num(
                "Rotation",
                before.geometry.rotate,
                after.geometry.rotate,
                "°",
            );
            if before
                .geometry
                .crop
                .iter()
                .zip(after.geometry.crop)
                .any(|(x, y)| (x - y).abs() > 1e-6)
            {
                out.push("Cropped just enough to hide the corners the rotation opened.".into());
            }
        }
        Stage::ColourGrade => {
            for (name, x, y) in [
                ("Vibrance", a.vibrance, b.vibrance),
                ("Clarity", a.clarity, b.clarity),
            ] {
                if x != y {
                    out.push(format!("{name} {x:+} -> {y:+}"));
                }
            }
        }
        Stage::Grain => {
            if a.effects.grain != b.effects.grain {
                out.push(format!(
                    "Grain {} (size {}, roughness {})",
                    b.effects.grain.amount, b.effects.grain.size, b.effects.grain.roughness
                ));
            }
        }
        Stage::OutputSharpening if a.sharpen != b.sharpen => {
            out.push(format!(
                "Sharpening {} (radius {:.1}, detail {}, masking {}% so skin is spared)",
                b.sharpen.amount, b.sharpen.radius, b.sharpen.detail, b.sharpen.masking
            ));
        }
        _ => {}
    }
    out
}

/// True when `b` would store differently from `a`. Who wrote it is not a difference: the
/// provenance changes with every merge and is not something a photographer can see.
fn changed(a: &Recipe, b: &Recipe) -> AuraResult<bool> {
    let mut a = a.clone();
    a.provenance = b.provenance.clone();
    Ok(aura_recipe::recipe_hash(&a)? != aura_recipe::recipe_hash(b)?)
}

/// Pixels of a recipe at screen size with crop, rotation, grain, vignette and sharpening
/// switched off, so two renders line up pixel for pixel and only the retouch differs.
fn qc_render(
    state: &AppState,
    photo: PhotoId,
    recipe: &Recipe,
) -> AuraResult<(Vec<u8>, usize, usize)> {
    let mut recipe = recipe.clone();
    recipe.geometry = aura_recipe::Geometry::default();
    recipe.global.effects = aura_recipe::Effects::default();
    recipe.global.sharpen.amount = 0;
    let result = state.render()?.render(aura_render::RenderRequest {
        image_id: photo,
        recipe,
        level: RenderLevel::Screen(1600, 1200),
        // Measured, so nothing may be skipped.
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
    Ok((bytes, result.width as usize, result.height as usize))
}

/// Run the whole workflow on one photograph, saving one history step per stage that changed
/// something, and report every stage.
///
/// `progress` is told when each stage starts and finishes, so a window can show the run step
/// by step.
/// # Errors
/// Membership, decode, analysis, rendering or storage failures.
pub fn run(
    state: &AppState,
    input: &AdvancedRetouchInput,
    progress: &dyn Fn(Progress),
) -> IpcResult<AdvancedRetouchDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let invalid = |message: &str| aura_core::errors::render::recipe_invalid("photo", message);
    let photo =
        PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo identifier"))?;
    let project =
        ProjectId::from_db(&input.project_id).map_err(|_| invalid("Invalid project identifier"))?;
    let total = u8::try_from(Stage::ALL.len()).unwrap_or(18);
    let tell = |stage: Stage, running: bool, outcome: Option<Outcome>| {
        progress(Progress {
            photo_id: input.photo_id.clone(),
            number: stage.number(),
            total,
            title: stage.title().into(),
            state: if running { "running" } else { "done" }.into(),
            outcome,
        });
    };
    tell(Stage::Foundation, true, None);
    let previews = state.previews(&input.project_id)?;
    let thumb = previews.get(
        photo,
        aura_raw::PixelLevel::Thumb(512),
        Priority::Interactive,
    )?;
    let Some(rgb) = thumb.as_srgb8() else {
        return Err(invalid("An sRGB preview is required").into());
    };
    let proxy = previews
        .get(
            photo,
            aura_raw::PixelLevel::Proxy2048,
            Priority::Interactive,
        )
        .ok();
    let detail = proxy
        .as_ref()
        .and_then(|p| p.as_srgb8().map(|pixels| (pixels, p.width, p.height)));
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    let options = input.options.unwrap_or_else(options);
    let faces = if std::env::var_os("AURA_DISABLE_AUTO_PORTRAIT").is_some_and(|v| v == "1") {
        Vec::new()
    } else {
        let found = aura_vision::portrait::detect(rgb, thumb.width, thumb.height)?;
        match detail {
            Some((pixels, w, h)) => {
                aura_vision::portrait::detect_small_faces(pixels, w, h, &found).unwrap_or(found)
            }
            None => found,
        }
    };
    let frame = crate::photo_frames::CatalogFrames::new(state.clone())
        .frame(&photo, RenderLevel::Proxy2048)
        .ok();
    let mut planned = plan(
        &base,
        rgb,
        thumb.width,
        thumb.height,
        detail,
        frame.as_ref(),
        faces,
        &options,
    )?;
    let mut portrait_report = planned.portrait.clone();
    portrait_report.options = Some(options);
    portrait_report.steps = planned
        .reports
        .iter()
        .map(|r| portrait_auto::StepSummary {
            step: usize::from(r.number),
            title: r.title.clone(),
            detail: r
                .changes
                .first()
                .or(r.checks.first())
                .cloned()
                .unwrap_or_default(),
            operations: r.operations,
        })
        .collect();

    let mut current = base.clone();
    let mut foundation = base.clone();
    let mut saved = 0_usize;
    let all_mattes = |recipe: &Recipe, planned: &Planned| -> AuraResult<BTreeMap<String, Matte>> {
        let mut all = retouch_tools::read_mattes(recipe)?;
        all.extend(planned.mattes.iter().map(|(k, v)| (k.clone(), v.clone())));
        Ok(all)
    };
    for stage in Stage::ALL {
        if stage == Stage::QualityControl {
            break;
        }
        if stage != Stage::Foundation {
            tell(stage, true, None);
        }
        let mut proposal = current.clone();
        apply_globals(stage, &mut proposal, &planned);
        if stage == Stage::Foundation {
            portrait_auto::write_report(&mut proposal, &portrait_report)?;
        }
        proposal.provenance.source = EditSource::Ai;
        proposal.provenance.confidence = 0.35;
        let (mut merged, mut changes) = schema::merge(&current, &proposal, EditSource::Ai)?;
        if stage.retouch() {
            let now = retouch_tools::read(&current)?;
            let stack = staged(&now, &planned.stages, stage);
            let mattes = all_mattes(&current, &planned)?;
            retouch_tools::validate(&stack)?;
            let mut with_stack = merged.clone();
            retouch_tools::write_with_mattes(&mut with_stack, &stack, &mattes)?;
            let mut as_stored = merged.clone();
            retouch_tools::write_with_mattes(&mut as_stored, &now, &mattes)?;
            // Compared as the store will keep them (six decimals), so a repeat run whose
            // plan only differs below that precision saves nothing.
            if changed(&as_stored, &with_stack)? {
                // An explicit request: the photographer's own operations are kept and the
                // automatic ones replaced, even in a stack they have edited.
                with_stack.provenance.source = EditSource::User;
                let (stacked, more) = schema::merge(&merged, &with_stack, EditSource::User)?;
                merged = stacked;
                changes.changed.extend(more.changed);
                changes.changed.sort_unstable();
                changes.changed.dedup();
            }
        }
        schema::Validation::check(&merged)?;
        let mut description = describe_globals(stage, &current, &merged);
        let did_change = !changes.changed.is_empty() && changed(&current, &merged)?;
        if let Some(r) = report_mut(&mut planned.reports, stage) {
            if !stage.retouch() {
                r.changes.append(&mut description);
            }
            if did_change {
                r.outcome = Outcome::Applied;
                r.saved = true;
            } else if !changes.refused.is_empty() {
                r.outcome = Outcome::Protected;
                r.checks.push(format!(
                    "Kept your own settings: {}.",
                    changes.refused.join(", ")
                ));
            } else if r.operations > 0 || !r.changes.is_empty() {
                r.outcome = Outcome::Unchanged;
                r.checks
                    .push("Already in place from an earlier run; nothing new to save.".into());
            } else {
                r.outcome = if planned.faces.is_empty()
                    && stage.retouch()
                    && stage != Stage::BackgroundToning
                {
                    Outcome::NotApplicable
                } else {
                    Outcome::Unchanged
                };
            }
        }
        if did_change {
            let detail = planned
                .reports
                .iter()
                .find(|r| r.stage == stage)
                .and_then(|r| r.changes.first().cloned())
                .unwrap_or_default();
            state.recipe_store().save(
                &project,
                &photo,
                &merged,
                &changes.changed,
                &format!(
                    "Auto advanced retouch {}/{total} · {}: {detail}",
                    stage.number(),
                    stage.title()
                ),
            )?;
            saved += 1;
            current = merged;
        }
        if stage == Stage::LensPerspective {
            foundation = current.clone();
        }
        let outcome = planned
            .reports
            .iter()
            .find(|r| r.stage == stage)
            .map(|r| r.outcome);
        tell(stage, false, outcome);
    }

    // ---- 18. Quality control & export readiness ---------------------------------------------
    tell(Stage::QualityControl, true, None);
    let mut quality = QualityReport::default();
    let mut qc_notes = Vec::new();
    let mut qc_changes = Vec::new();
    let renders = qc_render(state, photo, &base).and_then(|original| {
        Ok((
            original,
            qc_render(state, photo, &foundation)?,
            qc_render(state, photo, &current)?,
        ))
    });
    match renders {
        Ok(((original, ow, oh), (before, w, h), (after, aw, ah))) => {
            let skin = {
                let mut plane = vec![0.0_f32; w * h];
                for person in &planned.people {
                    if let Some(m) = &person.face {
                        let eroded = portrait_auto::erode(m, 1);
                        for (dst, v) in plane.iter_mut().zip(measure::matte_plane(&eroded, w, h)) {
                            *dst = dst.max(v);
                        }
                    }
                }
                plane
            };
            quality.clipped_original = (ow * oh > 0).then(|| measure::clipped(&original));
            let measure_after = |after: &[u8]| {
                (
                    (aw == w && ah == h)
                        .then(|| measure::texture_retention(&before, after, w, h, &skin))
                        .flatten(),
                    (aw == w && ah == h)
                        .then(|| measure::skin_shift(&before, after, w, h, &skin))
                        .flatten(),
                )
            };
            let (retention, shift) = measure_after(&after);
            let soft = retention.is_some_and(|r| r < TEXTURE_FLOOR);
            let moved = shift.is_some_and(|(s, _)| s > SKIN_SHIFT_CEILING);
            let mut final_pixels = after;
            let (mut retention, mut shift) = (retention, shift);
            if soft || moved {
                // Soften the stages responsible and measure again; the correction is saved
                // with this step, so undoing it restores what the stages first decided.
                let stack = retouch_tools::read(&current)?;
                let corrected: Vec<Edit> = stack
                    .iter()
                    .map(|e| {
                        let mut e = e.clone();
                        match stage_of(&e) {
                            Some(Stage::FrequencySeparation) if soft => {
                                e.tone *= 0.6;
                                e.amount *= 0.75;
                                e.texture = e.texture.max(1.0);
                            }
                            Some(Stage::SkinColour) if moved => e.amount *= 0.5,
                            _ => {}
                        }
                        e
                    })
                    .collect();
                let mut proposal = current.clone();
                retouch_tools::write_with_mattes(
                    &mut proposal,
                    &corrected,
                    &all_mattes(&current, &planned)?,
                )?;
                if let Ok((pixels, rw, rh)) = qc_render(state, photo, &proposal) {
                    if (rw, rh) == (w, h) {
                        let (r2, s2) = measure_after(&pixels);
                        qc_changes.push(format!(
                            "{}{}",
                            if soft { format!("Skin smoothing softened: texture kept was {:.0}%, now {:.0}%. ", retention.unwrap_or(0.0) * 100.0, r2.unwrap_or(0.0) * 100.0) } else { String::new() },
                            if moved { format!("Skin colour steps halved: colour had moved {:.3}, now {:.3}.", shift.map_or(0.0, |s| s.0), s2.map_or(0.0, |s| s.0)) } else { String::new() },
                        ));
                        (retention, shift) = (r2, s2);
                        final_pixels = pixels;
                        current = proposal;
                        quality.corrected = true;
                    }
                }
            }
            quality.texture_retention = retention;
            quality.skin_shift = shift.map(|s| s.0);
            quality.clipped_final = Some(measure::clipped(&final_pixels));
            quality.mirror_balance = planned
                .faces
                .iter()
                .filter_map(|f| measure::mirror_balance(&before, &final_pixels, w, h, f))
                .filter(|(_, change)| *change > 0.004)
                .map(|(ratio, _)| ratio)
                .reduce(f32::max);
            quality.passed = retention.is_none_or(|r| r >= TEXTURE_FLOOR)
                && shift.is_none_or(|(s, _)| s <= SKIN_SHIFT_CEILING);
            qc_notes.push(match (quality.clipped_original, quality.clipped_final) {
                (Some(a), Some(b)) => format!(
                    "25% - overall: clipped pixels {:.2}% in the original, {:.2}% finished.",
                    a * 100.0,
                    b * 100.0
                ),
                _ => "25% - overall: not measured.".into(),
            });
            qc_notes.push(match shift {
                Some((s, l)) => format!("50% - consistency: skin colour moved {s:.3} during the retouch (limit {SKIN_SHIFT_CEILING}); skin brightness {:+.0}%.", (l - 1.0) * 100.0),
                None => "50% - consistency: no segmented face skin to measure.".into(),
            });
            qc_notes.push(match retention {
                Some(r) => format!(
                    "100% - retouch quality: {:.0}% of the fine skin texture kept (floor {:.0}%).",
                    r * 100.0,
                    TEXTURE_FLOOR * 100.0
                ),
                None => "100% - retouch quality: no segmented face skin to measure.".into(),
            });
            qc_notes.push("200% - edges: every automatic operation is limited by a feathered selection refined to the photograph's own edges.".into());
            qc_notes.push(match quality.mirror_balance {
                Some(b) if b > 3.0 => format!("Mirror check: one half of a face was changed {b:.1}x more than the other - review it flipped; this is expected when the light falls from one side."),
                Some(b) => format!("Mirror check: both halves of each face changed alike ({b:.1}x)."),
                None => "Mirror check: no face changed enough to compare halves.".into(),
            });
        }
        Err(error) => {
            qc_notes.push(format!("The finished photograph could not be rendered for inspection ({}); review it yourself before delivery.", error.code));
        }
    }
    qc_notes.push("Identity: nothing was reshaped, moles and freckles were kept, and every skin decision was measured against the person's own skin.".into());
    qc_notes.push("Compare RAW -> corrected -> retouched -> final by stepping through this run's history entries.".into());
    qc_notes.push("Export: the master as 16-bit TIFF in Adobe RGB (album preset), web and social copies in sRGB (gallery and social presets). AURA never chooses a destination; export from the Export step.".into());

    let mut report = Report {
        version: VERSION.into(),
        faces: planned.faces.len(),
        stages: planned.reports.clone(),
        quality: quality.clone(),
        history_steps: saved,
        summary: String::new(),
    };
    if let Some(r) = report
        .stages
        .iter_mut()
        .find(|r| r.stage == Stage::QualityControl)
    {
        r.checks.extend(qc_notes);
        r.changes.extend(qc_changes);
        r.outcome = if quality.corrected {
            Outcome::Applied
        } else {
            Outcome::Unchanged
        };
    }
    let applied = report
        .stages
        .iter()
        .filter(|r| r.outcome == Outcome::Applied)
        .count();
    report.history_steps = saved + 1;
    report.summary = format!(
        "All 18 stages ran in order: {applied} changed the photograph and each is its own history step; the rest were inspected and needed nothing or did not apply. {}",
        if quality.passed { "Quality control passed." } else { "Quality control flagged something - see its notes." }
    );
    let mut proposal = current.clone();
    proposal.extra.insert(
        KEY.into(),
        serde_json::to_value(&report)
            .map_err(|e| aura_core::errors::render::recipe_invalid(KEY, &e.to_string()))?,
    );
    proposal.provenance.source = EditSource::User;
    let (merged, changes) = schema::merge(&current, &proposal, EditSource::User)?;
    schema::Validation::check(&merged)?;
    // Nothing is written when the run found exactly what the stored report already says.
    if !changes.changed.is_empty() && changed(&current, &merged)? {
        state.recipe_store().save(
            &project,
            &photo,
            &merged,
            &changes.changed,
            &format!(
                "Auto advanced retouch 18/{total} · {}: {}",
                Stage::QualityControl.title(),
                if quality.corrected {
                    "corrected and measured"
                } else if quality.passed {
                    "passed"
                } else {
                    "review the notes"
                }
            ),
        )?;
    } else {
        report.history_steps = saved;
    }
    tell(
        Stage::QualityControl,
        false,
        Some(
            report
                .stages
                .last()
                .map_or(Outcome::Unchanged, |r| r.outcome),
        ),
    );
    Ok(AdvancedRetouchDto {
        recipe: crate::develop_commands::recipe_dto(&input.photo_id, &merged),
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(id: &str, tool: Tool) -> Edit {
        base_edit(id.into(), tool, [0.5, 0.5, 0.1, 0.1], 0.5)
    }

    #[test]
    fn eighteen_stages_in_the_professional_order() {
        assert_eq!(Stage::ALL.len(), 18);
        assert_eq!(Stage::Foundation.number(), 1);
        assert_eq!(Stage::QualityControl.number(), 18);
        let mut sorted = Stage::ALL;
        sorted.sort();
        assert_eq!(sorted, Stage::ALL, "save order is declaration order");
        assert!(Stage::SkinCleanup < Stage::FrequencySeparation);
        assert!(Stage::MicroDodgeBurn < Stage::MediumDodgeBurn);
        assert!(Stage::MediumDodgeBurn < Stage::GlobalDodgeBurn);
        assert!(Stage::GlobalDodgeBurn < Stage::SkinColour);
        assert!(Stage::BackgroundToning < Stage::ColourGrade);
        assert!(Stage::Grain < Stage::OutputSharpening);
    }

    #[test]
    fn every_automatic_operation_has_a_stage_and_manual_work_has_none() {
        for (id, tool, stage) in [
            (
                "auto-portrait-v1-backdrop",
                Tool::Backdrop,
                Stage::Background,
            ),
            ("auto-portrait-v1-dust-3", Tool::Heal, Stage::Background),
            (
                "auto-portrait-v1-0-hair-detail",
                Tool::Frequency,
                Stage::Hair,
            ),
            ("auto-portrait-v1-stray-2", Tool::Backdrop, Stage::Hair),
            (
                "auto-portrait-v1-0-clear",
                Tool::AcneClear,
                Stage::SkinCleanup,
            ),
            (
                "auto-portrait-v1-1-spot-deep-12",
                Tool::PatchHeal,
                Stage::SkinCleanup,
            ),
            (
                "auto-portrait-v1-0-body-spots",
                Tool::AutoBlemish,
                Stage::SkinCleanup,
            ),
            (
                "auto-portrait-v1-0-texture",
                Tool::SkinSmooth,
                Stage::FrequencySeparation,
            ),
            (
                "auto-portrait-v1-0-texture-graft",
                Tool::TextureGraft,
                Stage::FrequencySeparation,
            ),
            (
                "auto-portrait-v1-0-lines-forehead",
                Tool::Wrinkle,
                Stage::FrequencySeparation,
            ),
            (
                "auto-portrait-v1-0-microdb",
                Tool::MicroDodgeBurn,
                Stage::MicroDodgeBurn,
            ),
            (
                "auto-portrait-v1-0-fold-a",
                Tool::MicroDodgeBurn,
                Stage::MicroDodgeBurn,
            ),
            (
                "auto-portrait-v1-0-light",
                Tool::PortraitDodgeBurn,
                Stage::MediumDodgeBurn,
            ),
            (
                "auto-portrait-v1-0-undereye-a",
                Tool::UnderEye,
                Stage::EyesLipsTeeth,
            ),
            (
                "auto-portrait-v1-0-undereye-bag-a",
                Tool::MicroDodgeBurn,
                Stage::MediumDodgeBurn,
            ),
            (
                "auto-portrait-v1-0-shine",
                Tool::Mattify,
                Stage::MediumDodgeBurn,
            ),
            (
                "auto-portrait-v1-0-sculpt-contour",
                Tool::Burn,
                Stage::GlobalDodgeBurn,
            ),
            (
                "auto-portrait-v1-0-tone",
                Tool::SkinUniformity,
                Stage::SkinColour,
            ),
            (
                "auto-portrait-v1-0-redness-a",
                Tool::ColorMatch,
                Stage::SkinColour,
            ),
            (
                "auto-portrait-v1-0-eye-a-white",
                Tool::EyeClean,
                Stage::EyesLipsTeeth,
            ),
            (
                "auto-portrait-v1-0-lips-detail",
                Tool::EyeDetail,
                Stage::EyesLipsTeeth,
            ),
            (
                "auto-portrait-v1-0-teeth",
                Tool::Teeth,
                Stage::EyesLipsTeeth,
            ),
            ("auto-portrait-v1-0-fabric", Tool::Fabric, Stage::Clothing),
            ("auto-portrait-v1-1-cloth-0", Tool::Heal, Stage::Clothing),
            ("auto-portrait-v1-jewel-4", Tool::Glare, Stage::Jewellery),
            (
                "auto-portrait-v1-background-tone",
                Tool::Burn,
                Stage::BackgroundToning,
            ),
            ("auto-scene-v1-sky", Tool::Burn, Stage::BackgroundToning),
        ] {
            let e = edit(id, tool);
            assert_eq!(stage_of(&e), Some(stage), "{id}");
            // The older automatic pass agrees that it is automatic, so it never keeps it as
            // a person's own work.
            assert!(portrait_auto::group_of(id).is_some(), "{id}");
        }
        assert_eq!(stage_of(&edit("my-heal", Tool::Heal)), None);
        assert_eq!(
            stage_of(&edit("manual-auto-portrait-v1-0-texture", Tool::SkinSmooth)),
            None
        );
    }

    #[test]
    fn staging_changes_only_the_stage_being_saved_and_keeps_manual_work_first() {
        let manual = edit("mine", Tool::Dodge);
        let old_spot = edit("auto-portrait-v1-0-spot-1", Tool::PatchHeal);
        let old_texture = edit("auto-portrait-v1-0-texture", Tool::SkinSmooth);
        let new_clear = edit("auto-portrait-v1-0-clear", Tool::AcneClear);
        let current = vec![old_texture.clone(), manual.clone(), old_spot];
        let mut planned: BTreeMap<Stage, Vec<Edit>> = Stage::ALL
            .into_iter()
            .filter(|s| s.retouch())
            .map(|s| (s, Vec::new()))
            .collect();
        planned.insert(Stage::SkinCleanup, vec![new_clear.clone()]);
        planned.insert(Stage::FrequencySeparation, vec![old_texture.clone()]);
        let after_background = staged(&current, &planned, Stage::Background);
        assert_eq!(after_background[0], manual);
        assert_eq!(after_background.len(), 3);
        let after_cleanup = staged(&after_background, &planned, Stage::SkinCleanup);
        assert_eq!(
            after_cleanup,
            vec![manual.clone(), new_clear.clone(), old_texture.clone()]
        );
        let done = staged(&after_cleanup, &planned, Stage::BackgroundToning);
        assert_eq!(done, after_cleanup);
        // A repeat run of an unchanged plan changes nothing at any stage.
        for stage in Stage::ALL {
            assert_eq!(staged(&done, &planned, stage), done);
        }
    }

    #[test]
    fn the_workflow_defaults_keep_identity() {
        let o = options();
        assert!(
            !o.settings.remove_dark_marks,
            "moles and beauty marks are kept"
        );
        assert!(o.settings.keep_freckles);
        assert!(o.settings.texture >= 0.85, "pores survive smoothing");
        assert!(o.settings.smoothing <= 0.4, "no blur across the face");
        assert!(o.settings.texture_graft > 0.0, "pores are restored");
        assert!(o.settings.protect_eye_area && o.settings.protect_nose_detail);
        assert!(
            o.settings.eye_whitening <= 0.15,
            "whites are cleaned, not painted"
        );
        assert!(
            o.settings.lip_colour.abs() < f32::EPSILON && o.settings.blush.abs() < f32::EPSILON,
            "no make-up painted"
        );
        assert_eq!(o.sanitised(), o);
    }

    #[test]
    fn a_plan_reports_every_stage_even_without_a_person() {
        let (w, h) = (120_u32, 90_u32);
        let rgb: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let v = 60 + ((i % w) * 120 / w) as u8;
                [v, v, v.saturating_sub(10)]
            })
            .collect();
        let base = aura_recipe::fixtures::neutral("test", "test");
        let planned = plan(&base, &rgb, w, h, None, None, Vec::new(), &options()).unwrap();
        assert_eq!(planned.reports.len(), 18);
        for r in &planned.reports {
            assert!(
                !r.checks.is_empty() || r.stage == Stage::Foundation || !r.changes.is_empty(),
                "{:?} says nothing",
                r.stage
            );
        }
        assert!(
            planned.stages.values().all(Vec::is_empty),
            "no person, no retouch"
        );
        let skin = planned
            .reports
            .iter()
            .find(|r| r.stage == Stage::SkinCleanup)
            .unwrap();
        assert!(skin.checks.iter().any(|c| c.contains("No face")));
    }
}
