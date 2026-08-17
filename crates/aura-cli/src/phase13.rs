//! The phase 13 mechanical gate.
//!
//! This is the assembly proof for explainability: migration 13 and its six objects, the
//! shipped band table, the reason registry against the four vocabularies that feed it, a real
//! selection recorded into the ledger, the append-only trigger, a supersession, a compaction
//! that keeps the photographer's own decision, a replay, the calibration estimator, the
//! grounding check, a support bundle and the two section 11 budgets.
//!
//! **Nothing here proves the confidence numbers.** Every calibration model in this build is
//! the identity map, and the outcomes the estimator is measured against are synthetic with a
//! known answer. What passes is the machinery; the distinction is printed at the end of every
//! run rather than hidden in a test helper.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::cull::{CullMode, Decision as CullDecision};
use aura_core::contract::ids::DecisionId;
use aura_core::contract::ledger::{
    Autonomy, DecisionKind, DecisionSource, DecisionSubject, ExplainService,
};
use aura_core::{AuraResult, ProjectId};
use aura_cull::engine::{Engine, Request};
use aura_cull::fixtures;
use aura_cull::rules::RuleTable;
use aura_cull::store::{CullStore, Provenance, RowContext};
use aura_cull::weights::WeightTable;
use aura_explain::adapt::{self, CullContext};
use aura_explain::calibration::{Calibrator, Reliability, ECE_GATE};
use aura_explain::fixtures as ledger_fixtures;
use aura_explain::fixtures::OutcomeShape;
use aura_explain::policy::{AutonomyPolicy, Risk};
use aura_explain::reason::Catalog as ReasonCatalog;
use aura_explain::replay::Replayer;
use aura_explain::summary::{Grounding, Summariser};
use aura_explain::{Bundle, BundleOptions, Explain, Ledger};
use rusqlite::params;

use crate::replay::StoredSelectionSource;

/// Section 11: the Explain panel opens, with its crops, in a quarter of a second.
const PANEL_BUDGET_MS: u128 = 250;

/// Section 11: one decision replays in a second.
const REPLAY_BUDGET_MS: u128 = 1_000;

/// Section 11: the ledger costs six megabytes per thousand images.
const LEDGER_BYTES_PER_THOUSAND: u64 = 6 * 1024 * 1024;

