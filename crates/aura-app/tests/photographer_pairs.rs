//! AURA's automatic edit beside the photographer's own finished photograph, for measuring how
//! close it comes on every kind of photograph. ADR-0191.
//!
//! Ignored. `AURA_PAIRS` names a text file with one camera file per line (each has a finished
//! JPEG from the photographer elsewhere; `scripts/photographer-pairs.py` pairs and scores
//! them). Each camera file is imported into a throwaway catalogue and, into `AURA_PAIRS_OUT`:
//!
//! * `<name>-shot.png`: the camera file as shot (no edit), 960 px;
//! * `<name>-aura.png`: after the full automatic edit, 960 px (only with `AURA_PAIRS_EDIT=1`);
//! * `<name>.json`: the faces found and every stage's decisions.
//!
//! `AURA_PAIRS_BASICS=1` runs only the measured white balance, tone and colour
//! (`full_edit::basics_only`, a few seconds a photograph) in place of the full edit, and
//! `AURA_PAIRS_VARIANT` names the output (`<name>-<variant>.png`, default `aura`).
//!
//! A photograph whose outputs exist is skipped, so a long run can be resumed.
//!
//! ```text
//! AURA_PAIRS=I:/aura-any/sample-raws.txt AURA_PAIRS_OUT=I:/aura-any/out AURA_PAIRS_EDIT=1 \
//!   cargo test -p aura-app --release --test photographer_pairs -- --ignored --nocapture
//! ```
#![allow(clippy::expect_used, clippy::print_stderr, clippy::disallowed_methods)]
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::full_edit::{self, FullEditInput, Look};
use aura_app::{create_project, AppState};
use aura_cloud::keys::MemoryKeyStore;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_render::RenderService;

fn render(
    state: &AppState,
    photo: PhotoId,
    recipe: aura_recipe::Recipe,
) -> Option<image::RgbImage> {
    let result = state
        .render()
        .ok()?
        .render(aura_render::RenderRequest {
            image_id: photo,
            recipe,
            level: aura_render::RenderLevel::Screen(960, 960),
            purpose: aura_render::RenderPurpose::Export,
            output: aura_render::OutputSpec {
                colour_space: aura_render::OutputColour::Srgb,
                bit_depth: 8,
                icc: None,
            },
        })
        .ok()?;
    let bytes = match &result.data {
        aura_render::RenderedData::Eight(v) => v.clone(),
        aura_render::RenderedData::Sixteen(v) => v.iter().map(|x| (x >> 8) as u8).collect(),
    };
    image::RgbImage::from_raw(result.width, result.height, bytes)
}

fn stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_uppercase())
        .unwrap_or_default()
}

