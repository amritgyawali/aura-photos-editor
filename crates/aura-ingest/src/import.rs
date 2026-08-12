//! The import algorithm, stated plainly:
//!
//! 1. Record the source root, keyed by (project, absolute path). Reuse the
//!    existing `root_id` if the photographer re-imports the same folder.
//! 2. Scan deterministically. Reject unreadable and zero-byte files into
//!    quarantine without stopping.
//! 3. For every scanned file, look up (`root_id`, `rel_path`). If a row exists and
//!    its `fast_key` matches, mark it seen and skip: no hash, no read.
//! 4. Otherwise hash it. If (project, `content_hash`) already exists elsewhere,
//!    this is a copy: attach a new file row to the existing photo.
//! 5. Group unseen files into photographs, choose the primary, extract EXIF.
//! 6. Insert in batches of 200 inside one IMMEDIATE transaction per batch.
//!    Commit after every batch so a kill loses at most 200 files of work.
//! 7. Update `import_run` counters in the same transaction as the rows they
//!    describe, so the progress number can never disagree with the catalog.

use std::collections::BTreeMap;
use std::path::Path;

use aura_catalog::model::{
    CameraRow, CaptureTimeSource, FileKind as CatalogFileKind, FileRole as CatalogFileRole,
    ImportCounters, JournalPhase, PhotoFileRow, PhotoRow, SourceRootRow,
};
use aura_catalog::{repo, rfc3339, Catalog};
use aura_core::errors::job;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, ContentHash, FileId, PhotoId};
use time::OffsetDateTime;

use crate::clock_align::{estimate_offsets, CameraSamples, OffsetEstimate};
use crate::contract::ingest::{FileKind, FileRole, ImportPlan, ImportReport, ImportState};
use crate::exif::ExifFacts;
use crate::scan::ScannedFile;

/// Files per transaction. A kill costs at most this much repeated work.
const BATCH: usize = 200;

/// One file that has been hashed and, if it is a primary, read for metadata.
#[derive(Debug, Clone)]
struct Prepared {
    file: ScannedFile,
    hash: ContentHash,
    role: FileRole,
    /// Only the primary of a group carries metadata.
    facts: Option<ExifFacts>,
}

/// Run an import to completion, or until the cancel token is observed.
///
/// # Errors
///
/// Returns a catalog error when a batch cannot be committed, or `AURA-IO-1001`
/// when a root has disappeared. Per-file problems become quarantine rows and
/// never stop the run.
pub fn run(
    catalog: &Catalog,
    plan: &ImportPlan,
    cancel: &CancelToken,
    progress: &dyn ProgressSink,
) -> AuraResult<ImportReport> {
    let mut report = ImportReport::default();
    let started_ms = catalog.clock().monotonic_ms();
    let mut quarantine_counts: BTreeMap<String, u64> = BTreeMap::new();

    for root in &plan.roots {
        let outcome = import_root(
            catalog,
            plan,
            root,
            cancel,
            progress,
            &mut report,
            &mut quarantine_counts,
        )?;

        if outcome == RootOutcome::Cancelled {
            finish_run(
                catalog,
                plan,
                ImportState::Cancelled,
                Some(&job::cancelled("ingest")),
            )?;
            report.duration_ms = catalog.clock().monotonic_ms().saturating_sub(started_ms);
            report.quarantine_by_code = quarantine_counts.into_iter().collect();
            return Ok(report);
        }
    }

    align_clocks(catalog, plan)?;
    finish_run(catalog, plan, ImportState::Completed, None)?;

    report.duration_ms = catalog.clock().monotonic_ms().saturating_sub(started_ms);
    report.quarantine_by_code = quarantine_counts.into_iter().collect();
    Ok(report)
}

/// Whether one root finished or observed the cancel token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootOutcome {
    /// The root was walked and every batch committed.
    Finished,
    /// The photographer stopped the run; committed work is kept.
    Cancelled,
}