/// Run the phase 13 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase13-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. Migration 13 and every object it owns.
    let catalog_path = work.join("phase13.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 13 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 13, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "decisions"),
        ("table", "decision_reasons"),
        ("table", "calibration_models"),
        ("trigger", "decisions_no_update"),
        ("view", "v_ledger_coverage"),
        ("index", "idx_decisions_project"),
        ("index", "idx_decisions_subject"),
        ("index", "idx_decisions_supersedes"),
    ] {
        match schema_object(&catalog, kind, name) {
            Ok(true) => println!("  {kind} {name}: present"),
            Ok(false) => {
                eprintln!("  {kind} {name}: missing");
                failures += 1;
            }
            Err(err) => {
                eprintln!("  {kind} {name}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 2. The product decision loads, and it descends.
    let policy = match AutonomyPolicy::embedded() {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("bands: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    println!(
        "config: autonomy bands v{} with {} per-kind rows, multipliers {:?}",
        policy.version(),
        policy.rows(),
        policy.multipliers()
    );
    for kind in DecisionKind::ALL {
        let bands = policy.bands(kind);
        if !bands.is_ordered() {
            eprintln!("  {}: thresholds do not descend", kind.as_str());
            failures += 1;
        }
    }
    if policy.decide(DecisionKind::Cull, 0.99, Risk::must_have()) == Autonomy::Auto {
        eprintln!("  a must-have decision was not raised a band");
        failures += 1;
    } else {
        println!("  a must-have decision is raised one band");
    }
    for kind in DecisionKind::ALL {
        if kind.is_irreversible()
            && policy.decide(kind, 1.0, Risk::NONE.for_kind(kind)) == Autonomy::Auto
        {
            eprintln!("  {} acts unattended at full confidence", kind.as_str());
            failures += 1;
        }
    }

    // 3. The registry is the vocabulary, and the reference is the registry.
    let registry = ReasonCatalog::shipped();
    let mut missing = Vec::new();
    for code in aura_core::contract::integrity::ReasonCode::ALL {
        if !registry.contains(code.as_str()) {
            missing.push(code.as_str());
        }
    }
    for code in aura_core::contract::emotion::EmotionCode::ALL {
        if !registry.contains(code.as_str()) {
            missing.push(code.as_str());
        }
    }
    for code in aura_core::contract::composition::CompositionCode::ALL {
        if !registry.contains(code.as_str()) {
            missing.push(code.as_str());
        }
    }
    for code in aura_core::CullCode::ALL {
        if !registry.contains(code.as_str()) {
            missing.push(code.as_str());
        }
    }
    if missing.is_empty() {
        println!(
            "registry: {} codes, every emittable vocabulary covered",
            registry.len()
        );
    } else {
        eprintln!("registry: {missing:?} can be emitted and are not documented");
        failures += 1;
    }

    // 4. A real selection, recorded, replayed, superseded, compacted and bundled.
    match end_to_end(&catalog, Arc::clone(&clock), &policy) {
        Ok(report) => {
            for line in report.lines {
                println!("{line}");
            }
            failures += report.failures;
        }
        Err(err) => {
            eprintln!("ledger: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 5. Calibration: the estimator recovers a known error, and the fit fixes it.
    let calibrated = Reliability::measure(&ledger_fixtures::outcomes(
        OutcomeShape::Calibrated,
        4_000,
        7,
    ));
    let overconfident = Reliability::measure(&ledger_fixtures::outcomes(
        OutcomeShape::Overconfident,
        4_000,
        11,
    ));
    println!(
        "calibration: ECE {:.4} on a calibrated predictor, {:.4} on an overconfident one \
         (gate {ECE_GATE})",
        calibrated.ece, overconfident.ece
    );
    if calibrated.ece > ECE_GATE {
        eprintln!("  a calibrated predictor measured above the gate");
        failures += 1;
    }
    if overconfident.passes_gate() {
        eprintln!("  an overconfident predictor passed the gate");
        failures += 1;
    }
    let training = ledger_fixtures::outcomes(OutcomeShape::Overconfident, 4_000, 3);
    let model = Calibrator::fit_isotonic(&training, 1);
    let held_out = ledger_fixtures::outcomes(OutcomeShape::Overconfident, 4_000, 99);
    let mapped: Vec<_> = held_out
        .iter()
        .map(|outcome| {
            aura_explain::calibration::Outcome::new(model.apply(outcome.raw), outcome.correct)
        })
        .collect();
    let after = Reliability::measure(&mapped);
    println!(
        "  isotonic fit: {} knots, held-out ECE {:.4} -> {:.4}",
        model.knots.len(),
        overconfident.ece,
        after.ece
    );
    if after.ece > ECE_GATE {
        eprintln!("  the fitted calibration did not bring held-out ECE under the gate");
        failures += 1;
    }

    println!();
    println!("What this gate proves: the machinery. Decisions cannot be recorded without a");
    println!("reason, a reason cannot cite a code nothing documents, the ledger cannot be");
    println!("updated, a summary cannot invent a number and a bundle cannot carry an id.");
    println!();
    println!("What it does not prove: that AURA's confidence means anything yet. Every");
    println!("calibration model in this build is the identity map at version 0, every outcome");
    println!("above is synthetic, and every sub-score underneath every decision comes from a");
    println!("placeholder head. Condition C1 and C2 of the phase 13 exit report.");

    if failures == 0 {
        println!();
        println!("phase 13: PASS");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("phase 13: {failures} FAILED");
        ExitCode::FAILURE
    }
}

/// What the end-to-end pass found.
struct Report {
    lines: Vec<String>,
    failures: usize,
}

/// Cull a synthetic wedding, record it, and put the ledger through everything it promises.
#[allow(clippy::too_many_lines)]
fn end_to_end(
    catalog: &Arc<Catalog>,
    clock: Arc<dyn Clock>,
    policy: &AutonomyPolicy,
) -> AuraResult<Report> {
    let mut lines = Vec::new();
    let mut failures = 0usize;

    let weights = WeightTable::embedded()?;
    let rules = RuleTable::embedded()?;
    let project = ProjectId::new();
    let now = aura_catalog::rfc3339(clock.now_utc());
    catalog.writer().transact({
        let row = ProjectRow {
            project_id: project.to_db(),
            name: "phase 13 gate".to_string(),
            couple_label: None,
            event_date: None,
            timezone: "UTC".to_string(),
            status: "active".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        move |conn| repo::create_project(conn, &row)
    })?;

    // A real wedding, culled by the real engine.
    let wedding = fixtures::sparse_elopement();
    let engine = Engine::new(&weights, &rules);
    let request = Request {
        mode: CullMode::Balanced,
        target: None,
        overrides: std::collections::BTreeMap::new(),
    };
    let (result, report) = engine.select(wedding.input.clone(), &request);

    let ids: Vec<String> = wedding
        .input
        .candidates
        .iter()
        .map(|candidate| candidate.image_id.to_db())
        .collect();
    let project_key = project.to_db();
    let stamp = now.clone();
    catalog.writer().transact(move |conn| {
        let mut statement = conn
            .prepare(
                "INSERT INTO photo (photo_id, project_id, capture_time_source, orientation,
                                    timeline_time, created_at, updated_at)
                 VALUES (?1, ?2, 'exif_original', 1, ?3, ?4, ?4)",
            )
            .map_err(|e| aura_core::errors::db::statement_failed("prepare photo insert", &e))?;
        for id in &ids {
            statement
                .execute(params![id, project_key, stamp, stamp])
                .map_err(|e| aura_core::errors::db::statement_failed("insert photo", &e))?;
        }
        Ok(())
    })?;

    let cull_store = Arc::new(CullStore::new(Arc::clone(catalog), Arc::clone(&clock)));
    let mut context_rows = std::collections::BTreeMap::new();
    for candidate in &wedding.input.candidates {
        context_rows.insert(
            candidate.image_id,
            RowContext {
                scene: candidate.scene.as_str().to_string(),
                chapter: candidate.chapter.as_str().to_string(),
                timeline_ts: candidate.timeline_ts,
                sub_scores: candidate.sub_scores(),
            },
        );
    }
    let mut required = std::collections::BTreeMap::new();
    for rule in rules.rules() {
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
    cull_store.put(&project, &result, &provenance, &context_rows)?;
    lines.push(format!(
        "selection: {} keepers and {} rejections stored",
        result.selected.len(),
        result.rejected.len()
    ));

    // Record every decision, both sides of the line.
    let ledger = Arc::new(Ledger::new(Arc::clone(catalog), Arc::clone(&clock)));
    let explain = Explain::with_tables(
        Arc::clone(&ledger),
        Arc::clone(&clock),
        Arc::new(policy.clone()),
        Arc::new(aura_explain::CalibrationSet::shipped()),
    );
    let cull_context = CullContext {
        model_ver: result.model_ver,
        analysis_ver: result.analysis_ver,
        score_calibration_ver: result.calibration_ver,
        run_hash: result.deterministic_hash,
        ..CullContext::default()
    };

    let mut recorded = 0usize;
    let mut refused = 0usize;
    let mut first_keeper = None;
    for keep in &result.selected {
        let decision = CullDecision::Keep(Box::new(keep.clone()));
        let risk = if keep.is_protected() {
            Risk::must_have()
        } else {
            Risk::NONE
        };
        match explain.record_built(
            adapt::cull_decision(project, &decision, &cull_context),
            risk,
        ) {
            Ok(stored) => {
                recorded += 1;
                if first_keeper.is_none() {
                    first_keeper = Some(stored);
                }
            }
            Err(_) => refused += 1,
        }
    }
    for drop in &result.rejected {
        let decision = CullDecision::Drop(Box::new(drop.clone()));
        match explain.record_built(
            adapt::cull_decision(project, &decision, &cull_context),
            Risk::NONE,
        ) {
            Ok(_) => recorded += 1,
            Err(_) => refused += 1,
        }
    }
    lines.push(format!(
        "ledger: {recorded} decisions recorded, {refused} refused"
    ));
    if refused > 0 {
        failures += 1;
    }

    // Invariant 2, measured rather than asserted in prose.
    let outline = explain.outline(project)?;
    lines.push(format!(
        "coverage: {} of {} decisions explained ({:.3}), {} reasons, {} with evidence",
        outline.explained,
        outline.decisions,
        outline.explanation_coverage(),
        outline.reasons,
        outline.evidenced
    ));
    if outline.explained != outline.decisions || outline.decisions == 0 {
        failures += 1;
    }

    // Size, against section 11's budget.
    let per_thousand = outline.bytes_per_thousand();
    lines.push(format!(
        "size: {} bytes for {} decisions, {} per thousand against a {LEDGER_BYTES_PER_THOUSAND} budget",
        outline.bytes, outline.decisions, per_thousand
    ));
    if per_thousand > LEDGER_BYTES_PER_THOUSAND {
        failures += 1;
    }

    // The band table did what it said. Nothing is calibrated, so nothing may be `Auto`.
    let auto = outline.by_autonomy.first().copied().unwrap_or_default();
    lines.push(format!(
        "autonomy: {:?} across {:?}",
        outline.by_autonomy,
        Autonomy::ALL.map(Autonomy::as_str)
    ));
    if auto > 0 {
        lines.push(
            "  a decision acted unattended on an uncalibrated confidence - FAILED".to_string(),
        );
        failures += 1;
    }

    // The trigger. An UPDATE on the ledger is refused by the database itself.
    let blocked = catalog.writer().transact(|conn| {
        let attempt = conn.execute("UPDATE decisions SET ms = 1 WHERE 1 = 1", params![]);
        Ok(attempt.is_err())
    })?;
    lines.push(format!("append-only: an UPDATE was refused: {blocked}"));
    if !blocked {
        failures += 1;
    }

    // A correction supersedes, and compaction keeps the photographer's own decision.
    if let Some(keeper) = first_keeper.as_ref() {
        if let Some(image) = keeper.subject.image() {
            let correction = ledger_fixtures::user_kept(project, image)
                .supersedes(keeper.id)
                .build(DecisionId::new(), keeper.created_at.saturating_add(1_000));
            explain.record(&correction)?;
            let history = explain.history(DecisionSubject::Image(image))?;
            lines.push(format!(
                "supersession: {} decisions on record for one frame, newest from {}",
                history.len(),
                history
                    .first()
                    .map_or("nothing", |decision| decision.source.as_str())
            ));
            if history.len() < 2
                || history.first().map(|decision| decision.source) != Some(DecisionSource::User)
            {
                failures += 1;
            }

            let removed = ledger.compact(project, aura_explain::Compaction::everything())?;
            let after = explain.history(DecisionSubject::Image(image))?;
            let user_survived = after
                .iter()
                .any(|decision| decision.source == DecisionSource::User);
            lines.push(format!(
                "compaction: {removed} superseded rows removed, the photographer's own \
                 decision survived: {user_survived}"
            ));
            if !user_survived {
                failures += 1;
            }
        }
    }

    // Replay, against section 11's one-second budget.
    let source = StoredSelectionSource::new(Arc::clone(&cull_store));
    let replayer = Replayer::new(&source);
    let stored_decisions = ledger.project(project)?;
    let mut identical = 0usize;
    let mut moved = 0usize;
    let started = Instant::now(); // DETERMINISM: telemetry only, never an input to a decision
    for decision in &stored_decisions {
        if decision.source == DecisionSource::User {
            // A photographer's own decision has no engine to re-run: it was not computed.
            continue;
        }
        match replayer.replay(decision) {
            Ok(outcome) if outcome.is_identical() => identical += 1,
            Ok(_) | Err(_) => moved += 1,
        }
    }
    let per_replay = started.elapsed().as_millis() / (stored_decisions.len().max(1) as u128);
    lines.push(format!(
        "replay: {identical} identical, {moved} moved, {per_replay} ms each against a \
         {REPLAY_BUDGET_MS} ms budget"
    ));
    if moved > 0 {
        failures += 1;
    }
    if per_replay > REPLAY_BUDGET_MS {
        failures += 1;
    }

    // A summary that says only what it was given.
    if let Some(keeper) = first_keeper.as_ref() {
        let summary = Summariser::template(keeper);
        let facts = Summariser::facts(keeper);
        let grounding = Grounding::check(&summary.summary, &facts);
        lines.push(format!(
            "summary: {} characters, grounded: {}",
            summary.summary.chars().count(),
            grounding.is_grounded()
        ));
        if !grounding.is_grounded() || !summary.is_bounded() {
            lines.push(format!("  ungrounded numbers: {:?}", grounding.ungrounded));
            failures += 1;
        }
    }

    // The panel's read path, against section 11's 250 ms.
    let started = Instant::now(); // DETERMINISM: telemetry only, never an input to a decision
    if let Some(keeper) = first_keeper.as_ref() {
        if let Some(image) = keeper.subject.image() {
            drop(explain.latest(DecisionSubject::Image(image), DecisionKind::Cull)?);
            drop(cull_store.decision(image)?);
        }
    }
    let panel_ms = started.elapsed().as_millis();
    lines.push(format!(
        "panel: one photograph's decision read in {panel_ms} ms against a {PANEL_BUDGET_MS} ms budget"
    ));
    if panel_ms > PANEL_BUDGET_MS {
        failures += 1;
    }

    // The bundle, and the scan that has never found anything and runs anyway.
    let bundle = Bundle::build(&stored_decisions, &outline, &BundleOptions::default())?;
    let leaked = bundle.scan();
    let carries_ids = stored_decisions
        .iter()
        .any(|decision| bundle.json.contains(&decision.subject.id_str()));
    lines.push(format!(
        "bundle: {} decisions, {} identifiers replaced, forbidden content {:?}, raw ids present: {}",
        bundle.decisions, bundle.anonymised, leaked, carries_ids
    ));
    if !leaked.is_empty() || carries_ids {
        failures += 1;
    }

    Ok(Report { lines, failures })
}

/// Does one schema object exist?
fn schema_object(catalog: &Catalog, kind: &str, name: &str) -> AuraResult<bool> {
    let kind = kind.to_string();
    let name = name.to_string();
    catalog.read(move |conn| {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                params![kind, name],
                |row| row.get(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("read the schema", &e))?;
        Ok(count > 0)
    })
}
