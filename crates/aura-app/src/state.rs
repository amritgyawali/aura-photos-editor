//! Process-wide state: one open catalog, plus the cancel tokens of running jobs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_cache::CacheBudget;
use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::progress::CancelToken;
use aura_core::AuraResult;
use aura_preview::{CatalogSource, PreviewConfig, PreviewSource, Previews};
use parking_lot::Mutex;

/// The version stamped onto rows written through the application layer.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Everything a command needs. Cheap to clone: the catalog lives behind an `Arc`.
#[derive(Debug, Clone)]
pub struct AppState {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
    jobs: Arc<Mutex<BTreeMap<String, CancelToken>>>,
    /// One preview service per project, created on first use. Keeping them
    /// separate keeps each wedding's cache accounting - and its budget - its
    /// own, which is what a photographer expects when they archive one job.
    previews: Arc<Mutex<BTreeMap<String, Arc<Previews>>>>,
    cache_root: PathBuf,
}

impl AppState {
    /// Open a catalog and build the state around it.
    ///
    /// # Errors
    ///
    /// Propagates the catalog open refusals, which are already photographer-facing.
    pub fn open(catalog_path: &Path) -> AuraResult<Self> {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
        let catalog = Catalog::open(catalog_path, Arc::clone(&clock), APP_VERSION)?;
        let cache_root = default_cache_root(catalog_path);
        Ok(Self {
            catalog: Arc::new(catalog),
            clock,
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
            previews: Arc::new(Mutex::new(BTreeMap::new())),
            cache_root,
        })
    }

    /// Build state around an already open catalog. Used by tests.
    #[must_use]
    pub fn with_catalog(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        let cache_root = default_cache_root(catalog.path());
        Self {
            catalog,
            clock,
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
            previews: Arc::new(Mutex::new(BTreeMap::new())),
            cache_root,
        }
    }

    /// Point the preview cache somewhere else. Used by tests and by the
    /// "move my cache to the fast SSD" setting.
    #[must_use]
    pub fn with_cache_root(mut self, root: &Path) -> Self {
        self.cache_root = root.to_path_buf();
        self.previews.lock().clear();
        self
    }

    /// Where cached previews live.
    #[must_use]
    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    /// The preview service for one project, created on first use.
    ///
    /// # Errors
    ///
    /// `AURA-IO-1009` when the cache directory cannot be created.
    pub fn previews(&self, project_id: &str) -> AuraResult<Arc<Previews>> {
        let mut services = self.previews.lock();
        if let Some(found) = services.get(project_id) {
            return Ok(Arc::clone(found));
        }
        let source: Arc<dyn PreviewSource> = Arc::new(CatalogSource::new(
            Arc::clone(&self.catalog),
            project_id.to_string(),
        ));
        let service = Arc::new(Previews::open(
            &self.cache_root.join(project_id),
            CacheBudget::default(),
            source,
            PreviewConfig::default(),
        )?);
        services.insert(project_id.to_string(), Arc::clone(&service));
        Ok(service)
    }

    /// The open catalog.
    #[must_use]
    pub fn catalog(&self) -> &Arc<Catalog> {
        &self.catalog
    }

    /// The clock every command stamps rows with.
    #[must_use]
    pub fn clock(&self) -> &Arc<dyn Clock> {
        &self.clock
    }

    /// Register a cancel token for a running job.
    pub fn register_job(&self, job_id: &str, token: CancelToken) {
        self.jobs.lock().insert(job_id.to_string(), token);
    }

    /// Signal cancellation. Returns false when the job is already gone.
    #[must_use]
    pub fn cancel_job(&self, job_id: &str) -> bool {
        match self.jobs.lock().get(job_id) {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    /// Forget a finished job.
    pub fn finish_job(&self, job_id: &str) {
        self.jobs.lock().remove(job_id);
    }
}

/// Previews live beside the catalog by default: same drive, same backup policy,
/// and deleting the project folder takes its derived data with it.
fn default_cache_root(catalog_path: &Path) -> PathBuf {
    catalog_path
        .parent()
        .map_or_else(|| PathBuf::from("cache"), |parent| parent.join("cache"))
}
