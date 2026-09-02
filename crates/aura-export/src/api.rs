//! The frozen [`ExportService`], and the pass that runs a job.
//!
//! ## The order of operations, and why every step is where it is
//!
//! 1. **Validate the job.** Before a frame is rendered, so a job that cannot succeed costs nothing.
//! 2. **Check the destination.** A read-only volume, a path that is a file and a directory that
//!    does not exist all look identical from inside the loop, and all three are things to say
//!    before rendering a wedding rather than on its four-hundredth frame.
//! 3. **Plan every name.** The whole job at once, collisions resolved, before anything is written.
//! 4. **For each file: render, resize, sharpen, encode, write, read back, hash.** In that order.
//!    The resize is in linear light and the sharpening is not - `resample`'s note says why.
//! 5. **Seal the manifest.** Only when every file was written; a cancelled or failed job leaves the
//!    files on disk and seals nothing, because a partial manifest is a document that says a wedding
//!    was delivered when four chapters of it were not.
//!
//! ## A render failure skips a frame and a verification failure stops everything
//!
//! The two failures look similar from the loop and are opposite in kind. A frame that will not
//! render is one photograph's problem: the wedding delivers without it and the summary names it. A
//! file that did not read back is the **volume's** problem, which means the next three hundred
//! frames are at the same risk. ADR-0061 decision 3.

use std::path::PathBuf;

use aura_core::contract::delivery::{
    DeliveryCode, DeliveryManifest, DeliveryReason, ExportJob, ExportOutline, ExportService,
    ExportedFile, ImageId, Resize,
};
use aura_core::{AuraResult, ProjectId};

use crate::errors::{destination_bad, job_refused};
use crate::manifest;
use crate::metadata;
use crate::naming;
use crate::read::{Field, Rendered, Source};
use crate::resample;
use crate::store::ExportStore;
use crate::verify::{check_destination, write_and_verify};
use crate::{jpeg, png, tiff};

/// One export pass over one project.
#[derive(Debug)]
pub struct ExportPass<'a> {
    store: &'a ExportStore,
    field: &'a dyn Field,
    source: &'a dyn Source,
    app_version: String,
}

/// What a pass produced.
#[derive(Debug, Clone, PartialEq)]
pub struct PassResult {
    /// The job's id.
    pub job_id: String,
    /// The manifest, when the job sealed one.
    pub manifest: Option<DeliveryManifest>,
    /// Every file that was written.
    pub files: Vec<ExportedFile>,
    /// Photographs that could not be rendered, with the reason.
    pub skipped: Vec<(ImageId, String)>,
}

impl<'a> ExportPass<'a> {
    /// A pass over one catalog, one field and one source of pixels.
    #[must_use]
    pub fn new(
        store: &'a ExportStore,
        field: &'a dyn Field,
        source: &'a dyn Source,
        app_version: impl Into<String>,
    ) -> Self {
        Self {
            store,
            field,
            source,
            app_version: app_version.into(),
        }
    }

