//! Process-wide state: one open catalog, plus the cancel tokens of running jobs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_brain_wedding::scene::classifier::SceneClassifier;
use aura_brain_wedding::scene::ritual::RitualClassifier;
use aura_brain_wedding::scene::taxonomy::Taxonomy;
use aura_brain_wedding::story::{Story, StoryStore};
use aura_cache::CacheBudget;
use aura_catalog::consent::CatalogConsent;
use aura_catalog::Catalog;
use aura_cloud::audit::{AuditSink, CatalogAudit};
use aura_cloud::budget::{CatalogBudget, CostGovernor};
use aura_cloud::cache::{CatalogCache, ResponseCache};
use aura_cloud::keys::{KeyStore, OsKeyStore, Platform};
use aura_cloud::provider::{
    Provider, ProviderClient, ProviderConfig, ProviderKind, ThreadSleeper, Transport,
};
use aura_cloud::{CloudAiGateway, CloudPolicy};
use aura_core::clock::{Clock, SystemClock};
use aura_core::progress::CancelToken;
use aura_core::AuraResult;
use aura_index::hnsw::{HnswIndex, HnswParams};
use aura_index::snapshot::Snapshot;
use aura_index::store::{EmbeddingStore, StoredEmbedding};
use aura_index::ImageEmbedding;
use aura_infer::contract::infer::{ExecutionProvider, HardwarePlan};
use aura_infer::ep::BackendRegistry;
use aura_infer::plan::{self, PlanPaths};
use aura_infer::probe::probe;
use aura_infer::service::InferEngine;
use aura_infer::source::ModelSource;
use aura_models::registry::{trusted_public_key, ModelRegistry};
use aura_people::store::PeopleStore;
use aura_people::vault::BiometricKeyStore;
use aura_people::{FaceScanner, People};
use aura_preview::{CatalogSource, PreviewConfig, PreviewSource, Previews};
use aura_vision::embed::model::{MODEL_VER, PREPROCESS_VER};
use aura_vision::face::prominence::{ProminenceWeights, OVERRIDE_RELATIVE_PATH};
use aura_vision::face::FacePipeline;
use aura_vision::EmbeddingRunner;
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
    /// The cloud gateway and its switches, built on first use.
    cloud: Arc<Mutex<CloudSlot>>,
    /// Where the operating system's credential store keeps its blob, on the one
    /// platform that needs a file. Per installation, never per project.
    key_dir: PathBuf,
    /// One similarity index per project, built or loaded from a snapshot on first
    /// use. Per project for the same reason the preview services are: one
    /// wedding's vectors are never compared with another's, and the memory is
    /// released when a project is closed.
    indexes: Arc<Mutex<BTreeMap<String, Arc<HnswIndex>>>>,
    /// The biometric store and the service over it, built on first use.
    ///
    /// One store for the whole catalog rather than one per project, because the store
    /// is itself keyed by project everywhere it matters and it caches one derived vault
    /// per project inside. A catalog that is opened and closed without touching the
    /// People panel never reads a credential store and never derives a key.
    people: Arc<Mutex<PeopleSlot>>,
}

/// The lazily-built people state.
#[derive(Debug, Default)]
struct PeopleSlot {
    store: Option<Arc<PeopleStore>>,
    service: Option<Arc<People>>,
    /// Swapped for an in-memory store by the tests and the phase gate, so no test ever
    /// writes a biometric key to a developer's real keychain.
    keys: Option<Arc<dyn BiometricKeyStore>>,
    /// The prominence weight table in force. Loaded from the installation override on
    /// first use, falling back to the table embedded in the build.
    weights: Option<ProminenceWeights>,
}

/// The lazily-built cloud state.
///
/// Nothing here is constructed until something asks, and a catalog that is
/// opened and closed without touching an AI feature never reads a credential
/// store, never resolves a provider and never opens a socket.
#[derive(Debug)]
struct CloudSlot {
    gateway: Option<Arc<CloudAiGateway>>,
    keys: Option<Arc<dyn KeyStore>>,
    provider: ProviderKind,
    endpoint: Option<String>,
    policy: CloudPolicy,
    /// Swapped for a cassette transport by the tests and the phase gate.
    transport: Option<Arc<dyn Transport>>,
}

