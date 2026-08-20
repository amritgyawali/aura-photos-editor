//! Phase 19 performance budgets, measured against the current analyser and store.
//!
//! Section 11 has three rows. All three are reported here and **one of them is waived**:
//!
//! | Row | Budget | This build |
//! |---|---|---|
//! | Local decisions + map generation per image | <= 80 ms | measured on the processor path |
//! | Render overhead for local application (proxy) | <= 12 ms | waived - no GPU backend, and no mask to apply |
//! | 1,000 selected images total | <= 90 s | extrapolated from the per-image figure |
//!
//! The render row is waived for two independent reasons and either would be enough. There is
//! no `wgpu` backend in this build (ADR-0029 section 4, and phase 14's condition C1), and
//! there is no phase 18 matte to apply through - `graph::plan` emits
//! `SkipReason::MaskGeneratorAbsent` for every generated mask a phase 19 plan writes, so the
//! local application costs nothing because it does not happen.
//!
//! Section 11 names no storage row for this phase. One is measured anyway, against the same
//! 1 KB per image every phase since 09 has used, because a decision table that quietly costs
//! four kilobytes a frame is a catalog nobody can back up and section 11 not asking is not a
//! reason not to know.

use std::sync::Arc;

use aura_brain_photo::local::fixtures::{self, Frame};
use aura_brain_photo::local::plan::Analyser;
use aura_brain_photo::local::policy::PolicyTable;
use aura_brain_photo::local::store::LocalStore;
use aura_brain_photo::local::BYTES_PER_IMAGE;
use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::local::LocalLightPlan;
use aura_core::{PhotoId, ProjectId};
use aura_perf::{Budgets, Measurement, StageTimer};
use tempfile::TempDir;

/// How many frames the timing run plans.
///
/// Small in debug, where the figure is reported rather than asserted, and large enough in
/// release that one slow first iteration does not decide the answer.
const FRAMES: usize = if cfg!(debug_assertions) { 4 } else { 28 };

/// How many photographs the storage run stores.
///
/// A thousand, because SQLite allocates in pages and a per-image figure taken over seven rows
/// is a measurement of page granularity rather than of the schema.
const STORAGE_FRAMES: usize = 1_000;

fn budgets() -> Budgets {
    Budgets::load(std::path::Path::new("../../perf/budgets.toml"))
        .unwrap_or_else(|err| panic!("budgets: {err}"))
}

