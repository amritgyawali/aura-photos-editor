//! Local, semantic-assisted reference colour matching with reversible independent controls.
//! The reference supplies colour descriptors, never pixels for the target output.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use image::ImageEncoder;
use std::path::Path;

use aura_core::contract::colour::HslBand;
use aura_core::contract::style::StyleDelta;
use aura_core::progress::CancelToken;
use aura_core::{AuraError, AuraResult, PhotoId, ProjectId};
use aura_raw::codec::Rgb8;
use aura_recipe::{schema, EditSource, Recipe};
use aura_render::{Frame, FrameSource, OutputSpec, RenderLevel, RenderPurpose, RenderedData};
use rusqlite::OptionalExtension;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{commands::IpcResult, AppState};

mod appearance;
mod baseline;
use baseline::AnalysisBaseline;
mod calibration;
mod content;
pub use calibration::CalibrationReport;
mod fine;
pub use fine::FineControls;
mod quality;
pub use quality::{MatchIntent, MatchQuality};
mod contours;
pub(crate) mod gallery_fit;
mod grade;
mod library;
mod lightroom;
pub use lightroom::{generate as generate_lightroom_preset, PresetInput, PresetReport};
mod matcher;
mod native_checks;
pub use matcher::TransferDiagnostics;
pub(crate) use native_checks::NativeChecks;
mod refine;
mod transfer;
pub use library::{
    capture as capture_reference, manage as manage_reference, workflow as library_workflow,
    CaptureInput, ManageInput, WorkflowInput as LibraryWorkflowInput,
    WorkflowSnapshot as LibraryWorkflowSnapshot,
};
pub use transfer::{Distance as TransferDistance, SkinReview};
mod reference_image;
mod regional;
use appearance::Region;
pub use regional::{
    apply as apply_regional, prepare as prepare_regional, status as regional_status,
    ApplyInput as RegionalApplyInput, Controls as RegionalControls, Kind as RegionalKind,
    MatchReport as RegionalMatchReport, PrepareInput as RegionalPrepareInput,
};

const ASSET: &str = "colour_reference_v1:";
const SESSION: &str = "colour_match_v1:";
const ENGINE: &str = "regional-colour-match-v2-person-guards";

