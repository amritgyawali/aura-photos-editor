//! Every statement the product runs against the catalog lives here.
//!
//! Callers pass a `&Connection`; a `&Transaction` derefs to one, so the same
//! function works inside and outside a transaction.

use aura_core::errors::db::statement_failed;
use aura_core::AuraResult;
use rusqlite::{params, Connection, OptionalExtension};

use crate::model::{
    CameraRow, ImportCounters, JournalPhase, PhotoFileRow, PhotoGridRow, PhotoRow, ProjectRow,
    SourceRootRow,
};

/// How the grid is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoOrder {
    /// Clock-aligned capture time, falling back to file name.
    Timeline,
    /// File name only. Used when a photographer distrusts the clocks.
    FileName,
}

fn failed(what: &'static str) -> impl FnOnce(rusqlite::Error) -> aura_core::AuraError {
    move |e| statement_failed(what, &e)
}

// ---------------------------------------------------------------- projects

/// Insert a project and its denied-by-default consent row in one step.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when either insert fails.
pub fn create_project(conn: &Connection, row: &ProjectRow) -> AuraResult<()> {
    conn.execute(
        "INSERT INTO project
           (project_id, name, couple_label, event_date, timezone, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            row.project_id,
            row.name,
            row.couple_label,
            row.event_date,
            row.timezone,
            row.status,
            row.created_at,
            row.updated_at
        ],
    )
    .map_err(failed("insert project"))?;

    conn.execute(
        "INSERT INTO project_consent (project_id) VALUES (?1)",
        params![row.project_id],
    )
    .map_err(failed("insert project consent"))?;
    Ok(())
}

