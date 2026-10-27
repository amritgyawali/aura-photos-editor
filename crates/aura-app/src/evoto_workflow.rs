//! Evoto-style framing, delivery and review workflows. ADR-0150 group J.
//!
//! Four commands carry ten tools:
//!
//! * [`evoto_faces`] - the faces in a photograph and how open each eye is, for the face picker
//!   that chooses which faces a portrait finish applies to (`StudioFinish::faces`).
//! * [`apply_evoto_framing`] - an ID photograph (three sizes, on white), a headshot, a half- or
//!   full-body crop, a square or a story crop, or a level eye line, measured from the face the
//!   parse found and saved as one undoable step.
//! * [`evoto_export`] - a transparent PNG cut-out, an Instagram grid, a carousel, a before and
//!   after, or a 2x enlargement, written into a folder the photographer chose. Nothing is ever
//!   written anywhere else, and an existing file is never replaced.
//! * [`blink_check`] - the photographs in a collection with a face showing no white of the eye.
//!
//! Nothing here runs unless asked, and the framing is an ordinary recipe crop and rotation, so it
//! can be adjusted or undone like any other.
// Image buffers here are `width * height * 3` bytes made by the renderer or by this module, and
// every index is derived from those same dimensions; the tiles, slides and rows are cut from
// rectangles checked against them.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::indexing_slicing,
    clippy::many_single_char_names
)]
use std::path::{Path, PathBuf};

use aura_core::{AuraError, PhotoId, ProjectId};
use aura_portrait::{FaceGeometry, PortraitMap, Region};
use aura_recipe::{schema, EditSource, Recipe};
use aura_render::contract::render::{
    OutputColour, OutputSpec, RenderLevel, RenderPurpose, RenderService, RenderedData,
};
use aura_render::FrameSource;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::commands::IpcResult;
use crate::contract::ipc::RecipeDto;
use crate::AppState;

fn invalid(message: &str) -> AuraError {
    let mut error = aura_core::errors::render::recipe_invalid("evoto workflow", message);
    message.clone_into(&mut error.user_message);
    error
}

fn ids(project: &str, photo: &str) -> Result<(ProjectId, PhotoId), AuraError> {
    let photo = PhotoId::from_db(photo).map_err(|_| invalid("Invalid photo identifier"))?;
    let project =
        ProjectId::from_db(project).map_err(|_| invalid("Invalid collection identifier"))?;
    Ok((project, photo))
}

// ---- faces and eyes ---------------------------------------------------------------------

/// How open each eye of a face is: how much white of the eye shows in a box around it, in units
/// of the distance between the eyes squared. The parse's eye region is the same size open or
/// shut, so it cannot say; a closed lid shows no white. ADR-0150.
#[must_use]
pub fn eye_openness(map: &PortraitMap, face: &FaceGeometry) -> Option<[f32; 2]> {
    let plane = map.plane(Region::Sclera)?;
    let io = face.interocular().max(2.0);
    let (hw, hh) = (0.3 * io, 0.2 * io);
    let open = |c: [f32; 2]| {
        let mut sum = 0.0_f32;
        let (x0, x1) = ((c[0] - hw).floor() as i64, (c[0] + hw).ceil() as i64);
        let (y0, y1) = ((c[1] - hh).floor() as i64, (c[1] + hh).ceil() as i64);
        for y in y0..=y1 {
            for x in x0..=x1 {
                sum += plane.at(x, y);
            }
        }
        sum / (io * io)
    };
    (face.eyes_measured > 0).then(|| [open(face.left_eye), open(face.right_eye)])
}

/// Below this on both eyes a face is reported as showing no white of the eye. Set on the 27
/// photographs of `H:/AURA EDIT`: most open eyes read 0.002 to 0.037, and below 0.002 are
/// closed eyes, squints, glasses glare, faces under a red veil and two small, shaded open eyes. It is a list to look through, not a
/// blink verdict - pale shimmer on a closed lid reads as white - and the panel says so. A
/// verdict needs a trained eye-state model. ADR-0150.
pub const CLOSED_BELOW: f32 = 0.002;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceDto {
    /// `[left, top, right, bottom]`, normalised.
    pub bounds: [f32; 4],
    /// Centre, normalised: what `StudioFinish::faces` stores to choose this face.
    pub centre: [f32; 2],
    /// Eye-line tilt in degrees, clockwise.
    pub tilt: f32,
    /// How open the image-left and image-right eyes are, when the eyes were measured.
    pub eyes: Option<[f32; 2]>,
    /// True when neither eye shows any white: closed, or a squint, glare or a veil.
    pub closed: bool,
    pub confidence: f32,
}

