//! The look command surface. PHASE-31.
//!
//! Nine commands: four read - the project's status, the look list, one look's buckets and the
//! measured match - one parses an address as somebody types it, one measures, and three act on a
//! selection.
//!
//! # The three things this module cannot do, and could not be made to
//!
//! **It cannot fetch a page.** `measure_look` takes a folder, and the one route that would go
//! and get the photographs refuses in `aura_look::source::resolve` before a disk is touched.
//! `LookStatusDto::network_transport_available` is on the wire so the panel renders the folder
//! picker rather than a button that always fails, and `aura-look` depends on no cloud crate,
//! which is the other half of the same statement. ADR-0063 section 4.
//!
//! **It cannot write a recipe.** A look reaches the pixels through `TonePass` and `ColourPass`,
//! which reach them through `aura_recipe::schema::merge`. Phase 14's rule for the fifth phase
//! running, and the reason there is no `apply_look` command here. What `measure_look` renders,
//! it renders in order to *measure*, and it stores a number rather than a photograph.
//!
//! **It cannot make a look stronger than the reference.** `SetLookStrengthInput::fraction` is
//! clamped to `0..=1` here, again in `LookOverride::strength`, and again by migration 31's
//! CHECK. Three locks because a slider that went to 150 % is the single most likely thing a
//! later change would add, and because it is the difference between matching a look and
//! caricaturing one.
//!
//! # Why `measure_look` renders the photographer's own frames
//!
//! Because a look is a *residual*. The difference between a page and a baseline is only a look
//! if the baseline is what AURA would actually have done to these photographs, and the only way
//! to know that is to render them. A version of this command that skipped the baseline would
//! produce an absolute edit wearing a residual's shape - which is exactly phase 17's condition
//! C4, written down before this phase existed.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_core::contract::look::{
    LookCode, LookMatchReport, LookOverride, LookProfile, LookReason, LookService, MediaSource,
    ReferenceOrigin, APPLY_ABOVE,
};
use aura_core::contract::style::LightingBucket;
use aura_core::contract::tone::ToneService;
use aura_core::progress::CancelToken;
use aura_core::{PhotoId, ProfileId, ProjectId};
use aura_look::verify::{OwnFrame, Renderer};
use aura_look::MeasurePass;
use aura_raw::contract::pixels::PixelData;
use aura_render::contract::render::OutputSpec;
use aura_render::cpu::Frame;

use crate::commands::IpcResult;
use crate::contract::ipc::{
    IpcError, LookBucketDto, LookBucketResidualDto, LookBucketsInput, LookMatchDto, LookProfileDto,
    LookProfileInput, LookReasonDto, LookStatusDto, MeasureLookDto, MeasureLookInput,
    ReferenceOriginDto, SelectLookInput, SetLookStrengthInput,
};
use crate::state::AppState;

/// How many of a project's own frames the baseline is measured over by default.
///
/// Sixty. The baseline is a median of distribution statistics, and a median over sixty frames
/// and a median over a thousand differ by far less than the nine hundred and forty extra
/// renders cost. It is a default rather than a ceiling: a caller may ask for fewer, and
/// `MeasureLookInput::baseline_frames` is how.
pub const BASELINE_FRAMES: u32 = 60;

/// The level the photographer's own frames are rendered at for measuring.
///
/// A 1024 px screen render. Every reading is a distribution statistic and none of them moves
/// meaningfully with resolution - `a_reading_does_not_depend_much_on_the_scale_it_was_taken_at`
/// is the test - so this is chosen for what it costs rather than for what it measures.
pub const MEASURE_EDGE: u32 = 1024;

/// What the match-a-look card on the first screen shows.
///
/// # Errors
///
/// `AURA-DB-3006` when the looks cannot be read.
pub fn look_status(state: &AppState, project_id: &str) -> IpcResult<LookStatusDto> {
    let project = parse_project(project_id)?;
    let outline = state.look().outline(project)?;
    let strength = state
        .look_store()
        .selected(project)?
        .map_or(1.0, |(_, fraction)| fraction);

    Ok(LookStatusDto {
        profiles: outline.profiles,
        selected: outline.selected.map(|id| id.to_db()),
        selected_name: outline.selected_name.clone().unwrap_or_default(),
        selected_origin: outline
            .selected_origin
            .as_ref()
            .map(ReferenceOrigin::title)
            .unwrap_or_default(),
        strength,
        appliable: outline.appliable,
        photographs: outline.photographs,
        baseline_coverage: outline.baseline_coverage,
        network_transport_available: outline.network_transport_available,
    })
}

