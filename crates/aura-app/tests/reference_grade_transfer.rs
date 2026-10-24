//! Reference colour matching by distribution transfer (ADR-0160), through the real commands
//! and the real renderer: a different photograph takes an edited reference's look, zero restores
//! the exact pixels, Undo and Redo walk it, and a second reference replaces the first.
#![allow(clippy::expect_used, clippy::too_many_lines, clippy::float_cmp)]
use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::{color_match, AppState};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_raw::colour::curve::{srgb_byte_to_linear, srgb_decode};
use aura_render::reference_grade::oklab_from_linear_srgb;
use std::sync::Arc;

fn setup(path: &std::path::Path, directory: &std::path::Path) -> (AppState, ProjectId, PhotoId) {
    let state = AppState::open(&directory.join("catalog.aura"))
        .expect("isolated catalogue")
        .with_cache_root(&directory.join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Reference grade transfer".into(),
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
            roots: vec![path.to_path_buf()],
            mode: ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import target");
    let photos = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id,
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("list target");
    let photo = PhotoId::from_db(&photos.first().expect("target").id).expect("photo id");
    let baseline = aura_app::develop_commands::neutral_recipe(&state, photo);
    state
        .recipe_store()
        .save(&project_id, &photo, &baseline, &[], "Camera baseline")
        .expect("initial edit");
    (state, project_id, photo)
}

fn job() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393)
        ^ y.wrapping_mul(668_265_263)
        ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65_535.0 - 0.5
}

fn ellipse(x: u32, y: u32, cx: f32, cy: f32, rx: f32, ry: f32) -> bool {
    ((x as f32 - cx) / rx).powi(2) + ((y as f32 - cy) / ry).powi(2) <= 1.0
}

fn paint(width: u32, height: u32, f: impl Fn(u32, u32) -> [f32; 3]) -> image::RgbImage {
    image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb(f(x, y).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
    })
}

fn outdoor() -> image::RgbImage {
    paint(192, 128, |x, y| {
        let n = noise(x, y, 1) * 0.04;
        let fy = y as f32 / 128.0;
        if ellipse(x, y, 70.0, 52.0, 14.0, 18.0) {
            [0.86 + n, 0.66 + n, 0.53 + n]
        } else if (55..90).contains(&x) && y > 70 {
            [0.93 + n * 0.5, 0.92 + n * 0.5, 0.9 + n * 0.5]
        } else if (108..138).contains(&x) && y > 74 {
            [0.1 + n, 0.11 + n, 0.13 + n]
        } else if fy < 0.35 {
            [0.45 + 0.3 * fy + n, 0.62 + 0.2 * fy + n, 0.92 + n]
        } else if fy < 0.55 {
            [0.62 + n, 0.47 + n, 0.33 + n * 2.0]
        } else {
            let g = 0.35 + 0.25 * noise(x / 6, y / 6, 7);
            [0.22 * g + n, 0.5 * g + 0.1 + n, 0.12 * g + n]
        }
    })
}

fn indoor() -> image::RgbImage {
    paint(176, 132, |x, y| {
        let n = noise(x, y, 3) * 0.04;
        let (fx, fy) = (x as f32 / 176.0, y as f32 / 132.0);
        if ellipse(x, y, 52.0, 44.0, 12.0, 16.0) {
            [0.78 + n, 0.58 + n, 0.47 + n]
        } else if (38..68).contains(&x) && y > 62 {
            [0.72 + n, 0.1 + n, 0.12 + n]
        } else if (104..134).contains(&x) && y > 58 {
            [0.85 + n, 0.84 + n, 0.82 + n]
        } else if fx > 0.85 {
            [0.06 + n * 0.5, 0.06 + n * 0.5, 0.07 + n * 0.5]
        } else if fx < 0.12 && fy > 0.3 {
            [0.15 + n, 0.42 + n, 0.18 + n]
        } else {
            [0.8 - 0.2 * fy + n, 0.7 - 0.2 * fy + n, 0.55 - 0.15 * fy + n]
        }
    })
}

fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// Teal shadows and orange highlights: the look the slider fit moved away from on new photos.
fn split_tone(rgb: [f32; 3]) -> [f32; 3] {
    let y = luma(rgb);
    let (shadow, high) = ((1.0 - y).powi(2), y * y);
    [
        rgb[0] - 0.06 * shadow + 0.07 * high,
        rgb[1] + 0.015 * shadow + 0.015 * high,
        rgb[2] + 0.07 * shadow - 0.07 * high,
    ]
}