/// List every project that has not been soft deleted, newest event first.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn list_projects(conn: &Connection) -> AuraResult<Vec<ProjectRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT project_id, name, couple_label, event_date, timezone, status,
                    created_at, updated_at
             FROM project
             WHERE deleted_at IS NULL
             ORDER BY COALESCE(event_date, created_at) DESC, project_id",
        )
        .map_err(failed("prepare list projects"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(ProjectRow {
                project_id: row.get(0)?,
                name: row.get(1)?,
                couple_label: row.get(2)?,
                event_date: row.get(3)?,
                timezone: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(failed("query list projects"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(failed("read project row"))?);
    }
    Ok(out)
}

// ------------------------------------------------------------ source roots

/// Insert a source root, or return the id of the one already recorded.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the insert or the lookup fails.
pub fn upsert_source_root(conn: &Connection, row: &SourceRootRow, now: &str) -> AuraResult<String> {
    conn.execute(
        "INSERT INTO source_root
           (root_id, project_id, abs_path, volume_label, volume_serial, is_removable, added_at, last_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT (project_id, abs_path) DO UPDATE SET last_seen_at = excluded.last_seen_at",
        params![
            row.root_id,
            row.project_id,
            row.abs_path,
            row.volume_label,
            row.volume_serial,
            i64::from(row.is_removable),
            now
        ],
    )
    .map_err(failed("upsert source root"))?;

    conn.query_row(
        "SELECT root_id FROM source_root WHERE project_id = ?1 AND abs_path = ?2",
        params![row.project_id, row.abs_path],
        |r| r.get(0),
    )
    .map_err(failed("read source root id"))
}

// ----------------------------------------------------------------- cameras

/// Insert or update the camera row for one body serial and return its id.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the statement fails.
pub fn upsert_camera(conn: &Connection, row: &CameraRow, now: &str) -> AuraResult<String> {
    conn.execute(
        "INSERT INTO camera
           (camera_id, project_id, make, model, body_serial, shooter_label,
            clock_offset_ms, offset_confidence, offset_source, photo_count, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
         ON CONFLICT (project_id, body_serial) DO UPDATE SET
            make = COALESCE(excluded.make, camera.make),
            model = COALESCE(excluded.model, camera.model),
            updated_at = excluded.updated_at",
        params![
            row.camera_id,
            row.project_id,
            row.make,
            row.model,
            row.body_serial,
            row.shooter_label,
            row.clock_offset_ms,
            row.offset_confidence,
            row.offset_source,
            row.photo_count,
            now
        ],
    )
    .map_err(failed("upsert camera"))?;

    conn.query_row(
        "SELECT camera_id FROM camera WHERE project_id = ?1 AND body_serial = ?2",
        params![row.project_id, row.body_serial],
        |r| r.get(0),
    )
    .map_err(failed("read camera id"))
}

/// Record a clock offset for one body, with its confidence and provenance.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the update fails.
pub fn set_camera_offset(
    conn: &Connection,
    camera_id: &str,
    offset_ms: i64,
    confidence: f64,
    source: &str,
    now: &str,
) -> AuraResult<()> {
    conn.execute(
        "UPDATE camera
            SET clock_offset_ms = ?2, offset_confidence = ?3, offset_source = ?4, updated_at = ?5
          WHERE camera_id = ?1",
        params![camera_id, offset_ms, confidence, source, now],
    )
    .map_err(failed("update camera offset"))?;
    Ok(())
}

/// Every camera in a project, ordered by frame count so the reference body is first.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn list_cameras(conn: &Connection, project_id: &str) -> AuraResult<Vec<CameraRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT camera_id, project_id, make, model, body_serial, shooter_label,
                    clock_offset_ms, offset_confidence, offset_source, photo_count
             FROM camera WHERE project_id = ?1
             ORDER BY photo_count DESC, body_serial",
        )
        .map_err(failed("prepare list cameras"))?;

    let rows = stmt
        .query_map(params![project_id], |row| {
            Ok(CameraRow {
                camera_id: row.get(0)?,
                project_id: row.get(1)?,
                make: row.get(2)?,
                model: row.get(3)?,
                body_serial: row.get(4)?,
                shooter_label: row.get(5)?,
                clock_offset_ms: row.get(6)?,
                offset_confidence: row.get(7)?,
                offset_source: row.get(8)?,
                photo_count: row.get(9)?,
            })
        })
        .map_err(failed("query list cameras"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(failed("read camera row"))?);
    }
    Ok(out)
}

/// Refresh `camera.photo_count` from the photo table.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the update fails.
pub fn refresh_camera_counts(conn: &Connection, project_id: &str) -> AuraResult<()> {
    conn.execute(
        "UPDATE camera
            SET photo_count = (
                SELECT COUNT(*) FROM photo p
                 WHERE p.project_id = camera.project_id
                   AND p.camera_serial = camera.body_serial)
          WHERE project_id = ?1",
        params![project_id],
    )
    .map_err(failed("refresh camera counts"))?;
    Ok(())
}

/// Capture times grouped by body serial, for the clock aligner.
///
/// Photos with no serial and no capture time are skipped: the aligner works on
/// evidence, never on guesses.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn capture_times_by_camera(
    conn: &Connection,
    project_id: &str,
) -> AuraResult<Vec<(String, String)>> {
    let mut stmt = conn
        .prepare(
            "SELECT camera_serial, capture_time FROM photo
              WHERE project_id = ?1 AND camera_serial IS NOT NULL AND capture_time IS NOT NULL
              ORDER BY camera_serial, capture_time",
        )
        .map_err(failed("prepare capture times"))?;
    let rows = stmt
        .query_map(params![project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(failed("query capture times"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(failed("read capture time row"))?);
    }
    Ok(out)
}

// ------------------------------------------------------------------ photos

/// Insert one photo row.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the insert fails.
pub fn insert_photo(conn: &Connection, row: &PhotoRow, now: &str) -> AuraResult<()> {
    conn.execute(
        "INSERT INTO photo
           (photo_id, project_id, capture_time, capture_time_source, camera_make, camera_model,
            camera_serial, lens_model, iso, aperture, shutter_us, focal_len_mm, flash_fired,
            orientation, width_px, height_px, sub_sec, timeline_time, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?19)",
        params![
            row.photo_id,
            row.project_id,
            row.capture_time,
            row.capture_time_source.as_str(),
            row.camera_make,
            row.camera_model,
            row.camera_serial,
            row.lens_model,
            row.iso,
            row.aperture,
            row.shutter_us,
            row.focal_len_mm,
            row.flash_fired.map(i64::from),
            row.orientation,
            row.width_px,
            row.height_px,
            row.sub_sec,
            row.timeline_time,
            now
        ],
    )
    .map_err(failed("insert photo"))?;
    Ok(())
}

/// Insert one file row.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the insert fails.
pub fn insert_file(conn: &Connection, row: &PhotoFileRow, now: &str) -> AuraResult<()> {
    conn.execute(
        "INSERT INTO photo_file
           (file_id, photo_id, project_id, root_id, rel_path, file_name, ext, kind, role,
            size_bytes, content_hash, mtime, fast_key, first_seen_at, last_seen_at, is_present)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14, 1)",
        params![
            row.file_id,
            row.photo_id,
            row.project_id,
            row.root_id,
            row.rel_path,
            row.file_name,
            row.ext,
            row.kind.as_str(),
            row.role.as_str(),
            row.size_bytes,
            row.content_hash,
            row.mtime,
            row.fast_key,
            now
        ],
    )
    .map_err(failed("insert photo file"))?;
    Ok(())
}

/// Store the raw EXIF key/value pairs for one file.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when an insert fails.
pub fn insert_exif(conn: &Connection, file_id: &str, tags: &[(String, String)]) -> AuraResult<()> {
    let mut stmt = conn
        .prepare("INSERT OR REPLACE INTO exif (file_id, tag, value) VALUES (?1, ?2, ?3)")
        .map_err(failed("prepare insert exif"))?;
    for (tag, value) in tags {
        stmt.execute(params![file_id, tag, value])
            .map_err(failed("insert exif"))?;
    }
    Ok(())
}

/// The photo id that already owns this content hash, if any.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn photo_id_for_hash(
    conn: &Connection,
    project_id: &str,
    content_hash: &str,
) -> AuraResult<Option<String>> {
    conn.query_row(
        "SELECT photo_id FROM photo_file WHERE project_id = ?1 AND content_hash = ?2 LIMIT 1",
        params![project_id, content_hash],
        |r| r.get(0),
    )
    .optional()
    .map_err(failed("lookup photo by hash"))
}

/// True when this exact path is already recorded with the same size and mtime.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn fast_key_matches(
    conn: &Connection,
    root_id: &str,
    rel_path: &str,
    fast_key: &str,
) -> AuraResult<bool> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM photo_file WHERE root_id = ?1 AND rel_path = ?2 AND fast_key = ?3",
            params![root_id, rel_path, fast_key],
            |r| r.get(0),
        )
        .optional()
        .map_err(failed("fast key lookup"))?;
    Ok(found.is_some())
}

