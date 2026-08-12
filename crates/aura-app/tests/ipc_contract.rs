//! The generated TypeScript must match the Rust command surface. A drift here is
//! a runtime `undefined` in the UI, so it fails CI instead.

use std::path::{Path, PathBuf};

use aura_app::contract::ipc::{
    CacheStatsDto, CreateProjectInput, GetPreviewInput, ImageRowLite, IngestEvent, IpcError,
    ListImagesInput, PrefetchInput, PreviewEvent, PreviewPayload, SetCacheBudgetInput,
    SetCameraLabelInput,
};

fn types_ts() -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/ipc/types.ts");
    std::fs::read_to_string(path).expect("ui/src/ipc/types.ts")
}

/// Serialise a value and assert every JSON key appears in the TypeScript type.
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

#[test]
fn create_project_input_matches_typescript() {
    assert_keys_declared(
        &CreateProjectInput {
            name: "Sarah and Tom".to_string(),
            couple_names: None,
            event_date: None,
        },
        "CreateProjectInput",
    );
}

#[test]
fn image_row_matches_typescript() {
    assert_keys_declared(
        &ImageRowLite {
            id: "pht_1".to_string(),
            file_name: "IMG_0001.CR2".to_string(),
            timeline_ts: None,
            camera_id: None,
            width: 0,
            height: 0,
            status: "indexed".to_string(),
        },
        "ImageRowLite",
    );
}

#[test]
fn list_images_input_matches_typescript() {
    assert_keys_declared(
        &ListImagesInput {
            project_id: "prj_1".to_string(),
            offset: 0,
            limit: 240,
            order_by: None,
        },
        "ListImagesInput",
    );
}

#[test]
fn set_camera_label_input_matches_typescript() {
    assert_keys_declared(
        &SetCameraLabelInput {
            camera_id: "cam_1".to_string(),
            shooter_label: "second".to_string(),
            clock_offset_ms: -3_600_000,
        },
        "SetCameraLabelInput",
    );
}

#[test]
fn ipc_error_matches_typescript_and_carries_a_runbook() {
    let error = IpcError::from(aura_core::errors::io::not_found(Path::new("D:/DCIM")));
    assert_keys_declared(&error, "IpcError");
    assert_eq!(error.code, "AURA-IO-1001");
    assert_eq!(error.runbook_url, "https://aura.app/e/AURA-IO-1001");
    assert!(
        !error.message.contains("D:/DCIM"),
        "a photographer-facing message must not leak a path"
    );
}

#[test]
fn preview_types_match_typescript() {
    assert_keys_declared(
        &GetPreviewInput {
            project_id: "prj_1".to_string(),
            photo_id: "pht_1".to_string(),
            level: "thumb".to_string(),
            priority: "visible".to_string(),
        },
        "GetPreviewInput",
    );
    assert_keys_declared(
        &PreviewPayload {
            photo_id: "pht_1".to_string(),
            tier: 1,
            width: 512,
            height: 341,
            source: "embedded".to_string(),
            data_url: "data:image/jpeg;base64,".to_string(),
        },
        "PreviewPayload",
    );
    assert_keys_declared(
        &PrefetchInput {
            project_id: "prj_1".to_string(),
            photo_ids: Vec::new(),
            level: "proxy".to_string(),
        },
        "PrefetchInput",
    );
    assert_keys_declared(
        &CacheStatsDto {
            bytes_used: 0,
            budget_bytes: 0,
            entries: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            hit_rate: 0.0,
        },
        "CacheStatsDto",
    );
    assert_keys_declared(
        &SetCacheBudgetInput {
            project_id: "prj_1".to_string(),
            budget_bytes: 40 * 1024 * 1024 * 1024,
        },
        "SetCacheBudgetInput",
    );
}

#[test]
fn every_preview_event_variant_is_declared_in_typescript() {
    let ts = types_ts();
    let variants = [
        PreviewEvent::Ready {
            photo_id: "pht_1".to_string(),
            tier: 1,
        },
        PreviewEvent::Failed {
            photo_id: "pht_1".to_string(),
            code: "AURA-RAW-2002".to_string(),
            message: String::new(),
        },
        PreviewEvent::CacheStats {
            bytes_used: 0,
            budget_bytes: 0,
            hit_rate: 0.0,
        },
    ];

    for variant in variants {
        let json = serde_json::to_value(&variant).expect("serialise");
        let kind = json
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .expect("tagged with kind")
            .to_string();
        assert!(
            ts.contains(&format!("kind: '{kind}'")),
            "PreviewEvent variant {kind} is missing from types.ts"
        );
    }
}

#[test]
fn every_ingest_event_variant_is_declared_in_typescript() {
    let ts = types_ts();
    let variants = [
        IngestEvent::Discovered { total_hint: 1 },
        IngestEvent::Progress {
            done: 1,
            total: 2,
            current: String::new(),
        },
        IngestEvent::Batch { rows: Vec::new() },
        IngestEvent::Warning {
            code: "AURA-IO-1006".to_string(),
            message: String::new(),
        },
        IngestEvent::Finished {
            inserted: 0,
            skipped: 0,
            failed: 0,
            elapsed_ms: 0,
        },
    ];

    for variant in variants {
        let json = serde_json::to_value(&variant).expect("serialise");
        let kind = json
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .expect("tagged with kind")
            .to_string();
        assert!(
            ts.contains(&format!("kind: '{kind}'")),
            "IngestEvent variant {kind} is missing from types.ts"
        );
    }
}
