//! Leases, heartbeats and the dependency gate.

use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, rfc3339, Catalog};
use aura_core::clock::{Clock, FixedClock};
use aura_jobs::graph::{JobGraph, Task, TaskState};
use aura_jobs::lease::{claim_next, heartbeat, reclaim_expired, LeaseConfig};
use time::OffsetDateTime;

struct Harness {
    _dir: tempfile::TempDir,
    catalog: Catalog,
    clock: Arc<FixedClock>,
    project_id: String,
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().expect("tmp");
    let clock = FixedClock::at(OffsetDateTime::UNIX_EPOCH + time::Duration::days(20_000));
    let catalog = Catalog::open(
        &dir.path().join("c.sqlite"),
        Arc::clone(&clock) as Arc<dyn Clock>,
        "0.1.0-test",
    )
    .expect("open catalog");

    let now = rfc3339(catalog.clock().now_utc());
    let row = ProjectRow {
        project_id: "prj_jobs".to_string(),
        name: "Jobs".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    catalog
        .writer()
        .transact(move |tx| repo::create_project(tx, &row))
        .expect("create project");

    Harness {
        _dir: dir,
        catalog,
        clock,
        project_id: "prj_jobs".to_string(),
    }
}

#[test]
fn planning_is_idempotent() {
    let h = harness();
    let graph = JobGraph::new(&h.catalog);
    let tasks = vec![Task::new(&h.project_id, "run1", "hash", "pht_1", 1)];

    assert_eq!(graph.plan(tasks.clone(), Vec::new()).expect("plan"), 1);
    assert_eq!(
        graph.plan(tasks, Vec::new()).expect("re-plan"),
        0,
        "re-planning the same run must not duplicate work"
    );
}

#[test]
fn dependencies_gate_readiness() {
    let h = harness();
    let graph = JobGraph::new(&h.catalog);

    let parent = Task::new(&h.project_id, "run1", "hash", "pht_1", 1);
    let child = Task::new(&h.project_id, "run1", "exif", "pht_1", 1);
    let edges = vec![(child.task_id.clone(), parent.task_id.clone())];
    graph
        .plan(vec![parent.clone(), child.clone()], edges)
        .expect("plan");

    assert_eq!(
        graph.state_of(&child.task_id).expect("state"),
        Some(TaskState::Pending)
    );

    graph.succeed(&parent.task_id).expect("succeed");
    graph.promote_ready().expect("promote");

    assert_eq!(
        graph.state_of(&child.task_id).expect("state"),
        Some(TaskState::Ready)
    );
}

#[test]
fn two_workers_cannot_claim_the_same_task() {
    let h = harness();
    let graph = JobGraph::new(&h.catalog);
    graph
        .plan(
            vec![Task::new(&h.project_id, "run1", "hash", "pht_1", 1)],
            Vec::new(),
        )
        .expect("plan");

    let first = claim_next(
        &h.catalog,
        &h.project_id,
        "worker-a",
        LeaseConfig::default(),
    )
    .expect("claim")
    .expect("a task");
    let second = claim_next(
        &h.catalog,
        &h.project_id,
        "worker-b",
        LeaseConfig::default(),
    )
    .expect("claim");

    assert_eq!(first.owner, "worker-a");
    assert!(
        second.is_none(),
        "the queue was empty for the second worker"
    );
}

#[test]
fn an_expired_lease_is_reclaimed_and_the_work_is_not_lost() {
    let h = harness();
    let graph = JobGraph::new(&h.catalog);
    graph
        .plan(
            vec![Task::new(&h.project_id, "run1", "hash", "pht_1", 1)],
            Vec::new(),
        )
        .expect("plan");

    let config = LeaseConfig {
        lease_ms: 1_000,
        heartbeat_ms: 200,
    };
    let lease = claim_next(&h.catalog, &h.project_id, "worker-a", config)
        .expect("claim")
        .expect("a task");

    // The worker stops responding; wall clock moves past the lease.
    h.clock.advance_ms(5_000);
    assert_eq!(reclaim_expired(&h.catalog).expect("reclaim"), 1);
    assert_eq!(
        graph.state_of(&lease.task_id).expect("state"),
        Some(TaskState::Ready)
    );

    // Its heartbeat now fails with the typed code rather than silently succeeding.
    let err = heartbeat(&h.catalog, &lease, config).expect_err("must fail");
    assert_eq!(err.code.0, "AURA-JOB-7002");
}

#[test]
fn a_live_worker_keeps_its_lease() {
    let h = harness();
    let graph = JobGraph::new(&h.catalog);
    graph
        .plan(
            vec![Task::new(&h.project_id, "run1", "hash", "pht_1", 1)],
            Vec::new(),
        )
        .expect("plan");

    let config = LeaseConfig {
        lease_ms: 10_000,
        heartbeat_ms: 1_000,
    };
    let lease = claim_next(&h.catalog, &h.project_id, "worker-a", config)
        .expect("claim")
        .expect("a task");

    h.clock.advance_ms(3_000);
    heartbeat(&h.catalog, &lease, config).expect("renew");
    h.clock.advance_ms(3_000);

    assert_eq!(reclaim_expired(&h.catalog).expect("reclaim"), 0);
    assert_eq!(
        graph.state_of(&lease.task_id).expect("state"),
        Some(TaskState::Running)
    );
}

#[test]
fn retries_are_bounded_and_end_in_quarantine() {
    let h = harness();
    let graph = JobGraph::new(&h.catalog);
    let task = Task::new(&h.project_id, "run1", "hash", "pht_1", 1);
    graph.plan(vec![task.clone()], Vec::new()).expect("plan");

    assert_eq!(
        graph
            .fail(&task.task_id, "AURA-IO-1006", "unreadable")
            .expect("fail"),
        TaskState::Failed
    );
    graph.requeue_failed().expect("requeue");
    assert_eq!(
        graph
            .fail(&task.task_id, "AURA-IO-1006", "unreadable")
            .expect("fail"),
        TaskState::Failed
    );
    graph.requeue_failed().expect("requeue");
    assert_eq!(
        graph
            .fail(&task.task_id, "AURA-IO-1006", "unreadable")
            .expect("fail"),
        TaskState::Quarantined,
        "the third failure exhausts the default budget"
    );

    graph.requeue_failed().expect("requeue");
    assert_eq!(
        graph.state_of(&task.task_id).expect("state"),
        Some(TaskState::Quarantined),
        "a quarantined task is never silently retried"
    );
}

#[test]
fn cancelling_a_run_stops_every_unfinished_task() {
    let h = harness();
    let graph = JobGraph::new(&h.catalog);
    graph
        .plan(
            vec![
                Task::new(&h.project_id, "run1", "hash", "pht_1", 1),
                Task::new(&h.project_id, "run1", "hash", "pht_2", 1),
            ],
            Vec::new(),
        )
        .expect("plan");
    graph.succeed("hash:pht_1:1").expect("succeed");

    assert_eq!(graph.cancel_run("run1").expect("cancel"), 1);

    let counts = graph.state_counts("run1").expect("counts");
    assert!(counts.contains(&("succeeded".to_string(), 1)));
    assert!(counts.contains(&("skipped".to_string(), 1)));
}
