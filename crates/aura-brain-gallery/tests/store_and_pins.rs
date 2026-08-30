//! Migration 25 against a real catalog: the bounds in the SQL, the pin that survives a re-pass,
//! the disabled frame that cannot keep a movement, and the round trip through the reason bitmask.
//!
//! ## Every refusal test runs a control first
//!
//! Phase 21's lesson, inherited: **a refusal test that cannot tell a working guard from a broken
//! fixture proves nothing.** An INSERT rejected for a missing foreign key looks exactly like one
//! rejected by the CHECK it is supposed to be testing. So each refusal below inserts the legal
//! version of the same row first and asserts it succeeds.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(clippy::panic, clippy::float_cmp, clippy::disallowed_methods)]
#![allow(clippy::uninlined_format_args, clippy::assertions_on_constants)]

use std::sync::Arc;

use aura_brain_gallery::api::{ConsistencyPass, Gallery};
use aura_brain_gallery::fixtures;
use aura_brain_gallery::store::GalleryStore;
use aura_brain_gallery::tree::Frame;
use aura_catalog::Catalog;
use aura_core::clock::{Clock, FixedClock};
use aura_core::contract::gallery::{GalleryOverride, GalleryService, MAX_D_CCT_K};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ProjectId, SceneId, SegmentId};
use rusqlite::params;

/// A catalog with one project, one chapter and one frame per fixture photograph.
fn catalog(frames: &[Frame]) -> (tempfile::TempDir, Arc<Catalog>, ProjectId) {
    let dir = tempfile::tempdir().expect("tempdir");
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::default());
    let catalog = Catalog::open(&dir.path().join("c.sqlite"), Arc::clone(&clock), "test")
        .expect("the catalog opens and migrates to 25");
    let catalog = Arc::new(catalog);

    let project = ProjectId::new();
    let key = project.to_db();
    let rows: Vec<(String, String, i64)> = frames
        .iter()
        .map(|frame| {
            (
                frame.image.to_db(),
                frame.segment.to_db(),
                frame.timeline_ms,
            )
        })
        .collect();
    let mut segments: Vec<String> = rows.iter().map(|(_, s, _)| s.clone()).collect();
    segments.sort();
    segments.dedup();

    catalog
        .writer()
        .transact(move |conn| {
            conn.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'wedding', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                params![key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))?;
            for (ordinal, segment) in segments.iter().enumerate() {
                conn.execute(
                    "INSERT INTO segments (id, project_id, ordinal, chapter, start_ts, end_ts,
                                           dominant_scene, confidence, reasons, image_count,
                                           created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'ceremony', 0, 1000000, 'ceremony', 0.9,
                             '[\"fixture\"]', 0,
                             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    params![segment, key, i64::try_from(ordinal).unwrap_or(0)],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("segments", &e))?;
            }
            for (photo, _, ms) in &rows {
                conn.execute(
                    "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                        created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    params![photo, key, format!("{ms:016}")],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
            }
            Ok(())
        })
        .expect("the fixture rows are inserted");

    (dir, catalog, project)
}

fn clock() -> Arc<dyn Clock> {
    Arc::new(FixedClock::default())
}

fn run(catalog: &Arc<Catalog>, project: ProjectId, frames: &[Frame]) {
    let pass = ConsistencyPass::new(Arc::clone(catalog), clock());
    pass.run(project, frames, None, &NullProgress, &CancelToken::new())
        .expect("the pass completes");
}

#[test]
fn migration_25_creates_every_object_it_promises() {
    let frames = fixtures::drifting_chapter(SegmentId::new(), SceneId::Ceremony, 20, 300.0);
    let (_dir, catalog, _project) = catalog(&frames);
    for (kind, name) in [
        ("table", "gallery_node"),
        ("table", "gallery_anchor"),
        ("table", "gallery_delta"),
        ("table", "gallery_skin_target"),
        ("table", "gallery_outlier"),
        ("view", "v_gallery_coverage"),
        ("view", "v_gallery_drift"),
        ("trigger", "gallery_outlier_needs_delta"),
        ("trigger", "gallery_anchor_pin_is_final"),
        ("trigger", "gallery_skin_target_needs_frames"),
        ("index", "idx_gallery_delta_node"),
        ("index", "idx_gallery_outlier_queue"),
    ] {
        let found: i64 = catalog
            .read(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                    params![kind, name],
                    |row| row.get(0),
                )
                .map_err(|e| aura_core::errors::db::statement_failed("sqlite_master", &e))
            })
            .expect("sqlite_master reads");
        assert_eq!(found, 1, "{kind} {name} is missing");
    }
}

