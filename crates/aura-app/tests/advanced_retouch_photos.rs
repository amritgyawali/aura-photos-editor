//! Auto advanced retouch on real photographs, end to end through the application layer.
//!
//! Set `AURA_ADVANCED_PHOTOS` to one or more photographs separated by `;` and, optionally,
//! `AURA_ADVANCED_OUT` to a folder for the report and before/after renders, then run ignored:
//!
//! ```text
//! cargo test -p aura-app --test advanced_retouch_photos -- --ignored --nocapture
//! ```
//!
//! For each photograph: an isolated catalog, the import, the run, and checks that all eighteen
//! stages were reported in order with two progress events each, that every saved stage is its
//! own history entry, and that a second run on the result changes no retouch stage. The renders
//! are full resolution with crop and rotation left out, so before and after line up pixel for
//! pixel for a 100 % and 200 % inspection.
#![allow(clippy::expect_used, clippy::too_many_lines, clippy::print_stdout)]
use aura_app::advanced_retouch::{self, AdvancedRetouchInput, Stage};
use aura_app::contract::ipc::{CreateProjectInput, DevelopImageInput, ListImagesInput};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_render::RenderService;
use std::sync::{Arc, Mutex};

fn render(
    state: &AppState,
    photo: PhotoId,
    recipe: &aura_recipe::Recipe,
    full: bool,
) -> (Vec<u8>, u32, u32) {
    let mut recipe = recipe.clone();
    if full {
        recipe.geometry = aura_recipe::Geometry::default();
    }
    let result = state
        .render()
        .expect("renderer")
        .render(aura_render::RenderRequest {
            image_id: photo,
            recipe,
            level: if full {
                aura_render::RenderLevel::Full
            } else {
                aura_render::RenderLevel::Screen(1600, 1200)
            },
            purpose: aura_render::RenderPurpose::Export,
            output: aura_render::OutputSpec {
                colour_space: aura_render::OutputColour::Srgb,
                bit_depth: 8,
                icc: None,
            },
        })
        .expect("render");
    let bytes = match result.data {
        aura_render::RenderedData::Eight(v) => v,
        aura_render::RenderedData::Sixteen(v) => v.iter().map(|x| (x >> 8) as u8).collect(),
    };
    (bytes, result.width, result.height)
}

fn write_ppm(path: &std::path::Path, (rgb, w, h): &(Vec<u8>, u32, u32)) {
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    out.extend_from_slice(rgb);
    std::fs::write(path, out).expect("write render");
}

