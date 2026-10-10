//! Masks in the Studio: create one from an AI selection, a gradient or a brush, change its
//! sliders, combine it with another selection, and show where it is. ADR-0102.
//!
//! The AI selections are measured once on the photograph and stored with the recipe:
//!
//! * **Subject, background, person, face skin, body skin, hair, clothes** - the bundled person
//!   segmenter (ADR-0082), with a crop per face so a guest in a group is segmented at their own
//!   scale.
//! * **Sky** - measured from the photograph (`aura_vision::sky`), less any person in front of it.
//! * **Eyes, irises, whites of the eyes, brows, lips, teeth, beard** - portrait regions the
//!   renderer measures from the pixels on every render (ADR-0065), so nothing is stored.
//!
//! A selection that finds nothing says so, with the reason, rather than adding an empty mask.
// Mattes are at most 256 cells a side: the cell arithmetic below is exact in f32, every index
// comes from a validated width x height grid, and coverage is clamped before it becomes a byte.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::indexing_slicing
)]
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use aura_core::{AuraError, PhotoId, ProjectId};
use aura_preview::contract::service::PreviewService;
use aura_recipe::local_masks::{self, Component, LocalMask, Mode, Source};
use aura_recipe::retouch_tools::Matte;
use aura_recipe::{schema, EditSource, MaskParams};
use aura_vision::skin;
use serde::{Deserialize, Serialize};

use crate::commands::IpcResult;
use crate::contract::ipc::RecipeDto;
use crate::AppState;

fn invalid(message: &str) -> AuraError {
    let mut error = aura_core::errors::render::recipe_invalid("local mask", message);
    error.user_message = message.to_string();
    error
}

/// Start loading the learned masking models in the background. ADR-0103.
pub fn warm_up() {
    aura_vision::ai::warm_up();
}

/// Which learned selections this machine can make: `subject`, `sky`, `objects`.
#[must_use]
pub fn installed() -> Vec<(&'static str, bool)> {
    aura_vision::ai::installed()
}

/// What every mask command returns: the masks as stored, and the recipe they are in.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalMasksDto {
    pub masks: Vec<LocalMask>,
    pub recipe: RecipeDto,
    /// The mask just created or changed, when there is one.
    pub mask_id: Option<String>,
    /// Why a selection found nothing, in a sentence for the panel.
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalMasksInput {
    pub project_id: String,
    pub photo_id: String,
}

/// Create a mask, or add a component to one.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateMaskInput {
    pub project_id: String,
    pub photo_id: String,
    /// `subject`, `background`, `person`, `sky`, `face_skin`, `body_skin`, `hair`, `clothes`,
    /// a portrait region (`eyes`, `iris`, `sclera`, `eyebrows`, `lips`, `teeth`, `facial_hair`),
    /// or `geometry` with `source` given.
    pub what: String,
    /// The gradient, ellipse, brush or range, when `what` is `geometry`.
    #[serde(default)]
    pub source: Option<Source>,
    /// The box drawn around an object, normalised left, top, right, bottom, when `what` is
    /// `object`.
    #[serde(default)]
    pub bounds: Option<[f32; 4]>,
    /// Add to this mask rather than creating one.
    #[serde(default)]
    pub into: Option<String>,
    #[serde(default)]
    pub mode: Option<Mode>,
    #[serde(default)]
    pub invert: bool,
}

/// Replace the masks: sliders, geometry, strokes, names, order, deletion.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveMasksInput {
    pub project_id: String,
    pub photo_id: String,
    pub masks: Vec<LocalMask>,
    /// A short label for the history row.
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaskCoverageInput {
    pub project_id: String,
    pub photo_id: String,
    pub mask_id: String,
}

/// One mask's coverage as grey RGB bytes, for the red overlay.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskCoverageDto {
    pub width: u32,
    pub height: u32,
    pub rgb_base64: String,
}

