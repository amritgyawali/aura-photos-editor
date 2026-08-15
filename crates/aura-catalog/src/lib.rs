#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    missing_debug_implementations,
    unreachable_pub,
    rust_2018_idioms
)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

//! The catalog is the only place AURA stores truth. Everything else - previews,
//! embeddings, renders - is a cache that can be deleted and rebuilt.
//!
//! That asymmetry drives every decision here: one writer, WAL, verified backups
//! before migrations, and a refusal to open a catalog we do not fully understand.

pub mod backup;
pub mod consent;
pub mod db;
pub mod guard;
pub mod migrate;
pub mod model;
pub mod repo;
pub mod writer;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use aura_core::clock::Clock;
use aura_core::errors::db::{open_failed_with, statement_failed};
use aura_core::paths::assert_not_cloud_synced;
use aura_core::AuraResult;
use parking_lot::Mutex;
use rusqlite::Connection;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub use crate::guard::CatalogLock;
pub use crate::migrate::APP_SCHEMA_VERSION;
pub use crate::writer::Writer;

/// Format a timestamp the way every TEXT time column in the schema expects.
#[must_use]
pub fn rfc3339(at: OffsetDateTime) -> String {
    at.format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// A small pool of read-only connections. Readers never block the writer.
#[derive(Debug)]
pub struct ReadPool {
    connections: Vec<Mutex<Connection>>,
    next: AtomicUsize,
}

impl ReadPool {
    /// Open `size` read-only connections against the same catalog file.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3001` when a connection cannot be opened.
    pub fn new(path: &Path, size: usize) -> AuraResult<Self> {
        let mut connections = Vec::with_capacity(size);
        for _ in 0..size.max(1) {
            connections.push(Mutex::new(crate::db::open_raw(path, false)?));
        }
        Ok(Self {
            connections,
            next: AtomicUsize::new(0),
        })
    }

    /// Run `f` on the next connection in round-robin order.
    ///
    /// # Errors
    ///
    /// Propagates the closure's error.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> AuraResult<T>) -> AuraResult<T> {
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.connections.len();
        let Some(slot) = self.connections.get(index) else {
            return Err(statement_failed(
                "read pool index out of range",
                &std::io::Error::from(std::io::ErrorKind::NotFound),
            ));
        };
        let guard = slot.lock();
        f(&guard)
    }
}

/// An open catalog: one writer thread, a read pool, and the instance lock.
#[derive(Debug)]
pub struct Catalog {
    writer: Writer,
    readers: ReadPool,
    path: PathBuf,
    clock: Arc<dyn Clock>,
    _lock: CatalogLock,
    _writer_thread: JoinHandle<()>,
}

impl Catalog {
    /// The full refusal chain, in the order that costs the least to fail.
    ///
    /// 1. Never open a catalog inside a sync folder (`AURA-IO-1008`).
    /// 2. Exactly one writer per catalog, enforced by the OS (`AURA-DB-3005`).
    /// 3. Open, apply pragmas, verify the file really is our catalog (`AURA-DB-3001`).
    /// 4. Integrity before we trust a single row (`AURA-DB-3003`).
    /// 5. Migrate forward only, with a verified backup first (`AURA-DB-3002`, `3004`, `3007`).
    /// 6. Only now is a writer thread allowed to exist.
    ///
    /// # Errors
    ///
    /// Any of the codes above, with the photographer-facing message already set.
    pub fn open(path: &Path, clock: Arc<dyn Clock>, app_version: &str) -> AuraResult<Self> {
        assert_not_cloud_synced(path)?;

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| open_failed_with("could not create the catalog folder", &e))?;
            }
        }

        let lock = CatalogLock::acquire(path)?;

        let mut conn = crate::db::open_raw(path, true)?;
        crate::db::assert_is_aura_catalog(&conn)?;
        crate::migrate::integrity_check(&conn)?;
        crate::migrate::migrate(&mut conn, path, clock.as_ref(), app_version)?;
        drop(conn);

        let (writer, thread) = Writer::spawn(path)?;
        let readers = ReadPool::new(path, 4)?;

        Ok(Self {
            writer,
            readers,
            path: path.to_path_buf(),
            clock,
            _lock: lock,
            _writer_thread: thread,
        })
    }

    /// The single-writer handle. Clone it freely; there is still one thread.
    #[must_use]
    pub fn writer(&self) -> &Writer {
        &self.writer
    }

    /// Borrow a read-only connection from the pool.
    ///
    /// # Errors
    ///
    /// Propagates the closure's error.
    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> AuraResult<T>) -> AuraResult<T> {
        self.readers.with(f)
    }

    /// The clock this catalog stamps rows with.
    #[must_use]
    pub fn clock(&self) -> &Arc<dyn Clock> {
        &self.clock
    }

    /// Where this catalog lives on disk.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The schema version currently applied.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3001` when the version cannot be read.
    pub fn schema_version(&self) -> AuraResult<i64> {
        self.read(crate::migrate::read_version)
    }

    /// Run the integrity and foreign-key checks against a reader connection.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3003` when either check reports a problem.
    pub fn integrity_check(&self) -> AuraResult<()> {
        self.read(crate::migrate::integrity_check)
    }

    /// Count rows in one of the schema tables. Used by tests and the support bundle.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3006` when the table name is unknown or the query fails.
    pub fn count(&self, table: &str) -> AuraResult<i64> {
        const COUNTABLE: &[&str] = &[
            "project",
            "camera",
            "photo",
            "photo_file",
            "exif",
            "source_root",
            "import_run",
            "quarantine",
            "ingest_journal",
            "task",
            "setting",
            "decision_ledger",
            "cloud_calls",
            "cloud_cache",
            "cloud_budget",
            "embeddings",
            "descriptors",
            // PHASE-06.
            "face_vault",
            "face_scan",
            "faces",
            "identities",
            "identity_links",
            "person_boxes",
            "cooccurrence",
            // PHASE-07.
            "image_scenes",
            "segments",
            "segment_images",
            "scene_profiles",
            // PHASE-08.
            "moments",
            "moment_images",
            "duplicates",
            "moment_edits",
            // PHASE-09.
            "image_integrity",
            "face_eye_state",
        ];
        if !COUNTABLE.contains(&table) {
            return Err(statement_failed(
                format!("refusing to count unknown table {table}"),
                &std::io::Error::from(std::io::ErrorKind::InvalidInput),
            ));
        }
        // The table name is checked against a fixed allow list above, so this is not
        // a string-interpolated query in the injectable sense.
        let sql = format!("SELECT COUNT(*) FROM {table}");
        self.read(|conn| {
            conn.query_row(&sql, [], |row| row.get(0))
                .map_err(|e| statement_failed(format!("count on {table} failed"), &e))
        })
    }
}