#[test]
fn a_whole_pass_writes_a_tree_a_service_can_read_back() {
    let frames = fixtures::drifting_chapter(SegmentId::new(), SceneId::Ceremony, 40, 500.0);
    let (_dir, catalog, project) = catalog(&frames);
    run(&catalog, project, &frames);

    let gallery = Gallery::new(Arc::clone(&catalog), clock());
    let nodes = gallery.nodes(project).expect("nodes read");
    assert_eq!(nodes.len(), 1);
    let node = &nodes[0];
    assert!(node.is_anchored(), "the fixture chapter is anchorable");
    assert_eq!(node.image_ids.len(), frames.len());
    assert!(node.anchors.len() >= 3);
    assert!(!node.label.is_empty());

    let outline = gallery.outline(project).expect("outline reads");
    assert_eq!(outline.photos, frames.len() as u32);
    assert_eq!(outline.normalised, frames.len() as u32);
    assert_eq!(outline.nodes, 1);
    assert_eq!(outline.anchored_nodes, 1);
    assert!((outline.coverage - 1.0).abs() < 1e-6);

    // The delta round trip, including the reason bitmask.
    let first = gallery
        .delta(frames[0].image)
        .expect("delta reads")
        .expect("every placed frame has one");
    assert_eq!(first.node_id, node.id);
    assert!(first.within_bounds());
    assert!(!first.reasons.is_empty(), "a delta with no reason is a bug");
    assert!(first.reasons.iter().all(|r| !r.text.is_empty()));

    // And membership is the delta table.
    let in_node = gallery.deltas_in(node.id).expect("deltas read");
    assert_eq!(in_node.len(), frames.len());
    assert_eq!(
        gallery.node_of(frames[0].image).expect("node_of reads"),
        Some(node.id)
    );
}

#[test]
fn the_sql_refuses_a_movement_wider_than_the_contract_ceiling() {
    let frames = fixtures::drifting_chapter(SegmentId::new(), SceneId::Ceremony, 20, 300.0);
    let (_dir, catalog, project) = catalog(&frames);
    run(&catalog, project, &frames);

    let photo = frames[0].image.to_db();
    // The control: a legal movement is accepted, so a refusal below is the CHECK rather than a
    // broken fixture.
    let legal = catalog.writer().transact({
        let photo = photo.clone();
        move |tx| {
            tx.execute(
                "UPDATE gallery_delta SET d_cct = 400.0 WHERE photo_id = ?1",
                params![photo],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("legal update", &e))?;
            Ok(())
        }
    });
    assert!(legal.is_ok(), "the control failed: {legal:?}");

    let refused = catalog.writer().transact(move |tx| {
        tx.execute(
            "UPDATE gallery_delta SET d_cct = 900.0 WHERE photo_id = ?1",
            params![photo],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("illegal update", &e))?;
        Ok(())
    });
    assert!(
        refused.is_err(),
        "the SQL accepted a movement of 900 K against a ceiling of {MAX_D_CCT_K}"
    );
}

#[test]
fn a_disabled_frame_cannot_keep_a_movement() {
    let frames = fixtures::drifting_chapter(SegmentId::new(), SceneId::Ceremony, 20, 400.0);
    let (_dir, catalog, project) = catalog(&frames);
    run(&catalog, project, &frames);
    let gallery = Gallery::new(Arc::clone(&catalog), clock());

    let image = frames[0].image;
    let before = gallery.delta(image).unwrap().unwrap();
    assert!(!before.is_zero(), "the fixture frame moved");

    gallery.set_enabled(image, false).expect("switching off");
    let after = gallery.delta(image).unwrap().unwrap();
    assert!(
        after.is_zero(),
        "a frame the photographer switched off kept its movement"
    );

    // And the SQL refuses the state the panel cannot produce.
    let photo = image.to_db();
    let refused = catalog.writer().transact(move |tx| {
        tx.execute(
            "UPDATE gallery_delta SET d_cct = 100.0 WHERE photo_id = ?1",
            params![photo],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("illegal update", &e))?;
        Ok(())
    });
    assert!(refused.is_err(), "a disabled frame was given a movement");
}

