//! One writable catalog per machine, enforced by the operating system.

use std::fs::{File, OpenOptions};
use std::path::Path;

use aura_core::errors::db::second_writer;
use aura_core::AuraResult;
use fs4::fs_std::FileExt;

/// Held for the lifetime of a writable catalog. Dropping it releases the lock.
#[derive(Debug)]
pub struct CatalogLock {
    file: File,
}

impl CatalogLock {
    /// Exclusive advisory lock on `<catalog>.lock`. We do not lock the catalog
    /// file itself: `SQLite` manages that, and taking our own lock on it would
    /// fight with the write-ahead log.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3005` when another process already holds the lock, or
    /// when the lock file cannot be created.
    pub fn acquire(catalog_path: &Path) -> AuraResult<Self> {
        let lock_path = catalog_path.with_extension("lock");
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| second_writer("cannot create the catalog folder").with_cause(&e))?;
        }

        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| second_writer("cannot create lock file").with_cause(&e))?;

        file.try_lock_exclusive()
            .map_err(|e| second_writer("catalog lock held by another process").with_cause(&e))?;

        Ok(Self { file })
    }
}

impl Drop for CatalogLock {
    fn drop(&mut self) {
        // Best effort: the kernel also releases the lock when the process exits,
        // which is what keeps a crashed instance from blocking the next launch.
        let _ = FileExt::unlock(&self.file);
    }
}
