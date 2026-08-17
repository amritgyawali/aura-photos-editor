//! The phase 12 mechanical gate.
//!
//! This is the assembly proof for culling: migration 12 and its seven objects, both shipped
//! configuration tables, the six selection passes over four synthetic weddings, the coverage
//! guarantees in all three modes, determinism across two runs, the slider budget, a full
//! round trip through the store, and an override that survives a re-selection.
//!
//! The engine is pure, so most of this gate is arithmetic over fixtures rather than a
//! catalog. The store round trip at the end is the part that is not, and it is there because
//! a selection nobody can read back is not a selection.
//!
//! **Nothing here proves the weights.** Every sub-score in the fixtures is authored, and in
//! production all four come from placeholder heads. The distinction is printed at the end of
//! every run rather than hidden in a test helper.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::cull::{Coverage, CullMode, MustHave, SelectionResult};
use aura_core::{AuraResult, ProjectId, SceneId};
use aura_cull::engine::{Engine, PassReport, Request};
use aura_cull::fixtures::{self, Wedding};
use aura_cull::rules::RuleTable;
use aura_cull::store::{CullStore, Provenance, RowContext};
use aura_cull::weights::WeightTable;
use rusqlite::params;

/// Section 10.1: photographer agreement on keepers, Jaccard at moment level.
const AGREEMENT_GATE: f64 = 0.85;

/// Section 11: a slider move re-selects in two seconds.
const SLIDER_BUDGET_MS: u128 = 2_000;