/// Attach a companion file (camera JPEG or XMP sidecar) to an existing photo.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the update fails.
pub fn set_primary_file(conn: &Connection, photo_id: &str, file_id: &str) -> AuraResult<()> {
    conn.execute(
        "UPDATE photo SET primary_file_id = ?2 WHERE photo_id = ?1 AND primary_file_id IS NULL",
        params![photo_id, file_id],
    )
    .map_err(failed("set primary file"))?;
    Ok(())
}

/// Recompute `timeline_time` for every photo from its body's clock offset.
///
/// This is a single UPDATE so a 4,000-row project re-orders in well under a second.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the update fails.
pub fn recompute_timeline(conn: &Connection, project_id: &str, now: &str) -> AuraResult<usize> {
    conn.execute(
        "UPDATE photo
            SET timeline_time = COALESCE(
                  (SELECT strftime('%Y-%m-%dT%H:%M:%fZ', photo.capture_time,
                          printf('%+.3f seconds', c.clock_offset_ms / 1000.0))
                     FROM camera c
                    WHERE c.project_id = photo.project_id
                      AND c.body_serial = photo.camera_serial),
                  photo.capture_time),
                updated_at = ?2
          WHERE project_id = ?1 AND capture_time IS NOT NULL",
        params![project_id, now],
    )
    .map_err(failed("recompute timeline"))
}

