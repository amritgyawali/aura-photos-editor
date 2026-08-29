//! Phase 23 performance budgets, measured against the current analyser and store.
//!
//! Section 11 has three rows. All three are reported here and **one of them is waived**:
//!
//! | Row | Budget | This build |
//! |---|---|---|
//! | Geometry decisions per image | <= 40 ms | measured on the processor path |
//! | Resampling overhead at export (45 MP) | <= 120 ms | waived - no GPU backend |
//! | 1,000 selected images total | <= 45 s decisions | extrapolated from the per-image figure |
//!
//! The resampling row is waived for the reason phases 14 and 19 to 22 waive theirs: this build
//! links no `wgpu` (ADR-0029 section 4). Unlike those phases there is a second half to the story
//! worth stating - the reference resampler in `aura_render::geometry` exists and is held to the
//! shader by `shader_parity.rs`, so what is missing is the device rather than the operator.
//!
//! What is measured is the **decision**: the horizon and the verticals read once, a lens profile
//! resolved, the rotation band walked, and a bounded crop search run for each aspect against the
//! safety filter. This phase's decision does **not** render, which is what separates its budget
//! from phases 20 to 22: those three have guards that must see their own output, and a crop's
//! effect on a protected region is decidable from the rectangle.
//!
//! Section 11 sets no storage row. One is measured anyway, and it is the second figure in the
//! product above a kilobyte per image - phase 21's was the first. The cause is the same shape and
//! the consequence is not: this schema stores a list, but the contract bounds it at five.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::geometry::GeometryPlan;
use aura_core::{PhotoId, ProjectId};
use aura_geometry::decide::Analyser;
use aura_geometry::fixtures;
use aura_geometry::store::{GeometryStore, BYTES_PER_IMAGE};
use aura_perf::{Budgets, Measurement, StageTimer};
use tempfile::TempDir;

/// How many frames the timing run plans.
///
/// Small in debug, where the figure is reported rather than asserted, and large enough in release
/// that one slow first iteration does not decide the answer.
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
fn one_frame_is_planned_inside_the_processor_budget() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let analyser = Analyser::embedded().unwrap_or_else(|err| panic!("tables: {}", err.detail));
    let cases = fixtures::wedding();
    assert!(!cases.is_empty(), "the fixture wedding must have frames");

    let timer = StageTimer::start("geometry_plan_frame", clock.as_ref());
    let mut done = 0u64;
    for index in 0..FRAMES {
        let Some(frame) = cases.get(index % cases.len()) else {
            continue;
        };
        let (plan, _) = analyser
            .plan(frame)
            .unwrap_or_else(|err| panic!("plan: {}", err.detail));
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
         11's 45 s",
        per_image * 1_000.0 / 1_000.0
    );
    println!(
        "  section 11's 120 ms resampling row is waived: no GPU backend (ADR-0029 section 4). \
         The reference resampler exists and the shader is held to it by shader_parity.rs."
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn the_storage_cost_of_one_plan_is_measured_rather_than_assumed() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let dir = TempDir::new().unwrap_or_else(|err| panic!("tempdir: {err}"));
    let (store, project, before) = catalog_with_photos(&dir, Arc::clone(&clock), STORAGE_FRAMES);

    // The widest rows this table stores: frames that carry the full five variants and a long
    // reason list. Measuring an untouched frame would produce a figure nothing in a real wedding
    // matches, because an untouched frame stores one crop row rather than five.
    let analyser = Analyser::embedded().unwrap_or_else(|err| panic!("tables: {}", err.detail));
    let templates: Vec<GeometryPlan> = fixtures::wedding()
        .iter()
        .filter_map(|frame| analyser.plan(frame).ok().map(|(plan, _)| plan))
        .collect();
    assert!(!templates.is_empty(), "the fixture wedding must plan");
    let widest = templates
        .iter()
        .map(|plan| plan.crops.len())
        .max()
        .unwrap_or(0);
    println!("  widest plan stores {widest} crop variants");

    let photos = photo_ids(store.catalog(), &project);
    assert_eq!(photos.len(), STORAGE_FRAMES);
    for (index, photo) in photos.into_iter().enumerate() {
        let Some(template) = templates.get(index % templates.len()) else {
            continue;
        };
        let mut plan = template.clone();
        plan.image_id = photo;
        // The identities the fixtures invented do not exist in this catalog, and a protected
        // region belonging to nobody is a valid row - it is what most guests produce.
        for region in &mut plan.safety.regions {
            region.identity = None;
        }
        store
            .put(&project, &plan)
            .unwrap_or_else(|err| panic!("put: {}", err.detail));
    }

    let after = page_bytes(store.catalog());
    let per_image = after.saturating_sub(before) / STORAGE_FRAMES as u64;
    println!("geometry_store_per_image: {per_image} B against a budget of {BYTES_PER_IMAGE} B");
    println!(
        "  a 4,000-image wedding costs about {:.1} MB",
        per_image as f64 * 4_000.0 / 1_048_576.0
    );
    if let Err(reason) =
        budgets().check_size("geometry_store_per_1000_images", after.saturating_sub(before))
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
) -> (Arc<GeometryStore>, ProjectId, u64) {
    let path = dir.path().join("geometry.sqlite");
    let catalog = Arc::new(
        Catalog::open(&path, Arc::clone(&clock), "0.1.0")
            .unwrap_or_else(|err| panic!("catalog: {}", err.detail)),
    );
    let project = ProjectId::new();
    let row = aura_catalog::model::ProjectRow {
        project_id: project.to_db(),
        name: "phase 23 budgets".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-08-23T00:00:00Z".to_string(),
        updated_at: "2026-08-23T00:00:00Z".to_string(),
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
                                        camera_make, camera_model, iso, width_px, height_px,
                                        created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 640, 6000, 4000,
                             '2026-08-23T00:00:00Z', '2026-08-23T00:00:00Z')",
                    rusqlite::params![
                        PhotoId::new().to_db(),
                        project_key,
                        format!(
                            "2026-08-23T{:02}:{:02}:{:02}Z",
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
    let store = Arc::new(GeometryStore::new(catalog, clock));
    (store, project, before)
}
