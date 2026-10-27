//! Match a look with one edited reference whose camera original is in the collection: the edit is
//! learned from that pair and every photograph receives the same operation (ADR-0161).
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::disallowed_methods,
    clippy::needless_range_loop
)]
use std::path::Path;
use std::sync::Arc;

use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::{reference_style, AppState};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};

fn scene(width: u32, height: u32, seed: u32) -> image::RgbImage {
    image::RgbImage::from_fn(width, height, |x, y| {
        let h = x.wrapping_mul(374_761_393 ^ seed) ^ y.wrapping_mul(668_265_263);
        let (fx, fy) = (x * 255 / width, y * 255 / height);
        let [a, b, c] = match seed % 3 {
            0 => [fx, fy, (h >> 16) % 200],
            1 => [(h >> 16) % 200, fx, fy],
            _ => [fy, (h >> 16) % 200, fx],
        };
        image::Rgb([
            20 + (a * 7 / 8) as u8,
            20 + (b * 7 / 8) as u8,
            20 + (c * 7 / 8) as u8,
        ])
    })
}

/// A warm, faded split tone: a photographer's look, not anything one scene's colours imply.
fn grade(bytes: &[u8]) -> Vec<u8> {
    bytes
        .chunks_exact(3)
        .flat_map(|p| {
            let [r, g, b] = [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.0);
            let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let shadow = (1.0 - y).powi(2);
            [
                0.05 + 0.9 * r + 0.04 * y * y - 0.02 * shadow,
                0.04 + 0.9 * g + 0.01 * shadow,
                0.04 + 0.86 * b + 0.05 * shadow - 0.03 * y * y,
            ]
            .map(|v| (255.0 * v.clamp(0.0, 1.0)).round() as u8)
        })
        .collect()
}

fn base64_decode(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    };
    let bytes: Vec<u8> = text.bytes().filter(|c| *c != b'=').map(value).collect();
    bytes
        .chunks(4)
        .flat_map(|chunk| {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |acc, (i, v)| acc | (u32::from(*v) << (18 - 6 * i)));
            let take = chunk.len().saturating_sub(1);
            [(n >> 16) as u8, (n >> 8) as u8, n as u8]
                .into_iter()
                .take(take)
        })
        .collect()
}

fn render(state: &AppState, project: ProjectId, photo: PhotoId) -> Vec<u8> {
    base64_decode(
        &aura_app::native_retouch::preview(state, &project.to_db(), &photo.to_db(), false)
            .expect("native render")
            .rgb_base64,
    )
}

fn error(actual: &[u8], reference: &[u8]) -> f32 {
    assert_eq!(actual.len(), reference.len());
    actual
        .iter()
        .zip(reference)
        .map(|(a, b)| f32::from(a.abs_diff(*b)))
        .sum::<f32>()
        / actual.len() as f32
}

struct Shoot {
    state: AppState,
    project: ProjectId,
    photos: Vec<(String, PhotoId)>,
}

