//! The Studio's finishing tools, end to end through the application layer. ADR-0108, and
//! Auto light (ADR-0127).
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::print_stdout
)]
use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput, RenderImageInput};
use aura_app::finish_commands::{self, SaveStudioFinishInput, StudioFinishInput};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_portrait::fixtures::{portrait, PortraitSpec};
use aura_raw::codec::{encode_jpeg, Rgb8};
use aura_recipe::studio_finish::{LiquifyMode, LiquifyStroke, StudioFinish};
use std::sync::Arc;

struct Fixture {
    _dir: tempfile::TempDir,
    state: AppState,
    project: String,
    photo: String,
}

fn import(path: &std::path::Path, dir: tempfile::TempDir) -> Fixture {
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("state")
        .with_cache_root(&dir.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Masks".into(),
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
            roots: vec![path.to_path_buf()],
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
            project_id: project.id.clone(),
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("photos")[0]
        .id
        .clone();
    Fixture {
        _dir: dir,
        state,
        project: project.id,
        photo,
    }
}

fn render(f: &Fixture) -> Vec<u8> {
    let out = aura_app::render_image(
        &f.state,
        &RenderImageInput {
            photo_id: f.photo.clone(),
            level: Some("full".into()),
            screen: None,
            colour_space: None,
            purpose: Some("export".into()),
        },
    )
    .expect("render");
    decode(&out.rgb_base64)
}

fn decode(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => u32::from(c - b'A'),
        b'a'..=b'z' => u32::from(c - b'a') + 26,
        b'0'..=b'9' => u32::from(c - b'0') + 52,
        b'+' => 62,
        _ => 63,
    };
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for chunk in text.as_bytes().chunks(4) {
        let pad = chunk.iter().filter(|c| **c == b'=').count();
        let n = chunk.iter().fold(0_u32, |n, c| {
            (n << 6) | if *c == b'=' { 0 } else { value(*c) }
        });
        out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8][..3 - pad]);
    }
    out
}

#[test]
fn a_liquify_is_saved_as_one_undoable_edit_and_changes_the_render() {
    let dir = tempfile::tempdir().expect("temp");
    let path = dir.path().join("stripes.jpg");
    let (w, h) = (120_usize, 80_usize);
    let rgb = Rgb8 {
        width: w as u32,
        height: h as u32,
        data: (0..w * h)
            .flat_map(|i| {
                let v = if (i % w) / 6 % 2 == 0 { 60 } else { 190 };
                [v, v, v]
            })
            .collect(),
    };
    std::fs::write(&path, encode_jpeg(&rgb, 95).expect("jpeg")).expect("write");
    let f = import(&path, dir);
    let ids = StudioFinishInput {
        project_id: f.project.clone(),
        photo_id: f.photo.clone(),
    };
    assert_eq!(
        finish_commands::studio_finish(&f.state, &ids)
            .expect("read")
            .finish,
        StudioFinish::default()
    );
    let before = render(&f);
    let finish = StudioFinish {
        liquify: vec![LiquifyStroke {
            mode: LiquifyMode::Push,
            radius: 0.3,
            strength: 1.0,
            points: vec![[0.45, 0.5], [0.55, 0.5]],
        }],
        ..StudioFinish::default()
    };
    let saved = finish_commands::save_studio_finish(
        &f.state,
        &SaveStudioFinishInput {
            project_id: f.project.clone(),
            photo_id: f.photo.clone(),
            finish: finish.clone(),
            label: Some("Liquify".into()),
        },
    )
    .expect("save");
    assert_eq!(saved.finish, finish);
    assert_eq!(
        finish_commands::studio_finish(&f.state, &ids)
            .expect("read")
            .finish,
        finish
    );
    assert_ne!(render(&f), before, "the liquify did not reach the render");

    // An out-of-range slider is refused and nothing is written.
    let mut wrong = StudioFinish::default();
    wrong.face.slim = 400.0;
    assert!(finish_commands::save_studio_finish(
        &f.state,
        &SaveStudioFinishInput {
            project_id: f.project.clone(),
            photo_id: f.photo.clone(),
            finish: wrong,
            label: None
        },
    )
    .is_err());
    assert_eq!(
        finish_commands::studio_finish(&f.state, &ids)
            .expect("read")
            .finish,
        finish
    );
}