#[test]
fn a_pinned_anchor_survives_a_whole_re_pass() {
    let frames = fixtures::drifting_chapter(SegmentId::new(), SceneId::Ceremony, 40, 500.0);
    let (_dir, catalog, project) = catalog(&frames);
    run(&catalog, project, &frames);
    let gallery = Gallery::new(Arc::clone(&catalog), clock());

    let node = gallery.nodes(project).unwrap()[0].id;
    // A frame that automation did *not* choose, so the assertion is about the pin rather than about
    // the ranking agreeing with it.
    let chosen = gallery.nodes(project).unwrap()[0].anchors.clone();
    let unchosen = frames
        .iter()
        .map(|f| f.image)
        .find(|image| !chosen.contains(image))
        .expect("the fixture has more frames than anchors");

    gallery.pin_anchor(node, unchosen).expect("pinning");
    let after_pin = gallery.node(node).unwrap().unwrap();
    assert!(
        after_pin.anchors.contains(&unchosen),
        "the pin did not take"
    );

    // Re-run the whole pass. The tree is rebuilt from scratch and the pin has to come back.
    run(&catalog, project, &frames);
    let nodes = gallery.nodes(project).unwrap();
    let carried = nodes
        .iter()
        .any(|node| node.anchors.first() == Some(&unchosen));
    assert!(
        carried,
        "a pinned anchor did not survive a re-pass; it is first in its node or nowhere"
    );

    let outline = gallery.outline(project).unwrap();
    assert!(outline.pinned_anchors >= 1);
}

#[test]
fn a_rejected_anchor_never_comes_back() {
    let frames = fixtures::drifting_chapter(SegmentId::new(), SceneId::Ceremony, 40, 500.0);
    let (_dir, catalog, project) = catalog(&frames);
    run(&catalog, project, &frames);
    let gallery = Gallery::new(Arc::clone(&catalog), clock());

    let node = gallery.nodes(project).unwrap()[0].id;
    let chosen = gallery.nodes(project).unwrap()[0].anchors[0];
    gallery.reject_anchor(node, chosen).expect("rejecting");

    run(&catalog, project, &frames);
    for node in gallery.nodes(project).unwrap() {
        assert!(
            !node.anchors.contains(&chosen),
            "a rejected anchor came back after a re-pass"
        );
    }
}

#[test]
fn an_override_survives_a_re_pass_and_a_wild_one_is_refused() {
    let frames = fixtures::drifting_chapter(SegmentId::new(), SceneId::Ceremony, 30, 400.0);
    let (_dir, catalog, project) = catalog(&frames);
    run(&catalog, project, &frames);
    let gallery = Gallery::new(Arc::clone(&catalog), clock());

    let image = frames[0].image;
    let empty = gallery.set_override(image, GalleryOverride::default());
    assert!(empty.is_err(), "an override that sets nothing is refused");

    let wild = gallery.set_override(
        image,
        GalleryOverride {
            d_cct: Some(MAX_D_CCT_K * 3.0),
            ..GalleryOverride::default()
        },
    );
    assert!(wild.is_err(), "an override past the bound is refused");

    gallery
        .set_override(
            image,
            GalleryOverride {
                d_cct: Some(-120.0),
                ..GalleryOverride::default()
            },
        )
        .expect("a legal override is recorded");
    let stored = gallery.delta(image).unwrap().unwrap();
    assert!(stored.user_edited);
    assert!((stored.d_cct + 120.0).abs() < 1e-3);

    run(&catalog, project, &frames);
    let after = gallery.delta(image).unwrap().unwrap();
    assert!(
        after.user_edited,
        "the override was overwritten by automation"
    );
    assert!((after.d_cct + 120.0).abs() < 1e-3);
}