fn ids(input_project: &str, input_photo: &str) -> Result<(ProjectId, PhotoId), AuraError> {
    let photo = PhotoId::from_db(input_photo).map_err(|_| invalid("Invalid photo"))?;
    let project = ProjectId::from_db(input_project).map_err(|_| invalid("Invalid collection"))?;
    Ok((project, photo))
}

/// Human names for the panel.
fn label(what: &str) -> &'static str {
    match what {
        "subject" => "Subject",
        "background" => "Background",
        "person" => "People",
        "sky" => "Sky",
        "face_skin" => "Face skin",
        "body_skin" => "Body skin",
        "hair" => "Hair",
        "clothes" => "Clothes",
        "eyes" => "Eyes",
        "iris" => "Irises",
        "sclera" => "Whites of the eyes",
        "eyebrows" => "Eyebrows",
        "lips" => "Lips",
        "teeth" => "Teeth",
        "facial_hair" => "Beard and moustache",
        "linear" => "Linear gradient",
        "radial" => "Radial gradient",
        "brush" => "Brush",
        "luminance" => "Brightness range",
        "object" => "Object",
        _ => "Mask",
    }
}

/// The portrait region slug the renderer resolves for a part of a face.
fn region_of(what: &str) -> Option<&'static str> {
    Some(match what {
        "eyes" => "eyes",
        "iris" => "iris",
        "sclera" => "sclera",
        "eyebrows" => "eyebrows",
        "lips" => "lips",
        "teeth" => "teeth",
        "facial_hair" => "facial_hair",
        _ => return None,
    })
}

/// The person segmentation of a photograph, kept for the last few photographs so adding
/// hair after the subject does not run the network twice.
static SEGMENTED: Mutex<Vec<(String, Arc<skin::Analysis>)>> = Mutex::new(Vec::new());

/// The proxy's packed sRGB pixels and the faces in it.
fn pixels(state: &AppState, project: &str, photo: PhotoId) -> IpcResult<(Vec<u8>, u32, u32)> {
    let previews = state.previews(project)?;
    let proxy = previews.get(
        photo,
        aura_raw::PixelLevel::Proxy2048,
        aura_preview::contract::service::Priority::Interactive,
    )?;
    let rgb = proxy
        .as_srgb8()
        .ok_or_else(|| invalid("An sRGB preview is required"))?;
    Ok((rgb.to_vec(), proxy.width, proxy.height))
}

fn segmentation(
    photo: PhotoId,
    rgb: &[u8],
    width: u32,
    height: u32,
) -> IpcResult<Arc<skin::Analysis>> {
    let key = format!(
        "{}|{}x{}|{}",
        photo.to_db(),
        width,
        height,
        blake3::hash(rgb).to_hex()
    );
    if let Some((_, found)) = SEGMENTED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|(k, _)| *k == key)
    {
        return Ok(Arc::clone(found));
    }
    let faces = aura_vision::portrait::detect(rgb, width, height)?;
    let faces =
        aura_vision::portrait::detect_small_faces(rgb, width, height, &faces).unwrap_or(faces);
    let analysis = Arc::new(skin::analyse(
        rgb,
        width,
        height,
        &faces,
        skin::Options {
            precision: 0.0,
            softness: 0.3,
            max_crops: 6,
            protect_dark_hair: false,
        },
    )?);
    let mut cache = SEGMENTED.lock().unwrap_or_else(PoisonError::into_inner);
    cache.retain(|(k, _)| *k != key);
    cache.push((key, Arc::clone(&analysis)));
    if cache.len() > 3 {
        cache.remove(0);
    }
    Ok(analysis)
}

