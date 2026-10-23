//! The exact Match a look screen: one edited reference, native pixels and safe repeated fits.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::float_cmp,
    clippy::disallowed_methods
)]
use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::{reference_style, AppState};
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
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

#[test]
fn calibrated_pair_transfers_to_another_scene_and_restores_exactly() {
    let directory = tempfile::tempdir().unwrap();
    let original_path = directory.path().join("original.png");
    let edited_path = directory.path().join("edited.png");
    let other_path = directory.path().join("other-scene.png");
    let source = image::RgbImage::from_fn(128, 96, |x, y| {
        let h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263);
        image::Rgb([
            30 + (x * 185 / 128) as u8,
            30 + (y * 185 / 96) as u8,
            35 + ((h >> 16) % 170) as u8,
        ])
    });
    source.save(&original_path).unwrap();
    image::RgbImage::from_fn(96, 128, |x, y| {
        let h = x.wrapping_mul(668_265_263) ^ y.wrapping_mul(374_761_393);
        image::Rgb([
            40 + ((h >> 16) % 160) as u8,
            50 + (x * 140 / 96) as u8,
            40 + (y * 170 / 128) as u8,
        ])
    })
    .save(&other_path)
    .unwrap();
    let input_bytes = std::fs::read(&original_path).unwrap();
    let other_bytes = std::fs::read(&other_path).unwrap();
    let (state, project, photo) = setup(&original_path, directory.path());
    let analyse =
        |state: &AppState, edited: &std::path::Path, original: Option<&std::path::Path>| {
            reference_style::analyse_reference(
                state,
                &reference_style::AnalyseReferenceInput {
                    address: "Calibrated batch look".into(),
                    folder: edited.to_string_lossy().into_owned(),
                    cancel_id: uuid::Uuid::new_v4().to_string(),
                    export: false,
                    original: original.map(|p| p.to_string_lossy().into_owned()),
                },
            )
            .unwrap()
        };
    let ordinary = analyse(&state, &original_path, None);
    let mut input = reference_style::ApplyReferenceInput {
        photo_id: photo.to_db(),
        reference_id: ordinary.id,
        strength: 0.0,
        profile_id: None,
        profile_strength: None,
        cancel_id: None,
    };
    reference_style::apply_reference(&state, &input).unwrap();
    let source_baseline = render(&state, project, photo);
    // Warm matte with tone-dependent colour: a shared operation rather than shared subjects.
    let grade = |bytes: &[u8]| {
        bytes
            .chunks_exact(3)
            .flat_map(|p| {
                let [r, g, b] = [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.0);
                let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                [
                    0.055 + 0.91 * r + 0.018 * y * y,
                    0.045 + 0.89 * g,
                    0.04 + 0.85 * b + 0.018 * (1.0 - y).powi(2),
                ]
                .map(|v| (255.0 * v.clamp(0.0, 1.0)).round() as u8)
            })
            .collect::<Vec<_>>()
    };
    image::RgbImage::from_raw(128, 96, grade(&source_baseline))
        .unwrap()
        .save(&edited_path)
        .unwrap();
    let reference_bytes = std::fs::read(&edited_path).unwrap();
    let paired = analyse(&state, &edited_path, Some(&original_path));
    let report = paired.calibration.as_ref().unwrap();
    assert!(report.mean_error < 0.5 && report.validation_samples > 1000);
    input.reference_id = paired.id;
    input.strength = 1.0;
    reference_style::apply_reference(&state, &input).unwrap();
    assert!(error(&render(&state, project, photo), &grade(&source_baseline)) < 1.0);

    // A different scene and aspect ratio use the exact learned transform.
    let target_dir = directory.path().join("other-catalog");
    std::fs::create_dir_all(&target_dir).unwrap();
    let (other, other_project, other_photo) = setup(&other_path, &target_dir);
    let ordinary = analyse(&other, &original_path, None);
    input.photo_id = other_photo.to_db();
    input.reference_id = ordinary.id.clone();
    input.strength = 0.0;
    reference_style::apply_reference(&other, &input).unwrap();
    let baseline = render(&other, other_project, other_photo);
    let paired = analyse(&other, &edited_path, Some(&original_path));
    input.reference_id = paired.id;
    input.strength = 1.0;
    let result = reference_style::apply_reference(&other, &input).unwrap();
    let once = render(&other, other_project, other_photo);
    let truth = grade(&baseline);
    let before = error(&baseline, &truth);
    let after = error(&once, &truth);
    println!("calibrated native different-scene RGB error: {before:.4} -> {after:.4}");
    assert!(result.changed > 0 && result.after_distance < result.before_distance);
    assert!(after < 1.0 && after < 0.2 * before, "{before} -> {after}");
    let export_render = aura_app::render_image(
        &other,
        &aura_app::contract::ipc::RenderImageInput {
            photo_id: other_photo.to_db(),
            level: Some("full".into()),
            screen: None,
            colour_space: Some("srgb".into()),
            purpose: Some("export".into()),
        },
    )
    .unwrap();
    assert_eq!((export_render.width, export_render.height), (96, 128));
    assert_eq!(
        base64_decode(&export_render.rgb_base64),
        once,
        "preview and full export render use the saved calibration"
    );
    assert_eq!(
        reference_style::apply_reference(&other, &input)
            .unwrap()
            .changed,
        0
    );
    assert_eq!(render(&other, other_project, other_photo), once);
    input.strength = 0.5;
    reference_style::apply_reference(&other, &input).unwrap();
    let half = render(&other, other_project, other_photo);
    assert!(error(&half, &baseline) < error(&once, &baseline));
    assert!(error(&half, &baseline) > 0.25 * error(&once, &baseline));
    input.strength = 0.0;
    reference_style::apply_reference(&other, &input).unwrap();
    assert_eq!(render(&other, other_project, other_photo), baseline);
    assert!(aura_recipe::calibrated_look::read(
        &other.recipe_store().load(&other_photo).unwrap().unwrap()
    )
    .unwrap()
    .is_none());
    input.strength = 1.0;
    reference_style::apply_reference(&other, &input).unwrap();
    // Switching to a normal reference also removes the earlier calibration.
    input.reference_id = ordinary.id;
    input.strength = 0.0;
    reference_style::apply_reference(&other, &input).unwrap();
    assert_eq!(render(&other, other_project, other_photo), baseline);
    assert_eq!(std::fs::read(&original_path).unwrap(), input_bytes);
    assert_eq!(std::fs::read(&other_path).unwrap(), other_bytes);
    assert_eq!(std::fs::read(&edited_path).unwrap(), reference_bytes);
}

