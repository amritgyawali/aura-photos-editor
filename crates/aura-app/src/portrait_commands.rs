//! Portrait retouching: what AURA found in a photograph, and what a photographer asks for.
//!
//! Four commands.
//!
//! * [`analyse_portrait`] shows the parse - every face with its landmarks and every region as a
//!   soft overlay - from **the same cached parse the renderer resolves its masks through**, so
//!   the overlay a photographer looks at is exactly the region an operator will touch.
//! * [`portrait_retouch`] reads the current settings back out of the recipe.
//! * [`set_portrait_retouch`] writes them as a person: fourteen operator strengths, region
//!   adjustments and any face a person drew because the detector missed it.
//! * [`auto_portrait_retouch`] sizes a suggestion from what the face measures as and writes it as
//!   an automated pass - which `aura_recipe::schema::merge` refuses wherever a person already set
//!   the retouch, so the suggestion can never overwrite a choice.
//!
//! # What is not here
//!
//! **No operator reshapes, slims, or changes anybody's skin tone**, and there is no field on any
//! shape here that could carry one. Every operator is a tone or colour change in place, and the
//! evening operator moves skin toward the local average of the same person's own skin.
//! **No command writes anything but `retouch` and `masks`.** The portrait surface cannot reach an
//! exposure slider, a curve or a crop.

use aura_core::{AuraError, PhotoId, ProjectId};
use aura_portrait::canvas::{Canvas, Primaries};
use aura_portrait::face::FaceHint;
use aura_portrait::{Region, ALL_REGIONS, ANALYSIS_EDGE, PARSE_VER};
use aura_recipe::{schema, EditSource, Mask, MaskKind, MaskParams, Recipe, RetouchOp};
use aura_render::{FrameSource, RenderLevel};
use serde::{Deserialize, Serialize};

use crate::commands::IpcResult;
use crate::contract::ipc::{IpcError, RecipeDto};
use crate::develop_commands::{load_or_neutral, recipe_dto};
use crate::state::AppState;

/// The long edge of the overlay planes sent to the panel. A display aid, not a mask: the
/// renderer resolves the full-resolution plane itself.
pub const OVERLAY_EDGE: u32 = 320;

/// Mask ids this surface owns. Everything else in `masks` belongs to another phase and is
/// carried through untouched.
const MASK_PREFIX: &str = "portrait-";

/// One photograph.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortraitInput {
    /// The photograph.
    pub photo_id: String,
}

/// One face, normalised to the frame.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortraitFaceDto {
    /// `x`, `y`, `w`, `h` in `0..1`, axis-aligned.
    pub bbox: [f32; 4],
    /// Image-left eye.
    pub left_eye: [f32; 2],
    /// Image-right eye.
    pub right_eye: [f32; 2],
    /// Nose tip.
    pub nose: [f32; 2],
    /// Mouth centre.
    pub mouth: [f32; 2],
    /// Head tilt, degrees, clockwise.
    pub roll_degrees: f32,
    /// `0..1`.
    pub confidence: f32,
    /// Which pass found it: `frontal`, `tilted`, `equalised`, `profile` or `hint`.
    pub source: String,
    /// How many of the two eyes were measured rather than placed.
    pub eyes_measured: u8,
    /// True when the mouth was measured rather than placed.
    pub mouth_measured: bool,
}

/// One region, with an overlay.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortraitRegionDto {
    /// Stable slug, e.g. `teeth`.
    pub region: String,
    /// The product's own name for it.
    pub label: String,
    /// Share of the frame, `0..1`.
    pub coverage: f32,
    /// `0..1`.
    pub confidence: f32,
    /// One byte of alpha per pixel at `overlay_width x overlay_height`, base64.
    pub alpha_base64: String,
}

/// What the parse found in one photograph.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortraitAnalysisDto {
    /// The photograph.
    pub photo_id: String,
    /// Overlay width.
    pub overlay_width: u32,
    /// Overlay height.
    pub overlay_height: u32,
    /// The faces, largest first.
    pub faces: Vec<PortraitFaceDto>,
    /// Every region that covers anything, in the parse's order.
    pub regions: Vec<PortraitRegionDto>,
    /// False for a black-and-white frame: skin was found by structure, not colour.
    pub colourful: bool,
    /// The parse version, which is also in the renderer's engine string.
    pub parse_version: u16,
    /// Milliseconds the analysis took, including the decode.
    pub ms: u32,
    /// Sentences for the panel.
    pub notes: Vec<String>,
}

