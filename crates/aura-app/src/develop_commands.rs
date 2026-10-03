//! The develop command surface.
//!
//! Nine commands. Three read - the edit, the history, the project's coverage - four change
//! an edit, one renders, and one reports what the renderer can do.
//!
//! # What is not here, and cannot be added without an ADR
//!
//! **No command names a destination.** There is no export, no save-as, no path in any input
//! shape. Invariant 1: the original is opened read-only, and phase 30 owns delivery.
//!
//! **No command can overwrite a parameter a person set.** `set_param` goes through
//! `aura_recipe::schema::merge` with `EditSource::User` - which is a person, and a person may
//! always overwrite a person - and every automated pass from phase 15 onward goes through the
//! same function with an automated source and is refused. There is no argument on this
//! surface that switches the protection off, because the protection is not implemented on
//! this surface.
//!
//! **No command decides a parameter value.** Phases 15 to 17 do that. Everything here either
//! reports a number or applies one a person typed.

use aura_core::{AuraError, PhotoId, ProjectId};
use aura_recipe::history::{History, ResetTo};
use aura_recipe::{fixtures as recipe_fixtures, schema, EditSource, Recipe};
use aura_render::contract::render::{
    OutputColour, OutputSpec, RenderLevel, RenderPurpose, RenderService, RenderedData,
};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    DevelopImageInput, DevelopParamDto, DevelopStatusDto, HistoryDto, HistoryEntryDto,
    HistoryStepInput, IpcError, RecipeDto, RenderCapsDto, RenderDto, RenderImageInput,
    RenderNoteDto, SetParamDto, SetParamInput, SnapshotInput,
};
use crate::state::AppState;

/// One photograph's edit, or the neutral starting point when it has none.
///
/// Never `None`. A photograph without a stored recipe has not been edited, which is not the
/// same as having no edit to show: the panel needs somewhere to put the first slider move,
/// and the camera's own starting point is what that is.
///
/// # Errors
///
/// `AURA-DB-3006` when the stored edit cannot be read, `AURA-RENDER-8005` when a stored body
/// is not a recipe.
pub fn image_recipe(state: &AppState, input: &DevelopImageInput) -> IpcResult<RecipeDto> {
    let photo = parse_photo(&input.photo_id)?;
    let recipe = load_or_neutral(state, photo)?;
    Ok(recipe_dto(&input.photo_id, &recipe))
}

/// Change one parameter, as a person.
///
/// # Errors
///
/// `AURA-RENDER-8002` when the path is unknown or the value has the wrong shape, and
/// `AURA-DB-3006` when the save fails.
pub fn set_param(state: &AppState, input: &SetParamInput) -> IpcResult<SetParamDto> {
    let project = parse_project(&input.project_id)?;
    let photo = parse_photo(&input.photo_id)?;
    let base = load_or_neutral(state, photo)?;

    let proposal = apply_path(&base, &input.path, &input.value)?;
    let (merged, report) = schema::merge(&base, &proposal, EditSource::User)?;
    let merged = merged.clamped();
    schema::Validation::check(&merged)?;

    let label = input
        .label
        .clone()
        .unwrap_or_else(|| control_label(&input.path));
    state
        .recipe_store()
        .save(&project, &photo, &merged, &report.changed, &label)?;

    Ok(SetParamDto {
        recipe: recipe_dto(&input.photo_id, &merged),
        changed: report.changed.clone(),
        invalidated_from: aura_render::graph::earliest_affected(&report.changed)
            .map(|stage| stage.as_str().to_string()),
    })
}

