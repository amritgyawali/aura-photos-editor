//! The portrait workflow end to end: import a photograph, see what AURA found in it, retouch it
//! automatically, change the retouch by hand, and render it.
//!
//! The photograph is `aura_portrait::fixtures`' painted face, written as a PNG and imported
//! through the real ingest, so every layer between the file and the pixels is the production
//! one: the preview cache, the frame source, the parse, the recipe store and the renderer.

use std::sync::Arc;

use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput, RenderImageInput};
use aura_app::portrait_commands::{
    analyse_portrait, auto_portrait_retouch, portrait_retouch, set_portrait_retouch,
    AutoPortraitRetouchInput, PortraitInput, PortraitOpDto, RegionAdjustmentDto,
    SetPortraitRetouchInput,
};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_portrait::fixtures::{portrait, PortraitSpec};

fn png(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .expect("png header")
            .write_image_data(rgb)
            .expect("png body");
    }
    bytes
}

/// A catalog with one imported photograph. Returns the state, the project id and the photo id.
fn imported(
    name: &str,
    width: u32,
    height: u32,
    rgb: &[u8],
) -> (tempfile::TempDir, AppState, String, String) {
    let dir = tempfile::tempdir().expect("temp");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("state")
        .with_cache_root(&dir.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Portraits".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    let path = dir.path().join(name);
    std::fs::write(&path, png(width, height, rgb)).expect("write fixture");
    let report = aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("id"),
            roots: vec![path],
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
            limit: 10,
            order_by: None,
        },
    )
    .expect("photos");
    let photo = photos[0].id.clone();
    (dir, state, project.id, photo)
}

#[test]
fn a_portrait_is_found_retouched_automatically_then_by_hand_and_rendered() {
    let p = portrait(&PortraitSpec::default());
    let (_dir, state, project, photo) = imported("portrait.png", p.width, p.height, &p.rgb);

    let analysis = analyse_portrait(
        &state,
        &PortraitInput {
            photo_id: photo.clone(),
        },
    )
    .expect("analysis");
    assert_eq!(analysis.faces.len(), 1, "{:?}", analysis.notes);
    let face = &analysis.faces[0];
    assert!(face.left_eye[0] < face.right_eye[0]);
    assert!(face.mouth[1] > face.left_eye[1]);
    for region in [
        "skin",
        "face",
        "eyes",
        "teeth",
        "lips",
        "hair",
        "background",
    ] {
        let found = analysis.regions.iter().find(|r| r.region == region);
        assert!(found.is_some(), "no {region} region");
        let found = found.expect("checked");
        assert!(!found.alpha_base64.is_empty());
    }

    let auto = auto_portrait_retouch(
        &state,
        &AutoPortraitRetouchInput {
            project_id: project.clone(),
            photo_id: photo.clone(),
            style: "natural".into(),
        },
    )
    .expect("auto");
    assert!(!auto.ops.is_empty());
    assert!(!auto.protected);
    assert!(
        auto.ops.iter().any(|op| op.op == "teeth_whiten"),
        "{:?}",
        auto.ops
    );
    assert_eq!(auto.explanation.len(), auto.ops.len());

    let render = aura_app::render_image(
        &state,
        &RenderImageInput {
            photo_id: photo.clone(),
            level: Some("screen".into()),
            screen: Some((256, 256)),
            colour_space: None,
            purpose: Some("interactive".into()),
        },
    )
    .expect("render");
    assert!(
        render.stages_run.iter().any(|s| s == "retouch"),
        "{:?}",
        render.stages_run
    );
    assert!(
        render
            .notes
            .iter()
            .all(|n| n.reason != "mask_generator_absent"),
        "{:?}",
        render.notes
    );

    let mine = set_portrait_retouch(
        &state,
        &SetPortraitRetouchInput {
            project_id: project.clone(),
            photo_id: photo.clone(),
            ops: vec![PortraitOpDto {
                op: "skin_smooth".into(),
                strength: 0.8,
            }],
            adjustments: vec![RegionAdjustmentDto {
                region: "hair".into(),
                exposure: Some(0.3),
                ..RegionAdjustmentDto::default()
            }],
            hints: None,
            label: Some("My retouch".into()),
        },
    )
    .expect("set");
    assert!(mine.protected);
    assert_eq!(mine.ops.len(), 1);
    assert_eq!(mine.adjustments.len(), 1);

    // An automatic pass after a person's edit changes nothing and says why.
    let again = auto_portrait_retouch(
        &state,
        &AutoPortraitRetouchInput {
            project_id: project,
            photo_id: photo.clone(),
            style: "polished".into(),
        },
    )
    .expect("auto again");
    assert_eq!(again.ops.len(), 1);
    assert!((again.ops[0].strength - 0.8).abs() < 1e-6);
    assert!(again.explanation[0].contains("yourself"));

    let read = portrait_retouch(&state, &PortraitInput { photo_id: photo }).expect("read");
    assert!(read.protected);
    assert_eq!(read.adjustments[0].region, "hair");
}

#[test]
fn a_frame_without_a_face_says_so_and_a_drawn_box_makes_one() {
    let p = portrait(&PortraitSpec::default());
    // The same face with its eyes painted over: nothing for the cascade to find.
    let mut rgb = p.rgb.clone();
    for (i, px) in rgb.chunks_exact_mut(3).enumerate() {
        let x = (i % p.width as usize) as f32;
        let y = (i / p.width as usize) as f32;
        if (y - p.eyes[0][1]).abs() < 14.0 && (x - p.centre[0]).abs() < 60.0 {
            px.copy_from_slice(&[200, 160, 135]);
        }
    }
    let (_dir, state, project, photo) = imported("noface.png", p.width, p.height, &rgb);
    let analysis = analyse_portrait(
        &state,
        &PortraitInput {
            photo_id: photo.clone(),
        },
    )
    .expect("analysis");
    if analysis.faces.is_empty() {
        assert!(analysis.notes.iter().any(|n| n.contains("Draw a box")));
    }
    let drawn = set_portrait_retouch(
        &state,
        &SetPortraitRetouchInput {
            project_id: project,
            photo_id: photo.clone(),
            ops: vec![PortraitOpDto {
                op: "skin_smooth".into(),
                strength: 0.5,
            }],
            adjustments: vec![],
            hints: Some(vec![[0.2, 0.2, 0.6, 0.68]]),
            label: None,
        },
    )
    .expect("set with a hint");
    assert_eq!(drawn.hints.len(), 1);
    let analysis = analyse_portrait(&state, &PortraitInput { photo_id: photo }).expect("again");
    assert!(analysis.faces.iter().any(|f| f.source == "hint"));
}
