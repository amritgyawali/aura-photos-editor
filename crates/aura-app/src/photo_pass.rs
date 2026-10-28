//! The hundred-step photo pass: new measured checks that every imported photograph goes through
//! after Auto enhance. ADR-0186.
//!
//! Each step measures the original before it changes anything and reports one of the outcomes
//! Auto advanced retouch reports. A step that applies something is its own history entry, so a
//! person can walk back past any one of them. None of the steps re-decides a value Auto enhance
//! owns: they are faults it does not look for.
//!
//! Sliders merge as an automatic proposal, so a value a person set is kept and the step says
//! so. Operations carry [`PREFIX`]; a repeat run replaces only its own and keeps every other one.

pub mod capture;
pub mod face;
pub mod scene;

use crate::{commands::IpcResult, contract::ipc::RecipeDto, portrait_features::Pixels, AppState};
use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_preview::contract::service::{PreviewService, Priority};
use std::collections::BTreeMap;

use aura_recipe::{
    retouch_tools::{self, Edit, Matte},
    schema, EditSource, Recipe,
};
use aura_render::{FrameSource, RenderLevel};
use serde::{Deserialize, Serialize};

pub use crate::advanced_retouch::Outcome;

/// Where the pass's report is stored in the recipe.
pub const KEY: &str = "photo_pass_v1";
/// Written into the report; bump on any behavioural change.
pub const VERSION: &str = "photo-pass-v1-batch2";
/// Every operation the pass places starts with this.
pub const PREFIX: &str = "pass-";
/// How many steps the finished pass will have (ADR-0186); the catalogue grows by batch.
pub const PLANNED: usize = 100;

/// The four parts of the pass, one per batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Capture, light and colour faults (steps 1-25).
    Capture,
    /// Faces and skin, checked on the edited photograph (steps 26-50).
    Face,
}

/// One step of the catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepDef {
    pub number: u8,
    pub id: &'static str,
    pub title: &'static str,
    pub category: Category,
}

const fn step(number: u8, id: &'static str, title: &'static str) -> StepDef {
    StepDef {
        number,
        id,
        title,
        category: if number <= 25 {
            Category::Capture
        } else {
            Category::Face
        },
    }
}

/// The steps implemented so far, in the order they run.
pub const STEPS: [StepDef; 50] = [
    step(1, "sensor_dust", "Sensor dust"),
    step(2, "clipped_channels", "Clipped colour channels"),
    step(3, "colour_noise", "Colour noise"),
    step(4, "shadow_noise", "Shadow noise"),
    step(5, "smooth_noise", "Noise on smooth areas"),
    step(6, "jpeg_compression", "JPEG compression"),
    step(7, "banding", "Banding"),
    step(8, "moire", "Moiré"),
    step(9, "vignetting", "Lens vignetting"),
    step(10, "mixed_light", "Mixed light"),
    step(11, "shadow_tint", "Shadow tint"),
    step(12, "shadow_cast", "Shadow colour cast"),
    step(13, "highlight_tint", "Highlight tint"),
    step(14, "regional_haze", "Regional haze"),
    step(15, "capture_sharpening", "Capture sharpening"),
    step(16, "camera_shake", "Camera shake & focus"),
    step(17, "green_fringes", "Green & blue fringes"),
    step(18, "saturated_lights", "Over-saturated light colours"),
    step(19, "horizon", "Level horizon"),
    step(20, "pale_sky", "Pale sky"),
    step(21, "scene_texture", "Scene texture"),
    step(22, "hard_corners", "Hard corner shading"),
    step(23, "dark_foreground", "Dark foreground"),
    step(24, "sharpen_mask", "Sharpening mask for noisy files"),
    step(25, "flicker", "Flicker bands"),
    step(26, "face_exposure", "Face against its background"),
    step(27, "skin_clipping", "Skin clipped by the edit"),
    step(28, "skin_hue", "Skin hue moved by the grade"),
    step(29, "skin_saturation", "Skin saturation raised by the grade"),
    step(30, "lip_colour", "Lips that lost their colour"),
    step(31, "pores_kept", "Pores kept"),
    step(32, "skin_tone_kept", "Skin colour kept by the retouch"),
    step(33, "beard_kept", "Beard and stubble kept"),
    step(34, "brows_kept", "Eyebrows kept"),
    step(35, "hairline_kept", "Hairline kept"),
    step(36, "under_eye_kept", "Under-eyes no lighter than the cheek"),
    step(37, "teeth_natural", "Teeth no whiter than the eyes"),
    step(38, "shine_left", "Shine left after mattifying"),
    step(39, "ceremonial_kept", "Bindi, tika and sindoor kept"),
    step(40, "moles_kept", "Moles and beauty marks kept"),
    step(41, "retouch_balance", "Both halves of each face alike"),
    step(42, "retouch_leak", "Retouch stays on skin"),
    step(43, "skin_noise", "Noise lifted onto skin"),
    step(44, "eye_focus", "Eyes in focus"),
    step(45, "side_light", "Coloured light on one side of a face"),
    step(46, "face_neck_colour", "Face and neck colour"),
    step(47, "mehndi_kept", "Mehndi kept"),
    step(48, "makeup_kept", "Eye make-up kept"),
    step(49, "small_faces", "Faces too small to retouch"),
    step(50, "neck_light", "Neck in shadow"),
];

