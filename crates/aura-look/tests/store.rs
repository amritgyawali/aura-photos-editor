//! Migration 31's rows, its triggers and its refusals.
//!
//! **Every refusal test runs a control first.** Phase 21's lesson: a trigger check that reads
//! "the statement failed" as a pass cannot tell a working guard from a broken fixture, and an
//! INSERT refused for a missing foreign key looks exactly like one refused by the promise. Each
//! test here asserts the *allowed* form of the same statement succeeds before asserting the
//! forbidden one does not.

use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, rfc3339, Catalog};
use aura_core::clock::{Clock, FixedClock};
use aura_core::contract::ids::{ProfileId, ProjectId};
use aura_core::contract::look::{
    BucketResidual, LookBucket, LookCode, LookMatchReport, LookProfile, LookReason, MediaSource,
    ReferenceOrigin,
};
use aura_core::contract::style::{LightingBucket, StyleDelta};
use aura_look::fixtures::{self, SyntheticLook};
use aura_look::store::LookStore;
use time::OffsetDateTime;

fn test_clock() -> Arc<dyn Clock> {
    FixedClock::at(OffsetDateTime::UNIX_EPOCH + time::Duration::days(20_000))
}

fn fresh() -> (tempfile::TempDir, Arc<Catalog>, ProjectId) {
    let dir = tempfile::tempdir().expect("tmp");
    let catalog = Catalog::open(&dir.path().join("c.sqlite"), test_clock(), "0.1.0-test")
        .expect("open catalog");
    let now = rfc3339(catalog.clock().now_utc());
    let project = ProjectId::new();
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "Sarah and Tom".to_string(),
        couple_label: None,
        event_date: Some("2026-05-02".to_string()),
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    catalog
        .writer()
        .transact(move |tx| repo::create_project(tx, &row))
        .expect("create project");
    (dir, Arc::new(catalog), project)
}

fn a_look() -> LookProfile {
    let mut look = LookProfile::empty(ProfileId::new(), "Warm film", "engine-test");
    look.origin = ReferenceOrigin::Instagram {
        handle: "the.photographer".to_string(),
    };
    look.source = MediaSource::Folder;
    look.references = 42;
    look.measured_at = 1_700_000_000_000;
    look.analysis_ver = aura_look::ANALYSIS_VER;
    look.global = StyleDelta {
        exposure: 0.18,
        temperature_k: 220.0,
        ..StyleDelta::neutral()
    }
    .clamped();
    look.diagnostics.found = 44;
    look.diagnostics.measured = 42;
    look.diagnostics.refused = 2;
    look.diagnostics.baseline_frames = 30;
    look.diagnostics.strength = 0.8;
    look.diagnostics.reasons = vec![
        LookReason::bare(LookCode::SceneAxisNotLearned),
        LookReason::measured(LookCode::ReferencesBelowUsable, 42.0, 24.0),
    ];
    look.buckets.insert(
        LightingBucket::Daylight,
        LookBucket {
            lighting: LightingBucket::Daylight,
            reference: fixtures::aggregate(12, SyntheticLook::warm_film()),
            baseline: fixtures::aggregate(12, SyntheticLook::neutral()),
            delta: StyleDelta {
                contrast: -4.0,
                ..StyleDelta::neutral()
            },
            confidence: 0.7,
            reasons: Vec::new(),
        },
    );
    look
}

fn a_match(look: &LookProfile, project: ProjectId, after: f32) -> LookMatchReport {
    LookMatchReport {
        profile: look.id,
        project,
        buckets: vec![BucketResidual {
            lighting: LightingBucket::Daylight,
            before_de00: 6.0,
            after_de00: after,
            frames: 30,
        }],
        before_de00: 6.0,
        after_de00: after,
        frames: 30,
        user_edited: 2,
        reasons: Vec::new(),
    }
}

#[test]
fn a_look_survives_a_trip_through_the_catalog() {
    let (_dir, catalog, _project) = fresh();
    let store = LookStore::new(catalog, test_clock());
    let look = a_look();
    let readings = fixtures::readings(4, SyntheticLook::warm_film());

    store.put(&look, &readings).expect("store the look");
    let read = store
        .profile(look.id)
        .expect("read back")
        .expect("the look is there");

    assert_eq!(read.name, look.name);
    assert_eq!(read.origin, look.origin);
    assert_eq!(read.source, look.source);
    assert_eq!(read.references, look.references);
    assert_eq!(read.analysis_ver, look.analysis_ver);
    assert!((read.global.exposure - look.global.exposure).abs() < 1e-4);
    assert_eq!(read.buckets.len(), 1);

    let bucket = read
        .buckets
        .get(&LightingBucket::Daylight)
        .expect("the daylight bucket");
    // Migration 31's note 3: a look stores what it was a difference FROM, not just the
    // difference, so all three come back.
    assert!(bucket.reference.is_usable());
    assert!(bucket.baseline.is_usable());
    assert!((bucket.confidence - 0.7).abs() < 1e-4);
}

