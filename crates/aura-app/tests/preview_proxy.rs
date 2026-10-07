//! Display proxies must never become the input to full-resolution delivery.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    clippy::too_many_lines
)]
use aura_app::contract::ipc::*;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_raw::codec::{decode_jpeg, encode_jpeg, Rgb8};

#[test]
fn preview_recipe_history_and_full_export_stay_independent() {
    let dir = tempfile::tempdir().unwrap();
    let state = aura_app::AppState::open(&dir.path().join("catalog.aura"))
        .unwrap()
        .with_cache_root(&dir.path().join("cache"));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Preview regression".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .unwrap();
    let rgb = Rgb8 {
        width: 1200,
        height: 900,
        data: (0..1200 * 900)
            .flat_map(|i| {
                [
                    35 + u8::try_from(i % 100).unwrap(),
                    45 + u8::try_from(i % 90).unwrap(),
                    55 + u8::try_from(i % 80).unwrap(),
                ]
            })
            .collect(),
    };
    let original = encode_jpeg(&rgb, 95).unwrap();
    let path = dir.path().join("original.jpg");
    std::fs::write(&path, &original).unwrap();
    aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).unwrap(),
            roots: vec![path.clone()],
            mode: ImportMode::Reference,
            extensions: vec![],
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .unwrap();
    let photo = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .unwrap()[0]
        .id
        .clone();
    let recipe_input = DevelopImageInput {
        photo_id: photo.clone(),
    };
    let initial = aura_app::image_recipe(&state, &recipe_input).unwrap();
    let mut request = RenderImageInput {
        photo_id: photo.clone(),
        level: Some("full".into()),
        screen: None,
        colour_space: None,
        purpose: Some("interactive".into()),
    };
    let preview = aura_app::render_image(&state, &request).unwrap();
    assert_eq!((preview.width, preview.height), (768, 576));
    let repeated = aura_app::render_image(&state.clone(), &request).unwrap();
    assert_eq!(preview.rgb_base64, repeated.rgb_base64);
    assert_eq!(
        repeated.ms, 0,
        "the second request must use the edited cache"
    );
    request.colour_space = Some("display_p3".into());
    let p3 = aura_app::render_image(&state, &request).unwrap();
    assert_eq!(
        p3.colour_space, "display_p3",
        "output spaces cannot share cached pixels"
    );
    request.colour_space = None;
    assert_eq!(
        initial,
        aura_app::image_recipe(&state, &recipe_input).unwrap()
    );
    request.level = Some("screen".into());
    request.screen = Some((300, 300));
    let small = aura_app::render_image(&state, &request).unwrap();
    assert_eq!((small.width, small.height), (300, 225));
    request.level = Some("full".into());
    request.screen = None;
    let edited = aura_app::set_param(
        &state,
        &SetParamInput {
            project_id: project.id.clone(),
            photo_id: photo.clone(),
            path: "global.exposure".into(),
            value: serde_json::json!(1.0),
            label: Some("Preview exposure".into()),
        },
    )
    .unwrap();
    let bright = aura_app::render_image(&state, &request).unwrap();
    assert_ne!(
        bright.rgb_base64, preview.rgb_base64,
        "changed edits must not return stale pixels"
    );
    assert_ne!(bright.render_hash, preview.render_hash);
    let history = |action: &str| {
        aura_app::history_step(
            &state,
            &HistoryStepInput {
                project_id: project.id.clone(),
                photo_id: photo.clone(),
                action: action.into(),
            },
        )
        .unwrap()
    };
    history("undo");
    let undone = aura_app::render_image(&state, &request).unwrap();
    assert_eq!(undone.rgb_base64, preview.rgb_base64);
    assert_eq!(undone.ms, 0);
    history("redo");
    assert_eq!(
        bright.rgb_base64,
        aura_app::render_image(&state, &request).unwrap().rgb_base64
    );
    assert_eq!(
        edited.recipe,
        aura_app::image_recipe(&state, &recipe_input).unwrap()
    );
    let retouch = aura_app::native_retouch::preview(&state, &project.id, &photo, false).unwrap();
    let selection = aura_app::native_retouch::saved_selection(
        &state,
        &aura_app::native_retouch::CoverageInput {
            project_id: project.id.clone(),
            photo_id: photo.clone(),
            operation_id: None,
        },
    )
    .unwrap();
    assert_eq!(
        (retouch.width, retouch.height),
        (selection.width, selection.height)
    );
    for purpose in ["analysis", "export"] {
        request.purpose = Some(purpose.into());
        let full = aura_app::render_image(&state, &request).unwrap();
        assert_eq!((full.width, full.height), (1200, 900));
    }
    // Actual delivery, after priming the preview cache, must still decode the original.
    let preset = aura_app::export_presets()
        .unwrap()
        .into_iter()
        .find(|p| p.format == "jpeg")
        .unwrap();
    let mut set = serde_json::to_value(preset).unwrap();
    set["imageIds"] = serde_json::json!([photo]);
    let destination = dir.path().join("export");
    let report = aura_app::export_run(
        &state,
        ExportJobInput {
            project_id: project.id,
            sets: vec![serde_json::from_value(set).unwrap()],
            destination: destination.to_string_lossy().into_owned(),
            destination_kind: "folder".into(),
            copyright: None,
            contact: None,
            creator: None,
            keywords: vec![],
            strip_gps: true,
            strip_camera_serial: true,
            verify: true,
        },
    )
    .unwrap();
    assert_eq!(report.verified, 1);
    assert_eq!(report.render_failed, 0);
    assert!(report.manifest_sealed);
    let output = decode_jpeg(
        &std::fs::read(exported(&destination)).unwrap(),
        aura_raw::DecodeLimits::tier3(),
    )
    .unwrap();
    assert_eq!((output.width, output.height), (1200, 900));
    assert_ne!(
        output.data,
        decode_jpeg(&original, aura_raw::DecodeLimits::tier3())
            .unwrap()
            .data
    );
    assert_eq!(std::fs::read(path).unwrap(), original);
    assert_eq!(
        edited.recipe,
        aura_app::image_recipe(&state, &recipe_input).unwrap()
    );
}

fn exported(root: &std::path::Path) -> std::path::PathBuf {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            return exported(&path);
        }
        if path
            .extension()
            .is_some_and(|ext| ext == "jpg" || ext == "jpeg")
        {
            return path;
        }
    }
    panic!("exported JPEG missing");
}