fn warm_matte(rgb: [f32; 3]) -> [f32; 3] {
    let lifted = rgb.map(|x| 0.08 + 0.84 * x);
    let lin = lifted.map(srgb_decode);
    let warmed = [lin[0] * 1.15, lin[1] * 1.02, lin[2] * 0.82];
    warmed.map(|v| {
        let x = v.clamp(0.0, 1.0);
        if x <= 0.003_130_8 {
            12.92 * x
        } else {
            1.055 * x.powf(1.0 / 2.4) - 0.055
        }
    })
}

fn graded(source: &image::RgbImage, grade: fn([f32; 3]) -> [f32; 3]) -> image::RgbImage {
    let mut out = source.clone();
    for pixel in out.pixels_mut() {
        let encoded = pixel.0.map(|v| f32::from(v) / 255.0);
        pixel.0 = grade(encoded).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    }
    out
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

/// Mean OKLab difference times a hundred - close to CIELAB ΔE in size.
fn error(rgb: &[u8], truth: &image::RgbImage) -> f32 {
    let lab = |p: &[u8]| oklab_from_linear_srgb([p[0], p[1], p[2]].map(srgb_byte_to_linear));
    assert_eq!(rgb.len(), truth.as_raw().len());
    let total: f32 = rgb
        .chunks_exact(3)
        .zip(truth.as_raw().chunks_exact(3))
        .map(|(a, b)| {
            let (a, b) = (lab(a), lab(b));
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
        })
        .sum();
    100.0 * total / (rgb.len() / 3) as f32
}

#[test]
fn a_new_photograph_takes_an_edited_references_look_through_the_renderer() {
    let directory = tempfile::tempdir().expect("isolated transfer");
    let target_path = directory.path().join("target.png");
    let reference_path = directory.path().join("reference.png");
    let second_path = directory.path().join("second-reference.png");
    let target = outdoor();
    target.save(&target_path).expect("target");
    graded(&indoor(), split_tone)
        .save(&reference_path)
        .expect("reference");
    graded(&indoor(), warm_matte)
        .save(&second_path)
        .expect("second reference");
    let truth = graded(&target, split_tone);
    let original = std::fs::read(&target_path).expect("original bytes");
    let (state, project, photo) = setup(&target_path, directory.path());
    let render = |state: &AppState| {
        base64_decode(
            &aura_app::native_retouch::preview(state, &project.to_db(), &photo.to_db(), false)
                .expect("native preview")
                .rgb_base64,
        )
    };
    let baseline_pixels = render(&state);
    let reference = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: reference_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("reference");
    let prepared = color_match::prepare(
        &state,
        &color_match::PrepareInput {
            intent: Default::default(),
            quality: Default::default(),
            photo_id: photo.to_db(),
            reference_id: reference.id.clone(),
            cancel_id: job(),
        },
    )
    .expect("transfer fit");
    let fitting = prepared.fitting.clone().expect("diagnostics");
    assert_eq!(fitting.engine, "reference-grade-transfer-v3");
    let transfer = fitting.transfer.expect("transfer diagnostics");
    assert!(
        transfer.rendered_after.total < transfer.rendered_before.total,
        "{transfer:?}"
    );
    assert_eq!(render(&state), baseline_pixels, "analysis changes nothing");

    let applied = color_match::apply(
        &state,
        &color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: prepared.session_id.clone(),
            controls: color_match::Controls::default(),
            cancel_id: job(),
        },
    )
    .expect("apply");
    assert!(applied.changed > 0 && applied.full_resolution_checked && applied.current);
    assert!(applied.after_distance < applied.before_distance);
    let matched_pixels = render(&state);
    let before = error(&baseline_pixels, &truth);
    let after = error(&matched_pixels, &truth);
    println!("split tone on a new photograph through the renderer: {before:.2} -> {after:.2}");

    let half = color_match::apply(
        &state,
        &color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: prepared.session_id.clone(),
            controls: color_match::Controls {
                amount: 0.5,
                ..color_match::Controls::default()
            },
            cancel_id: job(),
        },
    )
    .expect("half");
    assert!(half.after_distance > applied.after_distance);
    let zero = color_match::apply(
        &state,
        &color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: prepared.session_id.clone(),
            controls: color_match::Controls {
                amount: 0.0,
                ..color_match::Controls::default()
            },
            cancel_id: job(),
        },
    )
    .expect("zero");
    assert!(zero.current);
    assert_eq!(
        render(&state),
        baseline_pixels,
        "zero restores the exact pixels"
    );
    let mut step = aura_app::contract::ipc::HistoryStepInput {
        project_id: project.to_db(),
        photo_id: photo.to_db(),
        action: "undo".into(),
    };
    aura_app::history_step(&state, &step).expect("undo zero");
    aura_app::history_step(&state, &step).expect("undo half");
    assert_eq!(
        render(&state),
        matched_pixels,
        "undo returns the full match"
    );
    step.action = "redo".into();
    aura_app::history_step(&state, &step).expect("redo half");
    aura_app::history_step(&state, &step).expect("redo zero");
    assert_eq!(render(&state), baseline_pixels);
    step.action = "undo".into();
    aura_app::history_step(&state, &step).expect("back to half");
    let half_pixels = render(&state);

    // A second reference replaces the first rather than stacking on it, and its zero is the
    // first match exactly.
    let second = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: second_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("second reference");
    let replacing = color_match::prepare(
        &state,
        &color_match::PrepareInput {
            intent: Default::default(),
            quality: Default::default(),
            photo_id: photo.to_db(),
            reference_id: second.id,
            cancel_id: job(),
        },
    )
    .expect("second fit");
    assert!(replacing
        .fitting
        .and_then(|f| f.transfer)
        .is_some_and(|t| t.replaced_earlier_match));
    color_match::apply(
        &state,
        &color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: replacing.session_id.clone(),
            controls: color_match::Controls {
                amount: 0.0,
                ..color_match::Controls::default()
            },
            cancel_id: job(),
        },
    )
    .expect("second at zero");
    assert_eq!(
        render(&state),
        half_pixels,
        "zero keeps the earlier match exactly"
    );
    let second_full = color_match::apply(
        &state,
        &color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: replacing.session_id.clone(),
            controls: color_match::Controls::default(),
            cancel_id: job(),
        },
    )
    .expect("second at full");
    assert!(second_full.after_distance < second_full.before_distance);
    let warm_truth = graded(&target, warm_matte);
    let replaced = render(&state);
    let warm_before = error(&baseline_pixels, &warm_truth);
    let warm_after = error(&replaced, &warm_truth);
    println!("warm matte replacing the split tone: {warm_before:.2} -> {warm_after:.2}");

    // The saved session survives a restart and the original file was never written.
    drop(state);
    let state = AppState::open(&directory.path().join("catalog.aura"))
        .expect("reopen")
        .with_cache_root(&directory.path().join("cache"));
    let saved = color_match::status(&state, &photo.to_db())
        .expect("status")
        .expect("session");
    assert!(saved.current && saved.full_resolution_checked);
    assert_eq!(
        std::fs::read(&target_path).expect("read original"),
        original
    );
    // Retain both photographic accuracy gates, after the independent history, replacement,
    // restart and source-preservation checks so a fidelity failure cannot hide those results.
    assert!(
        after < 0.6 * before && warm_after < 0.6 * warm_before,
        "known grade: split {before:.2} -> {after:.2}; warm matte {warm_before:.2} -> {warm_after:.2}"
    );
}