#[test]
fn single_reference_changes_native_pixels_without_compounding_and_zero_restores() {
    let directory = tempfile::tempdir().unwrap();
    let target_path = directory.path().join("target.png");
    let reference_path = directory.path().join("finished.tiff");
    let target = image::RgbImage::from_fn(128, 96, |x, y| {
        let v = 35 + (x * 145 / 128 + y * 25 / 96) as u8;
        image::Rgb(if y < 32 {
            [v, v.saturating_add(10), v.saturating_add(24)]
        } else if y < 64 {
            [v.saturating_add(20), v, v.saturating_sub(12)]
        } else {
            [v.saturating_sub(10), v.saturating_add(15), v]
        })
    });
    target.save(&target_path).unwrap();
    let original = std::fs::read(&target_path).unwrap();
    let (state, project, photo) = setup(&target_path, directory.path());
    let analysis_input = |path: &std::path::Path| reference_style::AnalyseReferenceInput {
        address: "My edited reference".into(),
        folder: path.to_string_lossy().into_owned(),
        cancel_id: uuid::Uuid::new_v4().to_string(),
        export: false,
        original: None,
    };
    let initial_reference =
        reference_style::analyse_reference(&state, &analysis_input(&target_path)).unwrap();
    assert_eq!(initial_reference.measured, 1);
    let mut input = reference_style::ApplyReferenceInput {
        photo_id: photo.to_db(),
        reference_id: initial_reference.id,
        strength: 0.0,
        profile_id: None,
        profile_strength: None,
        cancel_id: None,
    };
    reference_style::apply_reference(&state, &input).unwrap();
    let baseline = state.recipe_store().load(&photo).unwrap().unwrap();
    let before_pixels = render(&state, project, photo);
    let mut truth = baseline.clone();
    truth.global.contrast += 12;
    truth.global.temperature = (truth.global.temperature + 350).min(50000);
    truth.global.colour_grade.shadows.hue = 215;
    truth.global.colour_grade.shadows.saturation = 7;
    truth.global.colour_grade.highlights.hue = 40;
    truth.global.colour_grade.highlights.saturation = 5;
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &truth,
            &["global.contrast".into()],
            "fixture grade",
        )
        .unwrap();
    let truth_pixels = render(&state, project, photo);
    image::RgbImage::from_raw(128, 96, truth_pixels.clone())
        .unwrap()
        .save(&reference_path)
        .unwrap();
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &baseline,
            &["global.contrast".into()],
            "restore fixture baseline",
        )
        .unwrap();
    let reference =
        reference_style::analyse_reference(&state, &analysis_input(&reference_path)).unwrap();
    assert_eq!(reference.measured, 1);
    input.reference_id = reference.id;
    input.strength = 1.0;
    let fitted = reference_style::apply_reference(&state, &input).unwrap();
    let fitted_pixels = render(&state, project, photo);
    let before = error(&before_pixels, &truth_pixels);
    let after = error(&fitted_pixels, &truth_pixels);
    println!(
        "main Match a look native known grade: {before:.3} -> {after:.3}; measured {:.3} -> {:.3}",
        fitted.before_distance, fitted.after_distance
    );
    assert!(fitted.changed > 0 && fitted.after_distance < fitted.before_distance);
    assert!(after < 0.75 * before, "actual grade: {before} -> {after}");
    let once = state.recipe_store().load(&photo).unwrap().unwrap();
    let repeated = reference_style::apply_reference(&state, &input).unwrap();
    assert_eq!(
        render(&state, project, photo),
        fitted_pixels,
        "repeat application is pixel-identical"
    );
    assert_eq!(repeated.changed, 0, "no duplicate history step");
    assert_eq!(
        aura_recipe::hash::recipe_hash(&state.recipe_store().load(&photo).unwrap().unwrap())
            .unwrap(),
        aura_recipe::hash::recipe_hash(&once).unwrap()
    );
    input.strength = 0.0;
    reference_style::apply_reference(&state, &input).unwrap();
    assert_eq!(
        render(&state, project, photo),
        before_pixels,
        "zero restores the auto baseline exactly"
    );
    let mut bw = baseline.clone();
    bw.global.saturation = -100;
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &bw,
            &["global.saturation".into()],
            "BW fixture",
        )
        .unwrap();
    let bw_pixels = render(&state, project, photo);
    let bw_path = directory.path().join("monochrome-reference.png");
    image::RgbImage::from_raw(128, 96, bw_pixels.clone())
        .unwrap()
        .save(&bw_path)
        .unwrap();
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &baseline,
            &["global.saturation".into()],
            "restore BW fixture",
        )
        .unwrap();
    let bw_reference =
        reference_style::analyse_reference(&state, &analysis_input(&bw_path)).unwrap();
    input.reference_id = bw_reference.id;
    input.strength = 1.0;
    reference_style::apply_reference(&state, &input).unwrap();
    assert_eq!(
        render(&state, project, photo),
        bw_pixels,
        "a representable native monochrome reference matches exactly"
    );
    input.strength = 0.0;
    reference_style::apply_reference(&state, &input).unwrap();
    assert_eq!(render(&state, project, photo), before_pixels);
    let mut manual = baseline;
    manual.global.exposure = 0.25;
    manual
        .provenance
        .user_edited_fields
        .push("global.exposure".into());
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &manual,
            &["global.exposure".into()],
            "manual exposure",
        )
        .unwrap();
    input.strength = 1.0;
    let protected = reference_style::apply_reference(&state, &input).unwrap();
    assert!(protected.protected_fields > 0);
    assert_eq!(
        state
            .recipe_store()
            .load(&photo)
            .unwrap()
            .unwrap()
            .global
            .exposure,
        0.25
    );
    assert_eq!(std::fs::read(&target_path).unwrap(), original);
}

