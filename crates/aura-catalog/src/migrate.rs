//! Forward-only migrations with a verified backup before any structural change.

use std::path::Path;

use aura_core::clock::Clock;
use aura_core::errors::db::{
    integrity_failed, integrity_unreadable, migration_failed, open_failed_with, schema_too_new,
};
use aura_core::AuraResult;
use rusqlite::{params, Connection, TransactionBehavior};

/// The schema version this build understands.
pub const APP_SCHEMA_VERSION: i64 = 30;

/// Every migration is (version, name, sql). Embedded so a shipped binary can
/// never disagree with its own migrations.
///
/// Versions 2 and 3 do not exist. Phases 02 and 03 added no tables - previews,
/// model files and hardware plans are all caches on disk rather than catalog
/// truth - and phase 04's file name `0004_cloud_audit.sql` is fixed by the phase
/// document, as phase 05's `0005_embeddings.sql` is. The loop below skips any
/// version at or below the applied one, so a gap costs nothing and renumbering a
/// published migration file would cost a great deal.
const MIGRATIONS: &[(i64, &str, &str)] = &[
    (1, "init", include_str!("../migrations/0001_init.sql")),
    (
        4,
        "cloud_audit",
        include_str!("../migrations/0004_cloud_audit.sql"),
    ),
    (
        5,
        "embeddings",
        include_str!("../migrations/0005_embeddings.sql"),
    ),
    (6, "people", include_str!("../migrations/0006_people.sql")),
    (7, "scenes", include_str!("../migrations/0007_scenes.sql")),
    (8, "moments", include_str!("../migrations/0008_moments.sql")),
    (
        9,
        "integrity",
        include_str!("../migrations/0009_integrity.sql"),
    ),
    (
        10,
        "emotion",
        include_str!("../migrations/0010_emotion.sql"),
    ),
    (
        11,
        "composition",
        include_str!("../migrations/0011_composition.sql"),
    ),
    (
        12,
        "selection",
        include_str!("../migrations/0012_selection.sql"),
    ),
    (13, "ledger", include_str!("../migrations/0013_ledger.sql")),
    (
        14,
        "develop",
        include_str!("../migrations/0014_develop.sql"),
    ),
    (15, "tone", include_str!("../migrations/0015_tone.sql")),
    (16, "colour", include_str!("../migrations/0016_colour.sql")),
    (17, "style", include_str!("../migrations/0017_style.sql")),
    (18, "masks", include_str!("../migrations/0018_masks.sql")),
    (
        19,
        "local_light",
        include_str!("../migrations/0019_local_light.sql"),
    ),
    (
        20,
        "geometry",
        include_str!("../migrations/0020_geometry.sql"),
    ),
    (
        21,
        "retouch",
        include_str!("../migrations/0021_retouch.sql"),
    ),
    (
        22,
        "micro_retouch",
        include_str!("../migrations/0022_micro_retouch.sql"),
    ),
    (
        23,
        "restoration",
        include_str!("../migrations/0023_restoration.sql"),
    ),
    (
        24,
        "cleanup",
        include_str!("../migrations/0024_cleanup.sql"),
    ),
    (
        25,
        "gallery",
        include_str!("../migrations/0025_gallery.sql"),
    ),
    (
        26,
        "camera_match",
        include_str!("../migrations/0026_camera_match.sql"),
    ),
    (27, "qc", include_str!("../migrations/0027_qc.sql")),
    (
        28,
        "autopilot",
        include_str!("../migrations/0028_autopilot.sql"),
    ),
    (
        29,
        "curation",
        include_str!("../migrations/0029_curation.sql"),
    ),
    (
        30,
        "delivery",
        include_str!("../migrations/0030_delivery.sql"),
    ),
];

/// Bring a catalog up to [`APP_SCHEMA_VERSION`].
///
/// # Errors
///
/// * `AURA-DB-3004` when the catalog is newer than this build.
/// * `AURA-DB-3007` when the pre-migration backup cannot be verified.
/// * `AURA-DB-3002` when a migration statement fails; the transaction rolls back.
/// * `AURA-DB-3003` when the post-migration integrity check fails.
pub fn migrate(
    conn: &mut Connection,
    path: &Path,
    clock: &dyn Clock,
    app_version: &str,
) -> AuraResult<()> {
    let mut current = read_version(conn)?;

    // Refusal: a catalog from a newer app version must not be opened. Opening it
    // read-only and guessing would silently drop columns we do not know about.
    if current > APP_SCHEMA_VERSION {
        return Err(schema_too_new(current, APP_SCHEMA_VERSION));
    }
    if current == APP_SCHEMA_VERSION {
        return Ok(());
    }

    // Verified backup before any structural change. Verification means opening the
    // copy and running integrity_check, not merely checking the file exists.
    if current > 0 {
        let backup = crate::backup::create_verified(conn, path, current, clock)?;
        tracing::info!(target: "catalog", backup = %backup.display(), "pre-migration backup verified");
    }

    current = reconcile_legacy_phase_numbering(conn, current, clock, app_version)?;

    for (version, name, sql) in MIGRATIONS {
        if *version <= current {
            continue;
        }
        let hash = blake3::hash(sql.as_bytes()).to_hex().to_string();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| migration_failed(*version, &e))?;
        tx.execute_batch(sql)
            .map_err(|e| migration_failed(*version, &e))?;
        tx.execute(
            "INSERT OR REPLACE INTO schema_version (version, applied_at, app_version, migration_hash)
             VALUES (?1, ?2, ?3, ?4)",
            params![version, crate::rfc3339(clock.now_utc()), app_version, hash],
        )
        .map_err(|e| migration_failed(*version, &e))?;
        tx.commit().map_err(|e| migration_failed(*version, &e))?;
        tracing::info!(target: "catalog", version, name, "migration applied");
    }

    seed_catalog_meta(conn, clock, app_version)?;

    // Post-migration integrity check. A migration that leaves a corrupt index is
    // worse than one that fails loudly.
    integrity_check(conn)?;
    Ok(())
}