#[test]
fn re_measuring_replaces_a_look_rather_than_adding_to_it() {
    let (_dir, catalog, _project) = fresh();
    let store = LookStore::new(catalog, test_clock());
    let mut look = a_look();
    let readings = fixtures::readings(4, SyntheticLook::warm_film());

    store.put(&look, &readings).expect("first");
    look.buckets.insert(
        LightingBucket::Tungsten,
        LookBucket {
            lighting: LightingBucket::Tungsten,
            reference: fixtures::aggregate(8, SyntheticLook::dark_and_moody()),
            baseline: fixtures::aggregate(8, SyntheticLook::neutral()),
            delta: StyleDelta::neutral(),
            confidence: 0.6,
            reasons: Vec::new(),
        },
    );
    store.put(&look, &readings).expect("second");

    let read = store.profile(look.id).expect("the fixture must hold").expect("the fixture must hold");
    assert_eq!(
        read.buckets.len(),
        2,
        "a re-measure must replace the buckets rather than leave two measurements side by side"
    );
    assert_eq!(store.profiles().expect("the fixture must hold").len(), 1, "a look was duplicated");
}

#[test]
fn a_look_that_was_never_measured_cannot_be_selected() {
    let (_dir, catalog, project) = fresh();
    let store = LookStore::new(catalog, test_clock());
    let look = a_look();
    store.put(&look, &[]).expect("store the look");

    // The control: with a match recorded, the same selection succeeds. Without this the test
    // below proves only that something failed.
    let unmeasured = store.select(project, Some(look.id));
    assert!(
        unmeasured.is_err(),
        "a look nobody has measured was selected"
    );

    store
        .put_match(&a_match(&look, project, 2.1), "engine-test")
        .expect("record the match");
    store
        .select(project, Some(look.id))
        .expect("a measured look must be selectable");

    let selected = store.selected(project).expect("the fixture must hold");
    assert_eq!(selected.map(|(id, _)| id), Some(look.id));
}

#[test]
fn a_record_of_what_was_measured_cannot_be_edited() {
    let (_dir, catalog, project) = fresh();
    let store = LookStore::new(Arc::clone(&catalog), test_clock());
    let look = a_look();
    store.put(&look, &[]).expect("store");
    store
        .put_match(&a_match(&look, project, 2.1), "engine-test")
        .expect("first match");

    // The control: a second INSERT is allowed, because re-measuring writes a new row.
    store
        .put_match(&a_match(&look, project, 1.8), "engine-test")
        .expect("a re-measure must be allowed to write a new row");

    // The refusal: an UPDATE is not.
    let refused = catalog.writer().transact(|conn| {
        conn.execute("UPDATE look_match SET after_de00 = 0.0", [])
            .map_err(|err| aura_core::errors::db::statement_failed("update", &err))?;
        Ok(())
    });
    assert!(
        refused.is_err(),
        "a match record was rewritten; migration 31's note 5"
    );

    // And the most recent one is the one that comes back.
    let latest = store.last_match(project).expect("the fixture must hold").expect("the fixture must hold");
    assert!((latest.after_de00 - 1.8).abs() < 1e-4);
}

#[test]
fn strength_is_bounded_below_one_and_at_one() {
    let (_dir, catalog, project) = fresh();
    let store = LookStore::new(Arc::clone(&catalog), test_clock());
    let look = a_look();
    store.put(&look, &[]).expect("the fixture must hold");
    store
        .put_match(&a_match(&look, project, 2.0), "engine-test")
        .expect("the fixture must hold");
    store.select(project, Some(look.id)).expect("the fixture must hold");

    store.set_strength(project, 0.5).expect("half is allowed");
    assert!((store.selected(project).expect("the fixture must hold").expect("the fixture must hold").1 - 0.5).abs() < 1e-4);

    // A value above one is clamped rather than refused, so the column's CHECK is never the
    // thing a photographer meets. Phase 21's rule: a ceiling can be lowered by a studio and
    // raised by nobody.
    store.set_strength(project, 4.0).expect("clamped, not refused");
    assert!((store.selected(project).expect("the fixture must hold").expect("the fixture must hold").1 - 1.0).abs() < 1e-4);
}

#[test]
fn forgetting_a_look_leaves_the_project_on_the_baseline() {
    let (_dir, catalog, project) = fresh();
    let store = LookStore::new(Arc::clone(&catalog), test_clock());
    let look = a_look();
    store.put(&look, &[]).expect("the fixture must hold");
    store
        .put_match(&a_match(&look, project, 2.0), "engine-test")
        .expect("the fixture must hold");
    store.select(project, Some(look.id)).expect("the fixture must hold");

    store.forget(look.id).expect("forget");

    assert!(store.profile(look.id).expect("the fixture must hold").is_none());
    assert!(
        store.selected(project).expect("the fixture must hold").is_none(),
        "a project kept pointing at a look that no longer exists"
    );
}

#[test]
fn the_outline_says_what_its_denominator_is() {
    let (_dir, catalog, project) = fresh();
    let store = LookStore::new(catalog, test_clock());

    let outline = store.outline(project).expect("outline");

    assert_eq!(outline.profiles, 0);
    assert_eq!(outline.photographs, 0);
    assert_eq!(outline.appliable, 0);
    // The one fact a photographer needs before they paste a link, on the wire rather than in a
    // dialog somebody has to remember to write.
    assert!(
        !outline.network_transport_available,
        "the outline claimed this build can fetch a page"
    );
}
