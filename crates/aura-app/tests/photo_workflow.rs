//! An actual JPEG import -> auto-edit -> render -> export round trip.
#![allow(clippy::disallowed_methods)] // test code: `serde_json::json!` expands to an unwrap
use aura_app::contract::ipc::*;
use aura_app::{photo_auto_edit, AppState};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_raw::codec::{decode_jpeg, encode_jpeg, Rgb8};
use std::sync::Arc;

/// The rest of the round trip reads `project.id`, as it did when the project was made inline.
struct CreatedProject {
    id: String,
}

/// One JPEG imported into a fresh catalogue with no AI provider configured.
struct Imported {
    dir: tempfile::TempDir,
    state: AppState,
    project: String,
    photo: String,
    path: std::path::PathBuf,
    original: Vec<u8>,
}

fn imported(rgb: &Rgb8) -> Imported {
    let dir = tempfile::tempdir().expect("temp");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("state")
        .with_licence_dir(dir.path().join("licence"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Real photo workflow".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    let path = dir.path().join("original.jpg");
    let original = encode_jpeg(rgb, 95).expect("JPEG");
    std::fs::write(&path, &original).expect("write fixture");
    let report = aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("id"),
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
    assert_eq!(report.files_imported, 1);
    let photos = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 100,
            order_by: None,
        },
    )
    .expect("photos");
    Imported {
        photo: photos[0].id.clone(),
        project: project.id,
        state,
        path,
        original,
        dir,
    }
}

/// With no provider to ask, the automatic edit is AURA's own measured correction. The
/// stand-in it replaced aimed every median at middle grey and took a bright studio frame
/// down by three quarters of a stop.
#[test]
fn a_bright_frame_is_not_darkened_when_no_ai_provider_answers() {
    let (w, h) = (96_u32, 64_u32);
    let frame = imported(&Rgb8 {
        width: w,
        height: h,
        // A white backdrop with a mid-toned subject in the middle third.
        data: (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                let texture = ((x * 7 + y * 13) % 9) as u8;
                if (w / 3..2 * w / 3).contains(&x) && y > h / 4 {
                    [150 + texture, 120 + texture, 105 + texture]
                } else {
                    [232 + texture, 232 + texture, 230 + texture]
                }
            })
            .collect(),
    });
    let edited = photo_auto_edit(
        &frame.state,
        &PhotoAutoEditInput {
            project_id: frame.project.clone(),
            photo_id: frame.photo.clone(),
            job_id: "test-bright".into(),
        },
    )
    .expect("local auto edit");
    assert_eq!(edited.source, "local_fallback");
    assert!(
        edited.reasons[0].contains("AURA's own measured correction"),
        "{:?}",
        edited.reasons
    );
    let body: serde_json::Value = serde_json::from_str(&edited.recipe.body).expect("recipe");
    let exposure = body["global"]["exposure"].as_f64().expect("exposure");
    assert!(
        exposure > -0.2,
        "a bright backdrop is not overexposure: {exposure} ({:?})",
        edited.reasons
    );
}