/// Repair catalogs created before geometry took schema version 20.
///
/// Retouch, micro-retouch and restoration briefly shipped as versions 20 through
/// 22 on one branch. Geometry later became canonical version 20 and those three
/// migrations moved to 21 through 23. A catalog written by that earlier build
/// therefore reports version 20 while already containing `retouch_plan`; blindly
/// applying canonical migration 21 then fails because that table already exists.
///
/// The caller has already made and verified a backup. Re-number existing provenance
/// rows and add geometry in one transaction so either the whole repair lands or none
/// of it does. Object presence distinguishes the legacy lineage from a canonical
/// version-20 catalog without trusting a mutable application version string.
fn reconcile_legacy_phase_numbering(
    conn: &mut Connection,
    current: i64,
    clock: &dyn Clock,
    app_version: &str,
) -> AuraResult<i64> {
    if !(20..=22).contains(&current) {
        return Ok(current);
    }

    let has_retouch =
        schema_object_exists(conn, "retouch_plan").map_err(|error| migration_failed(20, &error))?;
    let has_geometry = schema_object_exists(conn, "geometry_plan")
        .map_err(|error| migration_failed(20, &error))?;
    if !has_retouch || has_geometry {
        return Ok(current);
    }

    let Some(geometry_sql) = migration_sql(20) else {
        // Unreachable while migration 20 is registered, and a refusal rather than a
        // panic because this runs during `open` on a photographer's own catalog. R1
        // bans the panic family in library code even where it cannot fire.
        return Err(missing_migration(20));
    };
    let geometry_hash = blake3::hash(geometry_sql.as_bytes()).to_hex().to_string();
    let applied_at = crate::rfc3339(clock.now_utc());

    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| migration_failed(20, &error))?;
    tx.execute_batch(geometry_sql)
        .map_err(|error| migration_failed(20, &error))?;

    for legacy_version in (20..=current).rev() {
        let canonical_version = legacy_version + 1;
        let Some(canonical_sql) = migration_sql(canonical_version) else {
            return Err(missing_migration(canonical_version));
        };
        let canonical_hash = blake3::hash(canonical_sql.as_bytes()).to_hex().to_string();
        tx.execute(
            "UPDATE schema_version
                SET version = ?1, migration_hash = ?2
              WHERE version = ?3",
            params![canonical_version, canonical_hash, legacy_version],
        )
        .map_err(|error| migration_failed(canonical_version, &error))?;
    }

    tx.execute(
        "INSERT INTO schema_version (version, applied_at, app_version, migration_hash)
         VALUES (20, ?1, ?2, ?3)",
        params![applied_at, app_version, geometry_hash],
    )
    .map_err(|error| migration_failed(20, &error))?;
    tx.commit().map_err(|error| migration_failed(20, &error))?;

    let repaired = current + 1;
    tracing::info!(
        target: "catalog",
        legacy_version = current,
        schema_version = repaired,
        "legacy migration numbering reconciled"
    );
    Ok(repaired)
}

/// A migration this build should carry and does not.
///
/// Rolled up as a migration failure rather than as its own code: from a
/// photographer's point of view it is one, and the recovery - stop, keep the
/// catalog as it is, ask for help - is identical.
fn missing_migration(version: i64) -> aura_core::AuraError {
    migration_failed(version, &rusqlite::Error::InvalidQuery)
        .with_context("detail", format!("migration {version} is not registered in this build"))
}

/// The SQL of one registered migration, by version.
fn migration_sql(version: i64) -> Option<&'static str> {
    MIGRATIONS
        .iter()
        .find(|(registered, _, _)| *registered == version)
        .map(|(_, _, sql)| *sql)
}

fn schema_object_exists(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = ?1)",
        [name],
        |row| row.get(0),
    )
}

