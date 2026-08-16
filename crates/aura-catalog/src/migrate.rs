//! Forward-only migrations with a verified backup before any structural change.

use std::path::Path;

use aura_core::clock::Clock;
use aura_core::errors::db::{
    integrity_failed, integrity_unreadable, migration_failed, open_failed_with, schema_too_new,
};
use aura_core::AuraResult;
use rusqlite::{params, Connection, TransactionBehavior};

/// The schema version this build understands.
pub const APP_SCHEMA_VERSION: i64 = 11;

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
    let current = read_version(conn)?;

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
