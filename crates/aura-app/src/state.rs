//! Process-wide state: one open catalog, plus the cancel tokens of running jobs.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_brain_photo::composition::{Composition, CompositionPass, CompositionStore};
use aura_brain_photo::integrity::{Integrity, IntegrityPass, IntegrityStore};
use aura_brain_photo::local::{Local, LocalPass, LocalStore};
use aura_brain_photo::tone::api::FrameExif;
use aura_brain_photo::tone::{AsShot, Tone, TonePass, ToneStore};
use aura_brain_wedding::emotion::{Emotion, EmotionPass, EmotionStore};
use aura_brain_wedding::moments::{MomentStore, Moments};
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
use aura_cull::gather::Gatherer;
use aura_cull::store::CullStore;
use aura_cull::Cull;
use aura_explain::{Explain, Ledger};
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
use rusqlite::OptionalExtension as _;

/// The version stamped onto rows written through the application layer.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Process-level operations switch for the phase 11 analysis pass.
///
/// Composition is on by default. Set this to `0`, `false`, `off` or `no`
/// before starting AURA to stop new composition analysis without hiding or
/// deleting the judgements already stored in the catalog.
pub const COMPOSITION_ENABLED_ENV: &str = "AURA_COMPOSITION_ENABLED";

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
    /// Runtime kill switch for new phase 11 analysis.
    composition_enabled: bool,
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
            composition_enabled: composition_enabled_from_env(),
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
            composition_enabled: composition_enabled_from_env(),
        }
    }

    /// Whether new phase 11 composition analysis is allowed in this process.
    #[must_use]
    pub const fn composition_enabled(&self) -> bool {
        self.composition_enabled
    }

    /// Override the process setting while assembling an embedded shell or test.
    #[must_use]
    pub fn with_composition_enabled(mut self, enabled: bool) -> Self {
        self.composition_enabled = enabled;
        self
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

    /// The moment store for this catalog. PHASE-08.
    ///
    /// Stateless like the story store, and a method for the same reason.
    #[must_use]
    pub fn moment_store(&self) -> Arc<MomentStore> {
        Arc::new(MomentStore::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ))
    }

    /// The technical-verdict store for this catalog. PHASE-09.
    ///
    /// Stateless like the moment store, and a method for the same reason.
    #[must_use]
    pub fn integrity_store(&self) -> Arc<IntegrityStore> {
        Arc::new(IntegrityStore::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ))
    }

    /// The frozen `IntegrityService` for this catalog. PHASE-09.
    ///
    /// Infallible, unlike `moments()` and `story()`: it holds no config file. The
    /// calibration table is loaded by the *pass*, which is the only half that judges
    /// anything - so a broken table stops new verdicts being made and never stops stored
    /// ones being read.
    #[must_use]
    pub fn integrity(&self) -> Arc<Integrity> {
        Arc::new(Integrity::new(self.integrity_store()))
    }

    /// The emotion store for this catalog. PHASE-10.
    ///
    /// Stateless like the moment and integrity stores, and a method for the same reason.
    #[must_use]
    pub fn emotion_store(&self) -> Arc<EmotionStore> {
        Arc::new(EmotionStore::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ))
    }

    /// The frozen `EmotionService` for this catalog. PHASE-10.
    ///
    /// Fallible, unlike `integrity()`, because it holds `emotion_weights.toml` - and a
    /// build whose emotion weights will not parse must not open a project, for the reason
    /// `Moments::new` gives about grouping thresholds. The difference from phase 09 is
    /// that phase 09's config is read only by its *pass*, and this one is read by the
    /// service too: `weights_ver` is compared against every stored row on every outline.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5039` when the weight table will not load.
    pub fn emotion(&self) -> AuraResult<Arc<Emotion>> {
        Ok(Arc::new(Emotion::new(self.emotion_store())?))
    }

    /// The emotion pass for this catalog, wired to phases 06 and 07.
    ///
    /// Both services are attached rather than optional, for `integrity_pass`'s reason:
    /// this pass is already decoding a proxy and running two heads per frame, so one
    /// `PeopleService` call and one `StoryService` call per frame are lost in the noise.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5039` when the emotion weight table will not load, or whatever building
    /// the preview service and the inference engine raised.
    pub fn emotion_pass(&self, project_id: &str) -> AuraResult<EmotionPass> {
        let pass = EmotionPass::new(
            self.previews(project_id)?,
            self.infer_engine()?,
            self.emotion_store(),
            Arc::clone(&self.clock),
        )?;
        let pass = pass.with_people(self.people());
        match self.story() {
            Ok(story) => Ok(pass.with_story(story)),
            // A wedding with no scene labels is weighted by `[default]`, which is
            // invariant 7 degraded rather than broken - so a story service that will not
            // build costs the conditioning and not the pass.
            Err(err) => {
                tracing::warn!(
                    target: "emotion.pass",
                    code = %err.code,
                    "no scene service; every frame will be weighted by the default row"
                );
                Ok(pass)
            }
        }
    }

    /// The composition store for this catalog. PHASE-11.
    ///
    /// Stateless like the integrity and emotion stores. It owns no model and opens
    /// no preview; those belong to the pass below.
    #[must_use]
    pub fn composition_store(&self) -> Arc<CompositionStore> {
        Arc::new(CompositionStore::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ))
    }

    /// The frozen `CompositionService` for this catalog. PHASE-11.
    ///
    /// Stored judgements remain readable even if the installed rule table is
    /// broken. The service logs version drift through its outline; only a new pass
    /// needs to parse the rules and therefore may refuse to start.
    #[must_use]
    pub fn composition(&self) -> Arc<Composition> {
        Arc::new(Composition::new(self.composition_store()))
    }

    /// The composition pass, wired to previews, inference, people and story.
    ///
    /// People are attached unconditionally through the phase 06 service: without
    /// them the crop audit would be absent from every frame. Story is attached when
    /// its rule tables can be opened; otherwise the pass uses its documented neutral
    /// row, records `unknown`, and lowers confidence rather than silently applying a
    /// global threshold.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5046` when the composition rule table cannot load, or whatever
    /// opening the preview service and inference engine raised.
    pub fn composition_pass(&self, project_id: &str) -> AuraResult<CompositionPass> {
        let pass = CompositionPass::new(
            self.previews(project_id)?,
            self.infer_engine()?,
            self.composition_store(),
            Arc::clone(&self.clock),
        )?
        .with_people(self.people());
        match self.story() {
            Ok(story) => Ok(pass.with_story(story)),
            Err(err) => {
                tracing::warn!(
                    target: "composition.pass",
                    code = %err.code,
                    "no scene service; every frame will be judged on the neutral rule row"
                );
                Ok(pass)
            }
        }
    }

    /// The selection store for this catalog. PHASE-12.
    ///
    /// Stateless like the integrity, emotion and composition stores.
    #[must_use]
    pub fn cull_store(&self) -> Arc<CullStore> {
        Arc::new(CullStore::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ))
    }

    /// The frozen `CullService` for this catalog. PHASE-12.
    ///
    /// Six services are offered to the gatherer and five of them are optional. Phase 09 is
    /// not: a photograph with no technical verdict is not a photograph this phase may judge,
    /// and a build that culled without one would be deciding a wedding on emotion and
    /// framing alone while reporting full coverage.
    ///
    /// The other five degrade rather than refuse. A wedding whose phase 10 pass has not run
    /// is still cullable, with the emotion sub-score neutral, a confidence penalty on every
    /// decision, and an `emotion_aware` figure in the outline that says how much of the
    /// fusion was real. That number is the point: it is how a photographer finds out that
    /// their gallery was chosen on two signals instead of four.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5051` when the weight table will not load and `AURA-ML-5052` when the
    /// coverage rules will not. Both halt: a gallery chosen without weights is not a
    /// gallery, and one chosen without guarantees is worse than none.
    pub fn cull(&self) -> AuraResult<Arc<Cull>> {
        let mut gatherer = Gatherer::new(Arc::clone(&self.catalog), self.integrity())
            .with_people(self.people())
            .with_composition(self.composition());
        match self.emotion() {
            Ok(service) => gatherer = gatherer.with_emotion(service),
            Err(err) => tracing::warn!(
                target: "cull.gather",
                code = %err.code,
                "no emotion service; every frame will be fused on a neutral emotion score"
            ),
        }
        match self.story() {
            Ok(service) => gatherer = gatherer.with_story(service),
            Err(err) => tracing::warn!(
                target: "cull.gather",
                code = %err.code,
                "no scene service; every frame will be weighted by the neutral row and                  counted against the `other` chapter"
            ),
        }
        match self.moments() {
            Ok(service) => gatherer = gatherer.with_moments(service),
            Err(err) => tracing::warn!(
                target: "cull.gather",
                code = %err.code,
                "no grouping service; every frame will be ranked on its own"
            ),
        }
        Ok(Arc::new(Cull::new(self.cull_store(), Arc::new(gatherer))?))
    }

    /// The decision ledger for this catalog. PHASE-13.
    ///
    /// Stateless like the integrity, emotion, composition and selection stores.
    #[must_use]
    pub fn ledger(&self) -> Arc<Ledger> {
        Arc::new(Ledger::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ))
    }

    /// The explainability service for this catalog. PHASE-13.
    ///
    /// Built per call rather than held, like every service above it. The band table is
    /// parsed on each construction, which costs microseconds and buys the property that an
    /// installation override takes effect without a restart.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5055` when the autonomy band table will not load. It halts, because a
    /// product that cannot say what it is allowed to do must not do anything.
    pub fn explain(&self) -> AuraResult<Arc<Explain>> {
        Ok(Arc::new(Explain::new(
            self.ledger(),
            Arc::clone(&self.clock),
        )?))
    }

    /// The technical pass for this catalog, wired to phases 06 and 07.
    ///
    /// Both services are attached rather than optional here, which is the difference
    /// between this and phase 08's `PassContext`: the integrity pass is already reading a
    /// proxy per frame, so one `PeopleService` call and one `StoryService` call per frame
    /// are lost in the noise of a 130 ms decode-and-measure. Phase 08's pass reads only
    /// catalog rows, which is why the same two calls were four thousand round trips
    /// there and are free here.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5036` when the camera calibration table will not load, or whatever
    /// building the preview service and the inference engine raised.
    pub fn integrity_pass(&self, project_id: &str) -> AuraResult<IntegrityPass> {
        let pass = IntegrityPass::new(
            self.previews(project_id)?,
            self.infer_engine()?,
            self.integrity_store(),
            Arc::clone(&self.clock),
        )?;
        let pass = pass.with_people(self.people());
        match self.story() {
            Ok(story) => Ok(pass.with_story(story)),
            // A wedding with no scene labels is judged on the neutral profile, which is
            // invariant 7 degraded rather than broken - so a story service that will not
            // build costs the conditioning and not the pass.
            Err(err) => {
                tracing::warn!(
                    target: "integrity.pass",
                    code = %err.code,
                    "no scene service; every frame will be judged on the neutral profile"
                );
                Ok(pass)
            }
        }
    }

    /// The grouping service, built per call.
    ///
    /// Not cached, for `story()`'s reason: it holds one parsed config file, parsing it
    /// is under a millisecond, and caching it would mean an edited
    /// `moment_profiles.toml` needed a restart.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5031` when the shipped grouping thresholds will not load. Propagated
    /// rather than substituted: a build whose threshold table is broken must not open a
    /// project, because every grouping decision would silently change.
    pub fn moments(&self) -> AuraResult<Arc<Moments>> {
        Ok(Arc::new(Moments::new(
            self.moment_store(),
            Arc::clone(&self.clock),
        )?))
    }

    /// The evidence the grouping pass needs from the other phases.
    ///
    /// Three of the four closures are filled and one is deliberately not.
    ///
    /// **`same_file` is real.** Phase 01 already knows which photographs share a BLAKE3
    /// digest, and it is one indexed read of `photo_file`. Section 6.3's first duplicate
    /// class is *reported* here rather than detected here, and this is how it arrives.
    ///
    /// **`segment_of` is real** when the wedding has been segmented, and returns `None`
    /// when it has not - which is why `moments.segment_id` is nullable and why grouping
    /// does not depend on phase 07 having run.
    ///
    /// **The two face closures are left at their documented neutrals**, and that is a
    /// scoping decision rather than an oversight. Feeding phase 06's
    /// `subject_focus_score` and face-box overlap in would mean either a `PeopleService`
    /// call per frame - four thousand queries on a wedding - or this crate recomputing
    /// them from `faces`, which is exactly the "no phase keeps its own idea of who is in
    /// a photograph" rule phase 06 wrote. `PeopleService` has no bulk accessor for
    /// either, and adding one is a phase 06 contract change. Condition **C4** in
    /// `docs/progress/PHASE-08-EXIT.md`.
    ///
    /// The degradation is visible rather than silent: `duplicate::keep_hint` falls
    /// through to edge energy and the set's reasons say "no face pass has run", and the
    /// face-overlap test is skipped with the skip recorded in the set's confidence.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the catalog cannot be read.
    pub fn moment_pass_context(&self) -> AuraResult<aura_brain_wedding::MomentPassContext> {
        // Which photographs share a content hash. Read once, as a map from photo to a
        // small group id, so the closure is a pair of lookups rather than a query.
        let duplicates: BTreeMap<aura_core::PhotoId, u64> = self.catalog.read(|conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id, content_hash FROM photo_file
                      WHERE role = 'primary' ORDER BY content_hash, photo_id",
                )
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not read content hashes", &e)
                })?;
            let mut rows = statement.query([]).map_err(|e| {
                aura_core::errors::db::statement_failed("could not read content hashes", &e)
            })?;
            let mut out: BTreeMap<aura_core::PhotoId, u64> = BTreeMap::new();
            let mut seen: BTreeMap<String, u64> = BTreeMap::new();
            let mut next = 0u64;
            while let Some(row) = rows.next().map_err(|e| {
                aura_core::errors::db::statement_failed("could not read a hash row", &e)
            })? {
                let (Ok(photo), Ok(hash)) = (row.get::<_, String>(0), row.get::<_, String>(1))
                else {
                    continue;
                };
                let Ok(photo) = aura_core::PhotoId::from_db(&photo) else {
                    continue;
                };
                let group = *seen.entry(hash).or_insert_with(|| {
                    next += 1;
                    next
                });
                out.insert(photo, group);
            }
            Ok(out)
        })?;

        // The chapters, as a sorted list of (start, end, id). A wedding has at most
        // twenty by construction, so the lookup is a linear scan of a tiny vector.
        let segments: Vec<(i64, i64, aura_core::SegmentId)> = self.catalog.read(|conn| {
            let mut statement = conn
                .prepare("SELECT id, start_ts, end_ts FROM segments ORDER BY start_ts")
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not read segments", &e)
                })?;
            let mut rows = statement.query([]).map_err(|e| {
                aura_core::errors::db::statement_failed("could not read segments", &e)
            })?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().map_err(|e| {
                aura_core::errors::db::statement_failed("could not read a segment row", &e)
            })? {
                let (Ok(id), Ok(start), Ok(end)) = (
                    row.get::<_, String>(0),
                    row.get::<_, i64>(1),
                    row.get::<_, i64>(2),
                ) else {
                    continue;
                };
                if let Ok(id) = aura_core::SegmentId::from_db(&id) {
                    out.push((start, end, id));
                }
            }
            Ok(out)
        })?;

        Ok(aura_brain_wedding::MomentPassContext::bare(MODEL_VER)
            .with_same_file(move |left, right| {
                match (duplicates.get(&left), duplicates.get(&right)) {
                    (Some(a), Some(b)) => a == b,
                    _ => false,
                }
            })
            .with_segments(move |when| {
                segments
                    .iter()
                    .find(|(start, end, _)| *start <= when && when <= *end)
                    .map(|(_, _, id)| *id)
            }))
    }

    /// The median inter-frame interval of a project, for the moments header.
    ///
    /// Recomputed from `photo.timeline_time` rather than stored, because it is a
    /// property of the photographs and not of a grouping run - a re-import that adds a
    /// card changes it without any moment changing.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn moment_median_interval(&self, project: &aura_core::ProjectId) -> AuraResult<i64> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT timeline_time FROM photo
                      WHERE project_id = ?1 AND timeline_time IS NOT NULL
                      ORDER BY timeline_time",
                )
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not read timeline times", &e)
                })?;
            let rows = statement
                .query_map(rusqlite::params![key], |row| row.get::<_, String>(0))
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not read timeline times", &e)
                })?;
            let mut stamps: Vec<i64> = Vec::new();
            for row in rows.flatten() {
                if let Some(ms) = aura_index::store::parse_rfc3339_ms(&row) {
                    stamps.push(ms);
                }
            }
            let mut gaps: Vec<i64> = stamps
                .windows(2)
                .filter_map(|pair| match pair {
                    [first, second] => Some(second - first),
                    _ => None,
                })
                .collect();
            if gaps.is_empty() {
                return Ok(0);
            }
            gaps.sort_unstable();
            Ok(gaps.get(gaps.len() / 2).copied().unwrap_or(0))
        })
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

    /// The local light store for this catalog. PHASE-19.
    ///
    /// Stateless like the tone, integrity, emotion, composition and cull stores. It owns no
    /// model and opens no preview; those belong to the pass below.
    #[must_use]
    pub fn local_store(&self) -> Arc<LocalStore> {
        Arc::new(LocalStore::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ))
    }

    /// The frozen `LocalService` for this catalog. PHASE-19.
    #[must_use]
    pub fn local(&self) -> Arc<Local> {
        Arc::new(Local::new(self.local_store()))
    }

    /// The local light pass, wired to previews, people, story, integrity and composition.
    /// PHASE-19.
    ///
    /// **No mask service is attached, because there is not one.** Phase 18 owns masks and has
    /// not shipped; `LocalPass::with_masks` is the input port and nothing here fills it, so
    /// every operation is gated and every plan says so. That is condition C1 of the phase 19
    /// exit report and it is visible in `LocalOutline::mask_covered` rather than hidden.
    ///
    /// People, story, integrity and composition are all attached when their tables open, and
    /// the degradation when one of them is absent is documented on the builder method rather
    /// than being silent.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5087` when the local light policy table will not load, `AURA-ML-5063` when
    /// phase 15's exposure targets will not, or whatever opening the preview service raised.
    pub fn local_pass(&self, project_id: &str) -> AuraResult<LocalPass> {
        let mut pass = LocalPass::new(
            self.previews(project_id)?,
            self.local_store(),
            Arc::clone(&self.clock),
        )?
        .with_people(self.people())
        .with_integrity(self.integrity())
        .with_composition(self.composition());
        match self.story() {
            Ok(story) => pass = pass.with_story(story),
            Err(err) => {
                tracing::warn!(
                    target: "local.pass",
                    code = %err.code,
                    "no scene service; every frame will be shaped against the neutral policy row"
                );
            }
        }
        Ok(pass)
    }

    /// The tone store for this catalog. PHASE-15.
    ///
    /// Stateless like the integrity, emotion, composition and cull stores. It owns no model
    /// and opens no preview; those belong to the pass below.
    #[must_use]
    pub fn tone_store(&self) -> Arc<ToneStore> {
        Arc::new(ToneStore::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ))
    }

    /// The frozen `ToneService` for this catalog. PHASE-15.
    ///
    /// Stored estimates stay readable even when the installed target table is broken. The
    /// service reports version drift through its outline; only a new pass has to parse the
    /// bands and may therefore refuse to start.
    #[must_use]
    pub fn tone(&self) -> Arc<Tone> {
        Arc::new(Tone::new(self.tone_store()))
    }

    /// The exposure and white-balance pass, wired to previews, inference, people, story and
    /// the catalog's own EXIF. PHASE-15.
    ///
    /// People are attached unconditionally: without them **every frame becomes faceless**, no
    /// exposure is anchored on a subject, no locus is ever built and section 1's whole
    /// improvement is gone. Story is attached when its tables open; without it every frame is
    /// estimated against the neutral target row, which is invariant 7 degraded rather than
    /// broken.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5063` when the exposure target table will not load, `AURA-ML-5036` when the
    /// camera calibration table will not, or whatever opening the preview service and the
    /// inference engine raised.
    pub fn tone_pass(&self, project_id: &str) -> AuraResult<TonePass> {
        let pass = TonePass::new(
            self.previews(project_id)?,
            self.infer_engine()?,
            self.tone_store(),
            Arc::clone(&self.clock),
        )?
        .with_people(self.people())
        .with_exif(self.frame_exif(project_id)?);
        match self.story() {
            Ok(story) => Ok(pass.with_story(story)),
            Err(err) => {
                tracing::warn!(
                    target: "tone.pass",
                    code = %err.code,
                    "no scene service; every frame will be exposed against the neutral band"
                );
                Ok(pass)
            }
        }
    }

    /// What EXIF said about every frame in a project, for the tone pass. PHASE-15.
    ///
    /// Read from the catalog rather than from the files: phase 01 owns EXIF, it is already in
    /// `photo`, and opening an original to re-read a camera tag would break invariant 1's
    /// spirit and the pass's time budget at once. A photograph whose EXIF said nothing gets a
    /// default row, and `AsShot` degrades to D65 - which is one hypothesis among five rather
    /// than an answer, so an absent tag costs a little confidence and nothing else.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the photographs cannot be read.
    pub fn frame_exif(
        &self,
        project_id: &str,
    ) -> AuraResult<BTreeMap<aura_core::PhotoId, FrameExif>> {
        let project = aura_core::ProjectId::from_db(project_id)
            .map_err(|_| {
                aura_core::errors::db::statement_failed(
                    format!("not a project id: {project_id}"),
                    &std::io::Error::from(std::io::ErrorKind::InvalidInput),
                )
            })?
            .to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id, camera_make, camera_model, iso, width_px, height_px,                             flash_fired                        FROM photo WHERE project_id = ?1 ORDER BY photo_id",
                )
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not read exif", &e)
                })?;
            let mut rows = statement.query([&project]).map_err(|e| {
                aura_core::errors::db::statement_failed("could not read exif", &e)
            })?;
            let mut out = BTreeMap::new();
            while let Some(row) = rows.next().map_err(|e| {
                aura_core::errors::db::statement_failed("could not read an exif row", &e)
            })? {
                let Ok(id) = row.get::<_, String>(0) else {
                    continue;
                };
                let Ok(photo) = aura_core::PhotoId::from_db(&id) else {
                    continue;
                };
                let width = row.get::<_, i64>(4).unwrap_or(0).max(0);
                let height = row.get::<_, i64>(5).unwrap_or(0).max(0);
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let megapixels = ((width as f64) * (height as f64) / 1_000_000.0) as f32;
                out.insert(
                    photo,
                    FrameExif {
                        make: row.get::<_, String>(1).unwrap_or_default(),
                        model: row.get::<_, String>(2).unwrap_or_default(),
                        iso: u32::try_from(row.get::<_, i64>(3).unwrap_or(0).max(0)).unwrap_or(0),
                        megapixels,
                        // The camera's own temperature is not a column in `photo`, and the
                        // `exif` key-value table stores whatever the file happened to call
                        // the tag. `AsShot::uv` answers D65 when it does not know, which is
                        // the honest fallback: one weak hypothesis among five rather than a
                        // wrong answer among four.
                        as_shot: AsShot {
                            temperature_k: 0.0,
                            tint: 0.0,
                            flash_fired: row.get::<_, i64>(6).unwrap_or(0) == 1,
                        },
                    },
                );
            }
            Ok(out)
        })
    }

    /// The edit store for this catalog. PHASE-14.
    ///
    /// Stateless like the integrity, emotion, composition and cull stores.
    #[must_use]
    pub fn recipe_store(&self) -> Arc<aura_recipe::RecipeStore> {
        Arc::new(aura_recipe::RecipeStore::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.clock),
        ))
    }

    /// The develop engine for this catalog. PHASE-14.
    ///
    /// The processor reference path, over a frame source that reads phase 02's proxies. When
    /// a `wgpu` backend lands it is constructed here and nothing else changes; the port is
    /// `aura_render::gpu::GpuBackend` and it is deliberately not frozen (ADR-0029 section 4).
    ///
    /// # Errors
    ///
    /// Never today. The signature is fallible because a backend probe can fail and changing
    /// a public signature later is worse than carrying an unused `Result` now.
    pub fn render(&self) -> AuraResult<Arc<aura_render::CpuEngine>> {
        let source: Arc<dyn aura_render::FrameSource> = Arc::new(CatalogFrames {
            catalog: Arc::clone(&self.catalog),
        });
        Ok(Arc::new(aura_render::CpuEngine::new(
            source,
            Arc::clone(&self.clock),
        )))
    }

    /// A photograph's content address, for the recipe that belongs to it.
    #[must_use]
    pub fn photo_content_hash(&self, photo: aura_core::PhotoId) -> Option<String> {
        let key = photo.to_db();
        self.catalog
            .read(move |conn| {
                conn.query_row(
                    "SELECT content_hash FROM photo_file WHERE photo_id = ?1 ORDER BY file_id LIMIT 1",
                    rusqlite::params![key],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| aura_core::errors::db::statement_failed("content hash", &e))
            })
            .ok()
            .flatten()
    }

    /// A photograph's EXIF camera model, for the profile lookup.
    #[must_use]
    pub fn photo_camera(&self, photo: aura_core::PhotoId) -> Option<String> {
        let key = photo.to_db();
        self.catalog
            .read(move |conn| {
                conn.query_row(
                    "SELECT camera_model FROM photo WHERE photo_id = ?1",
                    rusqlite::params![key],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map(Option::flatten)
                .map_err(|e| aura_core::errors::db::statement_failed("camera model", &e))
            })
            .ok()
            .flatten()
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

fn composition_enabled_from_env() -> bool {
    composition_enabled_value(std::env::var_os(COMPOSITION_ENABLED_ENV).as_deref())
}

fn composition_enabled_value(value: Option<&OsStr>) -> bool {
    !value.and_then(OsStr::to_str).is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no"
        )
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::composition_enabled_value;

    #[test]
    fn composition_is_on_unless_the_operator_explicitly_turns_it_off() {
        assert!(composition_enabled_value(None));
        assert!(composition_enabled_value(Some(OsStr::new("1"))));
        assert!(composition_enabled_value(Some(OsStr::new("true"))));
        assert!(!composition_enabled_value(Some(OsStr::new("0"))));
        assert!(!composition_enabled_value(Some(OsStr::new(" FALSE "))));
        assert!(!composition_enabled_value(Some(OsStr::new("off"))));
        assert!(!composition_enabled_value(Some(OsStr::new("No"))));
    }
}

/// The frame source the develop engine reads. PHASE-14.
///
/// A port implementation, not a contract: `aura_render::FrameSource` is deliberately not
/// frozen, so the day the proxy pipeline changes shape this is the only file that moves.
///
/// **It opens no RAW.** Phase 02's cache is what holds pixels, and a photograph whose proxy
/// has not been built yet renders as a neutral grey frame rather than as an error, because a
/// develop panel that refuses to open until the whole wedding is decoded is a develop panel
/// nobody can use on the night of a wedding.
#[derive(Debug)]
struct CatalogFrames {
    catalog: Arc<Catalog>,
}

impl aura_render::FrameSource for CatalogFrames {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn frame(
        &self,
        image: &aura_core::PhotoId,
        level: aura_render::RenderLevel,
    ) -> AuraResult<aura_render::Frame> {
        let key = image.to_db();
        let size: Option<(i64, i64)> = self
            .catalog
            .read(move |conn| {
                conn.query_row(
                    "SELECT width_px, height_px FROM photo WHERE photo_id = ?1",
                    rusqlite::params![key],
                    |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)),
                )
                .optional()
                .map(|found| found.and_then(|(w, h)| w.zip(h)))
                .map_err(|e| aura_core::errors::db::statement_failed("photo size", &e))
            })
            .unwrap_or(None);

        let (width, height) =
            size.map_or((2048, 1365), |(w, h)| (w.max(1) as u32, h.max(1) as u32));
        let edge = level.long_edge().unwrap_or(width.max(height));
        let scale = f64::from(edge) / f64::from(width.max(height).max(1));
        let out_w = ((f64::from(width) * scale).round() as u32).clamp(1, width.max(1));
        let out_h = ((f64::from(height) * scale).round() as u32).clamp(1, height.max(1));

        let key = image.to_db();
        let camera = self
            .catalog
            .read(move |conn| {
                conn.query_row(
                    "SELECT camera_model FROM photo WHERE photo_id = ?1",
                    rusqlite::params![key],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map(Option::flatten)
                .map_err(|e| aura_core::errors::db::statement_failed("camera model", &e))
            })
            .ok()
            .flatten()
            .unwrap_or_default();

        Ok(aura_render::Frame::working(
            vec![0.18f32; (out_w as usize) * (out_h as usize) * 3],
            out_w,
            out_h,
            &camera,
        ))
    }
}
