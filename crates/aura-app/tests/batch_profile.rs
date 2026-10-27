//! Where a folder run spends its time, measured in-process on a folder of photographs.
//!
//! Runs the same calls "Finish a whole folder" makes - import, analysis, Auto edit with
//! retouch, verified export with the gallery preset - one photograph at a time, with
//! `AURA_PERF=1` timing every named step, and prints one table. Run by hand:
//!
//! ```text
//! AURA_PERF=1 AURA_PROFILE_DIR=<folder> cargo test --release -p aura-app --test batch_profile -- --ignored --nocapture
//! ```
//!
//! `AURA_PROFILE_EXPORT=<folder>` keeps the exported files there instead of in a temporary
//! folder, so they can be looked at; a photograph whose analysis or edit fails is counted
//! and named rather than skipped silently.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::print_stdout
)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use aura_app::contract::ipc::*;
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};

fn record(table: &mut BTreeMap<String, (u64, u128)>, label: &str, ms: u128) {
    let entry = table.entry(label.to_owned()).or_insert((0, 0));
    entry.0 += 1;
    entry.1 += ms;
}

#[test]
#[ignore = "needs AURA_PROFILE_DIR, a folder of photographs"]
fn where_the_time_goes() {
    let folder =
        std::path::PathBuf::from(std::env::var("AURA_PROFILE_DIR").expect("AURA_PROFILE_DIR"));
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .unwrap()
        .with_licence_dir(dir.path().join("licence"))
        .with_cache_root(&dir.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "profile".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .unwrap();
    let mut table: BTreeMap<String, (u64, u128)> = BTreeMap::new();
    let started = Instant::now();
    aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).unwrap(),
            roots: vec![folder],
            mode: ImportMode::Reference,
            extensions: vec![],
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .unwrap();
    record(
        &mut table,
        "1 import (whole folder)",
        started.elapsed().as_millis(),
    );
    let photos = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 10_000,
            order_by: Some("timeline".into()),
        },
    )
    .unwrap();
    let n = photos.len();
    // As many photographs at once as the image gate allows, as a folder run does.
    let lanes = aura_render::WorkGate::for_this_machine().permits().max(1);
    let each = |work: &(dyn Fn(&ImageRowLite) + Sync)| {
        let next = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..lanes {
                scope.spawn(|| loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(p) = photos.get(i) else { break };
                    work(p);
                });
            }
        });
    };
    let failures = std::sync::Mutex::new(Vec::<String>::new());
    let t = Instant::now();
    each(&|p| {
        if let Err(err) = aura_app::photo_analysis(
            &state,
            &PhotoAutoEditInput {
                project_id: project.id.clone(),
                photo_id: p.id.clone(),
                job_id: "profile".into(),
            },
        ) {
            failures
                .lock()
                .unwrap()
                .push(format!("analysis {}: {err:?}", p.file_name));
        }
    });
    record(
        &mut table,
        "2 analysis (whole folder)",
        t.elapsed().as_millis(),
    );
    let t = Instant::now();
    each(&|p| {
        if let Err(err) = aura_app::photo_enhance::enhance_photo(
            &state,
            &DevelopImageInput {
                photo_id: p.id.clone(),
            },
        ) {
            failures
                .lock()
                .unwrap()
                .push(format!("edit {}: {err:?}", p.file_name));
        }
    });
    record(
        &mut table,
        "3 edit + retouch (whole folder)",
        t.elapsed().as_millis(),
    );
    let presets = aura_app::export_presets().unwrap();
    let preset = presets.iter().find(|p| p.name == "gallery").unwrap();
    let mut set = serde_json::to_value(preset).unwrap();
    set["imageIds"] =
        serde_json::to_value(photos.iter().map(|p| p.id.clone()).collect::<Vec<_>>()).unwrap();
    let t = Instant::now();
    aura_app::export_run(
        &state,
        ExportJobInput {
            project_id: project.id.clone(),
            sets: vec![serde_json::from_value(set).unwrap()],
            destination: std::env::var("AURA_PROFILE_EXPORT")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| dir.path().join("export"))
                .to_string_lossy()
                .into_owned(),
            destination_kind: "folder".into(),
            copyright: None,
            contact: None,
            creator: None,
            keywords: vec![],
            strip_gps: true,
            strip_camera_serial: true,
            verify: true,
        },
    )
    .unwrap();
    record(
        &mut table,
        "4 export (whole folder)",
        t.elapsed().as_millis(),
    );
    for (label, ms) in aura_app::perf::take() {
        record(&mut table, &format!("  step {label}"), u128::from(ms));
    }
    for (label, ms) in aura_render::perf::take() {
        record(&mut table, &format!("    {label}"), u128::from(ms));
    }
    let failures = failures.into_inner().unwrap();
    for failure in &failures {
        println!("FAILED {failure}");
    }
    let total = started.elapsed().as_secs_f64();
    println!(
        "\n{n} photographs, {:.1} s in all, {:.2} s a photograph\n",
        total,
        total / n.max(1) as f64
    );
    println!(
        "{:<40} {:>6} {:>10} {:>10}",
        "step", "count", "total s", "mean ms"
    );
    for (label, (count, ms)) in &table {
        println!(
            "{label:<40} {count:>6} {:>10.1} {:>10.0}",
            *ms as f64 / 1000.0,
            *ms as f64 / (*count).max(1) as f64
        );
    }
}
