//! Phase 23 performance budgets, measured against the current planner and store.
//!
//! Section 11 has three rows. All three are reported here and **one of them is waived**:
//!
//! | Row | Budget | This build |
//! |---|---|---|
//! | Geometry decisions per image | <= 40 ms | measured on the processor path |
//! | Resampling overhead at export (45 MP) | <= 120 ms | waived - no GPU backend |
//! | 1,000 selected images total | <= 45 s decisions | extrapolated from the per-image figure |
//!
//! The resampling row is waived for the reason phase 14 gave and phase 19 repeated: there is
//! no `wgpu` backend in this build (ADR-0029 section 4, and phase 14's condition C1). The
//! reference path's resample is measured anyway, at proxy size, so the waiver has a number
//! beside it rather than only a sentence - a 45 MP export is 32 times the pixels of a 2048 px
//! proxy, and the ratio is what a future GPU row will be compared against.
//!
//! Section 11 names no storage row for this phase. One is measured anyway, against the same
//! 1 KB per image every phase since 09 has used.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::geometry::GeometryPlan;
use aura_core::{PhotoId, ProjectId};
use aura_geometry::plan::Planner;
use aura_geometry::profiles::ProfileTable;
use aura_geometry::rules::CropRules;
use aura_geometry::store::{GeometryStore, BYTES_PER_IMAGE};
use aura_geometry::{fixtures, lens};
use aura_perf::{Budgets, Measurement, StageTimer};
use tempfile::TempDir;

/// How many frames the timing run plans.
///
/// Small in debug, where the figure is reported rather than asserted, and large enough in
/// release that one slow first iteration does not decide the answer.
const FRAMES: usize = if cfg!(debug_assertions) { 10 } else { 60 };

/// How many photographs the storage run stores.
///
/// A thousand, because SQLite allocates in pages and a per-image figure taken over ten rows is
/// a measurement of page granularity rather than of the schema.
const STORAGE_FRAMES: usize = 1_000;

fn budgets() -> Budgets {
    Budgets::load(std::path::Path::new("../../perf/budgets.toml"))
        .unwrap_or_else(|err| panic!("budgets: {err}"))
}

