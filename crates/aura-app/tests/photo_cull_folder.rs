//! The Cull screen's whole path on a real folder, for checking decisions by eye. ADR-0185.
//!
//! Ignored. `AURA_CULL_FOLDER` names a folder of photographs (any format AURA imports). They
//! are imported into a throwaway catalog, culled exactly as the Cull screen culls them, and
//! every decision is printed in shooting order with its time, score, burst and moment.
//! `AURA_CULL_FOLDER_OUT` also writes each photograph's thumbnail, named
//! `<moment>-<keep|out>-<score>-<file>.jpg`, so a group can be looked at side by side.
//! `AURA_CULL_STRICTNESS` (`gentle`, `balanced`, `strict`) culls at that strictness.
//!
//! ```text
//! AURA_CULL_FOLDER="H:/AURA EDIT" cargo test -p aura-app --test photo_cull_folder -- --ignored --nocapture
//! ```
use std::sync::Arc;
use std::time::{Duration, Instant};

use aura_app::contract::ipc::{CreateProjectInput, PhotoCullInput};
use aura_app::{create_project, photo_cull_review, photo_cull_status, start_photo_cull, AppState};
use aura_cloud::keys::MemoryKeyStore;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_preview::contract::service::{PreviewService, Priority};

#[test]
#[ignore = "needs AURA_CULL_FOLDER, a folder of real photographs"]
fn cull_a_real_folder() {
    let Some(folder) = std::env::var_os("AURA_CULL_FOLDER") else {
        eprintln!("AURA_CULL_FOLDER is not set");
        return;
    };
    let dir = tempfile::tempdir().expect("temp");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("state")
        .with_licence_dir(dir.path().join("licence"))
        .with_key_store(Arc::new(MemoryKeyStore::default()));
    let project = create_project(
        &state,
        CreateProjectInput {
            name: "Folder".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    let started = Instant::now();
    let report = aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("id"),
            roots: vec![folder.into()],
            mode: ImportMode::Reference,
            extensions: vec![],
            extract_embedded_previews: true,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    eprintln!(
        "imported {} in {:.1}s",
        report.files_imported,
        started.elapsed().as_secs_f32()
    );
    let started = Instant::now();
    let job = start_photo_cull(
        &state,
        PhotoCullInput {
            project_id: project.id.clone(),
            strictness: std::env::var("AURA_CULL_STRICTNESS").ok(),
        },
    )
    .expect("started");
    let status = loop {
        let status = photo_cull_status(&job.job_id).expect("status");
        if status.status != "running" {
            break status;
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let seconds = started.elapsed().as_secs_f32();
    assert_eq!(status.status, "completed", "{status:?}");
    let review = photo_cull_review(&state, &project.id)
        .expect("review")
        .expect("culled");
    eprintln!(
        "culled {} in {seconds:.1}s ({:.2}s a photo): {}",
        review.frames,
        seconds / review.frames.max(1) as f32,
        review.summary
    );
    let out = std::env::var_os("AURA_CULL_FOLDER_OUT").map(std::path::PathBuf::from);
    if let Some(out) = &out {
        std::fs::create_dir_all(out).expect("out");
    }
    let previews = state.previews(&project.id).expect("previews");
    let times = aura_app::measured_cull::camera_times(&state, &project.id).expect("times");
    let first = times.values().min().copied().unwrap_or(0);
    for photo in &review.photos {
        let group = photo
            .burst
            .map(|b| format!("b{b}"))
            .or_else(|| photo.moment.map(|m| format!("m{m}")))
            .unwrap_or_else(|| "-".into());
        let at = times.get(&photo.photo_id).map_or_else(
            || "untimed".into(),
            |t| format!("{:.1}s", (t - first) as f32 / 1000.0),
        );
        eprintln!(
            "{:<28} {:>9} {:<5} {:<5} {:>5} {}{}  {}",
            photo.file_name,
            at,
            group,
            if photo.keep { "keep" } else { "out" },
            photo
                .score
                .map_or_else(|| "-".into(), |s| format!("{s:.0}")),
            photo.reason_label,
            if photo.top_pick { " *" } else { "" },
            photo.detail
        );
        let Some(out) = &out else { continue };
        let id = PhotoId::from_db(&photo.photo_id).expect("id");
        let Ok(thumb) = previews.get(id, aura_raw::PixelLevel::Thumb(512), Priority::Interactive)
        else {
            continue;
        };
        let Some(rgb) = thumb.as_srgb8() else {
            continue;
        };
        let Some(image) = image::RgbImage::from_raw(thumb.width, thumb.height, rgb.to_vec()) else {
            continue;
        };
        let name = format!(
            "{group}-{}-{:03.0}-{}.jpg",
            if photo.keep { "keep" } else { "out" },
            photo.score.unwrap_or(0.0),
            photo.file_name
        );
        image.save(out.join(name)).expect("thumbnail");
    }
}