/// One change to a recipe value, applied and described the same way everywhere.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    ColourNoise(i16),
    LensVignette(i16),
    ShadowsTint(i16),
    ShadowsWheel {
        hue: i16,
        saturation: i16,
    },
    HighlightsWheel {
        hue: i16,
        saturation: i16,
    },
    SharpenRadius(f32),
    SharpenMasking(i16),
    Texture(i16),
    /// One of Auto enhance's retouch strength factors (ADR-0130), `0..=2`.
    Intensity(aura_recipe::retouch_intensity::Category, f32),
    /// Rotation in degrees (positive clockwise) and the crop that hides the corners.
    Level {
        rotate: f32,
        crop: [f32; 4],
    },
    Crop([f32; 4]),
}

impl Change {
    pub fn apply(&self, recipe: &mut Recipe) {
        let g = &mut recipe.global;
        match *self {
            Self::ColourNoise(v) => g.noise.colour = v,
            Self::LensVignette(v) => recipe.lens.vignette = v,
            Self::ShadowsTint(v) => g.calibration.shadows_tint = v,
            Self::ShadowsWheel { hue, saturation } => {
                g.colour_grade.shadows.hue = hue;
                g.colour_grade.shadows.saturation = saturation;
            }
            Self::HighlightsWheel { hue, saturation } => {
                g.colour_grade.highlights.hue = hue;
                g.colour_grade.highlights.saturation = saturation;
            }
            Self::SharpenRadius(v) => g.sharpen.radius = v,
            Self::SharpenMasking(v) => g.sharpen.masking = v,
            Self::Texture(v) => g.texture = v,
            Self::Intensity(category, factor) => {
                use aura_recipe::retouch_intensity as intensity;
                if let Ok(mut factors) = intensity::read(recipe) {
                    factors.insert(category, factor.clamp(0.0, intensity::MAX_FACTOR));
                    intensity::write(recipe, &factors).ok();
                }
            }
            Self::Level { rotate, crop } => {
                recipe.geometry.rotate = rotate;
                recipe.geometry.crop = crop;
            }
            Self::Crop(crop) => recipe.geometry.crop = crop,
        }
    }
}

/// What one step found and what it proposes.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub outcome: Outcome,
    /// What was inspected and measured.
    pub checks: Vec<String>,
    /// What it changes.
    pub changes: Vec<String>,
    pub sliders: Vec<Change>,
    /// Operations, with ids from [`op_id`].
    pub ops: Vec<Edit>,
    /// Selections the operations refer to by id.
    pub mattes: BTreeMap<String, Matte>,
}

impl Finding {
    /// The step does not apply to this photograph.
    #[must_use]
    pub fn not_applicable(why: impl Into<String>) -> Self {
        Self {
            outcome: Outcome::NotApplicable,
            checks: vec![why.into()],
            changes: Vec::new(),
            sliders: Vec::new(),
            ops: Vec::new(),
            mattes: BTreeMap::new(),
        }
    }

    /// Inspected, and nothing needs changing.
    #[must_use]
    pub fn checked(what: impl Into<String>) -> Self {
        Self {
            outcome: Outcome::Unchanged,
            ..Self::not_applicable(what)
        }
    }

