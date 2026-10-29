//! The photo pass (ADR-0186) on real photographs, through the application's own commands.
//!
//! Ignored by default. Point it at one or more folders (separated by `;`) and an output folder:
//!
//! ```text
//! AURA_PASS_PHOTOS="H:/AURA EDIT" AURA_PASS_OUT=I:/aura-hundred-report \
//!   cargo test --release -p aura-app --test photo_pass_photos -- --ignored --nocapture
//! ```
//!
//! Every photograph is imported into an isolated catalogue (by reference: nothing in the source
//! folder is copied or changed), given Auto enhance as an import gives it, rendered, put through
//! the photo pass, and rendered again. For every photograph the pass changed, `pairs/` gets the
//! frame before and after the pass with the difference x8 beside them, and a close crop of each
//! local operation. `report.json` has every step's outcome and words for every photograph, and
//! the console prints how often each step applied. `AURA_PASS_LIMIT` caps the photographs,
//! `AURA_PASS_TEMP` is where the catalogue lives (default `I:/aura-hundred-tmp`).
//! `AURA_PASS_EACH=1` applies every applied step on its own first and writes its pair as
//! `<photo>-sNN-<step>-frame.png`, so each difference is that step's alone.
#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use aura_app::contract::ipc::{
    CreateProjectInput, DevelopImageInput, ListImagesInput, RenderImageInput,
};
use aura_app::photo_pass::{self, Outcome, PhotoPassInput};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use serde_json::json;

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

fn render(state: &AppState, photo: &str) -> (Vec<u8>, usize, usize) {
    let out = aura_app::render_image(
        state,
        &RenderImageInput {
            photo_id: photo.into(),
            level: Some("screen".into()),
            screen: Some((1600, 1600)),
            colour_space: None,
            purpose: Some("export".into()),
        },
    )
    .expect("render");
    (
        decode(&out.rgb_base64),
        out.width as usize,
        out.height as usize,
    )
}