/// The union of a class over every person, as one matte over the whole frame.
fn union(mattes: &[&skin::Matte], like: &skin::Matte) -> skin::Matte {
    let (w, h) = (like.width, like.height);
    let mut alpha = vec![0_u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            let a = mattes.iter().map(|m| m.at(u, v)).fold(0.0_f32, f32::max);
            alpha[y * w + x] = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    skin::Matte {
        bounds: [0.0, 0.0, 1.0, 1.0],
        width: w,
        height: h,
        alpha,
    }
}

fn encode(m: &skin::Matte) -> Option<Matte> {
    let mut out = Matte::encode(
        m.bounds,
        u32::try_from(m.width).ok()?,
        u32::try_from(m.height).ok()?,
        &m.alpha,
    );
    out.refine_edges = true;
    Some(out)
}

/// `m` with `other` taken out of it.
fn without(m: &skin::Matte, other: &skin::Matte) -> skin::Matte {
    let alpha = (0..m.alpha.len())
        .map(|i| {
            let (x, y) = (i % m.width, i / m.width);
            let (u, v) = (
                m.bounds[0] + (m.bounds[2] - m.bounds[0]) * (x as f32 + 0.5) / m.width as f32,
                m.bounds[1] + (m.bounds[3] - m.bounds[1]) * (y as f32 + 0.5) / m.height as f32,
            );
            let s = f32::from(m.alpha[i]) / 255.0;
            ((s * (1.0 - other.at(u, v))).clamp(0.0, 1.0) * 255.0).round() as u8
        })
        .collect();
    skin::Matte { alpha, ..m.clone() }
}

fn inverse(m: &skin::Matte) -> skin::Matte {
    skin::Matte {
        alpha: m.alpha.iter().map(|a| 255 - a).collect(),
        ..m.clone()
    }
}

/// Measure an AI selection. `Err(reason)` when it finds nothing.
///
/// The learned models (ADR-0103) are used when they are installed: the subject network for
/// subject and background, the sky network for sky, the object network for a drawn box. Without
/// them, subject and background come from the person segmenter and sky from the measured
/// horizon detector, as before; objects need their model.
fn measure(
    state: &AppState,
    project: &str,
    photo: PhotoId,
    what: &str,
    bounds: Option<[f32; 4]>,
) -> IpcResult<Result<Matte, String>> {
    let (rgb, width, height) = pixels(state, project, photo)?;
    let people = || -> IpcResult<Result<Arc<skin::Analysis>, String>> {
        let analysis = segmentation(photo, &rgb, width, height)?;
        Ok(if analysis.background.is_some() {
            Ok(analysis)
        } else {
            Err("The person segmenter could not run on this photograph.".into())
        })
    };
    let person_of = |analysis: &skin::Analysis| analysis.background.as_ref().map(inverse);
    // The subject: the learned network, or the people when it is not installed.
    let subject = || -> IpcResult<Result<skin::Matte, String>> {
        if let Ok(found) = aura_vision::ai::subject(&rgb, width, height) {
            return Ok(Ok(found));
        }
        Ok(people()?.and_then(|a| person_of(&a).ok_or_else(|| "No subject found.".into())))
    };
    let found = match what {
        "subject" => match subject()? {
            Ok(m) => m,
            Err(why) => return Ok(Err(why)),
        },
        "background" => match subject()? {
            Ok(m) => inverse(&m),
            Err(why) => return Ok(Err(why)),
        },
        "person" => match people()?.map(|a| person_of(&a)) {
            Ok(Some(m)) => m,
            Ok(None) => return Ok(Err("No people found.".into())),
            Err(why) => return Ok(Err(why)),
        },
        "object" => {
            let Some(bounds) = bounds else {
                return Ok(Err("Draw a box around the object.".into()));
            };
            match aura_vision::ai::object(&rgb, width, height, bounds) {
                Ok(m) => m,
                Err(why) => return Ok(Err(format!("Objects are not available: {why}."))),
            }
        }
        "sky" => {
            let sky = match aura_vision::ai::sky(&rgb, width, height) {
                Ok(Some(m)) => m,
                Ok(None) => {
                    return Ok(Err(
                        "No sky found: no open sky above a horizon in this photograph.".into(),
                    ))
                }
                // No sky model on this machine: the measured detector.
                Err(_) => match aura_vision::sky::find(&rgb, width, height) {
                    aura_vision::sky::Finding::Sky(m) => m,
                    aura_vision::sky::Finding::None(why) => {
                        return Ok(Err(format!("No sky found: {why}.")));
                    }
                },
            };
            // Never the sky through somebody: whatever is in front of it is not sky.
            match subject()? {
                Ok(front) => without(&sky, &front),
                Err(_) => sky,
            }
        }
        part => {
            fn pick<'a>(p: &'a skin::Person, part: &str) -> Option<&'a skin::Matte> {
                match part {
                    "face_skin" => p.face.as_ref(),
                    "body_skin" => p.body.as_ref(),
                    "hair" => p.hair.as_ref(),
                    "clothes" => p.clothes.as_ref(),
                    _ => None,
                }
            }
            let analysis = match people()? {
                Ok(a) => a,
                Err(why) => return Ok(Err(why)),
            };
            let found: Vec<&skin::Matte> = analysis
                .people
                .iter()
                .filter_map(|p| pick(p, part))
                .collect();
            let (Some(like), false) = (analysis.background.as_ref(), found.is_empty()) else {
                return Ok(Err(format!(
                    "No {} found: it needs a face the detector can see.",
                    label(part).to_lowercase()
                )));
            };
            union(&found, like)
        }
    };
    if found.area() < 0.001 {
        return Ok(Err(format!(
            "No {} found in this photograph.",
            label(what).to_lowercase()
        )));
    }
    Ok(encode(&found).ok_or_else(|| "The selection could not be stored.".to_string()))
}

