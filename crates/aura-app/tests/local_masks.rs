//! Masks in the Studio, end to end through the application layer. ADR-0102.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::print_stdout
)]
use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput, RenderImageInput};
use aura_app::local_mask_commands::{
    self, CreateMaskInput, LocalMasksInput, MaskCoverageInput, SaveMasksInput,
};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_raw::codec::{encode_jpeg, Rgb8};
use aura_recipe::local_masks::{Mode, Source};
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

fn luma(rgb: &[u8], w: usize, x: usize, y: usize) -> f32 {
    let i = (y * w + x) * 3;
    f32::from(rgb[i]) * 0.2126 + f32::from(rgb[i + 1]) * 0.7152 + f32::from(rgb[i + 2]) * 0.0722
}

#[test]
fn a_gradient_mask_brightens_only_its_side_and_can_be_combined_and_removed() {
    let dir = tempfile::tempdir().expect("temp");
    let path = dir.path().join("grey.jpg");
    let (w, h) = (120_usize, 80_usize);
    let rgb = Rgb8 {
        width: w as u32,
        height: h as u32,
        data: (0..w * h)
            .flat_map(|i| {
                let v = 100 + (i % 7) as u8;
                [v, v, v]
            })
            .collect(),
    };
    std::fs::write(&path, encode_jpeg(&rgb, 95).expect("jpeg")).expect("write");
    let f = import(&path, dir);
    let before = render(&f);
    let created = local_mask_commands::create(
        &f.state,
        &CreateMaskInput {
            project_id: f.project.clone(),
            photo_id: f.photo.clone(),
            what: "geometry".into(),
            source: Some(Source::Linear {
                start: [0.0, 0.5],
                end: [0.4, 0.5],
            }),
            into: None,
            mode: None,
            invert: false,
        },
    )
    .expect("create");
    let id = created.mask_id.clone().expect("a new mask");
    assert_eq!(created.masks.len(), 1);
    // A new mask changes nothing until a slider moves.
    assert_eq!(render(&f), before);
    let mut masks = created.masks.clone();
    masks[0].params.exposure = Some(1.0);
    local_mask_commands::save_all(
        &f.state,
        &SaveMasksInput {
            project_id: f.project.clone(),
            photo_id: f.photo.clone(),
            masks: masks.clone(),
            label: Some("Mask exposure".into()),
        },
    )
    .expect("save");
    let after = render(&f);
    assert!(luma(&after, w, 2, 40) > luma(&before, w, 2, 40) + 20.0);
    assert!((luma(&after, w, 110, 40) - luma(&before, w, 110, 40)).abs() < 2.0);
    // Subtract an ellipse on the left: that part goes back to how it was.
    local_mask_commands::create(
        &f.state,
        &CreateMaskInput {
            project_id: f.project.clone(),
            photo_id: f.photo.clone(),
            what: "geometry".into(),
            source: Some(Source::Radial {
                centre: [0.02, 0.5],
                radii: [0.06, 0.15],
                angle: 0.0,
                feather: 0.1,
            }),
            into: Some(id.clone()),
            mode: Some(Mode::Subtract),
            invert: false,
        },
    )
    .expect("subtract");
    let cut = render(&f);
    assert!((luma(&cut, w, 2, 40) - luma(&before, w, 2, 40)).abs() < 3.0);
    assert!(luma(&cut, w, 2, 5) > luma(&before, w, 2, 5) + 20.0);
    // The overlay shows the same shape.
    let coverage = local_mask_commands::coverage(
        &f.state,
        &MaskCoverageInput {
            project_id: f.project.clone(),
            photo_id: f.photo.clone(),
            mask_id: id,
        },
    )
    .expect("coverage");
    let grey = decode(&coverage.rgb_base64);
    let cw = coverage.width as usize;
    assert!(grey[(5 * cw + 1) * 3] > 200);
    assert!(grey[(coverage.height as usize / 2 * cw + cw - 2) * 3] < 10);
    // Removing the mask restores the photograph exactly.
    local_mask_commands::save_all(
        &f.state,
        &SaveMasksInput {
            project_id: f.project.clone(),
            photo_id: f.photo.clone(),
            masks: Vec::new(),
            label: None,
        },
    )
    .expect("clear");
    assert_eq!(render(&f), before);
    let listed = local_mask_commands::list(
        &f.state,
        &LocalMasksInput {
            project_id: f.project.clone(),
            photo_id: f.photo.clone(),
        },
    )
    .expect("list");
    assert!(listed.masks.is_empty());
}

#[test]
#[ignore = "needs AURA_MASK_PHOTOS: photographs separated by ';'; writes overlays to AURA_MASK_OUT"]
fn ai_selections_on_real_photographs() {
    let list = std::env::var("AURA_MASK_PHOTOS").expect("AURA_MASK_PHOTOS");
    let out = std::path::PathBuf::from(std::env::var("AURA_MASK_OUT").expect("AURA_MASK_OUT"));
    std::fs::create_dir_all(&out).expect("out");
    for path in list.split(';').filter(|p| !p.is_empty()) {
        let path = std::path::PathBuf::from(path);
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let f = import(&path, tempfile::tempdir().expect("temp"));
        for what in ["subject", "sky", "hair", "face_skin", "clothes", "lips"] {
            let started = std::time::Instant::now();
            let made = local_mask_commands::create(
                &f.state,
                &CreateMaskInput {
                    project_id: f.project.clone(),
                    photo_id: f.photo.clone(),
                    what: what.into(),
                    source: None,
                    into: None,
                    mode: None,
                    invert: false,
                },
            )
            .expect("create");
            let elapsed = started.elapsed();
            match made.mask_id {
                Some(id) => {
                    let c = local_mask_commands::coverage(
                        &f.state,
                        &MaskCoverageInput {
                            project_id: f.project.clone(),
                            photo_id: f.photo.clone(),
                            mask_id: id,
                        },
                    )
                    .expect("coverage");
                    let grey = decode(&c.rgb_base64);
                    let mut ppm = format!("P6\n{} {}\n255\n", c.width, c.height).into_bytes();
                    ppm.extend_from_slice(&grey);
                    std::fs::write(out.join(format!("{name}.{what}.ppm")), ppm).unwrap();
                    println!("{name} {what}: {elapsed:?}");
                }
                None => println!("{name} {what}: {:?}", made.message),
            }
        }
    }
}