fn planner() -> Planner {
    Planner::new(
        ProfileTable::empty(),
        CropRules::shipped().unwrap_or_else(|err| panic!("rules: {}", err.detail)),
    )
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
    let planner = planner();
    let cases = fixtures::wedding();

    let timer = StageTimer::start("geometry_plan_frame", clock.as_ref());
    let mut done = 0u64;
    for index in 0..FRAMES {
        let Some(case) = cases.get(index % cases.len()) else {
            continue;
        };
        let plan = planner.plan(&case.input);
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
        per_image
    );
    println!(
        "  section 11's 120 ms export resampling row is waived: there is no GPU backend \
         (ADR-0029 section 4). The reference resample is measured separately below."
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn the_reference_resample_is_measured_so_the_waiver_has_a_number() {
    // The waived row is about a 45 MP export on a GPU. What can be measured here is the
    // reference path's own resample at proxy size, which is what the interactive path actually
    // runs today - and it is the number a future GPU row will be compared against.
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let side = 1_024usize;
    let mut pixels = vec![0.5f32; side * side * 3];
    for (index, value) in pixels.iter_mut().enumerate() {
        *value = if (index / 3 / 8) % 2 == 0 { 0.8 } else { 0.2 };
    }

    let timer = StageTimer::start("geometry_resample_proxy", clock.as_ref());
    let mut done = 0u64;
    let iterations = if cfg!(debug_assertions) { 2 } else { 12 };
    for _ in 0..iterations {
        let mut frame = pixels.clone();
        let scale = aura_render::geometry::correct_distortion(
            &mut frame,
            side,
            side,
            [0.031, -0.008, 0.0],
        );
        assert!(scale <= 1.0);
        aura_render::geometry::correct_ca(&mut frame, side, side, [1.000_42, 0.999_61]);
        done += 1;
    }
    let measurement = timer.finish(done);
    let per_pass = if done == 0 {
        0.0
    } else {
        measurement.elapsed_ms as f64 / done as f64
    };
    println!(
        "geometry_resample_proxy: {per_pass:.1} ms for distortion plus fringing at {side} px \
         squared"
    );
    println!(
        "  a 45 MP frame is about {:.0}x the pixels, so the reference path would need roughly \
         {:.0} ms - which is why section 11's 120 ms row needs a GPU and is waived here.",
        45_000_000.0 / (side * side) as f64,
        per_pass * 45_000_000.0 / (side * side) as f64
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn the_edge_tracker_is_measured_because_it_is_the_expensive_half() {
    // The manual-lens estimator runs only when a lens has no profile, and it is by far the
    // most expensive thing in the phase: a chain walk over every seed pixel, then a hundred
    // and twenty-nine straightness evaluations per surviving chain. Measuring it separately is
    // what stops a future change to `MIN_CHAIN_SPAN` quietly tripling the pass.
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let side = fixtures::DISTORTION_SIDE;
    let plate = fixtures::grid_plate_at(0.035, side);

    let timer = StageTimer::start("geometry_estimate_lens", clock.as_ref());
    let iterations = if cfg!(debug_assertions) { 1 } else { 4 };
    let mut done = 0u64;
    for _ in 0..iterations {
        let chains = lens::track_edges(&plate, side, side);
        let found = lens::estimate_k1(&chains, 1.0);
        assert!(found.is_some(), "the estimator declined on a painted bend");
        done += 1;
    }
    let measurement = timer.finish(done);
    println!(
        "geometry_estimate_lens: {:.0} ms per frame at {side} px squared - and it runs only on \
         a lens with no profile",
        if done == 0 {
            0.0
        } else {
            measurement.elapsed_ms as f64 / done as f64
        }
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn the_storage_cost_of_one_plan_is_measured_rather_than_assumed() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let dir = TempDir::new().unwrap_or_else(|err| panic!("tempdir: {err}"));
    let (store, project, before) = catalog_with_photos(&dir, Arc::clone(&clock), STORAGE_FRAMES);

    // The widest rows this table stores: a frame carrying every aspect variant, a full set of
    // reasons and a non-zero refusal histogram. Measuring the narrow case - a `ritual` frame
    // delivered as shot with one reason - would produce a figure nothing in a real wedding
    // matches, and this table's narrow case is most of a wedding.
    let planner = planner();
    let templates: Vec<GeometryPlan> = fixtures::wedding()
        .iter()
        .map(|case| planner.plan(&case.input))
        .collect();
    assert!(
        templates.iter().any(|plan| plan.crops.len() >= 3),
        "the fixture set must include a frame carrying variants"
    );

    let photos = photo_ids(store.catalog(), &project);
    assert_eq!(photos.len(), STORAGE_FRAMES);
    for (index, photo) in photos.into_iter().enumerate() {
        let Some(template) = templates.get(index % templates.len()) else {
            continue;
        };
        let mut plan = template.clone();
        plan.image_id = photo;
        store
            .put(&plan)
            .unwrap_or_else(|err| panic!("put: {}", err.detail));
    }

    let after = page_bytes(store.catalog());
    let per_image = after.saturating_sub(before) / STORAGE_FRAMES as u64;
    println!("geometry_store_per_image: {per_image} B against a budget of {BYTES_PER_IMAGE} B");
    println!(
        "  a 4,000-image wedding costs about {:.1} MB",
        per_image as f64 * 4_000.0 / 1_048_576.0
    );
    println!(
        "  refusals are four counters rather than rows: at ~200 refused rectangles a frame, a \
         child table would be 800,000 rows a wedding for something nobody queries across"
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
        created_at: "2026-08-26T00:00:00Z".to_string(),
        updated_at: "2026-08-26T00:00:00Z".to_string(),
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
                     VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 800,
                             '2026-08-26T00:00:00Z', '2026-08-26T00:00:00Z')",
                    rusqlite::params![
                        PhotoId::new().to_db(),
                        project_key,
                        format!("2026-08-26T{:02}:{:02}:00Z", index / 60 % 24, index % 60),
                    ],
                )
                .map_err(|err| aura_core::errors::db::statement_failed("photo", &err))?;
            }
            Ok(())
        })
        .unwrap_or_else(|err| panic!("photos: {}", err.detail));

    let before = page_bytes(&catalog);
    (
        Arc::new(GeometryStore::new(catalog, clock)),
        project,
        before,
    )
}
