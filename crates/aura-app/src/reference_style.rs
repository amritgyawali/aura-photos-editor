//! Reference-first local style analysis and per-photo, rendered style fitting.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_core::contract::look::{LookAggregate, ReferenceOrigin, MIN_REFERENCES};
use aura_core::contract::style::{LightingBucket, StyleDelta};
use aura_core::progress::CancelToken;
use aura_core::{PhotoId, ProjectId};
use aura_look::verify::{OwnFrame, Renderer};
use aura_look::{aggregate, measure, solve};
use aura_preview::contract::service::{PreviewService, Priority};
use aura_recipe::{schema, EditSource};
use aura_render::{FrameSource, RenderLevel};
use serde::{Deserialize, Serialize};

use crate::{commands::IpcResult, AppState};
pub use aura_cloud::instagram::FetchReport;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceAnalysis {
    pub id: String,
    pub origin: String,
    pub measured: usize,
    pub skipped: usize,
    pub colors: Vec<String>,
    pub brightness: f32,
    pub contrast: f32,
    pub warmth: f32,
    pub saturation: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredReference {
    version: u16,
    analysis: ReferenceAnalysis,
    global: LookAggregate,
    lighting: BTreeMap<LightingBucket, LookAggregate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyseReferenceInput {
    pub address: String,
    pub folder: String,
    pub cancel_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchInstagramInput {
    pub address: String,
    pub limit: u32,
    pub cancel_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReferenceInput {
    pub photo_id: String,
    pub reference_id: String,
    pub strength: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReferenceReport {
    pub changed: usize,
    pub before_distance: f32,
    pub after_distance: f32,
    pub protected_fields: usize,
}

fn refused(message: impl Into<String>) -> aura_core::AuraError {
    let message = message.into();
    let mut error = aura_core::errors::ml::look_reference_refused(message.clone());
    error.user_message = message;
    error
}

fn profile_path(state: &AppState, id: &str) -> aura_core::AuraResult<PathBuf> {
    let id =
        uuid::Uuid::parse_str(id).map_err(|_| refused("Invalid reference style identifier"))?;
    Ok(state
        .cache_root()
        .join("reference-styles")
        .join(format!("{id}.json")))
}

/// Retrieve publicly accessible images, reporting actual coverage.
/// # Errors
/// Invalid profile links, unavailable Python, cancellation, and network failures.
pub fn fetch_instagram(
    state: &AppState,
    input: &FetchInstagramInput,
) -> IpcResult<aura_cloud::instagram::FetchReport> {
    let ReferenceOrigin::Instagram { handle } = ReferenceOrigin::parse(&input.address)? else {
        return Err(refused(
            "Paste an Instagram profile link, such as https://www.instagram.com/chrisburkard/",
        )
        .into());
    };
    let root = state
        .cache_root()
        .join("instagram-references")
        .join(uuid::Uuid::new_v4().to_string());
    let cancel = CancelToken::new();
    state.register_job(&input.cancel_id, cancel.clone());
    let result =
        aura_cloud::instagram::fetch(&handle, &root, input.limit, &cancel, state.clock().as_ref());
    state.finish_job(&input.cancel_id);
    Ok(result?)
}

/// Analyse saved reference pixels before the user imports any target photographs.
/// # Errors
/// Missing/insufficient reference images, cancellation, or cache write failure.
pub fn analyse_reference(
    state: &AppState,
    input: &AnalyseReferenceInput,
) -> IpcResult<ReferenceAnalysis> {
    let cancel = CancelToken::new();
    state.register_job(&input.cancel_id, cancel.clone());
    let result = analyse(state, input, &cancel);
    state.finish_job(&input.cancel_id);
    Ok(result?)
}

fn analyse(
    state: &AppState,
    input: &AnalyseReferenceInput,
    cancel: &CancelToken,
) -> aura_core::AuraResult<ReferenceAnalysis> {
    let reference = aura_look::source::resolve(
        &input.address,
        aura_core::contract::look::MediaSource::Folder,
        Some(Path::new(&input.folder)),
    )?;
    let mut readings = Vec::new();
    let mut palette = [0_usize; 64];
    for file in &reference.files {
        if cancel.is_cancelled() {
            return Err(refused(
                "Reference analysis stopped. No style was selected.",
            ));
        }
        if let Ok(image) = aura_look::source::decode(file) {
            let image = aura_raw::thumb::scale_to(&image, 256);
            readings.push(measure::read_at(&file.key, &image, 256));
            for pixel in image.data.chunks_exact(3) {
                if let [r, g, b] = pixel {
                    let bin =
                        usize::from(*r / 64) * 16 + usize::from(*g / 64) * 4 + usize::from(*b / 64);
                    if let Some(count) = palette.get_mut(bin) {
                        *count += 1;
                    }
                }
            }
        }
    }
    if readings.len() < MIN_REFERENCES as usize {
        return Err(refused(format!("Only {} reference photos decoded successfully. Add at least {MIN_REFERENCES} distinct JPEG or PNG photos.", readings.len())));
    }
    let global = aggregate::fold(&aggregate::all(&readings));
    let mut colors: Vec<_> = palette
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .collect();
    colors.sort_by_key(|(bin, count)| (std::cmp::Reverse(**count), *bin));
    let analysis = ReferenceAnalysis {
        id: uuid::Uuid::new_v4().to_string(),
        origin: reference.origin.title(),
        measured: readings.len(),
        skipped: reference.files.len() - readings.len(),
        colors: colors
            .iter()
            .take(6)
            .map(|(bin, _)| {
                format!(
                    "#{:02x}{:02x}{:02x}",
                    (bin / 16) * 64 + 32,
                    (bin / 4 % 4) * 64 + 32,
                    (bin % 4) * 64 + 32
                )
            })
            .collect(),
        brightness: global.tone.p50,
        contrast: global.tone.p95 - global.tone.p05,
        warmth: global.mid.b,
        saturation: global.chroma_p50,
    };
    let lighting = aggregate::by_lighting(&readings)
        .into_iter()
        .filter_map(|(bucket, rows)| (rows.len() >= 4).then(|| (bucket, aggregate::fold(&rows))))
        .collect();
    let stored = StoredReference {
        version: 1,
        analysis: analysis.clone(),
        global,
        lighting,
    };
    let path = profile_path(state, &analysis.id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| refused(e.to_string()))?;
    }
    let bytes = serde_json::to_vec(&stored).map_err(|e| refused(e.to_string()))?;
    std::fs::write(path, bytes).map_err(|e| refused(e.to_string()))?;
    Ok(analysis)
}

/// Fit the reference to one photo through actual rendered candidates, preserving manual edits.
/// Repeated application starts from a fresh local baseline, so the look never compounds.
/// # Errors
/// Missing cached reference, invalid inputs, pixel decode, render, or recipe storage failure.
pub fn apply_reference(
    state: &AppState,
    input: &ApplyReferenceInput,
) -> IpcResult<ApplyReferenceReport> {
    if !input.strength.is_finite() || !(0.0..=1.0).contains(&input.strength) {
        return Err(refused("Look strength must be between 0 and 100 percent").into());
    }
    let bytes = std::fs::read(profile_path(state, &input.reference_id)?).map_err(|_| {
        refused("This reference style is no longer cached. Analyse the references again.")
    })?;
    let reference: StoredReference =
        serde_json::from_slice(&bytes).map_err(|e| refused(e.to_string()))?;
    if reference.version != 1 {
        return Err(refused("Analyse this reference again with the current version.").into());
    }
    let photo =
        PhotoId::from_db(&input.photo_id).map_err(|_| refused("Invalid photo identifier"))?;
    let project_key: String = state.catalog().read(|conn| {
        conn.query_row(
            "SELECT project_id FROM photo WHERE photo_id=?1",
            [&input.photo_id],
            |row| row.get(0),
        )
        .map_err(|e| aura_core::errors::db::statement_failed("reference photo", &e))
    })?;
    let project = ProjectId::from_db(&project_key).map_err(|_| refused("Invalid collection"))?;
    let original = state.previews(&project_key)?.get(
        photo,
        aura_raw::PixelLevel::Thumb(384),
        Priority::Interactive,
    )?;
    let (exposure, highlights, shadows, contrast) = crate::photo_enhance::correction(
        original
            .as_srgb8()
            .ok_or_else(|| refused("Photo preview is not sRGB"))?,
    )?;
    let current = crate::develop_commands::load_or_neutral(state, photo)?;
    let neutral = aura_recipe::fixtures::neutral(&input.photo_id, &current.image.camera);
    let mut baseline = aura_style::extract::TheirParams::of_recipe(&neutral).into_recipe(&current);
    baseline.global.exposure = exposure;
    baseline.global.highlights = highlights;
    baseline.global.shadows = shadows;
    baseline.global.contrast = contrast;
    baseline = schema::merge(&current, &baseline, EditSource::Ai)?.0;
    let pixels = crate::photo_frames::CatalogFrames::new(state.clone())
        .frame(&photo, RenderLevel::Screen(384, 384))?;
    let mut frame = OwnFrame {
        key: input.photo_id.clone(),
        width: pixels.width,
        height: pixels.height,
        frame: pixels,
        baseline: baseline.clone(),
        lighting: LightingBucket::Unknown,
        user_edited: false,
    };
    let renderer = Renderer::new(
        Arc::clone(state.clock()),
        aura_render::contract::render::OutputSpec::default(),
    );
    let reading = renderer.read_with(&frame, &StyleDelta::neutral())?;
    let target = reference
        .lighting
        .get(&reading.lighting)
        .filter(|_| reading.lighting_confidence >= 0.5)
        .unwrap_or(&reference.global);
    let measured = aggregate::fold(&[&reading]);
    let initial = solve::initial(target, &measured);
    let before = solve::distance(target, &measured);
    let mut best_distance = before;
    let mut best = baseline.clone();
    // Compare bounded candidates through the same renderer used by export. A
    // stronger setting cannot force a candidate whose measured fit is worse.
    if input.strength > 0.0 {
        for factor in [1.0, 0.66, 0.33] {
            let proposal =
                aura_look::verify::shift(&baseline, &initial.scaled(input.strength * factor));
            let candidate = schema::merge(&current, &proposal, EditSource::Ai)?.0;
            schema::Validation::check(&candidate)?;
            frame.baseline = candidate.clone();
            let result = renderer.read_with(&frame, &StyleDelta::neutral())?;
            let distance = solve::distance(target, &aggregate::fold(&[&result]));
            if distance < best_distance {
                best_distance = distance;
                best = candidate;
            }
        }
    }
    best.provenance.source = EditSource::Ai;
    best.provenance.confidence = 0.4;
    let (merged, report) = schema::merge(&current, &best, EditSource::Ai)?;
    schema::Validation::check(&merged)?;
    state.recipe_store().save(&project, &photo, &merged, &report.changed,
        &format!("Reference style: {} ({} photos); measured appearance distance {:.2} → {:.2}. Original settings cannot be recovered from a finished photo.", reference.analysis.origin, reference.analysis.measured, before, best_distance))?;
    Ok(ApplyReferenceReport {
        changed: report.changed.len(),
        before_distance: before,
        after_distance: best_distance,
        protected_fields: current.provenance.user_edited_fields.len(),
    })
}
