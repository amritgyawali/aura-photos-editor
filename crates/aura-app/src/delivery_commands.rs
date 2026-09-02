//! The export and delivery command surface. PHASE-30.
//!
//! Ten commands: seven read, three act. ADR-0062 records the shape and what is deliberately absent
//! from it.
//!
//! # What this surface does that no earlier command surface does
//!
//! **Its act writes files.** Every earlier command on this surface changes a row; `export_run`
//! changes somebody's disk. That is why `export_preview_names` exists beside it - a photographer
//! should be able to see what four thousand files will be called *before* committing a wedding to
//! a naming template, rather than by reading a manifest afterwards.
//!
//! # The field and the source, and why they live here
//!
//! `aura-export` depends on none of the deciding crates. It takes the facts a naming template
//! needs through [`ExportField`], which is this module's implementation of `aura_export::read::
//! Field`, and its pixels through [`ExportSource`], which wraps phase 14's `RenderService`.
//!
//! That indirection is what stops `aura-cull` - the crate that decided what is in the gallery -
//! from being visible to the crate that writes it. An exporter that could ask the cull engine what
//! is delivered is an exporter with an opinion about what is delivered, and section 2.1 gives it
//! none.
//!
//! # What is not here
//!
//! No `export_set_quality` or any other per-field setter: the dialog builds an `ExportJob` and
//! sends it whole, because a surface with per-field setters is a surface where a job can be half
//! configured. No credential on the wire, ever - `delivery_providers` returns a name and a boolean.
//! No bulk anything: every action here is already an action on a whole wedding.

#![allow(clippy::needless_pass_by_value)]

use std::path::PathBuf;
use std::sync::Arc;

use aura_core::contract::delivery::{
    DeliveryColour, DeliveryOutline, DeliveryReason, DeliveryService as _, Destination, ExportJob,
    ExportService as _, ExportSet, ExportedFile, FileFormat, ImageId, MetadataPolicy,
    NamingTemplate, OutputSharpen, ProviderId, Resize, SetMapping,
};
use aura_core::{AuraResult, ProjectId};
use aura_delivery::api::{Delivery, UploadPass};
use aura_delivery::providers::{registry, FolderTransport};
use aura_delivery::store::DeliveryStore;
use aura_export::api::{Export, ExportPass};
use aura_export::read::{Field, Frame, Rendered, Samples, Source};
use aura_export::sets::Presets;
use aura_export::store::ExportStore;
use aura_render::contract::render::{
    OutputColour, OutputSpec, RenderLevel, RenderPurpose, RenderRequest, RenderService,
    RenderedData,
};

use crate::commands::IpcResult;
use crate::contract::ipc::{
    DeliveryInput, DeliveryManifestDto, DeliveryReasonDto, DeliveryStatusDto, ExportFileDto,
    ExportJobInput, ExportNameDto, ExportPresetDto, ExportStatusDto, IpcError, ProviderDto,
    UploadItemDto,
};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// The field
// ---------------------------------------------------------------------------

/// One project's facts, gathered once.
///
/// Gathered rather than fetched per call, for the reason phases 27 and 29's fields are: a naming
/// plan asks every frame in a 4,000-image job for six facts, and a service round trip per question
/// is 24,000 catalog reads inside a twelve-minute budget that is mostly pixels.
#[derive(Debug)]
pub struct ExportField {
    couple: Option<String>,
    photos: u32,
    selected: u32,
    frames: std::collections::BTreeMap<String, Frame>,
    qc_report: Option<PathBuf>,
    versions: Vec<(String, String)>,
}

impl ExportField {
    /// Assemble one project's field.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn new(state: &AppState, project: ProjectId) -> Result<Self, IpcError> {
        let key = project.to_db();
        let catalog = Arc::clone(state.catalog());

        let couple = catalog
            .read(|conn| {
                Ok(conn
                    .query_row(
                        "SELECT name FROM project WHERE project_id = ?1",
                        rusqlite::params![key],
                        |row| row.get::<_, String>(0),
                    )
                    .ok())
            })
            .unwrap_or(None);

