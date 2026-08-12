//! The hardware probe and the plan it writes.
//!
//! Section 13: "first run probes hardware in <= 15 s and writes a plan; Settings
//! shows the selected EP and lets the user override". Everything below is that
//! sentence, plus the two failure paths a real machine will eventually take: a
//! provider that misbehaves and has to be set aside, and a plan file that cannot
//! be read and has to be replaced.

use std::sync::Arc;
use std::time::{Duration, Instant};

use aura_core::clock::{Clock, SystemClock};
use aura_infer::backend::{FakeBackend, FakeBehaviour};
use aura_infer::contract::infer::ExecutionProvider;
use aura_infer::ep::BackendRegistry;
use aura_infer::plan::{self, PlanPaths};
use aura_infer::probe::probe;
use tempfile::TempDir;

fn clock() -> Arc<dyn Clock> {
    Arc::new(SystemClock::default())
}

#[test]
fn a_first_run_measures_the_machine_and_writes_a_plan() {
    let dir = TempDir::new().expect("tempdir");
    let paths = PlanPaths::new(dir.path());
    let clock = clock();

    let started = Instant::now();
    let outcome = probe(&BackendRegistry::with_reference(), &paths, &clock).expect("probe");
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(15),
        "the probe took {elapsed:?}; section 13 budgets 15 s"
    );
    assert!(outcome.persist);
    assert!(outcome.plan.probed);
    assert_eq!(outcome.plan.ep_order, vec![ExecutionProvider::Cpu]);
    assert!(outcome.plan.probe_scores_ms.contains_key("cpu"));

    plan::save(&paths, &outcome.plan).expect("save");
    let reloaded = plan::load(&paths).expect("load").expect("a plan");
    assert_eq!(reloaded, outcome.plan);
}

#[test]
fn a_machine_with_no_accelerator_says_why_rather_than_hiding_it() {
    let dir = TempDir::new().expect("tempdir");
    let outcome = probe(
        &BackendRegistry::with_reference(),
        &PlanPaths::new(dir.path()),
        &clock(),
    )
    .expect("probe");

    for ep in [
        ExecutionProvider::TensorRt,
        ExecutionProvider::Cuda,
        ExecutionProvider::DirectMl,
        ExecutionProvider::CoreMl,
    ] {
        let entry = outcome
            .plan
            .unavailable
            .iter()
            .find(|entry| entry.ep == ep)
            .unwrap_or_else(|| panic!("{} must be reported as unavailable", ep.as_str()));
        assert!(
            !entry.reason.is_empty(),
            "an unavailable provider needs a reason for the Settings panel"
        );
    }
    assert_eq!(outcome.plan.selected_ep(), ExecutionProvider::Cpu);
}

#[test]
fn a_provider_that_disagrees_with_the_processor_is_set_aside_in_the_plan() {
    let dir = TempDir::new().expect("tempdir");
    let registry = BackendRegistry::with_reference().register(Arc::new(FakeBackend::new(
        ExecutionProvider::Cuda,
        FakeBehaviour::Drift(1.0),
    )));

    let outcome = probe(&registry, &PlanPaths::new(dir.path()), &clock()).expect("probe");

    assert!(outcome.plan.is_set_aside(ExecutionProvider::Cuda));
    assert!(!outcome.plan.ep_order.contains(&ExecutionProvider::Cuda));
    assert_eq!(outcome.plan.selected_ep(), ExecutionProvider::Cpu);
}

#[test]
fn a_correct_accelerator_is_preferred_over_the_processor() {
    let dir = TempDir::new().expect("tempdir");
    let registry = BackendRegistry::with_reference().register(Arc::new(FakeBackend::new(
        ExecutionProvider::Cuda,
        FakeBehaviour::Correct,
    )));

    let outcome = probe(&registry, &PlanPaths::new(dir.path()), &clock()).expect("probe");

    assert_eq!(
        outcome.plan.ep_order.first(),
        Some(&ExecutionProvider::Cuda),
        "probe order prefers the accelerator: {:?}",
        outcome.plan.ep_order
    );
    assert!(outcome.plan.ep_order.contains(&ExecutionProvider::Cpu));
    assert_eq!(registry.select(&outcome.plan), ExecutionProvider::Cuda);
}

#[test]
fn a_set_aside_provider_survives_a_restart_and_is_cleared_by_a_recheck() {
    let dir = TempDir::new().expect("tempdir");
    let paths = PlanPaths::new(dir.path());
    let registry = BackendRegistry::with_reference().register(Arc::new(FakeBackend::new(
        ExecutionProvider::Cuda,
        FakeBehaviour::Crash,
    )));

    let outcome = probe(&registry, &paths, &clock()).expect("probe");
    plan::save(&paths, &outcome.plan).expect("save");

    let mut reloaded = plan::load(&paths).expect("load").expect("a plan");
    assert!(reloaded.is_set_aside(ExecutionProvider::Cuda));

    plan::clear_set_aside(&mut reloaded);
    assert!(!reloaded.is_set_aside(ExecutionProvider::Cuda));
}

#[test]
fn an_unreadable_plan_is_replaced_rather_than_repaired() {
    let dir = TempDir::new().expect("tempdir");
    let paths = PlanPaths::new(dir.path());
    std::fs::create_dir_all(dir.path()).expect("dir");
    std::fs::write(paths.plan(), b"half a plan").expect("write");

    let err = plan::load(&paths).expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-GPU-4005");

    // The recovery is to measure again, and it must succeed.
    let outcome = probe(&BackendRegistry::with_reference(), &paths, &clock()).expect("probe");
    plan::save(&paths, &outcome.plan).expect("save");
    assert!(plan::load(&paths).expect("load").is_some());
}

#[test]
fn a_build_with_no_backend_at_all_refuses_to_pretend_it_measured_something() {
    let dir = TempDir::new().expect("tempdir");
    let err = probe(
        &BackendRegistry::new(),
        &PlanPaths::new(dir.path()),
        &clock(),
    )
    .expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-GPU-4004");
}