/// Undo, redo, or one of the two resets.
///
/// # Errors
///
/// `AURA-RENDER-8002` when the action is unknown or a reset has nothing to go back to.
pub fn history_step(state: &AppState, input: &HistoryStepInput) -> IpcResult<SetParamDto> {
    let project = parse_project(&input.project_id)?;
    let photo = parse_photo(&input.photo_id)?;
    let mut history = load_history(state, photo)?;
    let before = history.current().clone();
    let clock = state.clock();

    let label = match input.action.as_str() {
        "undo" => {
            if history.undo().is_none() {
                return Err(IpcError::from(aura_core::errors::render::recipe_invalid(
                    "history",
                    "nothing to undo",
                )));
            }
            "Undo"
        }
        "redo" => {
            if history.redo().is_none() {
                return Err(IpcError::from(aura_core::errors::render::recipe_invalid(
                    "history",
                    "nothing to redo",
                )));
            }
            "Redo"
        }
        "reset_original" => {
            history.reset(ResetTo::Original, clock.as_ref())?;
            "Reset to original"
        }
        "reset_ai" => {
            history.reset(ResetTo::AiSuggestion, clock.as_ref())?;
            "Reset to AI suggestion"
        }
        other if other.starts_with("goto:") => {
            // Jump to the state right after a listed step (`goto:0` is before every step).
            // Recorded as one navigation row, so redo keeps working and nothing is discarded.
            let seq: u64 = other
                .trim_start_matches("goto:")
                .parse()
                .map_err(|_| aura_core::errors::render::recipe_invalid("action", other))?;
            let target = if seq == 0 {
                0
            } else {
                history
                    .entries()
                    .iter()
                    .position(|entry| entry.seq == seq)
                    .map(|index| index + 1)
                    .ok_or_else(|| {
                        aura_core::errors::render::recipe_invalid(
                            "history",
                            "that step no longer exists",
                        )
                    })?
            };
            let mut cursor = history_cursor(&history);
            while cursor > target && history.undo().is_some() {
                cursor -= 1;
            }
            while cursor < target && history.redo().is_some() {
                cursor += 1;
            }
            "Go to step"
        }
        other => {
            return Err(IpcError::from(aura_core::errors::render::recipe_invalid(
                "action", other,
            )))
        }
    };

    let current = history.current().clone();
    let changed = aura_recipe::history::changed_paths(&before, &current);
    // Navigation is an append-only journal event, not a new edit branch. These
    // reserved paths cannot be authored by set_param or an automated recipe merge.
    let mut recorded = changed.clone();
    match input.action.as_str() {
        "undo" => recorded.push("$history.undo".into()),
        "redo" => recorded.push("$history.redo".into()),
        goto if goto.starts_with("goto:") => {
            if changed.is_empty() {
                // Already there: nothing to record.
                return Ok(SetParamDto {
                    recipe: recipe_dto(&input.photo_id, &current),
                    changed: Vec::new(),
                    invalidated_from: None,
                });
            }
            recorded.push(format!("$history.{goto}"));
        }
        _ => {}
    }
    state
        .recipe_store()
        .save(&project, &photo, &current, &recorded, label)?;

    Ok(SetParamDto {
        recipe: recipe_dto(&input.photo_id, &current),
        changed: changed.clone(),
        invalidated_from: aura_render::graph::earliest_affected(&changed)
            .map(|stage| stage.as_str().to_string()),
    })
}

/// Take or restore a named snapshot.
///
/// # Errors
///
/// `AURA-RENDER-8002` when the name is empty, already taken, or unknown.
pub fn snapshot(state: &AppState, input: &SnapshotInput) -> IpcResult<HistoryDto> {
    let project = parse_project(&input.project_id)?;
    let photo = parse_photo(&input.photo_id)?;
    crate::studio_tools::require_member(state, &input.project_id, &input.photo_id)?;
    let mut history = load_history(state, photo)?;
    let clock = state.clock();

    match input.action.as_str() {
        "take" => {
            history.snapshot(input.name.clone(), clock.as_ref())?;
            state
                .recipe_store()
                .put_snapshot(&photo, &input.name, history.current())?;
        }
        "restore" => {
            let before = history.current().clone();
            history.restore(&input.name, clock.as_ref())?;
            let current = history.current().clone();
            let changed = aura_recipe::history::changed_paths(&before, &current);
            state.recipe_store().save(
                &project,
                &photo,
                &current,
                &changed,
                &format!("Restore snapshot: {}", input.name),
            )?;
        }
        other => {
            return Err(IpcError::from(aura_core::errors::render::recipe_invalid(
                "action", other,
            )))
        }
    }

    image_history(
        state,
        &DevelopImageInput {
            photo_id: input.photo_id.clone(),
        },
    )
}

