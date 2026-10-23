use super::*;
use crate::fixtures;
use aura_catalog::model::ProjectRow;
use aura_core::clock::SystemClock;

fn catalog() -> (tempfile::TempDir, Arc<Catalog>, Arc<dyn Clock>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog =
        Catalog::open(&dir.path().join("c.sqlite"), Arc::clone(&clock), "test").expect("open");
    (dir, Arc::new(catalog), clock)
}

fn seed(catalog: &Catalog, project: &ProjectId, photo: &PhotoId) {
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "wedding".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    catalog
        .writer()
        .transact(move |conn| aura_catalog::repo::create_project(conn, &row))
        .expect("project");
    let project_key = project.to_db();
    let photo_key = photo.to_db();
    catalog
        .writer()
        .transact(move |conn| {
            conn.execute(
                "INSERT INTO photo (photo_id, project_id, created_at, updated_at)
                 VALUES (?1, ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                params![photo_key, project_key],
            )
            .map_err(|e| statement_failed("seed photo", &e))?;
            Ok(())
        })
        .expect("photo");
}

#[test]
fn a_saved_recipe_reads_back_identically() {
    let (_dir, catalog, clock) = catalog();
    let project = ProjectId::new();
    let photo = PhotoId::new();
    seed(&catalog, &project, &photo);

    let store = RecipeStore::new(Arc::clone(&catalog), clock);
    let recipe = fixtures::reference();
    store
        .save(&project, &photo, &recipe, &[], "first")
        .expect("save");

    let back = store.load(&photo).expect("load").expect("some");
    assert_eq!(
        recipe_hash(&back).expect("back"),
        recipe_hash(&recipe).expect("original")
    );
}

#[test]
fn a_measured_save_cannot_overwrite_a_newer_edit_or_append_stale_history() {
    let (_dir, catalog, clock) = catalog();
    let project = ProjectId::new();
    let photo = PhotoId::new();
    seed(&catalog, &project, &photo);
    let store = RecipeStore::new(catalog, clock);
    let baseline = fixtures::reference();
    store
        .save(&project, &photo, &baseline, &[], "initial")
        .unwrap();
    let expected = recipe_hash(&baseline).unwrap();
    let mut measured = baseline.clone();
    measured.global.exposure += 0.1;
    let changed = vec!["global.exposure".to_string()];
    store
        .save_if_current(&project, &photo, &measured, &changed, "measured", &expected)
        .unwrap();
    let accepted = store.load(&photo).unwrap().unwrap();
    let history = store.history(&photo, baseline.clone()).unwrap();
    assert!(store
        .save_if_current(&project, &photo, &baseline, &changed, "stale", &expected)
        .is_err());
    assert_eq!(store.load(&photo).unwrap().unwrap(), accepted);
    assert_eq!(
        store.history(&photo, baseline).unwrap().entries(),
        history.entries()
    );
}

#[test]
fn first_edit_is_atomic_and_foreign_saves_cannot_relabel_or_append_history() {
    let (_dir, catalog, clock) = catalog();
    let project = ProjectId::new();
    let photo = PhotoId::new();
    seed(&catalog, &project, &photo);
    let store = RecipeStore::new(catalog, clock);
    let mut first = fixtures::reference();
    first.global.exposure = 0.25;
    let paths = vec!["global.exposure".to_owned()];
    let foreign = ProjectId::new();
    assert!(store
        .save_first(&foreign, &photo, &first, &paths, "foreign")
        .is_err());
    assert!(store.load(&photo).unwrap().is_none());
    store
        .save_first(&project, &photo, &first, &paths, "first")
        .unwrap();
    let before = store.history(&photo, fixtures::reference()).unwrap();
    let mut newer = first.clone();
    newer.global.exposure = 0.75;
    assert!(store
        .save_first(&project, &photo, &newer, &paths, "raced")
        .is_err());
    assert!(store
        .save(&foreign, &photo, &newer, &paths, "foreign")
        .is_err());
    assert!(store
        .save_if_current(
            &foreign,
            &photo,
            &newer,
            &paths,
            "foreign",
            &recipe_hash(&first).unwrap()
        )
        .is_err());
    assert_eq!(store.load(&photo).unwrap().unwrap(), first);
    assert_eq!(
        store
            .history(&photo, fixtures::reference())
            .unwrap()
            .entries(),
        before.entries()
    );
}

#[test]
fn a_history_step_is_recorded_and_rebuilt() {
    let (_dir, catalog, clock) = catalog();
    let project = ProjectId::new();
    let photo = PhotoId::new();
    seed(&catalog, &project, &photo);
    let store = RecipeStore::new(Arc::clone(&catalog), clock);

    let original = fixtures::neutral(fixtures::FIXTURE_HASH, "ILCE-7M4");
    store
        .save(&project, &photo, &original, &[], "first")
        .expect("save");

    let mut edited = original.clone();
    edited.global.exposure = 1.25;
    store
        .save(
            &project,
            &photo,
            &edited,
            &["global.exposure".to_string()],
            "Exposure",
        )
        .expect("save");

    let history = store.history(&photo, original).expect("history");
    assert_eq!(history.entries().len(), 1);
    assert!(history.can_undo());
    assert!((history.current().global.exposure - 1.25).abs() < 1e-6);
}

#[test]
fn the_history_is_trimmed_to_the_bound_on_write() {
    let (_dir, catalog, clock) = catalog();
    let project = ProjectId::new();
    let photo = PhotoId::new();
    seed(&catalog, &project, &photo);
    let store = RecipeStore::new(Arc::clone(&catalog), clock);

    let mut recipe = fixtures::neutral(fixtures::FIXTURE_HASH, "ILCE-7M4");
    for i in 0..(MAX_ENTRIES + 10) {
        recipe.global.exposure = i as f32 * 0.001;
        store
            .save(
                &project,
                &photo,
                &recipe,
                &["global.exposure".to_string()],
                "Exposure",
            )
            .expect("save");
    }

    let count = catalog.count("edit_history").expect("count");
    assert!(
        count <= MAX_ENTRIES as i64,
        "history is bounded on write, found {count}"
    );
}

#[test]
fn coverage_counts_every_photograph_not_only_the_edited_ones() {
    let (_dir, catalog, clock) = catalog();
    let project = ProjectId::new();
    let a = PhotoId::new();
    let b = PhotoId::new();
    seed(&catalog, &project, &a);
    let project_key = project.to_db();
    let b_key = b.to_db();
    catalog
        .writer()
        .transact(move |conn| {
            conn.execute(
                "INSERT INTO photo (photo_id, project_id, created_at, updated_at)
                 VALUES (?1, ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                params![b_key, project_key],
            )
            .map_err(|e| statement_failed("seed photo", &e))?;
            Ok(())
        })
        .expect("second photo");

    let store = RecipeStore::new(Arc::clone(&catalog), clock);
    let mut recipe = fixtures::reference();
    recipe.provenance.user_edited_fields = vec!["global.exposure".to_string()];
    store
        .save(&project, &a, &recipe, &[], "first")
        .expect("save");

    let coverage = store.coverage(&project).expect("coverage");
    assert_eq!(coverage.images, 2);
    assert_eq!(coverage.with_recipe, 1);
    assert_eq!(coverage.touched_by_hand, 1);
    assert_eq!(coverage.sidecar_behind, 1);
    assert!((coverage.fraction() - 0.5).abs() < 1e-6);
}

#[test]
fn an_export_records_the_four_inputs_of_its_hash() {
    let (_dir, catalog, clock) = catalog();
    let project = ProjectId::new();
    let photo = PhotoId::new();
    seed(&catalog, &project, &photo);
    let store = RecipeStore::new(Arc::clone(&catalog), clock);

    let inputs = ExportInputs {
        content_hash: fixtures::FIXTURE_HASH.to_string(),
        recipe_hash: "c".repeat(64),
        engine: crate::contract::recipe::ENGINE.to_string(),
        output_spec: "srgb/8".to_string(),
        level: "full".to_string(),
        colour_space: "srgb".to_string(),
        bit_depth: 8,
        backend: "cpu".to_string(),
        width: 6000,
        height: 4000,
        ms: 812,
    };
    let render_hash = "d".repeat(64);
    store
        .record_export(&project, &photo, &render_hash, inputs.clone())
        .expect("record");

    let back = store
        .export_inputs(&render_hash)
        .expect("read")
        .expect("some");
    assert_eq!(back, inputs);
}
