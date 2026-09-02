//! The store, over a real catalog.
//!
//! Migration 29's triggers, the transaction boundary, and the two things a re-run must not touch.
//! Every earlier phase learned some version of this the hard way; phases 25 and 26 both had a gate
//! fail on a foreign key because a fixture seeded ids without seeding the rows they point at, so
//! this file makes its project and its photographs first.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::curate::{CurateOverride, CurateService, PickKind};
use aura_core::ProjectId;
use aura_curate::api::{Curate, CuratePass};
use aura_curate::fixtures::{self, FixtureField, Shape};
use aura_curate::policy::Policy;
use aura_curate::store::CurateStore;
use rusqlite::params;

/// How many images the album's first chapter carries.
fn images_in_first_chapter(album: &aura_core::contract::curate::AlbumPlan) -> usize {
    let Some(first) = album.chapter_map.first() else {
        return 0;
    };
    album
        .spreads
        .iter()
        .filter(|s| s.chapter == first.chapter)
        .map(aura_core::contract::curate::Spread::len)
        .sum()
}

/// A catalog with one project and one photograph row per gallery frame.
fn catalog(field: &FixtureField) -> (tempfile::TempDir, Arc<Catalog>, Arc<dyn Clock>, ProjectId) {
    let dir = tempfile::tempdir().expect("tempdir");
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog = Arc::new(
        Catalog::open(&dir.path().join("c.sqlite"), Arc::clone(&clock), "test").expect("catalog"),
    );
    let project = field.wedding().project;
    let now = aura_catalog::rfc3339(clock.now_utc());
    let frames: Vec<String> = field
        .wedding()
        .frames
        .iter()
        .map(|f| f.image_id.to_db())
        .collect();
    catalog
        .writer()
        .transact(move |tx| {
            tx.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'fixture', ?2, ?2)",
                params![project.to_db(), now],
            )
            .expect("project");
            for id in &frames {
                tx.execute(
                    "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                        created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, ?3, ?3)",
                    params![id, project.to_db(), now],
                )
                .expect("photo");
            }
            Ok(())
        })
        .expect("seed");
    (dir, catalog, clock, project)
}

fn run(shape: Shape, seed: u64) -> (tempfile::TempDir, Curate, ProjectId, FixtureField) {
    let field = FixtureField::new(fixtures::wedding(shape, seed));
    let (dir, catalog, clock, project) = catalog(&field);
    let policy = Policy::default();
    let store = CurateStore::new(Arc::clone(&catalog), Arc::clone(&clock));
    let pass = CuratePass::new(&field, &policy, &store, 1);
    pass.run(project, Some(80), None).expect("the pass runs");
    (dir, Curate::new(catalog, clock), project, field)
}

#[test]
fn a_pass_round_trips_through_the_catalog() {
    let (_dir, service, project, _field) = run(Shape::complete(300), 1);

    let outline = service.outline(project).expect("outline");
    assert!(outline.selected > 0);
    assert_eq!(
        outline.curated, outline.selected,
        "every frame has readings"
    );
    assert!(outline.heroes > 0);
    assert!(outline.spreads > 0);
    assert!(outline.album_size > 0);
    assert!(
        !outline.heads_trained,
        "neither head is trained in this build"
    );

    let result = service.result(project).expect("result").expect("some");
    assert_eq!(result.heroes.len() as u32, outline.heroes);
    assert_eq!(result.album.spreads.len() as u32, outline.spreads);
    assert!(result.album.chapters_are_ordered());
    for hero in &result.heroes {
        assert!(hero.is_well_formed(), "{hero:?}");
    }
    for pick in &result.bw {
        assert!(pick.is_well_formed(), "{pick:?}");
    }
    for spread in &result.album.spreads {
        assert!(spread.is_well_formed(), "{spread:?}");
        assert!(!spread.reasons.is_empty(), "invariant 2");
    }
}

#[test]
fn one_spread_can_be_fetched_by_its_own_id() {
    // The whole reason `SpreadId` exists: the spread view is the screen a photographer spends the
    // most time on, and fetching 120 spreads to draw two frames is what ADR-0060 section 2 rejects.
    let (_dir, service, project, _field) = run(Shape::complete(200), 2);
    let album = service.album(project).expect("album").expect("some");
    let first = album.spreads.first().expect("a spread");
    let fetched = service.spread(first.id).expect("fetch").expect("some");
    assert_eq!(fetched.id, first.id);
    assert_eq!(fetched.left, first.left);
    assert_eq!(fetched.chapter, first.chapter);
    assert_eq!(fetched.pair.score, first.pair.score);
}

