//! The Cull screen's commands end to end, on a real catalog. ADR-0185.
//!
//! Four JPEGs go in - three different photographs and a black frame. The cull runs on its own
//! thread, its review is read back with a hundred checks per photograph, a left-out frame is
//! brought back and handed back again, a delivered one is left out, and every choice survives
//! closing and reopening the catalog. Nothing is deleted at any point.
use std::sync::Arc;
use std::time::{Duration, Instant};

use aura_app::contract::ipc::{
    CreateProjectInput, PhotoCullChecksInput, PhotoCullInput, PhotoCullReviewDto,
    SetPhotoCullChoiceInput, SetPhotoCullStrictnessInput,
};
use aura_app::{
    create_project, photo_cull_checks, photo_cull_review, photo_cull_status, set_photo_cull_choice,
    set_photo_cull_strictness, start_photo_cull, AppState,
};
use aura_cloud::keys::MemoryKeyStore;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_raw::codec::{encode_jpeg, Rgb8};

/// Large blocks of different tones, so each seed is a different, sharp photograph.
fn blocks(seed: u32) -> Vec<u8> {
    let (width, height) = (160_u32, 120_u32);
    (0..width * height)
        .flat_map(|i| {
            let (x, y) = (i % width, i / width);
            let cell = (x / 20) * 31 + (y / 20) * 17 + seed * 7919;
            let value = (cell.wrapping_mul(2_654_435_761).rotate_left(13) % 180 + 40) as u8;
            [value, value.saturating_sub(10), value.saturating_add(10)]
        })
        .collect()
}

fn open(path: &std::path::Path, licence: &std::path::Path) -> AppState {
    AppState::open(path)
        .expect("state")
        .with_licence_dir(licence.to_path_buf())
        .with_key_store(Arc::new(MemoryKeyStore::default()))
}

fn photo<'a>(
    review: &'a PhotoCullReviewDto,
    name: &str,
) -> &'a aura_app::contract::ipc::PhotoCullPhotoDto {
    review
        .photos
        .iter()
        .find(|p| p.file_name == name)
        .expect("photo in the review")
}