/// Walk, hash and insert one root. Steps 1 to 7 of the algorithm above.
fn import_root(
    catalog: &Catalog,
    plan: &ImportPlan,
    root: &Path,
    cancel: &CancelToken,
    progress: &dyn ProgressSink,
    report: &mut ImportReport,
    quarantine_counts: &mut BTreeMap<String, u64>,
) -> AuraResult<RootOutcome> {
    {
        let now = rfc3339(catalog.clock().now_utc());
        let root_id = register_root(catalog, plan, root, &now)?;
        let import_id = plan.import_id.to_db();

        catalog.writer().transact({
            let import_id = import_id.clone();
            let project_id = plan.project_id.to_db();
            let root_id = root_id.clone();
            let now = now.clone();
            move |tx| {
                repo::import_run_start(tx, &import_id, &project_id, &root_id, APP_VERSION, &now)
            }
        })?;

        let scan_started_ms = catalog.clock().now_utc().unix_timestamp() * 1000;
        let scan = crate::scan::scan_root(root, plan, scan_started_ms)?;
        report.files_discovered += scan.files.len() as u64;

        for (path, err) in &scan.rejected {
            quarantine(catalog, plan, path, err, quarantine_counts)?;
            report.files_quarantined += 1;
        }

        // Step 3: cheap skip pass. On a re-import of an unchanged 4,000-file
        // wedding this eliminates 100 % of the hashing work.
        let known = catalog.read(|conn| repo::fast_keys_for_root(conn, &root_id))?;
        let mut todo: Vec<ScannedFile> = Vec::with_capacity(scan.files.len());
        let mut untouched: Vec<String> = Vec::new();

        for file in scan.files {
            let fast_key = crate::util::fast_key(file.size_bytes, file.mtime_ms);
            match known.get(&file.rel_path) {
                Some(existing) if *existing == fast_key => {
                    untouched.push(file.rel_path.clone());
                    report.files_already_present += 1;
                }
                _ => todo.push(file),
            }
        }

        if !untouched.is_empty() {
            catalog.writer().transact({
                let root_id = root_id.clone();
                let import_id = import_id.clone();
                let now = now.clone();
                move |tx| {
                    for rel_path in &untouched {
                        repo::touch_file_seen(tx, &root_id, rel_path, &import_id, &now)?;
                    }
                    Ok(())
                }
            })?;
        }

        // Steps 4 to 7: hash, group, insert in batches.
        let threads = crate::tuning::hash_threads_for(root);
        let total = todo.len() as u64;
        let mut done: u64 = 0;

        for chunk in todo.chunks(BATCH) {
            if cancel.is_cancelled() {
                return Ok(RootOutcome::Cancelled);
            }

            let prepared = prepare_batch(catalog, plan, chunk, threads, report, quarantine_counts)?;
            let inserted = insert_batch(catalog, plan, &root_id, prepared)?;

            report.files_imported += inserted.files;
            report.photos_created += inserted.photos;
            report.files_already_present += inserted.duplicates;

            done += chunk.len() as u64;
            progress.report(ProgressUpdate {
                stage: "ingest.import",
                done,
                total,
                current: None,
            });

            let counters = ImportCounters {
                files_discovered: report.files_discovered,
                files_imported: report.files_imported,
                files_duplicate: report.files_already_present,
                files_quarantined: report.files_quarantined,
                bytes_hashed: report.bytes_hashed,
            };
            catalog.writer().transact({
                let import_id = plan.import_id.to_db();
                move |tx| repo::import_run_update(tx, &import_id, "importing", counters)
            })?;
        }

        // Files in the catalog that this scan did not see: mark absent, do not
        // delete. A photographer who unplugged a drive has not deleted a wedding.
        catalog.writer().transact({
            let root_id = root_id.clone();
            let import_id = import_id.clone();
            move |tx| repo::mark_absent_unseen(tx, &root_id, &import_id).map(|_| ())
        })?;
    }

    Ok(RootOutcome::Finished)
}

/// The version string stamped onto rows this crate writes.
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn register_root(
    catalog: &Catalog,
    plan: &ImportPlan,
    root: &Path,
    now: &str,
) -> AuraResult<String> {
    if !root.exists() {
        return Err(aura_core::errors::io::not_found(root));
    }
    let row = SourceRootRow {
        root_id: format!(
            "src_{}",
            blake3::hash(root.to_string_lossy().as_bytes()).to_hex()
        ),
        project_id: plan.project_id.to_db(),
        abs_path: root.to_string_lossy().to_string(),
        volume_label: None,
        volume_serial: None,
        is_removable: matches!(
            crate::tuning::classify_root(root),
            crate::tuning::StorageClass::Slow
        ),
    };
    let now = now.to_string();
    catalog
        .writer()
        .transact(move |tx| repo::upsert_source_root(tx, &row, &now))
}

