//! Phase 21 performance budgets, measured against the current analyser and store.
//!
//! Section 11 has four rows. All four are reported here and **two of them are waived**:
//!
//! | Row | Budget | This build |
//! |---|---|---|
//! | Micro pass at full resolution (GPU) | <= 250 ms | waived - no GPU backend |
//! | Micro pass at proxy (2048 px) | <= 35 ms | measured on the processor path |
//! | Cross-frame borrow (alignment + blend) | <= 180 ms | measured on the processor path |
//! | 1,000-image gallery at export | <= 5 min | extrapolated from the per-image figure |
//!
//! The GPU row is waived for the reason phases 14, 19 and 20 waive theirs: this build links no
//! `wgpu` (ADR-0029 section 4).
//!
//! What is measured instead is the **decision**, which is what this phase owns: finding a
//! flyaway, a sheet, a mark and a mouth, deciding what may be done about each, and running the
//! result through the real renderer so the naturalness guard can measure what it did to the
//! catchlights, the hairline and the teeth. That last step is why the figure is not trivial - the
//! guard renders the plan at least once and up to four times, exactly as phase 20's texture guard
//! does.
//!
//! The borrow row is measured separately rather than folded into the pass, because the two
//! answer different questions: the pass figure is what a photographer waits for on every
//! photograph, and the borrow figure is what one repair costs on the few frames that need one.
//!
//! Section 11 sets no storage row. One is measured anyway, against the kilobyte per image every
//! phase since 09 has aimed at.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::micro::MicroPlan;
use aura_core::{PhotoId, ProjectId};
use aura_perf::{Budgets, Measurement, StageTimer};
use aura_retouch::micro::ops::{to_linear, Analyser};
use aura_retouch::micro::store::{MicroStore, BYTES_PER_IMAGE};
use aura_retouch::micro::{borrow, fixtures, glare};
use tempfile::TempDir;

/// How many frames the timing run plans.
const FRAMES: usize = if cfg!(debug_assertions) { 4 } else { 24 };