#[test]
fn imported_pixels_are_edited_protected_and_exported_without_changing_originals() {
    let frame = imported(&Rgb8 {
        width: 96,
        height: 64,
        data: (0..96 * 64)
            .flat_map(|i| {
                [
                    35 + (i % 55) as u8,
                    40 + (i % 60) as u8,
                    45 + (i % 50) as u8,
                ]
            })
            .collect(),
    });
    let Imported {
        dir,
        state,
        project,
        photo,
        path,
        original,
    } = frame;
    let project = CreatedProject { id: project };
    let photo = &photo;
    let request = RenderImageInput {
        photo_id: photo.clone(),
        level: Some("full".into()),
        screen: None,
        colour_space: None,
        purpose: Some("export".into()),
    };
    let before = aura_app::render_image(&state, &request).expect("original render");
    assert_eq!((before.width, before.height), (96, 64));
    let edit_input = PhotoAutoEditInput {
        project_id: project.id.clone(),
        photo_id: photo.clone(),
        job_id: "test-auto".into(),
    };
    let edited = photo_auto_edit(&state, &edit_input).expect("local auto edit");
    assert_eq!(edited.source, "local_fallback");
    assert!(!edited.reasons.is_empty());
    let after = aura_app::render_image(&state, &request).expect("edited render");
    assert_ne!(
        before.rgb_base64, after.rgb_base64,
        "the actual pixels must change"
    );
    let repeated = photo_auto_edit(&state, &edit_input).expect("repeat");
    let first: serde_json::Value = serde_json::from_str(&edited.recipe.body).expect("recipe");
    let second: serde_json::Value = serde_json::from_str(&repeated.recipe.body).expect("recipe");
    assert_eq!(
        first["global"], second["global"],
        "automatic edits must not compound"
    );
    aura_app::set_param(
        &state,
        &SetParamInput {
            project_id: project.id.clone(),
            photo_id: photo.clone(),
            path: "global.exposure".into(),
            value: serde_json::json!(0.2),
            label: None,
        },
    )
    .expect("manual");
    let protected = photo_auto_edit(&state, &edit_input).expect("protected auto");
    let body: serde_json::Value = serde_json::from_str(&protected.recipe.body).expect("recipe");
    assert!((body["global"]["exposure"].as_f64().expect("exposure") - 0.2).abs() < 0.001);
    let presets = aura_app::export_presets().expect("presets");
    let preset = presets
        .iter()
        .find(|p| p.format == "jpeg")
        .expect("JPEG preset");
    let mut set = serde_json::to_value(preset).expect("preset json");
    set["imageIds"] = serde_json::json!([photo]);
    set["sidecar"] = serde_json::json!(true);
    let destination = dir.path().join("export");
    let job: ExportJobInput = serde_json::from_value(serde_json::json!({
        "projectId": project.id, "sets": [set], "destination": destination,
        "destinationKind": "folder", "copyright": null, "contact": null,
        "creator": null, "keywords": [], "stripGps": true, "stripCameraSerial": true, "verify": true
    }))
    .expect("job");
    let delivered = aura_app::export_run(&state, job).expect("first export");
    assert_eq!(
        delivered.sidecars, 1,
        "a requested recipe sidecar must be written"
    );
    let exported = walk_jpegs(&destination);
    assert!(!exported.is_empty(), "first export must create a JPEG");
    let sidecar: aura_recipe::Recipe = serde_json::from_str(
        &std::fs::read_to_string(exported[0].with_extension("xmp")).expect("recipe sidecar"),
    )
    .expect("canonical recipe JSON");
    let stored = state
        .recipe_store()
        .load(&PhotoId::from_db(photo).expect("photo id"))
        .expect("read saved edit")
        .expect("saved edit");
    assert_eq!(
        sidecar, stored,
        "sidecar must describe the rendered saved edit"
    );
    let decoded = decode_jpeg(
        &std::fs::read(&exported[0]).expect("export bytes"),
        aura_raw::DecodeLimits::tier3(),
    )
    .expect("valid exported JPEG");
    assert!(
        decoded.data.windows(3).any(|w| w != [118, 118, 118]),
        "not grey placeholder"
    );
    assert_eq!(std::fs::read(&path).expect("original remains"), original);
    let wrong_project = photo_auto_edit(
        &state,
        &PhotoAutoEditInput {
            project_id: ProjectId::new().to_db(),
            ..edit_input
        },
    );
    assert!(wrong_project.is_err());
}

fn walk_jpegs(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for entry in std::fs::read_dir(root).expect("export folder") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            out.extend(walk_jpegs(&path));
        } else if path.extension().is_some_and(|e| e == "jpg" || e == "jpeg") {
            out.push(path);
        }
    }
    out
}