fn import(shoot: &Path, directory: &Path) -> Shoot {
    let state = AppState::open(&directory.join("catalog.aura"))
        .expect("isolated catalogue")
        .with_cache_root(&directory.join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Original discovery".into(),
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
            roots: vec![shoot.to_path_buf()],
            mode: ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import shoot");
    let mut photos: Vec<(String, PhotoId)> = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id,
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("list shoot")
    .into_iter()
    .map(|p| (p.file_name, PhotoId::from_db(&p.id).expect("photo id")))
    .collect();
    photos.sort();
    for (_, photo) in &photos {
        let baseline = aura_app::develop_commands::neutral_recipe(&state, *photo);
        state
            .recipe_store()
            .save(&project_id, photo, &baseline, &[], "Camera baseline")
            .expect("initial edit");
    }
    Shoot {
        state,
        project: project_id,
        photos,
    }
}

fn analyse(state: &AppState, edited: &Path) -> reference_style::ReferenceAnalysis {
    reference_style::analyse_reference(
        state,
        &reference_style::AnalyseReferenceInput {
            // A folder takes a handle or nothing; a single photograph takes any label.
            address: if edited.is_dir() {
                String::new()
            } else {
                "Studio look".into()
            },
            folder: edited.to_string_lossy().into_owned(),
            cancel_id: uuid::Uuid::new_v4().to_string(),
            export: false,
            original: None,
        },
    )
    .unwrap()
}

fn apply(
    state: &AppState,
    photo: PhotoId,
    reference: &str,
    strength: f32,
) -> reference_style::ApplyReferenceReport {
    reference_style::apply_reference(
        state,
        &reference_style::ApplyReferenceInput {
            photo_id: photo.to_db(),
            reference_id: reference.into(),
            strength,
            profile_id: None,
            profile_strength: None,
            cancel_id: None,
        },
    )
    .unwrap()
}

/// The baseline every target is matched from, rendered: strength zero of an unrelated reference.
fn baselines(shoot: &Shoot, unrelated: &Path) -> Vec<Vec<u8>> {
    let plain = analyse(&shoot.state, unrelated);
    shoot
        .photos
        .iter()
        .map(|(_, photo)| {
            let report = apply(&shoot.state, *photo, &plain.id, 0.0);
            assert!(report.learned_from.is_none());
            render(&shoot.state, shoot.project, *photo)
        })
        .collect()
}

#[test]
fn an_edited_frame_from_the_collection_is_learned_from_its_original_and_applied_to_all() {
    let directory = tempfile::tempdir().unwrap();
    let shoot = directory.path().join("shoot");
    let exports = directory.path().join("exports");
    std::fs::create_dir_all(&shoot).unwrap();
    std::fs::create_dir_all(&exports).unwrap();
    scene(128, 96, 1).save(shoot.join("IMG_0001.png")).unwrap();
    scene(128, 96, 2).save(shoot.join("IMG_0002.png")).unwrap();
    scene(96, 128, 3).save(shoot.join("IMG_0003.png")).unwrap();
    let unrelated = exports.join("unrelated.png");
    scene(120, 90, 4).save(&unrelated).unwrap();
    let imported = import(&shoot, &directory.path().join("catalog"));
    let before = baselines(&imported, &unrelated);

    // The photographer edited IMG_0002 and exported it under an editor's suffix.
    let edited = exports.join("IMG_0002-Edit.png");
    image::RgbImage::from_raw(128, 96, grade(&before[1]))
        .unwrap()
        .save(&edited)
        .unwrap();
    let edited_bytes = std::fs::read(&edited).unwrap();
    let look = analyse(&imported.state, &edited);
    assert!(
        look.calibration.is_none(),
        "nothing is learned before a collection is chosen"
    );

    // The first photograph matched is not the original; the original is found before it is fitted.
    for (index, (name, photo)) in imported.photos.iter().enumerate() {
        let report = apply(&imported.state, *photo, &look.id, 1.0);
        assert_eq!(
            report.learned_from.as_deref(),
            Some("IMG_0002.png"),
            "{name}"
        );
        assert!(report.changed > 0, "{name}");
        let after = render(&imported.state, imported.project, *photo);
        let truth = grade(&before[index]);
        let (was, now) = (error(&before[index], &truth), error(&after, &truth));
        println!("{name}: RGB error against the photographer's look {was:.3} -> {now:.3}");
        assert!(now < 1.0 && now < 0.15 * was, "{name}: {was} -> {now}");
    }
    // Zero still restores the baseline, and a repeat does not compound the learned look.
    let (_, first) = imported.photos[0];
    apply(&imported.state, first, &look.id, 0.0);
    assert_eq!(render(&imported.state, imported.project, first), before[0]);
    apply(&imported.state, first, &look.id, 1.0);
    let once = render(&imported.state, imported.project, first);
    assert_eq!(apply(&imported.state, first, &look.id, 1.0).changed, 0);
    assert_eq!(render(&imported.state, imported.project, first), once);
    assert_eq!(std::fs::read(&edited).unwrap(), edited_bytes);
}

#[test]
fn a_reframed_export_is_not_taken_for_its_original() {
    let directory = tempfile::tempdir().unwrap();
    let shoot = directory.path().join("shoot");
    let exports = directory.path().join("exports");
    std::fs::create_dir_all(&shoot).unwrap();
    std::fs::create_dir_all(&exports).unwrap();
    scene(128, 96, 1).save(shoot.join("IMG_0001.png")).unwrap();
    scene(128, 96, 2).save(shoot.join("IMG_0002.png")).unwrap();
    let unrelated = exports.join("unrelated.png");
    scene(120, 90, 4).save(&unrelated).unwrap();
    let imported = import(&shoot, &directory.path().join("catalog"));
    let before = baselines(&imported, &unrelated);

    // Same name, but cropped to a square: its pixels no longer correspond to the camera file.
    let graded = image::RgbImage::from_raw(128, 96, grade(&before[1])).unwrap();
    let edited = exports.join("IMG_0002-Edit.png");
    image::imageops::crop_imm(&graded, 16, 0, 96, 96)
        .to_image()
        .save(&edited)
        .unwrap();
    let look = analyse(&imported.state, &edited);
    for (name, photo) in &imported.photos {
        let report = apply(&imported.state, *photo, &look.id, 1.0);
        assert!(report.learned_from.is_none(), "{name}");
    }

    // A same-named photograph of a different scene is refused by its pixels, not trusted by name.
    let impostor = exports.join("IMG_0001.png");
    image::RgbImage::from_raw(128, 96, grade(&before[1]))
        .unwrap()
        .save(&impostor)
        .unwrap();
    let look = analyse(&imported.state, &impostor);
    let (_, first) = imported.photos[0];
    assert!(apply(&imported.state, first, &look.id, 1.0)
        .learned_from
        .is_none());
}

/// A photographer's preset in Lightroom's own names: tone, colour grade and per-colour moves,
/// no white balance or exposure, which are each photograph's own correction.
const PRESET: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description
 crs:Contrast2012="+22" crs:Highlights2012="-35" crs:Shadows2012="+25" crs:Blacks2012="+12"
 crs:Vibrance="+18" crs:Saturation="-12"
 crs:SplitToningShadowHue="205" crs:SplitToningShadowSaturation="22"
 crs:SplitToningHighlightHue="42" crs:SplitToningHighlightSaturation="28"
 crs:HueAdjustmentGreen="-25" crs:SaturationAdjustmentGreen="-30"
 crs:HueAdjustmentOrange="+6" crs:LuminanceAdjustmentOrange="+8"
 crs:SaturationAdjustmentBlue="-20"/></rdf:RDF></x:xmpmeta>"#;

fn rendered(state: &AppState, project: ProjectId, photo: PhotoId) -> (u32, u32, Vec<u8>) {
    let dto = aura_app::native_retouch::preview(state, &project.to_db(), &photo.to_db(), false)
        .expect("native render");
    (dto.width, dto.height, base64_decode(&dto.rgb_base64))
}

#[test]
#[ignore = "needs AURA_ORIGINAL_SHOOT, a folder of camera files"]
fn camera_files_take_a_look_learned_from_one_edited_frame() {
    let Ok(folder) = std::env::var("AURA_ORIGINAL_SHOOT") else {
        return;
    };
    let count: usize = std::env::var("AURA_ORIGINAL_COUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let mut files: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| {
                [
                    "cr3", "cr2", "nef", "arw", "dng", "raf", "orf", "rw2", "jpg", "png",
                ]
                .contains(&e.to_string_lossy().to_lowercase().as_str())
            })
        })
        .collect();
    files.sort();
    files.truncate(count);
    let directory = tempfile::tempdir().unwrap();
    let shoot = directory.path().join("shoot");
    let exports = directory.path().join("exports");
    std::fs::create_dir_all(&shoot).unwrap();
    std::fs::create_dir_all(&exports).unwrap();
    for file in &files {
        std::fs::copy(file, shoot.join(file.file_name().unwrap())).unwrap();
    }
    let unrelated = exports.join("unrelated.png");
    scene(120, 90, 4).save(&unrelated).unwrap();
    let imported = import(&shoot, &directory.path().join("catalog"));
    let (state, project) = (&imported.state, imported.project);
    let before = baselines(&imported, &unrelated);

    // The photographer's look on every frame, rendered by AURA's own renderer.
    let truths: Vec<(u32, u32, Vec<u8>)> = imported
        .photos
        .iter()
        .map(|(_, photo)| {
            let base = state.recipe_store().load(photo).unwrap().unwrap();
            let looked = aura_recipe::xmp::read(PRESET, &base).unwrap();
            state
                .recipe_store()
                .save(
                    &project,
                    photo,
                    &looked,
                    &["global.contrast".into()],
                    "Preset",
                )
                .unwrap();
            let truth = rendered(state, project, *photo);
            state
                .recipe_store()
                .save(&project, photo, &base, &[], "Baseline")
                .unwrap();
            truth
        })
        .collect();
    let edited_index = 1.min(imported.photos.len() - 1);
    let (name, _) = &imported.photos[edited_index];
    let (w, h, pixels) = truths[edited_index].clone();
    let stem = Path::new(name).file_stem().unwrap().to_string_lossy();
    let edited = exports.join(format!("{stem}-Edit.png"));
    image::RgbImage::from_raw(w, h, pixels.clone())
        .unwrap()
        .save(&edited)
        .unwrap();
    // The same file under a name nothing in the collection has: the reference alone.
    let alone = exports.join("finished-look.png");
    image::RgbImage::from_raw(w, h, pixels)
        .unwrap()
        .save(&alone)
        .unwrap();

    // With AURA_ORIGINAL_EDITS above one, a reference folder holds that many edited frames of
    // the collection, filled to the folder minimum with the look on frames outside it.
    let edits: usize = std::env::var("AURA_ORIGINAL_EDITS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let folder = exports.join("edits");
    let edited = if edits > 1 {
        std::fs::create_dir_all(&folder).unwrap();
        for index in 1..=edits.min(imported.photos.len() - 1) {
            let (w, h, pixels) = truths[index].clone();
            let stem = Path::new(&imported.photos[index].0)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            image::RgbImage::from_raw(w, h, pixels)
                .unwrap()
                .save(folder.join(format!("{stem}-Edit.png")))
                .unwrap();
        }
        for (index, (w, h, pixels)) in truths.iter().enumerate().skip(edits + 1).take(8) {
            image::RgbImage::from_raw(*w, *h, pixels.clone())
                .unwrap()
                .save(folder.join(format!("elsewhere-{index}.png")))
                .unwrap();
        }
        folder.clone()
    } else {
        edited
    };

    let mut totals = [0.0_f32; 3];
    // The reference-alone route runs the full native parameter search per camera file; skip it
    // to check only the learned route quickly.
    let routes: Vec<(&str, &Path)> = if std::env::var_os("AURA_ORIGINAL_ONLY_LEARNED").is_some() {
        vec![("learned", edited.as_path())]
    } else {
        vec![
            ("reference alone", alone.as_path()),
            ("learned", edited.as_path()),
        ]
    };
    for (label, reference) in routes {
        let look = analyse(state, reference);
        let mut sum = 0.0;
        for (index, (photo_name, photo)) in imported.photos.iter().enumerate() {
            let report = apply(state, *photo, &look.id, 1.0);
            let (_, _, after) = rendered(state, project, *photo);
            let amount = aura_recipe::calibrated_look::read(
                &state.recipe_store().load(photo).unwrap().unwrap(),
            )
            .unwrap()
            .map(|m| m.amount);
            println!("{label:>15} {photo_name}: calibrated amount {amount:?}");
            let truth = &truths[index].2;
            let (was, now) = (error(&before[index], truth), error(&after, truth));
            println!(
                "{label:>15} {photo_name}: RGB error {was:.2} -> {now:.2} (learned from {:?}; {:?})",
                report.learned_from, report.original_refused
            );
            sum += now;
            if label == "learned" {
                assert!(report.learned_from.is_some(), "{photo_name}");
                totals[0] += was;
            }
        }
        totals[if label == "learned" { 2 } else { 1 }] = sum;
    }
    let n = imported.photos.len() as f32;
    println!(
        "mean RGB error against the look: untouched {:.2}, reference alone {:.2}, learned from its original {:.2}",
        totals[0] / n,
        totals[1] / n,
        totals[2] / n
    );
    // A preset with local tone (Highlights, Shadows) is only partly a global operation, so this
    // measures rather than demands closeness; it must still move every frame toward the look.
    assert!(totals[2] < totals[0], "{totals:?}");
}

