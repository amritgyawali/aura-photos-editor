#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(clippy::panic, clippy::float_cmp, clippy::disallowed_methods)]
#![allow(clippy::uninlined_format_args)]

//! PHASE-25 section 11's budgets, as tests.
//!
//! | Metric | Budget |
//! |---|---|
//! | Consistency pass for 1,000 images | <= 60 s |
//! | Incremental re-solve after one anchor change | <= 6 s |
//! | Timeline strips render | <= 400 ms |
//! | Extra storage per image | <= 500 B |
//!
//! Three of the four are measured here. The strips render is a browser measurement and belongs to
//! the UI suite; what is measured in its place is the **query** a strip is drawn from, because that
//! is the half that scales with a wedding and the half this crate can hold to a number.
//!
//! ## The storage figure was measured before it was written down
//!
//! Phase 21 shipped a per-image figure that had been written before it was measured and was wrong
//! by a factor of two and a half. Phase 19's correction is the other half of the rule: a budget must
//! not be *pinned at* its own measurement either, because a figure with no headroom fails on the
//! first row somebody adds. So this test prints the per-object breakdown on every run, and
//! `perf/budgets.toml` carries the measurement plus headroom.
//!
//! It counts `dbstat` payload rather than `PRAGMA page_count`, which quantises to 4 KiB - phase
//! 09's correction, and the reason its own 1 KB budget read "exactly 1,024" for ten phases.

use std::sync::Arc;

use aura_brain_gallery::api::{ConsistencyPass, Gallery};
use aura_brain_gallery::fixtures;
use aura_brain_gallery::tree::Frame;
use aura_catalog::Catalog;
use aura_core::clock::{Clock, FixedClock};
use aura_core::contract::gallery::GalleryService;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ProjectId, SceneId, SegmentId};
use rusqlite::params;

/// Section 11's storage row.
const BUDGET_BYTES_PER_IMAGE: u64 = 500;

/// Section 11's pass budget, scaled to the fixture size this test can build in CI.
const PASS_MS_PER_1000: u128 = 60_000;

/// Section 11's incremental budget.
const RESOLVE_MS: u128 = 6_000;

/// Section 11's strips budget, applied to the query a strip is drawn from.
const STRIP_QUERY_MS: u128 = 400;