    /// Inspected, and this is what it changes.
    #[must_use]
    pub fn applied(
        what: impl Into<String>,
        change: impl Into<String>,
        sliders: Vec<Change>,
        ops: Vec<Edit>,
    ) -> Self {
        Self {
            outcome: Outcome::Applied,
            checks: vec![what.into()],
            changes: vec![change.into()],
            sliders,
            ops,
            mattes: BTreeMap::new(),
        }
    }

    /// The same finding with every operation limited to `matte`, stored under `id`.
    #[must_use]
    pub fn within(mut self, id: &str, matte: Option<Matte>) -> Self {
        if let Some(matte) = matte {
            for op in &mut self.ops {
                op.matte = Some(id.to_owned());
            }
            self.mattes.insert(id.to_owned(), matte);
        }
        self
    }
}

/// The matte id of "everywhere but the people", shared by every step that uses it.
pub const OUTSIDE_PEOPLE: &str = "pass-outside-people";

/// The id of the `n`-th operation a step places: `pass-07-banding-0`.
#[must_use]
pub fn op_id(def: &StepDef, n: usize) -> String {
    format!("{PREFIX}{:02}-{}-{n}", def.number, def.id)
}

/// The step an operation belongs to, by its id; `None` for anything the pass did not place.
#[must_use]
pub fn step_of(id: &str) -> Option<u8> {
    id.strip_prefix(PREFIX)?.get(..2)?.parse().ok()
}

/// One step of the run, as reported to the photographer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepReport {
    pub number: u8,
    pub id: String,
    pub title: String,
    pub category: Category,
    pub outcome: Outcome,
    pub checks: Vec<String>,
    pub changes: Vec<String>,
    pub operations: usize,
    /// True when the step is its own history entry.
    pub saved: bool,
}

/// The whole run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub version: String,
    /// The steps the finished pass will have.
    pub planned: usize,
    pub steps: Vec<StepReport>,
    pub applied: usize,
    pub history_steps: usize,
    pub summary: String,
}

/// A request to run the pass on one photograph.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PhotoPassInput {
    pub project_id: String,
    pub photo_id: String,
}

/// The saved recipe and what every step did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoPassDto {
    pub recipe: RecipeDto,
    pub report: Report,
}

/// The report saved in a recipe, if the pass has run on it.
#[must_use]
pub fn read_report(recipe: &Recipe) -> Option<Report> {
    serde_json::from_value(recipe.extra.get(KEY)?.clone()).ok()
}

/// Measure the photograph and plan the capture steps (1-25), without saving anything.
#[must_use]
pub fn plan(scene: &scene::Scene<'_>, base: &Recipe) -> Vec<(StepDef, Finding)> {
    STEPS.into_iter().zip(capture::plan(scene, base)).collect()
}

/// Plan the face steps (26-50) against the recipe the capture steps leave, rendered.
#[must_use]
pub fn plan_faces(
    scene: &scene::Scene<'_>,
    rendered: Option<&face::Rendered>,
    current: &Recipe,
) -> Vec<(StepDef, Finding)> {
    STEPS
        .into_iter()
        .skip(25)
        .zip(face::plan(scene, rendered, current))
        .collect()
}

/// Run every step on one photograph; each step that changes it is saved as its own history step.
///
/// # Errors
/// Membership, decode, analysis or storage failures.
pub fn run(state: &AppState, input: &PhotoPassInput) -> IpcResult<PhotoPassDto> {
    let busy = crate::activity::begin(
        state,
        "edit",
        format!(
            "Photo pass: {}",
            crate::activity::photo_name(state, &input.photo_id)
        ),
    );
    let _total = crate::perf::step(state, "pass.total");
    busy.detail("Waiting for a free image worker");
    let _image_work = state.image_work_gate().acquire();
    busy.detail("Measuring the photograph");
    let (base, planned) = plan_photo(state, input)?;
    let (saves, report) = merge_all(&base, planned, |i, total, title| {
        busy.progress(i as u64, total as u64);
        busy.detail(format!("Step {} of {total}: {title}", i + 1));
    })?;
    save_all(state, input, &saves)?;
    let current = saves.last().map_or(base, |s| s.recipe.clone());
    Ok(PhotoPassDto {
        recipe: crate::develop_commands::recipe_dto(&input.photo_id, &current),
        report,
    })
}