#[test]
fn a_native_regression_is_refused_and_stale_measurements_can_be_refreshed() {
    let directory = tempfile::tempdir().expect("isolated verification");
    let target_path = directory.path().join("target.png");
    let reference_path = directory.path().join("reference.png");
    outdoor().save(&target_path).expect("target");
    graded(&indoor(), warm_matte)
        .save(&reference_path)
        .expect("reference");
    let (state, _, photo) = setup(&target_path, directory.path());
    let input = color_match::ImportInput {
        path: reference_path.to_string_lossy().into_owned(),
        cancel_id: job(),
    };
    let reference = color_match::import_reference(&state, &input).expect("reference");
    let prepare = color_match::PrepareInput {
        intent: Default::default(),
        quality: Default::default(),
        photo_id: photo.to_db(),
        reference_id: reference.id.clone(),
        cancel_id: job(),
    };
    let session = color_match::prepare(&state, &prepare).expect("analysis");
    let baseline = state.recipe_store().load(&photo).expect("baseline");
    let history = || {
        state
            .catalog()
            .read(|conn| {
                conn.query_row("SELECT COUNT(*) FROM edit_history", [], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(|e| aura_core::errors::db::statement_failed("count history", &e))
            })
            .expect("history")
    };
    let history_before = history();
    let session_key = format!("colour_match_v1:{}", photo.to_db());
    let black_curve =
        serde_json::to_string(&vec![0.0; aura_recipe::reference_grade::TONE_NODES]).expect("curve");
    // A valid stored transform that a damaged or older fit might propose. Native verification
    // must stop it before either the recipe or Undo history is written.
    state.catalog().writer().with(move |conn| {
        conn.execute("UPDATE setting SET value_json=json_set(value_json, '$.transfer.fitted.tone', json(?1)) WHERE key=?2",
            rusqlite::params![black_curve, session_key])
            .map_err(|e| aura_core::errors::db::statement_failed("inject bad fit", &e))?;
        Ok(())
    }).expect("inject fit");
    let refused = color_match::apply(
        &state,
        &color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: session.session_id,
            controls: color_match::Controls::default(),
            cancel_id: job(),
        },
    )
    .expect_err("worse native result must be refused");
    assert!(
        refused.message.contains("farther from the reference"),
        "{refused:?}"
    );
    assert_eq!(
        state.recipe_store().load(&photo).expect("unchanged"),
        baseline
    );
    assert_eq!(history(), history_before);

    let stats_key = format!("colour_reference_grade_v1:{}", reference.id);
    let session_key = format!("colour_match_v1:{}", photo.to_db());
    state
        .catalog()
        .writer()
        .with(move |conn| {
            conn.execute(
                "UPDATE setting SET value_json=json_set(value_json, '$.version', 2) WHERE key=?1",
                [stats_key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("stale measurement", &e))?;
            conn.execute(
                "UPDATE setting SET value_json=json_set(value_json, '$.transfer.reference.version', 2) WHERE key=?1",
                [session_key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("stale session", &e))?;
            Ok(())
        })
        .expect("stale cache");
    let saved = color_match::status(&state, &photo.to_db())
        .expect("old analysis")
        .expect("session");
    assert!(!saved.current && !saved.full_resolution_checked);
    let stale = color_match::apply(
        &state,
        &color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: saved.session_id,
            controls: color_match::Controls::default(),
            cancel_id: job(),
        },
    )
    .expect_err("an old analysis must not compare different measurement versions");
    assert!(
        stale.message.contains("fresh colour measurements"),
        "{stale:?}"
    );
    let stale = color_match::prepare(&state, &prepare).expect_err("refresh required");
    assert!(
        stale.message.contains("Upload its edited photo again"),
        "{stale:?}"
    );
    assert_eq!(
        state.recipe_store().load(&photo).expect("unchanged"),
        baseline
    );
    color_match::import_reference(&state, &input).expect("refresh the same content hash");
    assert_eq!(
        color_match::prepare(&state, &prepare)
            .expect("fresh analysis")
            .fitting
            .expect("diagnostics")
            .engine,
        "reference-grade-transfer-v3"
    );
    assert_eq!(history(), history_before);
}

#[test]
fn clipping_is_refused_even_when_it_would_match_reference_statistics() {
    let directory = tempfile::tempdir().expect("isolated clipping guard");
    let target_path = directory.path().join("target.png");
    let reference_path = directory.path().join("white.png");
    outdoor().save(&target_path).expect("target");
    image::RgbImage::from_pixel(96, 96, image::Rgb([255; 3]))
        .save(&reference_path)
        .expect("white reference");
    let (state, _, photo) = setup(&target_path, directory.path());
    let baseline = state.recipe_store().load(&photo).expect("baseline");
    let reference = color_match::import_reference(
        &state,
        &color_match::ImportInput {
            path: reference_path.to_string_lossy().into_owned(),
            cancel_id: job(),
        },
    )
    .expect("reference");
    let session = color_match::prepare(
        &state,
        &color_match::PrepareInput {
            intent: Default::default(),
            quality: Default::default(),
            photo_id: photo.to_db(),
            reference_id: reference.id,
            cancel_id: job(),
        },
    )
    .expect("analysis");
    let key = format!("colour_match_v1:{}", photo.to_db());
    let white = serde_json::to_string(&vec![1.0; aura_recipe::reference_grade::TONE_NODES])
        .expect("white curve");
    let before_history = state
        .catalog()
        .read(|conn| {
            conn.query_row("SELECT COUNT(*) FROM edit_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|e| aura_core::errors::db::statement_failed("history", &e))
        })
        .expect("history");
    state.catalog().writer().with(move |conn| {
        conn.execute("UPDATE setting SET value_json=json_set(value_json, '$.transfer.fitted.tone', json(?1)) WHERE key=?2",
            rusqlite::params![white, key])
            .map_err(|e| aura_core::errors::db::statement_failed("inject clipped fit", &e))?;
        Ok(())
    }).expect("clipped fit");
    let refused = color_match::apply(
        &state,
        &color_match::ApplyInput {
            photo_id: photo.to_db(),
            session_id: session.session_id,
            controls: color_match::Controls::default(),
            cancel_id: job(),
        },
    )
    .expect_err("a perfect white histogram cannot justify destroying target detail");
    assert!(
        refused.message.contains("full-resolution highlight"),
        "{refused:?}"
    );
    assert_eq!(
        state.recipe_store().load(&photo).expect("unchanged"),
        baseline
    );
    let after_history = state
        .catalog()
        .read(|conn| {
            conn.query_row("SELECT COUNT(*) FROM edit_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|e| aura_core::errors::db::statement_failed("history", &e))
        })
        .expect("history");
    assert_eq!(before_history, after_history);
}
