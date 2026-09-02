//! Migration 30's export half: the job, its sets, its files, its reasons and its manifest.
//!
//! ## A row here is the only record that the thing happened
//!
//! Every store from phase 05 to 29 caches a decision that can be recomputed from the catalog. This
//! one records a JPEG on somebody's external drive. Re-running the pass does not re-derive it; it
//! writes it again, which is a different operation with a different cost and a different risk.
//!
//! That is why the manifest is sealed once and never updated - `delivery_manifest_no_update`
//! aborts every UPDATE - and why a correction is a new job rather than an edit to an old one.
//!
//! ## Why the reasons live in one table with a discriminator
//!
//! Phase 29's shape and the same argument. A reason is a code plus an optional measured half in
//! every one of the four cases, and four tables would be four places to forget the `MAX_REASONS`
//! bound. The discriminator is checked in the schema.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::delivery::{
    DeliveryCode, DeliveryColour, DeliveryManifest, DeliveryReason, Destination, ExportJob,
    ExportOutline, ExportSet, ExportedFile, FileFormat, ImageId, MetadataPolicy, NamingTemplate,
    OutputSharpen, Resize, MAX_REASONS,
};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, ProjectId};
use rusqlite::{params, OptionalExtension};

/// One set's row, flattened for the writer thread.
///
/// A named struct rather than a ten-tuple, because the writer closure has to own its arguments and
/// a ten-tuple at the boundary is where a quality and a bit depth get swapped - both are `u8`.
struct SetRow {
    name: String,
    format: String,
    quality: u8,
    colour: String,
    bit_depth: u8,
    resize: String,
    sharpen: String,
    naming: String,
    sidecar: bool,
    requested: u32,
}

/// One set's stored shape, as it comes back off the row.
struct SetSpecRow {
    name: String,
    format: String,
    quality: u8,
    colour: String,
    bit_depth: u8,
    resize: String,
    sharpen: String,
    naming: String,
    sidecar: bool,
}

/// One set of a job, without its photographs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetSpec {
    /// What the set is called.
    pub name: String,
    /// What kind of file.
    pub format: FileFormat,
    /// JPEG quality.
    pub quality: u8,
    /// How large.
    pub resize: Resize,
    /// Output sharpening.
    pub sharpen: OutputSharpen,
    /// How the files are named.
    pub naming: NamingTemplate,
    /// The output colour space.
    pub colour: DeliveryColour,
    /// Bits per sample.
    pub bit_depth: u8,
    /// Whether an XMP sidecar goes beside each file.
    pub sidecar: bool,
}

impl SetSpec {
    /// Turn the specification into a set over these photographs.
    #[must_use]
    pub fn over(&self, images: Vec<ImageId>) -> ExportSet {
        ExportSet {
            name: self.name.clone(),
            images,
            format: self.format,
            quality: self.quality,
            resize: self.resize,
            sharpen: self.sharpen,
            naming: self.naming.clone(),
            colour: self.colour,
            bit_depth: self.bit_depth,
            sidecar: self.sidecar,
        }
    }
}

/// A job's shape without its photographs: everything a repeat needs except *what* to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSpec {
    /// Where the files went last time.
    pub destination: Destination,
    /// What metadata travelled with them.
    pub metadata: MetadataPolicy,
    /// Whether the job read every file back.
    pub verify: bool,
    /// The sets, in name order.
    pub sets: Vec<SetSpec>,
}

impl JobSpec {
    /// Turn the specification into a job over these photographs.
    ///
    /// Every set gets the same list, which is what a repeat of a single-set gallery job means.
    /// A multi-set job repeated this way writes the same gallery into each of its sets at that
    /// set's own size and quality, which is what those sets are for.
    #[must_use]
    pub fn over(&self, images: &[ImageId]) -> ExportJob {
        ExportJob {
            sets: self.sets.iter().map(|s| s.over(images.to_vec())).collect(),
            destination: self.destination.clone(),
            metadata: self.metadata.clone(),
            verify: self.verify,
        }
    }
}