/// Measure one photograph and plan every step, saving nothing: the saved recipe the plan was
/// made against, and each step's finding.
///
/// # Errors
/// Membership, decode or analysis failures.
pub fn plan_photo(
    state: &AppState,
    input: &PhotoPassInput,
) -> AuraResult<(Recipe, Vec<(StepDef, Finding)>)> {
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let invalid = |message: &str| aura_core::errors::render::recipe_invalid("photo", message);
    let photo =
        PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo identifier"))?;
    let previews = state.previews(&input.project_id)?;
    let thumb = previews.get(
        photo,
        aura_raw::PixelLevel::Thumb(512),
        Priority::Interactive,
    )?;
    let proxy = previews.get(
        photo,
        aura_raw::PixelLevel::Proxy2048,
        Priority::Interactive,
    )?;
    let (Some(small), Some(rgb)) = (thumb.as_srgb8(), proxy.as_srgb8()) else {
        return Err(invalid("An sRGB preview is required"));
    };
    let px = Pixels::new(rgb, proxy.width, proxy.height)
        .ok_or_else(|| invalid("The preview does not match its dimensions"))?;
    let faces = if std::env::var_os("AURA_DISABLE_AUTO_PORTRAIT").is_some_and(|v| v == "1") {
        Vec::new()
    } else {
        let found = aura_vision::portrait::detect(small, thumb.width, thumb.height)?;
        aura_vision::portrait::detect_small_faces(rgb, proxy.width, proxy.height, &found)
            .unwrap_or(found)
    };
    let frame = crate::photo_frames::CatalogFrames::new(state.clone())
        .frame(&photo, RenderLevel::Proxy2048)
        .ok();
    let capture = capture_of(state, &input.photo_id);
    let base = crate::develop_commands::load_or_neutral(state, photo)?;
    // The renderer's own parse of the faces, the one its face tools are anchored on.
    let map = frame
        .as_ref()
        .and_then(|f| aura_render::portrait::parse(f, &aura_render::portrait::hints(&base)));
    let scene = scene::Scene::new(px, faces, capture, frame).with_map(map);
    let mut planned = {
        let _t = crate::perf::step(state, "pass.plan");
        plan(&scene, &base)
    };
    // The face steps judge the edit itself: the recipe the capture steps leave, rendered with
    // and without its retouch operations.
    let (saves, _) = merge_all(&base, planned.clone(), |_, _, _| {})?;
    let current = saves
        .last()
        .map_or_else(|| base.clone(), |s| s.recipe.clone());
    let rendered = if scene.map_faces().is_empty() {
        None
    } else {
        let _t = crate::perf::step(state, "pass.render_faces");
        Some(face::Rendered::of(state, photo, &current)?)
    };
    planned.extend({
        let _t = crate::perf::step(state, "pass.plan_faces");
        plan_faces(&scene, rendered.as_ref(), &current)
    });
    Ok((base, planned))
}

/// Store each history entry [`merge_all`] produced, in order.
///
/// # Errors
/// Identifier or storage failures.
pub fn save_all(state: &AppState, input: &PhotoPassInput, saves: &[Save]) -> AuraResult<()> {
    let invalid = |message: &str| aura_core::errors::render::recipe_invalid("photo", message);
    let photo =
        PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo identifier"))?;
    let project =
        ProjectId::from_db(&input.project_id).map_err(|_| invalid("Invalid project identifier"))?;
    for save in saves {
        state
            .recipe_store()
            .save(&project, &photo, &save.recipe, &save.changed, &save.label)?;
    }
    Ok(())
}

/// One history entry to save.
#[derive(Debug, Clone)]
pub struct Save {
    pub recipe: Recipe,
    pub changed: Vec<String>,
    pub label: String,
}