#[test]
fn the_skin_target_trigger_refuses_a_target_with_too_few_frames() {
    let frames = fixtures::drifting_chapter(SegmentId::new(), SceneId::Ceremony, 10, 200.0);
    let (_dir, catalog, project) = catalog(&frames);
    let key = project.to_db();
    let identity = aura_core::IdentityId::new().to_db();

    // The control needs an identity row to point at, and a legal insert to prove the fixture works.
    let control = catalog.writer().transact({
        let (key, identity) = (key.clone(), identity.clone());
        move |tx| {
            tx.execute(
                "INSERT INTO identities (id, project_id, role, role_confidence, role_reasons,
                                         created_at, updated_at)
                 VALUES (?1, ?2, 'guest', 0.5, '[\"fixture\"]', '2026-01-01T00:00:00Z',
                         '2026-01-01T00:00:00Z')",
                params![identity, key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("identities", &e))?;
            tx.execute(
                "INSERT INTO gallery_skin_target (identity_id, project_id, u, v, luma, frames,
                                                  spread_before, spread_after, analysis_ver,
                                                  updated_at)
                 VALUES (?1, ?2, 0.24, 0.50, 0.45, 8, 2.0, 1.0, 1, '2026-01-01T00:00:00Z')",
                params![identity, key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("legal target", &e))?;
            Ok(())
        }
    });
    assert!(control.is_ok(), "the control failed: {control:?}");

    let refused = catalog.writer().transact(move |tx| {
        tx.execute(
            "DELETE FROM gallery_skin_target WHERE identity_id = ?1",
            params![identity],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("clear", &e))?;
        tx.execute(
            "INSERT INTO gallery_skin_target (identity_id, project_id, u, v, luma, frames,
                                              spread_before, spread_after, analysis_ver,
                                              updated_at)
             VALUES (?1, ?2, 0.24, 0.50, 0.45, 3, 2.0, 1.0, 1, '2026-01-01T00:00:00Z')",
            params![identity, key],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("weak target", &e))?;
        Ok(())
    });
    assert!(
        refused.is_err(),
        "a three-frame skin target was accepted; a weak target looks like evidence"
    );
}

#[test]
fn a_second_pass_over_an_unchanged_project_changes_nothing() {
    let frames = fixtures::drifting_chapter(SegmentId::new(), SceneId::Ceremony, 40, 600.0);
    let (_dir, catalog, project) = catalog(&frames);
    run(&catalog, project, &frames);
    let gallery = Gallery::new(Arc::clone(&catalog), clock());

    let before: Vec<_> = frames
        .iter()
        .map(|frame| gallery.delta(frame.image).unwrap().unwrap())
        .collect();
    run(&catalog, project, &frames);
    let after: Vec<_> = frames
        .iter()
        .map(|frame| gallery.delta(frame.image).unwrap().unwrap())
        .collect();

    for (a, b) in before.iter().zip(after.iter()) {
        assert!(
            a.agrees_with(b),
            "a frame moved again on the second pass: {} then {}",
            a.d_cct,
            b.d_cct
        );
    }
}

#[test]
fn the_pass_knows_whether_a_stored_tree_is_current() {
    let frames = fixtures::drifting_chapter(SegmentId::new(), SceneId::Ceremony, 20, 300.0);
    let (_dir, catalog, project) = catalog(&frames);
    let pass = ConsistencyPass::new(Arc::clone(&catalog), clock());
    assert!(!pass.is_current(project).unwrap(), "nothing is stored yet");
    run(&catalog, project, &frames);
    assert!(pass.is_current(project).unwrap());
    pass.check_versions(project).expect("no drift");

    // A policy table at a different version makes every stored row stale, which is what makes the
    // resumable pass a query rather than a journal.
    let other = aura_brain_gallery::Consistency::load("version = 99\n").unwrap();
    let moved = ConsistencyPass::with_policy(Arc::clone(&catalog), clock(), other);
    assert!(!moved.is_current(project).unwrap());
    let drift = moved.check_versions(project);
    assert!(drift.is_err(), "a version change must be visible");
    assert_eq!(
        drift.unwrap_err().code.0,
        "AURA-ML-5127",
        "the version-drift code"
    );
}

