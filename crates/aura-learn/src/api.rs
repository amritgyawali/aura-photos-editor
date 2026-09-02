//! The frozen [`LearnService`].
//!
//! Nine methods, and only two of them change anything: `adopt` and `roll_back`. Both take an
//! explicit act by a person, and there is no third.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::contract::ids::ProfileId;
use aura_core::contract::learn::{
    AbComparison, Aggregate, Consent, Correction, CorrectionContext, LearnOutline, LearnReason,
    LearnService, Learnable, LearningUpdate,
};
use aura_core::contract::ledger::ExplainService;
use aura_core::contract::scene::SceneId;
use aura_core::{AuraResult, ProjectId};

use crate::aggregate::fold;
use crate::capture::capture;
use crate::errors::no_update;
use crate::review;
use crate::rollback;
use crate::store::LearnStore;
use crate::update::{compute, Offsets};

/// The frozen [`LearnService`], over one catalog and one ledger.
#[derive(Debug, Clone)]
pub struct Learn {
    store: LearnStore,
    explain: Arc<dyn ExplainService>,
    app_version: String,
    /// The offsets each profile currently carries, supplied by the caller.
    ///
    /// Supplied rather than read, because this crate cannot see `aura-style`. A learning loop that
    /// could reach into a profile is a learning loop whose adoption step is decorative.
    current: BTreeMap<String, Offsets>,
}

impl Learn {
    /// Wrap a catalog and a ledger.
    #[must_use]
    pub fn new(
        catalog: Arc<Catalog>,
        explain: Arc<dyn ExplainService>,
        app_version: impl Into<String>,
    ) -> Self {
        Self {
            store: LearnStore::new(catalog),
            explain,
            app_version: app_version.into(),
            current: BTreeMap::new(),
        }
    }

    /// Tell the service what offsets a profile currently carries.
    #[must_use]
    pub fn with_current(mut self, profile: ProfileId, offsets: Offsets) -> Self {
        self.current.insert(profile.to_db(), offsets);
        self
    }

    /// The store, for a caller that needs to seed a snapshot.
    #[must_use]
    pub fn store(&self) -> &LearnStore {
        &self.store
    }

    /// Every bucket, aggregated, with its samples.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn folded(&self) -> AuraResult<Vec<(Aggregate, Vec<crate::aggregate::Sample>)>> {
        Ok(self
            .store
            .buckets()?
            .into_iter()
            .map(|(bucket, samples)| {
                let (agg, _) = fold(bucket, &samples);
                (agg, samples)
            })
            .collect())
    }
}

impl LearnService for Learn {
    fn outline(&self) -> AuraResult<LearnOutline> {
        self.store.outline(0)
    }

    fn capture(
        &self,
        correction: &Correction,
        context: &CorrectionContext,
    ) -> AuraResult<Vec<LearnReason>> {
        let consent = self.store.consent(context.project, &self.app_version)?;
        capture(
            &self.store,
            self.explain.as_ref(),
            correction,
            context,
            &consent,
        )
    }

    fn aggregates(&self, _profile: ProfileId) -> AuraResult<Vec<Aggregate>> {
        Ok(self.folded()?.into_iter().map(|(agg, _)| agg).collect())
    }

    fn compute(&self, profile: ProfileId) -> AuraResult<Option<LearningUpdate>> {
        let current = self
            .current
            .get(&profile.to_db())
            .cloned()
            .unwrap_or_default();
        let from = self.store.current_version(profile)?.unwrap_or(1);
        let folded = self.folded()?;
        // `Ok(None)` rather than an error when there is simply not enough evidence yet: "you have
        // not corrected enough" is the ordinary state of this feature, not a failure.
        match compute(profile, from, &current, &folded) {
            Ok(candidate) => {
                self.store
                    .write_candidate(&candidate.update, &candidate.comparison)?;
                Ok(Some(candidate.update))
            }
            Err(e) if e.code.0 == "AURA-LRN-11003" => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn compare(&self, profile: ProfileId) -> AuraResult<Option<AbComparison>> {
        Ok(self.store.candidate(profile)?.map(|(_, c)| c))
    }

    fn adopt(&self, profile: ProfileId) -> AuraResult<LearningUpdate> {
        review::adopt(&self.store, profile).map(|(update, _)| update)
    }

    fn roll_back(&self, profile: ProfileId) -> AuraResult<(u16, Vec<LearnReason>)> {
        rollback::restore(&self.store, profile)
            .map(|(restored, reasons)| (restored.version, reasons))
    }

    fn consent(&self, project: ProjectId) -> AuraResult<Consent> {
        self.store.consent(project, &self.app_version)
    }

    fn set_consent(&self, consent: &Consent) -> AuraResult<()> {
        self.store.set_consent(consent)
    }
}

/// The offsets a caller reads out of a profile, as this crate wants them.
///
/// A free function so `aura-app` can build the map from a `StyleProfile` without this crate ever
/// seeing one.
#[must_use]
pub fn offsets_from(rows: &[(Learnable, SceneId, f32)]) -> Offsets {
    let mut out = Offsets::new();
    for (learnable, scene, value) in rows {
        out.insert((*learnable, *scene), *value);
    }
    out
}

/// The error a caller meets when it asks for a candidate that does not exist.
#[must_use]
pub fn missing_candidate(profile: ProfileId) -> aura_core::AuraError {
    no_update(format!(
        "no candidate update for profile {}",
        profile.to_db()
    ))
}
