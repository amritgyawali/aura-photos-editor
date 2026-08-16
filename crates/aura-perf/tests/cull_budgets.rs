//! Phase 12 performance budgets, measured against the shipping engine and store.
//!
//! Section 11 names four rows. Two of them budget the *analysis* - "full analysis + cull,
//! 3,000 images" on an RTX 4070 and on an M3 Pro - and the analysis is phases 06 to 11 on
//! hardware this build does not have. They are waived rather than relabelled, exactly as
//! ADR-0007 waives every other GPU row, and phases 09, 10 and 11 each carry their own share
//! of the eight minutes in their own suites.
//!
//! The two rows phase 12 owns are asserted here, plus a storage row section 11 does not
//! state. The storage row exists because the absence of a stated budget is not the absence
//! of a measurement: a selection is one row per photograph across two tables plus a reason
//! array, which is precisely the shape that grows quietly.
//!
//! ## What is being timed, and where the boundary is
//!
//! The **pure engine**. `Engine::select` takes plain data and returns a `SelectionResult`;
//! the gather in front of it is a catalog read whose cost belongs to SQLite and is already
//! guarded by the phase 01 rows. Timing the two together would produce a number that moved
//! when an unrelated index changed.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, rfc3339, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::{CullMode, ProjectId};
use aura_cull::engine::{Engine, Request};
use aura_cull::fixtures;
use aura_cull::input::CullInput;
use aura_cull::rules::RuleTable;
use aura_cull::store::{CullStore, Provenance, RowContext};
use aura_cull::weights::WeightTable;
use aura_perf::{Budgets, Measurement, StageTimer};
use rusqlite::params;
use tempfile::TempDir;

/// The frame count section 11's two selection rows are stated against.
const FRAMES: usize = 4_000;

/// The frame count the storage row is stated against.
const STORAGE_FRAMES: usize = 1_000;

fn budgets() -> Budgets {
    Budgets::load(std::path::Path::new("../../perf/budgets.toml"))
        .unwrap_or_else(|err| panic!("budgets: {err}"))
}

