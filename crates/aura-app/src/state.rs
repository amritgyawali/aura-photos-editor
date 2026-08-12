//! Process-wide state: one open catalog, plus the cancel tokens of running jobs.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::progress::CancelToken;
use aura_core::AuraResult;
use parking_lot::Mutex;

/// The version stamped onto rows written through the application layer.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Everything a command needs. Cheap to clone: the catalog lives behind an `Arc`.
#[derive(Debug, Clone)]
pub struct AppState {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
    jobs: Arc<Mutex<BTreeMap<String, CancelToken>>>,
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
        Ok(Self {
            catalog: Arc::new(catalog),
            clock,
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    /// Build state around an already open catalog. Used by tests.
    #[must_use]
    pub fn with_catalog(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self {
            catalog,
            clock,
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
        }
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