fn face_dto(map: &PortraitMap, g: &FaceGeometry) -> FaceDto {
    let (sx, sy) = (map.width.max(1) as f32, map.height.max(1) as f32);
    let eyes = eye_openness(map, g);
    FaceDto {
        bounds: [
            (g.center[0] - g.size / 2.0) / sx,
            (g.center[1] - g.size / 2.0) / sy,
            (g.center[0] + g.size / 2.0) / sx,
            (g.center[1] + g.size / 2.0) / sy,
        ]
        .map(|v| v.clamp(0.0, 1.0)),
        centre: [g.center[0] / sx, g.center[1] / sy],
        tilt: g.roll.to_degrees(),
        eyes,
        closed: eyes.is_some_and(|e| e[0] < CLOSED_BELOW && e[1] < CLOSED_BELOW),
        confidence: g.confidence,
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PhotoInput {
    pub project_id: String,
    pub photo_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FacesDto {
    pub faces: Vec<FaceDto>,
}

/// The renderer's own parse of a photograph: the frame as decoded, before any slider moved,
/// which is what every portrait tool reads.
///
/// # Errors
///
/// The photograph cannot be read.
pub fn parse_of(
    state: &AppState,
    photo: PhotoId,
    recipe: &Recipe,
) -> Result<(aura_render::cpu::Frame, Option<std::sync::Arc<PortraitMap>>), AuraError> {
    let frame = crate::photo_frames::CatalogFrames::new(state.clone())
        .frame(&photo, RenderLevel::Proxy2048)?;
    let map = aura_render::portrait::parse(&frame, &aura_render::portrait::hints(recipe));
    Ok((frame, map))
}

/// The faces in one photograph, largest first, with how open each eye is.
/// # Errors
/// An invalid photograph, or pixels that cannot be read.
pub fn evoto_faces(state: &AppState, input: &PhotoInput) -> IpcResult<FacesDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let (_, photo) = ids(&input.project_id, &input.photo_id)?;
    let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
    let (_, map) = parse_of(state, photo, &recipe)?;
    Ok(FacesDto {
        faces: map.as_deref().map_or_else(Vec::new, |m| {
            m.faces.iter().map(|g| face_dto(m, g)).collect()
        }),
    })
}

// ---- framing ----------------------------------------------------------------------------

/// A head measured in frame pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Head {
    pub cx: f32,
    pub eye_y: f32,
    pub crown_y: f32,
    pub chin_y: f32,
    /// Eye-line tilt, radians clockwise.
    pub roll: f32,
}

impl Head {
    fn height(&self) -> f32 {
        (self.chin_y - self.crown_y).max(1.0)
    }
}

/// Measure a head from the parse, in pixels of a `w` x `h` frame: the chin from the mouth, the
/// crown from where the hair and the face stop above the eyes.
#[must_use]
pub fn head_of(map: &PortraitMap, g: &FaceGeometry, w: f32, h: f32) -> Head {
    let (sx, sy) = (w / map.width.max(1) as f32, h / map.height.max(1) as f32);
    let io = g.interocular().max(2.0);
    let eye = [
        (g.left_eye[0] + g.right_eye[0]) * 0.5,
        (g.left_eye[1] + g.right_eye[1]) * 0.5,
    ];
    let mouth_down = (g.mouth[1] - eye[1]).clamp(0.8 * io, 1.6 * io);
    let chin = eye[1] + mouth_down + 0.8 * io;
    let weight = |x: i64, y: i64| {
        [Region::Hair, Region::Face, Region::Skin]
            .iter()
            .filter_map(|r| map.plane(*r))
            .map(|p| p.at(x, y))
            .fold(0.0_f32, f32::max)
    };
    let mut crown = eye[1] - 0.6 * io;
    let mut gap = 0.0_f32;
    let mut y = crown;
    let band = (0.25 * io).max(1.0);
    while y > 0.0 && y > eye[1] - 2.8 * io {
        let mut best = 0.0_f32;
        let mut x = eye[0] - band;
        while x <= eye[0] + band {
            best = best.max(weight(x as i64, y as i64));
            x += (band / 4.0).max(1.0);
        }
        if best > 0.3 {
            crown = y;
            gap = 0.0;
        } else {
            gap += 1.0;
            if gap > 0.15 * io {
                break;
            }
        }
        y -= 1.0;
    }
    let crown = crown.clamp(eye[1] - 2.6 * io, eye[1] - 1.3 * io);
    Head {
        cx: eye[0] * sx,
        eye_y: eye[1] * sy,
        crown_y: crown * sy,
        chin_y: chin * sy,
        roll: g.roll,
    }
}

/// What a framing preset makes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Preset {
    /// An ID photograph: width over height, the head's share of the height, and the space above
    /// the crown as a share of the height.
    Id {
        aspect: f32,
        head: f32,
        above: f32,
    },
    Headshot,
    HalfBody,
    FullBody,
    Square,
    Story,
    LevelEyes,
}

impl Preset {
    /// The preset a wire name means.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            // ICAO / Schengen / Nepal passport: 35 x 45 mm, head 32-36 mm.
            "id_35x45" => Self::Id {
                aspect: 35.0 / 45.0,
                head: 0.74,
                above: 0.09,
            },
            // US passport and visa: 2 x 2 in, head 1 - 1 3/8 in.
            "id_2x2" => Self::Id {
                aspect: 1.0,
                head: 0.6,
                above: 0.11,
            },
            // 33 x 48 mm (China visa): head 28-33 mm.
            "id_33x48" => Self::Id {
                aspect: 33.0 / 48.0,
                head: 0.64,
                above: 0.08,
            },
            "headshot" => Self::Headshot,
            "half_body" => Self::HalfBody,
            "full_body" => Self::FullBody,
            "square" => Self::Square,
            "story" => Self::Story,
            "level_eyes" => Self::LevelEyes,
            _ => return None,
        })
    }

    fn is_id(self) -> bool {
        matches!(self, Self::Id { .. })
    }
}