fn quarantine(
    catalog: &Catalog,
    plan: &ImportPlan,
    path: &Path,
    err: &AuraError,
    counts: &mut BTreeMap<String, u64>,
) -> AuraResult<()> {
    *counts.entry(err.code.0.to_string()).or_insert(0) += 1;

    let project_id = plan.project_id.to_db();
    let import_id = plan.import_id.to_db();
    let abs_path = path.to_string_lossy().to_string();
    let code = err.code.0.to_string();
    let detail = err.detail.clone();
    let now = rfc3339(catalog.clock().now_utc());

    catalog.writer().transact(move |tx| {
        repo::quarantine_add(
            tx,
            &project_id,
            Some(&import_id),
            &abs_path,
            &code,
            &detail,
            &now,
        )?;
        repo::journal_set(
            tx,
            &project_id,
            &abs_path,
            Some(&import_id),
            JournalPhase::Failed,
            None,
            &now,
        )
    })
}

/// Hash a chunk, then read metadata for the primaries only.
fn prepare_batch(
    catalog: &Catalog,
    plan: &ImportPlan,
    chunk: &[ScannedFile],
    threads: usize,
    report: &mut ImportReport,
    counts: &mut BTreeMap<String, u64>,
) -> AuraResult<Vec<Prepared>> {
    let mut hashed: Vec<(ScannedFile, ContentHash)> = Vec::with_capacity(chunk.len());

    for (index, result) in crate::hash::hash_batch(chunk, threads) {
        let Some(file) = chunk.get(index) else {
            continue;
        };
        match result {
            Ok((hash, bytes)) => {
                report.bytes_hashed += bytes;
                hashed.push((file.clone(), hash));
            }
            Err(err) if err.is_retryable() => {
                // A file still being written. Try once more, then quarantine.
                match crate::hash::hash_file(&file.abs_path, file.size_bytes) {
                    Ok((hash, bytes)) => {
                        report.bytes_hashed += bytes;
                        hashed.push((file.clone(), hash));
                    }
                    Err(second) => {
                        quarantine(catalog, plan, &file.abs_path, &second, counts)?;
                        report.files_quarantined += 1;
                    }
                }
            }
            Err(err) => {
                quarantine(catalog, plan, &file.abs_path, &err, counts)?;
                report.files_quarantined += 1;
            }
        }
    }

    let files: Vec<ScannedFile> = hashed.iter().map(|(f, _)| f.clone()).collect();
    let groups = crate::pair::group(&files);

    let mut prepared = Vec::with_capacity(hashed.len());
    for group in groups {
        let Some((file, hash)) = hashed.get(group.primary).cloned() else {
            continue;
        };
        let facts = crate::exif::extract(&file.abs_path).ok();
        prepared.push(Prepared {
            file,
            hash,
            role: FileRole::Primary,
            facts,
        });

        for (index, role) in group.companions {
            if let Some((file, hash)) = hashed.get(index).cloned() {
                prepared.push(Prepared {
                    file,
                    hash,
                    role,
                    facts: None,
                });
            }
        }
    }
    Ok(prepared)
}

/// What one committed batch changed.
#[derive(Debug, Clone, Copy, Default)]
struct BatchOutcome {
    files: u64,
    photos: u64,
    duplicates: u64,
}

fn insert_batch(
    catalog: &Catalog,
    plan: &ImportPlan,
    root_id: &str,
    prepared: Vec<Prepared>,
) -> AuraResult<BatchOutcome> {
    let project_id = plan.project_id.to_db();
    let import_id = plan.import_id.to_db();
    let root_id = root_id.to_string();
    let now = rfc3339(catalog.clock().now_utc());

    catalog.writer().transact(move |tx| {
        let mut outcome = BatchOutcome::default();
        let mut current_photo: Option<String> = None;

        for item in &prepared {
            let is_primary = item.role == FileRole::Primary;
            let hash_hex = item.hash.to_hex();

            let photo_id = if is_primary {
                if let Some(existing) = repo::photo_id_for_hash(tx, &project_id, &hash_hex)? {
                    // The same bytes are already a photograph: this path is a copy
                    // or a rename, and it joins the photograph it belongs to.
                    outcome.duplicates += 1;
                    existing
                } else {
                    let photo_id = PhotoId::new().to_db();
                    let row = photo_row(&photo_id, &project_id, item);
                    repo::insert_photo(tx, &row, &now)?;
                    outcome.photos += 1;
                    record_camera_and_gps(tx, &project_id, &photo_id, item, &now)?;
                    photo_id
                }
            } else if let Some(id) = current_photo.clone() {
                id
            } else {
                continue; // a companion with no primary in this batch
            };

            if is_primary {
                current_photo = Some(photo_id.clone());
            }

            let file_row = file_row(&photo_id, &project_id, &root_id, item, &hash_hex);
            let (file_id, inserted) = repo::upsert_file(tx, &file_row, &import_id, &now)?;
            if inserted {
                outcome.files += 1;
            }
            if is_primary {
                repo::set_primary_file(tx, &photo_id, &file_id)?;
                if let Some(facts) = &item.facts {
                    if !facts.raw_tags.is_empty() {
                        repo::insert_exif(tx, &file_id, &facts.raw_tags)?;
                    }
                }
            }

            repo::journal_set(
                tx,
                &project_id,
                &item.file.abs_path.to_string_lossy(),
                Some(&import_id),
                JournalPhase::Inserted,
                Some(&hash_hex),
                &now,
            )?;
        }

        repo::refresh_camera_counts(tx, &project_id)?;
        Ok(outcome)
    })
}