/// Every stored look, newest first.
///
/// # Errors
///
/// `AURA-DB-3006` when the looks cannot be read.
pub fn list_looks(state: &AppState) -> IpcResult<Vec<LookProfileDto>> {
    let engine = aura_recipe::contract::recipe::ENGINE;
    Ok(state
        .look()
        .profiles()?
        .iter()
        .map(|look| profile_dto(look, engine))
        .collect())
}

/// What a photographer typed into the address box, read back as they type.
///
/// **Never fails.** An address that will not parse comes back with `understood: false` and the
/// refusal's own sentence, because a box that throws an error on the third keystroke of
/// `instagram.com/...` is a box nobody can type into.
#[must_use]
pub fn parse_reference(address: &str) -> ReferenceOriginDto {
    match ReferenceOrigin::parse(address) {
        Ok(origin) => ReferenceOriginDto {
            kind: origin_kind(&origin).to_string(),
            title: origin.title(),
            key: origin.as_key(),
            understood: true,
            refusal: None,
        },
        Err(error) => ReferenceOriginDto {
            kind: "local".to_string(),
            title: address.trim().to_string(),
            key: String::new(),
            understood: false,
            refusal: Some(error.user_message.clone()),
        },
    }
}

/// One look's lighting buckets, as the matrix renders them.
///
/// # Errors
///
/// `AURA-ML-5147` when the look is not stored.
pub fn look_buckets(state: &AppState, input: &LookBucketsInput) -> IpcResult<Vec<LookBucketDto>> {
    let id = parse_profile(&input.profile_id)?;
    let Some(look) = state.look().profile(id)? else {
        return Err(IpcError::from(aura_core::errors::ml::look_refused(
            "that look is not stored",
        )));
    };

    // What this look actually did on this project, when it has been measured here. A look
    // measured on a different project has no row and every `afterDe00` stays `None` - which is
    // the honest answer, because what a look did to one wedding says nothing about another.
    let measured = match input.project_id.as_deref() {
        Some(project) => {
            let project = parse_project(project)?;
            state
                .look()
                .match_report(project)?
                .filter(|report| report.profile == id)
        }
        None => None,
    };

    Ok(LightingBucket::ALL
        .into_iter()
        .filter_map(|lighting| {
            let bucket = look.buckets.get(&lighting)?;
            Some(LookBucketDto {
                lighting: lighting.as_str().to_string(),
                title: lighting.title().to_string(),
                samples: bucket.reference.samples,
                confidence: bucket.confidence,
                weak: bucket.reference.is_weak(),
                applied: bucket.confidence >= APPLY_ABOVE,
                exposure: bucket.delta.exposure,
                temperature_k: bucket.delta.temperature_k,
                tint: bucket.delta.tint,
                contrast: bucket.delta.contrast,
                vibrance: bucket.delta.vibrance,
                saturation: bucket.delta.saturation,
                // From the match report rather than from the bucket, because a bucket knows what
                // it asked for and only a measurement knows what happened. `None` where nothing
                // was measured in this light, never zero.
                after_de00: measured.as_ref().and_then(|report| {
                    report
                        .buckets
                        .iter()
                        .find(|row| row.lighting == lighting)
                        .map(|row| row.after_de00)
                }),
            })
        })
        .collect())
}

/// The last measured match on one project.
///
/// # Errors
///
/// `AURA-DB-3006` when it cannot be read.
pub fn look_match_report(state: &AppState, project_id: &str) -> IpcResult<Option<LookMatchDto>> {
    let project = parse_project(project_id)?;
    Ok(state.look().match_report(project)?.as_ref().map(match_dto))
}

