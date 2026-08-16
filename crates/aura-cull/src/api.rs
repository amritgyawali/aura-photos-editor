//! `CullService`, and the four operations a photographer actually performs.
//!
//! Reading is cheap and comes straight from the store. The other three - resize, mode
//! change and override - all go through the same path: re-gather, re-run the passes,
//! re-store. That is one code path rather than three, and it is what makes section 6.4's
//! guarantee hold for all of them at once: whatever a photographer does, the coverage guard
//! runs afterwards.
//!
//! ## Why a slider move re-runs the whole selection
//!
//! Section 6.4 says the slider "re-runs only the allocation passes (not analysis), so it
//! feels instant". That is exactly what happens - the analysis is phases 06 to 11 and none
//! of it runs here. What this crate calls "the whole selection" is six passes over numbers
//! that are already in the catalog, and section 11 budgets 1.5 seconds for it over four
//! thousand frames.
//!
//! Re-running the fusion as well as the allocation costs a few milliseconds and buys
//! something worth more than them: there is no second, cheaper, subtly different code path
//! that a future change can make disagree with the first one.
//!
//! ## An override is a write, then a re-selection
//!
//! [`Cull::override_decision`] records the row and then re-runs. It has to: a forced keep
//! changes a chapter's remaining quota, and a forced reject can leave a guarantee short. A
//! version of this that only flipped a row would produce a gallery whose numbers no longer
//! added up and whose coverage panel was quietly wrong.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_core::contract::cull::{
    CullMode, CullOutline, CullService, Decision, ImageId, SelectionResult,
};
use aura_core::{AuraError, AuraResult, ProjectId};

use crate::engine::{Engine, PassReport, Request};
use crate::errors;
use crate::gather::Gatherer;
use crate::rules::RuleTable;
use crate::store::{CullStore, Provenance};
use crate::weights::WeightTable;

/// The one implementation of [`CullService`].
#[derive(Debug)]
pub struct Cull {
    store: Arc<CullStore>,
    gatherer: Arc<Gatherer>,
    weights: Arc<WeightTable>,
    rules: Arc<RuleTable>,
}

impl Cull {
    /// A culling service over one catalog.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5051` or `AURA-ML-5052` when either configuration table will not load.
    /// Neither is recoverable here and both halt: a gallery chosen without weights is not a
    /// gallery, and one chosen without guarantees is worse than none.
    pub fn new(store: Arc<CullStore>, gatherer: Arc<Gatherer>) -> Result<Self, AuraError> {
        Ok(Self {
            store,
            gatherer,
            weights: Arc::new(WeightTable::embedded()?),
            rules: Arc::new(RuleTable::embedded()?),
        })
    }

    /// The same service with installation overrides already loaded.
    #[must_use]
    pub fn with_tables(
        store: Arc<CullStore>,
        gatherer: Arc<Gatherer>,
        weights: Arc<WeightTable>,
        rules: Arc<RuleTable>,
    ) -> Self {
        Self {
            store,
            gatherer,
            weights,
            rules,
        }
    }

    /// Choose a gallery for one project and store it.
    ///
    /// The entry point the IPC surface, the CLI gate and the two re-selection methods all
    /// share.
    ///
    /// # Errors
    ///
    /// Whatever the gatherer or the store raised.
    pub fn run(
        &self,
        project: ProjectId,
        mode: CullMode,
        target: Option<u32>,
    ) -> AuraResult<(SelectionResult, PassReport)> {
        let (mut input, context) = self.gatherer.gather(project)?;
        self.gatherer.apply_vetoes(&mut input, &self.rules)?;
        let overrides = self.store.overrides(project)?;

        let engine = Engine::new(&self.weights, &self.rules);
        let request = Request {
            mode,
            target,
            overrides,
        };
        let (result, report) = engine.select(input, &request);

        let mut required = BTreeMap::new();
        for rule in self.rules.rules() {
            required.insert(rule.id, rule.min);
        }
        let provenance = Provenance {
            config_digest: engine.config_digest(),
            candidates: report.rule_candidates.clone(),
            found: report.rule_found.clone(),
            required,
            warnings: report.rule_warnings.clone(),
            report: report.clone(),
        };
        self.store.put(&project, &result, &provenance, &context)?;
        Ok((result, report))
    }

    /// Which project a photograph is in, or a refusal naming it.
    fn project_of(&self, image: ImageId) -> Result<ProjectId, AuraError> {
        self.store.project_of(image)?.ok_or_else(|| {
            errors::cull_edit_refused("photograph", "is not in any project in this catalog")
        })
    }
}

impl CullService for Cull {
    fn outline(&self, project: ProjectId) -> AuraResult<CullOutline> {
        self.store.outline(project)
    }

    fn selection(&self, project: ProjectId) -> AuraResult<Option<SelectionResult>> {
        self.store.selection(project)
    }

    fn of_image(&self, image: ImageId) -> AuraResult<Option<Decision>> {
        self.store.decision(image)
    }

    fn resize(&self, project: ProjectId, target: u32) -> Result<SelectionResult, AuraError> {
        let outline = self.store.outline(project)?;
        if outline.is_empty() {
            return Err(errors::cull_edit_refused(
                "size",
                "this wedding has not been culled yet, so there is nothing to resize",
            ));
        }
        let (result, _) = self.run(project, outline.mode, Some(target))?;
        Ok(result)
    }

    fn set_mode(&self, project: ProjectId, mode: CullMode) -> Result<SelectionResult, AuraError> {
        let outline = self.store.outline(project)?;
        if outline.is_empty() {
            return Err(errors::cull_edit_refused(
                "mode",
                "this wedding has not been culled yet, so there is nothing to re-choose",
            ));
        }
        // The target is deliberately not carried across a mode change. A mode's whole
        // purpose is to change how many frames the wedding deserves, and keeping the old
        // count would make `Aggressive` and `Conservative` produce galleries of identical
        // size that differed only in which frames were in them.
        let (result, _) = self.run(project, mode, None)?;
        Ok(result)
    }

    fn override_decision(&self, image: ImageId, keep: bool) -> Result<(), AuraError> {
        let project = self.project_of(image)?;
        let outline = self.store.outline(project)?;
        if outline.is_empty() {
            return Err(errors::cull_edit_refused(
                "choice",
                "this wedding has not been culled yet, so there is nothing to change",
            ));
        }
        self.store.set_override(&project, image, keep)?;
        self.run(project, outline.mode, Some(outline.selected))?;
        Ok(())
    }

    fn clear_override(&self, image: ImageId) -> Result<(), AuraError> {
        let project = self.project_of(image)?;
        if !self.store.clear_override(image)? {
            return Err(errors::cull_edit_refused(
                "choice",
                "there was no keep or removal recorded for this photograph",
            ));
        }
        let outline = self.store.outline(project)?;
        if !outline.is_empty() {
            self.run(project, outline.mode, Some(outline.selected))?;
        }
        Ok(())
    }
}
