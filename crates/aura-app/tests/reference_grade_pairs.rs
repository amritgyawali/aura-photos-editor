//! Prepares real before/after pairs for the reference-grade evaluation (ADR-0160), run by hand.
//!
//! `AURA_GRADE_RAWS` names a folder holding `dng/NAME.dng` camera files and `expert_c/NAME.png`
//! finished versions (the MIT-Adobe FiveK layout, research licence, never committed).
//! `AURA_GRADE_OUT` receives `NAME-before.png` - AURA's neutral render of the camera file at the
//! finished version's size - and `NAME-after.png`, a copy of the finished version. The transfer
//! evaluation in `color_match::transfer::tests` reads that folder. Nothing here is a claim about
//! quality; it writes the inputs a measurement needs.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]
use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use std::path::Path;
use std::sync::Arc;

fn base64_decode(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for v in text.bytes().filter_map(value) {
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

fn neutral_render(raw: &Path) -> image::RgbImage {
    let dir = tempfile::tempdir().expect("isolated catalog directory");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("open isolated catalog")
        .with_cache_root(&dir.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Reference grade pairs".into(),
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
            roots: vec![raw.to_path_buf()],
            mode: ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import camera file");
    let photos = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 4,
            order_by: None,
        },
    )
    .expect("list");
    let photo = PhotoId::from_db(&photos.first().expect("one photograph").id).expect("id");
    let recipe = aura_app::develop_commands::neutral_recipe(&state, photo);
    state
        .recipe_store()
        .save(&project_id, &photo, &recipe, &[], "Neutral")
        .expect("neutral recipe");
    let render = aura_app::native_retouch::preview(&state, &project.id, &photo.to_db(), false)
        .expect("render");
    image::RgbImage::from_raw(
        render.width,
        render.height,
        base64_decode(&render.rgb_base64),
    )
    .expect("packed RGB")
}

#[test]
#[ignore = "needs AURA_GRADE_RAWS and AURA_GRADE_OUT"]
fn prepare_real_reference_grade_pairs() {
    let raws = std::env::var("AURA_GRADE_RAWS").expect("AURA_GRADE_RAWS");
    let out = std::env::var("AURA_GRADE_OUT").expect("AURA_GRADE_OUT");
    let (raws, out) = (Path::new(&raws), Path::new(&out));
    std::fs::create_dir_all(out).expect("output folder");
    // The FiveK layout has `dng/` and `expert_c/`; a folder of camera files with no finished
    // versions writes the neutral render alone, at a 1024-pixel long edge.
    let paired = raws.join("dng").is_dir();
    let source = if paired {
        raws.join("dng")
    } else {
        raws.to_path_buf()
    };
    let limit: usize = std::env::var("AURA_GRADE_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);
    let mut names: Vec<_> = std::fs::read_dir(&source)
        .expect("camera folder")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| {
                ["dng", "cr3", "cr2", "nef", "arw"]
                    .iter()
                    .any(|x| e.eq_ignore_ascii_case(x))
            })
        })
        .collect();
    names.sort();
    names.truncate(limit);
    for raw in names {
        let stem = raw.file_stem().unwrap().to_string_lossy().into_owned();
        let before_path = out.join(format!("{stem}-before.png"));
        if before_path.exists() {
            continue;
        }
        if !paired {
            let Ok(render) = std::panic::catch_unwind(|| neutral_render(&raw)) else {
                println!("{stem}: could not be rendered, skipped");
                continue;
            };
            let (w, h) = render.dimensions();
            let scale = 1024.0 / f64::from(w.max(h));
            let small = image::imageops::resize(
                &render,
                (f64::from(w) * scale).round() as u32,
                (f64::from(h) * scale).round() as u32,
                image::imageops::FilterType::Triangle,
            );
            small.save(&before_path).expect("write before");
            println!("{stem}: {}x{}", small.width(), small.height());
            continue;
        }
        let finished = raws.join("expert_c").join(format!("{stem}.png"));
        if !finished.exists() {
            continue;
        }
        let after = image::open(&finished).expect("finished version").to_rgb8();
        let Ok(before) = std::panic::catch_unwind(|| neutral_render(&raw)) else {
            println!("{stem}: could not be rendered, skipped");
            continue;
        };
        let aspect = |w: u32, h: u32| f64::from(w) / f64::from(h);
        if (aspect(before.width(), before.height()) - aspect(after.width(), after.height())).abs()
            > 0.02
        {
            println!("{stem}: framing differs, skipped");
            continue;
        }
        let before = image::imageops::resize(
            &before,
            after.width(),
            after.height(),
            image::imageops::FilterType::Triangle,
        );
        before.save(&before_path).expect("write before");
        after
            .save(out.join(format!("{stem}-after.png")))
            .expect("write after");
        println!("{stem}: {}x{}", after.width(), after.height());
    }
}