fn invalid(message: impl Into<String>) -> AuraError {
    let message = message.into();
    let mut error = aura_core::errors::ml::look_reference_refused(message.clone());
    error.user_message = message;
    error
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Controls {
    pub amount: f32,
    pub tone: f32,
    pub color: f32,
    pub white_balance: f32,
    #[serde(default)]
    pub detail: FineControls,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            amount: 1.0,
            tone: 1.0,
            color: 1.0,
            white_balance: 1.0,
            detail: FineControls::default(),
        }
    }
}
impl Controls {
    fn check(self) -> AuraResult<()> {
        if [self.amount, self.tone, self.color, self.white_balance]
            .into_iter()
            .chain(self.detail.values())
            .all(|v| v.is_finite() && (0.0..=2.0).contains(&v))
        {
            Ok(())
        } else {
            Err(invalid(
                "Colour matching strengths must be between 0 and 200%.",
            ))
        }
    }
    fn delta(self, fitted: &StyleDelta) -> StyleDelta {
        let mut out = fitted.scaled(self.amount);
        for value in [
            &mut out.exposure,
            &mut out.contrast,
            &mut out.highlights,
            &mut out.shadows,
            &mut out.whites,
            &mut out.blacks,
        ] {
            *value *= self.tone;
        }
        out.curve_shift = out.curve_shift.scaled(self.tone);
        out.temperature_k *= self.white_balance;
        out.tint *= self.white_balance;
        out.vibrance *= self.color;
        out.saturation *= self.color;
        for band in HslBand::ALL {
            let mut shift = out.hsl.get(band);
            shift.h *= self.color * self.detail.hue;
            shift.s *= self.color * self.detail.saturation;
            shift.l *= self.tone;
            out.hsl.set(band, shift);
        }
        self.detail.apply(&mut out);
        out.clamped()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceSummary {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub converted_icc: bool,
    pub regions: Vec<String>,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_controls: Option<Controls>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Reference {
    engine: String,
    summary: ReferenceSummary,
    regions: Vec<Region>,
    #[serde(default)]
    small_rgb: Vec<u8>,
    #[serde(default)]
    achromatic: bool,
}
#[derive(Debug, Serialize, Deserialize)]
struct Session {
    engine: String,
    id: String,
    photo_id: String,
    reference: ReferenceSummary,
    baseline: Recipe,
    baseline_hash: String,
    last_hash: String,
    fitted: StyleDelta,
    reference_regions: Vec<Region>,
    target_regions: Vec<Region>,
    before_distance: f32,
    fitted_distance: f32,
    controls: Controls,
    checked_hash: Option<String>,
    measured_before: Option<f32>,
    measured_after: Option<f32>,
    #[serde(default)]
    fitting: Option<FitDiagnostics>,
    #[serde(default)]
    reference_monochrome: bool,
    #[serde(default)]
    grade_offsets: [f32; 6],
    /// Present for sessions fitted by distribution transfer (ADR-0160); older sessions keep
    /// the slider fit above.
    #[serde(default)]
    transfer: Option<matcher::TransferSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitDiagnostics {
    pub engine: String,
    #[serde(default)]
    pub intent: MatchIntent,
    #[serde(default)]
    pub quality: MatchQuality,
    #[serde(default)]
    pub target_samples: usize,
    #[serde(default)]
    pub max_iterations: usize,
    pub refinement_candidates: usize,
    pub unique_renders: usize,
    pub cached_proposals: usize,
    #[serde(default)]
    pub grade_candidates: usize,
    #[serde(default)]
    pub native_candidate_checked: bool,
    #[serde(default)]
    pub native_backtrack_factor: f32,
    #[serde(default)]
    pub structure_similarity: Option<f32>,
    #[serde(default)]
    pub regularization_weight: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer: Option<TransferDiagnostics>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportInput {
    pub path: String,
    pub cancel_id: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareInput {
    pub photo_id: String,
    pub reference_id: String,
    pub cancel_id: String,
    #[serde(default)]
    pub intent: MatchIntent,
    #[serde(default)]
    pub quality: MatchQuality,
    /// Replace an earlier reference treatment while preserving manual recipe work.
    /// Omitted by legacy callers, which retain their earlier-look crossfade.
    #[serde(default)]
    pub replace_previous: bool,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyInput {
    pub photo_id: String,
    pub session_id: String,
    pub controls: Controls,
    pub cancel_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchReport {
    pub session_id: String,
    pub photo_id: String,
    pub reference: ReferenceSummary,
    pub controls: Controls,
    pub matched_regions: Vec<String>,
    pub before_distance: f32,
    pub after_distance: f32,
    pub changed: usize,
    pub protected_fields: usize,
    pub full_resolution_checked: bool,
    pub current: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fitting: Option<FitDiagnostics>,
    pub reference_monochrome: bool,
}

fn read<T: DeserializeOwned>(state: &AppState, key: &str) -> AuraResult<Option<T>> {
    let text: Option<String> = state.catalog().read(|conn| {
        conn.query_row(
            "SELECT value_json FROM setting WHERE key=?1",
            [key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| aura_core::errors::db::statement_failed("read colour reference", &e))
    })?;
    text.map(|text| {
        if text.len() > 8 * 1024 * 1024 {
            return Err(invalid("Saved colour reference exceeds its size limit."));
        }
        serde_json::from_str(&text)
            .map_err(|_| invalid("Saved colour reference is unreadable. Analyse it again."))
    })
    .transpose()
}

fn store<T: Serialize>(state: &AppState, key: &str, value: &T) -> AuraResult<()> {
    let text = serde_json::to_string(value)
        .map_err(|_| invalid("Cannot serialize colour matching settings."))?;
    if text.len() > 8 * 1024 * 1024 {
        return Err(invalid("Colour matching settings exceed their size limit."));
    }
    let key = key.to_owned();
    let at = aura_catalog::rfc3339(state.clock().now_utc());
    state.catalog().writer().with(move |conn| {
        if key.starts_with(ASSET) {
            // One SQLite statement makes admission atomic even across two catalogue handles.
            // Existing identifiers remain updateable when the library is full.
            let changed = conn.execute("INSERT INTO setting(key,value_json,updated_at)
                SELECT ?1,?2,?3 WHERE EXISTS(SELECT 1 FROM setting WHERE key=?1)
                OR (SELECT COUNT(*) FROM setting WHERE key LIKE 'colour_reference_v1:%') < 128
                ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",
                rusqlite::params![key,text,at])
                .map_err(|e| aura_core::errors::db::statement_failed("save colour reference", &e))?;
            if changed == 0 { return Err(invalid("The reference library holds at most 128 references. Remove an unused reference before importing another.")); }
            return Ok(());
        }
        conn.execute("INSERT INTO setting(key,value_json,updated_at) VALUES(?1,?2,?3)
            ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",
            rusqlite::params![key, text, at])
            .map_err(|e| aura_core::errors::db::statement_failed("save colour reference", &e))?;
        Ok(())
    })
}

fn job<T>(
    state: &AppState,
    id: &str,
    run: impl FnOnce(&CancelToken) -> AuraResult<T>,
) -> IpcResult<T> {
    uuid::Uuid::parse_str(id).map_err(|_| invalid("Invalid colour matching job."))?;
    let cancel = CancelToken::new();
    state.register_job(id, cancel.clone());
    let result = {
        let _work = state.image_work_gate().acquire();
        check_cancel(&cancel).and_then(|()| run(&cancel))
    };
    state.finish_job(id);
    Ok(result?)
}
fn check_cancel(cancel: &CancelToken) -> AuraResult<()> {
    if cancel.is_cancelled() {
        Err(invalid("Colour matching stopped. No edit was saved."))
    } else {
        Ok(())
    }
}
fn asset_key(id: &str) -> AuraResult<String> {
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("Invalid colour reference."));
    }
    Ok(format!("{ASSET}{id}"))
}
fn session_key(photo: &str) -> AuraResult<String> {
    PhotoId::from_db(photo).map_err(|_| invalid("Invalid photo."))?;
    Ok(format!("{SESSION}{photo}"))
}

/// Import one finished reference into the durable catalogue library.
/// # Errors
/// Unsupported input/profile, allocation bounds, cancellation or analysis failure.
pub fn import_reference(state: &AppState, input: &ImportInput) -> IpcResult<ReferenceSummary> {
    job(state, &input.cancel_id, |cancel| {
        let decoded = reference_image::decode(Path::new(&input.path))?;
        let name = Path::new(&input.path)
            .file_name()
            .map_or_else(|| "Reference".into(), |s| s.to_string_lossy().into_owned());
        let summary = new_summary(
            decoded.hash,
            name,
            decoded.width,
            decoded.height,
            decoded.converted_icc,
        );
        persist_reference(state, summary, &decoded.image, cancel)
    })
}

fn new_summary(
    id: String,
    name: String,
    width: u32,
    height: u32,
    converted_icc: bool,
) -> ReferenceSummary {
    ReferenceSummary {
        id,
        name,
        width,
        height,
        converted_icc,
        regions: Vec::new(),
        group: String::new(),
        favorite: false,
        default_controls: None,
        thumbnail: None,
        preview: None,
    }
}

fn persist_reference(
    state: &AppState,
    mut summary: ReferenceSummary,
    image: &Rgb8,
    cancel: &CancelToken,
) -> AuraResult<ReferenceSummary> {
    check_cancel(cancel)?;
    let key = asset_key(&summary.id)?;
    transfer::remember(state, &summary.id, image)?;
    if let Some(mut existing) = read::<Reference>(state, &key)? {
        if existing.engine == ENGINE {
            if existing.summary.thumbnail.is_some() {
                let mut updated = false;
                if existing.small_rgb.len() != content::BYTES {
                    existing.small_rgb = content::thumbnail(image);
                    existing.achromatic = content::achromatic(image);
                    updated = true;
                }
                let preview = reference_preview(image, summary.width, summary.height)?;
                if existing.summary.preview.as_deref() != Some(preview.as_str()) {
                    existing.summary.preview = Some(preview);
                    updated = true;
                }
                if updated {
                    store(state, &key, &existing)?;
                }
                return Ok(existing.summary);
            }
            summary = existing.summary;
        }
    }
    let mut regions = appearance::analyse(image)?;
    regions.retain(|r| !r.name.starts_with("guard:"));
    check_cancel(cancel)?;
    summary.regions = regions.iter().map(|r| r.name.clone()).collect();
    let rgb = image::RgbImage::from_raw(image.width, image.height, image.data.clone())
        .ok_or_else(|| invalid("Reference preview dimensions are invalid."))?;
    let small = image::imageops::thumbnail(&rgb, 128, 128);
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            small.as_raw(),
            small.width(),
            small.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|_| invalid("Cannot save the reference thumbnail."))?;
    summary.thumbnail = Some(format!(
        "data:image/png;base64,{}",
        crate::develop_commands::base64(&bytes)
    ));
    summary.preview = Some(reference_preview(image, summary.width, summary.height)?);
    let regions = regions
        .into_iter()
        .map(|mut r| {
            r.points.clear();
            r
        })
        .collect();
    check_cancel(cancel)?;
    store(
        state,
        &key,
        &Reference {
            engine: ENGINE.into(),
            summary: summary.clone(),
            regions,
            small_rgb: content::thumbnail(image),
            achromatic: content::achromatic(image),
        },
    )?;
    Ok(summary)
}

/// Locally stored comparison image; portable libraries deliberately export descriptors only.
/// Bound each JPEG to 32 KiB so 128 references still fit the library metadata budget.
fn reference_preview(image: &Rgb8, width: u32, height: u32) -> AuraResult<String> {
    Ok(format!(
        "data:image/jpeg;base64,{}",
        crate::develop_commands::base64(&reference_preview_jpeg(image, width, height)?)
    ))
}

fn reference_preview_jpeg(image: &Rgb8, width: u32, height: u32) -> AuraResult<Vec<u8>> {
    let rgb = image::RgbImage::from_raw(image.width, image.height, image.data.clone())
        .ok_or_else(|| invalid("Reference preview dimensions are invalid."))?;
    for edge in [512, 384, 256, 128] {
        // imageops resizes to exact dimensions. Keep the oriented source ratio
        // for display without changing the measured pixels used by the matcher.
        let scale = f64::from(edge) / f64::from(width.max(height).max(1));
        let display_width = (f64::from(width) * scale).round().max(1.0) as u32;
        let display_height = (f64::from(height) * scale).round().max(1.0) as u32;
        let small = image::imageops::resize(
            &rgb,
            display_width,
            display_height,
            image::imageops::FilterType::Triangle,
        );
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 82)
            .encode(
                small.as_raw(),
                small.width(),
                small.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|_| invalid("Cannot save the reference comparison preview."))?;
        if bytes.len() <= 32 * 1024 {
            return Ok(bytes);
        }
    }
    Err(invalid(
        "The reference comparison preview exceeds its storage limit.",
    ))
}

/// List locally saved reference descriptors. Clearing preview caches does not delete these.
/// # Errors
/// Catalogue or stored reference failure.
pub fn list_references(state: &AppState) -> IpcResult<Vec<ReferenceSummary>> {
    library::ordered_references(state)
}

fn rendered(state: &AppState, frame: &Frame, recipe: &Recipe) -> AuraResult<Rgb8> {
    let image = state.render()?.render_frame(
        frame,
        recipe,
        RenderLevel::Full,
        RenderPurpose::Analysis,
        &OutputSpec::default(),
    )?;
    let RenderedData::Eight(pixels) = image.data else {
        return Err(invalid("Colour matching requires sRGB analysis output."));
    };
    Ok(Rgb8 {
        width: image.width,
        height: image.height,
        data: pixels,
    })
}
fn report(
    session: &Session,
    changed: usize,
    protected: usize,
    distance: f32,
    full: bool,
    current: bool,
) -> MatchReport {
    MatchReport {
        session_id: session.id.clone(),
        photo_id: session.photo_id.clone(),
        reference: session.reference.clone(),
        controls: session.controls,
        matched_regions: session
            .target_regions
            .iter()
            .filter(|a| session.reference_regions.iter().any(|r| r.name == a.name))
            .map(|r| r.name.clone())
            .collect(),
        before_distance: session.before_distance,
        after_distance: distance,
        changed,
        protected_fields: protected,
        full_resolution_checked: full,
        current,
        fitting: session.fitting.clone(),
        reference_monochrome: session.reference_monochrome,
    }
}

fn proposal(baseline: &Recipe, delta: &StyleDelta) -> AuraResult<Recipe> {
    if delta.is_neutral() {
        return Ok(baseline.clone());
    }
    let candidate = aura_look::verify::shift(baseline, delta);
    let merged = schema::merge(baseline, &candidate, EditSource::Ai)?.0;
    schema::Validation::check(&merged)?;
    Ok(merged)
}

fn matching_proposal(
    baseline: &Recipe,
    delta: &StyleDelta,
    monochrome: bool,
    strength: f32,
) -> AuraResult<Recipe> {
    let mut candidate = proposal(baseline, delta)?;
    if monochrome && strength > 0.0 {
        // Native saturation and the default BW operator use the same linear
        // luminance. Blend toward -100 without replacing an existing BW mixer.
        let saturation = f32::from(candidate.global.saturation);
        candidate.global.saturation =
            (saturation - (saturation + 100.0) * strength.clamp(0.0, 1.0)).round() as i16;
        candidate = schema::merge(baseline, &candidate, EditSource::Ai)?.0;
        schema::Validation::check(&candidate)?;
    }
    Ok(candidate)
}

fn skin_guard(before: &[Region], after: &[Region], monochrome: bool) -> bool {
    if monochrome {
        appearance::skin_preserved_achromatic(before, after)
    } else {
        appearance::skin_preserved(before, after)
    }
}

fn treatment_proposal(
    baseline: &Recipe,
    delta: &StyleDelta,
    monochrome: bool,
    monochrome_strength: f32,
    offsets: &[f32; 6],
    grade_strength: f32,
) -> AuraResult<Recipe> {
    let mut candidate = matching_proposal(baseline, delta, monochrome, monochrome_strength)?;
    if grade_strength > 0.0 && offsets.iter().any(|v| *v != 0.0) {
        grade::apply(&mut candidate.global.colour_grade, offsets, grade_strength);
        candidate = schema::merge(baseline, &candidate, EditSource::Ai)?.0;
        schema::Validation::check(&candidate)?;
    }
    Ok(candidate)
}

/// Measure one target and fit a bounded appearance proposal without changing its saved edit.
/// # Errors
/// Missing reference/baseline, cancellation, decode, render or catalogue failures.
// Keep the legacy recipe/hash/render checks in their existing cancellation scope.
#[allow(clippy::too_many_lines)]
pub fn prepare(state: &AppState, input: &PrepareInput) -> IpcResult<MatchReport> {
    job(state, &input.cancel_id, |cancel| {
        let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo."))?;
        let reference: Reference = read(state, &asset_key(&input.reference_id)?)?
            .filter(|r: &Reference| r.engine == ENGINE)
            .ok_or_else(|| invalid("Choose a saved reference image first."))?;
        require_distinct_reference(state, photo, &reference.summary.id)?;
        let baseline =
            AnalysisBaseline::select(state, photo, &reference.summary.id, input.replace_previous)?;
        // A reference measured by this build is matched by distribution transfer (ADR-0160).
        // One imported before it keeps the slider fit below until it is imported again.
        if let Some(stats) = transfer::recall(state, &input.reference_id)? {
            return matcher::prepare(state, cancel, input, photo, reference, baseline, stats);
        }
        if input.intent == MatchIntent::Palette {
            return Err(invalid("Reference palette matching needs fresh colour measurements. Upload its edited photo again, then analyse the match. No edit was saved."));
        }
        let observed_hash = baseline.observed_hash;
        let baseline = baseline.recipe;
        let baseline_hash = aura_recipe::hash::recipe_hash(&baseline)?;
        // Fit reduced pixels from the native full decode. A camera's embedded thumbnail
        // already has its own colour treatment and is not the RAW export's fitting domain.
        let frame = fitting_frame_quality(state, &photo, input.quality)?;
        let before = rendered(state, &frame, &baseline)?;
        let target_regions = appearance::analyse(&before)?;
        let before_distance = appearance::distance(&reference.regions, &target_regions);
        let mut seed = StyleDelta::neutral();
        let mass: f32 = target_regions
            .iter()
            .filter(|a| reference.regions.iter().any(|r| r.name == a.name))
            .map(|a| appearance::weight(&a.name))
            .sum();
        for a in &target_regions {
            if let Some(r) = reference.regions.iter().find(|r| r.name == a.name) {
                seed = seed.plus(
                    &aura_look::solve::initial(&r.appearance, &a.appearance)
                        .scaled(appearance::weight(&a.name) / mass.max(0.01)),
                );
            }
        }
        let mut fitted = StyleDelta::neutral();
        let mut best_distance = before_distance;
        let before_clip = appearance::clipped(&before);
        let reference_monochrome = reference.achromatic
            && appearance::achromatic_reference(&reference.regions)
            && target_regions
                .iter()
                .find(|r| r.name == "whole image")
                .is_some_and(|r| r.appearance.chroma_p90 * 128.0 > 2.0);
        let mut scores =
            std::collections::BTreeMap::from([(baseline_hash.clone(), before_distance)]);
        let mut cached_proposals = 0;
        let neutral_grade = [0.0; 6];
        let mut evaluate = |candidate: &StyleDelta, offsets: &[f32; 6]| -> AuraResult<f32> {
            check_cancel(cancel)?;
            let candidate = treatment_proposal(
                &baseline,
                candidate,
                reference_monochrome,
                1.0,
                offsets,
                1.0,
            )?;
            let key = aura_recipe::hash::recipe_hash(&candidate)?;
            if let Some(score) = scores.get(&key) {
                cached_proposals += 1;
                return Ok(*score);
            }
            let pixels = rendered(state, &frame, &candidate)?;
            let readings = appearance::readings(&pixels, &target_regions);
            let distance = if appearance::clipped(&pixels) > before_clip + 0.01
                || !skin_guard(&target_regions, &readings, reference_monochrome)
            {
                f32::INFINITY
            } else {
                appearance::distance(&reference.regions, &readings)
            };
            scores.insert(key, distance);
            Ok(distance)
        };
        if reference_monochrome {
            let distance = evaluate(&fitted, &neutral_grade)?;
            if !distance.is_finite() {
                return Err(invalid("This monochrome reference could not preserve the photo's skin tones and highlights. No edit was saved."));
            }
            best_distance = distance;
        }
        // Exposure, curves and cast can compensate for each other. Test joint bounded
        // seeds before refinement rather than requiring each axis to improve alone.
        for factor in [0.25, 0.5, 0.75, 1.0] {
            let candidate = seed.scaled(factor);
            let distance = evaluate(&candidate, &neutral_grade)?;
            if distance.is_finite() && distance + 1e-4 < best_distance {
                fitted = candidate;
                best_distance = distance;
            }
        }
        // Each component is fitted separately from the immutable baseline. Disabled axes never
        // require compensating changes, and no candidate can trade skin protection for a lower loss.
        for axis in 0..12 {
            for factor in [0.0, 0.5, 1.0] {
                let mut candidate = fitted.clone();
                set_axis(&mut candidate, &seed, axis, factor);
                let distance = evaluate(&candidate, &neutral_grade)?;
                if distance.is_finite() && distance + 1e-4 < best_distance {
                    fitted = candidate;
                    best_distance = distance;
                }
            }
        }
        // Keep the established guarded fit as an incumbent, then search signed
        // additive corrections. A zero or incorrectly signed heuristic no longer
        // freezes a parameter; each tone-curve anchor and supported colour band
        // can move independently. No candidate can replace the incumbent unless
        // its actual rendered appearance improves and the existing guards pass.
        let incumbent = fitted.clone();
        let structure_similarity = content::similarity(&reference.small_rgb, &before);
        let regularization_weight = content::regularization(structure_similarity);
        let supported_bands = std::array::from_fn(|index| {
            target_regions.iter().any(|actual| {
                reference
                    .regions
                    .iter()
                    .find(|r| r.name == actual.name)
                    .is_some_and(|r| {
                        actual
                            .appearance
                            .bands
                            .get(index)
                            .zip(r.appearance.bands.get(index))
                            .is_some_and(|(a, b)| {
                                a.share >= aura_look::solve::BAND_MIN_SHARE
                                    && b.share >= aura_look::solve::BAND_MIN_SHARE
                            })
                    })
            })
        });
        let refined = refine::search(&fitted, best_distance, supported_bands, |d| {
            evaluate(d, &neutral_grade)
                .map(|score| score + regularization_weight * refine::effort(d, &incumbent))
        })?;
        fitted = refined.delta;
        best_distance = refined.distance;
        let (mut grade_offsets, grade_distance, grade_candidates) = if reference_monochrome {
            (neutral_grade, best_distance, 0)
        } else {
            grade::search(best_distance, |offsets| {
                evaluate(&fitted, offsets).map(|score| {
                    let wheel_effort: f32 = offsets
                        .chunks_exact(2)
                        .filter_map(|p| match p {
                            &[x, y] => Some(x.hypot(y) / grade::MAX_CAST),
                            _ => None,
                        })
                        .sum();
                    score
                        + regularization_weight
                            * (refine::effort(&fitted, &incumbent) + wheel_effort)
                })
            })?
        };
        best_distance = grade_distance;
        // Reduced fitting can miss a small highlight or a person's skin edge.
        // Validate the proposal on the actual native decode before publishing the
        // analysis. Backtrack the fitted treatment, not the user's later sliders.
        // Apply still rechecks the exact requested strengths against current data.
        let full_frame = crate::photo_frames::CatalogFrames::new(state.clone())
            .frame(&photo, RenderLevel::Full)?;
        check_cancel(cancel)?;
        let native_before = rendered(state, &full_frame, &baseline)?;
        let native_regions = appearance::readings(&native_before, &target_regions);
        let native_clip = appearance::clipped(&native_before);
        let native_distance = appearance::distance(&reference.regions, &native_regions);
        drop(native_before);
        let mut native_backtrack_factor = None;
        for factor in [1.0, 0.75, 0.5, 0.25, 0.0] {
            check_cancel(cancel)?;
            let delta = fitted.scaled(factor);
            let offsets = grade_offsets.map(|v| v * factor);
            let candidate =
                treatment_proposal(&baseline, &delta, reference_monochrome, 1.0, &offsets, 1.0)?;
            let pixels = rendered(state, &full_frame, &candidate)?;
            let readings = appearance::readings(&pixels, &target_regions);
            let distance = appearance::distance(&reference.regions, &readings);
            if distance.is_finite()
                && distance <= native_distance + 0.0001
                && appearance::clipped(&pixels) <= native_clip + 0.01
                && skin_guard(&native_regions, &readings, reference_monochrome)
            {
                fitted = delta;
                grade_offsets = offsets;
                native_backtrack_factor = Some(factor);
                // Keep preview before/after measurements in the same fitting domain.
                best_distance = evaluate(&fitted, &grade_offsets)?;
                break;
            }
        }
        let native_backtrack_factor = native_backtrack_factor.ok_or_else(|| invalid(
            "No colour treatment could preserve this photo's native skin tones and highlights. No edit was saved."))?;
        let fitting = Some(FitDiagnostics {
            engine: "bounded-pattern-search-v1".into(),
            intent: input.intent,
            quality: input.quality,
            target_samples: (frame.width as usize) * (frame.height as usize),
            max_iterations: 0,
            refinement_candidates: refined.evaluations,
            unique_renders: scores.len().saturating_sub(1),
            cached_proposals,
            grade_candidates,
            native_candidate_checked: true,
            native_backtrack_factor,
            structure_similarity,
            regularization_weight,
            transfer: None,
        });
        check_cancel(cancel)?;
        baseline::verify_current(state, photo, &observed_hash)?;
        let default_controls = reference.summary.default_controls.unwrap_or_default();
        let session = Session {
            engine: ENGINE.into(),
            id: uuid::Uuid::new_v4().to_string(),
            photo_id: input.photo_id.clone(),
            reference: reference.summary,
            baseline,
            baseline_hash: baseline_hash.clone(),
            last_hash: observed_hash,
            fitted,
            reference_regions: reference.regions,
            target_regions,
            before_distance,
            fitted_distance: best_distance,
            controls: default_controls,
            checked_hash: None,
            measured_before: None,
            measured_after: None,
            fitting,
            reference_monochrome,
            grade_offsets,
            transfer: None,
        };
        let result = report(
            &session,
            0,
            session.baseline.provenance.user_edited_fields.len(),
            best_distance,
            false,
            true,
        );
        store(state, &session_key(&input.photo_id)?, &session)?;
        Ok(result)
    })
}

fn fitting_frame(state: &AppState, photo: &PhotoId) -> AuraResult<Frame> {
    fitting_frame_quality(state, photo, MatchQuality::Precise)
}

fn fitting_frame_quality(
    state: &AppState,
    photo: &PhotoId,
    quality: MatchQuality,
) -> AuraResult<Frame> {
    let full =
        crate::photo_frames::CatalogFrames::new(state.clone()).frame(photo, RenderLevel::Full)?;
    let edge = full.width.max(full.height).max(1);
    let scale = (f64::from(quality.fitting_edge()) / f64::from(edge)).min(1.0);
    let width = (f64::from(full.width) * scale).round().max(1.0) as u32;
    let height = (f64::from(full.height) * scale).round().max(1.0) as u32;
    let small = aura_raw::demosaic::resize(
        &aura_raw::demosaic::RgbF32 {
            width: full.width,
            height: full.height,
            data: full.rgb,
        },
        width,
        height,
    );
    Ok(Frame::working(
        small.data,
        small.width,
        small.height,
        &full.camera,
    ))
}

fn set_axis(out: &mut StyleDelta, seed: &StyleDelta, axis: usize, factor: f32) {
    match axis {
        0 => out.exposure = seed.exposure * factor,
        1 => out.temperature_k = seed.temperature_k * factor,
        2 => out.tint = seed.tint * factor,
        3 => out.contrast = seed.contrast * factor,
        4 => out.highlights = seed.highlights * factor,
        5 => out.shadows = seed.shadows * factor,
        6 => out.whites = seed.whites * factor,
        7 => out.blacks = seed.blacks * factor,
        8 => out.vibrance = seed.vibrance * factor,
        9 => out.saturation = seed.saturation * factor,
        10 => out.curve_shift = seed.curve_shift.scaled(factor),
        _ => out.hsl = seed.hsl.attenuated(factor),
    }
}

/// Restore saved slider state after navigation/restart; stale sessions remain visibly stale.
/// # Errors
/// Invalid photo or unreadable catalogue state.
pub fn status(state: &AppState, photo_id: &str) -> IpcResult<Option<MatchReport>> {
    let Some(session) = read::<Session>(state, &session_key(photo_id)?)? else {
        return Ok(None);
    };
    let photo = PhotoId::from_db(photo_id).map_err(|_| invalid("Invalid photo."))?;
    let hash =
        aura_recipe::hash::recipe_hash(&crate::develop_commands::load_or_neutral(state, photo)?)?;
    let measurements_current = session
        .transfer
        .as_ref()
        .is_none_or(|saved| saved.reference.usable());
    let checked = measurements_current && session.checked_hash.as_deref() == Some(hash.as_str());
    let mut result = report(
        &session,
        0,
        session.baseline.provenance.user_edited_fields.len(),
        if checked {
            session.measured_after.unwrap_or(session.fitted_distance)
        } else {
            session.fitted_distance
        },
        checked,
        measurements_current
            && session.engine == ENGINE
            && (hash == session.last_hash || hash == session.baseline_hash),
    );
    if checked {
        result.before_distance = session.measured_before.unwrap_or(session.before_distance);
    }
    if hash == session.baseline_hash && hash != session.last_hash {
        result.controls.amount = 0.0;
    }
    Ok(Some(result))
}

/// Apply explicit strengths from the saved baseline, checking actual full-resolution renders.
/// # Errors
/// Invalid strengths, stale baseline, missing source, cancellation or full-resolution guard failure.
// Keep the legacy validation, merge and save in the same guarded job scope.
#[allow(clippy::too_many_lines)]
pub fn apply(state: &AppState, input: &ApplyInput) -> IpcResult<MatchReport> {
    input.controls.check()?;
    job(state, &input.cancel_id, |cancel| {
        let key = session_key(&input.photo_id)?;
        let mut session: Session = read(state, &key)?
            .ok_or_else(|| invalid("Analyse a reference for this photo first."))?;
        if session.engine != ENGINE
            || session.id != input.session_id
            || session.photo_id != input.photo_id
        {
            return Err(invalid(
                "This reference analysis is no longer current. Analyse it again.",
            ));
        }
        let photo = PhotoId::from_db(&input.photo_id).map_err(|_| invalid("Invalid photo."))?;
        require_distinct_reference(state, photo, &session.reference.id)?;
        let saved = state.recipe_store().load(&photo)?;
        let has_saved_recipe = saved.is_some();
        let current =
            saved.unwrap_or_else(|| crate::develop_commands::neutral_recipe(state, photo));
        let hash = aura_recipe::hash::recipe_hash(&current)?;
        if hash != session.last_hash && hash != session.baseline_hash {
            return Err(invalid("The photo changed after reference analysis. Your newer edit was kept. Analyse again before applying."));
        }
        if let Some(transfer) = session.transfer.clone() {
            return matcher::apply(
                state,
                cancel,
                input,
                &key,
                session,
                transfer,
                photo,
                &current,
                hash,
                has_saved_recipe,
            );
        }
        let mono_strength =
            input.controls.amount * input.controls.color * input.controls.detail.saturation;
        let grade_strength = input.controls.amount
            * input.controls.color
            * input.controls.detail.saturation
            * input.controls.detail.hue;
        let candidate = treatment_proposal(
            &session.baseline,
            &input.controls.delta(&session.fitted),
            session.reference_monochrome,
            mono_strength,
            &session.grade_offsets,
            grade_strength,
        )?;
        let frame = crate::photo_frames::CatalogFrames::new(state.clone())
            .frame(&photo, RenderLevel::Full)?;
        check_cancel(cancel)?;
        let before = rendered(state, &frame, &session.baseline)?;
        let baseline_regions = appearance::readings(&before, &session.target_regions);
        let before_distance = appearance::distance(&session.reference_regions, &baseline_regions);
        let before_clip = appearance::clipped(&before);
        drop(before);
        check_cancel(cancel)?;
        let after = rendered(state, &frame, &candidate)?;
        let after_regions = appearance::readings(&after, &session.target_regions);
        if appearance::clipped(&after) > before_clip + 0.01
            || !skin_guard(
                &baseline_regions,
                &after_regions,
                session.reference_monochrome,
            )
        {
            return Err(invalid("This strength failed the full-resolution highlight or skin-colour check. Reduce the strengths and apply again."));
        }
        let distance = appearance::distance(&session.reference_regions, &after_regions);
        if !distance.is_finite() {
            return Err(invalid(
                "The final colour measurement was unavailable. No edit was saved.",
            ));
        }
        if distance > before_distance + 0.0001 {
            return Err(invalid("These strengths moved the native-resolution colour farther from the reference. Reduce the strengths or analyse again. No edit was saved."));
        }
        let (mut merged, mut changes) = schema::merge(&current, &candidate, EditSource::Ai)?;
        // A zero setting must remove HSL entries introduced by this session. The generic
        // leaf merge retains absent map leaves. This session's hash check proves no newer
        // edit occurred, and candidate was already merged against the protected baseline.
        if merged.global.hsl != candidate.global.hsl {
            merged.global.hsl.clone_from(&candidate.global.hsl);
            changes.changed.push("global.hsl".into());
        }
        if merged.global.colour_grade != candidate.global.colour_grade {
            merged.global.colour_grade = candidate.global.colour_grade;
            changes.changed.push("global.colour_grade".into());
        }
        check_cancel(cancel)?;
        let project: String = state.catalog().read(|conn| {
            conn.query_row(
                "SELECT project_id FROM photo WHERE photo_id=?1",
                [&input.photo_id],
                |r| r.get(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("colour match photo", &e))
        })?;
        let project = ProjectId::from_db(&project).map_err(|_| invalid("Invalid collection."))?;
        if !changes.changed.is_empty() {
            save_verified(state, &project, &photo, &merged, &changes.changed,
                &format!("Reference colour: {}; amount {:.0}%, tone {:.0}%, colour {:.0}%, warmth {:.0}%; native-resolution appearance distance {:.2}",
                    session.reference.name, input.controls.amount * 100.0, input.controls.tone * 100.0,
                    input.controls.color * 100.0, input.controls.white_balance * 100.0, distance), has_saved_recipe.then_some(hash.as_str()))?;
        }
        session.last_hash = if changes.changed.is_empty() {
            hash
        } else {
            aura_recipe::hash::recipe_hash(&merged)?
        };
        session.controls = input.controls;
        session.checked_hash = Some(session.last_hash.clone());
        session.measured_before = Some(before_distance);
        session.measured_after = Some(distance);
        // If this bookkeeping write fails, later adjustments fail closed against the stale
        // session hash. The history entry and Undo remain available; no newer edit is overwritten.
        store(state, &key, &session)?;
        let mut result = report(
            &session,
            changes.changed.len(),
            changes.refused.len(),
            distance,
            true,
            true,
        );
        result.before_distance = before_distance;
        Ok(result)
    })
}

/// Both ingest and uploaded-reference IDs hash the complete source file with BLAKE3.
/// Check every associated source (for example RAW plus JPEG), independent of its filename.
fn require_distinct_reference(state: &AppState, photo: PhotoId, reference: &str) -> AuraResult<()> {
    let identical: bool = state.catalog().read(|conn| {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM photo_file WHERE photo_id=?1 AND content_hash=?2)",
            rusqlite::params![photo.to_db(), reference],
            |row| row.get(0),
        )
        .map_err(|e| {
            aura_core::errors::db::statement_failed("colour reference source identity", &e)
        })
    })?;
    if identical {
        return Err(invalid("Choose a different photo as the colour reference. This reference is the same source photo as the target."));
    }
    Ok(())
}

/// Preserve the recipe-presence snapshot taken before verification through the atomic save.
/// An absent recipe is an implicit neutral baseline, never permission to overwrite a first edit.
fn save_verified(
    state: &AppState,
    project: &ProjectId,
    photo: &PhotoId,
    recipe: &Recipe,
    changed: &[String],
    label: &str,
    expected_hash: Option<&str>,
) -> AuraResult<()> {
    match expected_hash {
        Some(hash) => state
            .recipe_store()
            .save_if_current(project, photo, recipe, changed, label, hash),
        None => state
            .recipe_store()
            .save_first(project, photo, recipe, changed, label),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_comparison_preview_is_bounded_and_separate_from_portable_descriptors() {
        let image = Rgb8 {
            width: 600,
            height: 400,
            data: (0..600 * 400 * 3).map(|i| (i % 251) as u8).collect(),
        };
        let preview = reference_preview(&image, 600, 400).unwrap();
        assert!(preview.starts_with("data:image/jpeg;base64,"));
        assert!(
            preview.len() <= "data:image/jpeg;base64,".len() + (32_usize * 1024).div_ceil(3) * 4
        );
        let old: ReferenceSummary = serde_json::from_str(r#"{"id":"reference","name":"Old","width":600,"height":400,"convertedIcc":false,"regions":[]}"#).unwrap();
        assert!(old.preview.is_none());
    }

    #[test]
    fn comparison_preview_preserves_landscape_and_portrait_photo_shapes() {
        for (width, height) in [(640, 384), (384, 640)] {
            let samples = Rgb8 {
                width,
                height,
                data: vec![128; (width * height * 3) as usize],
            };
            let bytes = reference_preview_jpeg(&samples, width, height).unwrap();
            let image = image::load_from_memory(&bytes).unwrap();
            assert!(image.width().max(image.height()) <= 512);
            let ratio = f64::from(image.width()) / f64::from(image.height());
            assert!((ratio - f64::from(width) / f64::from(height)).abs() < 0.004);
        }
    }

    #[test]
    fn monochrome_contribution_is_continuous_reversible_and_preserves_manual_saturation() {
        let mut baseline =
            aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "test");
        baseline.global.saturation = 20;
        let zero = StyleDelta::neutral();
        assert_eq!(
            matching_proposal(&baseline, &zero, true, 0.0).unwrap(),
            baseline
        );
        assert_eq!(
            matching_proposal(&baseline, &zero, true, 0.5)
                .unwrap()
                .global
                .saturation,
            -40
        );
        assert_eq!(
            matching_proposal(&baseline, &zero, true, 1.0)
                .unwrap()
                .global
                .saturation,
            -100
        );
        assert_eq!(
            matching_proposal(&baseline, &zero, false, 1.0).unwrap(),
            baseline
        );
        baseline
            .provenance
            .user_edited_fields
            .push("global.saturation".into());
        assert_eq!(
            matching_proposal(&baseline, &zero, true, 1.0)
                .unwrap()
                .global
                .saturation,
            20
        );
    }

    fn fitted() -> StyleDelta {
        let mut value = StyleDelta::neutral();
        value.exposure = 0.5;
        value.contrast = 10.0;
        value.temperature_k = 300.0;
        value.tint = 3.0;
        value.vibrance = 12.0;
        value.saturation = 8.0;
        value.hsl.set(
            HslBand::Blue,
            aura_core::contract::colour::HslShift {
                h: 0.0,
                s: 5.0,
                l: 4.0,
            },
        );
        value
    }

    #[test]
    fn zero_amount_restores_global_parameters_and_keeps_the_original_recipe() {
        let baseline = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "test");
        let controls = Controls {
            amount: 0.0,
            ..Controls::default()
        };
        let restored = proposal(&baseline, &controls.delta(&fitted())).unwrap();
        assert_eq!(restored.global, baseline.global);
        assert_eq!(restored.geometry, baseline.geometry);
        assert_eq!(restored.retouch, baseline.retouch);
        assert_eq!(restored.masks, baseline.masks);
    }

    #[test]
    fn tone_colour_and_warmth_are_independent() {
        let tone = Controls {
            tone: 0.0,
            ..Controls::default()
        }
        .delta(&fitted());
        assert_eq!(tone.exposure, 0.0);
        assert_eq!(tone.hsl.get(HslBand::Blue).l, 0.0);
        assert!(tone.vibrance > 0.0 && tone.temperature_k > 0.0);
        let color = Controls {
            color: 0.0,
            ..Controls::default()
        }
        .delta(&fitted());
        assert_eq!(color.saturation, 0.0);
        assert_eq!(color.hsl.get(HslBand::Blue).s, 0.0);
        assert!(color.exposure > 0.0 && color.temperature_k > 0.0);
        let warmth = Controls {
            white_balance: 0.0,
            ..Controls::default()
        }
        .delta(&fitted());
        assert_eq!(warmth.temperature_k, 0.0);
        assert_eq!(warmth.tint, 0.0);
        assert!(warmth.exposure > 0.0 && warmth.vibrance > 0.0);
    }

    #[test]
    fn lowering_then_raising_strength_uses_the_same_proposal_without_compounding() {
        let baseline = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "test");
        let full = proposal(&baseline, &Controls::default().delta(&fitted())).unwrap();
        let half = proposal(
            &baseline,
            &Controls {
                amount: 0.5,
                ..Controls::default()
            }
            .delta(&fitted()),
        )
        .unwrap();
        let again = proposal(&baseline, &Controls::default().delta(&fitted())).unwrap();
        assert_eq!(full.global, again.global);
        assert!(half.global.exposure < full.global.exposure);
        assert_eq!(half.global.exposure, 0.25);
    }

    #[test]
    fn invalid_strengths_are_refused_in_the_native_layer() {
        for amount in [-0.1, 2.1, f32::NAN, f32::INFINITY] {
            assert!(Controls {
                amount,
                ..Controls::default()
            }
            .check()
            .is_err());
        }
    }

    #[test]
    fn fine_strengths_separate_exposure_and_temperature_from_other_fitted_axes() {
        let changed = Controls {
            detail: FineControls {
                exposure: 0.0,
                temperature: 0.0,
                ..FineControls::default()
            },
            ..Controls::default()
        }
        .delta(&fitted());
        assert_eq!(changed.exposure, 0.0);
        assert_eq!(changed.temperature_k, 0.0);
        assert!(changed.contrast > 0.0 && changed.tint > 0.0 && changed.saturation > 0.0);
        assert!(Controls {
            amount: 1.5,
            ..Controls::default()
        }
        .check()
        .is_ok());
        assert!(Controls {
            detail: FineControls {
                hue: f32::NAN,
                ..FineControls::default()
            },
            ..Controls::default()
        }
        .check()
        .is_err());
    }

    #[test]
    fn older_matching_sessions_decode_to_identical_full_detail_strengths() {
        let old: Controls =
            serde_json::from_str(r#"{"amount":1,"tone":1,"color":1,"whiteBalance":1}"#).unwrap();
        assert_eq!(old, Controls::default());
        let partial: FineControls = serde_json::from_str(r#"{"exposure":0.5}"#).unwrap();
        assert_eq!(partial.exposure, 0.5);
        assert_eq!(partial.tint, 1.0);
    }
}
