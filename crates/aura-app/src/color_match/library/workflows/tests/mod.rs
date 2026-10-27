// End-to-end catalogue stories intentionally exercise multiple actions before reopening.
#![allow(clippy::too_many_lines)]
use super::*;

fn input(action: &str, snapshot: &WorkflowSnapshot) -> WorkflowInput {
    WorkflowInput {
        action: action.into(),
        expected_revision: Some(snapshot.revision.clone()),
        group: None,
        name: None,
        reference_ids: Vec::new(),
        order: Vec::new(),
        favorite: None,
        controls: None,
        document: None,
        replace_existing: false,
    }
}

fn read(state: &AppState) -> WorkflowSnapshot {
    run(
        state,
        &WorkflowInput {
            action: "read".into(),
            expected_revision: None,
            group: None,
            name: None,
            reference_ids: Vec::new(),
            order: Vec::new(),
            favorite: None,
            controls: None,
            document: None,
            replace_existing: false,
        },
    )
    .expect("library read")
}

fn seed(state: &AppState) {
    let appearance = aura_core::contract::look::LookAggregate {
        samples: 1,
        ..Default::default()
    };
    for (id, name) in [("a".repeat(64), "Warm"), ("b".repeat(64), "Cool")] {
        let reference = Reference {
            small_rgb: Vec::new(),
            achromatic: false,
            engine: ENGINE.into(),
            summary: ReferenceSummary {
                id: id.clone(),
                name: name.into(),
                width: 32,
                height: 48,
                converted_icc: false,
                group: String::new(),
                favorite: false,
                default_controls: None,
                thumbnail: None,
                regions: vec!["whole image".into()],
            },
            regions: vec![super::super::super::appearance::Region {
                name: "whole image".into(),
                points: Vec::new(),
                appearance: appearance.clone(),
            }],
        };
        super::super::super::store(state, &asset_key(&id).expect("id"), &reference)
            .expect("seed descriptor");
    }
    state.catalog().writer().with(|conn| {
        conn.execute("INSERT INTO setting(key,value_json,updated_at) VALUES('colour_match_v1:sentinel','{\"unchanged\":true}','2026-10-20')",[]).expect("session sentinel");
        Ok(())
    }).expect("sentinel write");
}