/// One photograph's history.
///
/// # Errors
///
/// `AURA-DB-3006` when the history cannot be read.
pub fn image_history(state: &AppState, input: &DevelopImageInput) -> IpcResult<HistoryDto> {
    let photo = parse_photo(&input.photo_id)?;
    let history = load_history(state, photo)?;
    Ok(HistoryDto {
        photo_id: input.photo_id.clone(),
        entries: history
            .entries()
            .iter()
            .map(|entry| HistoryEntryDto {
                seq: entry.seq,
                at_ms: entry.at_ms,
                source: entry.source.as_str().to_string(),
                changed: entry.changed.clone(),
                label: entry.label.clone(),
            })
            .collect(),
        snapshots: history
            .snapshots()
            .iter()
            .map(|snapshot| snapshot.name.clone())
            .collect(),
        can_undo: history.can_undo(),
        can_redo: history.can_redo(),
        has_ai_suggestion: history.ai_suggestion().is_some(),
    })
}

/// How much of a wedding has an edit.
///
/// # Errors
///
/// `AURA-DB-3006` when the view cannot be read.
pub fn develop_status(state: &AppState, project_id: &str) -> IpcResult<DevelopStatusDto> {
    let project = parse_project(project_id)?;
    let coverage = state.recipe_store().coverage(&project)?;
    Ok(DevelopStatusDto {
        images: coverage.images,
        with_recipe: coverage.with_recipe,
        from_ai: coverage.from_ai,
        from_user: coverage.from_user,
        touched_by_hand: coverage.touched_by_hand,
        sidecar_behind: coverage.sidecar_behind,
    })
}

/// Render one photograph.
///
/// # Errors
///
/// `AURA-RENDER-8002` when the recipe is invalid, and whatever the frame source raises when
/// the pixels cannot be obtained.
pub fn render_image(state: &AppState, input: &RenderImageInput) -> IpcResult<RenderDto> {
    let photo = parse_photo(&input.photo_id)?;
    let recipe = load_or_neutral(state, photo)?;

    let level = match input.level.as_deref() {
        Some("full") => RenderLevel::Full,
        Some("screen") => {
            let (w, h) = input.screen.unwrap_or((1920, 1080));
            RenderLevel::Screen(w, h)
        }
        _ => RenderLevel::Proxy2048,
    };
    let colour_space = match input.colour_space.as_deref() {
        Some("adobe_rgb") => OutputColour::AdobeRgb,
        Some("display_p3") => OutputColour::DisplayP3,
        _ => OutputColour::Srgb,
    };
    let purpose = match input.purpose.as_deref() {
        Some("export") => RenderPurpose::Export,
        Some("analysis") => RenderPurpose::Analysis,
        _ => RenderPurpose::Interactive,
    };

    let engine = state.render()?;
    let rendered = engine.render(aura_render::contract::render::RenderRequest {
        image_id: photo,
        recipe,
        level,
        output: OutputSpec {
            colour_space,
            bit_depth: 8,
            icc: None,
        },
        purpose,
    })?;

    let bytes = match &rendered.data {
        RenderedData::Eight(bytes) => bytes.clone(),
        // A 16-bit render is never asked for on this surface - the panel is a viewer - and
        // narrowing here rather than refusing keeps the command total.
        RenderedData::Sixteen(words) => words.iter().map(|w| (w >> 8) as u8).collect(),
    };

    Ok(RenderDto {
        width: rendered.width,
        height: rendered.height,
        rgb_base64: base64(&bytes),
        colour_space: rendered.colour_space.as_str().to_string(),
        icc: aura_render::output::icc_name(rendered.colour_space).to_string(),
        render_hash: rendered.render_hash,
        backend: rendered.backend,
        stages_run: rendered.stages_run,
        notes: rendered
            .notes
            .iter()
            .map(|note| RenderNoteDto {
                stage: note.stage.clone(),
                reason: note.reason.as_str().to_string(),
                detail: note.detail.clone(),
                is_caveat: note.reason.is_a_caveat(),
            })
            .collect(),
        ms: rendered.ms,
    })
}

