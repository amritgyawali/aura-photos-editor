//! Import, persisted operations, history, rendering and delivery through native commands.
#![allow(
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::disallowed_methods
)]
use aura_app::contract::ipc::{
    CreateProjectInput, DevelopImageInput, ExportJobInput, HistoryStepInput, ListImagesInput,
};
use aura_app::native_retouch::{self, RetouchInput};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_raw::codec::{encode_jpeg, Rgb8};
use aura_recipe::retouch_tools::Edit;

#[test]
fn material_tools_save_render_undo_redo_and_export_without_changing_the_source() {
    workflow(&[
        "fabric_moire",
        "fabric_stain",
        "shadow_balance",
        "background_blur",
        "highlight_glow",
    ]);
}

#[test]
fn optical_tools_save_render_undo_redo_and_export_without_changing_the_source() {
    workflow(&[
        "motion_blur",
        "zoom_blur",
        "color_despill",
        "highlight_compress",
    ]);
}

#[test]
fn sampled_colour_saturation_saves_renders_and_exports() {
    workflow(&["color_saturation"]);
}

fn workflow(tools: &[&str]) {
    let dir = tempfile::tempdir().expect("isolated directory");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("catalog")
        .with_cache_root(&dir.path().join("cache"))
        .with_licence_dir(dir.path().join("licence"));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Material workflow".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project")
    .id;
    let original = encode_jpeg(
        &Rgb8 {
            width: 96,
            height: 96,
            data: (0..96 * 96)
                .flat_map(|i| {
                    let x = i % 96;
                    if x < 32 {
                        [80 + (i % 8) as u8, 70, 65]
                    } else if x < 64 {
                        [200 + (i % 20) as u8, 160 + (i % 30) as u8, 120]
                    } else {
                        [60 + (i % 20) as u8, 210 + (i % 30) as u8, 85]
                    }
                })
                .collect(),
        },
        98,
    )
    .expect("fixture JPEG");
    let path = dir.path().join("fabric.jpg");
    std::fs::write(&path, &original).expect("fixture");
    aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project).expect("project id"),
            roots: vec![path.clone()],
            mode: ImportMode::Reference,
            extensions: vec![],
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
            project_id: project.clone(),
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("images")[0]
        .id
        .clone();
    let recipe_input = DevelopImageInput {
        photo_id: photo.clone(),
    };
    let mut request = RetouchInput {
        project_id: project.clone(),
        photo_id: photo.clone(),
        action: "append".into(),
        edits: vec![],
        id: None,
    };
    for &tool in tools {
        let before = aura_app::image_recipe(&state, &recipe_input).expect("before");
        let before_pixels =
            native_retouch::preview(&state, &project, &photo, false).expect("before pixels");
        let operation: Edit = serde_json::from_value(serde_json::json!({
            "id":"draft", "tool":tool, "enabled":true, "region":[0.5,0.5,1,1],
            "source":[0.8,0.5], "amount":0.8, "feather":0.0, "radius":0.04,
            "texture":1.0, "tone":if tool == "highlight_glow" { 0.0 } else { 0.2 },
            "targetColor":if tool == "color_despill" { Some([0.0,1.0,0.0]) } else { None },
            "warmth":if tool == "color_saturation" { -0.8 } else { 0.0 }, "tint":0.0,
            "selection":if tool == "color_saturation" { Some(serde_json::json!({"color":{
                "sample":[0.8,0.5], "tolerance":15.0, "softness":10.0, "lightness":0.5
            }})) } else { None }
        }))
        .expect("operation");
        request.edits = vec![operation];
        native_retouch::edit(&state, &request).expect("save new tool");
        let after = aura_app::image_recipe(&state, &recipe_input).expect("after");
        assert_ne!(before.recipe_hash, after.recipe_hash, "{tool}");
        let rendered =
            native_retouch::preview(&state, &project, &photo, false).expect("new tool preview");
        assert_ne!(
            before_pixels.rgb_base64, rendered.rgb_base64,
            "{tool} must affect pixels"
        );
        let mut history = HistoryStepInput {
            project_id: project.clone(),
            photo_id: photo.clone(),
            action: "undo".into(),
        };
        aura_app::history_step(&state, &history).expect("undo");
        assert_eq!(
            aura_app::image_recipe(&state, &recipe_input)
                .expect("undo recipe")
                .recipe_hash,
            before.recipe_hash
        );
        assert_eq!(
            native_retouch::preview(&state, &project, &photo, false)
                .expect("undo pixels")
                .rgb_base64,
            before_pixels.rgb_base64
        );
        history.action = "redo".into();
        aura_app::history_step(&state, &history).expect("redo");
        assert_eq!(
            native_retouch::preview(&state, &project, &photo, false)
                .expect("redo pixels")
                .rgb_base64,
            rendered.rgb_base64
        );
    }
    let before_invalid = aura_app::image_recipe(&state, &recipe_input).expect("saved stack");
    request.edits[0].tool = aura_recipe::retouch_tools::Tool::FabricStain;
    request.edits[0].source = None;
    assert!(native_retouch::edit(&state, &request).is_err());
    assert_eq!(
        aura_app::image_recipe(&state, &recipe_input)
            .expect("failure preserves stack")
            .recipe_hash,
        before_invalid.recipe_hash
    );

    let presets = aura_app::export_presets().expect("presets");
    let mut preset = serde_json::to_value(
        presets
            .iter()
            .find(|p| p.format == "jpeg")
            .expect("JPEG preset"),
    )
    .expect("preset");
    preset["imageIds"] = serde_json::json!([photo]);
    let destination = dir.path().join("export");
    let job: ExportJobInput = serde_json::from_value(serde_json::json!({
        "projectId":project, "sets":[preset], "destination":destination,
        "destinationKind":"folder", "copyright":null, "contact":null, "creator":null,
        "keywords":[], "stripGps":true, "stripCameraSerial":true, "verify":true
    }))
    .expect("export request");
    aura_app::export_run(&state, job).expect("verified export");
    assert!(has_jpeg(&destination), "a final JPEG is written");
    assert_eq!(std::fs::read(path).expect("original"), original);
}

fn has_jpeg(root: &std::path::Path) -> bool {
    std::fs::read_dir(root)
        .expect("output folder")
        .any(|entry| {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                has_jpeg(&path)
            } else {
                path.extension()
                    .is_some_and(|ext| ext == "jpg" || ext == "jpeg")
            }
        })
}