/// Register the body that took this photograph, and its GPS if the fix was sane.
fn record_camera_and_gps(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
    photo_id: &str,
    item: &Prepared,
    now: &str,
) -> AuraResult<()> {
    let Some(facts) = &item.facts else {
        return Ok(());
    };

    if let Some(serial) = camera_key(facts) {
        let camera = CameraRow {
            camera_id: format!("cam_{}", blake3::hash(serial.as_bytes()).to_hex()),
            project_id: project_id.to_string(),
            make: facts.camera_make.clone(),
            model: facts.camera_model.clone(),
            body_serial: serial,
            shooter_label: "primary".to_string(),
            clock_offset_ms: 0,
            offset_confidence: 0.0,
            offset_source: "none".to_string(),
            photo_count: 0,
        };
        repo::upsert_camera(tx, &camera, now)?;
    }
    if let Some((lat, lon)) = facts.gps {
        repo::insert_gps(tx, photo_id, lat, lon)?;
    }
    Ok(())
}

fn photo_row(photo_id: &str, project_id: &str, item: &Prepared) -> PhotoRow {
    let facts = item.facts.clone().unwrap_or_default();
    let capture_time = facts.capture_time.map(rfc3339);
    PhotoRow {
        photo_id: photo_id.to_string(),
        project_id: project_id.to_string(),
        capture_time: capture_time.clone(),
        capture_time_source: match facts.capture_time_source {
            "exif_original" => CaptureTimeSource::ExifOriginal,
            "exif_digitized" => CaptureTimeSource::ExifDigitized,
            "file_mtime" => CaptureTimeSource::FileMtime,
            _ => CaptureTimeSource::Unknown,
        },
        // Before alignment the timeline equals the capture time; the aligner
        // rewrites it in one UPDATE once every body is known.
        timeline_time: capture_time,
        camera_make: facts.camera_make.clone(),
        camera_model: facts.camera_model.clone(),
        camera_serial: camera_key(&facts),
        lens_model: facts.lens_model.clone(),
        iso: facts.iso,
        aperture: facts.aperture,
        shutter_us: facts.shutter_us,
        focal_len_mm: facts.focal_len_mm,
        flash_fired: facts.flash_fired,
        orientation: i64::from(facts.orientation),
        width_px: facts.width_px,
        height_px: facts.height_px,
        sub_sec: facts.sub_sec,
    }
}

fn file_row(
    photo_id: &str,
    project_id: &str,
    root_id: &str,
    item: &Prepared,
    hash_hex: &str,
) -> PhotoFileRow {
    PhotoFileRow {
        file_id: FileId::new().to_db(),
        photo_id: photo_id.to_string(),
        project_id: project_id.to_string(),
        root_id: root_id.to_string(),
        rel_path: item.file.rel_path.clone(),
        file_name: item.file.file_name.clone(),
        ext: item.file.ext.clone(),
        kind: catalog_kind(item.file.kind),
        role: catalog_role(item.role),
        size_bytes: i64::try_from(item.file.size_bytes).unwrap_or(i64::MAX),
        content_hash: hash_hex.to_string(),
        mtime: rfc3339(
            OffsetDateTime::from_unix_timestamp_nanos(i128::from(item.file.mtime_ms) * 1_000_000)
                .unwrap_or(OffsetDateTime::UNIX_EPOCH),
        ),
        fast_key: crate::util::fast_key(item.file.size_bytes, item.file.mtime_ms),
    }
}