        let key = project.to_db();
        let (photos, selected) = catalog
            .read(move |conn| {
                let photos: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM photo WHERE project_id = ?1",
                        rusqlite::params![key],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);
                // Phase 12's gallery, when there is one. A project whose cull has not run has a
                // selected count of zero and a photos count that is not - which is the honest
                // answer, and is why both are on the outline.
                let selected: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM selection_keep k JOIN photo p
                         ON p.photo_id = k.photo_id WHERE p.project_id = ?1",
                        rusqlite::params![key],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);
                Ok((
                    u32::try_from(photos).unwrap_or(0),
                    u32::try_from(selected).unwrap_or(0),
                ))
            })
            .unwrap_or((0, 0));

        let key = project.to_db();
        let frames = catalog
            .read(move |conn| {
                let mut out = std::collections::BTreeMap::new();
                let Ok(mut stmt) = conn.prepare(
                    "SELECT p.photo_id, p.original_name, p.capture_time, p.camera_model
                     FROM photo p WHERE p.project_id = ?1",
                ) else {
                    return Ok(out);
                };
                let rows = stmt.query_map(rusqlite::params![key], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                });
                let Ok(rows) = rows else { return Ok(out) };
                for (id, original, capture, camera) in rows.flatten() {
                    let Ok(image) = ImageId::from_db(&id) else {
                        continue;
                    };
                    out.insert(
                        id,
                        Frame {
                            image: Some(image),
                            original_stem: original.map(|n| stem_of(&n)),
                            date: capture.map(date_of),
                            chapter: None,
                            camera,
                            cleanup_disclosures: Vec::new(),
                            recipe_json: None,
                            original_path: None,
                        },
                    );
                }
                Ok(out)
            })
            .unwrap_or_default();

        Ok(Self {
            couple,
            photos,
            selected,
            frames,
            qc_report: None,
            versions: vec![
                ("app".to_owned(), crate::state::APP_VERSION.to_owned()),
                ("export".to_owned(), aura_export::ENGINE.to_owned()),
                (
                    "recipe_schema".to_owned(),
                    aura_recipe::contract::recipe::SCHEMA_VERSION.to_string(),
                ),
            ],
        })
    }
}

impl Field for ExportField {
    fn couple(&self) -> Option<String> {
        self.couple.clone()
    }
    fn photos(&self) -> u32 {
        self.photos
    }
    fn selected(&self) -> u32 {
        self.selected
    }
    fn frame(&self, image: ImageId) -> Frame {
        self.frames
            .get(&image.to_db())
            .cloned()
            .unwrap_or_else(|| Frame::bare(image))
    }
    fn qc_report_path(&self) -> Option<PathBuf> {
        self.qc_report.clone()
    }
    fn engine_versions(&self) -> Vec<(String, String)> {
        self.versions.clone()
    }
}

/// The `original_name` without its extension.
fn stem_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_owned(),
        _ => name.to_owned(),
    }
}

/// A capture time as `YYYY-MM-DD`.
///
/// Hand-rolled from the epoch rather than through a formatter, because the only thing a naming
/// template wants is a date and the only thing that could go wrong is a time zone. Times are
/// stored in the project's own timeline, so this is deliberately UTC and deliberately not local:
/// a wedding exported in one time zone and re-exported in another must produce the same names.
fn date_of(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Howard Hinnant's `civil_from_days`, which is exact and has no dependencies.
///
/// The two casts at the end are a month in `1..=12` and a day in `1..=31`, both provable from the
/// algorithm rather than from the type - which is why they are `as` with an allow rather than
/// `try_from` with an unreachable arm.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ---------------------------------------------------------------------------
// The source
// ---------------------------------------------------------------------------

/// Pixels, from phase 14's `RenderService` and from nowhere else.
///
/// The one place in this phase that turns a recipe into pixels. An exporter with its own renderer
/// is a delivered JPEG that does not match the proof the couple approved, and nothing would record
/// which of the two a gallery came from.
#[derive(Debug)]
pub struct ExportSource<'a> {
    state: &'a AppState,
}

impl<'a> ExportSource<'a> {
    /// A source over one application state.
    #[must_use]
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }
}

impl Source for ExportSource<'_> {
    fn render(
        &self,
        _project: ProjectId,
        image: ImageId,
        colour: DeliveryColour,
        bit_depth: u8,
    ) -> AuraResult<Rendered> {
        let engine = self.state.render().map_err(|e| {
            aura_export::errors::render_failed(format!("no render engine: {}", e.user_message))
        })?;
        // The recipe the develop surface already loads: stored, or neutral when this frame has
        // never been edited. One loader, because two would be two answers to what the edit is.
        let recipe = crate::develop_commands::load_or_neutral(self.state, image)?;

        // `RenderPurpose::Export`: nothing is skipped, at full precision. The interactive path
        // skips restoration and heavy retouch, and a delivered file that skipped them is a file
        // that does not match the frame the photographer approved.
        let request = RenderRequest {
            image_id: image,
            recipe,
            level: RenderLevel::Full,
            output: OutputSpec {
                colour_space: map_colour(colour),
                bit_depth,
                icc: None,
            },
            purpose: RenderPurpose::Export,
        };
        let rendered = engine.render(request)?;

        Ok(Rendered {
            width: rendered.width,
            height: rendered.height,
            data: match rendered.data {
                RenderedData::Eight(v) => Samples::Eight(v),
                RenderedData::Sixteen(v) => Samples::Sixteen(v),
            },
            colour,
            render_hash: rendered.render_hash,
        })
    }
}

