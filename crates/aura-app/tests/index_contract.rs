//! The similarity half of the IPC surface must match its TypeScript twin.
//!
//! Same argument as `ipc_contract.rs` and `infer_contract.rs`: a field that exists
//! on one side and not the other is a runtime `undefined` in a web view, which is
//! the one class of bug a typed boundary is supposed to make impossible. So the
//! check is mechanical and it runs in CI.
//!
//! It also asserts two things that are not shape: that no command returns a
//! vector, and that the panel's own file exists. Both are promises ADR-0012 makes,
//! and a promise nothing checks is a comment.

use std::path::{Path, PathBuf};

use aura_app::contract::ipc::{
    DescriptorsDto, EmbedProgressDto, EmbedProjectInput, FindSimilarInput, IndexStatusDto,
    SimilarNeighbourDto, SimilarResultDto,
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

fn neighbour() -> SimilarNeighbourDto {
    SimilarNeighbourDto {
        photo_id: "pht_00000000-0000-0000-0000-000000000001".to_string(),
        distance: 0.043,
        similarity: 0.957,
        dhash_distance: 12,
        near_duplicate: false,
    }
}

#[test]
fn find_similar_input_matches_typescript() {
    assert_keys_declared(
        &FindSimilarInput {
            project_id: "prj_1".to_string(),
            photo_id: "pht_1".to_string(),
            k: 32,
            time_window_s: None,
            camera_id: None,
            exclude: Vec::new(),
        },
        "FindSimilarInput",
    );
}

#[test]
fn similar_result_and_neighbour_match_typescript() {
    assert_keys_declared(&neighbour(), "SimilarNeighbourDto");
    assert_keys_declared(
        &SimilarResultDto {
            photo_id: "pht_1".to_string(),
            neighbours: vec![neighbour()],
            elapsed_ms: 1.25,
            filter_kind: "none".to_string(),
        },
        "SimilarResultDto",
    );
}

#[test]
fn index_status_matches_typescript() {
    assert_keys_declared(
        &IndexStatusDto {
            vectors: 4_000,
            photos: 4_000,
            coverage: 1.0,
            filterable: 4_000,
            model_ver: 100,
            stale_model_versions: Vec::new(),
            build_ms: 312,
            from_snapshot: false,
        },
        "IndexStatusDto",
    );
}

#[test]
fn embed_types_match_typescript() {
    assert_keys_declared(
        &EmbedProjectInput {
            project_id: "prj_1".to_string(),
        },
        "EmbedProjectInput",
    );
    assert_keys_declared(
        &EmbedProgressDto {
            embedded: 4_000,
            failed: 0,
            remaining: 0,
            elapsed_ms: 156_000,
            batches: 125,
            cancelled: false,
        },
        "EmbedProgressDto",
    );
}

#[test]
fn descriptors_match_typescript() {
    assert_keys_declared(
        &DescriptorsDto {
            photo_id: "pht_1".to_string(),
            dhash_hex: "a5a5f00d0badc0de".to_string(),
            luma_mean: 0.42,
            luma_p1: 0.02,
            luma_p50: 0.40,
            luma_p99: 0.95,
            clip_lo: 0.0,
            clip_hi: 0.03,
            edge_energy: 0.11,
            palette: vec!["#c8b4a0".to_string()],
            model_ver: 100,
            pixel_source: "tier 1 embedded".to_string(),
        },
        "DescriptorsDto",
    );
}

#[test]
fn every_index_event_variant_is_declared_in_typescript() {
    let ts = types_ts();
    // Section 11's three telemetry events, and no others: an event the UI cannot
    // name is an event nobody will ever see.
    for variant in ["embedProgress", "indexBuilt", "queryTimed"] {
        assert!(
            ts.contains(&format!("kind: '{variant}'")),
            "IndexEvent::{variant} is missing from types.ts"
        );
    }
    assert!(ts.contains("export type IndexEvent ="));
}

#[test]
fn the_query_event_carries_the_filter_kind_and_not_the_filter() {
    // ADR-0012: a time window plus a camera identifies a shoot, so the event
    // carries the *kind* of filter and never its contents.
    let ts = types_ts();
    let start = ts
        .find("export type IndexEvent =")
        .expect("IndexEvent is missing");
    let block = &ts[start..];
    let end = block.find("\n\n").unwrap_or(block.len());
    let declaration = &block[..end];

    assert!(declaration.contains("filterKind"));
    assert!(
        !declaration.contains("timeWindowS") && !declaration.contains("cameraId"),
        "the query event leaks the filter's contents"
    );
}

#[test]
fn no_command_returns_a_vector() {
    // ADR-0012: there is deliberately no `getEmbedding`. 512 halves per image across
    // a 4,000-cell grid is megabytes of JSON per screen, and a web view has no use
    // for a number it cannot compare.
    let ts = types_ts();
    for forbidden in ["getEmbedding", "EmbeddingDto", "vec:", "vector:"] {
        assert!(
            !ts.contains(forbidden),
            "the IPC surface exposes {forbidden}, which puts vectors in the web view"
        );
    }

    let client: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/ipc/client.ts");
    let source = std::fs::read_to_string(client).expect("ui/src/ipc/client.ts");
    assert!(!source.contains("get_embedding"));
    for command in [
        "find_similar",
        "index_status",
        "build_index",
        "embed_project",
        "image_descriptors",
    ] {
        assert!(
            source.contains(command),
            "the client cannot call {command}, so the panel cannot either"
        );
    }
}

#[test]
fn the_debug_panel_exists_where_the_adr_says_it_does() {
    // ADR-0012 names the file. A phase that shipped the commands and not the panel
    // would satisfy every other test here and satisfy no acceptance criterion.
    let panel: PathBuf =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/components/SimilarPanel.tsx");
    assert!(
        panel.exists(),
        "ui/src/components/SimilarPanel.tsx is missing"
    );
    let source = std::fs::read_to_string(panel).expect("read the panel");
    // The three things section 9 asks the panel for: distances, a cluster preview
    // and a readout.
    assert!(source.contains("similarityLabel"));
    assert!(source.contains("duplicateLabel"));
    assert!(source.contains("descriptorLines"));
}
