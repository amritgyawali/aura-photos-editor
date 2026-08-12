//! Verified backups. An unverified backup is not a backup.

use std::path::{Path, PathBuf};
use std::time::Duration;

use aura_core::clock::Clock;
use aura_core::errors::db::{backup_unverified, open_failed_with};
use aura_core::AuraResult;
use rusqlite::backup::Backup;
use rusqlite::Connection;

/// How many pre-migration backups to keep beside the catalog.
pub const KEEP_BACKUPS: usize = 5;

/// Create `backups/pre-<version>-<timestamp>.sqlite`, then open the copy and
/// verify it.
///
/// # Errors
///
/// Returns `AURA-DB-3007` when the copy cannot be proven identical and intact,
/// and `AURA-DB-3001` when the copy cannot be created at all.
pub fn create_verified(
    conn: &Connection,
    catalog: &Path,
    from_version: i64,
    clock: &dyn Clock,
) -> AuraResult<PathBuf> {
    let dir = catalog
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("backups");
    std::fs::create_dir_all(&dir)
        .map_err(|e| open_failed_with("could not create the backups folder", &e))?;

    let stamp = clock.now_utc().unix_timestamp();
    let dest_path = dir.join(format!("pre-{from_version}-{stamp}.sqlite"));
    let mut dest = Connection::open(&dest_path)
        .map_err(|e| open_failed_with("could not create the backup file", &e))?;

    {
        let backup = Backup::new(conn, &mut dest)
            .map_err(|e| open_failed_with("could not start the backup", &e))?;
        backup
            .run_to_completion(256, Duration::from_millis(1), None)
            .map_err(|e| open_failed_with("the backup copy failed", &e))?;
    }

    // Verify: integrity_check plus a row-count comparison on the largest table.
    let check: String = dest
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|e| open_failed_with("could not verify the backup", &e))?;
    if check != "ok" {
        drop(dest);
        let _ = std::fs::remove_file(&dest_path);
        return Err(backup_unverified(format!(
            "backup integrity check said {check}"
        )));
    }

    let src_files: i64 = conn
        .query_row("SELECT COUNT(*) FROM photo_file", [], |row| row.get(0))
        .unwrap_or(-1);
    let dst_files: i64 = dest
        .query_row("SELECT COUNT(*) FROM photo_file", [], |row| row.get(0))
        .unwrap_or(-2);
    if src_files != dst_files {
        drop(dest);
        let _ = std::fs::remove_file(&dest_path);
        return Err(backup_unverified(
            "row count mismatch between catalog and backup",
        ));
    }

    prune(&dir, KEEP_BACKUPS);
    Ok(dest_path)
}

/// Keep the newest `keep` backups. A disk full of backups is its own failure.
fn prune(dir: &Path, keep: usize) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("pre-"))
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    if entries.len() > keep {
        let excess = entries.len() - keep;
        for old in entries.iter().take(excess) {
            let _ = std::fs::remove_file(old.path());
        }
    }
}
