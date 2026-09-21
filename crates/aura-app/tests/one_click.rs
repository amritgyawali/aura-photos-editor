//! The one-click finish, end to end, with no network and no clicks (ADR-0065).
//!
//! Two JPEGs go in; delivered files come out, written and verified, with a
//! sealed manifest. Every stage runs offline: the analysis pass is refused by
//! preflight and the pipeline must *note* that and continue; the provider check
//! refuses and every frame must degrade to the local reference grade; the cull
//! finds no analysis and must fall back to delivering every frame rather than
//! dying. That is the whole promise of the design: the button delivers, and the
//! notes say what could not happen on the way.
use std::sync::Arc;
use std::time::{Duration, Instant};

use aura_app::contract::ipc::{CreateProjectInput, OneClickFinishInput};
use aura_app::{
    create_project, export_manifest, one_click_cancel, one_click_finish, one_click_status, AppState,
};
use aura_cloud::keys::MemoryKeyStore;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_raw::codec::{encode_jpeg, Rgb8};

fn gradient(seed: u8) -> Vec<u8> {
    (0..80 * 60u32)
        .flat_map(|i| {
            let base = ((i % 200) as u8).saturating_add(seed);
            [base, base / 2, 255 - base]
        })
        .collect()
}

#[test]
fn one_press_delivers_files_with_the_refusals_it_met_written_down() {
    let dir = tempfile::tempdir().expect("temp");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("state")
        .with_key_store(Arc::new(MemoryKeyStore::default()));

    let project = create_project(
        &state,
        CreateProjectInput {
            name: "One click".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");

    // Two originals on disk, imported synchronously so the run needs no ingest wait.
    for (index, seed) in [20u8, 140].into_iter().enumerate() {
        let rgb = Rgb8 {
            width: 80,
            height: 60,
            data: gradient(seed),
        };
        let bytes = encode_jpeg(&rgb, 92).expect("jpeg");
        std::fs::write(dir.path().join(format!("frame-{index}.jpg")), bytes).expect("write");
    }
    let report = aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("id"),
            roots: vec![dir.path().to_path_buf()],
            mode: ImportMode::Reference,
            extensions: vec![],
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    assert_eq!(report.files_imported, 2, "the fixture is two frames");

    let destination = dir.path().join("out");
    let handle = one_click_finish(
        &state,
        OneClickFinishInput {
            project_id: project.id.clone(),
            destination: destination.display().to_string(),
            ingest_job_id: None,
        },
    )
    .expect("start");

    // Poll the row until it stops moving, with a generous ceiling: a debug build
    // renders two small proxies, and every stage that could fail is bounded.
    let deadline = Instant::now() + Duration::from_secs(600);
    let final_row = loop {
        let row = one_click_status(&handle.job_id).expect("status");
        if row.status != "running" {
            break row;
        }
        assert!(
            Instant::now() < deadline,
            "the pipeline outlived the test: {:?}",
            row
        );
        std::thread::sleep(Duration::from_millis(250));
    };

    // The honest outcome: whatever degraded, the delivery did not.
    assert_eq!(
        final_row.status, "completed",
        "the run ended {} with notes {:?}",
        final_row.status, final_row.notes
    );
    assert_eq!(final_row.frames, 2, "both frames are in the run");
    assert!(
        final_row.written >= 2,
        "every frame must reach the disk: written={} notes={:?}",
        final_row.written,
        final_row.notes,
    );
    assert_eq!(
        final_row.verified, final_row.written,
        "a file that does not read back is not delivered"
    );
    assert!(
        final_row
            .notes
            .iter()
            .any(|line| line.contains("local reference") || line.contains("no working provider")),
        "the offline pipeline must say the provider did not answer: {:?}",
        final_row.notes,
    );

    let manifest = export_manifest(&state, &project.id)
        .expect("manifest read")
        .expect("a completed delivery seals a manifest");
    assert_eq!(u64::from(manifest.files), final_row.written);
    assert!(destination.exists(), "the destination the button was given");

    // Cancel after completion is a no-op on a finished token, and the row keeps
    // its answer until the process forgets it.
    assert_eq!(
        one_click_status(&handle.job_id)
            .expect("still readable")
            .status,
        "completed"
    );
    let _ = one_click_cancel(&handle.job_id);

    // A terminal status promises the report is available and a new run can start.
    assert!(destination.join("aura-run.json").is_file());
    assert_eq!(final_row.ai_edited, 0);
    assert_eq!(final_row.local_edited, 2);
    let _waiting = state.register_import("waiting-import");
    let pending = one_click_finish(
        &state,
        OneClickFinishInput {
            project_id: project.id,
            destination: dir.path().join("cancelled-out").display().to_string(),
            ingest_job_id: Some("waiting-import".into()),
        },
    )
    .expect("previous worker is fully finished");
    assert!(one_click_cancel(&pending.job_id));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = one_click_status(&pending.job_id).expect("cancel status");
        if status.status == "cancelled" {
            assert_eq!(status.written, 0, "cancelled import must never export");
            assert!(dir.path().join("cancelled-out/aura-run.json").is_file());
            break;
        }
        assert_eq!(status.status, "cancelling");
        assert!(Instant::now() < deadline, "cancellation never completed");
        std::thread::sleep(Duration::from_millis(25));
    }
    state.finish_import("waiting-import");

    // Input failures must preserve the useful message instead of being replaced
    // by the renderer's generic "one edit could not be read" explanation.
    let missing_selection = aura_app::automatic_start(
        &state,
        aura_app::contract::ipc::AutomaticStartInput {
            project_id: None,
            roots: vec![],
        },
    )
    .expect_err("empty selection");
    assert_eq!(
        missing_selection.message,
        "Select photographs or a folder first."
    );
}