/// What this machine's renderer can do, and what it is running without.
///
/// # Errors
///
/// Whatever building the engine raises.
pub fn render_caps(state: &AppState) -> IpcResult<RenderCapsDto> {
    let engine = state.render()?;
    let caps = engine.capabilities();
    let degradation = engine.degradation();
    Ok(RenderCapsDto {
        backend: caps.backend.as_str().to_string(),
        max_texture: caps.max_texture,
        precision_bits: caps.precision_bits,
        max_working_bytes: caps.max_working_bytes,
        engine: caps.engine,
        degradation: degradation.as_ref().map(|e| e.code.to_string()),
        degradation_message: degradation.as_ref().map(|e| e.user_message.clone()),
    })
}

// ---------------------------------------------------------------------------
// Conversions and helpers.
// ---------------------------------------------------------------------------

pub(crate) fn recipe_dto(photo_id: &str, recipe: &Recipe) -> RecipeDto {
    let protected: Vec<&str> = recipe
        .provenance
        .user_edited_fields
        .iter()
        .map(String::as_str)
        .collect();
    let value = serde_json::to_value(recipe).unwrap_or(serde_json::Value::Null);

    let params = schema::paths(recipe)
        .into_iter()
        .filter(|path| !path.starts_with("provenance") && !path.starts_with("image"))
        .map(|path| {
            let leaf = path
                .split('.')
                .try_fold(&value, |acc, part| acc.get(part))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            DevelopParamDto {
                protected: protected.contains(&path.as_str()),
                stage: aura_render::graph::stage_for(&path).map(|s| s.as_str().to_string()),
                path,
                value: leaf,
            }
        })
        .collect();

    RecipeDto {
        photo_id: photo_id.to_string(),
        body: aura_recipe::canonical(recipe).unwrap_or_default(),
        recipe_hash: aura_recipe::recipe_hash(recipe).unwrap_or_default(),
        schema: recipe.schema,
        engine: recipe.engine.clone(),
        source: recipe.provenance.source.as_str().to_string(),
        confidence: recipe.provenance.confidence,
        decision_id: recipe.provenance.decision_id.clone(),
        user_edited_fields: recipe.provenance.user_edited_fields.clone(),
        params,
    }
}

/// Build a proposal that differs from `base` at exactly one path.
///
/// The path is written into the document as JSON and parsed back, so a value of the wrong
/// shape - a string where a number belongs - is refused by serde rather than silently
/// coerced. `AURA-RENDER-8002` is what the caller sees.
fn apply_path(base: &Recipe, path: &str, value: &serde_json::Value) -> Result<Recipe, AuraError> {
    if path.starts_with("provenance") || path.starts_with("image") || path == "schema" {
        return Err(aura_core::errors::render::recipe_invalid(
            path,
            "not a parameter a person may set",
        ));
    }
    let mut document = serde_json::to_value(base)
        .map_err(|e| aura_core::errors::render::recipe_invalid(path, &e.to_string()))?;

    let parts: Vec<&str> = path.split('.').collect();
    let mut cursor = &mut document;
    for (index, part) in parts.iter().enumerate() {
        // ADR-0065. A neutral optional block, an untouched HSL band and a colour photograph's
        // black-and-white block are *absent* from the document rather than present and zero,
        // so the first time a person sets one of their fields the block is filled in with its
        // defaults here. Only blocks the schema defines can be created; anything else is still
        // "no such field".
        let prefix = parts.get(..index).map(|p| p.join(".")).unwrap_or_default();
        if let serde_json::Value::Object(map) = cursor {
            let missing =
                !map.contains_key(*part) || map.get(*part).is_some_and(serde_json::Value::is_null);
            if missing && index + 1 < parts.len() {
                if let Some(block) = default_block(&prefix, part) {
                    map.insert((*part).to_string(), block);
                }
            }
        }
        if index + 1 == parts.len() {
            let serde_json::Value::Object(map) = cursor else {
                return Err(aura_core::errors::render::recipe_invalid(
                    path,
                    "no such field",
                ));
            };
            // The black-and-white mix is a map keyed by band, and a band nobody moved is absent.
            let mix_band = prefix == "bw.mix" && aura_recipe::HSL_BANDS.contains(part);
            if !map.contains_key(*part) && !mix_band {
                return Err(aura_core::errors::render::recipe_invalid(
                    path,
                    "no such field",
                ));
            }
            map.insert((*part).to_string(), value.clone());
            break;
        }
        let serde_json::Value::Object(map) = cursor else {
            return Err(aura_core::errors::render::recipe_invalid(
                path,
                "no such field",
            ));
        };
        cursor = map
            .get_mut(*part)
            .ok_or_else(|| aura_core::errors::render::recipe_invalid(path, "no such field"))?;
    }

    serde_json::from_value(document)
        .map_err(|e| aura_core::errors::render::recipe_invalid(path, &e.to_string()))
}