/// The crop a preset makes around a head, normalised, with what had to give.
///
/// `person` is the whole person's box in pixels, for the full-body crop.
#[must_use]
pub fn crop_for(
    preset: Preset,
    head: &Head,
    person: Option<[f32; 4]>,
    w: f32,
    h: f32,
) -> ([f32; 4], Vec<String>) {
    let mut reasons = Vec::new();
    let hh = head.height();
    // Height, aspect (width / height) and top edge, in pixels.
    let (mut ch, aspect, top_of): (f32, f32, Box<dyn Fn(f32) -> f32>) = match preset {
        Preset::Id {
            aspect,
            head: share,
            above,
        } => {
            let crown = head.crown_y;
            (hh / share, aspect, Box::new(move |c| crown - above * c))
        }
        Preset::Headshot => {
            let eye = head.eye_y;
            (hh / 0.45, 0.8, Box::new(move |c| eye - 0.36 * c))
        }
        Preset::HalfBody => {
            let crown = head.crown_y;
            (hh * 3.0, 0.8, Box::new(move |c| crown - 0.12 * c))
        }
        Preset::Square => {
            let crown = head.crown_y;
            (hh / 0.5, 1.0, Box::new(move |c| crown - 0.15 * c))
        }
        #[allow(clippy::single_match_else)]
        Preset::FullBody => match person {
            Some(p) => {
                let top = p[1];
                (
                    (p[3] - p[1]) * 1.12,
                    2.0 / 3.0,
                    Box::new(move |c| top - 0.06 * c),
                )
            }
            None => {
                reasons.push(
                    "No whole person was found; the full-body crop is sized from the head."
                        .to_owned(),
                );
                let crown = head.crown_y;
                (hh * 8.0, 2.0 / 3.0, Box::new(move |c| crown - 0.06 * c))
            }
        },
        Preset::Story => {
            let eye = head.eye_y;
            (h, 9.0 / 16.0, Box::new(move |c| eye - 0.3 * c))
        }
        Preset::LevelEyes => return ([0.0, 0.0, 1.0, 1.0], reasons),
    };
    let mut cw = ch * aspect;
    let fit = (w / cw).min(h / ch).min(1.0);
    if fit < 0.999 {
        cw *= fit;
        ch *= fit;
        if preset.is_id() {
            reasons.push(format!(
                "The photograph is too tight for the standard head size: the head fills {:.0}% of the height instead.",
                hh / ch * 100.0
            ));
        } else {
            reasons.push("The crop was made smaller to stay inside the photograph.".to_owned());
        }
    }
    let wanted_left = head.cx - cw / 2.0;
    let wanted_top = top_of(ch);
    let left = wanted_left.clamp(0.0, (w - cw).max(0.0));
    let top = wanted_top.clamp(0.0, (h - ch).max(0.0));
    if (left - wanted_left).abs() > cw * 0.02 || (top - wanted_top).abs() > ch * 0.02 {
        reasons.push(
            "The face is near the edge of the photograph, so it sits off-centre in the crop."
                .to_owned(),
        );
    }
    (
        [left / w, top / h, (left + cw) / w, (top + ch) / h].map(|v| v.clamp(0.0, 1.0)),
        reasons,
    )
}