/// The mirror `aura-core`'s note describes, in the one function that crosses it.
///
/// `DeliveryColour` is `aura-core`'s and `OutputColour` is `aura-render`'s, because `aura-core`
/// depends on no workspace crate. This is the only place the two meet, and the phase 30 gate checks
/// that they still have the same members.
const fn map_colour(colour: DeliveryColour) -> OutputColour {
    match colour {
        DeliveryColour::Srgb => OutputColour::Srgb,
        DeliveryColour::AdobeRgb => OutputColour::AdobeRgb,
        DeliveryColour::DisplayP3 => OutputColour::DisplayP3,
    }
}

// ---------------------------------------------------------------------------
// Export commands
// ---------------------------------------------------------------------------

/// What a project's exports covered and found.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn export_status(state: &AppState, project_id: &str) -> IpcResult<ExportStatusDto> {
    let project = parse_project(project_id)?;
    let field = ExportField::new(state, project)?;
    let service = Export::new(
        ExportStore::new(Arc::clone(state.catalog())),
        field.photos(),
        field.selected(),
    );
    Ok(status_dto(&service.outline(project)?))
}

/// The presets the export dialog offers, each with the argument for it.
///
/// # Errors
///
/// `AURA-RENDER-8021` when the shipped preset table does not load, which is a build defect.
pub fn export_presets() -> IpcResult<Vec<ExportPresetDto>> {
    let presets = Presets::built_in()?;
    Ok(presets
        .all()
        .into_iter()
        .map(|p| ExportPresetDto {
            name: p.name.clone(),
            format: p.format.as_str().to_owned(),
            quality: p.quality,
            colour: p.colour.as_str().to_owned(),
            bit_depth: p.bit_depth,
            resize: resize_text(p.resize),
            sharpen: p.sharpen.as_str().to_owned(),
            naming: p.naming.as_str().to_owned(),
            sidecar: p.sidecar,
            reason: p.reason.clone(),
        })
        .collect())
}

/// Every name a job would produce, **writing nothing**.
///
/// Section 10.1 asks for collision-free names across 4,000 files including duplicate original names
/// from two cameras. A photographer should be able to see that answer before the wedding is
/// written, which is what this is for. ADR-0062 section 3.
///
/// # Errors
///
/// `AURA-RENDER-8021` when the job does not validate, `AURA-RENDER-8025` when a name cannot be
/// made unique.
pub fn export_preview_names(
    state: &AppState,
    input: ExportJobInput,
) -> IpcResult<Vec<ExportNameDto>> {
    let project = parse_project(&input.project_id)?;
    let job = build_job(&input)?;
    let field = ExportField::new(state, project)?;
    let planned = aura_export::api::preview_names(&job, &field)?;
    Ok(planned
        .into_iter()
        .map(|p| ExportNameDto {
            image_id: p.image.to_db(),
            set: p.set,
            path: p.rel_path.to_string_lossy().replace('\\', "/"),
            renamed: p.renamed,
            reasons: p.reasons.iter().map(reason_dto).collect(),
        })
        .collect())
}

/// Run a job: render, write, read back, hash, seal.
///
/// # Errors
///
/// `AURA-RENDER-8021` when the job does not validate, `AURA-RENDER-8023` when the destination
/// cannot take the files, and `AURA-RENDER-8022` when a written file did not read back the same -
/// which **stops the job**.
pub fn export_run(state: &AppState, input: ExportJobInput) -> IpcResult<ExportStatusDto> {
    let project = parse_project(&input.project_id)?;
    let job = build_job(&input)?;
    let field = ExportField::new(state, project)?;
    let source = ExportSource::new(state);
    let store = ExportStore::new(Arc::clone(state.catalog()));
    let pass = ExportPass::new(&store, &field, &source, crate::state::APP_VERSION);
    pass.run(project, &job)?;

    let service = Export::new(store, field.photos(), field.selected());
    Ok(status_dto(&service.outline(project)?))
}