/// One operator's setting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortraitOpDto {
    /// An operator slug from `aura_render::portrait::OPERATORS`.
    pub op: String,
    /// `0..1`.
    pub strength: f32,
}

/// One region adjustment. Every field is optional; absent means "not touched here".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionAdjustmentDto {
    /// A region slug, e.g. `hair` or `background`.
    pub region: String,
    /// Stops.
    #[serde(default)]
    pub exposure: Option<f32>,
    /// `-100..100`.
    #[serde(default)]
    pub contrast: Option<i16>,
    /// `-100..100`.
    #[serde(default)]
    pub saturation: Option<i16>,
    /// Kelvin offset from the global white balance.
    #[serde(default)]
    pub warmth: Option<i32>,
    /// `-100..100`.
    #[serde(default)]
    pub shadows: Option<i16>,
    /// `-100..100`.
    #[serde(default)]
    pub highlights: Option<i16>,
}

/// The portrait settings on one photograph.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortraitRetouchDto {
    /// The photograph.
    pub photo_id: String,
    /// Operators with a strength above zero.
    pub ops: Vec<PortraitOpDto>,
    /// Region adjustments.
    pub adjustments: Vec<RegionAdjustmentDto>,
    /// Faces a person drew: `x`, `y`, `w`, `h` in `0..1`.
    pub hints: Vec<[f32; 4]>,
    /// True when a person has set the retouch, so an automatic pass will not change it.
    pub protected: bool,
    /// Operators in the recipe this renderer does not execute, named rather than hidden.
    pub foreign_ops: Vec<String>,
    /// The whole edit, for the develop panel.
    pub recipe: RecipeDto,
    /// What an automatic pass did, when one ran: a sentence per decision.
    pub explanation: Vec<String>,
}

/// A photographer's portrait settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPortraitRetouchInput {
    /// The project the photograph belongs to.
    pub project_id: String,
    /// The photograph.
    pub photo_id: String,
    /// Every operator's strength. An operator absent here is removed.
    pub ops: Vec<PortraitOpDto>,
    /// Every region adjustment. One absent here is removed.
    #[serde(default)]
    pub adjustments: Vec<RegionAdjustmentDto>,
    /// Faces a person drew. `None` keeps the ones already there.
    #[serde(default)]
    pub hints: Option<Vec<[f32; 4]>>,
    /// The history label.
    #[serde(default)]
    pub label: Option<String>,
}

/// An automatic retouch request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoPortraitRetouchInput {
    /// The project the photograph belongs to.
    pub project_id: String,
    /// The photograph.
    pub photo_id: String,
    /// `natural`, `soft`, `polished` or `off`.
    pub style: String,
}

fn invalid(field: &str, message: &str) -> IpcError {
    IpcError::from(aura_core::errors::render::recipe_invalid(field, message))
}

fn parse_photo(id: &str) -> Result<PhotoId, IpcError> {
    PhotoId::from_db(id).map_err(|_| invalid("photo_id", "not a photo id"))
}

fn parse_project(id: &str) -> Result<ProjectId, IpcError> {
    ProjectId::from_db(id).map_err(|_| invalid("project_id", "not a project id"))
}

/// The photograph as the renderer's interactive path reads it, parsed through the renderer's own
/// cache.
fn parse_photograph(
    state: &AppState,
    photo: PhotoId,
    recipe: &Recipe,
) -> Result<
    (
        std::sync::Arc<aura_portrait::PortraitMap>,
        aura_render::Frame,
    ),
    AuraError,
> {
    let frames = crate::photo_frames::CatalogFrames::new(state.clone());
    let frame = frames.frame(&photo, RenderLevel::Proxy2048)?;
    let hints = aura_render::portrait::hints(recipe);
    let map = aura_render::portrait::parse(&frame, &hints).ok_or_else(|| {
        aura_core::errors::render::recipe_invalid("photo", "the photograph could not be parsed")
    })?;
    Ok((map, frame))
}