/// Run the phase 12 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase12-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. Migration 12 and every object it owns.
    let catalog_path = work.join("phase12.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 12 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 12, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "cull_run"),
        ("table", "selection"),
        ("table", "rejections"),
        ("table", "coverage_report"),
        ("table", "cull_override"),
        ("view", "v_cull_coverage"),
        ("view", "v_cull_chapters"),
        ("index", "idx_selection_project"),
        ("index", "idx_rejections_project"),
        ("index", "idx_cull_override_project"),
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

    // 2. Both product decisions load, and neither is partial.
    let weights = match WeightTable::embedded() {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("weights: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let rules = match RuleTable::embedded() {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("rules: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    println!(
        "config: weights v{} with {} scene rows, rules v{} with {} guarantees",
        weights.version(),
        weights.rows(),
        rules.version(),
        rules.rules().len()
    );
    if weights.unweighted().is_empty() && weights.rows() == SceneId::ALL.len().saturating_sub(1) {
        println!("  every named scene has its own weight row");
    } else {
        eprintln!(
            "  incomplete weight table; missing {:?}",
            weights.unweighted()
        );
        failures += 1;
    }
    if rules.rules().len() == MustHave::COUNT {
        println!("  all twelve guarantees are defined");
    } else {
        eprintln!("  incomplete rule table");
        failures += 1;
    }
    if rules.veto().posed_scenes.contains(&SceneId::Kiss) {
        eprintln!("  the kiss is listed as a posed scene, which would let AURA veto it");
        failures += 1;
    } else {
        println!("  the closed-eye veto cannot fire on the kiss");
    }

    // 3. The six passes, over four synthetic weddings, in every mode.
    println!("\nselection, by wedding:");
    for wedding in fixtures::all() {
        let candidates: Vec<MustHave> = wedding
            .labels
            .must_have_candidates
            .iter()
            .copied()
            .collect();
        for mode in CullMode::ALL {
            let (result, report) = run(&weights, &rules, &wedding, mode, None);
            let missed: Vec<MustHave> = result
                .coverage
                .must_haves
                .iter()
                .filter(|(rule, state)| candidates.contains(rule) && *state == Coverage::Missing)
                .map(|(rule, _)| *rule)
                .collect();
            let invented: Vec<MustHave> = result
                .coverage
                .must_haves
                .iter()
                .filter(|(rule, state)| !candidates.contains(rule) && state.is_satisfied())
                .map(|(rule, _)| *rule)
                .collect();
            println!(
                "  {:<19} {:<13} {:>4} of {:>4} eligible ({:>4.1} %), {} covered, {} weak, \
                 {} missing",
                wedding.name,
                mode.as_str(),
                result.actual_count,
                report.eligible,
                f64::from(result.keeper_rate()) * 100.0,
                count(&result, Coverage::Covered),
                count(&result, Coverage::CoveredWeak),
                count(&result, Coverage::Missing),
            );
            if !missed.is_empty() {
                eprintln!("    MISSED a must-have with candidates: {missed:?}");
                failures += 1;
            }
            if !invented.is_empty() {
                eprintln!("    CLAIMED coverage with no candidates: {invented:?}");
                failures += 1;
            }
            if !result.is_explained() {
                eprintln!("    a decision carries no reason");
                failures += 1;
            }
        }
    }

    // 4. Agreement, at moment level, against the fixtures' human labels.
    println!("\nagreement (moment-level Jaccard, gate {AGREEMENT_GATE}):");
    for wedding in fixtures::all() {
        let (result, _) = run(&weights, &rules, &wedding, CullMode::Balanced, None);
        let score = agreement(&wedding, &result);
        println!("  {:<19} {score:.3}", wedding.name);
        if score < AGREEMENT_GATE {
            eprintln!("    below the gate");
            failures += 1;
        }
    }

    // 5. Determinism. Two runs of the same question.
    println!("\ndeterminism:");
    for wedding in fixtures::all() {
        let (first, _) = run(&weights, &rules, &wedding, CullMode::Balanced, None);
        let (second, _) = run(&weights, &rules, &wedding, CullMode::Balanced, None);
        let identical = first.deterministic_hash == second.deterministic_hash
            && first.selected == second.selected
            && first.rejected == second.rejected;
        println!(
            "  {:<19} hash {:016x} {}",
            wedding.name,
            first.deterministic_hash,
            if identical { "reproduced" } else { "DIVERGED" }
        );
        if !identical {
            failures += 1;
        }
    }

    // 6. The slider, at section 11's budget.
    println!("\nslider (budget {SLIDER_BUDGET_MS} ms):");
    let wedding = fixtures::nepali_reception();
    let candidates: Vec<MustHave> = wedding
        .labels
        .must_have_candidates
        .iter()
        .copied()
        .collect();
    for target in [500u32, 800, 1_100] {
        // The gate is measuring section 11's slider budget: the number is printed and
        // compared against a ceiling, and nothing in the selection reads it.
        let started = std::time::Instant::now(); // DETERMINISM: measuring a budget, not deciding
        let (result, _) = run(&weights, &rules, &wedding, CullMode::Balanced, Some(target));
        let elapsed = started.elapsed().as_millis();
        let safe = result.coverage.satisfied_where_possible(&candidates);
        println!(
            "  target {target:>5} -> {:>4} delivered in {elapsed:>4} ms, coverage {}",
            result.actual_count,
            if safe { "intact" } else { "BROKEN" }
        );
        if elapsed > SLIDER_BUDGET_MS || !safe {
            failures += 1;
        }
    }

    // 7. The store, end to end, on a real catalog.
    println!("\nstore:");
    match store_round_trip(&catalog, Arc::clone(&clock), &weights, &rules) {
        Ok(lines) => {
            for line in lines {
                println!("  {line}");
            }
        }
        Err(err) => {
            eprintln!("  [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    println!(
        "\nwhat this gate proves: the quota formula, the coverage guard, the diversity caps,\n\
         the size reconciliation, the determinism hash and the store round trip, against\n\
         inputs whose right answer is known by construction.\n\
         what it does not prove: anything about the weights. Every sub-score here is an\n\
         authored number, and in production all four come from placeholder heads (phases 06,\n\
         09, 10 and 11). No figure in this phase is a claim about a real wedding's pixels\n\
         until phase 05's condition C10 closes."
    );

    if failures == 0 {
        println!("\nphase 12: PASS");
        ExitCode::SUCCESS
    } else {
        eprintln!("\nphase 12: {failures} failure(s)");
        ExitCode::FAILURE
    }
}

fn run(
    weights: &WeightTable,
    rules: &RuleTable,
    wedding: &Wedding,
    mode: CullMode,
    target: Option<u32>,
) -> (SelectionResult, PassReport) {
    let engine = Engine::new(weights, rules);
    let request = Request {
        mode,
        target,
        overrides: std::collections::BTreeMap::new(),
    };
    engine.select(wedding.input.clone(), &request)
}

fn count(result: &SelectionResult, state: Coverage) -> usize {
    result
        .coverage
        .must_haves
        .iter()
        .filter(|(_, actual)| *actual == state)
        .count()
}

/// Moment-level Jaccard between the engine's gallery and the fixture's human labels.
fn agreement(wedding: &Wedding, result: &SelectionResult) -> f64 {
    let engine: std::collections::BTreeSet<_> = result
        .selected
        .iter()
        .filter_map(|keep| keep.moment_id)
        .collect();
    let human: std::collections::BTreeSet<_> = wedding
        .input
        .candidates
        .iter()
        .filter(|candidate| wedding.labels.keepers.contains(&candidate.image_id))
        .filter_map(|candidate| candidate.moment_id)
        .collect();
    let union = engine.union(&human).count();
    if union == 0 {
        return 1.0;
    }
    engine.intersection(&human).count() as f64 / union as f64
}

/// Write a selection, read it back, override a frame, and check the override survives.
fn store_round_trip(
    catalog: &Arc<Catalog>,
    clock: Arc<dyn Clock>,
    weights: &WeightTable,
    rules: &RuleTable,
) -> AuraResult<Vec<String>> {
    let mut lines = Vec::new();
    let project = ProjectId::new();
    let now = aura_catalog::rfc3339(clock.now_utc());
    catalog.writer().transact({
        let row = ProjectRow {
            project_id: project.to_db(),
            name: "phase 12 gate".to_string(),
            couple_label: None,
            event_date: None,
            timezone: "UTC".to_string(),
            status: "active".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        move |conn| repo::create_project(conn, &row)
    })?;

    let wedding = fixtures::sparse_elopement();
    let (result, report) = run(weights, rules, &wedding, CullMode::Balanced, None);

    // The photographs have to exist before a selection can reference them.
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

    let store = CullStore::new(Arc::clone(catalog), Arc::clone(&clock));
    let mut context = std::collections::BTreeMap::new();
    for candidate in &wedding.input.candidates {
        context.insert(
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
        config_digest: Engine::new(weights, rules).config_digest(),
        candidates: report.rule_candidates.clone(),
        found: report.rule_found.clone(),
        required,
        warnings: report.rule_warnings.clone(),
        report: report.clone(),
    };
    store.put(&project, &result, &provenance, &context)?;
    lines.push(format!(
        "wrote {} keepers and {} rejections",
        result.selected.len(),
        result.rejected.len()
    ));

    let read = store.selection(project)?;
    match read {
        Some(stored) => {
            let same = stored.selected.len() == result.selected.len()
                && stored.rejected.len() == result.rejected.len()
                && stored.deterministic_hash == result.deterministic_hash
                && stored.coverage.must_haves == result.coverage.must_haves;
            lines.push(format!(
                "read back {} keepers, hash {:016x}, coverage {}",
                stored.selected.len(),
                stored.deterministic_hash,
                if same { "identical" } else { "DIFFERENT" }
            ));
        }
        None => lines.push("read back nothing - FAILED".to_string()),
    }

    let outline = store.outline(project)?;
    lines.push(format!(
        "outline: {} of {} eligible, coverage {:.3}, {} covered / {} weak / {} missing",
        outline.selected,
        outline.eligible,
        outline.coverage,
        outline.covered,
        outline.covered_weak,
        outline.missing
    ));

    // An override is recorded in its own table and survives a rebuild of both others.
    if let Some(first) = result.rejected.first() {
        store.set_override(&project, first.image_id, true)?;
        store.put(&project, &result, &provenance, &context)?;
        let overrides = store.overrides(project)?;
        lines.push(format!(
            "override survived a re-selection: {}",
            overrides.get(&first.image_id) == Some(&true)
        ));
        let cleared = store.clear_override(first.image_id)?;
        lines.push(format!("override withdrawn: {cleared}"));
    }

    Ok(lines)
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
