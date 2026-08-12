//! The phase 03 half of the IPC surface, and the one enum that exists twice.
//!
//! Same rule as `ipc_contract.rs`: a key that exists in Rust and not in
//! TypeScript is a runtime `undefined` in the UI, so it fails here instead.

use std::path::{Path, PathBuf};

use aura_app::contract::ipc::{
    HardwarePlanDto, InferStatsDto, ModelStatusDto, ProbeScoreDto, ProviderNoteDto,
    SetExecutionProviderInput, WarmupReportDto,
};

fn types_ts() -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/ipc/types.ts");
    std::fs::read_to_string(path).expect("ui/src/ipc/types.ts")
}

fn assert_keys_declared<T: serde::Serialize>(value: &T, type_name: &str) {
    let json = serde_json::to_value(value).expect("serialise");
    let object = json.as_object().expect("a struct serialises to an object");
    let ts = types_ts();

    let start = ts
        .find(&format!("export type {type_name} = {{"))
        .unwrap_or_else(|| panic!("{type_name} is missing from types.ts"));
    let end = ts[start..]
        .find("};")
        .map(|offset| start + offset)
        .unwrap_or(ts.len());
    let block = &ts[start..end];

    for key in object.keys() {
        assert!(
            block.contains(&format!("{key}:")),
            "{type_name}.{key} is missing from ui/src/ipc/types.ts"
        );
    }
}

fn plan_dto() -> HardwarePlanDto {
    HardwarePlanDto {
        gpu: None,
        ep_order: vec!["cpu".to_string()],
        selected_ep: "cpu".to_string(),
        override_ep: None,
        unavailable: vec![ProviderNoteDto {
            ep: "cuda".to_string(),
            reason: "not compiled into this build".to_string(),
        }],
        set_aside: Vec::new(),
        vram_budget_mb: 0,
        cpu_threads: 7,
        probe_scores_ms: vec![ProbeScoreDto {
            ep: "cpu".to_string(),
            median_ms: 1.4,
        }],
        probed_at: "2026-08-12T00:00:00Z".to_string(),
        probed: true,
    }
}

#[test]
fn the_hardware_plan_matches_typescript() {
    assert_keys_declared(&plan_dto(), "HardwarePlanDto");
    assert_keys_declared(
        &ProviderNoteDto {
            ep: "directml".to_string(),
            reason: "set aside: mismatch".to_string(),
        },
        "ProviderNoteDto",
    );
    assert_keys_declared(
        &ProbeScoreDto {
            ep: "cpu".to_string(),
            median_ms: 1.4,
        },
        "ProbeScoreDto",
    );
}

#[test]
fn the_model_listing_matches_typescript() {
    assert_keys_declared(
        &ModelStatusDto {
            name: "aura_tiny_embedding".to_string(),
            version: "1.0.0".to_string(),
            task: "embedding".to_string(),
            active_version: Some("1.0.0".to_string()),
            pending_version: None,
            rejected_versions: Vec::new(),
            model_card: "docs/model-cards/aura_tiny_embedding.md".to_string(),
            working_set_mb: 8,
            file_count: 3,
            int8_forbidden: false,
        },
        "ModelStatusDto",
    );
}

#[test]
fn the_warmup_report_and_counters_match_typescript() {
    assert_keys_declared(
        &WarmupReportDto {
            loaded: 2,
            elapsed_ms: 3,
            ep_used: "cpu".to_string(),
        },
        "WarmupReportDto",
    );
    assert_keys_declared(
        &InferStatsDto {
            resident_sessions: 2,
            pool_hits: 40,
            pool_loads: 2,
            requests: 42,
            downshifts: 0,
            mean_overhead_ms: 0.0001,
            peak_memory_mb: 16,
        },
        "InferStatsDto",
    );
    assert_keys_declared(
        &SetExecutionProviderInput {
            ep: "auto".to_string(),
        },
        "SetExecutionProviderInput",
    );
}

#[test]
fn the_plan_dto_carries_a_reason_for_every_provider_it_hides() {
    // The panel exists to answer "why is AURA not using my graphics card". A
    // provider listed without a reason answers it with silence.
    for note in &plan_dto().unavailable {
        assert!(!note.reason.trim().is_empty(), "{} has no reason", note.ep);
    }
}

#[test]
fn the_two_priority_enums_stay_in_lockstep() {
    // `aura-core::Priority` and the copy frozen inside `aura-preview` are the
    // same four classes. They are separate types because `aura-infer` must not
    // depend on preview infrastructure (Article II rule A2), and this test is
    // what stops them drifting - see the note in `contract/priority.rs`.
    let core = aura_core::Priority::all();
    let preview = aura_preview::Priority::all();
    assert_eq!(core.len(), preview.len());

    for (core, preview) in core.iter().zip(preview.iter()) {
        assert_eq!(
            core.as_str(),
            preview.as_str(),
            "the two Priority enums list their classes in different orders"
        );
    }
}

#[test]
fn only_the_two_interactive_classes_preempt() {
    use aura_core::Priority;

    assert!(Priority::Visible.preempts());
    assert!(Priority::Interactive.preempts());
    assert!(!Priority::AiBatch.preempts());
    assert!(
        !Priority::Background.preempts(),
        "an AI batch that could preempt background work would starve the \
         speculative decode that makes the next screen instant"
    );
}