impl Default for CloudSlot {
    fn default() -> Self {
        Self {
            gateway: None,
            keys: None,
            provider: ProviderKind::Anthropic,
            endpoint: None,
            policy: CloudPolicy::default(),
            transport: None,
        }
    }
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
            cloud: Arc::new(Mutex::new(CloudSlot::default())),
            key_dir: default_key_dir(),
            indexes: Arc::new(Mutex::new(BTreeMap::new())),
            people: Arc::new(Mutex::new(PeopleSlot::default())),
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
            cloud: Arc::new(Mutex::new(CloudSlot::default())),
            key_dir: default_key_dir(),
            indexes: Arc::new(Mutex::new(BTreeMap::new())),
            people: Arc::new(Mutex::new(PeopleSlot::default())),
        }
    }

    /// The vector store for this catalog.
    ///
    /// Cheap: it holds two `Arc`s and no connection of its own. It is a method
    /// rather than a field because the store is stateless, and a field would invite
    /// somebody to cache one against a catalog it does not belong to.
    #[must_use]
    pub fn embedding_store(&self) -> EmbeddingStore {
        EmbeddingStore::new(Arc::clone(&self.catalog), Arc::clone(&self.clock))
    }

    /// The scene and chapter store for this catalog. PHASE-07.
    ///
    /// Stateless like the embedding store, and a method for the same reason.
    #[must_use]
    pub fn story_store(&self) -> Arc<StoryStore> {
        Arc::new(StoryStore::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ))
    }

    /// The story service, built per call.
    ///
    /// Not cached, unlike `people()`, and the difference is what the two hold. `People`
    /// caches a clustering configuration and a weight table that are expensive to
    /// rebuild; `Story` holds two parsed config files, and parsing them is under a
    /// millisecond. Caching it would buy nothing and would mean an edited
    /// `scene_profiles.toml` needed a restart.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5024` when the shipped scene profiles or ritual taxonomies will not
    /// load. Propagated rather than substituted: a build whose threshold table is broken
    /// must not open a project, because every downstream number would silently change.
    pub fn story(&self) -> AuraResult<Arc<Story>> {
        Ok(Arc::new(Story::new(
            self.story_store(),
            Arc::clone(&self.clock),
        )?))
    }

    /// The scene classifier, built on the shared inference engine.
    ///
    /// # Errors
    ///
    /// Whatever building the inference engine raised.
    pub fn scene_classifier(&self) -> AuraResult<SceneClassifier> {
        Ok(SceneClassifier::new(self.infer_engine()?))
    }

    /// The ritual head, with the shipped taxonomy attached.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5024` when the taxonomy will not load, or whatever building the
    /// inference engine raised.
    pub fn ritual_classifier(&self) -> AuraResult<RitualClassifier> {
        let taxonomy = Arc::new(Taxonomy::embedded()?);
        Ok(RitualClassifier::new(self.infer_engine()?, taxonomy))
    }

    /// The PELT penalty the last segmentation of this project settled on.
    ///
    /// Read back from the stored chapters rather than recomputed. A penalty is a
    /// property of the pass that produced *these* boundaries, and running the search
    /// again would report what a pass run now would choose - a different number about a
    /// different timeline.
    ///
    /// Zero means either "no chapters yet" or "the search fell back to time gaps", and
    /// `StoryStatusDto` distinguishes the two by also carrying the chapter count.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn story_penalty(&self, project: &aura_core::ProjectId) -> AuraResult<f32> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let found: Result<f64, rusqlite::Error> = conn.query_row(
                "SELECT penalty FROM segments WHERE project_id = ?1 ORDER BY ordinal LIMIT 1",
                rusqlite::params![key],
                |row| row.get(0),
            );
            // A penalty is a small positive number by construction - the search
            // range is 0.0005 to 40 - so the narrowing is exact for every value this
            // column can hold.
            #[allow(clippy::cast_possible_truncation)]
            Ok(found.map_or(0.0, |value| value as f32))
        })
    }

    /// One photograph's stored ritual slug.
    ///
    /// The catalog stores the slug rather than the `RitualId`, so `SceneResult::ritual`
    /// is deliberately `None` on a row read back from the database and the text is
    /// fetched separately. See ADR-0015 section 3: a catalog whose meaning depends on a
    /// config file that has since been edited is a catalog that lies.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn story_ritual_slug(&self, photo: aura_core::PhotoId) -> AuraResult<Option<String>> {
        let key = photo.to_db();
        self.catalog.read(move |conn| {
            let found: Result<Option<String>, rusqlite::Error> = conn.query_row(
                "SELECT ritual FROM image_scenes WHERE photo_id = ?1",
                rusqlite::params![key],
                |row| row.get(0),
            );
            Ok(found.unwrap_or(None))
        })
    }

    /// Everything the scene pass needs that is not a model.
    ///
    /// Assembled here rather than inside `aura-brain-wedding`, because three of the five
    /// maps come from crates that one deliberately does not link: the face counts and the
    /// couple presence are `PeopleService`'s answers, and phase 06's rule is that no
    /// phase keeps its own idea of who the couple are.
    ///
    /// A missing map is a documented neutral rather than a failure. A project with no
    /// face scan classifies perfectly well; two of its sixteen context slots sit at their
    /// midpoint and the classifier is told so.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the catalog cannot be read.
    pub fn story_pass_context(
        &self,
        project: &aura_core::ProjectId,
    ) -> AuraResult<aura_brain_wedding::PassContext> {
        let key = project.to_db();
        let mut context = aura_brain_wedding::PassContext::default();

        // EXIF, from the catalog's own columns.
        let exif = self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare("SELECT photo_id, iso, flash_fired FROM photo WHERE project_id = ?1")
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not read exif for scenes", &e)
                })?;
            let mut rows = statement.query(rusqlite::params![key]).map_err(|e| {
                aura_core::errors::db::statement_failed("could not read exif for scenes", &e)
            })?;
            let mut out: Vec<(String, Option<i64>, Option<i64>)> = Vec::new();
            while let Some(row) = rows.next().map_err(|e| {
                aura_core::errors::db::statement_failed("could not read an exif row", &e)
            })? {
                let Ok(id) = row.get::<_, String>(0) else {
                    continue;
                };
                out.push((id, row.get(1).ok().flatten(), row.get(2).ok().flatten()));
            }
            Ok(out)
        })?;
        for (id, iso, flash) in exif {
            let Ok(photo) = aura_core::PhotoId::from_db(&id) else {
                continue;
            };
            if let Some(value) = iso {
                if let Ok(iso) = u32::try_from(value) {
                    context.iso.insert(photo, iso);
                }
            }
            if let Some(value) = flash {
                context.flash.insert(photo, value != 0);
            }
        }

        // Faces per frame, from phase 06's ledger. Absent when the face pass has not
        // run, which is the ordinary case for a project that has embeddings and no faces
        // yet - and `ContextFeatures` substitutes a documented neutral for it rather
        // than a zero, because "no faces" and "nobody looked" are different.
        //
        // Read from `face_scan` rather than by counting `faces` rows: the ledger is the
        // resumability record and already holds the count, and a `COUNT(*)` join over a
        // wedding's faces to learn a number that is stored is work for nothing.
        let scan_key = project.to_db();
        let counts = self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare("SELECT photo_id, faces_found FROM face_scan WHERE project_id = ?1")
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not read face counts", &e)
                })?;
            let mut rows = statement.query(rusqlite::params![scan_key]).map_err(|e| {
                aura_core::errors::db::statement_failed("could not read face counts", &e)
            })?;
            let mut out: Vec<(String, i64)> = Vec::new();
            while let Some(row) = rows.next().map_err(|e| {
                aura_core::errors::db::statement_failed("could not read a face-count row", &e)
            })? {
                let (Ok(id), Ok(count)) = (row.get::<_, String>(0), row.get::<_, i64>(1)) else {
                    continue;
                };
                out.push((id, count));
            }
            Ok(out)
        })?;
        for (id, count) in counts {
            let Ok(photo) = aura_core::PhotoId::from_db(&id) else {
                continue;
            };
            context
                .faces
                .insert(photo, u32::try_from(count).unwrap_or(0));
        }

        Ok(context)
    }

    /// One project's embeddings, keyed by photograph, for the segmenter's first signal
    /// term.
    ///
    /// Loaded here rather than inside the segmenter for the reason
    /// `Story::segment_with_embeddings` gives: the caller usually already has them, and
    /// a segmenter that read them itself would open the table a second time.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the vectors cannot be read.
    pub fn story_embeddings(
        &self,
        project: &aura_core::ProjectId,
    ) -> AuraResult<BTreeMap<aura_core::PhotoId, Vec<f32>>> {
        let (entries, _) = self.embedding_store().load_entries(&project.to_db())?;
        Ok(entries
            .into_iter()
            .map(|entry| {
                (
                    entry.embedding.image_id,
                    entry
                        .embedding
                        .vec
                        .iter()
                        .map(|half| half.to_f32())
                        .collect(),
                )
            })
            .collect())
    }

    /// One photograph's vector, or `None` when it has not been analysed.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read.
    pub fn embedding_of(
        &self,
        project_id: &str,
        photo: aura_core::PhotoId,
    ) -> AuraResult<Option<ImageEmbedding>> {
        Ok(self
            .embedding_store()
            .load_one(project_id, photo)?
            .map(|row| row.embedding))
    }

    /// One photograph's vector and descriptors.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read.
    pub fn stored_embedding(
        &self,
        project_id: &str,
        photo: aura_core::PhotoId,
    ) -> AuraResult<Option<StoredEmbedding>> {
        self.embedding_store().load_one(project_id, photo)
    }

    /// The similarity index for one project: from a snapshot when there is a good
    /// one, from the catalog rows otherwise.
    ///
    /// The snapshot is a cache and is treated as one. A missing, stale, corrupt or
    /// differently-parameterised file is a warning (`AURA-ML-5014`) and a rebuild,
    /// never a refusal to open the project. Acceptance criterion 4 - "re-opening a
    /// project rebuilds or loads the index in under 400 ms" - is satisfied by either
    /// branch, which is why the criterion is worded that way.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the project's vectors cannot be read. A snapshot that
    /// cannot be written is logged and swallowed: the index is already in memory,
    /// and losing a cache must not lose a session.
    pub fn similarity_index(&self, project_id: &str) -> AuraResult<Arc<HnswIndex>> {
        {
            let indexes = self.indexes.lock();
            if let Some(found) = indexes.get(project_id) {
                return Ok(Arc::clone(found));
            }
        }
        let index = self.load_or_build_index(project_id)?;
        self.indexes
            .lock()
            .insert(project_id.to_string(), Arc::clone(&index));
        Ok(index)
    }

    /// Throw a project's graph and snapshot away, and build both again.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the vectors cannot be read.
    pub fn rebuild_similarity_index(&self, project_id: &str) -> AuraResult<Arc<HnswIndex>> {
        self.indexes.lock().remove(project_id);
        drop(std::fs::remove_file(self.snapshot_path(project_id)));
        self.similarity_index(project_id)
    }

    /// Where a project's graph snapshot lives.
    #[must_use]
    pub fn snapshot_path(&self, project_id: &str) -> PathBuf {
        Snapshot::path_for(&self.cache_root, project_id, MODEL_VER)
    }

    /// Load a snapshot or build the graph, then leave a fresh snapshot behind.
    fn load_or_build_index(&self, project_id: &str) -> AuraResult<Arc<HnswIndex>> {
        let path = self.snapshot_path(project_id);
        let params = HnswParams::default();

        match Snapshot::load(&path, params, MODEL_VER, PREPROCESS_VER) {
            Ok(index) => {
                tracing::info!(
                    target: "index.build",
                    vectors = index.len(),
                    ms = 0,
                    snapshot_used = true,
                    "similarity index loaded from a snapshot"
                );
                return Ok(Arc::new(index));
            }
            Err(err) => {
                // A first open has no snapshot: the common case, and not worth a
                // warning. Anything else is.
                if path.exists() {
                    tracing::warn!(target: "index", code = %err.code, "{}", err.detail);
                }
            }
        }

        let store = self.embedding_store();
        let (entries, camera_labels) = store.load_entries(project_id)?;

        // Section 12: a version mismatch is reported, never silently compared.
        let versions = store.model_versions(project_id)?;
        for stale in versions.iter().filter(|version| **version != MODEL_VER) {
            let rows = entries
                .iter()
                .filter(|entry| entry.embedding.model_ver == *stale)
                .count();
            let mismatch = aura_index::errors::embed_version_mismatch(*stale, MODEL_VER, rows);
            tracing::warn!(target: "index", code = %mismatch.code, "{}", mismatch.detail);
        }

        let vectors = entries.len();
        if vectors > aura_index::hnsw::IN_MEMORY_CEILING {
            let over =
                aura_index::errors::index_too_large(vectors, aura_index::hnsw::IN_MEMORY_CEILING);
            tracing::warn!(target: "index", code = %over.code, "{}", over.detail);
        }

        let index = HnswIndex::build(entries, params, self.clock.as_ref());
        index.set_camera_labels(&camera_labels);

        // Section 11: `index.build` {vectors, ms, snapshot_used}.
        tracing::info!(
            target: "index.build",
            vectors = index.len(),
            ms = index.build_ms(),
            snapshot_used = false,
            "similarity index built"
        );

        if !index.is_empty() {
            if let Err(err) = Snapshot::write(&index, &path, MODEL_VER, PREPROCESS_VER) {
                tracing::warn!(target: "index", code = %err.code, "{}", err.detail);
            }
        }
        Ok(Arc::new(index))
    }

    /// The embedding runner for one project.
    ///
    /// Built per call rather than cached: it holds the preview service and the
    /// inference engine, both of which are already shared, and the batch size is
    /// read from the hardware plan at construction - so a re-check of the hardware
    /// takes effect on the next pass rather than after a restart.
    ///
    /// # Errors
    ///
    /// Whatever opening the preview cache or building the inference engine raised.
    pub fn embedding_runner(&self, project_id: &str) -> AuraResult<EmbeddingRunner> {
        let previews = self.previews(project_id)?;
        let engine = self.infer_engine()?;
        Ok(EmbeddingRunner::new(
            previews,
            engine,
            Arc::new(self.embedding_store()),
            Arc::clone(&self.clock),
        ))
    }

    // -----------------------------------------------------------------------
    // PHASE-06. People.
    // -----------------------------------------------------------------------

    /// Use a different biometric key store.
    ///
    /// The tests and the phase gate pass an in-memory one, so that no test in this
    /// workspace ever writes a biometric key to a developer's real keychain. Exactly the
    /// reason `with_key_store` exists for the cloud half.
    #[must_use]
    pub fn with_biometric_keys(self, keys: Arc<dyn BiometricKeyStore>) -> Self {
        {
            let mut slot = self.people.lock();
            slot.keys = Some(keys);
            // Both are derived from the key store, so both are stale.
            slot.store = None;
            slot.service = None;
        }
        self
    }

    /// The biometric key store, built on first use.
    ///
    /// Defaults to the operating system's credential store, through the adapter in
    /// [`crate::biometric_keys`] - which is what keeps `aura-people` free of a dependency
    /// on the one crate allowed to open a socket.
    ///
    /// # Errors
    ///
    /// Never in itself; the signature matches the other accessors so a caller does not
    /// have to know which of them can fail.
    pub fn biometric_keys(&self) -> AuraResult<Arc<dyn BiometricKeyStore>> {
        {
            let slot = self.people.lock();
            if let Some(keys) = &slot.keys {
                return Ok(Arc::clone(keys));
            }
        }
        let credential_store = self.key_store()?;
        let keys: Arc<dyn BiometricKeyStore> = Arc::new(
            crate::biometric_keys::OsBiometricKeys::new(credential_store),
        );

        let mut slot = self.people.lock();
        slot.keys = Some(Arc::clone(&keys));
        Ok(keys)
    }

    /// The prominence weight table in force.
    ///
    /// The installation override at `<catalog dir>/config/prominence.toml` when there is
    /// a usable one, and the table embedded in the build otherwise. A malformed override
    /// is a warning and a fall back, never a refusal to open a project: a photographer
    /// whose wedding will not open because a QA engineer left a trailing comma is a
    /// photographer with a support ticket and a deadline.
    #[must_use]
    pub fn prominence_weights(&self) -> ProminenceWeights {
        {
            let slot = self.people.lock();
            if let Some(weights) = &slot.weights {
                return weights.clone();
            }
        }
        let path = self.cache_root.parent().map_or_else(
            || PathBuf::from(OVERRIDE_RELATIVE_PATH),
            |parent| parent.join(OVERRIDE_RELATIVE_PATH),
        );
        let (weights, refusal) = ProminenceWeights::load_or_embedded(&path);
        if let Some(reason) = refusal {
            tracing::warn!(target: "people.weights", "{reason}");
        }

        let mut slot = self.people.lock();
        slot.weights = Some(weights.clone());
        weights
    }

    /// The biometric store for this catalog, built on first use.
    ///
    /// # Errors
    ///
    /// Whatever building the key store raised.
    pub fn people_store_arc(&self) -> AuraResult<Arc<PeopleStore>> {
        {
            let slot = self.people.lock();
            if let Some(store) = &slot.store {
                return Ok(Arc::clone(store));
            }
        }
        let keys = self.biometric_keys()?;
        let store = Arc::new(PeopleStore::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
            keys,
            &self.cache_root,
        ));

        let mut slot = self.people.lock();
        slot.store = Some(Arc::clone(&store));
        Ok(store)
    }

    /// The biometric store, for the commands that read it directly.
    ///
    /// # Panics
    ///
    /// Never. Building the store cannot fail - the key store's constructor is
    /// infallible - and this accessor exists so that eleven commands do not each have to
    /// handle a `Result` that has no failure path. The fallible spelling is
    /// [`AppState::people_store_arc`], which the fallible callers use.
    #[must_use]
    pub fn people_store(&self) -> Arc<PeopleStore> {
        // Constructed inline rather than unwrapped: `people_store_arc` only fails if the
        // credential-store adapter fails to build, which it cannot, and an `unwrap` here
        // would be banned by R1 anyway.
        match self.people_store_arc() {
            Ok(store) => store,
            Err(_) => Arc::new(PeopleStore::new(
                Arc::clone(&self.catalog),
                Arc::clone(&self.clock),
                Arc::new(aura_people::MemoryKeyStore::new()),
                &self.cache_root,
            )),
        }
    }

    /// The people service, built on first use.
    #[must_use]
    pub fn people(&self) -> Arc<People> {
        {
            let slot = self.people.lock();
            if let Some(service) = &slot.service {
                return Arc::clone(service);
            }
        }
        let store = self.people_store();
        let service = Arc::new(
            People::new(store, Arc::clone(&self.clock)).with_weights(self.prominence_weights()),
        );

        let mut slot = self.people.lock();
        slot.service = Some(Arc::clone(&service));
        service
    }

    /// The face scanner for this catalog.
    ///
    /// Built per call rather than cached, for the same reason the embedding runner is: it
    /// holds the preview service and the inference engine, both of which are already
    /// shared, and the models are resolved at construction - so a re-check of the
    /// hardware takes effect on the next pass rather than after a restart.
    ///
    /// # Errors
    ///
    /// Whatever building the inference engine or the key store raised. The preview
    /// service is per project and is resolved lazily inside the scan, so this does not
    /// need a project id.
    pub fn face_scanner(&self) -> AuraResult<FaceScanner> {
        let engine = self.infer_engine()?;
        let store = self.people_store_arc()?;
        // Previews are per project, and the scanner needs one service. The catalog's
        // first project is not a safe guess, so the scanner is built against a project's
        // service by `face_scanner_for`; this spelling exists for the single-project
        // case the CLI and the gate use.
        let previews = self.first_project_previews()?;
        Ok(FaceScanner::new(
            previews,
            FacePipeline::new(engine),
            store,
            Arc::clone(&self.clock),
        ))
    }

    /// The face scanner for one project.
    ///
    /// # Errors
    ///
    /// Whatever opening the preview cache or building the inference engine raised.
    pub fn face_scanner_for(&self, project_id: &str) -> AuraResult<FaceScanner> {
        let engine = self.infer_engine()?;
        let store = self.people_store_arc()?;
        let previews = self.previews(project_id)?;
        Ok(FaceScanner::new(
            previews,
            FacePipeline::new(engine),
            store,
            Arc::clone(&self.clock),
        ))
    }

    /// How many of a project's frames earned the tiled detection pass.
    ///
    /// Section 12's fifth failure mode is "tiled detection doubles cost", and the
    /// mitigation it asks for is a measurement rather than a promise. This is the number
    /// the People panel shows.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the coverage view cannot be read.
    pub fn people_tiled_frames(&self, project: &aura_core::ProjectId) -> AuraResult<i64> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let found: Result<i64, rusqlite::Error> = conn.query_row(
                "SELECT tiled_frames FROM v_people_coverage WHERE project_id = ?1",
                rusqlite::params![key],
                |row| row.get(0),
            );
            Ok(found.unwrap_or(0))
        })
    }

    /// True when this project's biometric data has been erased.
    ///
    /// Reported rather than inferred from an empty face table, because "there are no
    /// faces" and "the faces were deliberately destroyed" are different answers to a
    /// client who asks.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the vault row cannot be read.
    pub fn people_erased(&self, project: &aura_core::ProjectId) -> AuraResult<bool> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let found: Result<Option<String>, rusqlite::Error> = conn.query_row(
                "SELECT erased_at FROM face_vault WHERE project_id = ?1",
                rusqlite::params![key],
                |row| row.get(0),
            );
            Ok(found.ok().flatten().is_some())
        })
    }

    /// The preview service of the catalog's first live project.
    ///
    /// A convenience for the single-project callers - the CLI, the phase gate, the
    /// benchmarks. A multi-project caller uses [`AppState::face_scanner_for`].
    fn first_project_previews(&self) -> AuraResult<Arc<Previews>> {
        let project = self.catalog.read(|conn| {
            let found: Result<String, rusqlite::Error> = conn.query_row(
                "SELECT project_id FROM project WHERE deleted_at IS NULL
                  ORDER BY created_at, project_id LIMIT 1",
                [],
                |row| row.get(0),
            );
            Ok(found.unwrap_or_default())
        })?;
        self.previews(&project)
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

    /// Point the credential blob directory somewhere else. Tests use this.
    #[must_use]
    pub fn with_key_dir(mut self, dir: &Path) -> Self {
        self.key_dir = dir.to_path_buf();
        *self.cloud.lock() = CloudSlot::default();
        self
    }

    /// Use a different key store. The tests and the phase gate use an in-memory
    /// one so that no test ever writes to a developer's real keychain.
    #[must_use]
    pub fn with_key_store(self, keys: Arc<dyn KeyStore>) -> Self {
        {
            let mut slot = self.cloud.lock();
            slot.keys = Some(keys);
            slot.gateway = None;
        }
        self
    }

    /// Use a different transport. The tests and the phase gate pass a cassette
    /// transport, which is how CI exercises the whole gateway with no network.
    #[must_use]
    pub fn with_cloud_transport(self, transport: Arc<dyn Transport>) -> Self {
        {
            let mut slot = self.cloud.lock();
            slot.transport = Some(transport);
            slot.gateway = None;
        }
        self
    }

    /// The credential store, built on first use.
    ///
    /// # Errors
    ///
    /// Never in itself; the signature matches the other accessors so a caller
    /// does not have to know which of them can fail.
    pub fn key_store(&self) -> AuraResult<Arc<dyn KeyStore>> {
        let mut slot = self.cloud.lock();
        if let Some(keys) = &slot.keys {
            return Ok(Arc::clone(keys));
        }
        let keys: Arc<dyn KeyStore> = Arc::new(OsKeyStore::new(&self.key_dir));
        slot.keys = Some(Arc::clone(&keys));
        Ok(keys)
    }

    /// Which credential store this machine uses, for the settings panel.
    #[must_use]
    pub fn key_store_name(&self) -> &'static str {
        Platform::host().as_str()
    }

    /// The cloud policy in force.
    #[must_use]
    pub fn cloud_policy(&self) -> CloudPolicy {
        self.cloud.lock().policy
    }

    /// Change the switches. Rebuilds the gateway on next use.
    ///
    /// # Errors
    ///
    /// Never in itself; the signature matches the other setters.
    pub fn set_cloud_policy(&self, policy: CloudPolicy) -> AuraResult<()> {
        let mut slot = self.cloud.lock();
        slot.policy = policy;
        slot.gateway = None;
        Ok(())
    }

    /// Choose the provider and, optionally, its endpoint.
    ///
    /// # Errors
    ///
    /// Never in itself; the signature matches the other setters.
    pub fn set_cloud_provider(
        &self,
        provider: ProviderKind,
        endpoint: Option<&str>,
    ) -> AuraResult<()> {
        let mut slot = self.cloud.lock();
        slot.provider = provider;
        slot.endpoint = endpoint.map(ToString::to_string);
        slot.gateway = None;
        Ok(())
    }

    /// The cloud gateway, assembled on first use.
    ///
    /// # Errors
    ///
    /// Never in itself. Every cloud failure is a degradation handled inside the
    /// gateway, so building one cannot fail; the `Result` is here so a caller
    /// does not have to know that.
    pub fn cloud(&self) -> AuraResult<Arc<CloudAiGateway>> {
        {
            let slot = self.cloud.lock();
            if let Some(gateway) = &slot.gateway {
                return Ok(Arc::clone(gateway));
            }
        }
        let keys = self.key_store()?;

        let mut slot = self.cloud.lock();
        let provider = build_provider(slot.provider, slot.endpoint.as_deref());
        let transport = slot.transport.clone().unwrap_or_else(|| {
            Arc::new(aura_cloud::http::HttpTransport::new()) as Arc<dyn Transport>
        });

        let client = Arc::new(ProviderClient::new(
            provider,
            transport,
            Arc::new(ThreadSleeper),
        ));
        let cache: Arc<dyn ResponseCache> = Arc::new(CatalogCache::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ));
        let audit: Arc<dyn AuditSink> = Arc::new(CatalogAudit::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ));
        let governor = Arc::new(CostGovernor::new(Arc::new(CatalogBudget::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ))));
        let consent = Arc::new(CatalogConsent::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ));

        let gateway = Arc::new(CloudAiGateway::new(
            client,
            keys,
            cache,
            audit,
            governor,
            consent,
            Arc::clone(&self.clock),
            slot.policy,
        ));
        slot.gateway = Some(Arc::clone(&gateway));
        Ok(gateway)
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