/// One page of the grid.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn list_photos(
    conn: &Connection,
    project_id: &str,
    offset: i64,
    limit: i64,
    order: PhotoOrder,
) -> AuraResult<Vec<PhotoGridRow>> {
    let sql = match order {
        PhotoOrder::Timeline => {
            "SELECT p.photo_id,
                    COALESCE(f.file_name, ''),
                    COALESCE(p.timeline_time, p.capture_time),
                    c.camera_id,
                    COALESCE(p.width_px, 0),
                    COALESCE(p.height_px, 0),
                    CASE WHEN COALESCE(f.is_present, 1) = 0 THEN 'missing' ELSE 'indexed' END
               FROM photo p
               LEFT JOIN photo_file f ON f.photo_id = p.photo_id AND f.role = 'primary'
               LEFT JOIN camera c ON c.project_id = p.project_id AND c.body_serial = p.camera_serial
              WHERE p.project_id = ?1
              ORDER BY COALESCE(p.timeline_time, p.capture_time) IS NULL,
                       COALESCE(p.timeline_time, p.capture_time),
                       COALESCE(p.sub_sec, 0),
                       f.file_name
              LIMIT ?2 OFFSET ?3"
        }
        PhotoOrder::FileName => {
            "SELECT p.photo_id,
                    COALESCE(f.file_name, ''),
                    COALESCE(p.timeline_time, p.capture_time),
                    c.camera_id,
                    COALESCE(p.width_px, 0),
                    COALESCE(p.height_px, 0),
                    CASE WHEN COALESCE(f.is_present, 1) = 0 THEN 'missing' ELSE 'indexed' END
               FROM photo p
               LEFT JOIN photo_file f ON f.photo_id = p.photo_id AND f.role = 'primary'
               LEFT JOIN camera c ON c.project_id = p.project_id AND c.body_serial = p.camera_serial
              WHERE p.project_id = ?1
              ORDER BY f.file_name, p.photo_id
              LIMIT ?2 OFFSET ?3"
        }
    };

    let mut stmt = conn.prepare(sql).map_err(failed("prepare list photos"))?;
    let rows = stmt
        .query_map(params![project_id, limit, offset], |row| {
            Ok(PhotoGridRow {
                id: row.get(0)?,
                file_name: row.get(1)?,
                timeline_ts: row.get(2)?,
                camera_id: row.get(3)?,
                width: row.get(4)?,
                height: row.get(5)?,
                status: row.get(6)?,
            })
        })
        .map_err(failed("query list photos"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(failed("read photo row"))?);
    }
    Ok(out)
}

/// How many photos a project holds.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn count_photos(conn: &Connection, project_id: &str) -> AuraResult<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM photo WHERE project_id = ?1",
        params![project_id],
        |r| r.get(0),
    )
    .map_err(failed("count photos"))
}

/// Every recorded `rel_path` under a root with its `fast_key`.
///
/// One query instead of 4,000 lets a re-import skip hashing entirely.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn fast_keys_for_root(
    conn: &Connection,
    root_id: &str,
) -> AuraResult<std::collections::BTreeMap<String, String>> {
    let mut stmt = conn
        .prepare("SELECT rel_path, fast_key FROM photo_file WHERE root_id = ?1")
        .map_err(failed("prepare fast keys"))?;
    let rows = stmt
        .query_map(params![root_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(failed("query fast keys"))?;

    let mut out = std::collections::BTreeMap::new();
    for row in rows {
        let (rel_path, fast_key) = row.map_err(failed("read fast key row"))?;
        out.insert(rel_path, fast_key);
    }
    Ok(out)
}

/// Mark an unchanged file as seen by this import without rewriting its data.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the update fails.
pub fn touch_file_seen(
    conn: &Connection,
    root_id: &str,
    rel_path: &str,
    import_id: &str,
    now: &str,
) -> AuraResult<()> {
    conn.execute(
        "UPDATE photo_file SET last_seen_at = ?3, last_seen_import = ?4, is_present = 1
          WHERE root_id = ?1 AND rel_path = ?2",
        params![root_id, rel_path, now, import_id],
    )
    .map_err(failed("touch file seen"))?;
    Ok(())
}

/// Insert a file, or refresh the row that already owns this path.
///
/// Returns the file id that now owns the path and whether the row is new. On
/// conflict the existing id comes back, so a re-import can report zero inserts
/// honestly instead of claiming 4,000.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the statement fails.
pub fn upsert_file(
    conn: &Connection,
    row: &PhotoFileRow,
    import_id: &str,
    now: &str,
) -> AuraResult<(String, bool)> {
    let existing: String = conn
        .query_row(
            "INSERT INTO photo_file
               (file_id, photo_id, project_id, root_id, rel_path, file_name, ext, kind, role,
                size_bytes, content_hash, mtime, fast_key, first_seen_at, last_seen_at,
                last_seen_import, is_present)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14, ?15, 1)
             ON CONFLICT (root_id, rel_path) DO UPDATE SET
                last_seen_at     = excluded.last_seen_at,
                last_seen_import = excluded.last_seen_import,
                is_present       = 1,
                size_bytes       = excluded.size_bytes,
                mtime            = excluded.mtime,
                fast_key         = excluded.fast_key,
                content_hash     = excluded.content_hash
             RETURNING file_id",
            params![
                row.file_id,
                row.photo_id,
                row.project_id,
                row.root_id,
                row.rel_path,
                row.file_name,
                row.ext,
                row.kind.as_str(),
                row.role.as_str(),
                row.size_bytes,
                row.content_hash,
                row.mtime,
                row.fast_key,
                now,
                import_id
            ],
            |r| r.get(0),
        )
        .map_err(failed("upsert photo file"))?;

    let inserted = existing == row.file_id;
    Ok((existing, inserted))
}

/// Mark files under a root that this import did not see as absent.
///
/// An unplugged drive is not a deletion, so rows survive and keep their ratings.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the update fails.
pub fn mark_absent_unseen(conn: &Connection, root_id: &str, import_id: &str) -> AuraResult<usize> {
    conn.execute(
        "UPDATE photo_file SET is_present = 0
          WHERE root_id = ?1
            AND (last_seen_import IS NULL OR last_seen_import <> ?2)",
        params![root_id, import_id],
    )
    .map_err(failed("mark absent files"))
}

/// Store validated GPS for a photo. Out-of-range coordinates never reach here.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the insert fails.
pub fn insert_gps(conn: &Connection, photo_id: &str, lat: f64, lon: f64) -> AuraResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO photo_gps (photo_id, lat, lon, source) VALUES (?1, ?2, ?3, 'exif')",
        params![photo_id, lat, lon],
    )
    .map_err(failed("insert gps"))?;
    Ok(())
}