/// What AURA found in a photograph.
///
/// # Errors
///
/// A typed error when the photograph cannot be decoded or is not in the catalog.
// Grid sizes are at most a few thousand pixels and alphas are clamped into 0..=1 before they
// become bytes, so none of these casts can lose anything that matters.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn analyse_portrait(state: &AppState, input: &PortraitInput) -> IpcResult<PortraitAnalysisDto> {
    let started = state.clock().monotonic_us();
    let photo = parse_photo(&input.photo_id)?;
    let recipe = load_or_neutral(state, photo)?;
    let (map, _) = parse_photograph(state, photo, &recipe)?;
    let (ow, oh) = aura_portrait::canvas::fit(map.width, map.height, OVERLAY_EDGE);
    let w = map.width.max(1) as f32;
    let h = map.height.max(1) as f32;
    let norm = |p: [f32; 2]| [p[0] / w, p[1] / h];
    let faces = map
        .faces
        .iter()
        .map(|f| {
            let b = f.bbox();
            PortraitFaceDto {
                bbox: [b[0] / w, b[1] / h, b[2] / w, b[3] / h],
                left_eye: norm(f.left_eye),
                right_eye: norm(f.right_eye),
                nose: norm(f.nose),
                mouth: norm(f.mouth),
                roll_degrees: f.roll.to_degrees(),
                confidence: f.confidence,
                source: f.source.as_str().to_string(),
                eyes_measured: f.eyes_measured,
                mouth_measured: f.mouth_measured,
            }
        })
        .collect();
    let regions = ALL_REGIONS
        .iter()
        .filter_map(|region| {
            let stat = map.stat(*region)?;
            if stat.coverage <= 1e-4 {
                return None;
            }
            let plane = map.resolve(*region, ow, oh);
            let bytes: Vec<u8> = plane
                .values
                .iter()
                .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect();
            Some(PortraitRegionDto {
                region: region.as_str().to_string(),
                label: region.label().to_string(),
                coverage: stat.coverage,
                confidence: stat.confidence,
                alpha_base64: crate::develop_commands::base64(&bytes),
            })
        })
        .collect();
    let mut notes = Vec::new();
    if map.faces.is_empty() {
        notes.push(
            "AURA did not find a face in this photograph. Draw a box around a face and AURA will \
             retouch it."
                .to_string(),
        );
    } else if map.faces.iter().any(|f| f.eyes_measured < 2) {
        notes.push(
            "Some eyes were placed from the shape of the face rather than measured, so eye \
             retouching there is gentler."
                .to_string(),
        );
    }
    if !map.colourful {
        notes.push(
            "This photograph has almost no colour, so skin was found from the face's shape and \
             brightness alone."
                .to_string(),
        );
    }
    let elapsed = state.clock().monotonic_us().saturating_sub(started);
    Ok(PortraitAnalysisDto {
        photo_id: input.photo_id.clone(),
        overlay_width: ow,
        overlay_height: oh,
        faces,
        regions,
        colourful: map.colourful,
        parse_version: PARSE_VER,
        ms: u32::try_from(elapsed / 1_000).unwrap_or(u32::MAX),
        notes,
    })
}

/// The portrait settings already on a photograph.
///
/// # Errors
///
/// A typed error when the stored edit cannot be read.
pub fn portrait_retouch(state: &AppState, input: &PortraitInput) -> IpcResult<PortraitRetouchDto> {
    let photo = parse_photo(&input.photo_id)?;
    let recipe = load_or_neutral(state, photo)?;
    Ok(settings_dto(&input.photo_id, &recipe, Vec::new()))
}

