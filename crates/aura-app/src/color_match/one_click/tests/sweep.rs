//! Compare the conservative fit with look fits on real photographs, by numbers and by eye.
//! `AURA_LOOK_TARGETS=a.CR3,b.CR3 AURA_LOOK_REFERENCES=x.jpg,y.jpg AURA_LOOK_OUT=<folder>`,
//! release build, `--ignored --nocapture`.
#![allow(
    clippy::too_many_lines,
    clippy::print_stderr,
    clippy::expect_used,
    clippy::similar_names
)]
use super::super::super::transfer::{self, LookOptions};
use super::super::super::{
    asset_key, content, matcher, read, session_key, ImportInput, Reference, Session,
};
use super::*;
use crate::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_core::progress::NullProgress;
use aura_render::FrameSource;

pub(super) fn variants() -> Vec<(&'static str, Option<LookOptions>)> {
    vec![
        ("conservative", None),
        ("look", Some(LookOptions::default())),
        (
            "look-strong",
            Some(LookOptions {
                tone_shape: 0.9,
                brightness: 0.5,
                max_brightness: 0.1,
                chroma: 0.75,
                hue: 0.8,
                tint: 0.85,
                tint_chroma: 0.12,
                palette: 0.7,
                iterations: 3,
            }),
        ),
        (
            "look-soft",
            Some(LookOptions {
                tone_shape: 0.5,
                brightness: 0.25,
                max_brightness: 0.04,
                chroma: 0.35,
                hue: 0.45,
                tint: 0.4,
                tint_chroma: 0.12,
                palette: 0.7,
                iterations: 2,
            }),
        ),
    ]
}

fn small(image: &Rgb8) -> image::RgbImage {
    image::RgbImage::from_raw(image.width, image.height, image.data.clone())
        .map(|i| image::imageops::thumbnail(&i, 520, 520))
        .expect("image")
}

