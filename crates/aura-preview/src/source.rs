//! Where a photograph's bytes are, and where the results are recorded.
//!
//! The preview service never writes SQL. It talks to this trait, which the
//! catalog implements, so that the scheduler can be tested against an in-memory
//! stub and so that the catalog stays the only crate that knows the schema.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_catalog::{repo, rfc3339, Catalog};
use aura_core::{AuraError, AuraResult};

use crate::contract::service::ImageId;

/// One file on disk, plus the digest that keys its cache entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// Absolute path of the primary file.
    pub path: PathBuf,
    /// BLAKE3 of its contents, hex.
    pub content_hash: String,
}

/// What the preview service needs from the catalog.
pub trait PreviewSource: Send + Sync + std::fmt::Debug {
    /// Find the primary file of a photograph.
    ///
    /// # Errors
    ///
    /// A catalog error, or `AURA-DB-3006` when the photograph has no primary
    /// file - which means ingest and previews disagree and is worth surfacing.
    fn locate(&self, id: ImageId) -> AuraResult<SourceFile>;

    /// Record that a preview now exists on disk.
    ///
    /// # Errors
    ///
    /// A catalog error.
    fn record_preview(&self, record: &PreviewRecord) -> AuraResult<()>;

    /// Record that a photograph could not be decoded.
    ///
    /// # Errors
    ///
    /// A catalog error.
    fn record_problem(&self, id: ImageId, path: &Path, error: &AuraError) -> AuraResult<()>;
}

/// One row for the `preview` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRecord {
    /// Which photograph.
    pub id: ImageId,
    /// 1, 2 or 3.
    pub tier: i64,
    /// Path of the cached artefact, relative to the cache root.
    pub rel_cache_path: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `embedded` or `decoded`.
    pub source: String,
    /// Bytes stored.
    pub bytes: u64,
    /// The rendering pipeline version that produced it.
    pub stage_version: u32,
}

/// The catalog-backed implementation.
#[derive(Debug)]
pub struct CatalogSource {
    catalog: Arc<Catalog>,
    project_id: String,
}

impl CatalogSource {
    /// Bind to one project inside an open catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, project_id: impl Into<String>) -> Self {
        Self {
            catalog,
            project_id: project_id.into(),
        }
    }
}

impl PreviewSource for CatalogSource {
    fn locate(&self, id: ImageId) -> AuraResult<SourceFile> {
        let photo_id = id.to_db();
        let found = self
            .catalog
            .read(|conn| repo::primary_file_for_photo(conn, &photo_id))?;
        let (path, content_hash) = found.ok_or_else(|| {
            aura_core::errors::db::statement_failed(
                format!("photograph {photo_id} has no primary file"),
                &std::io::Error::from(std::io::ErrorKind::NotFound),
            )
        })?;
        Ok(SourceFile {
            path: PathBuf::from(path),
            content_hash,
        })
    }

    fn record_preview(&self, record: &PreviewRecord) -> AuraResult<()> {
        let now = rfc3339(self.catalog.clock().now_utc());
        let row = repo::PreviewRow {
            photo_id: record.id.to_db(),
            tier: record.tier,
            rel_cache_path: record.rel_cache_path.clone(),
            width_px: i64::from(record.width),
            height_px: i64::from(record.height),
            source: record.source.clone(),
            bytes: i64::try_from(record.bytes).unwrap_or(i64::MAX),
            stage_version: i64::from(record.stage_version),
        };
        self.catalog
            .writer()
            .transact(move |tx| repo::upsert_preview(tx, &row, &now))
    }

    fn record_problem(&self, _id: ImageId, path: &Path, error: &AuraError) -> AuraResult<()> {
        let now = rfc3339(self.catalog.clock().now_utc());
        let project_id = self.project_id.clone();
        let abs_path = path.display().to_string();
        let code = error.code.0.to_string();
        let detail = error.user_message.clone();
        self.catalog.writer().transact(move |tx| {
            repo::quarantine_add(tx, &project_id, None, &abs_path, &code, &detail, &now)
        })
    }
}
