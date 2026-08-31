#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(clippy::panic, clippy::float_cmp, clippy::disallowed_methods)]
#![allow(clippy::uninlined_format_args)]

//! PHASE-26 section 11's budgets, as tests.
//!
//! | Metric | Budget |
//! |---|---|
//! | Fingerprinting + pair discovery | <= 18 s per wedding |
//! | Solve per camera | <= 1 s |
//! | Total matching pass | <= 25 s |
//!
//! Plus a storage row section 11 does not name. Every phase since 21 has measured one anyway, and
//! this one has a shape no earlier phase's has: **the store does not grow with the wedding.**
//!
//! That was written the other way round first - "the pair table grows with the square of the
//! overlap" - and the measurement said otherwise, which is the whole reason this file prints a
//! breakdown. `pairs::find` truncates at `MAX_PAIRS_PER_CAMERA` verified pairs and the same number
//! again of rejected ones, so a two-body wedding stores a bounded number of pairs whether it is 200
//! frames or 4,000. Everything else here is one row per body per flash state, one per shooter and
//! one per project.
//!
//! So the per-image figure **falls** as a wedding gets bigger: 57 B/image over a thousand
//! photographs, about five times that over two hundred. The budget below is therefore a ceiling on
//! the *fixed* cost expressed at the size it is measured, not a rate - and phase 21's rule applies
//! to the sentence as much as to the number, because a figure written before it is measured is
//! wrong about its own shape as easily as about its size.
//!
//! ## Why the pass is measured on a wedding rather than on a frame
//!
//! Section 11's three rows are all per-*wedding*, unlike every budget before phase 25, and that is
//! the shape of the phase rather than a convention: a fingerprint is an aggregate over everything a
//! body shot, a pair is a relationship between two frames, and a transform is solved once per body.
//! There is no per-frame unit to divide by that would mean anything.
//!
//! The figure carries headroom for a reason named here rather than discovered later: this build
//! measures no skin, because phase 18's segmenter is untrained. When it is, every fingerprint gains
//! a masked statistic per contributing frame and the pass stops being pure arithmetic over stored
//! rows.

use std::sync::Arc;

use aura_brain_gallery::camera::api::MatchingPass;
use aura_brain_gallery::camera::fingerprint::CameraFrame;
use aura_brain_gallery::camera::fixtures::{Body, Shape};
use aura_brain_gallery::camera::policy::Matching;
use aura_brain_gallery::camera::{fixtures, pairs};
use aura_catalog::Catalog;
use aura_core::clock::{Clock, FixedClock};
use aura_core::contract::moment::CameraId;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::ProjectId;
use rusqlite::params;

/// Section 11's fingerprinting and pair-discovery row.
const DISCOVERY_MS: u128 = 18_000;

/// Section 11's per-camera solve row.
const SOLVE_MS: u128 = 1_000;

/// Section 11's whole-pass row.
const PASS_MS: u128 = 25_000;

/// Not in section 11. Measured at 57 B/image over a thousand photographs; 150 is that with
/// headroom, and see the header for why it is not a rate.
const BUDGET_BYTES_PER_IMAGE: u64 = 150;

/// A two-body wedding of roughly `per_node * nodes * 2` frames.
fn wedding(nodes: usize, per_node: usize) -> Vec<CameraFrame> {
    fixtures::wedding(
        &[Body::REFERENCE, Body::SECOND],
        Shape {
            nodes,
            per_node,
            ..Shape::default()
        },
    )
}

fn catalog_with(frames: &[CameraFrame]) -> (tempfile::TempDir, Arc<Catalog>, ProjectId) {
    let dir = tempfile::tempdir().expect("tempdir");
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::default());
    let catalog = Arc::new(
        Catalog::open(&dir.path().join("c.sqlite"), Arc::clone(&clock), "perf")
            .expect("catalog opens at 26"),
    );
    let project = ProjectId::new();
    let key = project.to_db();
    let rows: Vec<(String, i64)> = frames
        .iter()
        .map(|frame| (frame.image.to_db(), frame.timeline_ms))
        .collect();

    catalog
        .writer()
        .transact(move |conn| {
            conn.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'perf', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                params![key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))?;
            for (photo, ms) in &rows {
                let stamp = format!("{:016}", (*ms).max(0));
                conn.execute(
                    "INSERT OR IGNORE INTO photo (photo_id, project_id, capture_time,
                                                  timeline_time, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    params![photo, key, stamp],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
            }
            Ok(())
        })
        .expect("fixture rows insert");

    (dir, catalog, project)
}

fn clock() -> Arc<dyn Clock> {
    Arc::new(FixedClock::default())
}

/// The `dbstat` payload of one table or index, in bytes.
fn payload(catalog: &Arc<Catalog>, name: &str) -> u64 {
    let owned = name.to_string();
    catalog
        .read(move |conn| {
            conn.query_row(
                "SELECT COALESCE(SUM(payload), 0) FROM dbstat WHERE name = ?1",
                params![owned],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("dbstat", &e))
        })
        .map(|value| value.max(0) as u64)
        .unwrap_or(0)
}

#[test]
fn fingerprinting_and_pair_discovery_stay_inside_eighteen_seconds() {
    let frames = wedding(20, 25);
    let policy = Matching::default();
    let reference = CameraId::new(Body::REFERENCE.id);
    let other = CameraId::new(Body::SECOND.id);
    let started = std::time::Instant::now();
    let candidates = pairs::find(&frames, &reference, &other, &policy);
    let elapsed = started.elapsed().as_millis();
    println!(
        "discovery: {} frames -> {} candidate pairs in {elapsed} ms against {DISCOVERY_MS} ms",
        frames.len(),
        candidates.len()
    );
    assert!(
        elapsed <= DISCOVERY_MS,
        "discovery took {elapsed} ms, over the {DISCOVERY_MS} ms budget"
    );
    assert!(
        !candidates.is_empty(),
        "a two-body wedding produced no candidate pairs; the budget measured nothing"
    );
}