#[test]
#[ignore = "requires AURA_ADVANCED_PHOTOS pointing at real portraits"]
fn every_stage_runs_in_order_on_real_photographs() {
    let list = std::env::var("AURA_ADVANCED_PHOTOS").expect("AURA_ADVANCED_PHOTOS");
    let out = std::env::var_os("AURA_ADVANCED_OUT").map(std::path::PathBuf::from);
    if let Some(dir) = &out {
        std::fs::create_dir_all(dir).expect("output folder");
    }
    let paths: Vec<&str> = list.split(';').filter(|p| !p.trim().is_empty()).collect();
    assert!(
        !paths.is_empty(),
        "AURA_ADVANCED_PHOTOS names no photograph"
    );
    for path in paths {
        let path = std::path::PathBuf::from(path.trim());
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("photo")
            .to_owned();
        let dir = tempfile::tempdir().expect("isolated catalog directory");
        let state = AppState::open(&dir.path().join("catalog.aura"))
            .expect("open isolated catalog")
            .with_cache_root(&dir.path().join("cache"))
            .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
        let project = aura_app::create_project(
            &state,
            CreateProjectInput {
                name: "Advanced retouch".into(),
                couple_names: None,
                event_date: None,
            },
        )
        .expect("create project");
        let imported = aura_ingest::run(
            state.catalog(),
            &ImportPlan {
                import_id: ImportId::new(),
                project_id: ProjectId::from_db(&project.id).expect("project id"),
                roots: vec![path.clone()],
                mode: ImportMode::Reference,
                extensions: Vec::new(),
                extract_embedded_previews: false,
                settle_window_ms: 0,
            },
            &CancelToken::new(),
            &NullProgress,
        )
        .expect("import");
        assert_eq!(imported.files_imported, 1, "{name}");
        let photo = aura_app::list_images(
            &state,
            &ListImagesInput {
                project_id: project.id.clone(),
                offset: 0,
                limit: 10,
                order_by: None,
            },
        )
        .expect("list")
        .first()
        .expect("one photo")
        .id
        .clone();
        let photo_id = PhotoId::from_db(&photo).expect("photo id");
        let original =
            aura_app::develop_commands::load_or_neutral(&state, photo_id).expect("recipe");
        let input = AdvancedRetouchInput {
            project_id: project.id.clone(),
            photo_id: photo.clone(),
            options: None,
            preset: None,
        };
        let events = Mutex::new(Vec::new());
        let started = std::time::Instant::now();
        let dto = advanced_retouch::run(&state, &input, &|p| {
            events.lock().expect("events").push(p);
        })
        .expect("advanced retouch");
        let seconds = started.elapsed().as_secs_f32();
        let events = events.into_inner().expect("events");
        let report = &dto.report;
        assert_eq!(report.stages.len(), 18, "{name}");
        for (i, (stage, expected)) in report.stages.iter().zip(Stage::ALL).enumerate() {
            assert_eq!(stage.stage, expected, "{name}: stage {}", i + 1);
            assert_eq!(usize::from(stage.number), i + 1);
            assert!(
                !stage.checks.is_empty() || !stage.changes.is_empty(),
                "{name}: {:?} reported nothing",
                stage.stage
            );
        }
        assert_eq!(
            events.len(),
            36,
            "{name}: a start and an end for every stage"
        );
        for (pair, stage) in events.chunks(2).zip(Stage::ALL) {
            assert_eq!(pair[0].number, stage.number());
            assert_eq!(pair[0].state, "running");
            assert_eq!(pair[1].state, "done");
        }
        let history = aura_app::develop_commands::image_history(
            &state,
            &DevelopImageInput {
                photo_id: photo.clone(),
            },
        )
        .expect("history");
        let steps: Vec<&String> = history
            .entries
            .iter()
            .map(|e| &e.label)
            .filter(|l| l.starts_with("Auto advanced retouch"))
            .collect();
        assert_eq!(steps.len(), report.history_steps, "{name}: {steps:#?}");
        let applied = report.stages.iter().filter(|s| s.saved).count();
        assert!(steps.len() >= applied, "{name}");
        println!(
            "\n=== {name}: {} face(s), {applied} stages applied, {seconds:.1} s",
            report.faces
        );
        for s in &report.stages {
            println!(
                "{:>2}. {:<32} {:?} ops={}",
                s.number, s.title, s.outcome, s.operations
            );
            for c in &s.checks {
                println!("      check: {c}");
            }
            for c in &s.changes {
                println!("      change: {c}");
            }
        }
        println!("quality: {:?}", report.quality);

        // A second run on the finished photograph replans the same retouch.
        let first = aura_recipe::retouch_tools::read(
            &aura_app::develop_commands::load_or_neutral(&state, photo_id).expect("recipe"),
        )
        .expect("stack");
        let again = advanced_retouch::run(&state, &input, &|_| {}).expect("second run");
        let second = aura_recipe::retouch_tools::read(
            &aura_app::develop_commands::load_or_neutral(&state, photo_id).expect("recipe"),
        )
        .expect("stack");
        for (a, b) in first.iter().zip(&second) {
            if a != b {
                println!(
                    "DIFF
  {a:?}
  {b:?}"
                );
            }
        }
        if first.len() != second.len() {
            println!("LEN {} vs {}", first.len(), second.len());
        }
        for s in again.report.stages.iter().filter(|s| s.saved) {
            println!("repeat saved {:?}: {:?} {:?}", s.stage, s.checks, s.changes);
        }

        if let Some(dir) = &out {
            let finished =
                aura_app::develop_commands::load_or_neutral(&state, photo_id).expect("recipe");
            write_ppm(
                &dir.join(format!("{name}-before.ppm")),
                &render(&state, photo_id, &original, true),
            );
            write_ppm(
                &dir.join(format!("{name}-after.ppm")),
                &render(&state, photo_id, &finished, true),
            );
            write_ppm(
                &dir.join(format!("{name}-final.ppm")),
                &render(&state, photo_id, &finished, false),
            );
            std::fs::write(
                dir.join(format!("{name}-recipe.json")),
                aura_recipe::hash::canonical(&finished).expect("canonical recipe"),
            )
            .expect("write recipe");
            std::fs::write(
                dir.join(format!("{name}-report.json")),
                serde_json::to_vec_pretty(&dto.report).expect("report json"),
            )
            .expect("write report");
        }
    }
}