/// Input to [`sync_settings`]: Lightroom's Sync Settings.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettingsInput {
    /// The collection.
    pub project_id: String,
    /// The photograph whose develop settings are copied.
    pub source_photo_id: String,
    /// The photographs to copy onto; empty means every other photograph in the collection.
    #[serde(default)]
    pub target_photo_ids: Vec<String>,
    /// Also copy the crop and straighten. Off by default, as in Lightroom: a crop belongs to
    /// the composition of one frame.
    #[serde(default)]
    pub include_geometry: bool,
    /// Explicit groups; absent preserves the original sync command's behaviour.
    #[serde(default)]
    pub groups: Option<Vec<String>>,
}

/// What [`sync_settings`] did.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettingsReport {
    /// Photographs whose settings now match the source.
    pub synced: usize,
    /// Photographs that could not be written, with the reason.
    pub failed: Vec<String>,
}

/// Copy one photograph's develop settings onto others, as a person's edit.
///
/// Recorded as [`EditSource::User`], because choosing to sync is a person's decision about those
/// photographs: every copied field is protected from later automatic passes, exactly as if it had
/// been set by hand on each frame. Identity, masks, retouching and cleanup are never copied - they
/// describe one frame's pixels.
///
/// # Errors
///
/// `AURA-RENDER-8002` for an invalid identifier, `AURA-DB-3006` when the catalog cannot be read.
pub fn sync_settings(state: &AppState, input: &SyncSettingsInput) -> IpcResult<SyncSettingsReport> {
    let project = parse_project(&input.project_id)?;
    let source_id = parse_photo(&input.source_photo_id)?;
    crate::studio_tools::require_member(state, &input.project_id, &input.source_photo_id)?;
    let source = load_or_neutral(state, source_id)?;
    // Validate before touching any target, including an empty collection.
    let _ = sync_proposal(&source, &source, input)?;
    let targets: Vec<String> = if input.target_photo_ids.is_empty() {
        let key = input.project_id.clone();
        state.catalog().read(move |conn| {
            let mut stmt = conn
                .prepare("SELECT photo_id FROM photo WHERE project_id = ?1 ORDER BY photo_id")
                .map_err(|e| aura_core::errors::db::statement_failed("sync targets", &e))?;
            let rows = stmt
                .query_map([key], |row| row.get::<_, String>(0))
                .map_err(|e| aura_core::errors::db::statement_failed("sync targets", &e))?;
            rows.collect::<Result<Vec<String>, _>>()
                .map_err(|e| aura_core::errors::db::statement_failed("sync targets", &e))
        })?
    } else {
        input.target_photo_ids.clone()
    };
    let mut report = SyncSettingsReport {
        synced: 0,
        failed: Vec::new(),
    };
    let unique: std::collections::BTreeSet<_> = targets.iter().collect();
    for target in unique
        .into_iter()
        .filter(|id| **id != input.source_photo_id)
    {
        let result = (|| -> Result<(), AuraError> {
            crate::studio_tools::require_member(state, &input.project_id, target)?;
            let photo = PhotoId::from_db(target).map_err(|_| {
                aura_core::errors::render::recipe_invalid("photo", "invalid photo identifier")
            })?;
            let base = load_or_neutral(state, photo)?;
            let proposal = sync_proposal(&base, &source, input)?;
            let (merged, change) = schema::merge(&base, &proposal, EditSource::User)?;
            let merged = merged.clamped();
            schema::Validation::check(&merged)?;
            state.recipe_store().save(
                &project,
                &photo,
                &merged,
                &change.changed,
                "Synced settings",
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => report.synced += 1,
            Err(error) => report
                .failed
                .push(format!("{target}: {}", error.user_message)),
        }
    }
    Ok(report)
}

fn sync_proposal(
    base: &Recipe,
    source: &Recipe,
    input: &SyncSettingsInput,
) -> Result<Recipe, AuraError> {
    let mut proposal = base.clone();
    let Some(groups) = &input.groups else {
        proposal.global = source.global.clone();
        proposal.bw.clone_from(&source.bw);
        proposal.lens.vignette = source.lens.vignette;
        if input.include_geometry {
            proposal.geometry = source.geometry.clone();
        }
        for band in aura_recipe::HSL_BANDS {
            proposal.global.hsl.entry(band.to_string()).or_default();
        }
        return Ok(proposal);
    };
    if groups.is_empty() {
        return Err(aura_core::errors::render::recipe_invalid(
            "groups",
            "Select at least one settings group",
        ));
    }
    let src = &source.global;
    let dst = &mut proposal.global;
    for group in groups {
        match group.as_str() {
            "tone" => {
                dst.exposure = src.exposure;
                dst.contrast = src.contrast;
                dst.highlights = src.highlights;
                dst.shadows = src.shadows;
                dst.whites = src.whites;
                dst.blacks = src.blacks;
                dst.clarity = src.clarity;
                dst.texture = src.texture;
                dst.dehaze = src.dehaze;
            }
            "white_balance" => {
                dst.temperature = src.temperature;
                dst.tint = src.tint;
            }
            "curves" => {
                dst.curve.clone_from(&src.curve);
                dst.parametric = src.parametric;
                dst.channel_curves.clone_from(&src.channel_curves);
            }
            "color" => {
                dst.vibrance = src.vibrance;
                dst.saturation = src.saturation;
                dst.hsl.clone_from(&src.hsl);
                dst.colour_grade = src.colour_grade;
                proposal.bw.clone_from(&source.bw);
                for band in aura_recipe::HSL_BANDS {
                    dst.hsl.entry(band.to_string()).or_default();
                }
            }
            "detail" => {
                dst.sharpen = src.sharpen;
                dst.noise = src.noise.clone();
            }
            "effects" => {
                dst.effects = src.effects;
                proposal.lens.vignette = source.lens.vignette;
            }
            "calibration" => {
                dst.calibration = src.calibration;
            }
            "lens" => {
                // Optical profiles belong to the target camera/lens, not the source frame.
                proposal.lens.distortion = source.lens.distortion;
                proposal.lens.ca = source.lens.ca;
            }
            "geometry" => proposal.geometry = source.geometry.clone(),
            _ => {
                return Err(aura_core::errors::render::recipe_invalid(
                    "groups",
                    "Unknown settings group",
                ))
            }
        }
    }
    Ok(proposal)
}

/// The defaults of a block that may be absent from a recipe's document.
fn default_block(parent: &str, key: &str) -> Option<serde_json::Value> {
    let value = match (parent, key) {
        ("global", "parametric") => serde_json::to_value(aura_recipe::ParametricCurve::default()),
        ("global", "channel_curves") => serde_json::to_value(aura_recipe::ChannelCurves::default()),
        ("global", "colour_grade") => serde_json::to_value(aura_recipe::ColourGrade::default()),
        ("global", "calibration") => serde_json::to_value(aura_recipe::Calibration::default()),
        ("global", "effects") => serde_json::to_value(aura_recipe::Effects::default()),
        ("global.hsl", band) if aura_recipe::HSL_BANDS.contains(&band) => {
            serde_json::to_value(aura_recipe::HslShift::default())
        }
        ("", "bw") => serde_json::to_value(aura_recipe::Bw::default()),
        _ => return None,
    };
    value.ok()
}

/// The label the history panel shows for a control the caller did not name.
fn control_label(path: &str) -> String {
    let leaf = path.rsplit('.').next().unwrap_or(path);
    let mut chars = leaf.replace('_', " ");
    if let Some(first) = chars.get(0..1) {
        chars = format!("{}{}", first.to_uppercase(), &chars[1..]);
    }
    chars
}

/// The recipe stored for a photograph, or the neutral one when it has never been edited.
///
/// Public since PHASE-30, because the export source needs the same recipe the develop panel shows.
/// Two loaders would be two answers to what the edit is, which is the failure phase 14's rule about
/// `RenderService` exists to prevent one level down.
///
/// # Errors
///
/// `AURA-DB-3006` when the recipe cannot be read, `AURA-RENDER-8002` when it does not parse.
pub fn load_or_neutral(state: &AppState, photo: PhotoId) -> Result<Recipe, AuraError> {
    match state.recipe_store().load(&photo)? {
        Some(recipe) => Ok(recipe),
        None => Ok(recipe_fixtures::neutral(
            &state
                .photo_content_hash(photo)
                .unwrap_or_else(|| "0".repeat(64)),
            &state.photo_camera(photo).unwrap_or_default(),
        )),
    }
}

fn load_history(state: &AppState, photo: PhotoId) -> Result<History, AuraError> {
    let original = recipe_fixtures::neutral(
        &state
            .photo_content_hash(photo)
            .unwrap_or_else(|| "0".repeat(64)),
        &state.photo_camera(photo).unwrap_or_default(),
    );
    state
        .recipe_store()
        .history(&photo, original)
        .map(replay_history)
}

/// The number of steps currently applied: 0 at the original, `entries().len()` at the head.
fn history_cursor(history: &History) -> usize {
    let current = history.current();
    history
        .entries()
        .iter()
        .position(|entry| std::ptr::eq(std::ptr::from_ref(&entry.recipe), current))
        .map_or(0, |index| index + 1)
}

/// Reconstruct the active edit branch and its cursor from the saved journal.
/// Navigation rows remain in `SQLite`, so redo also survives reopening the app.
fn replay_history(stored: History) -> History {
    let mut entries = Vec::new();
    let mut cursor = 0;
    for entry in stored.entries() {
        let goto = entry
            .changed
            .iter()
            .find_map(|path| path.strip_prefix("$history.goto:"))
            .and_then(|seq| seq.parse::<u64>().ok());
        if let Some(seq) = goto {
            if seq == 0 {
                cursor = 0;
            } else if let Some(index) = entries
                .iter()
                .position(|e: &aura_recipe::history::HistoryEntry| e.seq == seq)
            {
                cursor = index + 1;
            }
        } else if entry.changed.iter().any(|path| path == "$history.undo") && cursor > 0 {
            cursor -= 1;
        } else if entry.changed.iter().any(|path| path == "$history.redo") && cursor < entries.len()
        {
            cursor += 1;
        } else {
            // A new edit discards the redo branch. If the bounded journal no
            // longer has a navigation target, its saved recipe is a checkpoint.
            entries.truncate(cursor);
            entries.push(entry.clone());
            cursor = entries.len();
        }
    }
    let head = entries.len();
    let mut history = History::rehydrate(
        stored.original().clone(),
        entries,
        stored.snapshots().to_vec(),
    );
    for _ in cursor..head {
        history.undo();
    }
    history
}

/// Base64, written out rather than pulled in.
///
/// One dependency avoided for twenty lines. The alphabet is standard and there is no line
/// wrapping, because the only consumer is a data URL.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk.first().copied().unwrap_or(0);
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        let triple = (u32::from(a) << 16) | (u32::from(b) << 8) | u32::from(c);
        let indices = [
            (triple >> 18) & 0x3f,
            (triple >> 12) & 0x3f,
            (triple >> 6) & 0x3f,
            triple & 0x3f,
        ];
        for (position, index) in indices.iter().enumerate() {
            if position > chunk.len() {
                out.push('=');
            } else {
                let symbol = ALPHABET
                    .get((*index as usize) & 0x3f)
                    .copied()
                    .unwrap_or(b'A');
                out.push(char::from(symbol));
            }
        }
    }
    out
}

