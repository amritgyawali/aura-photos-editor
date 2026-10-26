//! Reference-first local style analysis and per-photo, rendered style fitting.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use aura_core::contract::look::{LookAggregate, MIN_REFERENCES};
use aura_core::contract::style::LightingBucket;
use aura_core::progress::CancelToken;
use aura_core::{PhotoId, ProjectId};
use aura_look::{aggregate, measure};
use aura_recipe::EditSource;
use aura_render::{FrameSource, RenderLevel};
use serde::{Deserialize, Serialize};

use crate::color_match::gallery_fit;
use crate::{commands::IpcResult, AppState};

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration: Option<crate::color_match::CalibrationReport>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredReference {
    version: u16,
    analysis: ReferenceAnalysis,
    global: LookAggregate,
    lighting: BTreeMap<LightingBucket, LookAggregate>,
    #[serde(default)]
    thumbnail: Vec<u8>,
    #[serde(default)]
    dimensions: [u32; 2],
    #[serde(default)]
    monochrome: bool,
    #[serde(default)]
    calibration: Option<aura_recipe::calibrated_look::CalibratedLook>,
    #[serde(default)]
    evidence: Option<gallery_fit::ReferenceEvidence>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyseReferenceInput {
    /// Whose look this is, for the label: a name, an @handle or a profile address. Optional and
    /// never fetched - AURA reads only the folder the photographer chose.
    pub address: String,
    /// A folder of references, or one finished JPEG/PNG/TIFF.
    pub folder: String,
    pub cancel_id: String,
    /// The folder is an Instagram "Download your information" export, so its posted media is
    /// found where Instagram puts it rather than by walking the whole tree.
    #[serde(default)]
    pub export: bool,
    /// Optional unedited version of the same single reference, with identical framing.
    #[serde(default)]
    pub original: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReferenceInput {
    pub photo_id: String,
    pub reference_id: String,
    pub strength: f32,
    /// An edit profile to fit the reference on top of, instead of the plain correction.
    #[serde(default)]
    pub profile_id: Option<String>,
    /// That profile's strength, `0..=1.5`; 1 when absent.
    #[serde(default)]
    pub profile_strength: Option<f32>,
    #[serde(default)]
    pub cancel_id: Option<String>,
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

fn sources(
    input: &AnalyseReferenceInput,
) -> aura_core::AuraResult<(bool, Vec<aura_look::source::ReferenceFile>, String)> {
    let single = Path::new(&input.folder).is_file() && !input.export;
    let (files, origin) = if single {
        let label = input.address.trim();
        let origin = if label.is_empty() {
            "Edited reference".to_string()
        } else {
            aura_core::contract::look::ReferenceOrigin::parse(label)
                .map_or_else(|_| label.to_owned(), |value| value.title())
        };
        (
            vec![aura_look::source::ReferenceFile {
                path: PathBuf::from(&input.folder),
                key: "edited-reference".into(),
            }],
            origin,
        )
    } else {
        let reference = aura_look::source::resolve(
            &input.address,
            if input.export {
                aura_core::contract::look::MediaSource::InstagramExport
            } else {
                aura_core::contract::look::MediaSource::Folder
            },
            Some(Path::new(&input.folder)),
        )?;
        (reference.files, reference.origin.title())
    };
    Ok((single, files, origin))
}

fn decode_reference(
    file: &aura_look::source::ReferenceFile,
    single: bool,
) -> aura_core::AuraResult<aura_raw::codec::Rgb8> {
    if !single
        && file
            .path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("webp"))
    {
        aura_look::source::decode(file)
    } else {
        gallery_fit::decode(&file.path)
    }
}

#[allow(clippy::too_many_lines)]
fn analyse(
    state: &AppState,
    input: &AnalyseReferenceInput,
    cancel: &CancelToken,
) -> aura_core::AuraResult<ReferenceAnalysis> {
    let (single, files, origin) = sources(input)?;
    if input.original.is_some() && (!single || input.export) {
        return Err(refused(
            "An original/edited pair requires one edited reference photo.",
        ));
    }
    let mut calibrated = None;
    let mut calibration_report = None;
    let mut thumbnail = Vec::new();
    let mut dimensions = [0; 2];
    let mut monochrome = true;
    let mut readings = Vec::new();
    let mut palette = [0_usize; 64];
    let mut evidence = gallery_fit::EvidenceBuilder::default();
    for file in &files {
        if cancel.is_cancelled() {
            return Err(refused(
                "Reference analysis stopped. No style was selected.",
            ));
        }
        let image = match decode_reference(file, single) {
            Ok(image) => image,
            Err(error) if single => return Err(error),
            Err(error) => {
                tracing::debug!(code = %error.code, "Reference photo skipped during analysis");
                continue;
            }
        };
        {
            if let Some(original) = &input.original {
                let (model, report) =
                    gallery_fit::calibrate(state, Path::new(original), &file.path, cancel)?;
                calibrated = Some(model);
                calibration_report = Some(report);
            }
            monochrome &= gallery_fit::monochrome(&image);
            if single {
                thumbnail = gallery_fit::thumbnail(&image);
                dimensions = [image.width, image.height];
            }
            let reading = measure::read_at(&file.key, &image, 256);
            evidence.add(&file.key, reading.lighting, &image);
            readings.push(reading);
            let image = aura_raw::thumb::scale_to(&image, 256);
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
    if readings.is_empty() || (!single && readings.len() < MIN_REFERENCES as usize) {
        return Err(refused(format!("Only {} reference photos decoded successfully. Add at least {MIN_REFERENCES} distinct JPEG or PNG photos.", readings.len())));
    }
    if cancel.is_cancelled() {
        return Err(refused(
            "Reference analysis stopped. No style was selected.",
        ));
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
        origin,
        measured: readings.len(),
        skipped: files.len() - readings.len(),
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
        calibration: calibration_report,
    };
    let lighting = aggregate::by_lighting(&readings)
        .into_iter()
        .filter_map(|(bucket, rows)| (rows.len() >= 4).then(|| (bucket, aggregate::fold(&rows))))
        .collect();
    let stored = StoredReference {
        version: 4,
        analysis: analysis.clone(),
        global,
        lighting,
        thumbnail,
        dimensions,
        monochrome,
        calibration: calibrated,
        evidence: evidence.finish(),
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
    let cancel = CancelToken::new();
    if let Some(id) = &input.cancel_id {
        state.register_job(id, cancel.clone());
    }
    let result = apply_with_cancel(state, input, &cancel);
    if let Some(id) = &input.cancel_id {
        state.finish_job(id);
    }
    result
}

fn load_reference(
    state: &AppState,
    input: &ApplyReferenceInput,
) -> aura_core::AuraResult<StoredReference> {
    let bytes = std::fs::read(profile_path(state, &input.reference_id)?).map_err(|_| {
        refused("This reference style is no longer cached. Analyse the references again.")
    })?;
    let mut reference: StoredReference =
        serde_json::from_slice(&bytes).map_err(|e| refused(e.to_string()))?;
    if !matches!(reference.version, 1..=4) {
        return Err(refused(
            "Analyse this reference again with the current version.",
        ));
    }
    if let Some(model) = &reference.calibration {
        model.validate()?;
    }
    let native_pixels = u64::from(reference.dimensions[0]) * u64::from(reference.dimensions[1]);
    if reference.version < 4
        && reference.evidence.is_none()
        && reference.analysis.measured == 1
        && native_pixels >= 256
    {
        // Older single-reference caches retained a colour preview. It supplies bounded
        // migration evidence without requiring access to the original reference file.
        reference.evidence = gallery_fit::ReferenceEvidence::from_preview(&reference.thumbnail);
    }
    if let Some(evidence) = &reference.evidence {
        evidence.validate()?;
    }
    Ok(reference)
}

#[allow(clippy::too_many_lines)]
pub(crate) fn apply_with_cancel(
    state: &AppState,
    input: &ApplyReferenceInput,
    cancel: &CancelToken,
) -> IpcResult<ApplyReferenceReport> {
    if !input.strength.is_finite() || !(0.0..=1.0).contains(&input.strength) {
        return Err(refused("Look strength must be between 0 and 100 percent").into());
    }
    if cancel.is_cancelled() {
        return Err(refused("Reference matching stopped. No edit was saved.").into());
    }
    let reference = load_reference(state, input)?;
    let photo =
        PhotoId::from_db(&input.photo_id).map_err(|_| refused("Invalid photo identifier"))?;
    let current = state.recipe_store().load(&photo)?.ok_or_else(|| {
        refused("Save an initial edit or run Auto edit before matching this reference.")
    })?;
    let hash = aura_recipe::hash::recipe_hash(&current)?;
    let project_key: String = state.catalog().read(|conn| {
        conn.query_row(
            "SELECT project_id FROM photo WHERE photo_id=?1",
            [&input.photo_id],
            |row| row.get(0),
        )
        .map_err(|e| aura_core::errors::db::statement_failed("reference photo", &e))
    })?;
    let project = ProjectId::from_db(&project_key).map_err(|_| refused("Invalid collection"))?;
    let full =
        crate::photo_frames::CatalogFrames::new(state.clone()).frame(&photo, RenderLevel::Full)?;
    let small = gallery_fit::reduced(&full)?;
    let source = gallery_fit::render(
        state,
        &small,
        &crate::develop_commands::neutral_recipe(state, photo),
    )?;
    let chosen = input
        .profile_id
        .as_deref()
        .map(|id| crate::edit_profiles::resolve(state, id))
        .transpose()?
        .flatten();
    let mut baseline = crate::edit_profiles::build(
        &current,
        chosen.as_ref(),
        input.profile_strength.unwrap_or(1.0),
        crate::edit_profiles::AutoCorrection::measure(&source.data)?,
        crate::edit_profiles::SceneStats::measure(&source.data)?,
    )?
    .0;
    // Rebuild every match from its baseline. A calibration belongs to the selected reference,
    // so changing references or strength must replace it rather than compound it.
    baseline.extra.remove(aura_recipe::calibrated_look::KEY);
    baseline.extra.remove(aura_recipe::reference_grade::KEY);
    let rendered = gallery_fit::render(state, &small, &baseline)?;
    let reading = measure::read("reference-baseline", &rendered);
    let target = reference
        .lighting
        .get(&reading.lighting)
        .filter(|_| reading.lighting_confidence >= 0.5)
        .unwrap_or(&reference.global);
    let same_aspect = reference.dimensions[0] > 0
        && reference.dimensions[1] > 0
        && ((f64::from(rendered.width) / f64::from(rendered.height))
            / (f64::from(reference.dimensions[0]) / f64::from(reference.dimensions[1]))
            - 1.0)
            .abs()
            < 0.02;
    let request = gallery_fit::Request {
        full: &full,
        baseline: &baseline,
        current: &current,
        target,
        evidence: reference.evidence.as_ref(),
        lighting: reading.lighting,
        lighting_confidence: reading.lighting_confidence,
        thumbnail: if same_aspect {
            &reference.thumbnail
        } else {
            &[]
        },
        monochrome: reference.monochrome,
        strength: input.strength,
        cancel,
    };
    let fitted = if let Some(model) = &reference.calibration {
        gallery_fit::apply_calibrated(state, &request, model)?
    } else {
        gallery_fit::fit(state, &request)?
    };
    if cancel.is_cancelled() {
        return Err(refused("Reference matching stopped. No edit was saved.").into());
    }
    let mut recipe = fitted.recipe;
    recipe.provenance.source = EditSource::Ai;
    recipe.provenance.confidence = 0.4;
    if !fitted.changed.is_empty() {
        state.recipe_store().save_if_current(
            &project,
            &photo,
            &recipe,
            &fitted.changed,
            &format!(
                "Reference style: {} ({} photos); native measured distance {:.2} -> {:.2}.",
                reference.analysis.origin, reference.analysis.measured, fitted.before, fitted.after
            ),
            &hash,
        )?;
    }
    Ok(ApplyReferenceReport {
        changed: fitted.changed.len(),
        before_distance: fitted.before,
        after_distance: fitted.after,
        protected_fields: current.provenance.user_edited_fields.len(),
    })
}