#[test]
fn the_whole_matching_pass_stays_inside_twenty_five_seconds() {
    let frames = wedding(20, 25);
    let (_dir, catalog, project) = catalog_with(&frames);
    let pass = MatchingPass::new(Arc::clone(&catalog), clock());

    let started = std::time::Instant::now();
    let report = pass
        .run(project, &frames, &[], &NullProgress, &CancelToken::new())
        .expect("the pass completes");
    let elapsed = started.elapsed().as_millis();

    println!(
        "pass: {} frames, {} bodies, {} pairs, {} solved, {} blended, {} baseline-only \
         in {elapsed} ms against {PASS_MS} ms",
        frames.len(),
        report.cameras,
        report.pairs,
        report.solved,
        report.blended,
        report.baseline_only
    );
    assert!(
        elapsed <= PASS_MS,
        "the pass took {elapsed} ms, over the {PASS_MS} ms budget"
    );

    // Section 11's per-camera solve row, derived: the whole pass over two bodies bounds each solve.
    // Measuring one solve in isolation would need the fit's private inputs, and what section 11 is
    // protecting against is a wedding that takes minutes rather than a function that takes one.
    let per_camera = elapsed / u128::from(report.cameras.max(1));
    println!("solve: {per_camera} ms per camera against {SOLVE_MS} ms");
    assert!(
        per_camera <= SOLVE_MS,
        "one camera cost {per_camera} ms, over the {SOLVE_MS} ms budget"
    );
}

#[test]
fn a_second_pass_over_an_unchanged_wedding_is_no_slower_than_the_first() {
    // Section 9 gives PERF "cache fingerprints". What ships does not cache them across passes and
    // does not need to: a fingerprint is an aggregate over stored numbers, and the whole pass costs
    // less than the budget for one of its three stages. This asserts the property that matters -
    // that re-running is not pathological - rather than the mechanism, so a future cache is an
    // optimisation rather than a contract.
    let frames = wedding(10, 20);
    let (_dir, catalog, project) = catalog_with(&frames);
    let pass = MatchingPass::new(Arc::clone(&catalog), clock());

    let first = {
        let started = std::time::Instant::now();
        pass.run(project, &frames, &[], &NullProgress, &CancelToken::new())
            .expect("first pass");
        started.elapsed().as_millis()
    };
    let second = {
        let started = std::time::Instant::now();
        pass.run(project, &frames, &[], &NullProgress, &CancelToken::new())
            .expect("second pass");
        started.elapsed().as_millis()
    };
    println!("re-pass: {first} ms then {second} ms");
    assert!(
        second <= first.max(50) * 3,
        "the second pass took {second} ms against a first of {first} ms"
    );
}

#[test]
fn the_camera_store_stays_inside_its_measured_budget() {
    let frames = wedding(20, 25);
    let (_dir, catalog, project) = catalog_with(&frames);
    let pass = MatchingPass::new(Arc::clone(&catalog), clock());
    pass.run(project, &frames, &[], &NullProgress, &CancelToken::new())
        .expect("the pass completes");

    let objects = [
        "camera_pair",
        "idx_camera_pair_project",
        "idx_camera_pair_camera",
        "camera_fingerprint",
        "camera_transform",
        "idx_camera_transform_project",
        "camera_shooter_bias",
        "camera_reference",
    ];

    let mut total = 0u64;
    let mut rows: Vec<(&str, u64)> = objects
        .iter()
        .map(|name| (*name, payload(&catalog, name)))
        .collect();
    rows.sort_by_key(|(_, bytes)| std::cmp::Reverse(*bytes));

    let images = frames.len() as u64;
    println!("\ncamera store, over {images} photographs:");
    for (name, bytes) in &rows {
        total += bytes;
        if *bytes > 0 {
            println!("  {name:<30} {:>6} B/image", bytes / images.max(1));
        }
    }
    let per_image = total / images.max(1);
    println!("  {:<30} {per_image:>6} B/image", "measured total");

    assert!(
        total > 0,
        "nothing was measured; a budget test that reads zero proves nothing"
    );
    assert!(
        per_image <= BUDGET_BYTES_PER_IMAGE,
        "the camera store costs {per_image} B per image, over the {BUDGET_BYTES_PER_IMAGE} B budget"
    );

    // The property the number rests on: doubling the wedding does not double the store, because
    // the pair table is capped per camera. Asserting the size alone would pass on a build that had
    // quietly removed the cap and happened to be measured on a small fixture.
    let bigger = wedding(40, 25);
    let (_dir2, catalog2, project2) = catalog_with(&bigger);
    let pass2 = MatchingPass::new(Arc::clone(&catalog2), clock());
    pass2
        .run(project2, &bigger, &[], &NullProgress, &CancelToken::new())
        .expect("the pass completes");
    let doubled: u64 = objects.iter().map(|name| payload(&catalog2, name)).sum();
    println!(
        "  {:<30} {} B over {} photographs ({} B over {images})",
        "double the wedding",
        doubled,
        bigger.len(),
        total
    );
    assert!(
        doubled < total * 2,
        "doubling the wedding doubled the store: {total} B -> {doubled} B; the pair cap is gone"
    );
}
