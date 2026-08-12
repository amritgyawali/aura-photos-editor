//! Connection construction and the pragmas every connection must carry.

use std::path::Path;

use aura_core::errors::db::{open_failed, open_failed_with};
use aura_core::AuraResult;
use rusqlite::{Connection, OpenFlags};

/// Applied to every connection, read or write. Order matters: `journal_mode`
/// must be set before the first write, and `busy_timeout` before any contention.
///
/// # Errors
///
/// Returns `AURA-DB-3001` when a pragma cannot be applied.
pub fn apply_pragmas(conn: &Connection, writable: bool) -> AuraResult<()> {
    let statements: &[&str] = &[
        // Durability and concurrency
        "PRAGMA journal_mode = WAL",
        "PRAGMA synchronous = FULL",
        "PRAGMA foreign_keys = ON",
        "PRAGMA busy_timeout = 10000",
        // Performance
        "PRAGMA cache_size = -65536",   // 64 MB page cache
        "PRAGMA mmap_size = 268435456", // 256 MB
        "PRAGMA temp_store = MEMORY",
        "PRAGMA wal_autocheckpoint = 2000", // ~8 MB at 4 KB pages
        // Safety
        "PRAGMA trusted_schema = OFF",
        "PRAGMA recursive_triggers = ON",
    ];

    for sql in statements {
        conn.execute_batch(sql)
            .map_err(|e| open_failed_with(format!("pragma failed: {sql}"), &e))?;
    }

    if !writable {
        conn.execute_batch("PRAGMA query_only = ON")
            .map_err(|e| open_failed_with("could not mark connection read-only", &e))?;
    }
    Ok(())
}

/// Open one connection with the pragmas applied.
///
/// # Errors
///
/// Returns `AURA-DB-3001` when the file cannot be opened.
pub fn open_raw(path: &Path, writable: bool) -> AuraResult<Connection> {
    let flags = if writable {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;

    let conn = Connection::open_with_flags(path, flags).map_err(|e| {
        open_failed_with(format!("could not open catalog at {}", path.display()), &e)
    })?;
    apply_pragmas(&conn, writable)?;
    Ok(conn)
}

/// Refuse a file that opened cleanly but is not one of our catalogs.
///
/// A fresh, empty file is accepted: the migration runner creates the schema.
///
/// # Errors
///
/// Returns `AURA-DB-3001` when the file has tables but none of ours.
pub fn assert_is_aura_catalog(conn: &Connection) -> AuraResult<()> {
    let table_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| open_failed_with("could not read the catalog table list", &e))?;

    if table_count == 0 {
        return Ok(()); // A new catalog. The migration runner will populate it.
    }

    let ours: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name IN ('schema_version', 'catalog_meta')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| open_failed_with("could not read the catalog table list", &e))?;

    if ours < 2 {
        return Err(open_failed(
            "the file is a database but has no AURA schema tables",
        ));
    }
    Ok(())
}