fn settings_dto(photo_id: &str, recipe: &Recipe, explanation: Vec<String>) -> PortraitRetouchDto {
    let ops = recipe
        .retouch
        .iter()
        .filter(|op| aura_render::portrait::is_operator(&op.op))
        .map(|op| PortraitOpDto {
            op: op.op.clone(),
            strength: op.strength,
        })
        .collect();
    let foreign_ops = recipe
        .retouch
        .iter()
        .filter(|op| !aura_render::portrait::is_operator(&op.op))
        .map(|op| op.op.clone())
        .collect();
    let adjustments = recipe
        .masks
        .iter()
        .filter(|m| m.id.starts_with(MASK_PREFIX) && !aura_render::portrait::is_hint(m))
        .filter_map(|m| {
            let region = aura_render::portrait::mask_region(m)?;
            Some(RegionAdjustmentDto {
                region: region.as_str().to_string(),
                exposure: m.params.exposure,
                contrast: m.params.contrast,
                saturation: m.params.saturation,
                warmth: m.params.temperature,
                shadows: m.params.shadows,
                highlights: m.params.highlights,
            })
        })
        .collect();
    let hints = aura_render::portrait::hints(recipe)
        .iter()
        .map(|h| [h.x, h.y, h.w, h.h])
        .collect();
    let protected = recipe
        .provenance
        .user_edited_fields
        .iter()
        .any(|f| f == "retouch");
    PortraitRetouchDto {
        photo_id: photo_id.to_string(),
        ops,
        adjustments,
        hints,
        protected,
        foreign_ops,
        recipe: recipe_dto(photo_id, recipe),
        explanation,
    }
}

/// The recipe kind a region's mask is drawn from. The region itself travels in `target`.
fn mask_kind(region: Region) -> MaskKind {
    match region {
        Region::Skin | Region::BodySkin => MaskKind::Skin,
        Region::Face
        | Region::Eyes
        | Region::Iris
        | Region::Sclera
        | Region::Eyebrows
        | Region::UnderEyes
        | Region::Nose
        | Region::Lips
        | Region::Teeth
        | Region::Mouth
        | Region::FacialHair => MaskKind::Face,
        Region::Background => MaskKind::Background,
        Region::Sky => MaskKind::Sky,
        Region::Neck | Region::Hair | Region::Clothing | Region::Body => MaskKind::Subject,
    }
}

/// The base recipe with this surface's operators, adjustments and hints replaced and everything
/// another phase wrote carried through.
fn with_portrait(
    base: &Recipe,
    ops: &[PortraitOpDto],
    adjustments: &[RegionAdjustmentDto],
    hints: Option<&[[f32; 4]]>,
) -> Result<Recipe, IpcError> {
    let mut out = base.clone();
    out.retouch
        .retain(|op| !aura_render::portrait::is_operator(&op.op));
    // The panel's order, which is also the order the renderer applies them in.
    for name in aura_render::portrait::OPERATORS {
        let Some(op) = ops.iter().find(|op| op.op == name) else {
            continue;
        };
        if !op.strength.is_finite() {
            return Err(invalid("retouch.strength", "must be a number"));
        }
        let strength = op.strength.clamp(0.0, 1.0);
        if strength <= 0.0 {
            continue;
        }
        out.retouch.push(RetouchOp {
            op: name.to_string(),
            strength,
            // Texture is protected at the same fraction for every portrait operator; the
            // smoothing operator is the only one that reads it, and it keeps the pores.
            protect_texture: 0.7,
            mask: None,
            borrowed_from: None,
        });
    }
    for op in ops {
        if !aura_render::portrait::is_operator(&op.op) {
            return Err(invalid(
                "retouch.op",
                &format!("`{}` is not a portrait operator", op.op),
            ));
        }
    }

    let kept_hints: Vec<Mask> = base
        .masks
        .iter()
        .filter(|m| m.id.starts_with(MASK_PREFIX) && aura_render::portrait::is_hint(m))
        .cloned()
        .collect();
    out.masks.retain(|m| !m.id.starts_with(MASK_PREFIX));
    match hints {
        Some(hints) => {
            for (i, h) in hints.iter().enumerate() {
                let hint = FaceHint {
                    x: h[0],
                    y: h[1],
                    w: h[2],
                    h: h[3],
                };
                if !(hint.w > 0.005 && hint.h > 0.005)
                    || [hint.x, hint.y, hint.w, hint.h]
                        .iter()
                        .any(|v| !v.is_finite())
                {
                    return Err(invalid("hints", "a face box needs a width and a height"));
                }
                out.masks.push(Mask {
                    id: format!("{MASK_PREFIX}hint-{}", i + 1),
                    kind: MaskKind::Face,
                    target: Some(hint.to_target()),
                    invert_of: None,
                    feather: 0.0,
                    params: MaskParams::default(),
                });
            }
        }
        None => out.masks.extend(kept_hints),
    }
    for adjustment in adjustments {
        let Some(region) = Region::parse(&adjustment.region) else {
            return Err(invalid(
                "masks.target",
                &format!("`{}` is not a region", adjustment.region),
            ));
        };
        let params = MaskParams {
            exposure: adjustment.exposure.map(|v| v.clamp(-3.0, 3.0)),
            contrast: adjustment.contrast.map(|v| v.clamp(-100, 100)),
            saturation: adjustment.saturation.map(|v| v.clamp(-100, 100)),
            temperature: adjustment.warmth.map(|v| v.clamp(-3000, 3000)),
            shadows: adjustment.shadows.map(|v| v.clamp(-100, 100)),
            highlights: adjustment.highlights.map(|v| v.clamp(-100, 100)),
            ..MaskParams::default()
        };
        if params.is_empty() {
            continue;
        }
        let id = format!("{MASK_PREFIX}{}", region.as_str());
        if out.masks.iter().any(|m| m.id == id) {
            return Err(invalid("masks.id", "a region is adjusted twice"));
        }
        out.masks.push(Mask {
            id,
            kind: mask_kind(region),
            target: Some(region.as_str().to_string()),
            invert_of: None,
            feather: 0.3,
            params,
        });
    }
    Ok(out)
}