fn catalog_with(frames: &[Frame]) -> (tempfile::TempDir, Arc<Catalog>, ProjectId) {
    let dir = tempfile::tempdir().expect("tempdir");
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::default());
    let catalog = Arc::new(
        Catalog::open(&dir.path().join("c.sqlite"), Arc::clone(&clock), "perf")
            .expect("catalog opens at 25"),
    );
    let project = ProjectId::new();
    let key = project.to_db();
    let rows: Vec<(String, String)> = frames
        .iter()
        .map(|frame| (frame.image.to_db(), frame.segment.to_db()))
        .collect();
    let mut segments: Vec<String> = rows.iter().map(|(_, s)| s.clone()).collect();
    segments.sort();
    segments.dedup();

    catalog
        .writer()
        .transact(move |conn| {
            conn.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'perf', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                params![key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))?;
            for (ordinal, segment) in segments.iter().enumerate() {
                conn.execute(
                    "INSERT INTO segments (id, project_id, ordinal, chapter, start_ts, end_ts,
                                           dominant_scene, confidence, reasons, image_count,
                                           created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'ceremony', 0, 100000000, 'ceremony', 0.9, '[\"perf\"]',
                             0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    params![segment, key, i64::try_from(ordinal).unwrap_or(0)],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("segments", &e))?;
            }
            for (index, (photo, _)) in rows.iter().enumerate() {
                let stamp = format!("{:016}", index * 1_000);
                conn.execute(
                    "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                        created_at, updated_at)
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

/// A wedding of `count` frames across ten chapters, which is about the shape of a real one.
fn wedding_of(count: usize) -> Vec<Frame> {
    let mut frames = Vec::with_capacity(count);
    let per_chapter = count / 10;
    for chapter in 0..10 {
        let segment = SegmentId::new();
        let mut group = fixtures::drifting_chapter(segment, SceneId::Ceremony, per_chapter, 400.0);
        for frame in &mut group {
            frame.timeline_ms += (chapter as i64) * 20 * 60_000;
        }
        frames.extend(group);
    }
    frames
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
fn the_consistency_pass_stays_inside_sixty_seconds_for_a_thousand_images() {
    let frames = wedding_of(1_000);
    let (_dir, catalog, project) = catalog_with(&frames);
    let pass = ConsistencyPass::new(Arc::clone(&catalog), clock());

    let started = std::time::Instant::now();
    let report = pass
        .run(project, &frames, None, &NullProgress, &CancelToken::new())
        .expect("the pass completes");
    let elapsed = started.elapsed().as_millis();

    println!(
        "gallery pass: {} frames, {} nodes ({} anchored), {elapsed} ms against {PASS_MS_PER_1000} ms",
        report.normalised, report.nodes, report.anchored
    );
    assert!(
        elapsed <= PASS_MS_PER_1000,
        "the pass took {elapsed} ms for 1,000 images, over the {PASS_MS_PER_1000} ms budget"
    );
    assert_eq!(report.normalised, 1_000, "every frame was placed");
}

#[test]
fn one_anchor_change_re_solves_its_node_and_nothing_else() {
    // Section 11 budgets 6 s for an incremental re-solve. What ships re-solves *that node* and no
    // other, which is faster than the budget and is a property of the structure rather than an
    // optimisation: a node's target depends on its own anchors and on nothing outside it.
    // ADR-0051 section 11.
    let frames = wedding_of(1_000);
    let (_dir, catalog, project) = catalog_with(&frames);
    let pass = ConsistencyPass::new(Arc::clone(&catalog), clock());
    pass.run(project, &frames, None, &NullProgress, &CancelToken::new())
        .expect("the first pass completes");

    let gallery = Gallery::new(Arc::clone(&catalog), clock());
    let nodes = gallery.nodes(project).expect("nodes read");
    let node = nodes.first().expect("at least one node");
    let candidate = node
        .image_ids
        .iter()
        .find(|image| !node.anchors.contains(image))
        .copied()
        .expect("a frame that is not already an anchor");

    let started = std::time::Instant::now();
    gallery
        .pin_anchor(node.id, candidate)
        .expect("pinning succeeds");
    let elapsed = started.elapsed().as_millis();

    println!("anchor change: {elapsed} ms against {RESOLVE_MS} ms");
    assert!(
        elapsed <= RESOLVE_MS,
        "one anchor change took {elapsed} ms, over the {RESOLVE_MS} ms budget"
    );
}

#[test]
fn the_query_a_timeline_strip_is_drawn_from_stays_inside_four_hundred_milliseconds() {
    let frames = wedding_of(1_000);
    let (_dir, catalog, project) = catalog_with(&frames);
    let pass = ConsistencyPass::new(Arc::clone(&catalog), clock());
    pass.run(project, &frames, None, &NullProgress, &CancelToken::new())
        .expect("the pass completes");

    let gallery = Gallery::new(Arc::clone(&catalog), clock());
    let nodes = gallery.nodes(project).expect("nodes read");
    let node = nodes.first().expect("at least one node");

    let started = std::time::Instant::now();
    let deltas = gallery.deltas_in(node.id).expect("deltas read");
    let elapsed = started.elapsed().as_millis();

    println!(
        "strip query: {} deltas in {elapsed} ms against {STRIP_QUERY_MS} ms",
        deltas.len()
    );
    assert!(!deltas.is_empty());
    assert!(
        elapsed <= STRIP_QUERY_MS,
        "one node's strip took {elapsed} ms, over the {STRIP_QUERY_MS} ms budget"
    );
}

#[test]
fn the_store_stays_inside_five_hundred_bytes_per_image() {
    let frames = wedding_of(1_000);
    let (_dir, catalog, project) = catalog_with(&frames);
    let pass = ConsistencyPass::new(Arc::clone(&catalog), clock());
    pass.run(project, &frames, None, &NullProgress, &CancelToken::new())
        .expect("the pass completes");

    let objects = [
        "gallery_delta",
        "idx_gallery_delta_node",
        "idx_gallery_delta_project",
        "gallery_node",
        "idx_gallery_node_project",
        "idx_gallery_node_segment",
        "gallery_anchor",
        "idx_gallery_anchor_photo",
        "gallery_outlier",
        "idx_gallery_outlier_queue",
        "gallery_skin_target",
        "idx_gallery_skin_project",
    ];

    let mut total = 0u64;
    println!("\ngallery store, per image, over 1,000 photographs:");
    let mut rows: Vec<(&str, u64)> = objects
        .iter()
        .map(|name| (*name, payload(&catalog, name)))
        .collect();
    rows.sort_by_key(|(_, bytes)| std::cmp::Reverse(*bytes));
    for (name, bytes) in &rows {
        total += bytes;
        if *bytes > 0 {
            println!("  {name:<28} {:>6} B/image", bytes / 1_000);
        }
    }
    let per_image = total / 1_000;
    println!("  {:<28} {per_image:>6} B/image", "measured total");

    assert!(
        per_image <= BUDGET_BYTES_PER_IMAGE,
        "the gallery store costs {per_image} B per image, over the {BUDGET_BYTES_PER_IMAGE} B budget"
    );
    assert!(
        total > 0,
        "nothing was measured; a budget test that reads zero proves nothing"
    );
}
