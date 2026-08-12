//! Process-wide state: one open catalog, plus the cancel tokens of running jobs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_cache::CacheBudget;
use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::progress::CancelToken;
use aura_core::AuraResult;
use aura_infer::contract::infer::{ExecutionProvider, HardwarePlan};
use aura_infer::ep::BackendRegistry;
use aura_infer::plan::{self, PlanPaths};
use aura_infer::probe::probe;
use aura_infer::service::InferEngine;
use aura_infer::source::ModelSource;
use aura_models::registry::{trusted_public_key, ModelRegistry};
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
    /// Where `models.lock`, its signature and the model files live.
    ///
    /// Per process rather than per project: hardware is a property of the
    /// machine and models are shared by every wedding on it.
    models_root: PathBuf,
    /// The inference runtime and the hardware plan, built on first use.
    infer: Arc<Mutex<InferSlot>>,
}

/// The lazily-built inference state.
///
/// Probing costs up to fifteen seconds and loading a model pack costs a digest
/// per file, so neither happens until something asks. A catalog that is opened
/// and closed without touching an AI feature pays nothing.
#[derive(Debug, Default)]
struct InferSlot {
    plan: Option<HardwarePlan>,
    registry: Option<Arc<ModelRegistry>>,
    engine: Option<Arc<InferEngine>>,
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
            models_root: default_models_root(),
            infer: Arc::new(Mutex::new(InferSlot::default())),
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
            models_root: default_models_root(),
            infer: Arc::new(Mutex::new(InferSlot::default())),
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

    /// Point the model registry somewhere else. Used by tests and by an
    /// installation that keeps its model pack outside the working directory.
    #[must_use]
    pub fn with_models_root(mut self, root: &Path) -> Self {
        self.models_root = root.to_path_buf();
        *self.infer.lock() = InferSlot::default();
        self
    }

    /// Where the pinned model set lives.
    #[must_use]
    pub fn models_root(&self) -> &Path {
        &self.models_root
    }

    /// The hardware plan, measured on first use and cached afterwards.
    ///
    /// A stored plan is preferred to a fresh measurement: measuring is what makes
    /// start-up slow, and the answer does not change between launches unless a
    /// driver did. An unreadable stored plan is replaced rather than repaired -
    /// it is a cache of measurements, never a source of truth.
    ///
    /// # Errors
    ///
    /// `AURA-GPU-4004` when the machine cannot be measured at all.
    pub fn hardware_plan(&self) -> AuraResult<HardwarePlan> {
        {
            let slot = self.infer.lock();
            if let Some(plan) = &slot.plan {
                return Ok(plan.clone());
            }
        }

        let paths = PlanPaths::new(&self.plan_root());
        let stored = match plan::load(&paths) {
            Ok(found) => found,
            Err(err) => {
                tracing::warn!(
                    target: "infer.plan_selected",
                    code = err.code.0,
                    "stored hardware plan was unusable; measuring again"
                );
                None
            }
        };

        let plan = if let Some(plan) = stored {
            plan
        } else {
            let outcome = probe(&BackendRegistry::with_reference(), &paths, self.clock())?;
            if outcome.persist {
                if let Err(err) = plan::save(&paths, &outcome.plan) {
                    tracing::warn!(
                        target: "infer.plan_selected",
                        code = err.code.0,
                        "hardware plan could not be written; it will be measured again"
                    );
                }
            }
            outcome.plan
        };

        let mut slot = self.infer.lock();
        slot.plan = Some(plan.clone());
        Ok(plan)
    }

    /// Measure the machine again, clearing the set-aside list first.
    ///
    /// # Errors
    ///
    /// `AURA-GPU-4004` when the probe cannot finish.
    pub fn recheck_hardware(&self) -> AuraResult<HardwarePlan> {
        let paths = PlanPaths::new(&self.plan_root());
        let outcome = probe(&BackendRegistry::with_reference(), &paths, self.clock())?;
        let mut plan = outcome.plan;
        plan::clear_set_aside(&mut plan);
        if outcome.persist {
            plan::save(&paths, &plan)?;
        }

        let mut slot = self.infer.lock();
        slot.plan = Some(plan.clone());
        // The engine holds sessions compiled for the previous provider, and the
        // plan it was built with. Both are stale now.
        slot.engine = None;
        Ok(plan)
    }

    /// Honour a user's choice of provider, or return to the negotiated order.
    ///
    /// # Errors
    ///
    /// `AURA-GPU-4004` when there is no plan to change, `AURA-GPU-4005` when the
    /// change cannot be persisted.
    pub fn set_execution_provider(
        &self,
        chosen: Option<ExecutionProvider>,
    ) -> AuraResult<HardwarePlan> {
        let mut plan = self.hardware_plan()?;
        plan.ep_override = chosen;
        plan::save(&PlanPaths::new(&self.plan_root()), &plan)?;

        let mut slot = self.infer.lock();
        slot.plan = Some(plan.clone());
        slot.engine = None;
        Ok(plan)
    }

    /// The verified model registry, opened on first use.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5002` when the manifest or its signature is missing or does not
    /// verify against the key this build trusts.
    pub fn model_registry(&self) -> AuraResult<Arc<ModelRegistry>> {
        {
            let slot = self.infer.lock();
            if let Some(registry) = &slot.registry {
                return Ok(Arc::clone(registry));
            }
        }

        // Opened outside the lock: verification digests every file the first time
        // a model is resolved, and a slow open must not block the settings panel
        // asking for the hardware plan.
        let registry = Arc::new(ModelRegistry::open(
            &self.models_root,
            &card_root(&self.models_root),
            trusted_public_key(),
        )?);

        let mut slot = self.infer.lock();
        slot.registry = Some(Arc::clone(&registry));
        Ok(registry)
    }

    /// The inference runtime, assembled on first use.
    ///
    /// # Errors
    ///
    /// Whatever the probe or the registry raised.
    pub fn infer_engine(&self) -> AuraResult<Arc<InferEngine>> {
        {
            let slot = self.infer.lock();
            if let Some(engine) = &slot.engine {
                return Ok(Arc::clone(engine));
            }
        }

        let plan = self.hardware_plan()?;
        let registry = self.model_registry()?;
        let source: Arc<dyn ModelSource> = registry;
        let engine = Arc::new(InferEngine::new(
            BackendRegistry::with_reference(),
            source,
            plan,
            Arc::clone(self.clock()),
        ));

        let mut slot = self.infer.lock();
        slot.engine = Some(Arc::clone(&engine));
        Ok(engine)
    }

    /// Where `hardware_plan.json` lives: beside the catalog, like the cache.
    fn plan_root(&self) -> PathBuf {
        self.cache_root.parent().map_or_else(
            || PathBuf::from("hardware"),
            |parent| parent.join("hardware"),
        )
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

/// Models live with the installation rather than with a catalog: one pack serves
/// every wedding on the machine, and a photographer who archives a job should not
/// take a shared model pack with it.
fn default_models_root() -> PathBuf {
    PathBuf::from("models")
}

/// Model cards are named relative to the repository or installation root, which
/// is the parent of the models directory.
fn card_root(models_root: &Path) -> PathBuf {
    models_root
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// Previews live beside the catalog by default: same drive, same backup policy,
/// and deleting the project folder takes its derived data with it.
fn default_cache_root(catalog_path: &Path) -> PathBuf {
    catalog_path
        .parent()
        .map_or_else(|| PathBuf::from("cache"), |parent| parent.join("cache"))
}