fn parse_project(id: &str) -> Result<ProjectId, IpcError> {
    ProjectId::from_db(id).map_err(|_| {
        IpcError::from(aura_core::errors::db::statement_failed(
            format!("not a project id: {id}"),
            &std::io::Error::from(std::io::ErrorKind::InvalidInput),
        ))
    })
}

fn parse_photo(id: &str) -> Result<PhotoId, IpcError> {
    PhotoId::from_db(id).map_err(|_| {
        IpcError::from(aura_core::errors::db::statement_failed(
            format!("not a photo id: {id}"),
            &std::io::Error::from(std::io::ErrorKind::InvalidInput),
        ))
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn selective_sync_preserves_unselected_controls_and_camera_specific_profiles() {
        use super::*;
        let mut base = recipe_fixtures::reference();
        base.global.exposure = 1.25;
        base.lens.profile = Some("target lens".into());
        let mut source = base.clone();
        source.global.temperature = 7200;
        source.global.exposure = -1.0;
        source.geometry.rotate = 2.0;
        source.lens.profile = Some("different lens".into());
        let input = SyncSettingsInput {
            project_id: String::new(),
            source_photo_id: String::new(),
            target_photo_ids: vec![],
            include_geometry: true,
            groups: Some(vec!["white_balance".into(), "lens".into()]),
        };
        let copied = sync_proposal(&base, &source, &input).unwrap();
        assert_eq!(copied.global.temperature, 7200);
        assert_eq!(copied.global.exposure, base.global.exposure);
        assert_eq!(copied.geometry, base.geometry);
        assert_eq!(copied.lens.profile, base.lens.profile);
        assert!(sync_proposal(
            &base,
            &source,
            &SyncSettingsInput {
                groups: Some(vec![]),
                ..input.clone()
            }
        )
        .is_err());
        assert!(sync_proposal(
            &base,
            &source,
            &SyncSettingsInput {
                groups: Some(vec!["everything".into()]),
                ..input
            }
        )
        .is_err());
    }
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(
            base64(b"any carnal pleasure"),
            "YW55IGNhcm5hbCBwbGVhc3VyZQ=="
        );
    }

    #[test]
    fn a_control_label_is_readable() {
        assert_eq!(control_label("global.exposure"), "Exposure");
        assert_eq!(control_label("global.sharpen.amount"), "Amount");
        assert_eq!(control_label("global.noise.luminance"), "Luminance");
    }

    #[test]
    fn a_parameter_a_person_may_not_set_is_refused() {
        let base = recipe_fixtures::reference();
        for path in [
            "provenance.confidence",
            "provenance.user_edited_fields",
            "image.content_hash",
            "schema",
        ] {
            match apply_path(&base, path, &serde_json::json!(1)) {
                Err(err) => assert_eq!(err.code.to_string(), "AURA-RENDER-8002"),
                Ok(_) => panic!("{path} must not be settable by a person"),
            }
        }
    }

    #[test]
    fn a_value_of_the_wrong_shape_is_refused_rather_than_coerced() {
        let base = recipe_fixtures::reference();
        match apply_path(&base, "global.exposure", &serde_json::json!("bright")) {
            Err(err) => assert_eq!(err.code.to_string(), "AURA-RENDER-8002"),
            Ok(_) => panic!("a string is not an exposure"),
        }
    }

    #[test]
    fn an_unknown_path_is_refused() {
        let base = recipe_fixtures::reference();
        assert!(apply_path(&base, "global.sparkle", &serde_json::json!(1)).is_err());
    }

    #[test]
    fn setting_a_parameter_produces_a_document_that_differs_in_exactly_one_place() {
        let base = recipe_fixtures::reference();
        let proposal =
            apply_path(&base, "global.exposure", &serde_json::json!(1.5)).expect("apply");
        let (_, report) = schema::merge(&base, &proposal, EditSource::User).expect("merge");
        assert_eq!(report.changed, vec!["global.exposure".to_string()]);
        assert!(report.refused.is_empty());
    }

    #[test]
    fn the_recipe_dto_marks_protected_controls_and_never_carries_provenance_as_a_control() {
        let mut recipe = recipe_fixtures::reference();
        recipe.provenance.user_edited_fields = vec!["global.exposure".to_string()];
        let dto = recipe_dto("pht_x", &recipe);

        let exposure = dto
            .params
            .iter()
            .find(|p| p.path == "global.exposure")
            .expect("exposure is a control");
        assert!(exposure.protected);
        assert_eq!(exposure.stage.as_deref(), Some("exposure"));

        assert!(
            !dto.params.iter().any(|p| p.path.starts_with("provenance")),
            "provenance is metadata, not a slider"
        );
        assert!(!dto.params.iter().any(|p| p.path.starts_with("image")));
        assert_eq!(dto.user_edited_fields, vec!["global.exposure".to_string()]);
    }
}
