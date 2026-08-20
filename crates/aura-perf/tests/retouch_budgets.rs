//! Phase 20 performance budgets, measured against the current analyser and store.
//!
//! Section 11 has four rows. All four are reported here and **two of them are waived**:
//!
//! | Row | Budget | This build |
//! |---|---|---|
//! | Retouch at full resolution (45 MP, GPU) | <= 350 ms | waived - no GPU backend |
//! | Retouch at proxy (2048 px) | <= 45 ms | measured on the processor path |
//! | 1,000-image gallery at export | <= 7 min | extrapolated from the per-image figure |
//! | Processor fallback (45 MP) | <= 4 s | waived - no 45 MP frame in this repository |
//!
//! The GPU row is waived for the reason phases 14 and 19 waive theirs: this build links no
//! `wgpu` (ADR-0029 section 4). The 45 MP row is waived because there is no camera file in this
//! repository to produce one from - phase 02's condition, still open.
//!
//! What is measured instead is the **decision**, which is what this phase owns: reading a face,
//! finding its marks, deciding what to do about them, and running the result through the real
//! renderer to measure what it did to the texture. That last step is why the figure is not
//! trivial: the texture guard renders the plan at least once and up to four times.
//!
//! Section 11 sets no storage row. One is measured anyway, against the kilobyte per image every
//! phase since 09 has aimed at.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::retouch::RetouchPlan;
use aura_core::{PhotoId, ProjectId};
use aura_perf::{Budgets, Measurement, StageTimer};
use aura_retouch::fixtures;
use aura_retouch::ops::Analyser;
use aura_retouch::store::{RetouchStore, BYTES_PER_IMAGE};
use tempfile::TempDir;

/// How many frames the timing run plans.
const FRAMES: usize = if cfg!(debug_assertions) { 4 } else { 24 };

/// How many photographs the storage run stores.
///
/// A thousand, because SQLite allocates in pages and a per-image figure taken over a handful of
/// rows is a measurement of page granularity rather than of the schema.
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
fn one_frame_is_retouched_inside_the_processor_budget() {
    let analyser = Analyser::new().unwrap_or_else(|err| panic!("presets: {}", err.detail));
    let (_, pixels, context) = fixtures::planned_frame();

    let clock = SystemClock::default();
    let timer = StageTimer::start("retouch_plan_frame", &clock);
    let mut done = 0u64;
    for _ in 0..FRAMES {
        let outcome = analyser
            .analyse(PhotoId::new(), &pixels, &context)
            .unwrap_or_else(|err| panic!("analyse: {}", err.detail));
        // Read something off the plan so the whole computation cannot be optimised away.
        assert!(outcome.plan.confidence >= 0.0);
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
         11's 7 minutes",
        per_image * 1_000.0 / 1_000.0
    );
    println!(
        "  section 11's 350 ms GPU row is waived (no backend, ADR-0029 section 4) and its 45 MP \
         processor row is waived (no camera file in this repository)"
    );
    println!(
        "  the figure includes the texture guard, which renders the plan through the real \
         renderer at least once"
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn the_storage_cost_of_one_plan_is_measured_rather_than_assumed() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let dir = TempDir::new().unwrap_or_else(|err| panic!("tempdir: {err}"));
    let (store, project, before) = catalog_with_photos(&dir, Arc::clone(&clock), STORAGE_FRAMES);

    let analyser = Analyser::new().unwrap_or_else(|err| panic!("presets: {}", err.detail));
    let (_, pixels, context) = fixtures::planned_frame();
    let template: RetouchPlan = analyser
        .analyse(PhotoId::new(), &pixels, &context)
        .unwrap_or_else(|err| panic!("analyse: {}", err.detail))
        .plan;
    assert!(
        !template.ops.is_empty(),
        "the storage figure must be measured on a frame that was actually retouched"
    );

    let photos = photo_ids(store.catalog(), &project);
    assert_eq!(photos.len(), STORAGE_FRAMES);
    for photo in photos {
        let mut plan = template.clone();
        plan.image_id = photo;
        // The identity the fixture invented does not exist in this catalog, and an operation
        // belonging to nobody is a valid row - it is what every face gets on a build with no
        // face recognition.
        plan.per_identity_strength.clear();
        store
            .put(&project, &plan)
            .unwrap_or_else(|err| panic!("put: {}", err.detail));
    }

    let after = page_bytes(store.catalog());
    let per_image = after.saturating_sub(before) / STORAGE_FRAMES as u64;
    println!("retouch_store_per_image: {per_image} B against a budget of {BYTES_PER_IMAGE} B");
    println!(
        "  a 4,000-image wedding costs about {:.1} MB",
        per_image as f64 * 4_000.0 / 1_048_576.0
    );
    if let Err(reason) = budgets().check_size(
        "retouch_store_per_1000_images",
        after.saturating_sub(before),
    ) {
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
) -> (Arc<RetouchStore>, ProjectId, u64) {
    let path = dir.path().join("retouch.sqlite");
    let catalog = Arc::new(
        Catalog::open(&path, Arc::clone(&clock), "0.1.0")
            .unwrap_or_else(|err| panic!("catalog: {}", err.detail)),
    );
    let project = ProjectId::new();
    let row = aura_catalog::model::ProjectRow {
        project_id: project.to_db(),
        name: "phase 20 budgets".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-08-20T00:00:00Z".to_string(),
        updated_at: "2026-08-20T00:00:00Z".to_string(),
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
                let photo = PhotoId::new().to_db();
                tx.execute(
                    "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                        camera_make, camera_model, iso, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 800,
                             '2026-08-20T00:00:00Z', '2026-08-20T00:00:00Z')",
                    rusqlite::params![
                        photo,
                        project_key,
                        format!("2026-08-20T{:02}:{:02}:00Z", index / 60 % 24, index % 60),
                    ],
                )
                .map_err(|err| aura_core::errors::db::statement_failed("photo", &err))?;
            }
            Ok(())
        })
        .unwrap_or_else(|err| panic!("photos: {}", err.detail));

    let before = page_bytes(&catalog);
    (Arc::new(RetouchStore::new(catalog, clock)), project, before)
}