/// A content digest of everything an import decides, ignoring ids and timestamps.
///
/// Two catalogs with the same digest hold the same photographs, which is what a
/// re-import test needs to assert.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn catalog_digest(conn: &Connection, project_id: &str) -> AuraResult<String> {
    let mut hasher = blake3::Hasher::new();

    let mut files = conn
        .prepare(
            "SELECT rel_path, content_hash, role, kind, size_bytes, is_present
               FROM photo_file WHERE project_id = ?1 ORDER BY rel_path",
        )
        .map_err(failed("prepare digest files"))?;
    let rows = files
        .query_map(params![project_id], |row| {
            Ok(format!(
                "{}|{}|{}|{}|{}|{}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?
            ))
        })
        .map_err(failed("query digest files"))?;
    for row in rows {
        hasher.update(row.map_err(failed("read digest row"))?.as_bytes());
        hasher.update(b"\n");
    }

    let mut photos = conn
        .prepare(
            "SELECT COALESCE(p.capture_time, ''), p.capture_time_source,
                    COALESCE(p.camera_serial, ''), COALESCE(p.timeline_time, ''),
                    (SELECT COUNT(*) FROM photo_file f WHERE f.photo_id = p.photo_id)
               FROM photo p WHERE p.project_id = ?1
              ORDER BY COALESCE(p.capture_time, ''), p.photo_id",
        )
        .map_err(failed("prepare digest photos"))?;
    let rows = photos
        .query_map(params![project_id], |row| {
            Ok(format!(
                "{}|{}|{}|{}|{}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?
            ))
        })
        .map_err(failed("query digest photos"))?;
    for row in rows {
        hasher.update(row.map_err(failed("read digest row"))?.as_bytes());
        hasher.update(b"\n");
    }

    Ok(hasher.finalize().to_hex().to_string())
}

// ------------------------------------------------------------------ journal

/// Record how far one path has progressed. Idempotent, so a resume is cheap.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the upsert fails.
pub fn journal_set(
    conn: &Connection,
    project_id: &str,
    abs_path: &str,
    import_id: Option<&str>,
    phase: JournalPhase,
    content_hash: Option<&str>,
    now: &str,
) -> AuraResult<()> {
    conn.execute(
        "INSERT INTO ingest_journal
           (project_id, abs_path, import_id, phase, content_hash, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (project_id, abs_path) DO UPDATE SET
            phase = excluded.phase,
            import_id = excluded.import_id,
            content_hash = COALESCE(excluded.content_hash, ingest_journal.content_hash),
            updated_at = excluded.updated_at",
        params![
            project_id,
            abs_path,
            import_id,
            phase.as_str(),
            content_hash,
            now
        ],
    )
    .map_err(failed("write ingest journal"))?;
    Ok(())
}

/// The phase text recorded for a path, if the walker has seen it before.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn journal_phase(
    conn: &Connection,
    project_id: &str,
    abs_path: &str,
) -> AuraResult<Option<String>> {
    conn.query_row(
        "SELECT phase FROM ingest_journal WHERE project_id = ?1 AND abs_path = ?2",
        params![project_id, abs_path],
        |r| r.get(0),
    )
    .optional()
    .map_err(failed("read ingest journal"))
}