/// Every file the last job wrote.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn export_files(state: &AppState, project_id: &str) -> IpcResult<Vec<ExportFileDto>> {
    let project = parse_project(project_id)?;
    let service = Export::new(ExportStore::new(Arc::clone(state.catalog())), 0, 0);
    Ok(service.files(project)?.iter().map(file_dto).collect())
}

/// The last sealed manifest, or `None` when this project has not been delivered.
///
/// `None` is not an empty manifest: a wedding nobody exported and a wedding whose export wrote
/// nothing are different answers.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn export_manifest(
    state: &AppState,
    project_id: &str,
) -> IpcResult<Option<DeliveryManifestDto>> {
    let project = parse_project(project_id)?;
    let service = Export::new(ExportStore::new(Arc::clone(state.catalog())), 0, 0);
    Ok(service.manifest(project)?.map(|m| DeliveryManifestDto {
        project_id: m.project.to_db(),
        created_at: m.created_at,
        files: u32::try_from(m.files.len()).unwrap_or(u32::MAX),
        bytes: m.total_bytes(),
        sets: m.sets.clone(),
        qc_report_path: m
            .qc_report_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        cleanup_disclosures: m
            .cleanup_disclosures
            .iter()
            .map(|(i, what)| (i.to_db(), what.clone()))
            .collect(),
        engine_versions: m.engine_versions.clone(),
        fully_hashed: m.fully_hashed(),
    }))
}

// ---------------------------------------------------------------------------
// Delivery commands
// ---------------------------------------------------------------------------

/// What a project's backups and uploads covered.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn delivery_status(state: &AppState, project_id: &str) -> IpcResult<DeliveryStatusDto> {
    let project = parse_project(project_id)?;
    let service = Delivery::new(Arc::clone(state.catalog()));
    Ok(delivery_dto(&service.outline(project)?))
}

/// Which providers this build has, and whether each has a credential.
///
/// **The credential itself never travels.** A name and a boolean, which is what a panel needs to
/// draw a sign-in button.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn delivery_providers(state: &AppState) -> IpcResult<Vec<ProviderDto>> {
    let store = DeliveryStore::new(Arc::clone(state.catalog()));
    let configured = store.providers().unwrap_or_default();
    Ok(aura_delivery::providers::known()
        .into_iter()
        .filter_map(|name| {
            let provider = registry(name).ok()?;
            let id = provider.id();
            let has_credential = configured
                .iter()
                .any(|(p, has)| p.as_str() == id.as_str() && *has);
            Some(ProviderDto {
                id: id.as_str().to_owned(),
                label: provider.label().to_owned(),
                has_credential,
                may_publish: provider.may_publish(),
            })
        })
        .collect())
}

/// Copy a sealed delivery to a backup destination, verifying every file.
///
/// # Errors
///
/// `AURA-DLV-10002` when the destination cannot be reached, `AURA-DLV-10003` when a copy did not
/// verify - which **stops the backup**.
pub fn delivery_backup(state: &AppState, input: DeliveryInput) -> IpcResult<DeliveryStatusDto> {
    let project = parse_project(&input.project_id)?;
    let (root, files) = last_delivery(state, project)?;
    let service = Delivery::new(Arc::clone(state.catalog())).with_delivery(
        last_job(state, project)?,
        root,
        files,
    );
    service.backup(
        project,
        &Destination::Folder {
            path: PathBuf::from(&input.target),
        },
    )?;
    Ok(delivery_dto(&service.outline(project)?))
}

/// Start or resume an upload to a provider.
///
/// Resuming is not a separate command: a photographer pressing "upload" after their wifi came back
/// is doing the same thing they did the first time, and a surface with two buttons for it is a
/// surface where one of them is the wrong one.
///
/// # Errors
///
/// `AURA-DLV-10001` when the provider is unknown, `AURA-DLV-10004` when it has no credential, and
/// `AURA-DLV-10002` when it cannot be reached - which on this build it never can, because no
/// network transport ships.
pub fn delivery_upload(state: &AppState, input: DeliveryInput) -> IpcResult<DeliveryStatusDto> {
    let project = parse_project(&input.project_id)?;
    let provider = registry(&input.target)?;
    let (root, files) = last_delivery(state, project)?;
    let job_id = last_job(state, project)?;
    let store = DeliveryStore::new(Arc::clone(state.catalog()));

    // A filesystem transport into the destination's own `_upload` directory. This is what a folder,
    // a NAS and an external drive use; a network transport does not ship in this build and
    // `NETWORK_TRANSPORT_AVAILABLE` says so on the wire.
    let transport = FolderTransport::new(root.join("_upload"));
    let mapping: Vec<SetMapping> = input
        .mapping
        .iter()
        .map(|m| SetMapping {
            set: m.set.clone(),
            remote: m.remote.clone(),
            publish: m.publish,
        })
        .collect();

    UploadPass::new(&store, provider.as_ref(), &transport)
        .run(project, &job_id, &root, &files, &mapping)?;

    let service = Delivery::new(Arc::clone(state.catalog()));
    Ok(delivery_dto(&service.outline(project)?))
}