/// Before, after and the difference x8, side by side, scaled to at most 1800 px wide.
fn write_pair(path: &Path, before: &[u8], after: &[u8], w: usize, h: usize, b: [usize; 4]) {
    let [x0, y0, x1, y1] = b;
    let (cw, ch) = (x1 - x0, y1 - y0);
    if cw < 4 || ch < 4 || before.len() != after.len() || before.len() != w * h * 3 {
        return;
    }
    let gap = 6;
    let out_w = cw * 3 + gap * 2;
    let mut out = vec![255_u8; out_w * ch * 3];
    for y in 0..ch {
        for x in 0..cw {
            let src = ((y0 + y) * w + x0 + x) * 3;
            let row = y * out_w;
            out[(row + x) * 3..(row + x) * 3 + 3].copy_from_slice(&before[src..src + 3]);
            let dst = (row + cw + gap + x) * 3;
            out[dst..dst + 3].copy_from_slice(&after[src..src + 3]);
            let diff = (row + 2 * (cw + gap) + x) * 3;
            for c in 0..3 {
                out[diff + c] =
                    (u32::from(before[src + c].abs_diff(after[src + c])) * 8).min(255) as u8;
            }
        }
    }
    let img = image::RgbImage::from_raw(out_w as u32, ch as u32, out).expect("image");
    let scale = (1800.0 / out_w as f32).min(1.0);
    let img = if scale < 0.99 {
        image::imageops::resize(
            &img,
            (out_w as f32 * scale) as u32,
            (ch as f32 * scale).max(1.0) as u32,
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };
    img.save(path).expect("pair");
}

#[test]
#[ignore = "needs AURA_PASS_PHOTOS and AURA_PASS_OUT"]
fn the_photo_pass_on_real_photographs() {
    let Ok(sources) = std::env::var("AURA_PASS_PHOTOS") else {
        println!("AURA_PASS_PHOTOS is not set");
        return;
    };
    let out = PathBuf::from(std::env::var("AURA_PASS_OUT").expect("AURA_PASS_OUT"));
    std::fs::create_dir_all(out.join("pairs")).expect("out");
    let limit: usize = std::env::var("AURA_PASS_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);
    let each = std::env::var_os("AURA_PASS_EACH").is_some();
    let temp = std::env::var("AURA_PASS_TEMP").unwrap_or_else(|_| "I:/aura-hundred-tmp".into());
    std::fs::create_dir_all(&temp).expect("temp");
    let dir = tempfile::tempdir_in(temp).expect("temp");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("state")
        .with_cache_root(&dir.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Photo pass".into(),
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
            roots: sources.split(';').map(PathBuf::from).collect(),
            mode: ImportMode::Reference,
            extensions: vec![],
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    let photos = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 1000,
            order_by: None,
        },
    )
    .expect("list");
    let mut tally: BTreeMap<(u8, String), [usize; 4]> = BTreeMap::new();
    let mut rows = Vec::new();
    let mut pass_ms = Vec::new();
    for photo in photos.iter().take(limit) {
        let name = photo.file_name.clone();
        let started = Instant::now();
        if let Err(e) = aura_app::photo_enhance::enhance_photo(
            &state,
            &DevelopImageInput {
                photo_id: photo.id.clone(),
            },
        ) {
            println!("{name}: Auto enhance failed: {e:?}");
            continue;
        }
        let enhanced = started.elapsed().as_millis();
        let (before, w, h) = render(&state, &photo.id);
        let input = PhotoPassInput {
            project_id: project.id.clone(),
            photo_id: photo.id.clone(),
        };
        let stem = Path::new(&name)
            .file_stem()
            .map_or_else(|| name.clone(), |s| s.to_string_lossy().into_owned());
        if each {
            // Every applied step on its own, so a pair shows that step and nothing else.
            let (base, planned) = photo_pass::plan_photo(&state, &input).expect("plan");
            let pid = PhotoId::from_db(&photo.id).expect("photo");
            let project_id = ProjectId::from_db(&project.id).expect("project");
            for (def, finding) in planned
                .iter()
                .filter(|(_, f)| f.outcome == Outcome::Applied)
            {
                let (saves, _) =
                    photo_pass::merge_all(&base, vec![(*def, finding.clone())], |_, _, _| {})
                        .expect("merge");
                photo_pass::save_all(&state, &input, &saves).expect("save");
                let (after, aw, ah) = render(&state, &photo.id);
                let tag = format!("{stem}-s{:02}-{}", def.number, def.id);
                if (aw, ah) == (w, h) {
                    write_pair(
                        &out.join("pairs").join(format!("{tag}-frame.png")),
                        &before,
                        &after,
                        w,
                        h,
                        [0, 0, w, h],
                    );
                    for e in &finding.ops {
                        if e.region[2] >= 0.9 {
                            continue;
                        }
                        let [cx, cy, rx, ry] = e.region;
                        let pad = 40;
                        let bx = [
                            (((cx - rx * 1.3).max(0.0) * w as f32) as usize).saturating_sub(pad),
                            (((cy - ry * 1.3).max(0.0) * h as f32) as usize).saturating_sub(pad),
                            ((((cx + rx * 1.3).min(1.0) * w as f32) as usize) + pad).min(w),
                            ((((cy + ry * 1.3).min(1.0) * h as f32) as usize) + pad).min(h),
                        ];
                        write_pair(
                            &out.join("pairs").join(format!("{tag}-{}.png", e.id)),
                            &before,
                            &after,
                            w,
                            h,
                            bx,
                        );
                    }
                }
                state
                    .recipe_store()
                    .save(
                        &project_id,
                        &pid,
                        &base,
                        &["global".into()],
                        "harness: back to before the step",
                    )
                    .expect("restore");
            }
        }
        let started = Instant::now();
        let result = match photo_pass::run(&state, &input) {
            Ok(r) => r,
            Err(e) => {
                println!("{name}: photo pass failed: {e:?}");
                continue;
            }
        };
        let ms = started.elapsed().as_millis();
        pass_ms.push(ms);
        let report = result.report;
        if aura_app::perf::enabled() {
            let mut steps: BTreeMap<&str, u64> = BTreeMap::new();
            for (label, ms) in aura_app::perf::take() {
                if label.starts_with("pass.") {
                    *steps.entry(label).or_default() += ms;
                }
            }
            println!("  timing: {steps:?}");
        }
        println!(
            "\n=== {name}: {} (enhance {enhanced} ms, pass {ms} ms)",
            report.summary
        );
        for s in &report.steps {
            let slot = match s.outcome {
                Outcome::Applied => 0,
                Outcome::Unchanged => 1,
                Outcome::NotApplicable => 2,
                Outcome::Protected => 3,
            };
            tally.entry((s.number, s.title.clone())).or_default()[slot] += 1;
            let mark = match s.outcome {
                Outcome::Applied => "APPLIED",
                Outcome::Unchanged => "checked",
                Outcome::NotApplicable => "n/a",
                Outcome::Protected => "kept",
            };
            println!(
                "  {:>2}. {:<30} {:<8} {}",
                s.number,
                s.title,
                mark,
                [s.checks.join(" "), s.changes.join(" ")].join(" ").trim()
            );
        }
        if report.applied > 0 && !each {
            let (after, aw, ah) = render(&state, &photo.id);
            if (aw, ah) == (w, h) {
                write_pair(
                    &out.join("pairs").join(format!("{stem}-frame.png")),
                    &before,
                    &after,
                    w,
                    h,
                    [0, 0, w, h],
                );
                // A close look at every local operation the pass placed.
                let pid = PhotoId::from_db(&photo.id).expect("photo");
                let recipe =
                    aura_app::develop_commands::load_or_neutral(&state, pid).expect("recipe");
                for e in aura_recipe::retouch_tools::read(&recipe).expect("stack") {
                    if photo_pass::step_of(&e.id).is_none() || e.region[2] >= 0.9 {
                        continue;
                    }
                    let [cx, cy, rx, ry] = e.region;
                    let bx = [
                        ((cx - rx * 1.3).max(0.0) * w as f32) as usize,
                        ((cy - ry * 1.3).max(0.0) * h as f32) as usize,
                        (((cx + rx * 1.3).min(1.0) * w as f32) as usize).min(w),
                        (((cy + ry * 1.3).min(1.0) * h as f32) as usize).min(h),
                    ];
                    // Tiny regions get some context.
                    let pad = 40;
                    let bx = [
                        bx[0].saturating_sub(pad),
                        bx[1].saturating_sub(pad),
                        (bx[2] + pad).min(w),
                        (bx[3] + pad).min(h),
                    ];
                    write_pair(
                        &out.join("pairs").join(format!("{stem}-{}.png", e.id)),
                        &before,
                        &after,
                        w,
                        h,
                        bx,
                    );
                }
            } else {
                println!(
                    "  (the pass changed the frame size: {w}x{h} -> {aw}x{ah}; no pair written)"
                );
            }
        }
        rows.push(json!({ "photo": name, "passMs": ms, "report": report }));
    }
    println!(
        "\n=== Steps over {} photographs: applied / checked / n/a / kept",
        rows.len()
    );
    for ((n, title), c) in &tally {
        println!(
            "  {n:>2}. {title:<30} {:>3} {:>3} {:>3} {:>3}",
            c[0], c[1], c[2], c[3]
        );
    }
    pass_ms.sort_unstable();
    if let Some(mid) = pass_ms.get(pass_ms.len() / 2) {
        println!(
            "Pass time: median {mid} ms, slowest {} ms",
            pass_ms.last().unwrap_or(mid)
        );
    }
    std::fs::write(
        out.join("report.json"),
        serde_json::to_string_pretty(&rows).expect("json"),
    )
    .expect("report");
}
