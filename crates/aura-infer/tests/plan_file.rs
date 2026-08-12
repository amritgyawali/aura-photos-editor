//! `hardware_plan.json`: writing it, reading it, and refusing a bad one.
//!
//! The plan is a cache of measurements, never a source of truth, and every test
//! here is a consequence of that: an unreadable plan is replaced, a future schema
//! is discarded, and the processor path can never be removed from the order.

use std::collections::BTreeMap;

use aura_infer::contract::infer::{
    BatchDefaults, ExecutionProvider, HardwarePlan, ModelClass, SetAsideProvider,
};
use aura_infer::plan::{self, PlanPaths};
use tempfile::tempdir;

fn plan() -> HardwarePlan {
    HardwarePlan {
        schema: HardwarePlan::SCHEMA,
        gpu: None,
        ep_order: vec![ExecutionProvider::Cuda, ExecutionProvider::Cpu],
        vram_budget_mb: 4096,
        default_batch: BatchDefaults::conservative(),
        cpu_threads: 8,
        probe_scores_ms: BTreeMap::new(),
        probed_at: "2026-08-12T00:00:00Z".to_string(),
        probed: true,
        set_aside: Vec::new(),
        unavailable: Vec::new(),
        remembered_batch: BTreeMap::new(),
        ep_override: None,
    }
}

#[test]
fn a_plan_round_trips_through_the_file() {
    let dir = tempdir().expect("tempdir");
    let paths = PlanPaths::new(dir.path());
    let original = plan();
    plan::save(&paths, &original).expect("save");
    assert_eq!(
        plan::load(&paths).expect("load").expect("some plan"),
        original
    );
}

#[test]
fn no_plan_yet_is_not_an_error() {
    let dir = tempdir().expect("tempdir");
    assert!(plan::load(&PlanPaths::new(dir.path()))
        .expect("load")
        .is_none());
}

#[test]
fn an_unreadable_plan_is_reported_so_the_machine_is_measured_again() {
    let dir = tempdir().expect("tempdir");
    let paths = PlanPaths::new(dir.path());
    std::fs::write(paths.plan(), b"{not json").expect("write");
    assert_eq!(
        plan::load(&paths).expect_err("must refuse").code.0,
        "AURA-GPU-4005"
    );
}

#[test]
fn a_future_schema_is_refused_rather_than_guessed_at() {
    let dir = tempdir().expect("tempdir");
    let paths = PlanPaths::new(dir.path());
    let mut future = plan();
    future.schema = HardwarePlan::SCHEMA + 1;
    let rendered = serde_json::to_string(&future).expect("render");
    std::fs::write(paths.plan(), rendered).expect("write");
    assert_eq!(
        plan::load(&paths).expect_err("must refuse").code.0,
        "AURA-GPU-4005"
    );
}

#[test]
fn the_plan_file_is_written_in_the_spelling_the_phase_document_uses() {
    let dir = tempdir().expect("tempdir");
    let paths = PlanPaths::new(dir.path());
    let mut with_scores = plan();
    with_scores
        .probe_scores_ms
        .insert("tensorrt".to_string(), 3.1);
    with_scores.set_aside.push(SetAsideProvider {
        ep: ExecutionProvider::DirectMl,
        reason: "crash".to_string(),
        at: "2026-08-12T00:00:00Z".to_string(),
    });
    plan::save(&paths, &with_scores).expect("save");

    let text = std::fs::read_to_string(paths.plan()).expect("read");
    for expected in ["\"cuda\"", "\"cpu\"", "\"directml\"", "\"tensorrt\""] {
        assert!(
            text.contains(expected),
            "plan is missing {expected}: {text}"
        );
    }
}

#[test]
fn setting_a_provider_aside_removes_it_from_the_order_and_keeps_a_floor() {
    let mut plan = plan();
    plan::set_aside(&mut plan, ExecutionProvider::Cuda, "crash", "2026-08-12");
    assert!(plan.is_set_aside(ExecutionProvider::Cuda));
    assert_eq!(plan.ep_order, vec![ExecutionProvider::Cpu]);

    plan::set_aside(&mut plan, ExecutionProvider::Cpu, "crash", "2026-08-12");
    assert_eq!(
        plan.ep_order,
        vec![ExecutionProvider::Cpu],
        "the processor path is the floor and cannot be removed"
    );
}

#[test]
fn a_user_override_is_honoured_unless_that_provider_was_set_aside() {
    let mut plan = plan();
    plan.ep_override = Some(ExecutionProvider::Cuda);
    assert_eq!(plan.selected_ep(), ExecutionProvider::Cuda);

    plan::set_aside(&mut plan, ExecutionProvider::Cuda, "mismatch", "2026-08-12");
    assert_eq!(plan.selected_ep(), ExecutionProvider::Cpu);
}

#[test]
fn a_remembered_batch_beats_the_default() {
    let mut plan = plan();
    assert_eq!(
        plan.start_batch("scene", ModelClass::Embedding),
        BatchDefaults::conservative().embedding
    );
    plan::remember_batch(&mut plan, "scene", 3);
    assert_eq!(plan.start_batch("scene", ModelClass::Embedding), 3);
}
