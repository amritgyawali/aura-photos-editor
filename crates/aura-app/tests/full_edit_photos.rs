//! The full automatic edit on real photographs, stage by stage, for checking by eye. ADR-0187.
//!
//! Ignored. `AURA_FULL_EDIT_PHOTOS` names a folder; its photographs are imported into a
//! throwaway catalog and each is given the full automatic edit exactly as the window gives it.
//! Every stage's outcome and first change is printed with the time each photograph took.
//! `AURA_FULL_EDIT_LIMIT` edits only the first N in shooting order; `AURA_FULL_EDIT_OUT` writes
//! `<file>-before-after.jpg`, the original and the edit side by side at 1200 px.
//! `AURA_FULL_EDIT_PICTURES=1` edits as a watched window does, with a picture after every step.
//!
//! ```text
//! AURA_FULL_EDIT_PHOTOS="G:/to do edit full by AURA" AURA_FULL_EDIT_LIMIT=6 \
//!   cargo test -p aura-app --release --test full_edit_photos -- --ignored --nocapture
//! ```
use std::sync::Arc;
use std::time::Instant;

use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::full_edit::{self, FullEditInput, Look};
use aura_app::{create_project, AppState};
use aura_cloud::keys::MemoryKeyStore;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_render::RenderService;

fn render(
    state: &AppState,
    photo: PhotoId,
    recipe: aura_recipe::Recipe,
) -> Option<image::RgbImage> {
    let result = state
        .render()
        .ok()?
        .render(aura_render::RenderRequest {
            image_id: photo,
            recipe,
            level: aura_render::RenderLevel::Screen(1200, 1200),
            purpose: aura_render::RenderPurpose::Export,
            output: aura_render::OutputSpec {
                colour_space: aura_render::OutputColour::Srgb,
                bit_depth: 8,
                icc: None,
            },
        })
        .ok()?;
    let bytes = match &result.data {
        aura_render::RenderedData::Eight(v) => v.clone(),
        aura_render::RenderedData::Sixteen(v) => v.iter().map(|x| (x >> 8) as u8).collect(),
    };
    image::RgbImage::from_raw(result.width, result.height, bytes)
}