/// Read the highest applied schema version, or zero for an empty file.
///
/// # Errors
///
/// Returns `AURA-DB-3001` when the version table cannot be queried.
pub fn read_version(conn: &Connection) -> AuraResult<i64> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_version'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| open_failed_with("could not read the schema version", &e))?;
    if exists == 0 {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |r| r.get(0),
    )
    .map_err(|e| open_failed_with("could not read the schema version", &e))
}

/// Run `PRAGMA integrity_check` and the foreign-key check.
///
/// # Errors
///
/// Returns `AURA-DB-3003` when either check reports a problem.
pub fn integrity_check(conn: &Connection) -> AuraResult<()> {
    let result: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| integrity_unreadable(&e))?;
    if result != "ok" {
        return Err(integrity_failed(format!("integrity_check said {result}")));
    }

    let violations: i64 = conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    if violations > 0 {
        return Err(integrity_failed(format!(
            "{violations} foreign key violations"
        )));
    }
    Ok(())
}

/// Seed the identity rows a fresh catalog needs. Idempotent.
fn seed_catalog_meta(conn: &Connection, clock: &dyn Clock, app_version: &str) -> AuraResult<()> {
    let now = crate::rfc3339(clock.now_utc());
    let catalog_uuid = uuid_like(&now, app_version);
    conn.execute(
        "INSERT OR IGNORE INTO catalog_meta (key, value) VALUES
           ('catalog_uuid', ?1), ('created_at', ?2), ('created_by_app_version', ?3)",
        params![catalog_uuid, now, app_version],
    )
    .map_err(|e| open_failed_with("could not seed catalog metadata", &e))?;
    Ok(())
}

/// A stable catalog identifier derived from creation facts, so two catalogs
/// created in the same millisecond by different builds still differ.
fn uuid_like(now: &str, app_version: &str) -> String {
    blake3::hash(format!("{now}|{app_version}").as_bytes())
        .to_hex()
        .chars()
        .take(32)
        .collect()
}

#[cfg(test)]
mod tests {
    use aura_core::clock::FixedClock;
    use time::OffsetDateTime;

    use super::*;

    #[test]
    fn legacy_phase_numbering_is_reconciled_without_losing_catalog_data() {
        let dir = tempfile::tempdir().expect("temporary catalog directory");
        let path = dir.path().join("legacy.sqlite");
        let mut conn = crate::db::open_raw(&path, true).expect("open legacy catalog");

        for (version, _, sql) in MIGRATIONS.iter().filter(|(version, _, _)| *version <= 19) {
            apply_as_version(&mut conn, sql, *version);
        }
        conn.execute(
            "INSERT INTO project
                (project_id, name, couple_label, event_date, timezone, status, created_at, updated_at)
             VALUES ('legacy-project', 'Preserved wedding', NULL, NULL, 'UTC', 'active',
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("seed legacy project");

        // These schemas were identical before they moved from 20..22 to 21..23.
        for (legacy_version, canonical_version) in [(20, 21), (21, 22), (22, 23)] {
            let sql = MIGRATIONS
                .iter()
                .find(|(version, _, _)| *version == canonical_version)
                .expect("canonical migration")
                .2;
            apply_as_version(&mut conn, sql, legacy_version);
        }

        assert!(schema_object_exists(&conn, "retouch_plan").expect("retouch lookup"));
        assert!(!schema_object_exists(&conn, "geometry_plan").expect("geometry lookup"));

        let clock = FixedClock::at(OffsetDateTime::UNIX_EPOCH + time::Duration::days(20_000));
        migrate(&mut conn, &path, clock.as_ref(), "0.1.0-test").expect("legacy catalog migrates");

        assert_eq!(
            read_version(&conn).expect("schema version"),
            APP_SCHEMA_VERSION
        );
        assert!(schema_object_exists(&conn, "geometry_plan").expect("geometry lookup"));
        let projects: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM project WHERE project_id = 'legacy-project'",
                [],
                |row| row.get(0),
            )
            .expect("preserved project count");
        assert_eq!(projects, 1);

        for canonical_version in 20..=23 {
            let sql = MIGRATIONS
                .iter()
                .find(|(version, _, _)| *version == canonical_version)
                .expect("canonical migration")
                .2;
            let expected = blake3::hash(sql.as_bytes()).to_hex().to_string();
            let actual: String = conn
                .query_row(
                    "SELECT migration_hash FROM schema_version WHERE version = ?1",
                    [canonical_version],
                    |row| row.get(0),
                )
                .expect("migration provenance");
            assert_eq!(actual, expected, "schema version {canonical_version}");
        }

        integrity_check(&conn).expect("reconciled catalog integrity");
    }

    fn apply_as_version(conn: &mut Connection, sql: &str, version: i64) {
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("migration transaction");
        tx.execute_batch(sql).expect("migration SQL");
        tx.execute(
            "INSERT INTO schema_version (version, applied_at, app_version, migration_hash)
             VALUES (?1, '2026-01-01T00:00:00Z', 'legacy', 'legacy-hash')",
            [version],
        )
        .expect("schema version row");
        tx.commit().expect("migration commit");
    }
}