fn save(
    state: &AppState,
    project: &ProjectId,
    photo: PhotoId,
    masks: &[LocalMask],
    mattes: &BTreeMap<String, Matte>,
    label: &str,
) -> IpcResult<RecipeDto> {
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    let mut proposal = base.clone();
    local_masks::write(&mut proposal, masks, mattes)?;
    let (merged, changes) = schema::merge(&base, &proposal, EditSource::User)?;
    schema::Validation::check(&merged)?;
    state
        .recipe_store()
        .save(project, &photo, &merged, &changes.changed, label)?;
    Ok(crate::develop_commands::recipe_dto(&photo.to_db(), &merged))
}

/// The masks of one photograph.
/// # Errors
/// An invalid photograph or recipe.
pub fn list(state: &AppState, input: &LocalMasksInput) -> IpcResult<LocalMasksDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let (_, photo) = ids(&input.project_id, &input.photo_id)?;
    let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
    Ok(LocalMasksDto {
        masks: local_masks::read(&recipe)?,
        recipe: crate::develop_commands::recipe_dto(&input.photo_id, &recipe),
        mask_id: None,
        message: None,
    })
}

/// The component a request names: drawn geometry, a portrait region, or an AI selection
/// measured now and stored in `mattes`. `Err(reason)` when the photograph has none.
fn selection(
    state: &AppState,
    input: &CreateMaskInput,
    photo: PhotoId,
    mattes: &mut BTreeMap<String, Matte>,
) -> IpcResult<Result<(Source, &'static str), String>> {
    if input.what == "geometry" {
        let source = input
            .source
            .clone()
            .ok_or_else(|| invalid("Draw the gradient or brush first"))?;
        let kind = match &source {
            Source::Linear { .. } => "linear",
            Source::Radial { .. } => "radial",
            Source::Brush { .. } => "brush",
            Source::Luminance { .. } => "luminance",
            Source::Matte { .. } | Source::Region { .. } => {
                return Err(invalid("Choose a selection by name").into());
            }
        };
        return Ok(Ok((source, label(kind))));
    }
    let name = label(&input.what);
    if let Some(region) = region_of(&input.what) {
        // A part of a face is measured on every render; it needs a face to be there.
        let (rgb, width, height) = pixels(state, &input.project_id, photo)?;
        if aura_vision::portrait::detect(&rgb, width, height)?.is_empty() {
            return Ok(Err(format!(
                "No {} found: it needs a face the detector can see.",
                name.to_lowercase()
            )));
        }
        let source = Source::Region {
            region: region.into(),
        };
        return Ok(Ok((source, name)));
    }
    if name == "Mask" {
        return Err(invalid("Unknown selection").into());
    }
    Ok(
        match measure(state, &input.project_id, photo, &input.what, input.bounds)? {
            Ok(matte) => {
                let bytes = serde_json::to_vec(&matte).map_err(|_| invalid("Bad selection"))?;
                let id = format!("{}-{}", input.what, &blake3::hash(&bytes).to_hex()[..12]);
                mattes.insert(id.clone(), matte);
                let source = Source::Matte {
                    matte: id,
                    what: input.what.clone(),
                };
                Ok((source, name))
            }
            Err(message) => Err(message),
        },
    )
}