fn assert_timing(budgets: &Budgets, measurement: &Measurement) {
    println!(
        "{}: {} ms over {} frames",
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

/// Four thousand analysed frames, built by repeating the fixture weddings.
///
/// Repeating rather than generating a new fixture, so that the timed input has the shape of
/// a real wedding - moments, chapters, duplicate groups, identities and all - rather than
/// four thousand independent frames, which would make the moment and chapter passes trivial
/// and the number meaningless.
fn wide_input() -> CullInput {
    let mut input = CullInput::default();
    let mut round = 0i64;
    while input.candidates.len() < FRAMES {
        for wedding in fixtures::all() {
            let mut source = wedding.input;
            // Shift each repetition clear of the last so the diversity pass sees a
            // continuous day rather than four overlapping ones.
            let offset = round * 12 * 3_600_000;
            for candidate in &mut source.candidates {
                candidate.timeline_ts += offset;
            }
            input.candidates.append(&mut source.candidates);
            input.moments.append(&mut source.moments);
            input.identities.append(&mut source.identities);
            input.hours += source.hours;
            round += 1;
            if input.candidates.len() >= FRAMES {
                break;
            }
        }
    }
    input.candidates.truncate(FRAMES);
    input.model_ver = 1;
    input.sort();
    input
}

#[test]
fn the_selection_passes_meet_their_budget_over_four_thousand_frames() {
    let weights = WeightTable::embedded().unwrap_or_else(|err| panic!("weights: {err:?}"));
    let rules = RuleTable::embedded().unwrap_or_else(|err| panic!("rules: {err:?}"));
    let engine = Engine::new(&weights, &rules);
    let input = wide_input();
    let request = Request {
        mode: CullMode::Balanced,
        target: None,
        overrides: BTreeMap::new(),
    };

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let timer = StageTimer::start("cull_select_4000", clock.as_ref());
    let (result, report) = engine.select(input.clone(), &request);
    let measurement = timer.finish(u64::try_from(input.candidates.len()).unwrap_or(u64::MAX));
    println!(
        "  {} eligible, {} delivered, {} swaps, {} forced by coverage",
        report.eligible, result.actual_count, report.swaps, report.coverage_added
    );
    assert_timing(&budgets(), &measurement);

    // A slider move is the same six passes at a new target, which is why section 11 budgets
    // it at two seconds rather than at a fraction of the first run.
    let resized = Request {
        mode: CullMode::Balanced,
        target: Some(result.actual_count.saturating_sub(200).max(100)),
        overrides: BTreeMap::new(),
    };
    let timer = StageTimer::start("cull_resize_4000", clock.as_ref());
    let (after, _) = engine.select(input.clone(), &resized);
    let measurement = timer.finish(u64::try_from(input.candidates.len()).unwrap_or(u64::MAX));
    println!("  resized to {} photographs", after.actual_count);
    assert_timing(&budgets(), &measurement);
}

#[test]
fn a_stored_selection_stays_inside_its_storage_budget() {
    let weights = WeightTable::embedded().unwrap_or_else(|err| panic!("weights: {err:?}"));
    let rules = RuleTable::embedded().unwrap_or_else(|err| panic!("rules: {err:?}"));
    let dir = TempDir::new().unwrap_or_else(|err| panic!("temp dir: {err}"));
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog = Arc::new(
        Catalog::open(&dir.path().join("cull.sqlite"), Arc::clone(&clock), "test")
            .unwrap_or_else(|err| panic!("catalog: {err:?}")),
    );

    let project = ProjectId::new();
    let now = rfc3339(clock.now_utc());
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "budget".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    catalog
        .writer()
        .transact(move |conn| repo::create_project(conn, &row))
        .unwrap_or_else(|err| panic!("project: {err:?}"));

    let mut input = wide_input();
    input.candidates.truncate(STORAGE_FRAMES);
    input.sort();

    let ids: Vec<String> = input
        .candidates
        .iter()
        .map(|candidate| candidate.image_id.to_db())
        .collect();
    let project_key = project.to_db();
    let stamp = now.clone();
    catalog
        .writer()
        .transact(move |conn| {
            let mut statement = conn
                .prepare(
                    "INSERT INTO photo (photo_id, project_id, capture_time_source, orientation,
                                        timeline_time, created_at, updated_at)
                     VALUES (?1, ?2, 'exif_original', 1, ?3, ?4, ?4)",
                )
                .map_err(|e| aura_core::errors::db::statement_failed("prepare", &e))?;
            for id in &ids {
                statement
                    .execute(params![id, project_key, stamp, stamp])
                    .map_err(|e| aura_core::errors::db::statement_failed("insert", &e))?;
            }
            Ok(())
        })
        .unwrap_or_else(|err| panic!("photos: {err:?}"));

    let before = page_bytes(&catalog);

    let engine = Engine::new(&weights, &rules);
    let (result, report) = engine.select(
        input.clone(),
        &Request {
            mode: CullMode::Balanced,
            target: None,
            overrides: BTreeMap::new(),
        },
    );
    let mut context = BTreeMap::new();
    for candidate in &input.candidates {
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
    let mut required = BTreeMap::new();
    for rule in rules.rules() {
        required.insert(rule.id, rule.min);
    }
    let store = CullStore::new(Arc::clone(&catalog), Arc::clone(&clock));
    store
        .put(
            &project,
            &result,
            &Provenance {
                config_digest: engine.config_digest(),
                candidates: report.rule_candidates.clone(),
                found: report.rule_found.clone(),
                required,
                warnings: report.rule_warnings.clone(),
                report: report.clone(),
            },
            &context,
        )
        .unwrap_or_else(|err| panic!("store: {err:?}"));

    let after = page_bytes(&catalog);
    let used = after.saturating_sub(before);
    println!(
        "cull_store_per_1000_images: {used} bytes for {} keepers and {} rejections",
        result.selected.len(),
        result.rejected.len()
    );
    if cfg!(debug_assertions) {
        println!("  (debug build: reported, not asserted)");
        return;
    }
    if let Err(reason) = budgets().check_size("cull_store_per_1000_images", used) {
        panic!("{reason}");
    }
}

/// How many bytes the catalog file currently occupies, by SQLite's own page accounting.
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