// ----------------------------------------------------------------- previews

/// One row of the `preview` index. The bytes live in the content-addressed
/// cache; this row only records that they exist, so deleting the cache folder
/// is safe and the index heals on the next request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRow {
    /// Owning photograph.
    pub photo_id: String,
    /// 1 for the embedded thumbnail, 2 for the proxy, 3 for a full render.
    pub tier: i64,
    /// Path of the cached artefact relative to the cache root.
    pub rel_cache_path: String,
    /// Width in pixels.
    pub width_px: i64,
    /// Height in pixels.
    pub height_px: i64,
    /// `embedded` or `decoded`.
    pub source: String,
    /// Bytes stored on disk.
    pub bytes: i64,
    /// The rendering pipeline version that produced it.
    pub stage_version: i64,
}

/// Record a preview, replacing any earlier one for the same photo and tier.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the upsert fails.
pub fn upsert_preview(conn: &Connection, row: &PreviewRow, now: &str) -> AuraResult<()> {
    conn.execute(
        "INSERT INTO preview
           (photo_id, tier, rel_cache_path, width_px, height_px, source, bytes, built_at,
            stage_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT (photo_id, tier) DO UPDATE SET
            rel_cache_path = excluded.rel_cache_path,
            width_px       = excluded.width_px,
            height_px      = excluded.height_px,
            source         = excluded.source,
            bytes          = excluded.bytes,
            built_at       = excluded.built_at,
            stage_version  = excluded.stage_version",
        params![
            row.photo_id,
            row.tier,
            row.rel_cache_path,
            row.width_px,
            row.height_px,
            row.source,
            row.bytes,
            now,
            row.stage_version,
        ],
    )
    .map_err(failed("write preview row"))?;
    Ok(())
}

/// Read one preview row, if it has been built.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn preview_row(conn: &Connection, photo_id: &str, tier: i64) -> AuraResult<Option<PreviewRow>> {
    conn.query_row(
        "SELECT photo_id, tier, rel_cache_path, width_px, height_px, source, bytes, stage_version
           FROM preview WHERE photo_id = ?1 AND tier = ?2",
        params![photo_id, tier],
        |row| {
            Ok(PreviewRow {
                photo_id: row.get(0)?,
                tier: row.get(1)?,
                rel_cache_path: row.get(2)?,
                width_px: row.get(3)?,
                height_px: row.get(4)?,
                source: row.get(5)?,
                bytes: row.get(6)?,
                stage_version: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(failed("read preview row"))
}

/// How many previews of one tier exist in a project, for the exit report and
/// for the progress bar the grid shows while thumbnails fill in.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn count_previews(conn: &Connection, project_id: &str, tier: i64) -> AuraResult<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM preview p
           JOIN photo ph ON ph.photo_id = p.photo_id
          WHERE ph.project_id = ?1 AND p.tier = ?2",
        params![project_id, tier],
        |row| row.get(0),
    )
    .map_err(failed("count previews"))
}

/// Absolute path and content hash of a photograph's primary file.
///
/// Returns `None` when the photograph has no primary file, which means ingest
/// and previews disagree - worth surfacing rather than papering over.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn primary_file_for_photo(
    conn: &Connection,
    photo_id: &str,
) -> AuraResult<Option<(String, String)>> {
    conn.query_row(
        "SELECT r.abs_path || '/' || f.rel_path, f.content_hash
           FROM photo_file f
           JOIN source_root r ON r.root_id = f.root_id
          WHERE f.photo_id = ?1
          ORDER BY CASE f.role WHEN 'primary' THEN 0 WHEN 'companion_jpeg' THEN 1 ELSE 2 END,
                   f.rel_path
          LIMIT 1",
        params![photo_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(failed("read primary file"))
}

/// Every photograph in a project that has no preview at the given tier, oldest
/// first. This is the work list a batch proxy run walks, and it is why a killed
/// run resumes without recomputing: the query only returns what is missing.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn photos_without_preview(
    conn: &Connection,
    project_id: &str,
    tier: i64,
    limit: i64,
) -> AuraResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT ph.photo_id FROM photo ph
              WHERE ph.project_id = ?1
                AND NOT EXISTS (
                      SELECT 1 FROM preview p
                       WHERE p.photo_id = ph.photo_id AND p.tier = ?2)
              ORDER BY ph.timeline_time, ph.photo_id
              LIMIT ?3",
        )
        .map_err(failed("prepare photos without preview"))?;
    let rows = stmt
        .query_map(params![project_id, tier, limit], |row| row.get(0))
        .map_err(failed("query photos without preview"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(failed("read photo id"))?);
    }
    Ok(out)
}