#[test]
fn auto_light_measures_the_face_proposes_a_light_and_stores_nothing() {
    let dir = tempfile::tempdir().expect("temp");
    let path = dir.path().join("portrait.jpg");
    let p = portrait(&PortraitSpec::default());
    let rgb = Rgb8 {
        width: p.width,
        height: p.height,
        data: p.rgb.clone(),
    };
    std::fs::write(&path, encode_jpeg(&rgb, 95).expect("jpeg")).expect("write");
    let f = import(&path, dir);
    let ids = StudioFinishInput {
        project_id: f.project.clone(),
        photo_id: f.photo.clone(),
    };
    let suggestion = finish_commands::suggest_portrait_light(&f.state, &ids).expect("suggest");
    assert_eq!(suggestion.faces, 1, "{suggestion:?}");
    assert!(!suggestion.reasons.is_empty());
    // The painted face is evenly lit, so it is given a soft key light and no fill.
    assert!(suggestion.light.key > 0.0, "{suggestion:?}");
    assert!(suggestion.light.fill <= 0.0, "{suggestion:?}");
    assert!(
        suggestion.light.glow <= 0.0,
        "glow is a look and is never proposed"
    );
    // Asking stores nothing.
    assert_eq!(
        finish_commands::studio_finish(&f.state, &ids)
            .expect("read")
            .finish,
        StudioFinish::default()
    );
    // Saving the proposal is one edit, and it reaches the render.
    let before = render(&f);
    let finish = StudioFinish {
        light: suggestion.light,
        ..StudioFinish::default()
    };
    finish_commands::save_studio_finish(
        &f.state,
        &SaveStudioFinishInput {
            project_id: f.project.clone(),
            photo_id: f.photo.clone(),
            finish: finish.clone(),
            label: Some("Auto light".into()),
        },
    )
    .expect("save");
    assert_eq!(
        finish_commands::studio_finish(&f.state, &ids)
            .expect("read")
            .finish,
        finish
    );
    assert_ne!(render(&f), before, "the light did not reach the render");
}