/// The largest crop of the same aspect, centred where `crop` is, that a rotation by `degrees`
/// leaves free of empty corners.
#[must_use]
pub fn level_crop(crop: [f32; 4], degrees: f32, w: f32, h: f32) -> [f32; 4] {
    let (cw, ch) = ((crop[2] - crop[0]) * w, (crop[3] - crop[1]) * h);
    let t = degrees.to_radians().abs();
    let (s, c) = t.sin_cos();
    let scale = (cw / (cw * c + ch * s))
        .min(ch / (cw * s + ch * c))
        .min(1.0);
    let (mx, my) = ((crop[0] + crop[2]) * 0.5, (crop[1] + crop[3]) * 0.5);
    let (hx, hy) = (
        (crop[2] - crop[0]) * 0.5 * scale,
        (crop[3] - crop[1]) * 0.5 * scale,
    );
    [mx - hx, my - hy, mx + hx, my + hy]
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FramingInput {
    pub project_id: String,
    pub photo_id: String,
    /// `id_35x45`, `id_2x2`, `id_33x48`, `headshot`, `half_body`, `full_body`, `square`,
    /// `story` or `level_eyes`.
    pub preset: String,
    /// The face to frame, normalised; the largest face when absent.
    #[serde(default)]
    pub face: Option<[f32; 2]>,
    /// Level the eye line as well. An ID photograph is always levelled.
    #[serde(default)]
    pub level: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FramingDto {
    pub crop: [f32; 4],
    pub rotate: f32,
    pub white_background: bool,
    pub reasons: Vec<String>,
    pub recipe: RecipeDto,
}

/// The whole person's box in frame pixels.
fn person_box(map: &PortraitMap, w: f32, h: f32) -> Option<[f32; 4]> {
    let plane = map.plane(Region::Body)?;
    let (pw, ph) = (plane.width as usize, plane.height as usize);
    let (mut x0, mut y0, mut x1, mut y1) = (pw, ph, 0, 0);
    for y in 0..ph {
        for x in 0..pw {
            if plane.at(x as i64, y as i64) > 0.5 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    let (sx, sy) = (w / pw.max(1) as f32, h / ph.max(1) as f32);
    (x0 < x1 && y0 < y1).then_some({
        [
            x0 as f32 * sx,
            y0 as f32 * sy,
            x1 as f32 * sx,
            y1 as f32 * sy,
        ]
    })
}

/// Frame a photograph with a preset and save it as one manual edit.
/// # Errors
/// An invalid photograph or preset, no face to frame from, or a failed save.
pub fn apply_evoto_framing(state: &AppState, input: &FramingInput) -> IpcResult<FramingDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let (project, photo) = ids(&input.project_id, &input.photo_id)?;
    let preset = Preset::parse(&input.preset).ok_or_else(|| invalid("Unknown framing preset."))?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    let (frame, map) = parse_of(state, photo, &base)?;
    let map = map.ok_or_else(|| invalid("The photograph could not be read for faces."))?;
    let (w, h) = (frame.width as f32, frame.height as f32);
    let face = match input.face {
        Some(p) => map.faces.iter().min_by(|a, b| {
            let d = |g: &&FaceGeometry| {
                (g.center[0] / map.width as f32 - p[0])
                    .hypot(g.center[1] / map.height as f32 - p[1])
            };
            d(a).total_cmp(&d(b))
        }),
        None => map.faces.first(),
    }
    .ok_or_else(|| {
        invalid("No face was found. Draw the face in Portrait retouch, then frame again.")
    })?;
    let head = head_of(&map, face, w, h);
    let (mut crop, mut reasons) = if preset == Preset::LevelEyes {
        (base.geometry.crop, Vec::new())
    } else {
        crop_for(preset, &head, person_box(&map, w, h), w, h)
    };
    let level = input.level || preset.is_id() || preset == Preset::LevelEyes;
    let rotate = if level {
        (-head.roll.to_degrees()).clamp(-15.0, 15.0)
    } else {
        base.geometry.rotate
    };
    if level && rotate.abs() > 0.05 {
        crop = level_crop(crop, rotate, w, h);
        reasons.push(format!(
            "The eye line was levelled by {:.1} degrees.",
            -rotate
        ));
    }
    let mut proposal = base.clone();
    proposal.geometry.crop = crop;
    proposal.geometry.rotate = rotate;
    let white = preset.is_id();
    if white {
        // The finishing extension has one writer (ADR-0108's guard): the white background goes
        // through it.
        if let Some(source) = crate::finish_commands::white_id_background(
            state,
            &input.project_id,
            photo,
            &base,
            &mut proposal,
        )? {
            reasons.push(format!(
                "The person was separated from the background by {source}."
            ));
        }
        reasons
            .push("The background was replaced with white, as ID photographs require.".to_owned());
    }
    let (merged, changes) = schema::merge(&base, &proposal, EditSource::User)?;
    schema::Validation::check(&merged)?;
    let label = format!("Framing: {}", input.preset.replace('_', " "));
    state
        .recipe_store()
        .save(&project, &photo, &merged, &changes.changed, &label)?;
    Ok(FramingDto {
        crop: merged.geometry.crop,
        rotate: merged.geometry.rotate,
        white_background: white,
        reasons,
        recipe: crate::develop_commands::recipe_dto(&input.photo_id, &merged),
    })
}

// ---- delivery ---------------------------------------------------------------------------

/// What [`evoto_export`] writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    Cutout,
    Grid { rows: u32 },
    Carousel { slides: u32 },
    BeforeAfter,
    Upscale,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportInput {
    pub project_id: String,
    pub photo_id: String,
    /// `png_cutout`, `grid`, `carousel`, `before_after` or `upscale_2x`.
    pub kind: String,
    /// The folder to write into. It must be chosen by the photographer.
    pub folder: String,
    /// Grid rows (1 to 3) or carousel slides (2 to 10).
    #[serde(default)]
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDto {
    pub files: Vec<String>,
    pub width: u32,
    pub height: u32,
    pub reasons: Vec<String>,
}

fn kind_of(input: &ExportInput) -> Result<ExportKind, AuraError> {
    Ok(match input.kind.as_str() {
        "png_cutout" => ExportKind::Cutout,
        "grid" => ExportKind::Grid {
            rows: input.count.unwrap_or(1).clamp(1, 3),
        },
        "carousel" => ExportKind::Carousel {
            slides: input.count.unwrap_or(3).clamp(2, 10),
        },
        "before_after" => ExportKind::BeforeAfter,
        "upscale_2x" => ExportKind::Upscale,
        _ => return Err(invalid("Unknown export.")),
    })
}

fn render_full(
    state: &AppState,
    photo: PhotoId,
    recipe: Recipe,
) -> Result<(Vec<u8>, u32, u32), AuraError> {
    let rendered = state
        .render()?
        .render(aura_render::contract::render::RenderRequest {
            image_id: photo,
            recipe,
            level: RenderLevel::Full,
            output: OutputSpec {
                colour_space: OutputColour::Srgb,
                bit_depth: 8,
                icc: None,
            },
            purpose: RenderPurpose::Export,
        })?;
    let bytes = match rendered.data {
        RenderedData::Eight(bytes) => bytes,
        RenderedData::Sixteen(words) => words.iter().map(|v| (v >> 8) as u8).collect(),
    };
    Ok((bytes, rendered.width, rendered.height))
}

/// The photograph's own file name, when the catalog has one.
fn file_name_of(state: &AppState, photo: &str) -> Option<String> {
    let id = photo.to_owned();
    state
        .catalog()
        .read(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT f.file_name FROM photo p JOIN photo_file f ON f.file_id = p.primary_file_id
                     WHERE p.photo_id = ?1",
                    [&id],
                    |row| row.get(0),
                )
                .ok())
        })
        .ok()
        .flatten()
}

/// The stem of the photograph's own file name, for the files written beside it.
fn stem_of(state: &AppState, photo: &str) -> String {
    let name = file_name_of(state, photo);
    let stem = name
        .as_deref()
        .and_then(|n| Path::new(n).file_stem())
        .and_then(|s| s.to_str())
        .map_or_else(|| photo.chars().take(8).collect(), str::to_owned);
    stem.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// A path in `folder` that does not exist yet.
fn fresh(folder: &Path, name: &str, ext: &str) -> PathBuf {
    let first = folder.join(format!("{name}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..10_000)
        .map(|n| folder.join(format!("{name}-{n}.{ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

fn write_jpeg(path: &Path, rgb: &[u8], w: u32, h: u32) -> Result<(), AuraError> {
    let file = std::fs::File::create(path).map_err(|e| write_failed(path, &e.to_string()))?;
    let mut out = std::io::BufWriter::new(file);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 93)
        .encode(rgb, w, h, image::ExtendedColorType::Rgb8)
        .map_err(|e| write_failed(path, &e.to_string()))
}

fn write_png_rgba(path: &Path, rgba: &[u8], w: u32, h: u32) -> Result<(), AuraError> {
    use image::ImageEncoder;
    let file = std::fs::File::create(path).map_err(|e| write_failed(path, &e.to_string()))?;
    let out = std::io::BufWriter::new(file);
    image::codecs::png::PngEncoder::new(out)
        .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
        .map_err(|e| write_failed(path, &e.to_string()))
}

fn write_failed(path: &Path, why: &str) -> AuraError {
    invalid(&format!("Could not write {}: {why}", path.display()))
}

/// A rectangle cut out of an RGB image.
#[must_use]
pub fn cut(rgb: &[u8], w: u32, rect: [u32; 4]) -> Vec<u8> {
    let [x0, y0, x1, y1] = rect;
    let mut out = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 3) as usize);
    for y in y0..y1 {
        let start = ((y * w + x0) * 3) as usize;
        let end = ((y * w + x1) * 3) as usize;
        out.extend_from_slice(rgb.get(start..end).unwrap_or_default());
    }
    out
}

/// The largest centred rectangle of `aspect` (width over height) inside `w` x `h`.
#[must_use]
pub fn centred(w: u32, h: u32, aspect: f32) -> [u32; 4] {
    let (wf, hf) = (w as f32, h as f32);
    let (cw, ch) = if wf / hf > aspect {
        (hf * aspect, hf)
    } else {
        (wf, wf / aspect)
    };
    let (cw, ch) = (cw.floor() as u32, ch.floor() as u32);
    let x0 = (w - cw) / 2;
    let y0 = (h - ch) / 2;
    [x0, y0, x0 + cw, y0 + ch]
}

/// Tiles for an Instagram grid: the image cropped to three columns by `rows`, cut into
/// squares, each with the order it must be posted in (the bottom-right square first).
#[must_use]
pub fn grid_tiles(w: u32, h: u32, rows: u32) -> Vec<(u32, u32, u32, [u32; 4])> {
    let area = centred(w, h, 3.0 / rows as f32);
    let side = (area[2] - area[0]) / 3;
    let total = 3 * rows;
    let mut out = Vec::new();
    for r in 0..rows {
        for c in 0..3 {
            let x0 = area[0] + c * side;
            let y0 = area[1] + r * side;
            let post = total - (r * 3 + c);
            out.push((post, r, c, [x0, y0, x0 + side, y0 + side]));
        }
    }
    out
}

/// Slides for a carousel: the image cropped to `slides` portraits of 4:5 side by side.
#[must_use]
pub fn carousel_slides(w: u32, h: u32, slides: u32) -> Vec<[u32; 4]> {
    let area = centred(w, h, slides as f32 * 0.8);
    let each = (area[2] - area[0]) / slides;
    (0..slides)
        .map(|i| {
            let x0 = area[0] + i * each;
            [x0, area[1], x0 + each, area[3]]
        })
        .collect()
}

/// Two images of one height side by side with a white divider.
#[must_use]
pub fn side_by_side(a: &[u8], b: &[u8], w: u32, h: u32, gap: u32) -> (Vec<u8>, u32) {
    let out_w = w * 2 + gap;
    let mut out = vec![255_u8; (out_w * h * 3) as usize];
    out.par_chunks_mut((out_w * 3) as usize)
        .enumerate()
        .for_each(|(y, row)| {
            let line = (y as u32 * w * 3) as usize..((y as u32 + 1) * w * 3) as usize;
            if let (Some(left), Some(right)) = (a.get(line.clone()), b.get(line)) {
                row[..left.len()].copy_from_slice(left);
                let start = ((w + gap) * 3) as usize;
                row[start..start + right.len()].copy_from_slice(right);
            }
        });
    (out, out_w)
}

fn lanczos(x: f32) -> f32 {
    let a = 3.0_f32;
    if x.abs() < 1e-6 {
        1.0
    } else if x.abs() >= a {
        0.0
    } else {
        let px = std::f32::consts::PI * x;
        a * px.sin() * (px / a).sin() / (px * px)
    }
}

/// Lanczos-3 weights for resampling `n` samples to `m`.
fn taps(n: u32, m: u32) -> Vec<(i64, Vec<f32>)> {
    let scale = n as f32 / m as f32;
    (0..m)
        .map(|i| {
            let centre = (i as f32 + 0.5) * scale - 0.5;
            let first = (centre - 3.0).floor() as i64 + 1;
            let mut w: Vec<f32> = (0..6)
                .map(|k| lanczos(centre - (first + k) as f32))
                .collect();
            let sum: f32 = w.iter().sum();
            for v in &mut w {
                *v /= sum.max(1e-6);
            }
            (first, w)
        })
        .collect()
}

/// An RGB image enlarged by `factor` with a Lanczos-3 filter, a band of rows at a time so the
/// working memory stays small whatever the size.
#[must_use]
pub fn upscale(rgb: &[u8], w: u32, h: u32, factor: f32) -> (Vec<u8>, u32, u32) {
    let (w2, h2) = (
        ((w as f32) * factor).round() as u32,
        ((h as f32) * factor).round() as u32,
    );
    let (tx, ty) = (taps(w, w2), taps(h, h2));
    let mut out = vec![0_u8; (w2 * h2 * 3) as usize];
    let band = 64_usize;
    out.par_chunks_mut((w2 * 3) as usize * band)
        .enumerate()
        .for_each(|(b, rows)| {
            let y_start = b * band;
            let y_end = (y_start + band).min(h2 as usize);
            let first = ty[y_start].0.max(0);
            let last = (ty[y_end - 1].0 + 5).min(i64::from(h) - 1);
            // The input rows this band needs, resampled across once.
            let across: Vec<Vec<f32>> = (first..=last)
                .map(|y| {
                    let row = &rgb[(y as u32 * w * 3) as usize..((y as u32 + 1) * w * 3) as usize];
                    let mut line = vec![0.0_f32; (w2 * 3) as usize];
                    for (x2, (start, weights)) in tx.iter().enumerate() {
                        let mut acc = [0.0_f32; 3];
                        for (k, wt) in weights.iter().enumerate() {
                            let x = (start + k as i64).clamp(0, i64::from(w) - 1) as usize;
                            for c in 0..3 {
                                acc[c] += f32::from(row[x * 3 + c]) * wt;
                            }
                        }
                        line[x2 * 3..x2 * 3 + 3].copy_from_slice(&acc);
                    }
                    line
                })
                .collect();
            for (j, row) in rows.chunks_mut((w2 * 3) as usize).enumerate() {
                let y2 = y_start + j;
                let (start, weights) = &ty[y2];
                // The 2 x 2 source pixels nearest this row's centre bound what it may become.
                let sy = ((y2 as f32 + 0.5) / factor - 0.5).max(0.0);
                let (ya, yb) = ((sy as u32).min(h - 1), (sy as u32 + 1).min(h - 1));
                for x2 in 0..(w2 * 3) as usize {
                    let mut acc = 0.0_f32;
                    for (k, wt) in weights.iter().enumerate() {
                        let y = (start + k as i64).clamp(first, last);
                        acc += across[(y - first) as usize][x2] * wt;
                    }
                    // Lanczos rings by about a tenth of an edge's height; clamping to the
                    // nearest source pixels removes the halo and keeps the sharpness.
                    let (px, c) = ((x2 / 3) as f32, x2 % 3);
                    let sx = ((px + 0.5) / factor - 0.5).max(0.0);
                    let (xa, xb) = ((sx as u32).min(w - 1), (sx as u32 + 1).min(w - 1));
                    let near = [(xa, ya), (xb, ya), (xa, yb), (xb, yb)]
                        .map(|(x, y)| f32::from(rgb[((y * w + x) * 3) as usize + c]));
                    let lo = near.iter().copied().fold(f32::INFINITY, f32::min);
                    let hi = near.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                    row[x2] = acc.clamp(lo, hi).round().clamp(0.0, 255.0) as u8;
                }
            }
        });
    (out, w2, h2)
}

/// The people's matte for a rendered image, `0..=255` per pixel: the learned subject model when
/// it is installed, otherwise the portrait parse's person region. Says which it used.
fn cutout_alpha(rgb: &[u8], w: u32, h: u32) -> (Vec<u8>, String) {
    if let Ok(matte) = aura_vision::ai::subject(rgb, w, h) {
        let mut alpha = vec![0_u8; (w * h) as usize];
        let [bx0, by0, bx1, by1] = matte.bounds;
        alpha
            .par_chunks_mut(w as usize)
            .enumerate()
            .for_each(|(y, row)| {
                let v = (y as f32 + 0.5) / h as f32;
                for (x, slot) in row.iter_mut().enumerate() {
                    let u = (x as f32 + 0.5) / w as f32;
                    if u < bx0 || u > bx1 || v < by0 || v > by1 {
                        continue;
                    }
                    let gx = ((u - bx0) / (bx1 - bx0).max(1e-6) * matte.width as f32) as usize;
                    let gy = ((v - by0) / (by1 - by0).max(1e-6) * matte.height as f32) as usize;
                    let i = gy.min(matte.height.saturating_sub(1)) * matte.width
                        + gx.min(matte.width.saturating_sub(1));
                    *slot = matte.alpha.get(i).copied().unwrap_or(0);
                }
            });
        return (
            alpha,
            "The cut-out follows AURA's learned subject selection.".to_owned(),
        );
    }
    let map = aura_portrait::Canvas::from_srgb8(rgb, w, h, aura_portrait::ANALYSIS_EDGE)
        .map(|c| aura_portrait::analyse(&c, &[]));
    let plane = map.map(|m| m.resolve(Region::Body, w, h));
    let alpha = plane.map_or_else(
        || vec![255_u8; (w * h) as usize],
        |p| {
            p.values
                .iter()
                .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect()
        },
    );
    (
        alpha,
        "The learned subject selection is not installed here, so the cut-out follows the portrait parse's person region; check the edges.".to_owned(),
    )
}

/// Write a cut-out, grid, carousel, before and after, or enlargement of one photograph.
/// # Errors
/// An invalid photograph, folder or kind, a render that fails, or a file that cannot be written.
pub fn evoto_export(state: &AppState, input: &ExportInput) -> IpcResult<ExportDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let (_, photo) = ids(&input.project_id, &input.photo_id)?;
    let kind = kind_of(input)?;
    let folder = PathBuf::from(input.folder.trim());
    if input.folder.trim().is_empty() || !folder.is_absolute() {
        return Err(invalid("Choose a folder to write into.").into());
    }
    std::fs::create_dir_all(&folder).map_err(|e| write_failed(&folder, &e.to_string()))?;
    let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
    let stem = stem_of(state, &input.photo_id);
    let mut files = Vec::new();
    let mut reasons = Vec::new();
    let (rgb, w, h) = render_full(state, photo, recipe.clone())?;
    let (mut out_w, mut out_h) = (w, h);
    match kind {
        ExportKind::Cutout => {
            let (alpha, why) = cutout_alpha(&rgb, w, h);
            reasons.push(why);
            let rgba: Vec<u8> = rgb
                .chunks_exact(3)
                .zip(&alpha)
                .flat_map(|(p, a)| [p[0], p[1], p[2], *a])
                .collect();
            let path = fresh(&folder, &format!("{stem}-cutout"), "png");
            write_png_rgba(&path, &rgba, w, h)?;
            files.push(path);
        }
        ExportKind::Grid { rows } => {
            for (post, r, c, rect) in grid_tiles(w, h, rows) {
                let tile = cut(&rgb, w, rect);
                let side = rect[2] - rect[0];
                let path = fresh(
                    &folder,
                    &format!("{stem}-grid-post{post:02}-r{}c{}", r + 1, c + 1),
                    "jpg",
                );
                write_jpeg(&path, &tile, side, side)?;
                files.push(path);
                (out_w, out_h) = (side, side);
            }
            reasons
                .push("Post the files in the order of their post number: post01 first.".to_owned());
        }
        ExportKind::Carousel { slides } => {
            for (i, rect) in carousel_slides(w, h, slides).into_iter().enumerate() {
                let slide = cut(&rgb, w, rect);
                let path = fresh(
                    &folder,
                    &format!("{stem}-carousel-{}of{slides}", i + 1),
                    "jpg",
                );
                write_jpeg(&path, &slide, rect[2] - rect[0], rect[3] - rect[1])?;
                files.push(path);
                (out_w, out_h) = (rect[2] - rect[0], rect[3] - rect[1]);
            }
        }
        ExportKind::BeforeAfter => {
            let mut before = crate::develop_commands::neutral_recipe(state, photo);
            before.geometry = recipe.geometry.clone();
            let (b, bw, bh) = render_full(state, photo, before)?;
            if (bw, bh) != (w, h) {
                return Err(invalid("The before and after renders differ in size.").into());
            }
            let gap = (w / 200).max(4);
            let (both, both_w) = side_by_side(&b, &rgb, w, h, gap);
            let path = fresh(&folder, &format!("{stem}-before-after"), "jpg");
            write_jpeg(&path, &both, both_w, h)?;
            files.push(path);
            out_w = both_w;
        }
        ExportKind::Upscale => {
            // At most 120 megapixels out, so the file stays one an editor can open.
            let factor = (120.0e6 / (w as f32 * h as f32)).sqrt().min(2.0);
            if factor < 1.999 {
                reasons.push(format!(
                    "Enlarged {factor:.3}x rather than 2x, to stay under 120 megapixels."
                ));
            }
            let (big, bw, bh) = upscale(&rgb, w, h, factor);
            let path = fresh(&folder, &format!("{stem}-x{factor:.1}"), "jpg");
            write_jpeg(&path, &big, bw, bh)?;
            files.push(path);
            (out_w, out_h) = (bw, bh);
        }
    }
    Ok(ExportDto {
        files: files.iter().map(|p| p.display().to_string()).collect(),
        width: out_w,
        height: out_h,
        reasons,
    })
}

// ---- review -----------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlinkInput {
    pub project_id: String,
    /// The photographs to check; every photograph in the collection when absent.
    #[serde(default)]
    pub photo_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlinkPhotoDto {
    pub photo_id: String,
    /// The photograph's file name, so a list of them can be read: ids all start alike.
    pub file_name: String,
    pub faces: Vec<FaceDto>,
    /// How many faces show no white of either eye.
    pub closed: usize,
    /// How many faces had no eyes measured, and so could not be checked.
    pub unchecked: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlinkDto {
    pub checked: usize,
    pub flagged: Vec<BlinkPhotoDto>,
    pub without_faces: usize,
}

/// Every photograph in a collection with a face showing no white of either eye.
/// # Errors
/// An invalid collection, or a catalog read that fails.
pub fn blink_check(state: &AppState, input: &BlinkInput) -> IpcResult<BlinkDto> {
    let project = input.project_id.clone();
    ProjectId::from_db(&project).map_err(|_| invalid("Invalid collection identifier"))?;
    let photos: Vec<String> = match &input.photo_ids {
        Some(list) => list.iter().take(5000).cloned().collect(),
        None => state.catalog().read(move |conn| {
            let mut stmt = conn
                .prepare("SELECT photo_id FROM photo WHERE project_id = ?1 ORDER BY capture_time, photo_id LIMIT 5000")
                .map_err(|e| aura_core::errors::db::statement_failed("blink check", &e))?;
            let rows = stmt
                .query_map([&project], |row| row.get::<_, String>(0))
                .map_err(|e| aura_core::errors::db::statement_failed("blink check", &e))?;
            Ok(rows.filter_map(Result::ok).collect())
        })?,
    };
    let mut flagged = Vec::new();
    let mut without_faces = 0;
    for id in &photos {
        crate::studio_tools::require_member(state, &input.project_id, id)?;
        let photo = PhotoId::from_db(id).map_err(|_| invalid("Invalid photo identifier"))?;
        let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
        let Ok((_, Some(map))) = parse_of(state, photo, &recipe) else {
            without_faces += 1;
            continue;
        };
        if map.faces.is_empty() {
            without_faces += 1;
            continue;
        }
        let faces: Vec<FaceDto> = map.faces.iter().map(|g| face_dto(&map, g)).collect();
        let closed = faces.iter().filter(|f| f.closed).count();
        let unchecked = faces.iter().filter(|f| f.eyes.is_none()).count();
        if closed > 0 {
            flagged.push(BlinkPhotoDto {
                photo_id: id.clone(),
                file_name: file_name_of(state, id).unwrap_or_else(|| id.clone()),
                faces,
                closed,
                unchecked,
            });
        }
    }
    Ok(BlinkDto {
        checked: photos.len(),
        flagged,
        without_faces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head() -> Head {
        Head {
            cx: 500.0,
            eye_y: 400.0,
            crown_y: 300.0,
            chin_y: 600.0,
            roll: 0.0,
        }
    }

    #[test]
    fn an_id_crop_has_the_standard_head_size_and_aspect() {
        let (crop, reasons) = crop_for(
            Preset::parse("id_35x45").unwrap(),
            &head(),
            None,
            1000.0,
            1500.0,
        );
        let (cw, ch) = ((crop[2] - crop[0]) * 1000.0, (crop[3] - crop[1]) * 1500.0);
        assert!((cw / ch - 35.0 / 45.0).abs() < 0.01, "{cw} {ch}");
        assert!(
            (300.0 / ch - 0.74).abs() < 0.01,
            "head share {}",
            300.0 / ch
        );
        // The crown sits nine per cent of the height below the top.
        assert!(((300.0 - crop[1] * 1500.0) / ch - 0.09).abs() < 0.01);
        assert!(reasons.is_empty(), "{reasons:?}");
    }

    #[test]
    fn a_tight_photograph_says_the_head_is_larger_than_the_standard() {
        let (crop, reasons) = crop_for(
            Preset::parse("id_2x2").unwrap(),
            &head(),
            None,
            400.0,
            420.0,
        );
        assert!(crop.iter().all(|v| (0.0..=1.0).contains(v)));
        assert!(
            reasons.iter().any(|r| r.contains("too tight")),
            "{reasons:?}"
        );
    }

    #[test]
    fn a_face_near_the_edge_is_kept_inside_the_frame() {
        let mut h = head();
        h.cx = 30.0;
        let (crop, reasons) = crop_for(Preset::Headshot, &h, None, 1000.0, 1500.0);
        assert!(crop[0].abs() < 1e-6);
        assert!(reasons.iter().any(|r| r.contains("off-centre")));
    }

    #[test]
    fn a_level_crop_leaves_no_empty_corner() {
        let crop = level_crop([0.0, 0.0, 1.0, 1.0], 5.0, 1200.0, 800.0);
        let (cw, ch) = ((crop[2] - crop[0]) * 1200.0, (crop[3] - crop[1]) * 800.0);
        let t = 5.0_f32.to_radians();
        // Rotated back, the crop's half extents fit inside the frame's.
        assert!(cw / 2.0 * t.cos() + ch / 2.0 * t.sin() <= 600.0 + 1e-3);
        assert!(cw / 2.0 * t.sin() + ch / 2.0 * t.cos() <= 400.0 + 1e-3);
        assert!((cw / ch - 1.5).abs() < 1e-3);
    }

    #[test]
    fn grid_tiles_are_squares_posted_bottom_right_first() {
        let tiles = grid_tiles(3000, 2500, 2);
        assert_eq!(tiles.len(), 6);
        assert!(tiles.iter().all(|t| t.3[2] - t.3[0] == t.3[3] - t.3[1]));
        let first = tiles.iter().find(|t| t.0 == 1).unwrap();
        assert_eq!((first.1, first.2), (1, 2));
    }

    #[test]
    fn carousel_slides_are_four_by_five_and_side_by_side() {
        let slides = carousel_slides(6000, 4000, 3);
        assert_eq!(slides.len(), 3);
        for s in &slides {
            let ratio = (s[2] - s[0]) as f32 / (s[3] - s[1]) as f32;
            assert!((ratio - 0.8).abs() < 0.01, "{ratio}");
        }
        assert_eq!(slides[0][2], slides[1][0]);
    }

    #[test]
    fn an_enlargement_keeps_a_flat_colour_flat_and_doubles_the_size() {
        let rgb = vec![120_u8; 40 * 30 * 3];
        let (big, w, h) = upscale(&rgb, 40, 30, 2.0);
        assert_eq!((w, h), (80, 60));
        assert!(big.iter().all(|v| *v == 120));
        // A step stays a step: no ringing past either side of it.
        let step: Vec<u8> = (0..30)
            .flat_map(|_| (0..40).flat_map(|x| if x < 20 { [40_u8; 3] } else { [200_u8; 3] }))
            .collect();
        let (big, w, _) = upscale(&step, 40, 30, 2.0);
        let row: Vec<u8> = (0..w as usize)
            .map(|x| big[(30 * w as usize + x) * 3])
            .collect();
        assert!(row.iter().all(|v| (40..=200).contains(v)), "{row:?}");
        assert!(row.windows(2).all(|p| p[1] >= p[0]), "{row:?}");
    }

    #[test]
    fn side_by_side_puts_the_before_on_the_left() {
        let a = vec![10_u8; 4 * 2 * 3];
        let b = vec![200_u8; 4 * 2 * 3];
        let (both, w) = side_by_side(&a, &b, 4, 2, 2);
        assert_eq!(w, 10);
        assert_eq!(both[0], 10);
        assert_eq!(both[(4 * 3) as usize], 255);
        assert_eq!(both[(6 * 3) as usize], 200);
    }
}
