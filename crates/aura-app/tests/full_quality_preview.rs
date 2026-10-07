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
#![allow(clippy::expect_used, clippy::print_stdout, clippy::disallowed_methods)]
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
    // A brush stroke on top of the saved retouch: only the new operation is rendered.
    let draft = |quality: &str| native_retouch::DraftInput {
        project_id: project.id.clone(),
        photo_id: photo.clone(),
        edit: serde_json::from_value(serde_json::json!({
            "id": "draft", "tool": "dodge", "enabled": true, "region": [0.5, 0.4, 0.05, 0.05],
            "source": null, "amount": 0.3, "feather": 0.7, "radius": 0.003, "texture": 1.0,
            "tone": 0.5, "warmth": 0.0, "tint": 0.0
        }))
        .expect("draft"),
        replace_id: None,
        quality: Some(quality.into()),
    };
    let (_, draft_fast_ms) = time("draft stroke, quick look", &|| {
        native_retouch::draft_preview(&state, &draft("fast")).expect("draft fast")
    });
    let (_, draft_full_ms) = time("draft stroke, full quality", &|| {
        native_retouch::draft_preview(&state, &draft("full")).expect("draft full")
    });
    println!(
        "(a stroke re-renders one operation: {draft_fast_ms} ms quick, {draft_full_ms} ms full)"
    );
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
    // The disk copy is not written when the disk is nearly full (ADR-0097); then the second
    // run renders again, with the same pixels.
    if disk_ms >= 4_000 {
        println!("disk cache not used: the cache folder's disk has under 2 GB free");
    }
    // A slider moves twice: after the first exact quick look, the second move is answered by
    // the live estimate at once, then the exact quick look follows.
    let exposure = |value: f64| {
        aura_app::set_param(
            &restarted,
            &aura_app::contract::ipc::SetParamInput {
                project_id: project.id.clone(),
                photo_id: photo.clone(),
                path: "global.exposure".into(),
                value: serde_json::json!(value),
                label: Some("Exposure".into()),
            },
        )
        .expect("set exposure");
    };
    exposure(1.1);
    time("slider moved, exact quick look", &|| {
        native_retouch::preview_at(&restarted, &project.id, &photo, false, Quality::Fast)
            .expect("fast")
    });
    exposure(1.37);
    let live_started = Instant::now();
    let live = native_retouch::live_preview(&restarted, &project.id, &photo, "retouch")
        .expect("live")
        .expect("an estimate after a render of the same stack");
    println!(
        "{:<34} {:>5} x {:<5} {:>7} ms",
        "slider moved again, live estimate",
        live.width,
        live.height,
        live_started.elapsed().as_millis()
    );
    time("slider moved again, exact", &|| {
        native_retouch::preview_at(&restarted, &project.id, &photo, false, Quality::Fast)
            .expect("fast")
    });
}
