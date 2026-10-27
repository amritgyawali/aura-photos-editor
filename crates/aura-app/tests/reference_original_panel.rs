//! The Match a look panel learns a reference's look from its own original in the collection
//! (ADR-0161, ADR-0180), through the native commands, and never returns a silent no-op.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::disallowed_methods
)]
use aura_app::color_match;
use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use std::path::Path;
use std::sync::Arc;

/// A varied photograph-like scene: two gradients and a hashed channel, so its colours cover
/// enough of the cube for a pixel calibration, laid out differently per seed.
fn scene(seed: u32) -> image::RgbImage {
    image::RgbImage::from_fn(144, 96, |x, y| {
        let (u, v) = if seed.is_multiple_of(2) {
            (x, y)
        } else {
            (143 - x, 95 - y)
        };
        let h =
            (x + seed * 31).wrapping_mul(374_761_393) ^ (y + seed * 17).wrapping_mul(668_265_263);
        image::Rgb([
            25 + (u * 190 / 144) as u8,
            30 + (v * 185 / 96) as u8,
            35 + ((h >> 16) % 170) as u8,
        ])
    })
}

/// A declared warm matte look on encoded sRGB: lifted blacks, softened colour, warm balance.
fn warm_matte(image: &image::RgbImage) -> image::RgbImage {
    image::RgbImage::from_fn(image.width(), image.height(), |x, y| {
        let p = image.get_pixel(x, y).0.map(|v| f32::from(v) / 255.0);
        let luma = 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
        let gain = [1.07, 1.015, 0.88];
        image::Rgb(std::array::from_fn(|c| {
            let soft = (luma + 0.82 * (p[c] - luma)).max(0.0).powf(0.92) * gain[c];
            ((0.035 + 0.935 * soft).clamp(0.0, 1.0) * 255.0).round() as u8
        }))
    })
}

fn open(shoot: &Path, directory: &Path) -> (AppState, ProjectId, Vec<(String, PhotoId)>) {
    let state = AppState::open(&directory.join("catalog.aura"))
        .expect("isolated catalogue")
        .with_cache_root(&directory.join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Reference original".into(),
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
    let photos = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id,
            offset: 0,
            limit: 20,
            order_by: Some("filename".into()),
        },
    )
    .expect("list shoot")
    .into_iter()
    .map(|row| (row.file_name, PhotoId::from_db(&row.id).expect("photo id")))
    .collect();
    (state, project_id, photos)
}

fn job() -> String {
    uuid::Uuid::new_v4().to_string()
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

/// Mean absolute RGB byte difference.
fn difference(a: &[u8], b: &[u8]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(x, y)| f32::from(x.abs_diff(*y)))
        .sum::<f32>()
        / a.len() as f32
}

fn photo(photos: &[(String, PhotoId)], name: &str) -> PhotoId {
    photos
        .iter()
        .find(|(file, _)| file == name)
        .unwrap_or_else(|| panic!("{name} in {photos:?}"))
        .1
}

fn prepare(state: &AppState, photo: PhotoId, reference: &str) -> color_match::MatchReport {
    color_match::prepare(
        state,
        &color_match::PrepareInput {
            replace_previous: false,
            photo_id: photo.to_db(),
            reference_id: reference.into(),
            cancel_id: job(),
            quality: color_match::MatchQuality::Quick,
            intent: color_match::MatchIntent::Natural,
        },
    )
    .expect("analysis")
}

fn apply(state: &AppState, photo: PhotoId, session: &str, amount: f32) -> color_match::MatchReport {
    color_match::apply(
        state,
        &color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: session.into(),
            controls: color_match::Controls {
                amount,
                ..color_match::Controls::default()
            },
            cancel_id: job(),
        },
    )
    .expect("apply")
}