#[test]
fn an_outlier_cannot_exist_without_a_delta() {
    let frames = fixtures::drifting_chapter(SegmentId::new(), SceneId::Ceremony, 20, 300.0);
    let (_dir, catalog, project) = catalog(&frames);
    run(&catalog, project, &frames);

    let gallery = Gallery::new(Arc::clone(&catalog), clock());
    let node = gallery.nodes(project).unwrap()[0].id.to_db();
    let key = project.to_db();

    // The control: an outlier on a frame that has a delta is accepted.
    let present = frames[0].image.to_db();
    let control = catalog.writer().transact({
        let (node, key, present) = (node.clone(), key.clone(), present);
        move |tx| {
            tx.execute(
                "INSERT INTO gallery_outlier (photo_id, project_id, node_id, deviation, reasons,
                                              analysis_ver, created_at)
                 VALUES (?1, ?2, ?3, 0.8, 0, 1, '2026-01-01T00:00:00Z')",
                params![present, key, node],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("legal outlier", &e))?;
            Ok(())
        }
    });
    assert!(control.is_ok(), "the control failed: {control:?}");

    let orphan = aura_core::PhotoId::new().to_db();
    let refused = catalog.writer().transact(move |tx| {
        tx.execute(
            "INSERT INTO gallery_outlier (photo_id, project_id, node_id, deviation, reasons,
                                          analysis_ver, created_at)
             VALUES (?1, ?2, ?3, 0.8, 0, 1, '2026-01-01T00:00:00Z')",
            params![orphan, key, node],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("orphan outlier", &e))?;
        Ok(())
    });
    assert!(
        refused.is_err(),
        "an outlier was accepted for a frame nobody normalised"
    );
}

#[test]
fn a_node_nothing_could_anchor_still_gets_a_row_per_frame_and_a_reason() {
    // Every frame doubtful, so no anchor clears the floors. The node is written with a NULL target
    // and every frame gets a zero delta carrying `NodeUnanchored` - which is a different row from a
    // frame that needed nothing.
    let segment = SegmentId::new();
    let frames: Vec<Frame> = (0..20)
        .map(|i| {
            let mut frame = fixtures::frame_at(segment, i * 2_000, SceneId::Ceremony);
            frame.wb_conf = 0.20;
            frame
        })
        .collect();
    let (_dir, catalog, project) = catalog(&frames);
    run(&catalog, project, &frames);

    let gallery = Gallery::new(Arc::clone(&catalog), clock());
    let nodes = gallery.nodes(project).unwrap();
    assert_eq!(nodes.len(), 1);
    assert!(!nodes[0].is_anchored());
    assert!(
        nodes[0].target.is_none(),
        "an unanchored node has no target"
    );

    let outline = gallery.outline(project).unwrap();
    assert_eq!(outline.nodes, 1);
    assert_eq!(outline.anchored_nodes, 0, "the second number matters");
    assert_eq!(outline.normalised, frames.len() as u32);

    for frame in &frames {
        let delta = gallery.delta(frame.image).unwrap().unwrap();
        assert!(delta.is_zero());
        assert!(delta
            .reasons
            .iter()
            .any(|r| r.code == aura_core::contract::gallery::GalleryCode::NodeUnanchored));
    }
}

#[test]
fn the_store_reports_a_budget_a_thousand_images_stay_inside() {
    let frames = fixtures::drifting_chapter(SegmentId::new(), SceneId::Ceremony, 200, 500.0);
    let (_dir, catalog, project) = catalog(&frames);
    run(&catalog, project, &frames);

    let store = GalleryStore::new(Arc::clone(&catalog), clock());
    let outline = store.outline(project, Vec::new()).unwrap();
    assert_eq!(outline.normalised, 200);
    // The measurement itself lives in `crates/aura-perf/tests/gallery_budgets.rs`, which counts
    // `dbstat` payload rather than `PRAGMA page_count` - phase 09's correction: a budget measured
    // with an instrument that quantises to 4 KiB must not be pinned at its own measurement. This is
    // only the smoke test that the rows exist to measure.
    assert!(aura_brain_gallery::store::BUDGET_BYTES_PER_IMAGE >= 100);
}