#[test]
fn group_and_batch_actions_survive_reopening_without_touching_sessions() {
    let directory = tempfile::tempdir().expect("isolated catalogue");
    let path = directory.path().join("library.aura");
    let state = AppState::open(&path).expect("state");
    seed(&state);
    let mut snapshot = read(&state);
    for name in ["Wedding", "Empty"] {
        let mut request = input("create_group", &snapshot);
        request.group = Some(name.into());
        snapshot = run(&state, &request).expect("empty group");
    }
    let mut request = input("reorder_groups", &snapshot);
    request.order = vec!["Empty".into(), "Wedding".into()];
    snapshot = run(&state, &request).expect("group order");
    let ids: Vec<_> = snapshot
        .references
        .iter()
        .map(|reference| reference.id.clone())
        .collect();
    let mut request = input("move", &snapshot);
    request.reference_ids = ids.clone();
    request.group = Some("Wedding".into());
    snapshot = run(&state, &request).expect("move both");
    let mut request = input("reorder_references", &snapshot);
    request.group = Some("Wedding".into());
    request.order = vec!["b".repeat(64), "a".repeat(64)];
    snapshot = run(&state, &request).expect("reference order");
    assert_eq!(snapshot.references[0].id, "b".repeat(64));
    let mut request = input("favorite", &snapshot);
    request.reference_ids = ids.clone();
    request.favorite = Some(true);
    snapshot = run(&state, &request).expect("favorites");
    let mut request = input("defaults", &snapshot);
    request.reference_ids = ids.clone();
    request.controls = Some(Controls {
        amount: 1.3,
        ..Controls::default()
    });
    snapshot = run(&state, &request).expect("batch defaults");
    assert!(snapshot
        .references
        .iter()
        .all(|reference| reference.favorite
            && reference
                .default_controls
                .is_some_and(|controls| controls.amount == 1.3)));
    let mut request = input("reset_defaults", &snapshot);
    request.reference_ids = ids.clone();
    snapshot = run(&state, &request).expect("reset defaults");
    let mut request = input("rename_group", &snapshot);
    request.group = Some("Wedding".into());
    request.name = Some("Portraits".into());
    snapshot = run(&state, &request).expect("rename group");
    let mut request = input("delete_group", &snapshot);
    request.group = Some("Portraits".into());
    snapshot = run(&state, &request).expect("ungroup");
    assert!(snapshot
        .references
        .iter()
        .all(|reference| reference.group.is_empty() && reference.default_controls.is_none()));
    let mut request = input("remove", &snapshot);
    request.reference_ids = vec![ids[0].clone()];
    let final_snapshot = run(&state, &request).expect("remove selected");
    assert_eq!(read(&state).revision, final_snapshot.revision);
    drop(state);
    let state = AppState::open(&path).expect("reopen");
    let restored = read(&state);
    assert_eq!(restored.groups, vec!["", "Empty"]);
    assert_eq!(restored.references.len(), 1);
    assert_eq!(restored.revision, final_snapshot.revision);
    let sentinel: String = state
        .catalog()
        .read(|conn| {
            conn.query_row(
                "SELECT value_json FROM setting WHERE key='colour_match_v1:sentinel'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| db(&e))
        })
        .expect("session");
    assert_eq!(sentinel, "{\"unchanged\":true}");
}

#[test]
fn stale_or_invalid_batch_is_atomic() {
    let directory = tempfile::tempdir().expect("isolated catalogue");
    let state = AppState::open(&directory.path().join("library.aura")).expect("state");
    seed(&state);
    let baseline = read(&state);
    let mut request = input("favorite", &baseline);
    request.favorite = Some(true);
    request.reference_ids = vec!["a".repeat(64), "c".repeat(64)];
    assert!(run(&state, &request).is_err());
    assert_eq!(read(&state).revision, baseline.revision);
    request.reference_ids = vec!["a".repeat(64)];
    request.expected_revision = Some("stale".into());
    assert!(run(&state, &request).is_err());
    assert!(read(&state)
        .references
        .iter()
        .all(|reference| !reference.favorite));
    let mut request = input("reorder_references", &baseline);
    request.order = vec!["a".repeat(64), "a".repeat(64)];
    assert!(run(&state, &request).is_err());
    assert_eq!(read(&state).revision, baseline.revision);
}

#[test]
fn portable_library_round_trips_and_refuses_descriptor_collisions_and_bad_data() {
    let directory = tempfile::tempdir().expect("isolated catalogue");
    let first = AppState::open(&directory.path().join("first.aura")).expect("first");
    seed(&first);
    let baseline = read(&first);
    let document = run(&first, &input("export", &baseline))
        .expect("export")
        .document
        .expect("document");
    assert!(
        !document.contains("thumbnail")
            && !document.contains("points")
            && !document.contains("path")
    );
    let second = AppState::open(&directory.path().join("second.aura")).expect("second");
    let mut request = input("import", &read(&second));
    request.document = Some(document.clone());
    let imported = run(&second, &request).expect("import");
    assert_eq!(imported.references.len(), 2);
    request.expected_revision = Some(imported.revision.clone());
    assert!(!run(&second, &request).expect("idempotent merge").changed);
    let mut bad: serde_json::Value = serde_json::from_str(&document).expect("json");
    bad["references"][0]["regions"][0]["appearance"]["tone"]["p50"] = 1.1.into();
    request.document = Some(bad.to_string());
    assert!(run(&second, &request).is_err());
    assert_eq!(read(&second).revision, imported.revision);
    bad["references"][0]["regions"][0]["appearance"]["tone"]["p50"] = 0.into();
    bad["references"][0]["regions"][0]["appearance"]["rendered_cct_k"] = 5500.into();
    request.document = Some(bad.to_string());
    request.replace_existing = true;
    assert!(
        run(&second, &request).is_err(),
        "same id with different descriptors must not replace"
    );
    assert_eq!(read(&second).revision, imported.revision);
    request.document = Some(document.replace("\"version\": 1", "\"version\": 99"));
    assert!(run(&second, &request).is_err());
}

#[test]
fn malformed_oversized_and_nonfinite_portable_data_never_changes_the_library() {
    let directory = tempfile::tempdir().expect("isolated catalogue");
    let state = AppState::open(&directory.path().join("library.aura")).expect("state");
    seed(&state);
    let baseline = read(&state);
    let document = run(&state, &input("export", &baseline))
        .expect("export")
        .document
        .expect("document");
    let mut request = input("import", &baseline);
    for malformed in [
        "{".into(),
        "x".repeat(MAX_DOCUMENT + 1),
        document.replace("\"tone_spread\": 0.0", "\"tone_spread\": NaN"),
    ] {
        request.document = Some(malformed);
        assert!(run(&state, &request).is_err());
        assert_eq!(read(&state).revision, baseline.revision);
    }
    let mut value: serde_json::Value = serde_json::from_str(&document).expect("json");
    value["references"][0]["regions"][0]["appearance"]["rendered_cct_k"] = serde_json::Value::Null;
    request.document = Some(value.to_string());
    assert!(run(&state, &request).is_err());
    let mut value: serde_json::Value = serde_json::from_str(&document).expect("json");
    value["references"][0]["width"] = 16_001.into();
    request.document = Some(value.to_string());
    assert!(run(&state, &request).is_err());
    let mut value: serde_json::Value = serde_json::from_str(&document).expect("json");
    let duplicate = value["references"][0].clone();
    value["references"]
        .as_array_mut()
        .expect("array")
        .push(duplicate);
    request.document = Some(value.to_string());
    assert!(run(&state, &request).is_err());
    assert_eq!(read(&state).revision, baseline.revision);
    let mut request = input("defaults", &baseline);
    request.reference_ids = vec!["a".repeat(64)];
    request.controls = Some(Controls {
        amount: f32::NAN,
        ..Controls::default()
    });
    assert!(run(&state, &request).is_err());
    assert_eq!(read(&state).revision, baseline.revision);
}

#[test]
fn full_library_refuses_new_imports_but_existing_reference_metadata_can_still_update() {
    let directory = tempfile::tempdir().expect("isolated catalogue");
    let state = AppState::open(&directory.path().join("library.aura")).expect("state");
    seed(&state);
    let existing_key = asset_key(&"a".repeat(64)).expect("id");
    let mut reference: Reference = super::super::super::read(&state, &existing_key)
        .expect("read")
        .expect("existing reference");
    let template = reference.clone();
    state
        .catalog()
        .writer()
        .with(move |conn| {
            let transaction = conn.unchecked_transaction().map_err(|e| db(&e))?;
            for index in 0..126 {
                let mut item = template.clone();
                item.summary.id = format!("{index:064x}");
                transaction
                    .execute(
                        "INSERT INTO setting(key,value_json,updated_at) VALUES(?1,?2,'2026-10-20')",
                        rusqlite::params![
                            asset_key(&item.summary.id)?,
                            serde_json::to_string(&item).expect("reference json")
                        ],
                    )
                    .map_err(|e| db(&e))?;
            }
            transaction.commit().map_err(|e| db(&e))?;
            Ok(())
        })
        .expect("fill library");
    assert_eq!(read(&state).references.len(), 128);
    reference.summary.name = "Updated at capacity".into();
    super::super::super::store(&state, &existing_key, &reference)
        .expect("existing reference is updateable");
    assert!(read(&state)
        .references
        .iter()
        .any(|item| item.name == "Updated at capacity"));
    reference.summary.id = "c".repeat(64);
    assert!(super::super::super::store(
        &state,
        &asset_key(&reference.summary.id).expect("new id"),
        &reference
    )
    .is_err());
    assert_eq!(read(&state).references.len(), 128);
    assert!(super::super::super::read::<Reference>(
        &state,
        &asset_key(&reference.summary.id).expect("new id")
    )
    .expect("read")
    .is_none());
}