/// Measure a reference and store the look.
///
/// # Errors
///
/// `AURA-ML-5146` when the reference will not resolve or holds too few photographs,
/// `AURA-ML-5147` when the look cannot be stored.
pub fn measure_look(state: &AppState, input: &MeasureLookInput) -> IpcResult<MeasureLookDto> {
    let project = parse_project(&input.project_id)?;
    let source = MediaSource::from_str_or_folder(&input.source);
    let folder = input.folder.as_ref().map(PathBuf::from);

    // The refusal, before any work: a route this build cannot fetch through is answered here
    // rather than after a folder walk that was never going to happen.
    if !source.can_fetch() {
        return Err(IpcError::from(aura_core::contract::look::refuse_fetch(
            source,
        )));
    }

    let name = if input.name.trim().is_empty() {
        ReferenceOrigin::parse(&input.address)
            .map_or_else(|_| "A look".to_string(), |origin| origin.title())
    } else {
        input.name.trim().to_string()
    };

    let cap = input.baseline_frames.unwrap_or(BASELINE_FRAMES);
    let frames = own_frames(state, project, cap)?;
    let renderer = Renderer::new(Arc::clone(state.clock()), OutputSpec::default());

    // Registered before the pass starts and removed however it ends, including on the error
    // path - a token left in the map is a job id that can never be reused and a `cancel_job`
    // that silently succeeds against nothing.
    let cancel = CancelToken::new();
    if let Some(id) = input.cancel_id.as_deref() {
        state.register_job(id, cancel.clone());
    }
    let pass = MeasurePass::new(state.look_store(), Arc::clone(state.clock()), name);
    let request = aura_look::api::MeasureRequest {
        address: &input.address,
        media: source,
        folder: folder.as_deref(),
        project,
        frames: &frames,
        renderer: &renderer,
    };
    let report = pass.run(&request, || cancel.is_cancelled());
    if let Some(id) = input.cancel_id.as_deref() {
        state.finish_job(id);
    }
    let report = report?;

    let stored = match report.profile {
        Some(id) => state.look().profile(id)?,
        None => None,
    };
    let reasons: Vec<LookReasonDto> = stored
        .as_ref()
        .map(|look| look.diagnostics.reasons.iter().map(reason_dto).collect())
        .unwrap_or_default();

    Ok(MeasureLookDto {
        profile: report.profile.map(|id| id.to_db()),
        cancelled: report.cancelled,
        found: u32::try_from(report.found).unwrap_or(u32::MAX),
        measured: u32::try_from(report.measured).unwrap_or(u32::MAX),
        refused: u32::try_from(report.refused).unwrap_or(u32::MAX),
        baseline_frames: u32::try_from(report.baseline_frames).unwrap_or(u32::MAX),
        buckets: u32::try_from(report.buckets).unwrap_or(u32::MAX),
        matched: report.matched.as_ref().map(match_dto),
        summary: stored
            .as_ref()
            .map(|look| look.diagnostics.summary.clone())
            .unwrap_or_default(),
        reasons,
    })
}

/// Point a project at a look, or back at the baseline.
///
/// # Errors
///
/// `AURA-ML-5147` when the look is not stored or has never been measured.
pub fn select_look(state: &AppState, input: &SelectLookInput) -> IpcResult<()> {
    let project = parse_project(&input.project_id)?;
    let profile = match input.profile_id.as_deref() {
        Some(id) => Some(parse_profile(id)?),
        None => None,
    };
    state.look().select(project, profile)?;
    Ok(())
}

/// Apply a look at less than its measured strength.
///
/// # Errors
///
/// `AURA-DB-3006` when the write fails.
pub fn set_look_strength(state: &AppState, input: &SetLookStrengthInput) -> IpcResult<()> {
    let project = parse_project(&input.project_id)?;
    state
        .look()
        .override_with(project, &LookOverride::strength(input.fraction))?;
    Ok(())
}

/// Rename a look.
///
/// # Errors
///
/// `AURA-ML-5147` when the look is not stored.
pub fn rename_look(state: &AppState, input: &LookProfileInput) -> IpcResult<()> {
    let id = parse_profile(&input.profile_id)?;
    let name = input.name.clone().unwrap_or_default();
    if name.trim().is_empty() {
        return Err(IpcError::from(aura_core::errors::ml::look_refused(
            "a look needs a name",
        )));
    }
    state.look_store().rename(id, name.trim())?;
    Ok(())
}