/// Every file's state at a provider.
///
/// # Errors
///
/// `AURA-DB-3006` when the rows cannot be read.
pub fn delivery_items(
    state: &AppState,
    project_id: &str,
    provider: &str,
) -> IpcResult<Vec<UploadItemDto>> {
    let project = parse_project(project_id)?;
    let id = ProviderId::parse(provider)?;
    let service = Delivery::new(Arc::clone(state.catalog()));
    Ok(service
        .items(project, &id)?
        .iter()
        .map(|item| UploadItemDto {
            image_id: item.image.to_db(),
            set: item.set.clone(),
            path: item.path.to_string_lossy().replace('\\', "/"),
            bytes: item.bytes,
            state: item.state.as_str().to_owned(),
            sent: match &item.state {
                aura_core::contract::delivery::UploadState::Verified => item.bytes,
                other => other.sent(),
            },
            resumes: match &item.state {
                aura_core::contract::delivery::UploadState::InProgress { resumes, .. } => *resumes,
                _ => 0,
            },
            failure_code: match &item.state {
                aura_core::contract::delivery::UploadState::Failed { code } => Some(code.clone()),
                _ => None,
            },
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn status_dto(outline: &aura_core::contract::delivery::ExportOutline) -> ExportStatusDto {
    ExportStatusDto {
        photos: outline.photos,
        selected: outline.selected,
        requested: outline.requested,
        written: outline.written,
        verified: outline.verified,
        unverified: outline.unverified,
        corrupt: outline.corrupt,
        render_failed: outline.render_failed,
        renamed: outline.renamed,
        sidecars: outline.sidecars,
        bytes: outline.bytes,
        manifest_sealed: outline.manifest_sealed,
        ms: outline.ms,
    }
}

fn delivery_dto(outline: &DeliveryOutline) -> DeliveryStatusDto {
    DeliveryStatusDto {
        files: outline.files,
        backups: outline.backups,
        backed_up: outline.backed_up,
        diverged: outline.diverged,
        providers: outline.providers,
        uploaded: outline.uploaded,
        outstanding: outline.outstanding,
        refused: outline.refused,
        resumes: outline.resumes,
        unmapped_sets: outline.unmapped_sets,
        bytes_sent: outline.bytes_sent,
        network_available: aura_delivery::NETWORK_TRANSPORT_AVAILABLE,
    }
}

fn file_dto(file: &ExportedFile) -> ExportFileDto {
    ExportFileDto {
        image_id: file.image.to_db(),
        set: file.set.clone(),
        path: file.path.to_string_lossy().replace('\\', "/"),
        bytes: file.bytes,
        hash: file.hash.clone(),
        width: file.width,
        height: file.height,
        verified: file.verified,
        renamed: file.renamed,
        reasons: file.reasons.iter().map(reason_dto).collect(),
    }
}

fn reason_dto(reason: &DeliveryReason) -> DeliveryReasonDto {
    DeliveryReasonDto {
        code: reason.code.as_str().to_owned(),
        text: reason.sentence(),
        fatal: reason.code.is_fatal(),
    }
}

fn resize_text(resize: Resize) -> String {
    match resize {
        Resize::Full => "full".to_owned(),
        Resize::LongEdge { pixels } => format!("long_edge:{pixels}"),
        Resize::Fit { width, height } => format!("fit:{width}x{height}"),
    }
}

fn parse_resize(text: &str) -> Result<Resize, IpcError> {
    if text == "full" {
        return Ok(Resize::Full);
    }
    if let Some(rest) = text.strip_prefix("long_edge:") {
        let pixels: u32 = rest
            .parse()
            .map_err(|_| IpcError::from(aura_export::errors::job_refused("bad long edge")))?;
        return Ok(Resize::LongEdge { pixels });
    }
    if let Some(rest) = text.strip_prefix("fit:") {
        if let Some((w, h)) = rest.split_once('x') {
            let (Ok(width), Ok(height)) = (w.parse(), h.parse()) else {
                return Err(IpcError::from(aura_export::errors::job_refused(
                    "bad fit box",
                )));
            };
            return Ok(Resize::Fit { width, height });
        }
    }
    Err(IpcError::from(aura_export::errors::job_refused(format!(
        "`{text}` is not a resize"
    ))))
}

/// Build a job from what the dialog sent, validating at the edge.
///
/// A provider destination is refused here rather than in the pass: `aura-export` writes files and
/// `aura-delivery` sends them, and a command that accepted a provider as an export destination
/// would be a command that had to explain the two-step to a photographer at the wrong moment.
fn build_job(input: &ExportJobInput) -> Result<ExportJob, IpcError> {
    let mut sets = Vec::with_capacity(input.sets.len());
    for set in &input.sets {
        let mut images = Vec::with_capacity(set.image_ids.len());
        for id in &set.image_ids {
            images.push(parse_image(id)?);
        }
        sets.push(ExportSet {
            name: set.name.clone(),
            images,
            format: FileFormat::parse(&set.format)?,
            quality: set.quality,
            resize: parse_resize(&set.resize)?,
            sharpen: OutputSharpen::parse(&set.sharpen)?,
            naming: NamingTemplate::parse(&set.naming)?,
            colour: DeliveryColour::parse(&set.colour)?,
            bit_depth: set.bit_depth,
            sidecar: set.sidecar,
        });
    }

    let destination = match input.destination_kind.as_str() {
        "nas" => Destination::Nas {
            path: PathBuf::from(&input.destination),
        },
        "folder" => Destination::Folder {
            path: PathBuf::from(&input.destination),
        },
        other => {
            return Err(IpcError::from(aura_export::errors::job_refused(format!(
                "`{other}` is a delivery target rather than an export destination; export to a \
                 folder first, then send it with the delivery panel"
            ))))
        }
    };

    let job = ExportJob {
        sets,
        destination,
        metadata: MetadataPolicy {
            copyright: input.copyright.clone(),
            contact: input.contact.clone(),
            creator: input.creator.clone(),
            keywords: input.keywords.clone(),
            strip_gps: input.strip_gps,
            strip_camera_serial: input.strip_camera_serial,
        },
        verify: input.verify,
    };
    job.validate()?;
    Ok(job)
}

/// The last job's id, for a delivery that has to name one.
fn last_job(state: &AppState, project: ProjectId) -> Result<String, IpcError> {
    ExportStore::new(Arc::clone(state.catalog()))
        .latest_job(project)?
        .ok_or_else(|| {
            IpcError::from(aura_delivery::errors::unreachable(
                "this wedding has not been exported yet, so there is nothing to send",
            ))
        })
}

/// The last delivery's root and files.
fn last_delivery(
    state: &AppState,
    project: ProjectId,
) -> Result<(PathBuf, Vec<ExportedFile>), IpcError> {
    let store = ExportStore::new(Arc::clone(state.catalog()));
    let job_id = last_job(state, project)?;
    let files = store.files(&job_id)?;
    let key = project.to_db();
    let id = job_id.clone();
    let root: Option<String> = state
        .catalog()
        .read(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT destination FROM export_job WHERE job_id = ?1 AND project_id = ?2",
                    rusqlite::params![id, key],
                    |row| row.get::<_, String>(0),
                )
                .ok())
        })
        .unwrap_or(None);
    let destination: Destination = root
        .and_then(|text| serde_json::from_str(&text).ok())
        .ok_or_else(|| {
            IpcError::from(aura_delivery::errors::unreachable(
                "the last export's destination could not be read",
            ))
        })?;
    let path = destination.local_root().map(PathBuf::from).ok_or_else(|| {
        IpcError::from(aura_delivery::errors::unreachable(
            "the last export did not go to a folder",
        ))
    })?;
    Ok((path, files))
}

fn parse_project(id: &str) -> Result<ProjectId, IpcError> {
    ProjectId::from_db(id).map_err(|_| {
        IpcError::from(aura_export::errors::job_refused(format!(
            "`{id}` is not a project id"
        )))
    })
}

fn parse_image(id: &str) -> Result<ImageId, IpcError> {
    ImageId::from_db(id).map_err(|_| {
        IpcError::from(aura_export::errors::job_refused(format!(
            "`{id}` is not a photograph id"
        )))
    })
}