/// Merge every applied step into the recipe in order, one save each, and write the report into
/// the last. Nothing is stored here, so the whole run can be tested without a catalogue.
///
/// # Errors
/// A proposal the schema refuses.
#[allow(clippy::too_many_lines)]
pub fn merge_all(
    base: &Recipe,
    planned: Vec<(StepDef, Finding)>,
    progress: impl Fn(usize, usize, &str),
) -> AuraResult<(Vec<Save>, Report)> {
    let total = planned.len();
    let mut current = base.clone();
    let mut saves: Vec<Save> = Vec::new();
    let mut steps = Vec::with_capacity(total);
    let applied_steps: Vec<u8> = planned
        .iter()
        .filter(|(_, f)| f.outcome == Outcome::Applied)
        .map(|(d, _)| d.number)
        .collect();
    // Operations a previous run placed for a step that applies nothing now are taken off.
    let stale = retouch_tools::read(base)?
        .iter()
        .any(|e| step_of(&e.id).is_some_and(|n| !applied_steps.contains(&n)));
    let owned = base.provenance.user_edited_fields.iter().any(|path| {
        path == retouch_tools::KEY || path.starts_with(&format!("{}.", retouch_tools::KEY))
    });
    for (i, (def, finding)) in planned.into_iter().enumerate() {
        progress(i, total, def.title);
        let mut report = StepReport {
            number: def.number,
            id: def.id.into(),
            title: def.title.into(),
            category: def.category,
            outcome: finding.outcome,
            checks: finding.checks.clone(),
            changes: Vec::new(),
            operations: finding.ops.len(),
            saved: false,
        };
        if finding.outcome == Outcome::Applied {
            let mut proposal = current.clone();
            for change in &finding.sliders {
                change.apply(&mut proposal);
            }
            proposal.provenance.source = EditSource::Ai;
            proposal.provenance.confidence = 0.35;
            let (mut merged, mut changes) = schema::merge(&current, &proposal, EditSource::Ai)?;
            if !finding.ops.is_empty() {
                let mut stack: Vec<Edit> = retouch_tools::read(&current)?
                    .into_iter()
                    .filter(|e| step_of(&e.id) != Some(def.number))
                    .collect();
                stack.extend(finding.ops.iter().cloned());
                retouch_tools::validate(&stack)?;
                let mut mattes = retouch_tools::read_mattes(&current)?;
                mattes.extend(finding.mattes.clone());
                let mut with_stack = merged.clone();
                retouch_tools::write_with_mattes(&mut with_stack, &stack, &mattes)?;
                let source = if owned {
                    EditSource::User
                } else {
                    EditSource::Ai
                };
                with_stack.provenance.source = source;
                let (stacked, more) = schema::merge(&merged, &with_stack, source)?;
                merged = stacked;
                changes.changed.extend(more.changed);
                changes.refused.extend(more.refused);
            }
            schema::Validation::check(&merged)?;
            // Compared as the store keeps them (six decimals): rewriting the stack round-trips
            // every stored matte, and a difference below that precision is not a change.
            let real = !changes.changed.is_empty()
                && aura_recipe::recipe_hash(&current)? != aura_recipe::recipe_hash(&merged)?;
            if !real && !changes.refused.is_empty() {
                report.outcome = Outcome::Protected;
                report.changes.push(
                    "You set this by hand, so your value was kept and nothing was changed.".into(),
                );
            } else if !real {
                // Its fix is already in the photograph from an earlier run: still applied.
                report.changes.clone_from(&finding.changes);
            } else {
                changes.changed.sort_unstable();
                changes.changed.dedup();
                report.changes.clone_from(&finding.changes);
                report.saved = true;
                saves.push(Save {
                    recipe: merged.clone(),
                    changed: changes.changed,
                    label: format!(
                        "Photo pass {}/{PLANNED} · {}: {}",
                        def.number,
                        def.title,
                        finding.changes.join(" ")
                    ),
                });
                current = merged;
            }
        }
        steps.push(report);
    }
    let applied = steps
        .iter()
        .filter(|s| s.outcome == Outcome::Applied)
        .count();
    let mut report = Report {
        version: VERSION.into(),
        planned: PLANNED,
        applied,
        history_steps: saves.len(),
        summary: format!(
            "{applied} of {} steps changed the photograph; {} found nothing to fix; {} did not apply; {} kept your own settings.",
            steps.len(),
            steps.iter().filter(|s| s.outcome == Outcome::Unchanged).count(),
            steps.iter().filter(|s| s.outcome == Outcome::NotApplicable).count(),
            steps.iter().filter(|s| s.outcome == Outcome::Protected).count(),
        ),
        steps,
    };
    // The report, and the removal of stale operations, ride on the last saved step; when no step
    // saved anything they are their own entry, and only if they change what is stored.
    let mut last = saves.pop().unwrap_or_else(|| Save {
        recipe: current.clone(),
        changed: Vec::new(),
        label: format!("Photo pass: {}", report.summary),
    });
    let mut finished = last.recipe.clone();
    if stale {
        let kept: Vec<Edit> = retouch_tools::read(&finished)?
            .into_iter()
            .filter(|e| step_of(&e.id).is_none_or(|n| applied_steps.contains(&n)))
            .collect();
        let mattes = retouch_tools::read_mattes(&finished)?;
        retouch_tools::write_with_mattes(&mut finished, &kept, &mattes)?;
    }
    if last.changed.is_empty() {
        // Nothing else saved: the report alone is history only when it is new.
        let stored = read_report(&finished);
        if !stale && stored.as_ref().is_some_and(|r| same_findings(r, &report)) {
            return Ok((
                Vec::new(),
                stored.map_or(report, |mut r| {
                    r.history_steps = 0;
                    r
                }),
            ));
        }
    }
    report.history_steps = saves.len() + 1;
    finished.extra.insert(
        KEY.into(),
        serde_json::to_value(&report).map_err(|e| {
            aura_core::errors::render::recipe_invalid("photo pass report", &e.to_string())
        })?,
    );
    finished.provenance.source = EditSource::Ai;
    let (merged, more) = schema::merge(&last.recipe, &finished, EditSource::Ai)?;
    last.changed.extend(more.changed);
    last.changed.sort_unstable();
    last.changed.dedup();
    last.recipe = merged;
    saves.push(last);
    Ok((saves, report))
}