/// Delete a look. A project that had it selected falls back to the baseline.
///
/// # Errors
///
/// `AURA-DB-3006` when the write fails.
pub fn forget_look(state: &AppState, input: &LookProfileInput) -> IpcResult<()> {
    let id = parse_profile(&input.profile_id)?;
    state.look().forget(id)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The photographer's own frames
// ---------------------------------------------------------------------------

/// The frames the baseline is measured over, and the look is measured against.
///
/// Only photographs phase 15 has an estimate for, because a frame with no tone decision has no
/// baseline for a look to be a residual from. Phase 18's rule about denominators is why
/// `LookOutline::appliable` counts the same set and puts both numbers on the wire.
///
/// **A frame whose proxy is not built yet is skipped rather than rendered grey.** The
/// alternative is `CatalogFrames`, which returns a neutral frame so that a develop panel opens
/// on the night of a wedding rather than refusing until the whole gallery is decoded. That is
/// right for a panel and wrong here: it would make the baseline a measurement of mid grey, and
/// every look measured against it would be the difference between a page and a rectangle.
fn own_frames(state: &AppState, project: ProjectId, cap: u32) -> IpcResult<Vec<OwnFrame>> {
    use aura_preview::contract::service::{PreviewService, Priority};
    use aura_raw::contract::pixels::PixelLevel;

    let previews = state.previews(&project.to_db())?;
    let recipes = state.recipe_store();
    let tone = state.tone();

    let key = project.to_db();
    let ids: Vec<String> = state.catalog().read(move |conn| {
        let mut statement = conn
            .prepare(
                "SELECT photo_id FROM image_tone_estimate WHERE project_id = ?1
                 ORDER BY photo_id",
            )
            .map_err(|err| aura_core::errors::db::statement_failed("prepare look frames", &err))?;
        let rows = statement
            .query_map([&key], |row| row.get::<_, String>(0))
            .map_err(|err| aura_core::errors::db::statement_failed("query look frames", &err))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|err| {
                aura_core::errors::db::statement_failed("read look frames", &err)
            })?);
        }
        Ok(out)
    })?;

    // A stride rather than the first `cap`, so a wedding that starts in one room and ends in
    // another is sampled across both. Deterministic, so two measurements of one project read the
    // same frames - which is what makes a re-measure comparable to the one before it.
    let stride = if cap == 0 {
        1
    } else {
        ids.len().div_ceil(cap.max(1) as usize).max(1)
    };

    let mut out = Vec::new();
    for id in ids.iter().step_by(stride).take(cap as usize) {
        let Ok(image) = PhotoId::from_db(id) else {
            continue;
        };
        let Ok(buffer) = previews.get(image, PixelLevel::Proxy2048, Priority::AiBatch) else {
            continue;
        };
        let Some((rgb, width, height)) = linear_rgb(&buffer) else {
            continue;
        };
        let recipe = recipes
            .load(&image)?
            .unwrap_or_else(|| aura_recipe::fixtures::neutral(id, ""));
        let estimate = tone.of_image(image).ok().flatten();
        let lighting = estimate
            .as_ref()
            .map_or(LightingBucket::Unknown, dominant_lighting);

        out.push(OwnFrame {
            key: id.clone(),
            frame: Frame::working(rgb, width, height, &recipe.image.camera),
            width,
            height,
            // What phases 15 and 16 decided. This is the thing a look is a residual *from*, and
            // handing a neutral recipe here instead is phase 17's condition C4 repeated.
            baseline: recipe,
            lighting,
            user_edited: false,
        });
    }

    Ok(out)
}

/// Which of phase 17's ten lighting buckets one of phase 15's estimates lands in.
///
/// **Phase 17's `lighting_of` and nothing else.** The reference photographs are sorted by
/// `aura_look::light::bucket` from their pixels and the photographer's own frames are sorted
/// here from phase 15's answer, and the two sides of the comparison have to agree about where
/// the boundaries are - which they do because `aura_look::light` uses phase 17's own
/// `GOLDEN_BELOW_K` rather than a second one.
///
/// The light on the *subject* is what a photographer graded for, so a frame with two
/// illuminants is sorted by `dominant_on_subject` rather than by whichever the solver listed
/// first. Phase 15's rule, read the way that phase asks for: ask the room, not the winner.
fn dominant_lighting(estimate: &aura_core::contract::tone::ToneEstimate) -> LightingBucket {
    let dominant = estimate
        .dominant_on_subject
        .and_then(|index| estimate.illuminants.get(index))
        .or_else(|| estimate.illuminants.first());
    let Some(illuminant) = dominant else {
        return LightingBucket::Unknown;
    };
    let flash = illuminant.kind == aura_core::contract::tone::IlluminantKind::Flash;
    aura_style::bucket::lighting_of(illuminant.kind, estimate.temperature_k, flash)
}