    /// Run a job.
    ///
    /// # Errors
    ///
    /// `AURA-RENDER-8021` when the job does not validate, `AURA-RENDER-8023` when the destination
    /// cannot take the files, `AURA-RENDER-8022` when a written file did not read back - which
    /// **stops the job**.
    pub fn run(&self, project: ProjectId, job: &ExportJob) -> AuraResult<PassResult> {
        // 1. The job, before a frame is rendered.
        job.validate()?;

        // 2. The destination, before a frame is rendered.
        let Some(root) = job.destination.local_root().map(PathBuf::from) else {
            return Err(job_refused(format!(
                "`{}` is not a destination this crate writes to; use aura-delivery to send a \
                 sealed delivery to a provider",
                job.destination.kind()
            )));
        };
        check_destination(&root)?;

        // 3. Every name, before anything is written.
        let plan = naming::plan(job, self.field)?;

        let versions = self.field.engine_versions();
        let job_id = self
            .store
            .open_job(project, job, &self.app_version, &versions)?;

        let started = std::time::Instant::now(); // DETERMINISM: wall-clock duration for telemetry only
        let mut files: Vec<ExportedFile> = Vec::with_capacity(plan.len());
        let mut skipped: Vec<(ImageId, String)> = Vec::new();
        let mut bytes_total = 0_u64;
        let mut verified_count = 0_u32;
        let mut per_set: std::collections::BTreeMap<String, u32> =
            std::collections::BTreeMap::new();
        let mut sidecars = 0_u32;

        for planned in &plan {
            let Some(set) = job.sets.iter().find(|s| s.name == planned.set) else {
                continue;
            };

            // 4a. Pixels, from the one place they come from.
            let rendered =
                match self
                    .source
                    .render(project, planned.image, set.colour, set.bit_depth)
                {
                    Ok(r) if r.is_well_formed() => r,
                    Ok(_) => {
                        skipped.push((
                            planned.image,
                            "the render returned a malformed buffer".to_owned(),
                        ));
                        continue;
                    }
                    Err(e) => {
                        // Item-level. The wedding delivers without this frame and the summary names it.
                        skipped.push((planned.image, e.to_string()));
                        continue;
                    }
                };

            let mut reasons = planned.reasons.clone();

            // 4b. Resize, in linear light, never upward.
            let (tw, th) = set.resize.target(rendered.width, rendered.height);
            if set.resize.would_upscale(rendered.width, rendered.height) {
                reasons.push(DeliveryReason::with(
                    DeliveryCode::ResizeIgnoredUpscale,
                    format!("{}x{}", rendered.width, rendered.height),
                ));
            }
            let resized: Rendered = if tw == rendered.width && th == rendered.height {
                rendered
            } else {
                resample::downscale(&rendered, tw, th)
            };

            // 4c. Sharpen, in the encoded domain, by how far the frame was scaled.
            let scale = if resized.width.max(resized.height) == 0 {
                1.0
            } else {
                f64::from(resized.width.max(resized.height)) as f32
                    / f64::from(tw.max(th).max(1)) as f32
            };
            let long_before = f64::from(set.resize.target(resized.width, resized.height).0);
            let _ = long_before;
            let scale_factor = if matches!(set.resize, Resize::Full) {
                1.0
            } else {
                scale
            };
            let sharpened = resample::sharpen(&resized, set.sharpen, scale_factor);
            if set.sharpen != aura_core::contract::delivery::OutputSharpen::None {
                reasons.push(DeliveryReason::with(
                    DeliveryCode::SharpenedForOutput,
                    format!("{}x{}", sharpened.width, sharpened.height),
                ));
            }

            // 4d. Encode.
            let (bytes, mut encode_reasons) = match set.format {
                aura_core::contract::delivery::FileFormat::Jpeg => {
                    jpeg::encode(&sharpened, set.quality, &job.metadata)?
                }
                aura_core::contract::delivery::FileFormat::Tiff => {
                    tiff::encode(&sharpened, &job.metadata)?
                }
                aura_core::contract::delivery::FileFormat::Png => {
                    png::encode(&sharpened, &job.metadata)?
                }
            };
            reasons.append(&mut encode_reasons);

            // 4e. Write, read back, hash. A failure here stops the job.
            let path = root.join(&planned.rel_path);
            let written = write_and_verify(&path, &bytes, job.verify)?;
            reasons.push(DeliveryReason::plain(if written.verified {
                DeliveryCode::WrittenAndVerified
            } else {
                DeliveryCode::WrittenUnverified
            }));
            if written.verified {
                verified_count += 1;
            }
            bytes_total = bytes_total.saturating_add(written.bytes);

            // 4f. The sidecar, when the set asked for one.
            let frame = self.field.frame(planned.image);
            if set.sidecar {
                if let Some(recipe) = &frame.recipe_json {
                    let sidecar_path =
                        path.with_extension(aura_core::contract::delivery::SIDECAR_EXT);
                    write_and_verify(&sidecar_path, recipe.as_bytes(), job.verify)?;
                    reasons.push(DeliveryReason::plain(DeliveryCode::SidecarWritten));
                    sidecars += 1;
                }
            }

            // 4g. Phase 24's disclosures travel to the manifest.
            for what in &frame.cleanup_disclosures {
                reasons.push(DeliveryReason::with(
                    DeliveryCode::CleanupDisclosed,
                    what.clone(),
                ));
            }

            reasons.truncate(aura_core::contract::delivery::MAX_REASONS);

            let file = ExportedFile {
                image: planned.image,
                set: planned.set.clone(),
                path: planned.rel_path.clone(),
                bytes: written.bytes,
                hash: written.hash,
                width: sharpened.width,
                height: sharpened.height,
                render_hash: sharpened.render_hash,
                verified: written.verified,
                renamed: planned.renamed,
                reasons,
            };
            self.store.write_file(&job_id, &file)?;
            *per_set.entry(planned.set.clone()).or_insert(0) += 1;
            files.push(file);
        }

        for (name, count) in &per_set {
            self.store.set_written(&job_id, name, *count)?;
        }

        let written = u32::try_from(files.len()).unwrap_or(u32::MAX);
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let _ = sidecars;

        // 5. Seal only when nothing was skipped. A partial manifest is a document that says a
        // wedding was delivered when four chapters of it were not.
        if skipped.is_empty() && !files.is_empty() {
            let sets: Vec<(String, u32)> = per_set.into_iter().collect();
            let manifest = manifest::assemble(
                project,
                self.field
                    .engine_versions()
                    .iter()
                    .find(|(k, _)| k == "created_at")
                    .and_then(|(_, v)| v.parse().ok())
                    .unwrap_or(0),
                &files,
                &sets,
                self.field.qc_report_path(),
                versions,
            );
            let (path, hash) = manifest::seal(&root, &manifest, job.verify)?;
            self.store.close_job(
                &job_id,
                project,
                "sealed",
                written,
                verified_count,
                bytes_total,
                ms,
                Some((&manifest, &path.to_string_lossy(), &hash)),
            )?;
            return Ok(PassResult {
                job_id,
                manifest: Some(manifest),
                files,
                skipped,
            });
        }

        self.store.close_job(
            &job_id,
            project,
            if files.is_empty() { "failed" } else { "sealed" },
            written,
            verified_count,
            bytes_total,
            ms,
            None,
        )?;
        Ok(PassResult {
            job_id,
            manifest: None,
            files,
            skipped,
        })
    }
}

