//! The full automatic edit shows its work: with `preview` on, a picture of the photograph before
//! the edit and after every step that changed it; with it off, none (ADR-0187).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]
use std::sync::{Arc, Mutex};

use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::full_edit::{self, FullEditInput, FullProgress, Look};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};

/// A dull, cool, underexposed frame: something the edit has reason to change.
fn dull_photo(path: &std::path::Path) {
    let image = image::RgbImage::from_fn(900, 600, |x, y| {
        let v = 40 + ((x + y) % 60) as u8;
        image::Rgb([v, v.saturating_add(6), v.saturating_add(18)])
    });
    image.save(path).expect("write the photograph");
}

fn edit(preview: bool) -> Vec<FullProgress> {
    let dir = tempfile::tempdir().expect("isolated catalog directory");
    let photos = dir.path().join("photos");
    std::fs::create_dir_all(&photos).expect("photo folder");
    dull_photo(&photos.join("dull.jpg"));
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("open isolated catalog")
        .with_cache_root(&dir.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Pictures".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("create project");
    aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("project id"),
            roots: vec![photos],
            mode: ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    let photo = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 1,
            order_by: None,
        },
    )
    .expect("list")
    .first()
    .expect("one photo")
    .id
    .clone();
    let events = Mutex::new(Vec::new());
    full_edit::run(
        &state,
        &FullEditInput {
            project_id: project.id,
            photo_id: photo,
            look: Look::None,
            preview,
        },
        &|event| events.lock().unwrap().push(event),
    )
    .expect("full edit");
    events.into_inner().unwrap()
}

#[test]
fn a_watched_edit_shows_the_original_then_every_step_that_changed_it() {
    let events = edit(true);
    let pictures: Vec<_> = events.iter().filter(|e| e.state == "picture").collect();
    let first = pictures.first().expect("a picture of the original");
    assert_eq!(first.number, 0);
    assert_eq!(
        first.picture.as_ref().map(|p| p.step.as_str()),
        Some("Original")
    );
    assert!(
        pictures.len() >= 2,
        "a dull photograph is changed by at least one step"
    );
    for event in &pictures {
        let picture = event
            .picture
            .as_ref()
            .expect("a picture event carries its picture");
        assert!(picture.image.starts_with("data:image/jpeg;base64,"));
        assert!(picture.width.max(picture.height) <= full_edit::PICTURE_EDGE);
        assert!(picture.width > 0 && picture.height > 0);
    }
    // Every picture after the original follows the stage that made it.
    for event in pictures.iter().skip(1) {
        let done = events
            .iter()
            .position(|e| std::ptr::eq(e, *event))
            .expect("in the stream");
        assert!(
            events[..done]
                .iter()
                .any(|e| e.state == "running" && e.number == event.number),
            "picture of stage {} before it started",
            event.number
        );
    }
    assert!(events
        .iter()
        .all(|e| (e.state == "picture") == e.picture.is_some()));
}

#[test]
fn an_unwatched_edit_renders_no_pictures() {
    let events = edit(false);
    assert!(!events.is_empty());
    assert!(events
        .iter()
        .all(|e| e.picture.is_none() && e.state != "picture"));
}