#[test]
#[ignore = "needs real photographs and references"]
fn look_sweep() {
    let (Ok(targets), Ok(references), Ok(out)) = (
        std::env::var("AURA_LOOK_TARGETS"),
        std::env::var("AURA_LOOK_REFERENCES"),
        std::env::var("AURA_LOOK_OUT"),
    ) else {
        return;
    };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).expect("out");
    let directory = tempfile::tempdir().expect("isolated");
    let state = AppState::open(&directory.path().join("catalog.aura"))
        .expect("catalogue")
        .with_cache_root(&directory.path().join("cache"))
        .with_key_store(std::sync::Arc::new(
            aura_cloud::keys::MemoryKeyStore::default(),
        ));
    let project = crate::create_project(
        &state,
        CreateProjectInput {
            name: "look sweep".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    let project_id = ProjectId::from_db(&project.id).expect("project id");
    aura_ingest::run(
        state.catalog(),
        &aura_ingest::contract::ingest::ImportPlan {
            import_id: aura_core::ImportId::new(),
            project_id,
            roots: targets.split(',').map(std::path::PathBuf::from).collect(),
            mode: aura_ingest::contract::ingest::ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    let photos = crate::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id,
            offset: 0,
            limit: 50,
            order_by: None,
        },
    )
    .expect("list");
    for reference_path in references.split(',') {
        let summary = super::super::super::import_reference(
            &state,
            &ImportInput {
                path: reference_path.into(),
                cancel_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .expect("reference");
        let stats = transfer::recall(&state, &summary.id)
            .expect("recall")
            .expect("stats");
        let stored: Reference = read(&state, &asset_key(&summary.id).expect("key"))
            .expect("read")
            .expect("reference");
        let reference_image = image::open(reference_path).expect("open").to_rgb8();
        for row in &photos {
            let photo = PhotoId::from_db(&row.id).expect("photo");
            let baseline = crate::develop_commands::neutral_recipe(&state, photo);
            let fitting = fitting_frame(&state, &photo).expect("fitting");
            let plain = rendered(&state, &fitting, &baseline).expect("plain");
            let regions = appearance::analyse(&plain).expect("regions");
            let samples = transfer::samples(&plain, transfer::TARGET_SAMPLES);
            let frame = crate::photo_frames::CatalogFrames::new(state.clone())
                .frame(&photo, aura_render::RenderLevel::Full)
                .expect("frame");
            let base_full = rendered(&state, &frame, &baseline).expect("base");
            let before_regions = appearance::readings(&base_full, &regions);
            let before_clip = appearance::clipped(&base_full);
            let similarity = content::similarity(&stored.small_rgb, &plain);
            let mut tiles = vec![
                image::imageops::thumbnail(&reference_image, 520, 520),
                small(&base_full),
            ];
            for (name, options) in variants() {
                let mut fit = match options {
                    None => transfer::fit_with_checkpoint(&stats, &samples, similarity, || Ok(())),
                    Some(o) => transfer::fit_look(
                        &stats,
                        &samples,
                        &transfer::skin_samples(&plain, &regions),
                        &transfer::palette(&stored.small_rgb),
                        &o,
                        || Ok(()),
                    ),
                }
                .expect("fit");
                let review = transfer::review_skin(&mut fit.grade, &plain, &regions, false);
                let mut candidate = baseline.clone();
                aura_recipe::reference_grade::write(&mut candidate, &fit.grade).expect("write");
                let image = rendered(&state, &frame, &candidate).expect("render");
                let after = appearance::readings(&image, &regions);
                let face: Vec<String> = before_regions
                    .iter()
                    .filter(|r| r.name == "face skin")
                    .filter_map(|a| {
                        after.iter().find(|b| b.name == a.name).map(|b| {
                            format!(
                                "face mid {:.1} hi {:.1} chroma {:+.1} tone {:+.3} strict {} look {}",
                                a.appearance.mid.distance(b.appearance.mid),
                                a.appearance.high.distance(b.appearance.high),
                                (b.appearance.chroma_p50 - a.appearance.chroma_p50) * 128.0,
                                b.appearance.tone.p50 - a.appearance.tone.p50,
                                super::super::super::skin_guard(&before_regions, &after, false),
                                appearance::skin_natural(&before_regions, &after)
                            )
                        })
                    })
                    .collect();
                eprintln!(
                    "{} <- {}: {name:13} distance {:.2} -> {:.2} clip {:+.4} turn {:.1} deg x{:.2} {}",
                    row.file_name,
                    summary.name,
                    fit.before.total,
                    matcher::measured(&stats, &image).total,
                    appearance::clipped(&image) - before_clip,
                    review.worst_hue_shift_degrees,
                    review.worst_chroma_ratio,
                    face.join(" ")
                );
                tiles.push(small(&image));
            }
            let width: u32 = tiles.iter().map(|t| t.width() + 6).sum();
            let height = tiles.iter().map(image::RgbImage::height).max().unwrap_or(1);
            let mut sheet = image::RgbImage::from_pixel(width, height, image::Rgb([24, 24, 24]));
            let mut x = 0_i64;
            for tile in &tiles {
                image::imageops::replace(&mut sheet, tile, x, 0);
                x += i64::from(tile.width() + 6);
            }
            let stem = summary.name.replace('.', "_");
            sheet
                .save(out.join(format!("{}__{stem}.jpg", row.file_name)))
                .expect("save");
        }
    }
}

/// The one-click command itself on every target and reference: what was saved, how close, how
/// fast. Same environment variables as [`look_sweep`].
#[test]
#[ignore = "needs real photographs and references"]
fn one_click_real() {
    let (Ok(targets), Ok(references)) = (
        std::env::var("AURA_LOOK_TARGETS"),
        std::env::var("AURA_LOOK_REFERENCES"),
    ) else {
        return;
    };
    let directory = tempfile::tempdir().expect("isolated");
    let state = AppState::open(&directory.path().join("catalog.aura"))
        .expect("catalogue")
        .with_cache_root(&directory.path().join("cache"))
        .with_key_store(std::sync::Arc::new(
            aura_cloud::keys::MemoryKeyStore::default(),
        ));
    let project = crate::create_project(
        &state,
        CreateProjectInput {
            name: "one click".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    aura_ingest::run(
        state.catalog(),
        &aura_ingest::contract::ingest::ImportPlan {
            import_id: aura_core::ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("id"),
            roots: targets.split(',').map(std::path::PathBuf::from).collect(),
            mode: aura_ingest::contract::ingest::ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    let photos = crate::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id,
            offset: 0,
            limit: 50,
            order_by: None,
        },
    )
    .expect("list");
    for reference_path in references.split(',') {
        let summary = super::super::super::import_reference(
            &state,
            &ImportInput {
                path: reference_path.into(),
                cancel_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .expect("reference");
        let reference_picture = image::open(reference_path).expect("open").to_rgb8();
        for row in &photos {
            let photo = PhotoId::from_db(&row.id).expect("photo");
            let fitting = fitting_frame(&state, &photo).expect("fitting");
            let neutral = crate::develop_commands::neutral_recipe(&state, photo);
            let mut tiles = vec![
                image::imageops::thumbnail(&reference_picture, 520, 520),
                small(&rendered(&state, &fitting, &neutral).expect("before")),
            ];
            for (level, amount) in [("Standard", 1.0_f32), ("Extreme", 1.6)] {
                let clock = std::time::Instant::now(); // ALLOW-BANNED: test timing
                let result = quick_match(
                    &state,
                    &QuickMatchInput {
                        photo_id: row.id.clone(),
                        reference_id: summary.id.clone(),
                        controls: Controls {
                            amount,
                            ..Controls::default()
                        },
                        cancel_id: uuid::Uuid::new_v4().to_string(),
                        step_down: true,
                        reanalyse: false,
                    },
                );
                if result.is_ok() {
                    let saved = state
                        .recipe_store()
                        .load(&photo)
                        .expect("load")
                        .expect("saved");
                    tiles.push(small(&rendered(&state, &fitting, &saved).expect("after")));
                }
                match result {
                    Ok(r) => eprintln!(
                        "ONECLICK {} <- {} {level}: {:.1}s saved {:.0}% in {} checks, distance {:.2} -> {:.2} limited by {:?}",
                        row.file_name,
                        summary.name,
                        clock.elapsed().as_secs_f32(),
                        r.report.controls.amount * 100.0,
                        r.attempts,
                        r.report.before_distance,
                        r.report.after_distance,
                        r.limited_by
                    ),
                    Err(e) => {
                        let photo = PhotoId::from_db(&row.id).expect("photo");
                        let loaded = state
                            .recipe_store()
                            .load(&photo)
                            .expect("load")
                            .map(|r| aura_recipe::hash::recipe_hash(&r).expect("hash"));
                        let stored: Option<String> = state
                            .catalog()
                            .read(|c| {
                                rusqlite::OptionalExtension::optional(c.query_row(
                                    "SELECT recipe_hash FROM edit_recipes WHERE photo_id=?1",
                                    [&row.id],
                                    |r| r.get(0),
                                ))
                                .map_err(|e| aura_core::errors::db::statement_failed("read", &e))
                            })
                            .expect("stored");
                        let session: Option<Session> =
                            read(&state, &session_key(&row.id).expect("key")).expect("session");
                        eprintln!(
                            "ONECLICK {} <- {} {level}: {:.1}s REFUSED {} | stored {:?} loaded {:?} session last {:?} baseline {:?}",
                            row.file_name,
                            summary.name,
                            clock.elapsed().as_secs_f32(),
                            e.message,
                            stored.as_deref().map(|h| &h[..10]),
                            loaded.as_deref().map(|h| &h[..10]),
                            session.as_ref().map(|s| &s.last_hash[..10]),
                            session.as_ref().map(|s| &s.baseline_hash[..10]),
                        );
                    }
                }
            }
            if let Ok(out) = std::env::var("AURA_LOOK_OUT") {
                let out = std::path::PathBuf::from(out);
                std::fs::create_dir_all(&out).expect("out");
                let width: u32 = tiles.iter().map(|t| t.width() + 6).sum();
                let height = tiles.iter().map(image::RgbImage::height).max().unwrap_or(1);
                let mut sheet =
                    image::RgbImage::from_pixel(width, height, image::Rgb([24, 24, 24]));
                let mut x = 0_i64;
                for tile in &tiles {
                    image::imageops::replace(&mut sheet, tile, x, 0);
                    x += i64::from(tile.width() + 6);
                }
                sheet
                    .save(out.join(format!(
                        "{}__{}.jpg",
                        row.file_name,
                        summary.name.replace('.', "_")
                    )))
                    .expect("save");
            }
        }
    }
}

/// Does a saved one-click match read back as the same recipe? `AURA_LOOK_TARGETS` (first) and
/// `AURA_LOOK_REFERENCES` (first).
#[test]
#[ignore = "needs a real photograph and reference"]
fn saved_match_round_trips() {
    let (Ok(targets), Ok(references)) = (
        std::env::var("AURA_LOOK_TARGETS"),
        std::env::var("AURA_LOOK_REFERENCES"),
    ) else {
        return;
    };
    let directory = tempfile::tempdir().expect("isolated");
    let state = AppState::open(&directory.path().join("catalog.aura"))
        .expect("catalogue")
        .with_cache_root(&directory.path().join("cache"))
        .with_key_store(std::sync::Arc::new(
            aura_cloud::keys::MemoryKeyStore::default(),
        ));
    let project = crate::create_project(
        &state,
        CreateProjectInput {
            name: "round trip".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    aura_ingest::run(
        state.catalog(),
        &aura_ingest::contract::ingest::ImportPlan {
            import_id: aura_core::ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("id"),
            roots: vec![std::path::PathBuf::from(
                targets.split(',').next().expect("target"),
            )],
            mode: aura_ingest::contract::ingest::ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    let row = crate::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id,
            offset: 0,
            limit: 1,
            order_by: None,
        },
    )
    .expect("list")
    .remove(0);
    let summary = super::super::super::import_reference(
        &state,
        &ImportInput {
            path: references.split(',').next().expect("reference").into(),
            cancel_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .expect("reference");
    let photo = PhotoId::from_db(&row.id).expect("photo");
    let hashes = || {
        let loaded = state.recipe_store().load(&photo).expect("load");
        let stored: Option<String> = state
            .catalog()
            .read(|c| {
                rusqlite::OptionalExtension::optional(c.query_row(
                    "SELECT recipe_hash FROM edit_recipes WHERE photo_id=?1",
                    [&row.id],
                    |r| r.get(0),
                ))
                .map_err(|e| aura_core::errors::db::statement_failed("read", &e))
            })
            .expect("stored");
        (
            stored,
            loaded.map(|r| aura_recipe::hash::recipe_hash(&r).expect("hash")),
        )
    };
    for (step, amount) in [1.0_f32, 1.6, 1.0, 0.5].into_iter().enumerate() {
        let result = quick_match(
            &state,
            &QuickMatchInput {
                photo_id: row.id.clone(),
                reference_id: summary.id.clone(),
                controls: Controls {
                    amount,
                    ..Controls::default()
                },
                cancel_id: uuid::Uuid::new_v4().to_string(),
                step_down: true,
                reanalyse: false,
            },
        );
        let (stored, loaded) = hashes();
        eprintln!(
            "step {step} amount {amount}: {:?} | stored {:?} loaded {:?} same {}",
            result
                .as_ref()
                .map(|r| (r.report.controls.amount, r.report.changed))
                .map_err(|e| e.message.clone()),
            stored.as_deref().map(|h| &h[..12]),
            loaded.as_deref().map(|h| &h[..12]),
            stored == loaded
        );
    }
    let loaded = state
        .recipe_store()
        .load(&photo)
        .expect("load")
        .expect("saved");
    let stored: (String, String) = state
        .catalog()
        .read(|c| {
            c.query_row(
                "SELECT recipe_hash, body FROM edit_recipes WHERE photo_id=?1",
                [&row.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("read", &e))
        })
        .expect("stored");
    let again = aura_recipe::hash::canonical(&loaded).expect("canonical");
    eprintln!(
        "stored hash {} loaded hash {}",
        stored.0,
        aura_recipe::hash::recipe_hash(&loaded).expect("hash")
    );
    if stored.1 != again {
        let (a, b) = (stored.1.as_bytes(), again.as_bytes());
        let at = a
            .iter()
            .zip(b)
            .position(|(x, y)| x != y)
            .unwrap_or(a.len().min(b.len()));
        let from = at.saturating_sub(120);
        eprintln!(
            "bodies differ at byte {at}:\n stored {}\n loaded {}",
            String::from_utf8_lossy(&a[from..(at + 120).min(a.len())]),
            String::from_utf8_lossy(&b[from..(at + 120).min(b.len())])
        );
    }
}

/// Where does a look's new clipping sit: in broad areas, or in specular sparkles that vanish
/// at viewing size? First target and first reference.
#[test]
#[ignore = "needs a real photograph and reference"]
fn clipping_by_size() {
    let (Ok(targets), Ok(references)) = (
        std::env::var("AURA_LOOK_TARGETS"),
        std::env::var("AURA_LOOK_REFERENCES"),
    ) else {
        return;
    };
    let directory = tempfile::tempdir().expect("isolated");
    let state = AppState::open(&directory.path().join("catalog.aura"))
        .expect("catalogue")
        .with_cache_root(&directory.path().join("cache"))
        .with_key_store(std::sync::Arc::new(
            aura_cloud::keys::MemoryKeyStore::default(),
        ));
    let project = crate::create_project(
        &state,
        CreateProjectInput {
            name: "clip".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    aura_ingest::run(
        state.catalog(),
        &aura_ingest::contract::ingest::ImportPlan {
            import_id: aura_core::ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("id"),
            roots: vec![std::path::PathBuf::from(
                targets.split(',').next().expect("t"),
            )],
            mode: aura_ingest::contract::ingest::ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    let row = crate::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id,
            offset: 0,
            limit: 1,
            order_by: None,
        },
    )
    .expect("list")
    .remove(0);
    let photo = PhotoId::from_db(&row.id).expect("photo");
    let summary = super::super::super::import_reference(
        &state,
        &ImportInput {
            path: references.split(',').next().expect("r").into(),
            cancel_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .expect("reference");
    let stats = transfer::recall(&state, &summary.id)
        .expect("recall")
        .expect("stats");
    let stored: Reference = read(&state, &asset_key(&summary.id).expect("key"))
        .expect("read")
        .expect("reference");
    let baseline = crate::develop_commands::neutral_recipe(&state, photo);
    let fitting = fitting_frame(&state, &photo).expect("fitting");
    let plain = rendered(&state, &fitting, &baseline).expect("plain");
    let regions = appearance::analyse(&plain).expect("regions");
    let mut fit = transfer::fit_look(
        &stats,
        &transfer::samples(&plain, transfer::TARGET_SAMPLES),
        &transfer::skin_samples(&plain, &regions),
        &transfer::palette(&stored.small_rgb),
        &LookOptions::default(),
        || Ok(()),
    )
    .expect("fit");
    transfer::review_skin(&mut fit.grade, &plain, &regions, false);
    let mut candidate = baseline.clone();
    aura_recipe::reference_grade::write(&mut candidate, &fit.grade).expect("write");
    let frame = crate::photo_frames::CatalogFrames::new(state.clone())
        .frame(&photo, aura_render::RenderLevel::Full)
        .expect("frame");
    let before = rendered(&state, &frame, &baseline).expect("before");
    let after = rendered(&state, &frame, &candidate).expect("after");
    let view = |img: &Rgb8, edge: u32| {
        let rgb = image::RgbImage::from_raw(img.width, img.height, img.data.clone()).expect("rgb");
        let small = image::imageops::thumbnail(&rgb, edge, edge);
        Rgb8 {
            width: small.width(),
            height: small.height(),
            data: small.into_raw(),
        }
    };
    for edge in [0_u32, 3000, 2048, 1024] {
        let (b, a) = if edge == 0 {
            (appearance::clipped(&before), appearance::clipped(&after))
        } else {
            (
                appearance::clipped(&view(&before, edge)),
                appearance::clipped(&view(&after, edge)),
            )
        };
        eprintln!(
            "CLIP {} <- {} at {}: {:.4} -> {:.4} (+{:.4})",
            row.file_name,
            summary.name,
            if edge == 0 {
                "full".to_owned()
            } else {
                format!("{edge}px")
            },
            b,
            a,
            a - b
        );
    }
}

/// An isolated catalogue holding these photographs.
fn workspace(
    targets: &str,
) -> (
    tempfile::TempDir,
    AppState,
    Vec<crate::contract::ipc::ImageRowLite>,
) {
    let directory = tempfile::tempdir().expect("isolated");
    let state = AppState::open(&directory.path().join("catalog.aura"))
        .expect("catalogue")
        .with_cache_root(&directory.path().join("cache"))
        .with_key_store(std::sync::Arc::new(
            aura_cloud::keys::MemoryKeyStore::default(),
        ));
    let project = crate::create_project(
        &state,
        CreateProjectInput {
            name: "look".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    aura_ingest::run(
        state.catalog(),
        &aura_ingest::contract::ingest::ImportPlan {
            import_id: aura_core::ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("id"),
            roots: targets.split(',').map(std::path::PathBuf::from).collect(),
            mode: aura_ingest::contract::ingest::ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    let rows = crate::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id,
            offset: 0,
            limit: 100,
            order_by: None,
        },
    )
    .expect("list");
    (directory, state, rows)
}

/// Every skin region's look-guard readings after the full look: which of the four limits a
/// match runs into, for whom. Same environment variables as [`look_sweep`].
#[test]
#[ignore = "needs real photographs and references"]
fn skin_report() {
    let (Ok(targets), Ok(references)) = (
        std::env::var("AURA_LOOK_TARGETS"),
        std::env::var("AURA_LOOK_REFERENCES"),
    ) else {
        return;
    };
    let (_dir, state, rows) = workspace(&targets);
    let turn = |p: aura_core::contract::look::ZoneTint, q: aura_core::contract::look::ZoneTint| {
        if p.a.hypot(p.b) < 3.0 || q.a.hypot(q.b) < 3.0 {
            return 0.0;
        }
        let d = q.b.atan2(q.a) - p.b.atan2(p.a);
        ((d + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI)
            .abs()
            .to_degrees()
    };
    for reference_path in references.split(',') {
        let summary = super::super::super::import_reference(
            &state,
            &ImportInput {
                path: reference_path.into(),
                cancel_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .expect("reference");
        let stats = transfer::recall(&state, &summary.id)
            .expect("recall")
            .expect("stats");
        let stored: Reference = read(&state, &asset_key(&summary.id).expect("key"))
            .expect("read")
            .expect("reference");
        for row in &rows {
            let photo = PhotoId::from_db(&row.id).expect("photo");
            let baseline = crate::develop_commands::neutral_recipe(&state, photo);
            let fitting = fitting_frame(&state, &photo).expect("fitting");
            let plain = rendered(&state, &fitting, &baseline).expect("plain");
            let regions = appearance::analyse(&plain).expect("regions");
            let mut fit = transfer::fit_look(
                &stats,
                &transfer::samples(&plain, transfer::TARGET_SAMPLES),
                &transfer::skin_samples(&plain, &regions),
                &transfer::palette(&stored.small_rgb),
                &LookOptions::default(),
                || Ok(()),
            )
            .expect("fit");
            transfer::review_skin(&mut fit.grade, &plain, &regions, false);
            transfer::settle(
                &mut fit.grade,
                &transfer::samples(&plain, transfer::TARGET_SAMPLES),
                &plain,
                &regions,
            );
            let mut candidate = baseline.clone();
            aura_recipe::reference_grade::write(&mut candidate, &fit.grade).expect("write");
            let frame = crate::photo_frames::CatalogFrames::new(state.clone())
                .frame(&photo, aura_render::RenderLevel::Full)
                .expect("frame");
            let before = appearance::readings(
                &rendered(&state, &frame, &baseline).expect("before"),
                &regions,
            );
            let after = appearance::readings(
                &rendered(&state, &frame, &candidate).expect("after"),
                &regions,
            );
            eprintln!(
                "SKIN {} <- {}: guard {} skin protection {:.2}",
                row.file_name,
                summary.name,
                appearance::skin_natural(&before, &after),
                fit.grade.skin
            );
            for a in before.iter().filter(|r| r.name.ends_with("skin")) {
                let Some(b) = after.iter().find(|b| b.name == a.name) else {
                    continue;
                };
                let (x, y) = (&a.appearance, &b.appearance);
                let ratio = if x.chroma_p50 > 1e-4 {
                    y.chroma_p50 / x.chroma_p50
                } else {
                    1.0
                };
                let tone = y.tone.p50 - x.tone.p50;
                let (mid, high) = (turn(x.mid, y.mid), turn(x.high, y.high));
                let flag = |bad: bool| if bad { "X" } else { " " };
                eprintln!(
                    "    {:28} tone {:+.3}{} chroma x{:.2}{} mid turn {:5.1}{} high turn {:5.1}{} (mid {:.1},{:.1} -> {:.1},{:.1})",
                    a.name,
                    tone,
                    flag(tone.abs() > 0.06),
                    ratio,
                    flag(!(0.7..=1.45).contains(&ratio)),
                    mid,
                    flag(mid > 15.0),
                    high,
                    flag(high > 20.0),
                    x.mid.a,
                    x.mid.b,
                    y.mid.a,
                    y.mid.b
                );
            }
        }
    }
}
