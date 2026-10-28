//! Real background import, event delivery, polling and durable failure state.
use std::sync::{mpsc, Arc};
use std::time::Duration;

use aura_app::contract::ipc::{CreateProjectInput, IngestEvent, StartIngestInput};
use aura_app::{commands::start_ingest_with_events, create_project, ingest_progress, AppState};
use aura_raw::codec::{encode_jpeg, Rgb8};

fn run(fail: bool) {
    let dir = tempfile::tempdir().expect("temporary fixture");
    let state = AppState::open(&dir.path().join("catalog.aura")).expect("catalog");
    let project = create_project(
        &state,
        CreateProjectInput {
            name: "Import regression".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    let bytes = encode_jpeg(
        &Rgb8 {
            width: 8,
            height: 8,
            data: vec![120; 8 * 8 * 3],
        },
        90,
    )
    .expect("jpeg");
    let names = ["first.jpg", "duplicate.jpg", "last.jpg"];
    for name in names {
        std::fs::write(dir.path().join(name), &bytes).expect("fixture");
    }
    if fail {
        state.catalog().writer().transact(|tx| {
            tx.execute_batch("CREATE TRIGGER reject_second_file BEFORE INSERT ON photo_file WHEN NEW.file_name='duplicate.jpg' BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;")
                .map_err(|error| aura_core::errors::db::statement_failed("inject failure", &error))
        }).expect("failure injection");
    }
    let (sender, receiver) = mpsc::sync_channel(32);
    let handle = start_ingest_with_events(
        &state,
        &StartIngestInput {
            project_id: project.id.clone(),
            roots: names
                .iter()
                .map(|name| dir.path().join(name).display().to_string())
                .collect(),
        },
        Arc::new(move |event| {
            sender.send(event).expect("event receiver");
        }),
    )
    .expect("start");
    let mut updates = Vec::new();
    let mut warned = false;
    loop {
        match receiver
            .recv_timeout(Duration::from_secs(20))
            .expect("worker must terminate")
        {
            IngestEvent::Progress { done, total, .. } => updates.push((done, total)),
            IngestEvent::Warning { .. } => warned = true,
            IngestEvent::Finished { inserted, .. } => {
                if !fail {
                    assert_eq!(inserted, 3);
                }
                break;
            }
            _ => {}
        }
    }
    assert_eq!(warned, fail);
    assert_eq!(updates.first(), Some(&(0, 3)));
    assert!(updates.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    if fail {
        assert_eq!(
            ingest_progress(&state, &handle.job_id)
                .expect_err("polling retains failure")
                .code,
            "AURA-DB-3006"
        );
        assert!(
            ingest_progress(&state, &handle.job_id).is_err(),
            "error survives repeated polls"
        );
        let finish = aura_app::one_click_finish(
            &state,
            aura_app::contract::ipc::OneClickFinishInput {
                project_id: project.id,
                destination: dir.path().join("delivery").display().to_string(),
                ingest_job_id: Some(handle.job_id.clone()),
                look: None,
                keep_everything: true,
                only_photos: Vec::new(),
            },
        )
        .expect("start dependent workflow");
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            let status = aura_app::one_click_status(&finish.job_id).expect("workflow status");
            if status.status != "running" {
                assert_eq!(
                    status.status, "failed",
                    "a partial import must not reach delivery"
                );
                assert_eq!(status.written, 0);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "workflow did not stop"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    } else {
        let status = ingest_progress(&state, &handle.job_id).expect("progress");
        assert_eq!(
            (status.known, status.done, status.total, status.running),
            (true, 3, 3, false)
        );
    }
    let journal: (String, u64, bool) = state.catalog().read(|conn| {
        conn.query_row("SELECT state, files_imported, finished_at IS NOT NULL FROM import_run WHERE import_id=?1", [&handle.job_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|error| aura_core::errors::db::statement_failed("read journal", &error))
    }).expect("journal");
    assert_eq!(
        journal,
        (
            if fail { "failed" } else { "completed" }.into(),
            if fail { 1 } else { 3 },
            true
        )
    );
    for name in names {
        assert_eq!(
            std::fs::read(dir.path().join(name)).expect("original"),
            bytes
        );
    }
}

#[test]
fn mixed_import_delivers_cumulative_events_and_finishes_polling() {
    run(false);
}

#[test]
fn failed_import_is_reported_to_events_and_polling_with_committed_work_preserved() {
    run(true);
}
