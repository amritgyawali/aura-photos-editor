//! `aura-cli replay` - ask a stored decision again, and say whether the answer moved.
//!
//! Section 2.1: "`aura replay <decision_id>` re-runs a decision from the ledger and asserts
//! an identical outcome." This is that command, and it is the support tool the whole phase is
//! built around: a complaint about a wedding delivered eight months ago becomes a question
//! with an answer.
//!
//! ## What "re-run" means for a culling decision
//!
//! The decision is rebuilt from the catalog **as it stands now**: today's stored selection,
//! today's sub-scores, today's config and model versions, through the same
//! `aura_explain::adapt` path that produced the row in the first place. Then the two are
//! compared - the inputs hash first, the outputs second - and
//! `aura_explain::replay::compare` says which of the three things happened.
//!
//! It re-derives rather than re-culling, and the difference is worth stating. Phase 12's
//! engine is pure and every operation that could change a gallery - the slider, a mode
//! change, an override - re-runs all six passes and rewrites the selection. So the stored
//! selection *is* what the engine currently produces for this project, and re-running it
//! here would compute the same rows at the cost of six passes over four thousand frames
//! inside section 11's one-second replay budget.
//!
//! ## What it can find
//!
//! * **Identical.** The question and the answer are both unchanged.
//! * **Drifted.** The inputs hash moved: a model, a config table or an analysis pass changed
//!   underneath the decision. Expected after an upgrade.
//! * **Nondeterministic.** The inputs hash is identical and the decision is not. That is a
//!   defect, it breaks invariant 4, and `AURA-ML-5057` says so in those words.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::cull::Decision as CullDecision;
use aura_core::contract::ids::DecisionId;
use aura_core::contract::ledger::{DecisionSubject, LedgerDecision};
use aura_core::{AuraResult, ProjectId};
use aura_cull::store::CullStore;
use aura_explain::adapt::{self, CullContext};
use aura_explain::replay::{Recomputed, ReplaySource, Replayer};
use aura_explain::{Bundle, BundleOptions, Ledger};

/// Re-derives a culling decision from the catalog as it stands now.
#[derive(Debug)]
pub struct StoredSelectionSource {
    store: Arc<CullStore>,
}

impl StoredSelectionSource {
    /// A source over one catalog's stored selection.
    #[must_use]
    pub const fn new(store: Arc<CullStore>) -> Self {
        Self { store }
    }

    /// The context a fresh decision is built with: today's versions, today's hash.
    fn context(&self, project: ProjectId) -> AuraResult<CullContext> {
        let outline = self.store.outline(project)?;
        Ok(CullContext {
            model_ver: outline.model_ver,
            analysis_ver: outline.analysis_ver,
            score_calibration_ver: outline.calibration_ver,
            run_hash: outline.deterministic_hash,
            ..CullContext::default()
        })
    }
}

impl ReplaySource for StoredSelectionSource {
    fn recompute(&self, decision: &LedgerDecision) -> AuraResult<Recomputed> {
        let Some(image) = decision.subject.image() else {
            return Err(aura_core::errors::db::statement_failed(
                "only decisions about one photograph can be replayed in this build",
                &std::io::Error::from(std::io::ErrorKind::Unsupported),
            ));
        };
        let Some(current): Option<CullDecision> = self.store.decision(image)? else {
            return Err(aura_core::errors::db::statement_failed(
                "this photograph is no longer part of a stored selection",
                &std::io::Error::from(std::io::ErrorKind::NotFound),
            ));
        };
        let context = self.context(decision.project)?;
        let fresh = adapt::cull_decision(decision.project, &current, &context)
            // The id and the timestamp are deliberately the stored ones: neither is in the
            // inputs hash and neither is in the canonical outputs, and using a fresh id here
            // would make the two rows look different in a diff for no reason.
            .build(decision.id, decision.created_at);
        Ok(Recomputed {
            inputs_hash: fresh.inputs_hash,
            outputs_json: fresh.outputs_json,
        })
    }
}

