//! Full-quality editing previews on a real photograph: resolution, speed and the caches.
//!
//! ```text
//! AURA_PREVIEW_PHOTO=C:\photos\portrait.jpg cargo test -p aura-app --test full_quality_preview -- --ignored --nocapture
//! ```
//!
//! An isolated catalog imports the photograph and Auto advanced retouch edits it. Then the
//! retouch view's preview is asked for the way the window asks: the fast first look, the
//! full-quality preview, the same again (memory), and again from a second application state over
//! the same cache folder (disk, as after a restart). The full-quality preview must be the
//! original's own size and pixel-identical every time; the cached answers must be fast.
#![allow(clippy::expect_used, clippy::print_stdout)]
use aura_app::advanced_retouch::{self, AdvancedRetouchInput};
use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::native_retouch::{self, Quality};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use std::sync::Arc;
use std::time::Instant;

fn open(dir: &std::path::Path) -> AppState {
    AppState::open(&dir.join("catalog.aura"))
        .expect("open isolated catalog")
        .with_cache_root(&dir.join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()))
}

#[test]
#[ignore = "requires AURA_PREVIEW_PHOTO pointing at a real photograph"]
fn the_full_quality_preview_is_the_original_size_and_cached() {
    let path =
        std::path::PathBuf::from(std::env::var("AURA_PREVIEW_PHOTO").expect("AURA_PREVIEW_PHOTO"));
    let dir = tempfile::tempdir().expect("isolated catalog directory");
    let state = open(dir.path());
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Previews".into(),
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
            roots: vec![path],
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
    advanced_retouch::run(
        &state,
        &AdvancedRetouchInput {
            project_id: project.id.clone(),
            photo_id: photo.clone(),
            options: None,
        },
        &|_| {},
    )
    .expect("auto advanced retouch");

    let time = |label: &str, run: &dyn Fn() -> aura_app::contract::ipc::RenderDto| {
        let started = Instant::now();
        let image = run();
        let ms = started.elapsed().as_millis();
        println!(
            "{label:<34} {:>5} x {:<5} {ms:>7} ms",
            image.width, image.height
        );
        (image, ms)
    };
    let (fast, _) = time("fast first look", &|| {
        native_retouch::preview_at(&state, &project.id, &photo, false, Quality::Fast).expect("fast")
    });
    let (full, _) = time("full quality, rendered", &|| {
        native_retouch::preview_at(&state, &project.id, &photo, false, Quality::Full).expect("full")
    });
    let (again, memory_ms) = time("full quality, from memory", &|| {
        native_retouch::preview_at(&state, &project.id, &photo, false, Quality::Full)
            .expect("again")
    });
    let (original, _) = time("original, full quality", &|| {
        native_retouch::original_at(&state, &project.id, &photo, Quality::Full).expect("original")
    });
    let (before, _) = time("before retouch, full quality", &|| {
        native_retouch::preview_at(&state, &project.id, &photo, true, Quality::Full)
            .expect("before")
    });
    // A restart: the catalog is closed and opened again, and memory starts empty.
    drop(state);
    let restarted = open(dir.path());
    let (disk, disk_ms) = time("full quality, from disk", &|| {
        native_retouch::preview_at(&restarted, &project.id, &photo, false, Quality::Full)
            .expect("disk")
    });

    assert!(fast.width.max(fast.height) <= 1600);
    assert!(
        full.width.max(full.height) > fast.width.max(fast.height)
            || full.width.max(full.height) <= 1600
    );
    assert_eq!((original.width, original.height), (full.width, full.height));
    assert_eq!((before.width, before.height), (full.width, full.height));
    assert_eq!(
        again.rgb_base64, full.rgb_base64,
        "memory returns the same pixels"
    );
    assert_eq!(
        disk.rgb_base64, full.rgb_base64,
        "disk returns the same pixels"
    );
    assert_eq!(disk.render_hash, full.render_hash);
    assert_ne!(original.rgb_base64, full.rgb_base64, "the edit is visible");
    assert!(memory_ms < 2_000, "memory hit took {memory_ms} ms");
    assert!(disk_ms < 4_000, "disk hit took {disk_ms} ms");
}
