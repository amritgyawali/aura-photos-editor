//! Look at Match a look: every goal, one reference, real photographs, rendered to files.
//!
//! `AURA_LOOK_TARGETS` (files or folders separated by `;`), `AURA_LOOK_REFERENCE`,
//! `AURA_LOOK_OUT`, optional `AURA_LOOK_INTENTS` (default `natural,palette,exact`). Instead of the
//! targets and the reference, `AURA_LOOK_PAIRS` names a file of `target|reference` lines.
#![allow(
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::disallowed_methods
)]
use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput, RenderImageInput};
use aura_app::{color_match, AppState};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn job() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn open(path: &Path, directory: &Path) -> (AppState, PhotoId) {
    let state = AppState::open(&directory.join("catalog.aura"))
        .expect("isolated catalogue")
        .with_cache_root(&directory.join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Match a look visual check".into(),
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
    (state, photo)
}

fn decode(text: &str) -> Vec<u8> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &c) in TABLE.iter().enumerate() {
        lookup[c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for &c in text.as_bytes() {
        let value = lookup[c as usize];
        if value == 255 {
            continue;
        }
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}

fn save_render(state: &AppState, photo: PhotoId, path: &Path) {
    let rendered = aura_app::develop_commands::render_image(
        state,
        &RenderImageInput {
            photo_id: photo.to_db(),
            level: Some("screen".into()),
            screen: Some((1600, 1600)),
            colour_space: None,
            purpose: Some("export".into()),
        },
    )
    .expect("render");
    image::RgbImage::from_raw(
        rendered.width,
        rendered.height,
        decode(&rendered.rgb_base64),
    )
    .expect("pixels")
    .save(path)
    .expect("write render");
}

fn pairs() -> Vec<(PathBuf, String)> {
    if let Ok(file) = std::env::var("AURA_LOOK_PAIRS") {
        return std::fs::read_to_string(file)
            .expect("pairs file")
            .lines()
            .filter_map(|line| line.trim().split_once('|'))
            .map(|(target, reference)| (PathBuf::from(target), reference.to_string()))
            .collect();
    }
    let reference = std::env::var("AURA_LOOK_REFERENCE").expect("AURA_LOOK_REFERENCE");
    targets()
        .into_iter()
        .map(|target| (target, reference.clone()))
        .collect()
}

fn targets() -> Vec<PathBuf> {
    let list = std::env::var("AURA_LOOK_TARGETS").expect("AURA_LOOK_TARGETS");
    let mut out = Vec::new();
    for part in list.split(';').filter(|p| !p.is_empty()) {
        let path = PathBuf::from(part);
        if path.is_dir() {
            let mut files: Vec<_> = std::fs::read_dir(&path)
                .expect("folder")
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_file())
                .collect();
            files.sort();
            out.extend(files);
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
#[ignore = "requires AURA_LOOK_TARGETS and AURA_LOOK_REFERENCE (or AURA_LOOK_PAIRS), and AURA_LOOK_OUT"]
fn every_goal_rendered_for_a_look() {
    let out = PathBuf::from(std::env::var("AURA_LOOK_OUT").expect("AURA_LOOK_OUT"));
    std::fs::create_dir_all(&out).expect("output folder");
    let intents =
        std::env::var("AURA_LOOK_INTENTS").unwrap_or_else(|_| "natural,palette,exact".into());
    let mut summary = Vec::new();
    for (target, reference) in pairs() {
        let stem = target
            .file_stem()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        // A file is imported from a folder of its own, so each run sees only that photograph.
        let directory = tempfile::tempdir().expect("isolated test");
        let source = directory.path().join("source");
        std::fs::create_dir_all(&source).expect("source folder");
        let copy = source.join(target.file_name().expect("name"));
        std::fs::copy(&target, &copy).expect("copy target");
        let (state, photo) = open(&source, directory.path());
        save_render(&state, photo, &out.join(format!("{stem}-before.png")));
        let imported = color_match::import_reference(
            &state,
            &color_match::ImportInput {
                path: reference.clone(),
                cancel_id: job(),
            },
        )
        .expect("reference import");
        for intent in intents.split(',').filter(|i| !i.is_empty()) {
            let parsed: color_match::MatchIntent =
                serde_json::from_value(serde_json::Value::String(intent.into())).expect("intent");
            let started = std::time::Instant::now();
            let prepared = color_match::prepare(
                &state,
                &color_match::PrepareInput {
                    replace_previous: true,
                    intent: parsed,
                    quality: Default::default(),
                    photo_id: photo.to_db(),
                    reference_id: imported.id.clone(),
                    cancel_id: job(),
                },
            );
            let prepared = match prepared {
                Ok(report) => report,
                Err(error) => {
                    summary.push(serde_json::json!({"photo": stem, "reference": reference, "intent": intent, "prepareError": format!("{error:?}")}));
                    continue;
                }
            };
            let applied = color_match::apply(
                &state,
                &color_match::ApplyInput {
                    photo_id: photo.to_db(),
                    session_id: prepared.session_id.clone(),
                    controls: color_match::Controls::default(),
                    cancel_id: job(),
                },
            );
            let seconds = started.elapsed().as_secs_f32();
            match applied {
                Ok(report) => {
                    save_render(&state, photo, &out.join(format!("{stem}-{intent}.png")));
                    summary.push(serde_json::json!({"photo": stem, "reference": reference, "intent": intent, "seconds": seconds, "prepared": prepared, "applied": report}));
                }
                Err(error) => {
                    summary.push(serde_json::json!({"photo": stem, "reference": reference, "intent": intent, "seconds": seconds, "prepared": prepared, "applyError": format!("{error:?}")}));
                }
            }
            println!("{stem} {intent} {seconds:.1}s");
        }
        std::fs::write(
            out.join("summary.json"),
            serde_json::to_string_pretty(&summary).expect("summary"),
        )
        .expect("write summary");
    }
}