/// Set the portrait retouch, as a person.
///
/// # Errors
///
/// `AURA-RENDER-8002` for an unknown operator or region, and whatever saving raises.
pub fn set_portrait_retouch(
    state: &AppState,
    input: &SetPortraitRetouchInput,
) -> IpcResult<PortraitRetouchDto> {
    let project = parse_project(&input.project_id)?;
    let photo = parse_photo(&input.photo_id)?;
    let base = load_or_neutral(state, photo)?;
    let proposal = with_portrait(
        &base,
        &input.ops,
        &input.adjustments,
        input.hints.as_deref(),
    )?;
    let (merged, report) = schema::merge(&base, &proposal, EditSource::User)?;
    schema::Validation::check(&merged)?;
    let label = input
        .label
        .clone()
        .unwrap_or_else(|| "Portrait retouch".to_string());
    state
        .recipe_store()
        .save(&project, &photo, &merged, &report.changed, &label)?;
    Ok(settings_dto(&input.photo_id, &merged, Vec::new()))
}

/// A measured suggestion: each operator's strength, and the sentence that says why.
#[must_use]
pub fn suggest(
    readings: &aura_portrait::PortraitReadings,
    style: &str,
) -> (Vec<PortraitOpDto>, Vec<String>) {
    let scale = match style {
        "off" => 0.0,
        "soft" => 0.6,
        "polished" => 1.4,
        _ => 1.0,
    };
    let ramp = aura_portrait::skin::ramp;
    let mut ops = Vec::new();
    let mut why = Vec::new();
    if readings.faces == 0 || scale <= 0.0 {
        if readings.faces == 0 {
            why.push("No face was found, so nothing was retouched.".to_string());
        }
        return (ops, why);
    }
    let mut push = |op: &str, strength: f32, reason: String| {
        let s = (strength * scale).clamp(0.0, 1.0);
        if s >= 0.05 {
            ops.push(PortraitOpDto {
                op: op.to_string(),
                strength: (s * 100.0).round() / 100.0,
            });
            why.push(reason);
        }
    };
    if let Some(rough) = readings.skin_roughness {
        push(
            "skin_smooth",
            0.2 + 0.4 * ramp(rough, 2.0, 6.0),
            format!("Skin smoothed to match how uneven it measured ({rough:.1}); pores are kept."),
        );
    }
    if let Some(tone) = readings.tone_variation {
        push(
            "skin_even",
            0.15 + 0.35 * ramp(tone, 2.0, 5.0),
            "Blotchy redness evened toward the person's own skin tone.".to_string(),
        );
    }
    push(
        "blemish_clear",
        0.5,
        "Small marks cleared; moles and freckles that are part of the face are kept.".to_string(),
    );
    if let Some(depth) = readings.under_eye_depth {
        push(
            "under_eye_lift",
            0.6 * ramp(depth, 1.0, 8.0),
            format!("Under-eye shadow lifted toward the cheek ({depth:.1} darker)."),
        );
    }
    if let Some(shine) = readings.shine_share {
        push(
            "shine_control",
            0.15 + 0.4 * ramp(shine, 0.002, 0.03),
            "Specular shine on the skin softened.".to_string(),
        );
    }
    push("eye_brighten", 0.3, "Eyes brightened slightly.".to_string());
    push(
        "iris_enhance",
        0.3,
        "Iris detail and colour brought out.".to_string(),
    );
    if let Some(red) = readings.sclera_redness {
        push(
            "sclera_whiten",
            0.5 * ramp(red, 6.0, 16.0),
            "Redness in the whites of the eyes reduced.".to_string(),
        );
    }
    if let Some(yellow) = readings.teeth_yellowness {
        push(
            "teeth_whiten",
            0.6 * ramp(yellow, 5.0, 16.0),
            format!("Teeth whitened in proportion to how yellow they measured ({yellow:.1})."),
        );
    }
    push(
        "lip_enhance",
        0.15,
        "Lip colour given a touch more life.".to_string(),
    );
    if readings.hair_coverage > 0.005 {
        push("hair_define", 0.25, "Hair texture defined.".to_string());
    }
    if let Some(face) = readings.face_lightness {
        push(
            "face_light",
            0.4 * ramp(readings.frame_lightness - face + 5.0, 0.0, 15.0),
            "A little fill light on a face darker than its surroundings.".to_string(),
        );
    }
    (ops, why)
}

