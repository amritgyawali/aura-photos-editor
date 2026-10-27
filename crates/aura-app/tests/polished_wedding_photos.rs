//! Native RAW integration evidence. Run ignored with AURA_POLISHED_PHOTOS (semicolon
//! separated source paths) and AURA_POLISHED_OUT. Uses an isolated catalog, never
//! the photographer's saved catalog, and writes review renders rather than deliveries.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::print_stdout,
    clippy::too_many_lines
)]
use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::{
    portrait_features::Options,
    smart_edit::{self, AutoRetouchInput},
    AppState,
};
use aura_core::{
    progress::{CancelToken, NullProgress},
    ImportId, PhotoId, ProjectId,
};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_recipe::retouch_tools::{self, Tool};
use aura_render::RenderService;

#[test]
#[ignore = "requires AURA_POLISHED_PHOTOS and real photographs"]
fn native_raw_finish_preserves_originals_and_obeys_portrait_policy() {
    let sources = std::env::var("AURA_POLISHED_PHOTOS").expect("source paths");
    let out =
        std::path::PathBuf::from(std::env::var_os("AURA_POLISHED_OUT").expect("review folder"));
    std::fs::create_dir_all(&out).expect("review folder");
    let mut results = Vec::new();
    for source in sources.split(';').filter(|p| !p.trim().is_empty()) {
        let path = std::path::PathBuf::from(source.trim());
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let original_hash = blake3::hash(&std::fs::read(&path).expect("source bytes"));
        let temp = tempfile::tempdir().unwrap();
        let state = AppState::open(&temp.path().join("isolated.sqlite"))
            .unwrap()
            .with_cache_root(&temp.path().join("cache"))
            .with_key_store(std::sync::Arc::new(
                aura_cloud::keys::MemoryKeyStore::default(),
            ));
        let project = aura_app::create_project(
            &state,
            CreateProjectInput {
                name: "Polished wedding validation".into(),
                couple_names: None,
                event_date: None,
            },
        )
        .unwrap();
        let imported = aura_ingest::run(
            state.catalog(),
            &ImportPlan {
                import_id: ImportId::new(),
                project_id: ProjectId::from_db(&project.id).unwrap(),
                roots: vec![path.clone()],
                mode: ImportMode::Reference,
                extensions: Vec::new(),
                extract_embedded_previews: false,
                settle_window_ms: 0,
            },
            &CancelToken::new(),
            &NullProgress,
        )
        .unwrap();
        assert_eq!(imported.files_imported, 1);
        let photo = aura_app::list_images(
            &state,
            &ListImagesInput {
                project_id: project.id.clone(),
                offset: 0,
                limit: 1,
                order_by: None,
            },
        )
        .unwrap()[0]
            .id
            .clone();
        let pid = PhotoId::from_db(&photo).unwrap();
        // Optional saved look for corrective review: preserve the actual job's
        // colour/crop while replanning in this disposable catalog.
        if let Some(root) = std::env::var_os("AURA_POLISHED_BASELINE_ROOT") {
            let seed: aura_recipe::Recipe = serde_json::from_str(
                &std::fs::read_to_string(
                    std::path::PathBuf::from(root).join(format!("Recipes/{name}.json")),
                )
                .unwrap(),
            )
            .unwrap();
            state
                .recipe_store()
                .save(
                    &ProjectId::from_db(&project.id).unwrap(),
                    &pid,
                    &seed,
                    &[],
                    "Saved review look",
                )
                .unwrap();
            aura_app::native_retouch::edit(
                &state,
                &aura_app::native_retouch::RetouchInput {
                    project_id: project.id.clone(),
                    photo_id: photo.clone(),
                    action: "clear".into(),
                    edits: vec![],
                    id: None,
                },
            )
            .unwrap();
        }
        let before = aura_app::develop_commands::load_or_neutral(&state, pid).unwrap();
        let started = std::time::Instant::now();
        let input = AutoRetouchInput {
            project_id: project.id,
            photo_id: photo,
            global: false,
            options: Options::polished_wedding(),
        };
        println!("PLANNING {name}");
        smart_edit::auto_retouch(&state, &input).expect("native polished finish");
        let after = aura_app::develop_commands::load_or_neutral(&state, pid).unwrap();
        assert_eq!(
            before.global, after.global,
            "skin pass changed global colour"
        );
        assert_eq!(before.geometry, after.geometry, "skin pass changed crop");
        let ops = retouch_tools::read(&after).unwrap();
        retouch_tools::validate(&ops).unwrap();
        let report = after
            .extra
            .get(aura_app::portrait_auto::KEY)
            .expect("portrait report");
        let faces = report["detectedFaces"].as_u64().unwrap();
        assert!(faces > 0, "validation image must have a detected face");
        if faces != 1 {
            assert!(
                ops.iter().all(|e| !matches!(
                    e.tool,
                    Tool::Frequency
                        | Tool::FrequencyHeal
                        | Tool::Dodge
                        | Tool::Burn
                        | Tool::MicroDodgeBurn
                        | Tool::PortraitDodgeBurn
                )),
                "group contains isolated-portrait tools"
            );
        }
        let mut rendered = Vec::new();
        for (label, mut recipe) in [("before", before.clone()), ("after", after.clone())] {
            recipe.geometry = aura_recipe::Geometry::default();
            recipe.global.effects = aura_recipe::Effects::default();
            println!("FULL RENDER {name} {label}");
            let frame = state
                .render()
                .unwrap()
                .render(aura_render::RenderRequest {
                    image_id: pid,
                    recipe,
                    level: aura_render::RenderLevel::Full,
                    purpose: aura_render::RenderPurpose::Export,
                    output: aura_render::OutputSpec {
                        colour_space: aura_render::OutputColour::Srgb,
                        bit_depth: 8,
                        icc: None,
                    },
                })
                .unwrap();
            let rgb = match frame.data {
                aura_render::RenderedData::Eight(v) => v,
                aura_render::RenderedData::Sixteen(v) => v.iter().map(|x| (x >> 8) as u8).collect(),
            };
            assert_eq!(rgb.len(), frame.width as usize * frame.height as usize * 3);
            let mut ppm = format!("P6\n{} {}\n255\n", frame.width, frame.height).into_bytes();
            ppm.extend_from_slice(&rgb);
            std::fs::write(out.join(format!("{name}-{label}.ppm")), ppm).unwrap();
            rendered.push((rgb, frame.width, frame.height));
        }
        assert_eq!(
            (rendered[0].1, rendered[0].2),
            (rendered[1].1, rendered[1].2)
        );
        let changed = rendered[0]
            .0
            .chunks_exact(3)
            .zip(rendered[1].0.chunks_exact(3))
            .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) >= 2))
            .count();
        assert!(changed > 0, "saved operations produced no visible changes");
        let geometry: Vec<aura_vision::portrait::PortraitFace> =
            serde_json::from_value(report["faces"].clone()).unwrap();
        let native_regions: Vec<_> = geometry
            .iter()
            .enumerate()
            .map(|(i, face)| {
                aura_app::advanced_retouch::quality::contour(
                    &rendered[0].0,
                    &rendered[1].0,
                    rendered[0].1 as usize,
                    rendered[0].2 as usize,
                    face,
                    i,
                )
                .unwrap()
            })
            .collect();
        assert!(native_regions.iter().all(|r| r.complete()));
        // Use the same dimensions, purpose and disabled finishing effects as delivery QC.
        let mut screen = Vec::new();
        for mut recipe in [before.clone(), after.clone()] {
            recipe.geometry = aura_recipe::Geometry::default();
            recipe.global.effects = aura_recipe::Effects::default();
            recipe.global.sharpen.amount = 0;
            let frame = state
                .render()
                .unwrap()
                .render(aura_render::RenderRequest {
                    image_id: pid,
                    recipe,
                    level: aura_render::RenderLevel::Screen(1600, 1200),
                    purpose: aura_render::RenderPurpose::Analysis,
                    output: aura_render::OutputSpec {
                        colour_space: aura_render::OutputColour::Srgb,
                        bit_depth: 8,
                        icc: None,
                    },
                })
                .unwrap();
            let aura_render::RenderedData::Eight(rgb) = frame.data else {
                panic!("8-bit QC output");
            };
            screen.push((rgb, frame.width, frame.height));
        }
        assert_eq!((screen[0].1, screen[0].2), (screen[1].1, screen[1].2));
        let screen_regions: Vec<_> = geometry
            .iter()
            .enumerate()
            .map(|(i, face)| {
                aura_app::advanced_retouch::quality::contour(
                    &screen[0].0,
                    &screen[1].0,
                    screen[0].1 as usize,
                    screen[0].2 as usize,
                    face,
                    i,
                )
                .unwrap()
            })
            .collect();
        assert!(screen_regions.iter().all(|r| r.complete()));
        std::fs::write(out.join(format!("{name}-regional-quality.json")),
            serde_json::to_vec_pretty(&serde_json::json!({
                "nativeDimensions": [rendered[0].1, rendered[0].2], "native": native_regions,
                "analysisDimensions": [screen[0].1, screen[0].2], "analysis": screen_regions,
                "scope": "Landmark-estimated local contour measurements, not semantic skin classification or Evoto parity.",
            })).unwrap()).unwrap();
        assert_eq!(blake3::hash(&std::fs::read(&path).unwrap()), original_hash);
        std::fs::write(
            out.join(format!("{name}-recipe.json")),
            serde_json::to_vec_pretty(&after).unwrap(),
        )
        .unwrap();
        results.push(serde_json::json!({"photo":name,"faces":faces,"operations":ops.len(),
            "dimensions":[rendered[1].1,rendered[1].2],"changedPixels":changed,
            "sourceUnchanged":true,"globalAndCropPreserved":true,
            "seconds":started.elapsed().as_secs_f32(),"sourceBlake3":original_hash.to_hex().to_string()}));
        std::fs::write(
            out.join("results.json"),
            serde_json::to_vec_pretty(&results).unwrap(),
        )
        .unwrap();
        println!("VERIFIED {name}: {faces} faces, {} changed pixels", changed);
    }
}