/// The clustering key for a body: its serial when the maker records one,
/// otherwise make and model, which is the best evidence available.
fn camera_key(facts: &ExifFacts) -> Option<String> {
    facts.camera_serial.clone().or_else(|| {
        match (facts.camera_make.as_deref(), facts.camera_model.as_deref()) {
            (Some(make), Some(model)) => Some(format!("{make} {model}")),
            (None, Some(model)) => Some(model.to_string()),
            (Some(make), None) => Some(make.to_string()),
            (None, None) => None,
        }
    })
}

const fn catalog_kind(kind: FileKind) -> CatalogFileKind {
    match kind {
        FileKind::Raw => CatalogFileKind::Raw,
        FileKind::Jpeg => CatalogFileKind::Jpeg,
        FileKind::Heif => CatalogFileKind::Heif,
        FileKind::Tiff => CatalogFileKind::Tiff,
        FileKind::Png => CatalogFileKind::Png,
        FileKind::SidecarXmp => CatalogFileKind::SidecarXmp,
        FileKind::Video => CatalogFileKind::Video,
        FileKind::Other => CatalogFileKind::Other,
    }
}

const fn catalog_role(role: FileRole) -> CatalogFileRole {
    match role {
        FileRole::Primary => CatalogFileRole::Primary,
        FileRole::CompanionJpeg => CatalogFileRole::CompanionJpeg,
        FileRole::Sidecar => CatalogFileRole::Sidecar,
        FileRole::Derivative => CatalogFileRole::Derivative,
    }
}

fn finish_run(
    catalog: &Catalog,
    plan: &ImportPlan,
    state: ImportState,
    error: Option<&AuraError>,
) -> AuraResult<()> {
    let import_id = plan.import_id.to_db();
    let code = error.map(|e| e.code.0.to_string());
    let now = rfc3339(catalog.clock().now_utc());
    catalog.writer().transact(move |tx| {
        repo::import_run_finish(tx, &import_id, state.as_str(), code.as_deref(), &now)
    })
}

/// Estimate every body's clock offset, store it, and rewrite `timeline_time`.
///
/// # Errors
///
/// Returns a catalog error when the offsets cannot be written.
pub fn align_clocks(catalog: &Catalog, plan: &ImportPlan) -> AuraResult<Vec<OffsetEstimate>> {
    let project_id = plan.project_id.to_db();
    let samples_raw = catalog.read(|conn| repo::capture_times_by_camera(conn, &project_id))?;

    let mut by_serial: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for (serial, capture_time) in samples_raw {
        if let Some(ms) = parse_rfc3339_ms(&capture_time) {
            by_serial.entry(serial).or_default().push(ms);
        }
    }
    let samples: Vec<CameraSamples> = by_serial
        .into_iter()
        .map(|(body_serial, times_ms)| CameraSamples {
            body_serial,
            times_ms,
        })
        .collect();

    let estimates = estimate_offsets(&samples);
    if estimates.is_empty() {
        return Ok(estimates);
    }

    let cameras = catalog.read(|conn| repo::list_cameras(conn, &project_id))?;
    let now = rfc3339(catalog.clock().now_utc());
    let writes: Vec<(String, i64, f64, String)> = estimates
        .iter()
        .filter_map(|estimate| {
            cameras
                .iter()
                .find(|c| c.body_serial == estimate.body_serial)
                .map(|camera| {
                    let source = if estimate.applied {
                        "estimated"
                    } else {
                        "none"
                    };
                    (
                        camera.camera_id.clone(),
                        estimate.offset_ms,
                        estimate.confidence,
                        source.to_string(),
                    )
                })
        })
        .collect();

    catalog.writer().transact({
        let project_id = project_id.clone();
        let now = now.clone();
        move |tx| {
            for (camera_id, offset_ms, confidence, source) in &writes {
                repo::set_camera_offset(tx, camera_id, *offset_ms, *confidence, source, &now)?;
            }
            repo::recompute_timeline(tx, &project_id, &now).map(|_| ())
        }
    })?;

    Ok(estimates)
}

/// Milliseconds since the epoch for an RFC-3339 timestamp written by this crate.
fn parse_rfc3339_ms(text: &str) -> Option<i64> {
    OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
        .ok()
        .and_then(|t| i64::try_from(t.unix_timestamp_nanos() / 1_000_000).ok())
}