fn assert_timing(budgets: &Budgets, measurement: &Measurement) {
    let per_unit = if measurement.units == 0 {
        0.0
    } else {
        measurement.elapsed_ms as f64 / measurement.units as f64
    };
    println!(
        "{}: {} ms over {} frames ({per_unit:.2} ms per image)",
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

#[test]
fn one_frame_is_planned_inside_the_processor_budget() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let analyser = Analyser::new(PolicyTable::embedded().unwrap_or_else(|err| {
        panic!("policy: {}", err.detail);
    }));
    let cases: Vec<Frame> = fixtures::all();

    let timer = StageTimer::start("local_plan_frame", clock.as_ref());
    let mut done = 0u64;
    for index in 0..FRAMES {
        let Some(frame) = cases.get(index % cases.len()) else {
            continue;
        };
        let plan = analyser
            .analyse(&frame.buffer, PhotoId::new(), &frame.context)
            .plan;
        // Read something off the plan so the whole computation cannot be optimised away.
        assert!(plan.confidence >= 0.0);
        done += 1;
    }
    let measurement = timer.finish(done);
    let per_image = if done == 0 {
        0.0
    } else {
        measurement.elapsed_ms as f64 / done as f64
    };
    println!(
        "  extrapolated 1,000 selected images: {:.1} s on the processor path against section \
         11's 90 s",
        per_image * 1_000.0 / 1_000.0
    );
    println!(
        "  section 11's 12 ms render row is waived twice over: there is no GPU backend \
         (ADR-0029 section 4) and no phase 18 matte to apply through"
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn the_storage_cost_of_one_plan_is_measured_rather_than_assumed() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let dir = TempDir::new().unwrap_or_else(|err| panic!("tempdir: {err}"));
    let (store, project, before) = catalog_with_photos(&dir, Arc::clone(&clock), STORAGE_FRAMES);

    // The widest rows this table stores: a group formal with four lit faces and ten shaping
    // zones, and a frame where every operation was gated. Measuring the narrow case would
    // produce a figure nothing in a real wedding matches.
    let analyser = Analyser::new(PolicyTable::embedded().unwrap_or_else(|err| {
        panic!("policy: {}", err.detail);
    }));
    let templates: Vec<LocalLightPlan> = fixtures::all()
        .iter()
        .map(|frame| {
            analyser
                .analyse(&frame.buffer, PhotoId::new(), &frame.context)
                .plan
        })
        .collect();
    assert!(
        templates.iter().any(|plan| plan.face_light.len() >= 4),
        "the fixture set must include a group formal"
    );
    assert!(
        templates
            .iter()
            .any(|plan| plan.dodge_burn.as_ref().is_some_and(|m| !m.is_empty())),
        "and one that carries shaping zones"
    );

    let photos = photo_ids(store.catalog(), &project);
    assert_eq!(photos.len(), STORAGE_FRAMES);
    for (index, photo) in photos.into_iter().enumerate() {
        let Some(template) = templates.get(index % templates.len()) else {
            continue;
        };
        let mut plan = template.clone();
        plan.image_id = photo;
        // The identities the group fixture invented do not exist in this catalog, and a lit
        // face belonging to nobody is a valid row - it is what most guests get.
        for (identity, _) in &mut plan.face_light {
            *identity = None;
        }
        store
            .put(&project, &plan)
            .unwrap_or_else(|err| panic!("put: {}", err.detail));
    }

    let after = page_bytes(store.catalog());
    let per_image = after.saturating_sub(before) / STORAGE_FRAMES as u64;
    println!("local_store_per_image: {per_image} B against a budget of {BYTES_PER_IMAGE} B");
    println!(
        "  a 4,000-image wedding costs about {:.1} MB",
        per_image as f64 * 4_000.0 / 1_048_576.0
    );
    if let Err(reason) =
        budgets().check_size("local_store_per_1000_images", after.saturating_sub(before))
    {
        panic!("{reason}");
    }
}

fn page_bytes(catalog: &Arc<Catalog>) -> u64 {
    catalog
        .read(|conn| {
            let pages: i64 = conn
                .query_row("PRAGMA page_count", [], |row| row.get(0))
                .unwrap_or(0);
            let size: i64 = conn
                .query_row("PRAGMA page_size", [], |row| row.get(0))
                .unwrap_or(0);
            Ok(u64::try_from(pages.saturating_mul(size)).unwrap_or(0))
        })
        .unwrap_or(0)
}

fn photo_ids(catalog: &Arc<Catalog>, project: &ProjectId) -> Vec<PhotoId> {
    let project_key = project.to_db();
    catalog
        .read(move |conn| {
            let mut statement = conn
                .prepare("SELECT photo_id FROM photo WHERE project_id = ?1 ORDER BY photo_id")
                .map_err(|err| aura_core::errors::db::statement_failed("photos", &err))?;
            let mut cursor = statement
                .query(rusqlite::params![project_key])
                .map_err(|err| aura_core::errors::db::statement_failed("photos", &err))?;
            let mut out = Vec::new();
            while let Some(row) = cursor
                .next()
                .map_err(|err| aura_core::errors::db::statement_failed("photos", &err))?
            {
                let text: String = row.get(0).unwrap_or_default();
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
        .unwrap_or_default()
}

fn catalog_with_photos(
    dir: &TempDir,
    clock: Arc<dyn Clock>,
    count: usize,
) -> (Arc<LocalStore>, ProjectId, u64) {
    let path = dir.path().join("local.sqlite");
    let catalog = Arc::new(
        Catalog::open(&path, Arc::clone(&clock), "0.1.0")
            .unwrap_or_else(|err| panic!("catalog: {}", err.detail)),
    );
    let project = ProjectId::new();
    let row = aura_catalog::model::ProjectRow {
        project_id: project.to_db(),
        name: "phase 19 budgets".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-08-19T00:00:00Z".to_string(),
        updated_at: "2026-08-19T00:00:00Z".to_string(),
    };
    catalog
        .writer()
        .transact(move |conn| aura_catalog::repo::create_project(conn, &row))
        .unwrap_or_else(|err| panic!("project: {}", err.detail));

    let project_key = project.to_db();
    catalog
        .writer()
        .transact(move |tx| {
            for index in 0..count {
                tx.execute(
                    "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                        camera_make, camera_model, iso, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 1600,
                             '2026-08-19T00:00:00Z', '2026-08-19T00:00:00Z')",
                    rusqlite::params![
                        PhotoId::new().to_db(),
                        project_key,
                        format!(
                            "2026-08-19T{:02}:{:02}:{:02}Z",
                            index / 3_600 % 24,
                            index / 60 % 60,
                            index % 60
                        ),
                    ],
                )
                .map_err(|err| aura_core::errors::db::statement_failed("photo", &err))?;
            }
            Ok(())
        })
        .unwrap_or_else(|err| panic!("seed: {}", err.detail));

    let before = page_bytes(&catalog);
    let store = Arc::new(LocalStore::new(catalog, clock));
    (store, project, before)
}