/// Build a provider for one vendor at one endpoint.
///
/// The endpoint is the user's when they gave one, so a region-pinned or
/// self-hosted deployment is a setting rather than a rebuild.
fn build_provider(kind: ProviderKind, endpoint: Option<&str>) -> Arc<dyn Provider> {
    match kind {
        ProviderKind::Anthropic => Arc::new(aura_cloud::anthropic::AnthropicProvider::new(
            endpoint.unwrap_or(aura_cloud::anthropic::DEFAULT_ENDPOINT),
        )),
        ProviderKind::OpenAi => Arc::new(aura_cloud::openai::OpenAiProvider::new(
            endpoint.unwrap_or(aura_cloud::openai::DEFAULT_ENDPOINT),
        )),
        ProviderKind::Google => Arc::new(aura_cloud::google::GoogleProvider::new(
            endpoint.unwrap_or(aura_cloud::google::DEFAULT_ENDPOINT),
        )),
        // A compatible server runs whatever the user loaded into it, so there is
        // no default model name worth guessing. `local-model` is what Ollama and
        // LM Studio both accept as an alias, and Settings overwrites it.
        ProviderKind::Compat => Arc::new(aura_cloud::compat::provider(
            endpoint.unwrap_or(aura_cloud::compat::DEFAULT_ENDPOINT),
            "local-model",
        )),
    }
}

/// Fold an endpoint into a configuration without rebuilding the alias table.
#[must_use]
pub fn config_at(mut config: ProviderConfig, endpoint: &str) -> ProviderConfig {
    config.endpoint = endpoint.trim_end_matches('/').to_string();
    config
}

/// Where the credential blob lives on the one platform that needs a file.
///
/// Beside the models rather than beside a catalog: a key belongs to the machine
/// and its user, not to one wedding, and a photographer who archives a project
/// folder must not archive their API key with it.
fn default_key_dir() -> PathBuf {
    PathBuf::from("credentials")
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