/// Read back the text [`resize_text`] wrote.
fn parse_resize(text: &str) -> Option<Resize> {
    if text == "full" {
        return Some(Resize::Full);
    }
    if let Some(rest) = text.strip_prefix("long_edge:") {
        return rest
            .parse::<u32>()
            .ok()
            .map(|pixels| Resize::LongEdge { pixels });
    }
    if let Some(rest) = text.strip_prefix("fit:") {
        let (w, h) = rest.split_once('x')?;
        return Some(Resize::Fit {
            width: w.parse().ok()?,
            height: h.parse().ok()?,
        });
    }
    None
}

/// One catalog, wrapped.
#[derive(Debug, Clone)]
pub struct ExportStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl ExportStore {
    /// Wrap a catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>) -> Self {
        let clock = Arc::clone(catalog.clock());
        Self { catalog, clock }
    }

    /// Open a job and return its id.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3002` when the rows cannot be written.
    pub fn open_job(
        &self,
        project: ProjectId,
        job: &ExportJob,
        app_version: &str,
        engine_versions: &[(String, String)],
    ) -> AuraResult<String> {
        let job_id = format!("exp_{}", uuid_like(&self.clock));
        let started = aura_catalog::rfc3339(self.clock.now_utc());
        let destination = destination_json(&job.destination);
        let kind = job.destination.kind().to_owned();
        let policy = serde_json::to_string(&job.metadata).unwrap_or_else(|_| "{}".to_owned());
        let versions = serde_json::to_string(engine_versions).unwrap_or_else(|_| "[]".to_owned());
        let verify = i64::from(job.verify);
        let project_db = project.to_db();
        let app = app_version.to_owned();
        let id = job_id.clone();

        let sets: Vec<SetRow> = job
            .sets
            .iter()
            .map(|set| SetRow {
                name: set.name.clone(),
                format: set.format.as_str().to_owned(),
                quality: set.quality,
                colour: set.colour.as_str().to_owned(),
                bit_depth: set.bit_depth,
                resize: resize_text(set.resize),
                sharpen: set.sharpen.as_str().to_owned(),
                naming: set.naming.as_str().to_owned(),
                sidecar: set.sidecar,
                requested: u32::try_from(set.images.len()).unwrap_or(u32::MAX),
            })
            .collect();

        self.catalog.writer().with(move |conn| {
            let tx = conn
                .transaction()
                .map_err(|e| statement_failed("export_job", &e))?;
            tx.execute(
                "INSERT INTO export_job (job_id, project_id, destination_kind, destination,
                     metadata_policy, verify, status, started_at, engine_versions, app_version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running', ?7, ?8, ?9)",
                params![
                    id,
                    project_db,
                    kind,
                    destination,
                    policy,
                    verify,
                    started,
                    versions,
                    app
                ],
            )
            .map_err(|e| statement_failed("export_job", &e))?;
            for row in sets {
                tx.execute(
                    "INSERT INTO export_set (job_id, name, format, quality, colour_space,
                         bit_depth, resize, sharpen, naming, sidecar, requested)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        id,
                        row.name,
                        row.format,
                        row.quality,
                        row.colour,
                        row.bit_depth,
                        row.resize,
                        row.sharpen,
                        row.naming,
                        i64::from(row.sidecar),
                        row.requested
                    ],
                )
                .map_err(|e| statement_failed("export_set", &e))?;
            }
            tx.commit()
                .map_err(|e| statement_failed("export_job", &e))?;
            Ok(())
        })?;

        Ok(job_id)
    }

    /// Record one written file, with its reasons.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3002` when the row cannot be written - including when it claims to be verified
    /// with no digest, which `export_file_verified_needs_a_hash` refuses.
    pub fn write_file(&self, job_id: &str, file: &ExportedFile) -> AuraResult<()> {
        let at = aura_catalog::rfc3339(self.clock.now_utc());
        let id = job_id.to_owned();
        let rel = file.path.to_string_lossy().replace('\\', "/");
        let f = file.clone();
        let reasons: Vec<(String, Option<String>)> = f
            .reasons
            .iter()
            .take(MAX_REASONS)
            .map(|r| (r.code.as_str().to_owned(), r.detail.clone()))
            .collect();
        let subject_key = format!("{id}|{rel}");

        self.catalog.writer().with(move |conn| {
            let tx = conn
                .transaction()
                .map_err(|e| statement_failed("export_file", &e))?;
            tx.execute(
                "INSERT OR REPLACE INTO export_file (job_id, set_name, photo_id, rel_path, bytes,
                     hash, width, height, render_hash, verified, renamed, written_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    id,
                    f.set,
                    f.image.to_db(),
                    rel,
                    f.bytes,
                    f.hash,
                    f.width,
                    f.height,
                    f.render_hash,
                    i64::from(f.verified),
                    i64::from(f.renamed),
                    at
                ],
            )
            .map_err(|e| statement_failed("export_file", &e))?;
            tx.execute(
                "DELETE FROM export_reason WHERE subject_kind = 'file' AND subject_key = ?1",
                params![subject_key],
            )
            .map_err(|e| statement_failed("export_reason", &e))?;
            for (ix, (code, detail)) in reasons.iter().enumerate() {
                tx.execute(
                    "INSERT INTO export_reason (subject_kind, subject_key, ix, code, detail)
                     VALUES ('file', ?1, ?2, ?3, ?4)",
                    params![subject_key, ix as i64, code, detail],
                )
                .map_err(|e| statement_failed("export_reason", &e))?;
            }
            tx.commit()
                .map_err(|e| statement_failed("export_file", &e))?;
            Ok(())
        })
    }

    /// Close a job, and seal its manifest when it produced one.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3002` when the rows cannot be written.
    #[allow(clippy::too_many_arguments)]
    pub fn close_job(
        &self,
        job_id: &str,
        project: ProjectId,
        status: &str,
        written: u32,
        verified: u32,
        bytes: u64,
        ms: u64,
        sealed: Option<(&DeliveryManifest, &str, &str)>,
    ) -> AuraResult<()> {
        let at = aura_catalog::rfc3339(self.clock.now_utc());
        let id = job_id.to_owned();
        let st = status.to_owned();
        let project_db = project.to_db();
        let manifest = sealed.map(|(m, path, hash)| {
            (
                m.created_at,
                u32::try_from(m.files.len()).unwrap_or(u32::MAX),
                m.total_bytes(),
                m.qc_report_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                serde_json::to_string(&m.cleanup_disclosures).unwrap_or_else(|_| "[]".to_owned()),
                path.to_owned(),
                hash.to_owned(),
            )
        });

        self.catalog.writer().with(move |conn| {
            let tx = conn
                .transaction()
                .map_err(|e| statement_failed("export_job", &e))?;
            tx.execute(
                "UPDATE export_job SET status = ?2, files_written = ?3, files_verified = ?4,
                     bytes_written = ?5, ms = ?6, finished_at = ?7 WHERE job_id = ?1",
                params![id, st, written, verified, bytes as i64, ms as i64, at],
            )
            .map_err(|e| statement_failed("export_job", &e))?;
            if let Some((created_at, files, total, qc, disclosures, path, hash)) = manifest {
                tx.execute(
                    "INSERT INTO delivery_manifest (job_id, project_id, created_at, files, bytes,
                         qc_report_path, cleanup_disclosures, manifest_path, manifest_hash)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        id,
                        project_db,
                        created_at,
                        files,
                        total as i64,
                        qc,
                        disclosures,
                        path,
                        hash
                    ],
                )
                .map_err(|e| statement_failed("delivery_manifest", &e))?;
            }
            tx.commit()
                .map_err(|e| statement_failed("export_job", &e))?;
            Ok(())
        })
    }

    /// Update a set's written count.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3002` when the row cannot be written.
    pub fn set_written(&self, job_id: &str, set: &str, written: u32) -> AuraResult<()> {
        let id = job_id.to_owned();
        let name = set.to_owned();
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "UPDATE export_set SET written = ?3 WHERE job_id = ?1 AND name = ?2",
                params![id, name, written],
            )
            .map_err(|e| statement_failed("export_set", &e))?;
            Ok(())
        })
    }

    /// The id of the project's most recent job, or `None`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read.
    pub fn latest_job(&self, project: ProjectId) -> AuraResult<Option<String>> {
        let key = project.to_db();
        self.catalog.read(|conn| {
            conn.query_row(
                "SELECT job_id FROM export_job WHERE project_id = ?1
                 ORDER BY started_at DESC, job_id DESC LIMIT 1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| statement_failed("export_job", &e))
        })
    }

    /// The shape of the project's most recent job, without its images.
    ///
    /// Every field of an [`ExportJob`] except the photographs, read back out of the rows the job
    /// itself wrote: the destination, the metadata policy, whether it verified, and each set's
    /// format, quality, colour, depth, resize, sharpening, template and sidecar flag.
    ///
    /// **The images are deliberately absent**, and that is what makes this a *specification*
    /// rather than a stored job. `export_set` does not keep the requested image list - the note in
    /// migration 30 says why - so a repeat runs over whatever is selected *now*, which after a
    /// re-cull or a re-edit is a different gallery. A reader that reconstructed the old list would
    /// be re-delivering last week's selection under this week's name.
    ///
    /// `None` when the project has never been exported. Used by the autopilot, which is not
    /// allowed to invent a destination: a run over a wedding nobody has set an export up for skips
    /// the stage and says so, rather than writing three thousand files somewhere nobody chose.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn last_spec(&self, project: ProjectId) -> AuraResult<Option<JobSpec>> {
        let Some(job_id) = self.latest_job(project)? else {
            return Ok(None);
        };
        let id = job_id.clone();
        let (destination, policy, verify) = self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT destination, metadata_policy, verify FROM export_job WHERE job_id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)? == 1,
                    ))
                },
            )
            .map_err(|e| statement_failed("export_job", &e))
        })?;

        let Ok(destination) = serde_json::from_str::<Destination>(&destination) else {
            return Ok(None);
        };
        let metadata = serde_json::from_str::<MetadataPolicy>(&policy).unwrap_or_default();

        let id = job_id.clone();
        let rows: Vec<SetSpecRow> = self.catalog.read(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT name, format, quality, colour_space, bit_depth, resize, sharpen,
                         naming, sidecar
                     FROM export_set WHERE job_id = ?1 ORDER BY name",
                )
                .map_err(|e| statement_failed("export_set", &e))?;
            let mapped = stmt
                .query_map(params![id], |row| {
                    Ok(SetSpecRow {
                        name: row.get(0)?,
                        format: row.get(1)?,
                        quality: row.get(2)?,
                        colour: row.get(3)?,
                        bit_depth: row.get(4)?,
                        resize: row.get(5)?,
                        sharpen: row.get(6)?,
                        naming: row.get(7)?,
                        sidecar: row.get::<_, i64>(8)? == 1,
                    })
                })
                .map_err(|e| statement_failed("export_set", &e))?;
            let mut out = Vec::new();
            for row in mapped {
                out.push(row.map_err(|e| statement_failed("export_set", &e))?);
            }
            Ok(out)
        })?;

        // A set whose stored shape no longer parses drops out rather than being repeated wrong:
        // the vocabulary is checked by the schema, so this is only reachable across a downgrade,
        // and a set repeated at the wrong quality is worse than a set nobody repeated.
        let mut sets = Vec::with_capacity(rows.len());
        for row in rows {
            let Ok(format) = FileFormat::parse(&row.format) else {
                continue;
            };
            let Ok(colour) = DeliveryColour::parse(&row.colour) else {
                continue;
            };
            let Ok(naming) = NamingTemplate::parse(&row.naming) else {
                continue;
            };
            let Ok(sharpen) = OutputSharpen::parse(&row.sharpen) else {
                continue;
            };
            let Some(resize) = parse_resize(&row.resize) else {
                continue;
            };
            sets.push(SetSpec {
                name: row.name,
                format,
                quality: row.quality,
                resize,
                sharpen,
                naming,
                colour,
                bit_depth: row.bit_depth,
                sidecar: row.sidecar,
            });
        }
        if sets.is_empty() {
            return Ok(None);
        }
        Ok(Some(JobSpec {
            destination,
            metadata,
            verify,
            sets,
        }))
    }

    /// What a project's exports covered and found.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn outline(
        &self,
        project: ProjectId,
        photos: u32,
        selected: u32,
    ) -> AuraResult<ExportOutline> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let Some((job_id, written, verified, bytes, ms, status)) = conn
                .query_row(
                    "SELECT job_id, files_written, files_verified, bytes_written, ms, status
                     FROM export_job WHERE project_id = ?1
                     ORDER BY started_at DESC, job_id DESC LIMIT 1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, String>(5)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| statement_failed("export_job", &e))?
            else {
                return Ok(ExportOutline {
                    photos,
                    selected,
                    ..ExportOutline::default()
                });
            };

            let requested: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(requested), 0) FROM export_set WHERE job_id = ?1",
                    params![job_id],
                    |row| row.get(0),
                )
                .map_err(|e| statement_failed("export_set", &e))?;
            let renamed: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM export_file WHERE job_id = ?1 AND renamed = 1",
                    params![job_id],
                    |row| row.get(0),
                )
                .map_err(|e| statement_failed("export_file", &e))?;
            let sidecars: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM export_reason r
                     JOIN export_file f ON r.subject_key = ?1 || '|' || f.rel_path
                     WHERE f.job_id = ?1 AND r.code = 'sidecar_written'",
                    params![job_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let sealed: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM delivery_manifest WHERE job_id = ?1",
                    params![job_id],
                    |row| row.get(0),
                )
                .map_err(|e| statement_failed("delivery_manifest", &e))?;

            Ok(ExportOutline {
                photos,
                selected,
                requested: u32::try_from(requested).unwrap_or(0),
                written: u32::try_from(written).unwrap_or(0),
                verified: u32::try_from(verified).unwrap_or(0),
                unverified: u32::try_from((written - verified).max(0)).unwrap_or(0),
                corrupt: 0,
                render_failed: u32::try_from((requested - written).max(0)).unwrap_or(0),
                renamed: u32::try_from(renamed).unwrap_or(0),
                sidecars: u32::try_from(sidecars).unwrap_or(0),
                bytes: u64::try_from(bytes).unwrap_or(0),
                manifest_sealed: sealed > 0 && status == "sealed",
                ms: u64::try_from(ms).unwrap_or(0),
            })
        })
    }

    /// Every file the named job wrote, with its reasons.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn files(&self, job_id: &str) -> AuraResult<Vec<ExportedFile>> {
        let id = job_id.to_owned();
        self.catalog.read(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT set_name, photo_id, rel_path, bytes, hash, width, height,
                            render_hash, verified, renamed
                     FROM export_file WHERE job_id = ?1 ORDER BY set_name, rel_path",
                )
                .map_err(|e| statement_failed("export_file", &e))?;
            let rows = stmt
                .query_map(params![id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                    ))
                })
                .map_err(|e| statement_failed("export_file", &e))?;

            let mut out = Vec::new();
            for row in rows {
                let (set, photo, rel, bytes, hash, w, h, render_hash, verified, renamed) =
                    row.map_err(|e| statement_failed("export_file", &e))?;
                let Ok(image) = ImageId::from_db(&photo) else {
                    continue;
                };
                let key = format!("{id}|{rel}");
                let mut reasons = Vec::new();
                let mut rstmt = conn
                    .prepare(
                        "SELECT code, detail FROM export_reason
                         WHERE subject_kind = 'file' AND subject_key = ?1 ORDER BY ix",
                    )
                    .map_err(|e| statement_failed("export_reason", &e))?;
                let rrows = rstmt
                    .query_map(params![key], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
                    })
                    .map_err(|e| statement_failed("export_reason", &e))?;
                for r in rrows.flatten() {
                    // A slug this build does not know is a note a newer release wrote. Degraded:
                    // draw the panel without it rather than refusing to show a delivery.
                    if let Ok(code) = DeliveryCode::parse(&r.0) {
                        reasons.push(DeliveryReason { code, detail: r.1 });
                    }
                }

                out.push(ExportedFile {
                    image,
                    set,
                    path: std::path::PathBuf::from(rel),
                    bytes: u64::try_from(bytes).unwrap_or(0),
                    hash,
                    width: u32::try_from(w).unwrap_or(0),
                    height: u32::try_from(h).unwrap_or(0),
                    render_hash,
                    verified: verified != 0,
                    renamed: renamed != 0,
                    reasons,
                });
            }
            Ok(out)
        })
    }

    /// The last sealed manifest for a project, or `None`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn manifest(&self, project: ProjectId) -> AuraResult<Option<DeliveryManifest>> {
        let Some(job_id) = self.latest_manifest_job(project)? else {
            return Ok(None);
        };
        let files = self.files(&job_id)?;
        let key = project.to_db();
        let id = job_id.clone();
        let head = self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT created_at, qc_report_path, cleanup_disclosures FROM delivery_manifest
                 WHERE job_id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| statement_failed("delivery_manifest", &e))
        })?;
        let Some((created_at, qc, disclosures)) = head else {
            return Ok(None);
        };
        let sets = self.set_counts(&job_id)?;
        let versions = self.engine_versions(&job_id)?;
        let _ = key;

        Ok(Some(DeliveryManifest {
            project,
            created_at,
            files: files
                .iter()
                .map(|f| (f.path.clone(), f.bytes, f.hash.clone()))
                .collect(),
            sets,
            qc_report_path: qc.map(std::path::PathBuf::from),
            cleanup_disclosures: serde_json::from_str(&disclosures).unwrap_or_default(),
            engine_versions: versions,
        }))
    }

    fn latest_manifest_job(&self, project: ProjectId) -> AuraResult<Option<String>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT m.job_id FROM delivery_manifest m
                 JOIN export_job j ON j.job_id = m.job_id
                 WHERE m.project_id = ?1 ORDER BY j.started_at DESC LIMIT 1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| statement_failed("delivery_manifest", &e))
        })
    }

    fn set_counts(&self, job_id: &str) -> AuraResult<Vec<(String, u32)>> {
        let id = job_id.to_owned();
        self.catalog.read(move |conn| {
            let mut stmt = conn
                .prepare("SELECT name, written FROM export_set WHERE job_id = ?1 ORDER BY name")
                .map_err(|e| statement_failed("export_set", &e))?;
            let rows = stmt
                .query_map(params![id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })
                .map_err(|e| statement_failed("export_set", &e))?;
            Ok(rows
                .flatten()
                .map(|(n, c)| (n, u32::try_from(c).unwrap_or(0)))
                .collect())
        })
    }

    fn engine_versions(&self, job_id: &str) -> AuraResult<Vec<(String, String)>> {
        let id = job_id.to_owned();
        self.catalog.read(move |conn| {
            let text: String = conn
                .query_row(
                    "SELECT engine_versions FROM export_job WHERE job_id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| statement_failed("export_job", &e))?
                .unwrap_or_else(|| "[]".to_owned());
            Ok(serde_json::from_str(&text).unwrap_or_default())
        })
    }
}

fn destination_json(destination: &Destination) -> String {
    serde_json::to_string(destination).unwrap_or_else(|_| "{}".to_owned())
}

fn resize_text(resize: aura_core::contract::delivery::Resize) -> String {
    use aura_core::contract::delivery::Resize as R;
    match resize {
        R::Full => "full".to_owned(),
        R::LongEdge { pixels } => format!("long_edge:{pixels}"),
        R::Fit { width, height } => format!("fit:{width}x{height}"),
    }
}

/// A monotonically increasing, collision-free identifier built from the clock.
///
/// Not `Uuid::now_v7`, because phase 29 found the trap: a v7 id is random in its low bits, so a
/// fixture that mints one looks deterministic and is not, and every tie-break downstream falls
/// back on the identifier. A job id is derived from the wall clock plus a monotonic counter, so a
/// seeded test clock produces the same ids on every run.
fn uuid_like(clock: &Arc<dyn Clock>) -> String {
    let ms = clock.now_utc().unix_timestamp_nanos() / 1_000_000;
    let mono = clock.monotonic_us();
    format!("{ms:013}_{mono:012}")
}