/// Retouch a photograph automatically, from what its faces measure as.
///
/// Written as an automated pass. When a person has already set the retouch on this photograph
/// the merge refuses the change and the response says so.
///
/// # Errors
///
/// A typed error when the photograph cannot be decoded or the edit cannot be saved.
pub fn auto_portrait_retouch(
    state: &AppState,
    input: &AutoPortraitRetouchInput,
) -> IpcResult<PortraitRetouchDto> {
    let project = parse_project(&input.project_id)?;
    let photo = parse_photo(&input.photo_id)?;
    let base = load_or_neutral(state, photo)?;
    let (map, frame) = parse_photograph(state, photo, &base)?;
    let canvas = Canvas::from_linear(
        &frame.rgb,
        frame.width,
        frame.height,
        Primaries::Rec2020,
        ANALYSIS_EDGE,
    )
    .ok_or_else(|| invalid("photo", "the photograph has no pixels"))?;
    let readings = aura_portrait::readings::read(&canvas, &map);
    let (ops, mut explanation) = suggest(&readings, &input.style);
    let adjustments: Vec<RegionAdjustmentDto> =
        settings_dto(&input.photo_id, &base, Vec::new()).adjustments;
    let mut proposal = with_portrait(&base, &ops, &adjustments, None)?;
    proposal.provenance.source = EditSource::Ai;
    proposal.provenance.confidence = map
        .faces
        .iter()
        .map(|f| f.confidence)
        .fold(0.0_f32, f32::max)
        .min(0.6);
    let (merged, report) = schema::merge(&base, &proposal, EditSource::Ai)?;
    schema::Validation::check(&merged)?;
    if report.refused.iter().any(|path| path == "retouch") {
        explanation = vec![
            "You have set this photograph's retouch yourself, so AURA left it as it is."
                .to_string(),
        ];
    } else {
        state.recipe_store().save(
            &project,
            &photo,
            &merged,
            &report.changed,
            "Automatic portrait retouch: measured from this face; no trained model used",
        )?;
    }
    Ok(settings_dto(&input.photo_id, &merged, explanation))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral() -> Recipe {
        aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "Bench-01")
    }

    #[test]
    fn writing_portrait_settings_keeps_every_other_phases_operators_and_masks() {
        let mut base = neutral();
        base.retouch.push(RetouchOp {
            op: "blemish".to_string(),
            strength: 0.4,
            protect_texture: 0.8,
            mask: Some("skin".to_string()),
            borrowed_from: None,
        });
        base.masks.push(Mask {
            id: "m1".to_string(),
            kind: MaskKind::Radial,
            target: None,
            invert_of: None,
            feather: 0.5,
            params: MaskParams {
                exposure: Some(0.2),
                ..MaskParams::default()
            },
        });
        let ops = vec![
            PortraitOpDto {
                op: "teeth_whiten".to_string(),
                strength: 0.5,
            },
            PortraitOpDto {
                op: "skin_smooth".to_string(),
                strength: 0.3,
            },
        ];
        let adjustments = vec![RegionAdjustmentDto {
            region: "background".to_string(),
            exposure: Some(-0.5),
            ..RegionAdjustmentDto::default()
        }];
        let out = with_portrait(&base, &ops, &adjustments, Some(&[[0.2, 0.2, 0.3, 0.4]])).unwrap();
        schema::Validation::check(&out).unwrap();
        assert!(out.retouch.iter().any(|op| op.op == "blemish"));
        // The panel's order, not the input's.
        let names: Vec<&str> = out
            .retouch
            .iter()
            .filter(|op| aura_render::portrait::is_operator(&op.op))
            .map(|op| op.op.as_str())
            .collect();
        assert_eq!(names, vec!["skin_smooth", "teeth_whiten"]);
        assert!(out.masks.iter().any(|m| m.id == "m1"));
        assert!(out.masks.iter().any(|m| m.id == "portrait-background"));
        assert_eq!(aura_render::portrait::hints(&out).len(), 1);
        let dto = settings_dto("p", &out, Vec::new());
        assert_eq!(dto.ops.len(), 2);
        assert_eq!(dto.adjustments.len(), 1);
        assert_eq!(dto.hints.len(), 1);
        assert_eq!(dto.foreign_ops, vec!["blemish".to_string()]);
    }

    #[test]
    fn an_unknown_operator_or_region_is_refused() {
        let bad_op = vec![PortraitOpDto {
            op: "slim_face".to_string(),
            strength: 1.0,
        }];
        assert!(with_portrait(&neutral(), &bad_op, &[], None).is_err());
        let bad_region = vec![RegionAdjustmentDto {
            region: "wings".to_string(),
            exposure: Some(1.0),
            ..RegionAdjustmentDto::default()
        }];
        assert!(with_portrait(&neutral(), &[], &bad_region, None).is_err());
    }

    #[test]
    fn a_suggestion_follows_the_measurements_and_the_style() {
        let readings = aura_portrait::PortraitReadings {
            faces: 1,
            skin_roughness: Some(5.0),
            tone_variation: Some(4.0),
            under_eye_depth: Some(6.0),
            shine_share: Some(0.01),
            teeth_yellowness: None,
            sclera_redness: Some(3.0),
            hair_coverage: 0.05,
            face_lightness: Some(60.0),
            frame_lightness: 40.0,
        };
        let (natural, why) = suggest(&readings, "natural");
        assert!(!natural.is_empty() && natural.len() == why.len());
        // A closed mouth gets no whitening and white eyes get none either.
        assert!(natural
            .iter()
            .all(|op| op.op != "teeth_whiten" && op.op != "sclera_whiten"));
        let smooth = |ops: &[PortraitOpDto]| {
            ops.iter()
                .find(|op| op.op == "skin_smooth")
                .map_or(0.0, |op| op.strength)
        };
        let (soft, _) = suggest(&readings, "soft");
        let (polished, _) = suggest(&readings, "polished");
        assert!(smooth(&soft) < smooth(&natural) && smooth(&natural) < smooth(&polished));
        assert!(suggest(&readings, "off").0.is_empty());
        let nobody = aura_portrait::PortraitReadings::default();
        assert!(suggest(&nobody, "natural").0.is_empty());
    }
}
