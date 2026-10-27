//! The one-click colour match (ADR-0170) through the real commands: an unedited photo,
//! a reference chosen once, preference levels that back off, presets, previews and
//! recommendations.
#![allow(clippy::expect_used, clippy::too_many_lines, clippy::float_cmp)]
use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::{color_match, AppState};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use std::path::Path;
use std::sync::Arc;

fn setup(path: &Path, directory: &Path) -> (AppState, ProjectId, PhotoId) {
    let state = AppState::open(&directory.join("catalog.aura"))
        .expect("isolated catalogue")
        .with_cache_root(&directory.join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "One-click colour match".into(),
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
    (state, project_id, photo)
}

fn job() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// A neutral, mid-grey scene with soft structure: the photograph to be matched.
fn target(path: &Path) {
    image::RgbImage::from_fn(192, 128, |x, y| {
        let l = (70 + x / 3 + y / 3) as u8;
        image::Rgb([l, l, l.saturating_add(4)])
    })
    .save(path)
    .expect("target");
}

/// A warm, bright, different scene: an edited reference.
fn warm_reference(path: &Path) {
    image::RgbImage::from_fn(180, 120, |x, y| {
        let l = 120 + ((x * 7 + y * 3) % 90) as u8;
        image::Rgb([
            l.saturating_add(30),
            l.saturating_add(8),
            l.saturating_sub(28),
        ])
    })
    .save(path)
    .expect("reference");
}

/// A very dark, cold reference: unlike the target.
fn dark_reference(path: &Path) {
    image::RgbImage::from_fn(120, 180, |x, y| {
        let l = 10 + ((x + y) % 30) as u8;
        image::Rgb([l, l.saturating_add(6), l.saturating_add(26)])
    })
    .save(path)
    .expect("reference");
}

fn controls(amount: f32) -> color_match::Controls {
    color_match::Controls {
        amount,
        ..color_match::Controls::default()
    }
}

fn quick(
    state: &AppState,
    photo: PhotoId,
    reference: &str,
    amount: f32,
    step_down: bool,
) -> aura_app::commands::IpcResult<color_match::QuickMatchReport> {
    color_match::quick_match(
        state,
        &color_match::QuickMatchInput {
            photo_id: photo.to_db(),
            reference_id: reference.to_owned(),
            controls: controls(amount),
            cancel_id: job(),
            step_down,
            reanalyse: false,
        },
    )
}

#[test]
fn one_click_matches_an_unedited_photo_and_keeps_its_starting_point() {
    let directory = tempfile::tempdir().expect("isolated");
    let target_path = directory.path().join("target.png");
    target(&target_path);
    let reference_path = directory.path().join("refs").join("warm.png");
    std::fs::create_dir_all(reference_path.parent().expect("parent")).expect("refs");
    warm_reference(&reference_path);
    let (state, _project, photo) = setup(&target_path, directory.path());
    assert!(state.recipe_store().load(&photo).expect("load").is_none());
    let reference = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: reference_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("import");

    let first = quick(&state, photo, &reference.id, 1.0, true).expect("one-click match");
    assert!(
        first.created_baseline,
        "an unedited photo gets its starting point"
    );
    assert!(first.report.current && first.report.full_resolution_checked);
    assert_eq!(first.report.reference.id, reference.id);
    assert!(first.report.controls.amount <= first.requested_amount);
    assert_eq!(first.limited, first.report.controls.amount < 1.0);
    let history = aura_app::image_history(
        &state,
        &aura_app::contract::ipc::DevelopImageInput {
            photo_id: photo.to_db(),
        },
    )
    .expect("history");
    assert_eq!(
        history.entries.first().map(|e| e.label.as_str()),
        Some("Starting point for reference colour")
    );

    // A different preference re-uses the analysis rather than measuring again.
    let natural = quick(&state, photo, &reference.id, 0.75, true).expect("natural");
    assert_eq!(natural.report.session_id, first.report.session_id);
    assert!(!natural.created_baseline);
    assert!(natural.report.controls.amount <= 0.75 + f32::EPSILON);

    // Zero removes the matched treatment and returns to the starting point.
    let removed = quick(&state, photo, &reference.id, 0.0, true).expect("remove");
    assert_eq!(removed.report.controls.amount, 0.0);
    assert!(!removed.limited);
    let start = aura_app::develop_commands::neutral_recipe(&state, photo);
    let now = state
        .recipe_store()
        .load(&photo)
        .expect("load")
        .expect("saved");
    assert_eq!(
        aura_recipe::hash::recipe_hash(&now).expect("hash"),
        aura_recipe::hash::recipe_hash(&start).expect("hash"),
        "strength zero restores the unedited look exactly"
    );
}

#[test]
fn a_refused_strength_is_stepped_down_instead_of_failing() {
    let directory = tempfile::tempdir().expect("isolated");
    let target_path = directory.path().join("target.png");
    // A bright scene already near white: a large lift pushes it past the clipping check.
    image::RgbImage::from_fn(192, 128, |x, y| {
        let l = (150 + x / 2 + y / 4).min(250) as u8;
        image::Rgb([l, l, l])
    })
    .save(&target_path)
    .expect("bright target");
    let reference_path = directory.path().join("refs").join("high-key.png");
    std::fs::create_dir_all(reference_path.parent().expect("parent")).expect("refs");
    image::RgbImage::from_fn(180, 120, |x, y| {
        let l = (225 + (x + y) % 30) as u8;
        image::Rgb([l, l.saturating_sub(4), l.saturating_sub(12)])
    })
    .save(&reference_path)
    .expect("high-key reference");
    let (state, _project, photo) = setup(&target_path, directory.path());
    let reference = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: reference_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("import");
    let strict = quick(&state, photo, &reference.id, 2.0, false);
    let relaxed = quick(&state, photo, &reference.id, 2.0, true).expect("stepped down");
    eprintln!(
        "200% strict: {:?}; stepped: amount {} after {} attempts",
        strict
            .as_ref()
            .map(|r| r.report.controls.amount)
            .map_err(|e| e.message.clone()),
        relaxed.report.controls.amount,
        relaxed.attempts
    );
    let error = strict.expect_err("this fixture clips at 200%, so the strict request is refused");
    assert!(error.message.contains("highlight"), "{}", error.message);
    assert!(
        relaxed.limited,
        "200% was refused, so a smaller amount was saved"
    );
    assert!(relaxed.report.controls.amount < 2.0 && relaxed.report.controls.amount > 0.15);
    assert_eq!(
        relaxed.attempts, 1,
        "the small preview predicts the passing level, so one full check confirms it"
    );
    assert!(relaxed.report.changed > 0, "the smaller amount was saved");
    assert_eq!(relaxed.requested_amount, 2.0);
    assert!(relaxed.report.full_resolution_checked);
}

#[test]
fn previews_presets_and_recommendations_follow_the_library() {
    let directory = tempfile::tempdir().expect("isolated");
    let target_path = directory.path().join("target.png");
    target(&target_path);
    let refs = directory.path().join("refs");
    std::fs::create_dir_all(&refs).expect("refs");
    warm_reference(&refs.join("warm.png"));
    dark_reference(&refs.join("dark.png"));
    let (state, _project, photo) = setup(&target_path, directory.path());
    let import = |name: &str| {
        color_match::import_reference(
            &state,
            &color_match::ImportInput {
                path: refs.join(name).to_string_lossy().into_owned(),
                cancel_id: job(),
            },
        )
        .expect("import")
    };
    let warm = import("warm.png");
    let dark = import("dark.png");

    let preview = color_match::reference_preview(&state, &warm.id)
        .expect("preview")
        .expect("kept for the side-by-side view");
    assert!(preview.starts_with("data:image/jpeg;base64,"));

    let ranked = color_match::recommend_references(
        &state,
        &color_match::RecommendInput {
            photo_id: photo.to_db(),
            cancel_id: job(),
        },
    )
    .expect("recommend");
    assert_eq!(ranked.len(), 2);
    assert_eq!(
        ranked[0].reference_id, warm.id,
        "the reference closer in brightness and shape comes first: {ranked:?}"
    );
    assert!(ranked[0].score > ranked[1].score);

    let preset = |action: &str, id: Option<&str>, name: Option<&str>| {
        color_match::color_match_presets(
            &state,
            &color_match::PresetInput {
                action: action.into(),
                id: id.map(str::to_owned),
                name: name.map(str::to_owned),
                reference_id: Some(warm.id.clone()),
                level: Some("strong".into()),
                controls: Some(controls(1.25)),
            },
        )
    };
    let saved = preset("save", None, Some("Warm church")).expect("save");
    assert_eq!(saved.len(), 1);
    assert!(saved[0].available);
    assert_eq!(saved[0].reference_name, warm.name);
    let again = preset("save", None, Some("warm CHURCH")).expect("replace by name");
    assert_eq!(
        again.len(),
        1,
        "the same name replaces rather than duplicates"
    );
    let id = again[0].id.clone();
    preset("save", None, Some("Second")).expect("second");
    assert!(
        preset("rename", Some(&id), Some("Second")).is_err(),
        "names stay unique"
    );
    let renamed = preset("rename", Some(&id), Some("Golden hour")).expect("rename");
    assert!(renamed.iter().any(|p| p.name == "Golden hour"));
    assert!(preset("bogus", None, None).is_err());

    // Removing the reference keeps the preset, marked unavailable, and drops its preview.
    color_match::manage_reference(
        &state,
        &color_match::ManageInput {
            reference_id: warm.id.clone(),
            action: "remove".into(),
            name: None,
            group: None,
            favorite: None,
            default_controls: None,
        },
    )
    .expect("remove reference");
    assert!(color_match::reference_preview(&state, &warm.id)
        .expect("lookup")
        .is_none());
    let listed = preset("list", None, None).expect("list");
    assert!(listed.iter().all(|p| !p.available));
    let left = preset("remove", Some(&id), None).expect("remove preset");
    assert_eq!(left.len(), 1);
    assert!(color_match::reference_preview(&state, &dark.id)
        .expect("lookup")
        .is_some());
}

#[test]
fn the_photo_being_matched_cannot_be_its_own_reference() {
    let directory = tempfile::tempdir().expect("isolated");
    let target_path = directory.path().join("target.png");
    target(&target_path);
    let refs = directory.path().join("refs");
    std::fs::create_dir_all(&refs).expect("refs");
    // The same file, and an edited copy of the same frame: both are the same photograph.
    std::fs::copy(&target_path, refs.join("copy.png")).expect("copy");
    let graded = image::open(&target_path).expect("open").to_rgb8();
    image::RgbImage::from_fn(graded.width(), graded.height(), |x, y| {
        let p = graded.get_pixel(x, y).0;
        image::Rgb([
            p[0].saturating_add(25),
            p[1].saturating_add(8),
            p[2].saturating_sub(20),
        ])
    })
    .save(refs.join("graded-copy.png"))
    .expect("graded");
    warm_reference(&refs.join("different.png"));
    let (state, _project, photo) = setup(&target_path, directory.path());
    for name in ["copy.png", "graded-copy.png"] {
        let reference = color_match::import_reference(
            &state,
            &color_match::ImportInput {
                path: refs.join(name).to_string_lossy().into_owned(),
                cancel_id: job(),
            },
        )
        .expect("import");
        let refused = quick(&state, photo, &reference.id, 1.0, true)
            .expect_err("the same photo is refused as a reference");
        assert!(
            refused
                .message
                .starts_with("This reference is the same photo you are matching"),
            "{name}: {}",
            refused.message
        );
        assert!(
            color_match::status(&state, &photo.to_db())
                .expect("status")
                .is_none(),
            "{name}: a refused duplicate stores no match"
        );
    }
    let different = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: refs.join("different.png").to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("import");
    let matched =
        quick(&state, photo, &different.id, 1.0, true).expect("a different photo matches");
    assert!(matched.report.current);
}