/// How many borrows the borrow run searches for.
const BORROWS: usize = if cfg!(debug_assertions) { 2 } else { 12 };

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
        "{}: {} ms over {} units ({per_unit:.2} ms each)",
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
    let analyser = Analyser::new().unwrap_or_else(|err| panic!("matrix: {}", err.detail));
    let (_, pixels, context) = fixtures::planned_frame();

    let clock = SystemClock::default();
    let timer = StageTimer::start("micro_plan_frame", &clock);
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
         11's 5 minutes",
        per_image * 1_000.0 / 1_000.0
    );
    println!(
        "  section 11's 250 ms full-resolution row is waived: no GPU backend, ADR-0029 section 4"
    );
    println!(
        "  the figure includes the naturalness guard, which renders the plan through the real \
         renderer at least once"
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn one_borrow_is_searched_and_blended_inside_the_budget() {
    let (_, pixels, context) = fixtures::glare_frame();
    let target = to_linear(&pixels).unwrap_or_else(|| panic!("the fixture carries no pixels"));
    let sibling = context
        .siblings
        .first()
        .unwrap_or_else(|| panic!("the fixture carries no sibling"));
    let sibling_frame = borrow::SiblingFrame {
        image: sibling.image,
        frame: to_linear(&sibling.pixels).unwrap_or_else(|| panic!("no sibling pixels")),
        face: *sibling
            .faces
            .first()
            .unwrap_or_else(|| panic!("no sibling face")),
    };
    let face = *context
        .faces
        .first()
        .unwrap_or_else(|| panic!("the fixture carries no face"));
    let eyes = aura_retouch::micro::ops::upsample(
        context
            .regions
            .get(&aura_core::contract::micro::MicroRegion::Eyes)
            .unwrap_or_else(|| panic!("the fixture carries no eye region")),
        target.width,
        target.height,
    );
    let sheets = glare::detect(&target, &eyes, &context.faces);
    let sheet = sheets
        .first()
        .unwrap_or_else(|| panic!("the painted sheet was not detected"));

    let clock = SystemClock::default();
    let timer = StageTimer::start("micro_borrow", &clock);
    let mut done = 0u64;
    for _ in 0..BORROWS {
        let chosen = borrow::choose(
            &target,
            sheet.region,
            &face,
            std::slice::from_ref(&sibling_frame),
        );
        assert!(chosen.is_ok(), "the aligned sibling was refused");
        done += 1;
    }
    let measurement = timer.finish(done);
    println!(
        "  the alignment search is the whole of this figure; the composite itself is a blend in \
         `aura_render::micro` and runs with the rest of the plan"
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn the_storage_cost_of_one_plan_is_measured_rather_than_assumed() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let dir = TempDir::new().unwrap_or_else(|err| panic!("tempdir: {err}"));
    let (store, project, before) = catalog_with_photos(&dir, Arc::clone(&clock), STORAGE_FRAMES);

    let analyser = Analyser::new().unwrap_or_else(|err| panic!("matrix: {}", err.detail));
    let (_, pixels, context) = fixtures::planned_frame();
    let template: MicroPlan = analyser
        .analyse(PhotoId::new(), &pixels, &context)
        .unwrap_or_else(|err| panic!("analyse: {}", err.detail))
        .plan;
    assert!(
        !template.ops.is_empty(),
        "the storage figure must be measured on a frame that actually carried operations"
    );
    println!(
        "  the fixture plan carries {} operations and {} reasons: {:?}",
        template.ops.len(),
        template.reasons.len(),
        template
            .ops
            .iter()
            .map(aura_core::contract::micro::MicroOp::as_str)
            .collect::<Vec<_>>()
    );

    let photos = photo_ids(store.catalog(), &project);
    assert_eq!(photos.len(), STORAGE_FRAMES);
    for photo in photos {
        let mut plan = template.clone();
        plan.image_id = photo;
        store
            .put(&project, &plan)
            .unwrap_or_else(|err| panic!("put: {}", err.detail));
    }

    let after = payload_bytes(store.catalog(), &MICRO_TABLES);
    let payload = after.saturating_sub(before);
    let per_image = payload / STORAGE_FRAMES as u64;
    println!("micro_store_per_image: {per_image} B against a budget of {BYTES_PER_IMAGE} B");
    println!(
        "  a 4,000-image wedding costs about {:.1} MB",
        per_image as f64 * 4_000.0 / 1_048_576.0
    );
    for table in MICRO_TABLES {
        println!(
            "    {table} and its indexes: {} B/image",
            payload_bytes(store.catalog(), &[table]) / STORAGE_FRAMES as u64
        );
    }

    // Page overhead, asserted as a bounded ratio rather than folded into the per-image figure.
    // Phase 09's correction, which phase 19 had to make twice: a 4 KiB page quantises the
    // whole-file number, so a byte of row growth and two pages of packing drift look identical.
    let file = page_bytes(store.catalog());
    let overhead = file as f64 / payload.max(1) as f64;
    println!("  whole file is {overhead:.2}x the payload, including every earlier phase's tables");
    assert!(
        overhead < 12.0,
        "page overhead is {overhead:.2}x the payload, which is packing gone wrong rather than          rows"
    );

    if let Err(reason) = budgets().check_size("micro_store_per_1000_images", payload) {
        panic!("{reason}");
    }
}

/// The tables migration 22 stores a plan in, with their indexes.
///
/// `micro_matrix` is deliberately absent: it is one row per *project*, so including it would put
/// a constant into a per-image figure and make the number smaller on a larger wedding.
const MICRO_TABLES: [&str; 2] = ["micro_plan", "micro_op"];

/// Payload bytes held by `tables` and every index on them.
///
/// `dbstat.payload` is what the rows themselves occupy. Phase 09's budget was measured with
/// `PRAGMA page_count` first and pinned at its own measurement, which quantises to a 4 KiB page
/// and flipped on the first change that touched a row. A payload figure moves by the bytes that
/// were actually added.
fn payload_bytes(catalog: &Arc<Catalog>, tables: &[&str]) -> u64 {
    catalog
        .read(|conn| {
            let mut total: i64 = 0;
            for table in tables {
                let bytes: i64 = conn
                    .query_row(
                        "SELECT COALESCE(SUM(payload), 0) FROM dbstat WHERE name IN (
                             SELECT name FROM sqlite_master
                             WHERE name = ?1 OR (type = 'index' AND tbl_name = ?1)
                         )",
                        [table],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);
                total = total.saturating_add(bytes);
            }
            Ok(u64::try_from(total).unwrap_or(0))
        })
        .unwrap_or(0)
}

/// Whole-file bytes, page-quantised. Only used for the overhead ratio.
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
) -> (Arc<MicroStore>, ProjectId, u64) {
    let path = dir.path().join("micro.sqlite");
    let catalog = Arc::new(
        Catalog::open(&path, Arc::clone(&clock), "0.1.0")
            .unwrap_or_else(|err| panic!("catalog: {}", err.detail)),
    );
    let project = ProjectId::new();
    let row = aura_catalog::model::ProjectRow {
        project_id: project.to_db(),
        name: "phase 21 budgets".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-08-21T00:00:00Z".to_string(),
        updated_at: "2026-08-21T00:00:00Z".to_string(),
    };
    catalog
        .writer()
        .transact(move |conn| aura_catalog::repo::create_project(conn, &row))
        .unwrap_or_else(|err| panic!("project: {}", err.detail));

    // The identity the fixture invented, so the teeth and eye operations can carry the person
    // they belong to. Storing a plan whose identity-bearing operations had been stripped would
    // measure a narrower row than any real frame produces.
    let identity = fixtures::identity(1).to_db();
    let identity_project = project.to_db();
    catalog
        .writer()
        .transact(move |tx| {
            tx.execute(
                "INSERT INTO identities (id, project_id, created_at, updated_at)
                 VALUES (?1, ?2, '2026-08-21T00:00:00Z', '2026-08-21T00:00:00Z')",
                rusqlite::params![identity, identity_project],
            )
            .map_err(|err| aura_core::errors::db::statement_failed("identity", &err))?;
            Ok(())
        })
        .unwrap_or_else(|err| panic!("identity: {}", err.detail));

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
                             '2026-08-21T00:00:00Z', '2026-08-21T00:00:00Z')",
                    rusqlite::params![
                        photo,
                        project_key,
                        format!("2026-08-21T{:02}:{:02}:00Z", index / 60 % 24, index % 60),
                    ],
                )
                .map_err(|err| aura_core::errors::db::statement_failed("photo", &err))?;
            }
            Ok(())
        })
        .unwrap_or_else(|err| panic!("photos: {}", err.detail));

    let before = payload_bytes(&catalog, &MICRO_TABLES);
    (Arc::new(MicroStore::new(catalog, clock)), project, before)
}