/// Run `aura-cli replay`.
///
/// ```text
/// aura-cli replay --catalog FILE --decision dcn_...
/// aura-cli replay --catalog FILE --project prj_...        # every decision in the wedding
/// aura-cli replay --catalog FILE --project prj_... --bundle FILE
/// ```
pub fn run(args: &[String]) -> ExitCode {
    let Some(catalog_path) = crate::flag(args, "--catalog") else {
        eprintln!("replay: --catalog FILE is required");
        return ExitCode::FAILURE;
    };
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog = match Catalog::open(
        &PathBuf::from(catalog_path),
        Arc::clone(&clock),
        crate::APP_VERSION,
    ) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };

    let ledger = Ledger::new(Arc::clone(&catalog), Arc::clone(&clock));
    let store = Arc::new(CullStore::new(Arc::clone(&catalog), Arc::clone(&clock)));
    let source = StoredSelectionSource::new(store);
    let replayer = Replayer::new(&source);

    let decisions = match collect(args, &ledger) {
        Ok(found) => found,
        Err(err) => {
            eprintln!("replay: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    if decisions.is_empty() {
        eprintln!("replay: no decisions matched");
        return ExitCode::FAILURE;
    }

    let mut identical = 0usize;
    let mut drifted = 0usize;
    let mut broken = 0usize;
    for decision in &decisions {
        match replayer.replay(decision) {
            Ok(outcome) => {
                match outcome.as_str() {
                    "identical" => identical += 1,
                    "drifted" => drifted += 1,
                    _ => broken += 1,
                }
                if decisions.len() == 1 || !outcome.is_identical() {
                    println!(
                        "{} {} {}",
                        decision.id.to_db(),
                        outcome.as_str(),
                        describe(decision)
                    );
                }
            }
            Err(err) => {
                broken += 1;
                eprintln!("{}: [{}] {}", decision.id.to_db(), err.code, err.detail);
            }
        }
    }

    println!("replay: {identical} identical, {drifted} drifted after an upgrade, {broken} failed");

    if let Some(path) = crate::flag(args, "--bundle") {
        if let Err(err) = write_bundle(&ledger, &decisions, &path) {
            eprintln!("bundle: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
        println!("bundle: {path}");
    }

    // A drift is not a failure: an upgrade is supposed to decide differently. A
    // nondeterministic replay is, and so is one that could not run.
    if broken == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Which decisions the command was asked about.
fn collect(args: &[String], ledger: &Ledger) -> AuraResult<Vec<LedgerDecision>> {
    if let Some(text) = crate::flag(args, "--decision") {
        let Ok(id) = DecisionId::from_db(&text) else {
            return Err(aura_core::errors::db::statement_failed(
                format!("`{text}` is not a decision id"),
                &std::io::Error::from(std::io::ErrorKind::InvalidInput),
            ));
        };
        return Ok(ledger.get(id)?.into_iter().collect());
    }
    if let Some(text) = crate::flag(args, "--project") {
        let Ok(project) = ProjectId::from_db(&text) else {
            return Err(aura_core::errors::db::statement_failed(
                format!("`{text}` is not a project id"),
                &std::io::Error::from(std::io::ErrorKind::InvalidInput),
            ));
        };
        return ledger.project(project);
    }
    if let Some(text) = crate::flag(args, "--photo") {
        let Ok(photo) = aura_core::PhotoId::from_db(&text) else {
            return Err(aura_core::errors::db::statement_failed(
                format!("`{text}` is not a photo id"),
                &std::io::Error::from(std::io::ErrorKind::InvalidInput),
            ));
        };
        return ledger.history(DecisionSubject::Image(photo));
    }
    Err(aura_core::errors::db::statement_failed(
        "replay needs --decision, --photo or --project",
        &std::io::Error::from(std::io::ErrorKind::InvalidInput),
    ))
}

/// The one-line description a support engineer reads beside the verdict.
fn describe(decision: &LedgerDecision) -> String {
    format!(
        "{} about {} at {:.2} ({}), question {:016x}",
        decision.kind.as_str(),
        decision.subject,
        decision.calibrated_confidence,
        decision.autonomy.as_str(),
        decision.inputs_hash
    )
}

/// Write the anonymised support bundle beside the replay.
fn write_bundle(ledger: &Ledger, decisions: &[LedgerDecision], path: &str) -> AuraResult<()> {
    let Some(first) = decisions.first() else {
        return Ok(());
    };
    let outline = ledger.outline(first.project)?;
    let bundle = Bundle::build(decisions, &outline, &BundleOptions::default())?;
    if !bundle.is_safe() {
        return Err(aura_core::errors::db::statement_failed(
            format!("the bundle would have carried {:?}", bundle.scan()),
            &std::io::Error::from(std::io::ErrorKind::InvalidData),
        ));
    }
    std::fs::write(path, bundle.json.as_bytes()).map_err(|err| {
        aura_core::errors::db::statement_failed("could not write the support bundle", &err)
    })
}
