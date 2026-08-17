//! Phase 13 performance budgets, measured against the shipping ledger.
//!
//! Section 11 names four rows and three of them are asserted here. The fourth - "Explain
//! panel open (with crops) <= 250 ms" - is a *rendering* budget: it includes a web view
//! laying out a panel and a preview service returning crops, neither of which exists inside
//! a Rust test. What is asserted instead is the part this phase owns, which is the read that
//! feeds it, at a tenth of the budget - and the phase 13 gate prints the measured figure so
//! the number is never merely claimed.
//!
//! ## What is being timed, and where the boundary is
//!
//! The **store**. Recording a decision is one transaction with two inserts; reading one back
//! is two seeks. Both are measured against a real catalog rather than a mock, because the
//! cost this budget exists to catch is exactly the cost a mock removes: a reason table that
//! grew an index nobody needed, or a row that started carrying its sentences again.

use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, rfc3339, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::ids::DecisionId;
use aura_core::contract::ledger::{DecisionKind, DecisionSubject, ExplainService};
use aura_core::{PhotoId, ProjectId};
use aura_explain::fixtures;
use aura_explain::{Explain, Ledger};
use aura_perf::{Budgets, Measurement, StageTimer};
use tempfile::TempDir;

/// The decision count section 11's write row is stated against.
const DECISIONS: usize = 4_000;

/// The decision count the storage row is stated against.
const STORAGE_DECISIONS: usize = 1_000;

fn budgets() -> Budgets {
    Budgets::load(std::path::Path::new("../../perf/budgets.toml"))
        .unwrap_or_else(|err| panic!("budgets: {err}"))
}

fn assert_timing(budgets: &Budgets, measurement: &Measurement) {
    println!(
        "{}: {} ms over {} decisions",
        measurement.stage, measurement.elapsed_ms, measurement.units
    );
    if cfg!(debug_assertions) {
        println!("  (debug build: reported, not asserted)");
        return;
    }
    if let Err(reason) = budgets.check(measurement) {
        panic!("{reason}");
    }
}

/// A catalog with one project and an explainer over it.
fn fixture() -> (TempDir, Arc<Catalog>, Explain, ProjectId) {
    let dir = tempfile::tempdir().unwrap_or_else(|err| panic!("tempdir: {err}"));
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog = Arc::new(
        Catalog::open(
            &dir.path().join("ledger.sqlite"),
            Arc::clone(&clock),
            "0.1.0",
        )
        .unwrap_or_else(|err| panic!("catalog: {err:?}")),
    );
    let project = ProjectId::new();
    let now = rfc3339(clock.now_utc());
    catalog
        .writer()
        .transact({
            let row = ProjectRow {
                project_id: project.to_db(),
                name: "ledger budgets".to_string(),
                couple_label: None,
                event_date: None,
                timezone: "UTC".to_string(),
                status: "active".to_string(),
                created_at: now.clone(),
                updated_at: now,
            };
            move |conn| repo::create_project(conn, &row)
        })
        .unwrap_or_else(|err| panic!("project: {err:?}"));

    let ledger = Arc::new(Ledger::new(Arc::clone(&catalog), Arc::clone(&clock)));
    let explain = Explain::new(ledger, clock).unwrap_or_else(|err| panic!("bands: {err:?}"));
    (dir, catalog, explain, project)
}

#[test]
fn recording_a_decision_meets_its_budget() {
    let (_dir, _catalog, explain, project) = fixture();
    let decisions = fixtures::wedding(project, DECISIONS, 1_770_000_000_000);

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let timer = StageTimer::start("ledger_write_4000", clock.as_ref());
    let mut recorded = 0u64;
    for decision in &decisions {
        if explain.record(decision).is_ok() {
            recorded += 1;
        }
    }
    let measurement = timer.finish(recorded);
    println!("  {recorded} decisions written");
    assert_eq!(
        recorded as usize,
        decisions.len(),
        "every fixture decision must be recordable"
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn reading_one_decision_back_meets_the_panel_budget() {
    let (_dir, _catalog, explain, project) = fixture();
    let decisions = fixtures::wedding(project, 1_000, 1_770_000_000_000);
    for decision in &decisions {
        explain
            .record(decision)
            .unwrap_or_else(|err| panic!("record: {err:?}"));
    }
    let subject = decisions
        .first()
        .map(|decision| decision.subject)
        .unwrap_or(DecisionSubject::Image(PhotoId::new()));

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let timer = StageTimer::start("ledger_read_one", clock.as_ref());
    let found = explain
        .latest(subject, DecisionKind::Cull)
        .unwrap_or_else(|err| panic!("latest: {err:?}"));
    let measurement = timer.finish(1);
    assert!(found.is_some(), "the decision must be readable back");
    assert_timing(&budgets(), &measurement);
}

#[test]
fn the_ledger_stays_inside_its_storage_budget() {
    let (_dir, catalog, explain, project) = fixture();
    let before = page_bytes(&catalog);
    let decisions = fixtures::wedding(project, STORAGE_DECISIONS, 1_770_000_000_000);
    for decision in &decisions {
        explain
            .record(decision)
            .unwrap_or_else(|err| panic!("record: {err:?}"));
    }
    let after = page_bytes(&catalog);
    let grown = after.saturating_sub(before);

    // Two figures: what SQLite actually grew by, and what the outline reports. They are
    // measured differently on purpose - one is pages on disk and one is the sum of the row
    // contents - and a large gap between them means the schema is carrying overhead nobody
    // accounted for.
    let outline = explain
        .outline(project)
        .unwrap_or_else(|err| panic!("outline: {err:?}"));
    println!(
        "ledger_store_per_1000_decisions: {grown} bytes on disk, {} reported by the outline",
        outline.bytes
    );

    if let Err(reason) = budgets().check_size("ledger_store_per_1000_decisions", grown) {
        panic!("{reason}");
    }
}

/// How many bytes the catalog occupies, by page accounting.
fn page_bytes(catalog: &Arc<Catalog>) -> u64 {
    catalog
        .read(|conn| {
            let pages: i64 = conn
                .query_row("PRAGMA page_count", [], |row| row.get(0))
                .map_err(|e| aura_core::errors::db::statement_failed("page_count", &e))?;
            let size: i64 = conn
                .query_row("PRAGMA page_size", [], |row| row.get(0))
                .map_err(|e| aura_core::errors::db::statement_failed("page_size", &e))?;
            Ok(u64::try_from(pages.max(0)).unwrap_or(0) * u64::try_from(size.max(0)).unwrap_or(0))
        })
        .unwrap_or(0)
}

/// A decision id is never reused, and the fixtures rely on it.
#[test]
fn every_fixture_decision_has_its_own_id() {
    let project = ProjectId::new();
    let decisions = fixtures::wedding(project, 500, 0);
    let mut ids: Vec<DecisionId> = decisions.iter().map(|decision| decision.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), decisions.len(), "an id was reused");
}