#[test]
fn a_second_pass_replaces_the_result_rather_than_appending_to_it() {
    let field = FixtureField::new(fixtures::wedding(Shape::complete(200), 3));
    let (_dir, catalog, clock, project) = catalog(&field);
    let policy = Policy::default();
    let store = CurateStore::new(Arc::clone(&catalog), Arc::clone(&clock));
    let pass = CuratePass::new(&field, &policy, &store, 1);

    let first = pass.run(project, Some(80), None).expect("first");
    let second = pass.run(project, Some(80), None).expect("second");
    assert_eq!(first.heroes, second.heroes);
    assert_eq!(first.spreads, second.spreads);
    assert_eq!(first.album_size, second.album_size);

    let service = Curate::new(catalog, clock);
    let album = service.album(project).expect("album").expect("some");
    assert_eq!(album.spreads.len() as u32, second.spreads);
}

#[test]
fn a_photographers_decision_survives_a_re_run() {
    // The operating manual's fifth code rule. A re-run rewrites every pick and touches no override.
    let field = FixtureField::new(fixtures::wedding(Shape::complete(200), 4));
    let (_dir, catalog, clock, project) = catalog(&field);
    let policy = Policy::default();
    let store = CurateStore::new(Arc::clone(&catalog), Arc::clone(&clock));
    let pass = CuratePass::new(&field, &policy, &store, 1);
    pass.run(project, Some(80), None).expect("first");

    let service = Curate::new(Arc::clone(&catalog), Arc::clone(&clock));
    let hero = service.heroes(project).expect("heroes")[0].image_id;
    service
        .decide(
            project,
            hero,
            CurateOverride {
                kind: PickKind::Hero,
                accepted: false,
                note: Some("not my style".into()),
            },
        )
        .expect("decide");
    assert_eq!(
        service.heroes(project).expect("heroes")[0].accepted,
        Some(false)
    );

    pass.run(project, Some(80), None).expect("second");
    let after = service.heroes(project).expect("heroes");
    let same = after
        .iter()
        .find(|h| h.image_id == hero)
        .expect("still there");
    assert_eq!(
        same.accepted,
        Some(false),
        "a decision is never overwritten"
    );
    assert_eq!(
        service.outline(project).expect("outline").heroes_accepted,
        0
    );
}

#[test]
fn a_photographers_order_survives_a_re_run_and_a_pass_never_writes_one() {
    let field = FixtureField::new(fixtures::wedding(Shape::complete(200), 5));
    let (_dir, catalog, clock, project) = catalog(&field);
    let policy = Policy::default();
    let store = CurateStore::new(Arc::clone(&catalog), Arc::clone(&clock));
    let pass = CuratePass::new(&field, &policy, &store, 1);
    pass.run(project, Some(80), None).expect("first");

    let service = Curate::new(Arc::clone(&catalog), Arc::clone(&clock));
    let album = service.album(project).expect("album").expect("some");
    let mut order = album.images();
    // Reverse inside the first chapter only, which is always allowed.
    //
    // Counted from the spreads rather than as `len * 2`: a chapter ending on a single carries an
    // odd number of images, and the arithmetic version reaches into the next chapter - which the
    // service correctly refuses, and which is a bug in the test rather than in the rule.
    let head = images_in_first_chapter(&album);
    assert!(
        head > 1 && head < order.len(),
        "the fixture has several chapters"
    );
    order[..head].reverse();

    // What the IPC command does: record the order, then re-compose. `set_order` alone stores the
    // sequence and stops, because which two images share a spread is still AURA's decision and the
    // service has no readings to make it with. ADR-0060 section 4.
    service.set_order(project, &order).expect("set order");
    pass.run(project, Some(80), None).expect("re-compose");

    let after = service.album(project).expect("album").expect("some");
    assert!(after.user_ordered);
    assert_eq!(after.images(), order, "the album is now what they dragged");
    assert!(after.chapters_are_ordered());

    // And a later pass leaves it alone.
    pass.run(project, Some(80), None).expect("second");
    let rerun = service.album(project).expect("album").expect("some");
    assert!(rerun.user_ordered, "a pass never clears the flag");
    assert_eq!(rerun.images(), order, "a pass never rewrites the order");
    assert_eq!(service.outline(project).expect("outline").reorders, 1);
}