/// The frozen [`ExportService`], over one catalog.
#[derive(Debug, Clone)]
pub struct Export {
    store: ExportStore,
    photos: u32,
    selected: u32,
}

impl Export {
    /// Wrap a store.
    ///
    /// `photos` and `selected` are the outline's two wider denominators and are supplied rather
    /// than queried, because a service that counted the gallery itself would be a service with an
    /// opinion about what is in it - which is `aura-cull`'s.
    #[must_use]
    pub fn new(store: ExportStore, photos: u32, selected: u32) -> Self {
        Self {
            store,
            photos,
            selected,
        }
    }
}

impl ExportService for Export {
    fn outline(&self, project: ProjectId) -> AuraResult<ExportOutline> {
        self.store.outline(project, self.photos, self.selected)
    }

    fn run(&self, _project: ProjectId, _job: &ExportJob) -> AuraResult<DeliveryManifest> {
        // The service reads; the pass writes. `ExportPass::run` needs a `Field` and a `Source`,
        // which `aura-app` assembles out of the frozen services and hands to the command - and a
        // service that could render would be a service that needs a renderer to answer "what did
        // this project export", which is the shape phase 27 rejected for the same reason.
        Err(job_refused(
            "an export runs through ExportPass, which the command surface assembles; \
             ExportService reads what a pass produced",
        ))
    }

    fn files(&self, project: ProjectId) -> AuraResult<Vec<ExportedFile>> {
        match self.store.latest_job(project)? {
            Some(job) => self.store.files(&job),
            None => Ok(Vec::new()),
        }
    }

    fn manifest(&self, project: ProjectId) -> AuraResult<Option<DeliveryManifest>> {
        self.store.manifest(project)
    }

    fn preview_names(
        &self,
        _project: ProjectId,
        _job: &ExportJob,
    ) -> AuraResult<Vec<(ImageId, PathBuf)>> {
        Err(job_refused(
            "a name preview runs through naming::plan, which needs the project's field",
        ))
    }
}

/// The dry run behind `export_preview_names`: every name a job would produce, writing nothing.
///
/// A free function rather than a method, because it needs a `Field` and nothing else - no store, no
/// renderer, no destination. Section 10.1 asks for collision-free names across 4,000 files, and a
/// photographer should be able to see the answer before the wedding is written.
///
/// # Errors
///
/// `AURA-RENDER-8021` when the job does not validate, `AURA-RENDER-8025` when a name cannot be
/// made unique.
pub fn preview_names(job: &ExportJob, field: &dyn Field) -> AuraResult<Vec<naming::PlannedName>> {
    job.validate()?;
    naming::plan(job, field)
}

/// Whether a destination is one this crate can write to at all.
///
/// # Errors
///
/// `AURA-RENDER-8023` when it is not.
pub fn writable_root(job: &ExportJob) -> AuraResult<PathBuf> {
    job.destination
        .local_root()
        .map(PathBuf::from)
        .ok_or_else(|| {
            destination_bad(format!(
                "`{}` is a delivery target rather than an export destination",
                job.destination.kind()
            ))
        })
}

/// The Exif block a delivered file would carry, for the export dialog's preview.
#[must_use]
pub fn metadata_preview(job: &ExportJob) -> Vec<DeliveryReason> {
    metadata::build(&job.metadata, 0, 0, true).reasons
}