#[test]
fn a_cull_is_reviewed_corrected_and_remembered() {
    let dir = tempfile::tempdir().expect("temp");
    let catalog = dir.path().join("catalog.aura");
    let licence = dir.path().join("licence");
    let state = open(&catalog, &licence);
    let project = create_project(
        &state,
        CreateProjectInput {
            name: "Cull".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    let photos = dir.path().join("photos");
    std::fs::create_dir_all(&photos).expect("folder");
    for (name, data) in [
        ("a.jpg", blocks(1)),
        ("b.jpg", blocks(2)),
        ("c.jpg", blocks(3)),
        ("black.jpg", vec![2_u8; 160 * 120 * 3]),
    ] {
        let rgb = Rgb8 {
            width: 160,
            height: 120,
            data,
        };
        std::fs::write(photos.join(name), encode_jpeg(&rgb, 92).expect("jpeg")).expect("write");
    }
    let report = aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("id"),
            roots: vec![photos.clone()],
            mode: ImportMode::Reference,
            extensions: vec![],
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    assert_eq!(report.files_imported, 4);

    assert!(
        photo_cull_review(&state, &project.id)
            .expect("review")
            .is_none(),
        "a collection nobody culled has no review"
    );
    let job = start_photo_cull(
        &state,
        PhotoCullInput {
            project_id: project.id.clone(),
            strictness: None,
        },
    )
    .expect("started");
    let deadline = Instant::now() + Duration::from_secs(300);
    let status = loop {
        let status = photo_cull_status(&job.job_id).expect("status");
        if status.status != "running" {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "the cull did not finish: {status:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(status.status, "completed", "{status:?}");
    assert_eq!((status.done, status.total), (4, 4));

    let review = photo_cull_review(&state, &project.id)
        .expect("review")
        .expect("culled");
    assert_eq!(review.frames, 4);
    assert_eq!(review.groups.len(), 10);
    let black = photo(&review, "black.jpg");
    assert!(!black.keep, "{black:?}");
    assert_eq!(black.reason, "unusable_exposure");
    for name in ["a.jpg", "b.jpg", "c.jpg"] {
        let kept = photo(&review, name);
        assert!(kept.keep, "{kept:?}");
        assert!(kept.score.is_some());
    }
    // Every level is counted, and moving between them measures nothing again.
    assert_eq!(review.strictness, "balanced");
    let levels: Vec<&str> = review
        .levels
        .iter()
        .map(|l| l.strictness.as_str())
        .collect();
    assert_eq!(levels, ["gentle", "balanced", "strict"]);
    assert!(review.levels.windows(2).all(|w| w[0].best >= w[1].best));
    let strict = set_photo_cull_strictness(
        &state,
        SetPhotoCullStrictnessInput {
            project_id: project.id.clone(),
            strictness: "strict".into(),
        },
    )
    .expect("strict");
    assert_eq!(strict.strictness, "strict");
    assert_eq!(strict.measured_at, review.measured_at);
    assert_eq!(strict.best, review.levels[2].best);
    let back = set_photo_cull_strictness(
        &state,
        SetPhotoCullStrictnessInput {
            project_id: project.id.clone(),
            strictness: "balanced".into(),
        },
    )
    .expect("balanced");
    assert_eq!(
        back.photos, review.photos,
        "the same level is the same decision"
    );
    assert!(set_photo_cull_strictness(
        &state,
        SetPhotoCullStrictnessInput {
            project_id: project.id.clone(),
            strictness: "harsh".into(),
        },
    )
    .is_err());
    let checks = photo_cull_checks(
        &state,
        PhotoCullChecksInput {
            project_id: project.id.clone(),
            photo_id: photo(&review, "a.jpg").photo_id.clone(),
        },
    )
    .expect("checks");
    assert_eq!(checks.len(), 100);
    assert!(checks.iter().filter(|c| c.score.is_some()).count() > 40);

    // Bring the black frame back, then leave a good one out.
    let black_id = black.photo_id.clone();
    let a_id = photo(&review, "a.jpg").photo_id.clone();
    let back = set_photo_cull_choice(
        &state,
        SetPhotoCullChoiceInput {
            project_id: project.id.clone(),
            photo_ids: vec![black_id.clone()],
            choice: "keep".into(),
        },
    )
    .expect("brought back");
    assert!(photo(&back, "black.jpg").keep);
    assert_eq!(photo(&back, "black.jpg").choice.as_deref(), Some("keep"));
    assert!(!photo(&back, "black.jpg").cull_keep);
    assert_eq!((back.best, back.left_out, back.kept_by_you), (4, 0, 1));
    let out = set_photo_cull_choice(
        &state,
        SetPhotoCullChoiceInput {
            project_id: project.id.clone(),
            photo_ids: vec![a_id.clone()],
            choice: "leave_out".into(),
        },
    )
    .expect("left out");
    assert!(!photo(&out, "a.jpg").keep);
    assert!(set_photo_cull_choice(
        &state,
        SetPhotoCullChoiceInput {
            project_id: project.id.clone(),
            photo_ids: vec![a_id.clone()],
            choice: "maybe".into(),
        },
    )
    .is_err());

    // Every choice survives closing the catalog, and every photograph is still there.
    drop(state);
    let state = open(&catalog, &licence);
    let reopened = photo_cull_review(&state, &project.id)
        .expect("review")
        .expect("still culled");
    assert!(photo(&reopened, "black.jpg").keep);
    assert!(!photo(&reopened, "a.jpg").keep);
    assert_eq!(reopened.frames, 4);
    for name in ["a.jpg", "b.jpg", "c.jpg", "black.jpg"] {
        assert!(photos.join(name).exists(), "{name} must never be deleted");
    }

    // Handing a photograph back to the cull restores the measured decision.
    let auto = set_photo_cull_choice(
        &state,
        SetPhotoCullChoiceInput {
            project_id: project.id.clone(),
            photo_ids: vec![black_id, a_id],
            choice: "auto".into(),
        },
    )
    .expect("auto");
    assert!(!photo(&auto, "black.jpg").keep);
    assert!(photo(&auto, "a.jpg").keep);
    assert_eq!((auto.kept_by_you, auto.left_out_by_you), (0, 0));
}