#[test]
#[ignore = "needs AURA_FULL_EDIT_PHOTOS, a folder of real photographs"]
fn full_edit_real_photographs() {
    let Some(folder) = std::env::var_os("AURA_FULL_EDIT_PHOTOS") else {
        eprintln!("AURA_FULL_EDIT_PHOTOS is not set");
        return;
    };
    let limit: usize = std::env::var("AURA_FULL_EDIT_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);
    let out = std::env::var_os("AURA_FULL_EDIT_OUT").map(std::path::PathBuf::from);
    if let Some(out) = &out {
        std::fs::create_dir_all(out).expect("out");
    }
    let dir = tempfile::tempdir().expect("temp");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("state")
        .with_licence_dir(dir.path().join("licence"))
        .with_key_store(Arc::new(MemoryKeyStore::default()));
    let project = create_project(
        &state,
        CreateProjectInput {
            name: "Full edit".into(),
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
            roots: vec![folder.into()],
            mode: ImportMode::Reference,
            extensions: vec![],
            extract_embedded_previews: true,
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
            order_by: Some("timeline".into()),
        },
    )
    .expect("photos");
    let mut times = Vec::new();
    let mut tally = std::collections::BTreeMap::<String, [usize; 4]>::new();
    for photo in photos.iter().take(limit) {
        let id = PhotoId::from_db(&photo.id).expect("id");
        let before = aura_app::develop_commands::load_or_neutral(&state, id).expect("neutral");
        let started = Instant::now();
        let result = full_edit::run(
            &state,
            &FullEditInput {
                project_id: project.id.clone(),
                photo_id: photo.id.clone(),
                look: Look::None,
                // `AURA_FULL_EDIT_PICTURES=1` times the pictures a watched edit sends.
                preview: std::env::var_os("AURA_FULL_EDIT_PICTURES").is_some_and(|v| v == "1"),
            },
            &|_| {},
        );
        let seconds = started.elapsed().as_secs_f32();
        let edited = match result {
            Ok(edited) => edited,
            Err(error) => {
                eprintln!(
                    "{}: FAILED {} {}",
                    photo.file_name, error.code, error.message
                );
                continue;
            }
        };
        times.push(seconds);
        let mut spent = std::collections::BTreeMap::<&str, u64>::new();
        for (label, ms) in aura_app::perf::take()
            .into_iter()
            .chain(aura_render::perf::take())
        {
            *spent.entry(label).or_default() += ms;
        }
        if !spent.is_empty() {
            let mut rows: Vec<_> = spent.into_iter().collect();
            rows.sort_by_key(|row| std::cmp::Reverse(row.1));
            eprintln!(
                "  time: {}",
                rows.iter()
                    .take(14)
                    .map(|(l, ms)| format!("{l} {:.1}s", *ms as f32 / 1000.0))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        eprintln!(
            "\n== {} · {seconds:.1}s · {} face(s) · QC {}",
            photo.file_name,
            edited.faces,
            if edited.quality_passed {
                "passed"
            } else {
                "flagged"
            }
        );
        for stage in &edited.stages {
            let row = tally.entry(stage.title.clone()).or_default();
            let slot = match full_edit::outcome_label(stage.outcome) {
                "applied" => 0,
                "nothing needed" => 1,
                "does not apply" => 2,
                _ => 3,
            };
            row[slot] += 1;
            eprintln!(
                "  {:>2} {:<18} {:<15} {}",
                stage.number,
                stage.title,
                full_edit::outcome_label(stage.outcome),
                stage
                    .changes
                    .iter()
                    .take(2)
                    .chain(
                        stage
                            .checks
                            .iter()
                            .take(usize::from(stage.changes.is_empty()))
                    )
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
        }
        if std::env::var_os("AURA_FULL_EDIT_REPEAT").is_some() {
            let first = aura_app::develop_commands::load_or_neutral(&state, id).expect("first");
            full_edit::run(
                &state,
                &FullEditInput {
                    project_id: project.id.clone(),
                    photo_id: photo.id.clone(),
                    look: Look::None,
                    preview: false,
                },
                &|_| {},
            )
            .expect("again");
            let second = aura_app::develop_commands::load_or_neutral(&state, id).expect("second");
            let flat = |r: &aura_recipe::Recipe| {
                let mut v = serde_json::to_value(r).expect("json");
                if let Some(o) = v.as_object_mut() {
                    for k in ["provenance", "full_edit_v1", "studio_advanced_retouch_v1"] {
                        o.remove(k);
                    }
                }
                let mut out = std::collections::BTreeMap::new();
                fn walk(
                    p: String,
                    v: &serde_json::Value,
                    out: &mut std::collections::BTreeMap<String, String>,
                ) {
                    match v {
                        serde_json::Value::Object(m) => {
                            m.iter().for_each(|(k, x)| walk(format!("{p}/{k}"), x, out))
                        }
                        serde_json::Value::Array(a) => a
                            .iter()
                            .enumerate()
                            .for_each(|(i, x)| walk(format!("{p}/{i}"), x, out)),
                        other => {
                            out.insert(p, other.to_string().chars().take(60).collect());
                        }
                    }
                }
                walk(String::new(), &v, &mut out);
                out
            };
            let (a, b) = (flat(&first), flat(&second));
            let diff: Vec<_> = a
                .keys()
                .chain(b.keys())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .filter(|k| a.get(*k) != b.get(*k))
                .map(|k| format!("{k}: {:?} -> {:?}", a.get(k), b.get(k)))
                .collect();
            eprintln!("  repeat: {} parameter(s) changed", diff.len());
            for d in diff.iter().take(25) {
                eprintln!("    {d}");
            }
        }
        let Some(out) = &out else { continue };
        let after = aura_app::develop_commands::load_or_neutral(&state, id).expect("edited");
        if let (Some(a), Some(b)) = (render(&state, id, before), render(&state, id, after)) {
            let (w, h) = (a.width() + b.width() + 12, a.height().max(b.height()));
            let mut sheet = image::RgbImage::from_pixel(w, h, image::Rgb([20, 20, 20]));
            image::imageops::overlay(&mut sheet, &a, 0, 0);
            image::imageops::overlay(&mut sheet, &b, i64::from(a.width() + 12), 0);
            sheet
                .save(out.join(format!("{}-before-after.jpg", photo.file_name)))
                .expect("sheet");
        }
    }
    if !times.is_empty() {
        let total: f32 = times.iter().sum();
        eprintln!(
            "\n{} photographs in {total:.0}s, {:.1}s a photograph (slowest {:.1}s).",
            times.len(),
            total / times.len() as f32,
            times.iter().copied().fold(0.0, f32::max)
        );
        eprintln!(
            "{:<20} applied / nothing needed / does not apply / kept",
            "stage"
        );
        for (title, [a, b, c, d]) in &tally {
            eprintln!("{title:<20} {a:>4} {b:>4} {c:>4} {d:>4}");
        }
    }
}