#[test]
#[ignore = "needs AURA_PAIRS, a list of camera files the photographer finished"]
fn the_automatic_edit_beside_the_photographers() {
    let (Some(list), Some(out)) = (
        std::env::var_os("AURA_PAIRS"),
        std::env::var_os("AURA_PAIRS_OUT"),
    ) else {
        eprintln!("AURA_PAIRS and AURA_PAIRS_OUT are not set");
        return;
    };
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).expect("out");
    let basics = std::env::var_os("AURA_PAIRS_BASICS").is_some_and(|v| v == "1");
    let edit = basics || std::env::var_os("AURA_PAIRS_EDIT").is_some_and(|v| v == "1");
    let variant = std::env::var("AURA_PAIRS_VARIANT").unwrap_or_else(|_| "aura".into());
    let files: Vec<String> = std::fs::read_to_string(&list)
        .expect("list")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .filter(|f| {
            let name = stem(f);
            let done = out.join(format!("{name}-shot.png")).exists()
                && (!edit || out.join(format!("{name}-{variant}.png")).exists());
            !done && Path::new(f).exists()
        })
        .collect();
    eprintln!("{} camera file(s) to do", files.len());
    for chunk in files.chunks(12) {
        let dir = tempfile::tempdir().expect("temp");
        let state = AppState::open(&dir.path().join("catalog.aura"))
            .expect("state")
            .with_licence_dir(dir.path().join("licence"))
            .with_key_store(Arc::new(MemoryKeyStore::default()));
        let project = create_project(
            &state,
            CreateProjectInput {
                name: "Pairs".into(),
                couple_names: None,
                event_date: None,
            },
        )
        .expect("project");
        aura_ingest::run(
            state.catalog(),
            &ImportPlan {
                import_id: ImportId::new(),
                project_id: ProjectId::from_db(&project.id).expect("id"),
                roots: chunk.iter().map(PathBuf::from).collect(),
                mode: ImportMode::Reference,
                extensions: vec![],
                extract_embedded_previews: true,
                settle_window_ms: 0,
            },
            &CancelToken::new(),
            &NullProgress,
        )
        .expect("import");
        let photos = aura_app::list_images(
            &state,
            &ListImagesInput {
                project_id: project.id.clone(),
                offset: 0,
                limit: 1000,
                order_by: Some("timeline".into()),
            },
        )
        .expect("photos");
        for photo in &photos {
            let name = stem(&photo.file_name);
            let id = PhotoId::from_db(&photo.id).expect("id");
            let before = aura_app::develop_commands::load_or_neutral(&state, id).expect("neutral");
            let shot_path = out.join(format!("{name}-shot.png"));
            if !shot_path.exists() {
                if let Some(shot) = render(&state, id, before) {
                    shot.save(&shot_path).expect("shot");
                }
            }
            if !edit {
                continue;
            }
            if basics {
                let started = Instant::now();
                match full_edit::basics_only(&state, &project.id, &photo.id) {
                    Ok(lines) => {
                        let after = aura_app::develop_commands::load_or_neutral(&state, id)
                            .expect("edited");
                        if let Some(done) = render(&state, id, after.clone()) {
                            done.save(out.join(format!("{name}-{variant}.png")))
                                .expect("basics");
                        }
                        let g = &after.global;
                        let report = serde_json::json!({
                            "file": photo.file_name,
                            "seconds": started.elapsed().as_secs_f32(),
                            "recipe": {
                                "exposure": g.exposure, "contrast": g.contrast, "highlights": g.highlights,
                                "shadows": g.shadows, "whites": g.whites, "blacks": g.blacks,
                                "temperature": g.temperature, "tint": g.tint, "vibrance": g.vibrance,
                                "saturation": g.saturation,
                            },
                            "lines": lines,
                        });
                        std::fs::write(
                            out.join(format!("{name}-{variant}.json")),
                            serde_json::to_string_pretty(&report).unwrap_or_default(),
                        )
                        .expect("report");
                        eprintln!("{name}: {:.1}s", started.elapsed().as_secs_f32());
                    }
                    Err(error) => eprintln!("{name}: FAILED {} {}", error.code, error.message),
                }
                continue;
            }
            let started = Instant::now();
            let result = full_edit::run(
                &state,
                &FullEditInput {
                    project_id: project.id.clone(),
                    photo_id: photo.id.clone(),
                    look: Look::None,
                    preview: false,
                },
                &|_| {},
            );
            let seconds = started.elapsed().as_secs_f32();
            let edited = match result {
                Ok(edited) => edited,
                Err(error) => {
                    eprintln!("{name}: FAILED {} {}", error.code, error.message);
                    continue;
                }
            };
            let after = aura_app::develop_commands::load_or_neutral(&state, id).expect("edited");
            if let Some(done) = render(&state, id, after.clone()) {
                done.save(out.join(format!("{name}-{variant}.png")))
                    .expect("aura");
            }
            let g = &after.global;
            let report = serde_json::json!({
                "file": photo.file_name,
                "seconds": seconds,
                "faces": edited.faces,
                "recipe": {
                    "exposure": g.exposure, "contrast": g.contrast, "highlights": g.highlights,
                    "shadows": g.shadows, "whites": g.whites, "blacks": g.blacks,
                    "temperature": g.temperature, "tint": g.tint, "vibrance": g.vibrance,
                    "saturation": g.saturation, "clarity": g.clarity, "dehaze": g.dehaze,
                },
                "stages": edited.stages.iter().map(|s| serde_json::json!({
                    "number": s.number,
                    "title": s.title,
                    "outcome": full_edit::outcome_label(s.outcome),
                    "changes": s.changes,
                    "checks": s.checks,
                })).collect::<Vec<_>>(),
            });
            std::fs::write(
                out.join(format!("{name}.json")),
                serde_json::to_string_pretty(&report).unwrap_or_default(),
            )
            .expect("report");
            eprintln!("{name}: {seconds:.1}s, {} face(s)", edited.faces);
        }
    }
}