/// A preview buffer as linear RGB, scaled down to [`MEASURE_EDGE`].
///
/// Returns `None` for a tiled buffer, which is a full-resolution image the measuring path has no
/// use for - a distribution statistic taken over a tile is a statistic about that tile.
fn linear_rgb(buffer: &aura_raw::contract::pixels::PixelBuffer) -> Option<(Vec<f32>, u32, u32)> {
    let (width, height) = (buffer.width.max(1), buffer.height.max(1));
    let stride = usize::try_from(width.max(height).div_ceil(MEASURE_EDGE.max(1)))
        .unwrap_or(1)
        .max(1);
    let out_w = u32::try_from(width as usize / stride).unwrap_or(1).max(1);
    let out_h = u32::try_from(height as usize / stride).unwrap_or(1).max(1);

    let mut rgb = Vec::with_capacity((out_w * out_h * 3) as usize);
    for y in 0..out_h as usize {
        for x in 0..out_w as usize {
            let source = ((y * stride) * width as usize + x * stride) * 3;
            match &buffer.data {
                PixelData::Srgb8(bytes) => {
                    let triple = bytes.get(source..source + 3)?;
                    for sample in triple {
                        let value = f32::from(*sample) / 255.0;
                        rgb.push(if value <= 0.040_45 {
                            value / 12.92
                        } else {
                            ((value + 0.055) / 1.055).powf(2.4)
                        });
                    }
                }
                PixelData::Linear16(samples) => {
                    let triple = samples.get(source..source + 3)?;
                    for sample in triple {
                        rgb.push(f32::from(*sample) / 65_535.0);
                    }
                }
                PixelData::Tiled(_) => return None,
            }
        }
    }

    Some((rgb, out_w, out_h))
}

// ---------------------------------------------------------------------------
// Mapping
// ---------------------------------------------------------------------------

fn origin_kind(origin: &ReferenceOrigin) -> &'static str {
    match origin {
        ReferenceOrigin::Instagram { .. } => "instagram",
        ReferenceOrigin::Web { .. } => "web",
        ReferenceOrigin::Local { .. } => "local",
    }
}

fn reason_dto(reason: &LookReason) -> LookReasonDto {
    LookReasonDto {
        code: reason.code.as_str().to_string(),
        // Rendered here, never stored. Migration 31's note 6.
        sentence: reason.code.sentence().to_string(),
        value: reason.value,
        threshold: reason.threshold,
        actionable: reason.code.is_actionable(),
    }
}

fn profile_dto(look: &LookProfile, engine: &str) -> LookProfileDto {
    LookProfileDto {
        id: look.id.to_db(),
        name: look.name.clone(),
        origin: look.origin.title(),
        origin_kind: origin_kind(&look.origin).to_string(),
        source: look.source.as_str().to_string(),
        references: look.references,
        strength: look.diagnostics.strength,
        buckets: look.diagnostics.buckets_populated,
        measured_at: look.measured_at,
        summary: look.diagnostics.summary.clone(),
        reasons: look.diagnostics.reasons.iter().map(reason_dto).collect(),
        // A look measured against a renderer that has since moved is not applied, and the panel
        // says so rather than showing a figure about a build that no longer exists.
        stale: look.engine_ver != engine,
    }
}

fn match_dto(report: &LookMatchReport) -> LookMatchDto {
    LookMatchDto {
        profile: report.profile.to_db(),
        before_de00: report.before_de00,
        after_de00: report.after_de00,
        realised_share: report.realised_share(),
        reached: report.reached(),
        frames: report.frames,
        measured_frames: report.measured_frames,
        measured_coverage: report.measured_coverage(),
        user_edited: report.user_edited,
        buckets: report
            .buckets
            .iter()
            .map(|row| LookBucketResidualDto {
                lighting: row.lighting.as_str().to_string(),
                title: row.lighting.title().to_string(),
                before_de00: row.before_de00,
                after_de00: row.after_de00,
                realised_share: row.realised_share(),
                frames: row.frames,
            })
            .collect(),
        reasons: report.reasons.iter().map(reason_dto).collect(),
    }
}

fn parse_project(id: &str) -> IpcResult<ProjectId> {
    ProjectId::from_db(id).map_err(|_| {
        IpcError::from(aura_core::errors::ml::look_refused(format!(
            "{id} is not a project"
        )))
    })
}

fn parse_profile(id: &str) -> IpcResult<ProfileId> {
    ProfileId::from_db(id).map_err(|_| {
        IpcError::from(aura_core::errors::ml::look_refused(format!(
            "{id} is not a look"
        )))
    })
}

/// The one code every measured look carries, exposed so the gate can assert it is on the wire.
#[must_use]
pub const fn scene_axis_code() -> LookCode {
    LookCode::SceneAxisNotLearned
}

/// The path a folder argument becomes, for a caller that wants to check it first.
#[must_use]
pub fn folder_of(input: &MeasureLookInput) -> Option<&Path> {
    input.folder.as_deref().map(Path::new)
}