#[test]
fn cancelling_a_registered_native_fit_keeps_recipe_and_history() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target.png");
    image::RgbImage::from_fn(128, 96, |x, y| {
        image::Rgb([40 + x as u8, 60 + y as u8, 100])
    })
    .save(&target)
    .unwrap();
    let (state, _, photo) = setup(&target, directory.path());
    let reference = reference_style::analyse_reference(
        &state,
        &reference_style::AnalyseReferenceInput {
            address: "Cancellation reference".into(),
            folder: target.to_string_lossy().into_owned(),
            cancel_id: "analysis".into(),
            export: false,
            original: None,
        },
    )
    .unwrap();
    let recipe = state.recipe_store().load(&photo).unwrap();
    let count = || {
        state
            .catalog()
            .read(|conn| {
                conn.query_row("SELECT COUNT(*) FROM edit_history", [], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(|e| aura_core::errors::db::statement_failed("history count", &e))
            })
            .unwrap()
    };
    let history = count();
    let id = uuid::Uuid::new_v4().to_string();
    let worker_state = state.clone();
    let job_id = id.clone();
    let worker = std::thread::spawn(move || {
        reference_style::apply_reference(
            &worker_state,
            &reference_style::ApplyReferenceInput {
                photo_id: photo.to_db(),
                reference_id: reference.id,
                strength: 1.0,
                profile_id: None,
                profile_strength: None,
                cancel_id: Some(job_id),
            },
        )
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut cancelled = false;
    while std::time::Instant::now() < deadline {
        if state.cancel_job(&id) {
            cancelled = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let result = worker.join().unwrap();
    assert!(cancelled && result.is_err(), "{result:?}");
    assert_eq!(state.recipe_store().load(&photo).unwrap(), recipe);
    assert_eq!(count(), history);
    assert!(!state.cancel_job(&id), "completed jobs are unregistered");
}

/// Optional local-photo review. Explicit paths keep CI independent of workstation photos.
#[test]
#[ignore = "set AURA_REFERENCE_REVIEW_TARGETS, AURA_REFERENCE_REVIEW_REFERENCES and AURA_REFERENCE_REVIEW_OUTPUT"]
fn review_existing_photos_through_the_main_reference_workflow() {
    let paths = |name| {
        serde_json::from_str::<Vec<std::path::PathBuf>>(&std::env::var(name).expect("review paths"))
            .unwrap()
    };
    let targets = paths("AURA_REFERENCE_REVIEW_TARGETS");
    let references = paths("AURA_REFERENCE_REVIEW_REFERENCES");
    let output = std::path::PathBuf::from(
        std::env::var("AURA_REFERENCE_REVIEW_OUTPUT").expect("review output"),
    );
    std::fs::create_dir_all(&output).unwrap();
    let mut results = Vec::new();
    let mut html = String::from("<!doctype html><meta charset='utf-8'><title>Main Match a look native review</title><style>body{background:#201d26;color:#eee;font:16px system-ui;max-width:1400px;margin:30px auto}article{display:grid;grid-template-columns:repeat(3,1fr);gap:16px}img{width:100%;max-height:600px;object-fit:contain}figure{margin:0}section{margin:40px 0}p{line-height:1.5}</style><h1>Main Match a look: native review</h1><p>Existing local photo previews and edited references, processed through analyse_reference_style and apply_reference_style. Reference distance is not a visual accuracy percentage. These photos are different scenes; no independent target grade is known.</p>");
    for (index, (target, reference)) in targets.iter().zip(&references).enumerate() {
        let directory = output.join(format!("case-{index}"));
        std::fs::create_dir_all(&directory).unwrap();
        let original = std::fs::read(target).unwrap();
        let reference_bytes = std::fs::read(reference).unwrap();
        let (state, project, photo) = setup(target, &directory);
        let measured = reference_style::analyse_reference(
            &state,
            &reference_style::AnalyseReferenceInput {
                address: format!("Edited local reference {}", index + 1),
                folder: reference.to_string_lossy().into_owned(),
                cancel_id: uuid::Uuid::new_v4().to_string(),
                export: false,
                original: None,
            },
        )
        .unwrap();
        let mut input = reference_style::ApplyReferenceInput {
            photo_id: photo.to_db(),
            reference_id: measured.id,
            strength: 0.0,
            profile_id: None,
            profile_strength: None,
            cancel_id: None,
        };
        reference_style::apply_reference(&state, &input).unwrap();
        let save_preview = |name| {
            let preview =
                aura_app::native_retouch::preview(&state, &project.to_db(), &photo.to_db(), false)
                    .unwrap();
            image::RgbImage::from_raw(
                preview.width,
                preview.height,
                base64_decode(&preview.rgb_base64),
            )
            .unwrap()
            .save(output.join(format!("{index}-{name}.png")))
            .unwrap();
        };
        save_preview("before");
        input.strength = 0.8;
        let fitted = reference_style::apply_reference(&state, &input).unwrap();
        save_preview("after");
        std::fs::copy(reference, output.join(format!("{index}-reference.jpg"))).unwrap();
        assert!(fitted.after_distance <= fitted.before_distance);
        assert_eq!(std::fs::read(target).unwrap(), original);
        assert_eq!(std::fs::read(reference).unwrap(), reference_bytes);
        println!(
            "local main-workflow case {index}: {:.3} -> {:.3}; {} changes",
            fitted.before_distance, fitted.after_distance, fitted.changed
        );
        html.push_str(&format!("<section><h2>Case {}</h2><p>Native appearance distance {:.3} to {:.3}; requested strength 80%; {} fields changed.</p><article><figure><img src='{index}-reference.jpg'><figcaption>Uploaded edited reference</figcaption></figure><figure><img src='{index}-before.png'><figcaption>Target auto baseline</figcaption></figure><figure><img src='{index}-after.png'><figcaption>Main-workflow match</figcaption></figure></article></section>", index + 1, fitted.before_distance, fitted.after_distance, fitted.changed));
        results.push(serde_json::json!({"case": index, "before": fitted.before_distance, "after": fitted.after_distance, "changed": fitted.changed, "originalsPreserved": true}));
    }
    assert!(!results.is_empty());
    std::fs::write(output.join("index.html"), html).unwrap();
    std::fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
}
