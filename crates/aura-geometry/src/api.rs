//! The frozen service and the resumable pass. PHASE-23.
//!
//! Two things live here: [`Geometry`], which is the one implementation of
//! [`GeometryService`], and [`GeometryPass`], which walks a project.
//!
//! ## The pass is resumable because the work remaining is a query
//!
//! Invariant 5. There is no journal, no cursor and no checkpoint file: `GeometryStore::pending`
//! asks the catalog which photographs have no plan at the current three versions, and a run
//! killed at 10, 50 or 90 per cent resumes by asking again. A `profile_ver` bump therefore
//! heals itself, and so does a `rules_ver` bump - which is what makes editing
//! `crop_rules.toml` a safe thing for a product manager to do.
//!
//! ## The pass opens no file phase 11 has not opened
//!
//! Invariant 3's medium tier: the 2048 px proxy, the same rung phases 09, 11, 15, 16 and 19
//! measure on. Straight edges, converging verticals and edge distractions are geometry rather
//! than detail, and a geometry pass that decoded a full-resolution frame to find a horizon
//! would be spending forty times the budget for the same answer.

use std::fmt;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::error::{AuraError, AuraResult};
use aura_core::contract::geometry::{
    CropPurpose, CropVariant, GeometryOutline, GeometryOverride, GeometryPlan, GeometryService,
    ImageId,
};
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::ProjectId;

use crate::guard;
use crate::plan::{GeometryInput, Planner, ANALYSIS_VER};
use crate::profiles::ProfileTable;
use crate::rules::CropRules;
use crate::store::GeometryStore;

/// The stage name this pass reports progress under.
pub const PASS_STAGE: &str = "geometry.plan";

/// The one implementation of [`GeometryService`].
pub struct Geometry {
    store: GeometryStore,
    planner: Planner,
}

impl fmt::Debug for Geometry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Geometry")
            .field("profiles", &self.planner.profiles().len())
            .field("rules", &self.planner.rules().len())
            .finish_non_exhaustive()
    }
}

impl Geometry {
    /// Open the service over a catalog, a profile table and a rules file.
    #[must_use]
    pub fn new(
        catalog: Arc<Catalog>,
        clock: Arc<dyn Clock>,
        profiles: ProfileTable,
        rules: CropRules,
    ) -> Self {
        Self {
            store: GeometryStore::new(catalog, clock),
            planner: Planner::new(profiles, rules),
        }
    }

    /// Open the service with the profile table and rules this build ships.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5093` when either file will not load.
    pub fn shipped(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> AuraResult<Self> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(crate::profiles::PROFILE_DIR);
        Ok(Self::new(
            catalog,
            clock,
            ProfileTable::load_dir(&dir)?,
            CropRules::shipped()?,
        ))
    }

    /// The three versions this build plans at.
    #[must_use]
    pub fn versions(&self) -> (u16, u16, u16) {
        (
            self.planner.profiles().version(),
            ANALYSIS_VER,
            self.planner.rules().version(),
        )
    }

    /// The planner underneath, for the pass and the gate.
    #[must_use]
    pub const fn planner(&self) -> &Planner {
        &self.planner
    }

    /// The store underneath, for the pass and the gate.
    #[must_use]
    pub const fn store(&self) -> &GeometryStore {
        &self.store
    }

    /// Plan one photograph and store it.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5092` when the plan breaks one of this phase's guarantees, and
    /// `AURA-DB-3006` when it cannot be written.
    pub fn plan_and_store(&self, input: &GeometryInput) -> Result<GeometryPlan, AuraError> {
        let plan = self.planner.plan(input);
        guard::check_plan(&plan)?;
        self.store.put(&plan)?;
        Ok(plan)
    }

    /// Record that the photographer has looked at one plan and agrees.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5091` when the photograph has no plan.
    pub fn accept(&self, image: ImageId) -> Result<(), AuraError> {
        self.store.accept(image)
    }
}

impl GeometryService for Geometry {
    fn outline(&self, project: ProjectId) -> AuraResult<GeometryOutline> {
        let mut outline = self.store.outline(&project, self.versions())?;
        outline.missing_profiles.truncate(GeometryOutline::MAX_MISSING);
        Ok(outline)
    }

    fn of_image(&self, image: ImageId) -> AuraResult<Option<GeometryPlan>> {
        self.store.get(image)
    }

    fn variant(&self, image: ImageId, purpose: CropPurpose) -> AuraResult<Option<CropVariant>> {
        self.store.variant(image, purpose)
    }

    fn needs_review(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<ImageId>> {
        self.store.needs_review(&project, limit)
    }

    fn set_framing(&self, over: GeometryOverride) -> Result<(), AuraError> {
        guard::check_override(&over)?;
        self.store.set_framing(over)
    }
}

/// What one run of the pass did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PassOutcome {
    /// Photographs planned.
    pub planned: u32,
    /// Photographs that could not be planned. `AURA-ML-5092`, one at a time.
    pub failed: u32,
    /// Photographs skipped because the photographer had framed them.
    pub protected: u32,
    /// True when the run was cancelled part-way.
    pub cancelled: bool,
}

/// The resumable walk.
///
/// Generic over how an input is built, so the pass can be driven from a real preview service in
/// the application and from authored fixtures in the gate without either of them being a
/// special case of the other.
pub struct GeometryPass<'a> {
    service: &'a Geometry,
    progress: &'a dyn ProgressSink,
    cancel: &'a CancelToken,
}

impl fmt::Debug for GeometryPass<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeometryPass").finish_non_exhaustive()
    }
}

impl<'a> GeometryPass<'a> {
    /// Build a pass.
    #[must_use]
    pub fn new(
        service: &'a Geometry,
        progress: &'a dyn ProgressSink,
        cancel: &'a CancelToken,
    ) -> Self {
        Self {
            service,
            progress,
            cancel,
        }
    }

    /// Walk everything in a project that has no current plan.
    ///
    /// `build` is asked for one photograph's input; returning `None` counts the frame as
    /// failed and moves on, which is what a proxy that will not decode does.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the pending set cannot be read.
    pub fn run(
        &self,
        project: ProjectId,
        mut build: impl FnMut(ImageId) -> Option<GeometryInput>,
    ) -> AuraResult<PassOutcome> {
        let pending = self.service.store.pending(&project, self.service.versions())?;
        let total = pending.len() as u64;
        let mut outcome = PassOutcome::default();
        for (index, image) in pending.into_iter().enumerate() {
            if self.cancel.is_cancelled() {
                outcome.cancelled = true;
                break;
            }
            match build(image) {
                None => outcome.failed += 1,
                Some(input) => match self.service.plan_and_store(&input) {
                    Ok(_) => outcome.planned += 1,
                    Err(err) => {
                        tracing::warn!(photo = %image.to_db(), code = %err.code.0, "geometry");
                        outcome.failed += 1;
                    }
                },
            }
            self.progress.report(ProgressUpdate {
                stage: PASS_STAGE,
                done: index as u64 + 1,
                total,
                current: None,
            });
        }
        Ok(outcome)
    }
}