/// Create a mask from a selection, or add the selection to an existing mask.
/// # Errors
/// An invalid photograph, an unknown selection, a full recipe, or a failed segmentation.
pub fn create(state: &AppState, input: &CreateMaskInput) -> IpcResult<LocalMasksDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let (project, photo) = ids(&input.project_id, &input.photo_id)?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    let mut masks = local_masks::read(&base)?;
    let mut mattes = local_masks::read_mattes(&base)?;
    let (source, name) = match selection(state, input, photo, &mut mattes)? {
        Ok(found) => found,
        Err(message) => {
            return Ok(LocalMasksDto {
                masks,
                recipe: crate::develop_commands::recipe_dto(&input.photo_id, &base),
                mask_id: input.into.clone(),
                message: Some(message),
            });
        }
    };
    let component = Component {
        mode: input.mode.unwrap_or(Mode::Add),
        invert: input.invert,
        source,
    };
    let (id, history) = if let Some(into) = &input.into {
        let mask = masks
            .iter_mut()
            .find(|m| &m.id == into)
            .ok_or_else(|| invalid("That mask no longer exists"))?;
        mask.components.push(component);
        (
            into.clone(),
            format!("Mask: {} {}", verb(component_mode(mask)), name),
        )
    } else {
        let id = format!("mask-{}", uuid::Uuid::new_v4().simple());
        let number = masks.len() + 1;
        masks.push(LocalMask {
            id: id.clone(),
            name: format!("{name} {number}"),
            enabled: true,
            amount: 1.0,
            components: vec![Component {
                mode: Mode::Add,
                ..component
            }],
            params: MaskParams::default(),
        });
        (id, format!("New mask: {name}"))
    };
    let recipe = save(state, &project, photo, &masks, &mattes, &history)?;
    Ok(LocalMasksDto {
        masks,
        recipe,
        mask_id: Some(id),
        message: None,
    })
}

fn component_mode(mask: &LocalMask) -> Mode {
    mask.components.last().map_or(Mode::Add, |c| c.mode)
}

fn verb(mode: Mode) -> &'static str {
    match mode {
        Mode::Add => "add",
        Mode::Subtract => "subtract",
        Mode::Intersect => "intersect with",
    }
}

/// Store the masks as the panel has them.
/// # Errors
/// An invalid photograph or mask, or a mask referring to a selection that is not stored.
pub fn save_all(state: &AppState, input: &SaveMasksInput) -> IpcResult<LocalMasksDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let (project, photo) = ids(&input.project_id, &input.photo_id)?;
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    let mattes = local_masks::read_mattes(&base)?;
    let label = input.label.clone().unwrap_or_else(|| "Masks".to_string());
    let recipe = save(state, &project, photo, &input.masks, &mattes, &label)?;
    Ok(LocalMasksDto {
        masks: input.masks.clone(),
        recipe,
        mask_id: None,
        message: None,
    })
}

/// Where one mask is, for the overlay.
/// # Errors
/// An invalid photograph, a mask that no longer exists, or unavailable pixels.
pub fn coverage(state: &AppState, input: &MaskCoverageInput) -> IpcResult<MaskCoverageDto> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let (_, photo) = ids(&input.project_id, &input.photo_id)?;
    let recipe = crate::develop_commands::load_or_neutral(state, photo)?;
    let (rgb, width, height) =
        state
            .render()?
            .local_mask_coverage(&photo, &recipe, &input.mask_id)?;
    Ok(MaskCoverageDto {
        width,
        height,
        rgb_base64: crate::develop_commands::base64(&rgb),
    })
}