#[test]
fn a_reference_folder_learns_one_look_from_every_original_in_the_collection() {
    let directory = tempfile::tempdir().unwrap();
    let shoot = directory.path().join("shoot");
    let exports = directory.path().join("exports");
    std::fs::create_dir_all(&shoot).unwrap();
    std::fs::create_dir_all(&exports).unwrap();
    for (name, seed) in [
        ("IMG_0001.png", 1),
        ("IMG_0002.png", 2),
        ("IMG_0003.png", 3),
        ("IMG_0004.png", 5),
    ] {
        scene(128, 96, seed).save(shoot.join(name)).unwrap();
    }
    let unrelated = directory.path().join("unrelated.png");
    scene(120, 90, 4).save(&unrelated).unwrap();
    let imported = import(&shoot, &directory.path().join("catalog"));
    let before = baselines(&imported, &unrelated);

    // Two edited frames from this collection, and six of the same look from other weddings.
    for index in [0, 1] {
        let stem = Path::new(&imported.photos[index].0)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        image::RgbImage::from_raw(128, 96, grade(&before[index]))
            .unwrap()
            .save(exports.join(format!("{stem}-Edit.png")))
            .unwrap();
    }
    for seed in 10..16 {
        let (width, height) = (100 + seed, 75 + seed);
        let other = scene(width, height, seed);
        image::RgbImage::from_raw(width, height, grade(other.as_raw()))
            .unwrap()
            .save(exports.join(format!("elsewhere-{seed}.png")))
            .unwrap();
    }
    let look = analyse(&imported.state, &exports);
    for (index, (name, photo)) in imported.photos.iter().enumerate() {
        let report = apply(&imported.state, *photo, &look.id, 1.0);
        assert_eq!(
            report.learned_from.as_deref(),
            Some("IMG_0001.png and IMG_0002.png"),
            "{name}"
        );
        let after = render(&imported.state, imported.project, *photo);
        let truth = grade(&before[index]);
        let (was, now) = (error(&before[index], &truth), error(&after, &truth));
        println!("{name}: RGB error against the folder's look {was:.3} -> {now:.3}");
        assert!(now < 1.0 && now < 0.15 * was, "{name}: {was} -> {now}");
    }
}