/// True when two runs found the same thing on every step: which run saved each fix differs.
fn same_findings(a: &Report, b: &Report) -> bool {
    a.version == b.version
        && a.steps.len() == b.steps.len()
        && a.steps.iter().zip(&b.steps).all(|(x, y)| {
            (x.number, x.outcome, &x.checks, &x.changes, x.operations)
                == (y.number, y.outcome, &y.checks, &y.changes, y.operations)
        })
}

/// What the catalogue says about how the photograph was taken.
fn capture_of(state: &AppState, photo_id: &str) -> scene::Capture {
    let id = photo_id.to_owned();
    let row = state.catalog().read(move |conn| {
        let row = conn
            .query_row(
                "SELECT iso, shutter_us, focal_len_mm, width_px, aperture FROM photo WHERE photo_id = ?1",
                [&id],
                |row| {
                    Ok((
                        row.get::<_, Option<i64>>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, Option<f64>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, Option<f64>>(4)?,
                    ))
                },
            )
            .ok();
        let kind: Option<String> = conn
            .query_row(
                "SELECT f.kind FROM photo p JOIN photo_file f ON f.file_id = p.primary_file_id
                  WHERE p.photo_id = ?1",
                [&id],
                |row| row.get(0),
            )
            .ok();
        let path = aura_catalog::repo::primary_file_for_photo(conn, &id)?.map(|(p, _)| p);
        Ok((row, kind, path))
    });
    let Ok((row, kind, path)) = row else {
        return scene::Capture::default();
    };
    let (iso, shutter, focal, width, aperture) = row.unwrap_or_default();
    let jpeg_quality = (kind.as_deref() == Some("jpeg"))
        .then(|| path.and_then(|p| capture::jpeg_quality_of_file(std::path::Path::new(&p))))
        .flatten();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    scene::Capture {
        iso: iso.and_then(|v| u32::try_from(v).ok()),
        shutter: shutter.filter(|v| *v > 0).map(|us| us as f32 / 1e6),
        focal_mm: focal.filter(|v| *v > 0.0).map(|v| v as f32),
        aperture: aperture.filter(|v| *v > 0.0).map(|v| v as f32),
        width: width.and_then(|v| u32::try_from(v).ok()),
        kind,
        jpeg_quality,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_recipe::retouch_tools::Tool;

    fn recipe() -> Recipe {
        aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "t")
    }

    fn op(def: &StepDef) -> Edit {
        crate::advanced_retouch::base_edit(op_id(def, 0), Tool::Deband, [0.5, 0.5, 0.3, 0.3], 0.5)
    }

    #[test]
    fn the_catalogue_is_numbered_in_order_and_ids_are_unique() {
        for (i, s) in STEPS.iter().enumerate() {
            assert_eq!(usize::from(s.number), i + 1);
        }
        let mut ids: Vec<&str> = STEPS.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), STEPS.len());
    }

    #[test]
    fn the_first_25_steps_are_capture_and_the_next_25_are_faces() {
        assert!(STEPS[..25].iter().all(|s| s.category == Category::Capture));
        assert!(STEPS[25..].iter().all(|s| s.category == Category::Face));
    }

    #[test]
    fn a_strength_factor_change_writes_that_factor_and_keeps_the_others() {
        use aura_recipe::retouch_intensity::{self as intensity, Category as Kind};
        let mut r = recipe();
        intensity::write(&mut r, &BTreeMap::from([(Kind::Teeth, 0.8)])).unwrap();
        Change::Intensity(Kind::Smoothing, 0.6).apply(&mut r);
        let f = intensity::read(&r).unwrap();
        assert_eq!(f.get(&Kind::Smoothing), Some(&0.6));
        assert_eq!(f.get(&Kind::Teeth), Some(&0.8));
        // A person's factor is kept by the merge, like any value they set.
        let mut mine = recipe();
        intensity::write(&mut mine, &BTreeMap::from([(Kind::Smoothing, 1.2)])).unwrap();
        let (mine, _) = schema::merge(&recipe(), &mine, EditSource::User).unwrap();
        let (_, report) = merge_all(
            &mine,
            vec![(
                STEPS[30],
                Finding::applied(
                    "flat",
                    "Smoothing.",
                    vec![Change::Intensity(Kind::Smoothing, 0.6)],
                    vec![],
                ),
            )],
            |_, _, _| {},
        )
        .unwrap();
        assert_eq!(report.steps[0].outcome, Outcome::Protected);
    }

    #[test]
    fn operation_ids_name_their_step() {
        let id = op_id(&STEPS[6], 3);
        assert_eq!(id, "pass-07-banding-3");
        assert_eq!(step_of(&id), Some(7));
        assert_eq!(step_of("auto-portrait-v1-0-skin"), None);
        assert!(crate::portrait_auto::group_of(&id).is_none());
    }

    #[test]
    fn each_applied_step_is_its_own_history_entry_and_the_report_rides_on_the_last() {
        let base = recipe();
        let planned = vec![
            (
                STEPS[2],
                Finding::applied(
                    "noisy",
                    "Colour noise 30.",
                    vec![Change::ColourNoise(30)],
                    vec![],
                ),
            ),
            (STEPS[5], Finding::not_applicable("Not a JPEG.")),
            (
                STEPS[6],
                Finding::applied("banded", "Deband.", vec![], vec![op(&STEPS[6])]),
            ),
        ];
        let (saves, report) = merge_all(&base, planned, |_, _, _| {}).unwrap();
        assert_eq!(saves.len(), 2);
        assert!(saves[0].label.contains("Photo pass 3/100"));
        assert_eq!(saves[0].recipe.global.noise.colour, 30);
        let last = &saves[1].recipe;
        assert_eq!(last.global.noise.colour, 30);
        assert_eq!(retouch_tools::read(last).unwrap().len(), 1);
        assert_eq!(read_report(last), Some(report.clone()));
        assert_eq!(report.applied, 2);
        assert_eq!(report.steps[1].outcome, Outcome::NotApplicable);
    }

    #[test]
    fn an_operation_kept_off_people_carries_its_matte_into_the_recipe() {
        let base = recipe();
        let matte = Matte::encode([0.0, 0.0, 1.0, 1.0], 2, 2, &[255, 0, 255, 255]);
        let finding = Finding::applied("hazy", "Dehaze.", vec![], vec![op(&STEPS[13])])
            .within(OUTSIDE_PEOPLE, Some(matte.clone()));
        assert_eq!(finding.ops[0].matte.as_deref(), Some(OUTSIDE_PEOPLE));
        let (saves, _) = merge_all(&base, vec![(STEPS[13], finding)], |_, _, _| {}).unwrap();
        let last = &saves.last().unwrap().recipe;
        assert_eq!(
            retouch_tools::read_mattes(last)
                .unwrap()
                .get(OUTSIDE_PEOPLE),
            Some(&matte)
        );
        // Nobody in the frame: no matte, and the operation is not limited.
        let free = Finding::applied("hazy", "Dehaze.", vec![], vec![op(&STEPS[13])])
            .within(OUTSIDE_PEOPLE, None);
        assert!(free.ops[0].matte.is_none() && free.mattes.is_empty());
    }

    #[test]
    fn a_repeat_run_over_stored_mattes_saves_nothing() {
        // Auto enhance's own operation and matte, with more precision than the store keeps.
        let mut base = recipe();
        let mut theirs = op(&STEPS[0]);
        theirs.id = "auto-portrait-v1-0-skin".into();
        theirs.matte = Some("auto-portrait-v1-0-skin".into());
        let mut matte = Matte::encode(
            [0.123_456_7, 0.2, 0.876_543_2, 0.9],
            2,
            2,
            &[255, 0, 255, 255],
        );
        matte.refine_edges = true;
        let mattes = BTreeMap::from([("auto-portrait-v1-0-skin".to_owned(), matte)]);
        retouch_tools::write_with_mattes(&mut base, &[theirs], &mattes).unwrap();
        let planned = || {
            vec![(
                STEPS[19],
                Finding::applied("pale", "Sky.", vec![], vec![op(&STEPS[19])]),
            )]
        };
        let (saves, _) = merge_all(&base, planned(), |_, _, _| {}).unwrap();
        // What the store gives back is what the next run starts from.
        let stored: Recipe =
            serde_json::from_str(&aura_recipe::canonical(&saves.last().unwrap().recipe).unwrap())
                .unwrap();
        let (again, _) = merge_all(&stored, planned(), |_, _, _| {}).unwrap();
        assert!(
            again.is_empty(),
            "{:?}",
            again.iter().map(|s| &s.changed).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_value_a_person_set_is_kept_and_reported() {
        let base = recipe();
        let mut mine = base.clone();
        mine.global.noise.colour = 5;
        let (base, _) = schema::merge(&base, &mine, EditSource::User).unwrap();
        let planned = vec![(
            STEPS[2],
            Finding::applied(
                "noisy",
                "Colour noise 30.",
                vec![Change::ColourNoise(30)],
                vec![],
            ),
        )];
        let (saves, report) = merge_all(&base, planned, |_, _, _| {}).unwrap();
        assert_eq!(report.steps[0].outcome, Outcome::Protected);
        assert!(saves.iter().all(|s| s.recipe.global.noise.colour == 5));
    }

    #[test]
    fn a_repeat_run_saves_nothing_and_stale_operations_are_removed() {
        let base = recipe();
        let planned = || {
            vec![(
                STEPS[6],
                Finding::applied("banded", "Deband.", vec![], vec![op(&STEPS[6])]),
            )]
        };
        let (saves, _) = merge_all(&base, planned(), |_, _, _| {}).unwrap();
        let once = saves.last().unwrap().recipe.clone();
        let (again, report) = merge_all(&once, planned(), |_, _, _| {}).unwrap();
        assert!(again.is_empty(), "{again:?}");
        assert_eq!(report.steps[0].outcome, Outcome::Applied);
        assert!(
            report.steps[0].saved,
            "the stored report says which run saved it"
        );
        assert_eq!(report.history_steps, 0);

        // The banding is gone on a later run: its operation is taken off, others are kept.
        let mut theirs = once.clone();
        let mut stack = retouch_tools::read(&theirs).unwrap();
        let mut own = op(&STEPS[6]);
        own.id = "manual-heal-1".into();
        stack.push(own);
        let mattes = retouch_tools::read_mattes(&theirs).unwrap();
        retouch_tools::write_with_mattes(&mut theirs, &stack, &mattes).unwrap();
        let (saves, _) = merge_all(
            &theirs,
            vec![(STEPS[6], Finding::checked("No banding."))],
            |_, _, _| {},
        )
        .unwrap();
        let left = retouch_tools::read(&saves.last().unwrap().recipe).unwrap();
        assert_eq!(
            left.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["manual-heal-1"]
        );
    }
}