#[test]
fn an_order_that_reorders_chapters_is_refused_by_the_service() {
    let (_dir, service, project, _field) = run(Shape::complete(200), 6);
    let album = service.album(project).expect("album").expect("some");
    let images = album.images();
    let boundary = images_in_first_chapter(&album);
    assert!(boundary < images.len(), "the fixture has several chapters");

    let mut swapped: Vec<_> = images[boundary..].to_vec();
    swapped.extend_from_slice(&images[..boundary]);
    let err = service.set_order(project, &swapped).unwrap_err();
    assert_eq!(err.code.0, "AURA-ML-5143");

    // And the album is untouched.
    let after = service.album(project).expect("album").expect("some");
    assert!(!after.user_ordered);
    assert_eq!(after.images(), images);
}

#[test]
fn an_order_that_adds_or_drops_an_image_is_refused() {
    let (_dir, service, project, _field) = run(Shape::complete(200), 7);
    let album = service.album(project).expect("album").expect("some");
    let mut short = album.images();
    short.pop();
    assert!(service.set_order(project, &short).is_err());
}

#[test]
fn a_note_longer_than_the_bound_is_refused() {
    let (_dir, service, project, _field) = run(Shape::complete(120), 8);
    let hero = service.heroes(project).expect("heroes")[0].image_id;
    let err = service
        .decide(
            project,
            hero,
            CurateOverride {
                kind: PickKind::Bw,
                accepted: true,
                note: Some("x".repeat(aura_core::contract::curate::MAX_NOTE + 1)),
            },
        )
        .unwrap_err();
    assert_eq!(err.code.0, "AURA-ML-5143");
}

#[test]
fn a_project_nobody_curated_is_not_an_empty_result() {
    // `None` and an empty `CurationResult` are different answers, and a caller that rendered them
    // the same would show a photographer an empty album for a wedding AURA never looked at.
    let field = FixtureField::new(fixtures::wedding(Shape::complete(50), 9));
    let (_dir, catalog, clock, project) = catalog(&field);
    let service = Curate::new(catalog, clock);
    assert!(service.result(project).expect("result").is_none());
    assert!(service.album(project).expect("album").is_none());
    let outline = service.outline(project).expect("outline");
    assert_eq!(outline.selected, 0);
    assert_eq!(outline.coverage(), 0.0);
}

#[test]
fn every_export_format_produces_something_a_consumer_can_read() {
    use aura_core::contract::curate::{ExportFormat, ExportSubject};
    let (_dir, service, project, _field) = run(Shape::complete(200), 10);
    for subject in ExportSubject::ALL {
        for format in ExportFormat::ALL {
            let text = service.export(project, subject, format).expect("export");
            assert!(!text.is_empty(), "{subject:?}/{format:?}");
            if format == ExportFormat::Json {
                let parsed: serde_json::Value = serde_json::from_str(&text)
                    .unwrap_or_else(|e| panic!("{subject:?}: {e}\n{text}"));
                assert!(parsed.is_object());
                assert_eq!(parsed["version"].as_u64(), Some(1));
            }
        }
    }
}

#[test]
fn the_store_reports_its_own_size() {
    let (_dir, service, project, _field) = run(Shape::complete(400), 11);
    let outline = service.outline(project).expect("outline");
    // `dbstat` is a compile-time option; a build without it reports zero rather than failing.
    assert!(outline.bytes < 4_000_000, "{} bytes", outline.bytes);
}

#[test]
fn a_gallery_with_no_frames_curates_to_nothing_rather_than_failing() {
    let field = FixtureField::new(aura_curate::fixtures::Wedding {
        project: ProjectId::new(),
        frames: Vec::new(),
        identities: Vec::new(),
        loci: Default::default(),
        coverage: Default::default(),
        rituals: Vec::new(),
    });
    let (_dir, catalog, clock, project) = catalog(&field);
    let policy = Policy::default();
    let store = CurateStore::new(Arc::clone(&catalog), Arc::clone(&clock));
    let pass = CuratePass::new(&field, &policy, &store, 1);
    let outline = pass.run(project, None, None).expect("a pass over nothing");
    assert_eq!(outline.selected, 0);
    assert_eq!(outline.spreads, 0);
    assert_eq!(outline.heroes, 0);
}
