//! A different reference frame, native renderer, saved strengths and stale-edit protection.
#![allow(clippy::expect_used, clippy::too_many_lines, clippy::float_cmp)]
use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::{color_match, AppState};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use std::sync::Arc;

fn setup(path: &std::path::Path, directory: &std::path::Path) -> (AppState, ProjectId, PhotoId) {
    let state = AppState::open(&directory.join("catalog.aura"))
        .expect("isolated catalogue")
        .with_cache_root(&directory.join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Reference colour verification".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    let project_id = ProjectId::from_db(&project.id).expect("project id");
    aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id,
            roots: vec![path.to_path_buf()],
            mode: ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import target");
    let photos = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id,
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("list target");
    let photo = PhotoId::from_db(&photos.first().expect("target").id).expect("photo id");
    let baseline = aura_app::develop_commands::neutral_recipe(&state, photo);
    state
        .recipe_store()
        .save(&project_id, &photo, &baseline, &[], "Camera baseline")
        .expect("initial edit");
    (state, project_id, photo)
}
fn job() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[test]
fn different_dimensions_match_strengths_restore_and_new_manual_edits_are_kept() {
    let directory = tempfile::tempdir().expect("isolated directory");
    let target_path = directory.path().join("original.jpg");
    let reference_path = directory.path().join("edited-reference.tiff");
    image::RgbImage::from_fn(160, 120, |x, y| {
        let l = (45 + x * 100 / 160 + y * 35 / 120) as u8;
        image::Rgb([l, l.saturating_add(6), l.saturating_add(12)])
    })
    .save(&target_path)
    .expect("target fixture");
    image::RgbImage::from_fn(120, 160, |x, y| {
        let l = (55 + x * 100 / 120 + y * 35 / 160) as u8;
        image::Rgb([l.saturating_add(7), l.saturating_add(4), l])
    })
    .save(&reference_path)
    .expect("reference fixture");
    let (state, project, photo) = setup(&target_path, directory.path());
    let original = state
        .recipe_store()
        .load(&photo)
        .expect("baseline")
        .expect("saved baseline");
    let reference = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: reference_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("import TIFF reference");
    assert_eq!((reference.width, reference.height), (120, 160));
    assert_eq!(
        color_match::list_references(&state)
            .expect("saved references")
            .len(),
        1
    );
    let prepared = color_match::prepare(
        &state,
        &color_match::PrepareInput {
            photo_id: photo.to_db(),
            reference_id: reference.id.clone(),
            cancel_id: job(),
        },
    )
    .expect("fit a different image");
    assert!(!prepared.full_resolution_checked);
    assert!(prepared.after_distance <= prepared.before_distance);
    assert!(
        prepared.after_distance < prepared.before_distance - 0.01,
        "fixture must exercise an actual colour fit"
    );
    let input = |controls| color_match::ApplyInput {
        photo_id: photo.to_db(),
        session_id: prepared.session_id.clone(),
        controls,
        cancel_id: job(),
    };
    let applied =
        color_match::apply(&state, &input(color_match::Controls::default())).expect("native apply");
    assert!(applied.full_resolution_checked && applied.changed > 0);
    let full = state
        .recipe_store()
        .load(&photo)
        .expect("read full strength")
        .expect("recipe");
    let half_controls = color_match::Controls {
        amount: 0.5,
        ..color_match::Controls::default()
    };
    color_match::apply(&state, &input(half_controls)).expect("reduce strength");
    color_match::apply(&state, &input(color_match::Controls::default())).expect("restore strength");
    assert_eq!(
        state
            .recipe_store()
            .load(&photo)
            .expect("recipe")
            .expect("saved")
            .global,
        full.global
    );
    let zero = color_match::Controls {
        amount: 0.0,
        ..color_match::Controls::default()
    };
    color_match::apply(&state, &input(zero)).expect("remove treatment");
    let restored = state
        .recipe_store()
        .load(&photo)
        .expect("restored recipe")
        .expect("saved");
    assert_eq!(restored.global, original.global);
    assert_eq!(restored.retouch, original.retouch);
    let status = color_match::status(&state, &photo.to_db())
        .expect("status")
        .expect("saved strengths");
    assert_eq!(status.controls.amount, 0.0);
    assert!(status.full_resolution_checked);
    let mut manual = restored;
    manual.global.exposure = 0.15;
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &manual,
            &["global.exposure".into()],
            "Later manual edit",
        )
        .expect("manual change");
    assert!(
        !color_match::status(&state, &photo.to_db())
            .expect("stale status")
            .expect("session")
            .current
    );
    assert!(color_match::apply(&state, &input(color_match::Controls::default())).is_err());
    assert_eq!(
        state
            .recipe_store()
            .load(&photo)
            .expect("manual recipe")
            .expect("saved")
            .global
            .exposure,
        0.15
    );
}

#[test]
#[ignore = "requires AURA_COLOR_MATCH_RAW and AURA_COLOR_MATCH_REFERENCE; isolated native RAW verification"]
fn native_raw_reference_match_does_not_touch_originals() {
    let path =
        std::path::PathBuf::from(std::env::var_os("AURA_COLOR_MATCH_RAW").expect("RAW path"));
    let reference_path = std::path::PathBuf::from(
        std::env::var_os("AURA_COLOR_MATCH_REFERENCE").expect("reference path"),
    );
    let before = blake3::hash(&std::fs::read(&path).expect("source before"));
    let directory = tempfile::tempdir().expect("isolated test");
    let (state, _, photo) = setup(&path, directory.path());
    let reference = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: reference_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("edited reference import");
    let prepared = color_match::prepare(
        &state,
        &color_match::PrepareInput {
            photo_id: photo.to_db(),
            reference_id: reference.id,
            cancel_id: job(),
        },
    )
    .expect("native RAW analysis");
    let report = color_match::apply(
        &state,
        &color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: prepared.session_id,
            controls: color_match::Controls {
                amount: 0.5,
                ..color_match::Controls::default()
            },
            cancel_id: job(),
        },
    )
    .expect("native RAW apply");
    assert!(report.full_resolution_checked);
    assert_eq!(
        before,
        blake3::hash(&std::fs::read(&path).expect("source after"))
    );
    println!(
        "{}",
        serde_json::to_string(&report).expect("verification report")
    );
}