#[test]
fn background_repair_is_saved_rendered_and_undoable_without_changing_the_original() {
    let dir = tempfile::tempdir().expect("temp");
    let path = dir.path().join("portrait.jpg");
    let p = portrait(&PortraitSpec::default());
    let bytes = encode_jpeg(
        &Rgb8 {
            width: p.width,
            height: p.height,
            data: p.rgb,
        },
        95,
    )
    .expect("jpeg");
    std::fs::write(&path, &bytes).expect("write");
    let f = import(&path, dir);
    let ids = StudioFinishInput {
        project_id: f.project.clone(),
        photo_id: f.photo.clone(),
    };
    let mut finish = StudioFinish {
        liquify: vec![LiquifyStroke {
            mode: LiquifyMode::Push,
            radius: 0.4,
            strength: 1.0,
            points: vec![[0.2, 0.45], [0.3, 0.45]],
        }],
        ..StudioFinish::default()
    };
    finish.face.slim = 20.0;
    let save = |finish: &StudioFinish| {
        finish_commands::save_studio_finish(
            &f.state,
            &SaveStudioFinishInput {
                project_id: f.project.clone(),
                photo_id: f.photo.clone(),
                finish: finish.clone(),
                label: Some("Background repair".into()),
            },
        )
        .expect("save")
    };
    save(&finish);
    let warped = render(&f);
    finish.background_repair = 100.0;
    save(&finish);
    assert_eq!(
        finish_commands::studio_finish(&f.state, &ids)
            .unwrap()
            .finish,
        finish
    );
    let repaired = render(&f);
    assert_ne!(
        repaired, warped,
        "repair must reach the native export renderer"
    );
    // Preserve main's reset behaviour while keeping the new zero field omitted in old recipes.
    finish.background_repair = 0.0;
    save(&finish);
    assert_eq!(render(&f), warped, "zero must disable the saved repair");
    aura_app::history_step(
        &f.state,
        &aura_app::contract::ipc::HistoryStepInput {
            project_id: f.project.clone(),
            photo_id: f.photo.clone(),
            action: "undo".into(),
        },
    )
    .expect("undo zero repair");
    assert_eq!(render(&f), repaired);
    aura_app::history_step(
        &f.state,
        &aura_app::contract::ipc::HistoryStepInput {
            project_id: f.project.clone(),
            photo_id: f.photo.clone(),
            action: "undo".into(),
        },
    )
    .expect("undo");
    assert_eq!(render(&f), warped);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn background_repair_refuses_a_frame_without_a_detected_subject() {
    let dir = tempfile::tempdir().expect("temp");
    let path = dir.path().join("empty.jpg");
    std::fs::write(
        &path,
        encode_jpeg(
            &Rgb8 {
                width: 64,
                height: 64,
                data: vec![110; 64 * 64 * 3],
            },
            95,
        )
        .unwrap(),
    )
    .unwrap();
    let f = import(&path, dir);
    let mut finish = StudioFinish {
        background_repair: 100.0,
        ..StudioFinish::default()
    };
    finish.face.slim = 20.0;
    assert!(finish_commands::save_studio_finish(
        &f.state,
        &SaveStudioFinishInput {
            project_id: f.project.clone(),
            photo_id: f.photo.clone(),
            finish,
            label: None,
        }
    )
    .is_err());
    assert!(finish_commands::studio_finish(
        &f.state,
        &StudioFinishInput {
            project_id: f.project,
            photo_id: f.photo,
        }
    )
    .unwrap()
    .finish
    .is_identity());
}

#[test]
fn background_repair_uses_the_saved_subject_matte_and_rechecks_a_changed_selection() {
    let dir = tempfile::tempdir().expect("temp");
    let path = dir.path().join("manual-subject.jpg");
    std::fs::write(
        &path,
        encode_jpeg(
            &Rgb8 {
                width: 64,
                height: 64,
                data: vec![110; 64 * 64 * 3],
            },
            95,
        )
        .unwrap(),
    )
    .unwrap();
    let f = import(&path, dir);
    let mut alpha = vec![0; 64 * 64];
    for y in 16..48 {
        for x in 16..48 {
            alpha[y * 64 + x] = 255;
        }
    }
    let mut finish = StudioFinish {
        background_repair: 100.0,
        subject_matte: Some(aura_recipe::retouch_tools::Matte::encode(
            [0.0, 0.0, 1.0, 1.0],
            64,
            64,
            &alpha,
        )),
        ..StudioFinish::default()
    };
    let save = |finish: &StudioFinish| {
        finish_commands::save_studio_finish(
            &f.state,
            &SaveStudioFinishInput {
                project_id: f.project.clone(),
                photo_id: f.photo.clone(),
                finish: finish.clone(),
                label: None,
            },
        )
    };
    let saved = save(&finish).expect("an explicit subject selection must agree with the renderer");
    finish.subject_matte = Some(aura_recipe::retouch_tools::Matte::encode(
        [0.0, 0.0, 1.0, 1.0],
        64,
        64,
        &vec![255; 64 * 64],
    ));
    assert!(
        save(&finish).is_err(),
        "changing the selection must recheck foreground/background support"
    );
    let unchanged = finish_commands::studio_finish(
        &f.state,
        &StudioFinishInput {
            project_id: f.project,
            photo_id: f.photo,
        },
    )
    .unwrap();
    assert_eq!(unchanged.recipe.recipe_hash, saved.recipe.recipe_hash);
}