// --------------------------------------------------------------- quarantine

/// Set a file aside with its reason, or bump the attempt counter.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the upsert fails.
pub fn quarantine_add(
    conn: &Connection,
    project_id: &str,
    import_id: Option<&str>,
    abs_path: &str,
    error_code: &str,
    detail: &str,
    now: &str,
) -> AuraResult<()> {
    conn.execute(
        "INSERT INTO quarantine
           (project_id, import_id, abs_path, error_code, detail, attempts, first_at, last_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)
         ON CONFLICT (project_id, abs_path) DO UPDATE SET
            attempts = quarantine.attempts + 1,
            error_code = excluded.error_code,
            detail = excluded.detail,
            last_at = excluded.last_at,
            resolved_at = NULL",
        params![project_id, import_id, abs_path, error_code, detail, now],
    )
    .map_err(failed("write quarantine"))?;
    Ok(())
}

/// The open Problems list: path, code and detail.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the query fails.
pub fn list_quarantine(
    conn: &Connection,
    project_id: &str,
) -> AuraResult<Vec<(String, String, String)>> {
    let mut stmt = conn
        .prepare(
            "SELECT abs_path, error_code, detail FROM quarantine
              WHERE project_id = ?1 AND resolved_at IS NULL
              ORDER BY last_at DESC",
        )
        .map_err(failed("prepare list quarantine"))?;
    let rows = stmt
        .query_map(params![project_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(failed("query list quarantine"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(failed("read quarantine row"))?);
    }
    Ok(out)
}

// -------------------------------------------------------------- import runs

/// Open an import run row and return its id.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the insert fails.
pub fn import_run_start(
    conn: &Connection,
    import_id: &str,
    project_id: &str,
    root_id: &str,
    app_version: &str,
    now: &str,
) -> AuraResult<()> {
    conn.execute(
        "INSERT INTO import_run (import_id, project_id, root_id, state, started_at, app_version)
         VALUES (?1, ?2, ?3, 'scanning', ?4, ?5)",
        params![import_id, project_id, root_id, now, app_version],
    )
    .map_err(failed("start import run"))?;
    Ok(())
}

/// Update the counters and state of an in-flight import run.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the update fails.
pub fn import_run_update(
    conn: &Connection,
    import_id: &str,
    state: &str,
    counters: ImportCounters,
) -> AuraResult<()> {
    conn.execute(
        "UPDATE import_run
            SET state = ?2,
                files_discovered = ?3,
                files_imported = ?4,
                files_duplicate = ?5,
                files_quarantined = ?6,
                bytes_hashed = ?7
          WHERE import_id = ?1",
        params![
            import_id,
            state,
            i64::try_from(counters.files_discovered).unwrap_or(i64::MAX),
            i64::try_from(counters.files_imported).unwrap_or(i64::MAX),
            i64::try_from(counters.files_duplicate).unwrap_or(i64::MAX),
            i64::try_from(counters.files_quarantined).unwrap_or(i64::MAX),
            i64::try_from(counters.bytes_hashed).unwrap_or(i64::MAX),
        ],
    )
    .map_err(failed("update import run"))?;
    Ok(())
}

/// Close an import run with its terminal state.
///
/// # Errors
///
/// Returns `AURA-DB-3006` when the update fails.
pub fn import_run_finish(
    conn: &Connection,
    import_id: &str,
    state: &str,
    error_code: Option<&str>,
    now: &str,
) -> AuraResult<()> {
    conn.execute(
        "UPDATE import_run SET state = ?2, finished_at = ?3, error_code = ?4 WHERE import_id = ?1",
        params![import_id, state, now, error_code],
    )
    .map_err(failed("finish import run"))?;
    Ok(())
}