#[test]
fn the_panel_learns_the_look_from_the_references_own_original() {
    let directory = tempfile::tempdir().unwrap();
    let shoot = directory.path().join("shoot");
    let exports = directory.path().join("exports");
    std::fs::create_dir_all(&shoot).unwrap();
    std::fs::create_dir_all(&exports).unwrap();
    for seed in 1..=3 {
        scene(seed)
            .save(shoot.join(format!("DSC000{seed}.png")))
            .unwrap();
    }
    // The photographer's own edit of the first frame, exported under an editor's name.
    let edited = exports.join("DSC0001-Edit.png");
    warm_matte(&scene(1)).save(&edited).unwrap();
    let (state, project, photos) = open(&shoot, directory.path());
    let reference = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: edited.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("import reference");

    // A different photograph of the shoot takes the photographer's own edit.
    let target = photo(&photos, "DSC0002.png");
    let before = render(&state, project, target);
    let analysis = prepare(&state, target, &reference.id);
    let fitting = analysis.fitting.as_ref().expect("diagnostics");
    assert_eq!(fitting.engine, "reference-original-calibrated-v1");
    let transfer = fitting.transfer.as_ref().expect("transfer diagnostics");
    assert_eq!(transfer.learned_from.as_deref(), Some("DSC0001.png"));
    assert!(transfer.original_refused.is_none());
    assert!(fitting.native_candidate_checked);
    assert_eq!(
        render(&state, project, target),
        before,
        "analysis edits nothing"
    );

    let applied = apply(&state, target, &analysis.session_id, 1.0);
    assert!(applied.full_resolution_checked && applied.changed > 0);
    let saved = state.recipe_store().load(&target).unwrap().unwrap();
    assert!(saved.extra.contains_key(aura_recipe::calibrated_look::KEY));
    assert!(!saved.extra.contains_key(aura_recipe::reference_grade::KEY));
    let after = render(&state, project, target);
    let truth = warm_matte(&scene(2)).into_raw();
    let moved = difference(&after, &before);
    let left = difference(&after, &truth);
    let start = difference(&before, &truth);
    // The photograph is visibly changed - never returned as it was - and it now carries the
    // photographer's edit: most of the way from where it started to the edit applied to it.
    assert!(
        moved > 4.0,
        "the matched photograph must differ from its original: {moved}"
    );
    assert!(
        left < 0.25 * start,
        "learned edit {left} against untouched {start}"
    );

    // Strength zero is the photograph exactly as it was; half lands between.
    let half = apply(&state, target, &analysis.session_id, 0.5);
    assert!(half.full_resolution_checked);
    let midway = render(&state, project, target);
    let (to_before, to_after) = (difference(&midway, &before), difference(&midway, &after));
    assert!(to_before > 1.0 && to_after > 1.0, "{to_before} {to_after}");
    apply(&state, target, &analysis.session_id, 0.0);
    assert_eq!(render(&state, project, target), before);

    // The search is remembered: a second photograph does not search again, and still learns.
    let third = photo(&photos, "DSC0003.png");
    let second = prepare(&state, third, &reference.id);
    assert_eq!(
        second.fitting.as_ref().unwrap().engine,
        "reference-original-calibrated-v1"
    );
}

#[test]
fn a_cropped_export_is_refused_with_a_reason_and_matching_falls_back_to_the_estimate() {
    let directory = tempfile::tempdir().unwrap();
    let shoot = directory.path().join("shoot");
    let exports = directory.path().join("exports");
    std::fs::create_dir_all(&shoot).unwrap();
    std::fs::create_dir_all(&exports).unwrap();
    for seed in 1..=2 {
        scene(seed)
            .save(shoot.join(format!("DSC000{seed}.png")))
            .unwrap();
    }
    // Cropped to a square: the pixels can no longer line up with the original.
    let cropped = image::imageops::crop_imm(&warm_matte(&scene(1)), 24, 0, 96, 96).to_image();
    let edited = exports.join("DSC0001-Edit.png");
    cropped.save(&edited).unwrap();
    // An export named after nothing in the shoot.
    let unrelated = exports.join("wedding-inspiration.png");
    warm_matte(&scene(1)).save(&unrelated).unwrap();
    let (state, project, photos) = open(&shoot, directory.path());
    let target = photo(&photos, "DSC0002.png");
    let before = render(&state, project, target);
    for (path, refused) in [(&edited, true), (&unrelated, false)] {
        let reference = color_match::import_reference(
            &state,
            &color_match::ImportInput {
                path: path.to_string_lossy().into_owned(),
                cancel_id: job(),
            },
        )
        .expect("import reference");
        let analysis = prepare(&state, target, &reference.id);
        let fitting = analysis.fitting.as_ref().unwrap();
        assert_ne!(fitting.engine, "reference-original-calibrated-v1");
        let transfer = fitting.transfer.as_ref().unwrap();
        assert!(transfer.learned_from.is_none());
        assert_eq!(
            transfer.original_refused.is_some(),
            refused,
            "{path:?}: {:?}",
            transfer.original_refused
        );
        if refused {
            assert!(transfer
                .original_refused
                .as_deref()
                .unwrap()
                .contains("DSC0001.png"));
        }
        // Estimated matching still works, and it either changes the photograph or says no
        // closer match passed; it never claims a change it did not make.
        let applied = apply(&state, target, &analysis.session_id, 1.0);
        let after = render(&state, project, target);
        if applied.changed > 0 {
            assert!(difference(&after, &before) > 0.0);
        } else {
            assert_eq!(after, before);
        }
        apply(&state, target, &analysis.session_id, 0.0);
    }
}
