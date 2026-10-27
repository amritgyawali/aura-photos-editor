#![allow(clippy::too_many_lines)]
use super::*;
use aura_core::{
    progress::{CancelToken, NullProgress},
    ImportId,
};

struct Fixture {
    directory: tempfile::TempDir,
    state: AppState,
    project: ProjectId,
    photos: Vec<String>,
    originals: Vec<(std::path::PathBuf, Vec<u8>)>,
}
fn fixture() -> Fixture {
    fixture_with_recipes(true)
}
fn fixture_with_recipes(save_initial_recipes: bool) -> Fixture {
    let directory = tempfile::tempdir().expect("workspace");
    let state = AppState::open(&directory.path().join("catalog.aura")).expect("catalog");
    let project = crate::create_project(
        &state,
        crate::contract::ipc::CreateProjectInput {
            name: "Review".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    let project = ProjectId::from_db(&project.id).expect("project id");
    let mut originals = Vec::new();
    for number in 0..2_u8 {
        let path = directory.path().join(format!("{number}.png"));
        image::RgbImage::from_fn(16, 12, |x, y| {
            image::Rgb([
                number * 30 + u8::try_from(x).expect("small chart x") * 3,
                40 + u8::try_from(y).expect("small chart y") * 5,
                90,
            ])
        })
        .save(&path)
        .expect("photo");
        originals.push((path.clone(), std::fs::read(&path).expect("source")));
    }
    aura_ingest::run(
        state.catalog(),
        &aura_ingest::contract::ingest::ImportPlan {
            import_id: ImportId::new(),
            project_id: project,
            roots: originals.iter().map(|(p, _)| p.clone()).collect(),
            mode: aura_ingest::contract::ingest::ImportMode::Reference,
            extensions: vec![],
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    let photos = crate::list_images(
        &state,
        &crate::contract::ipc::ListImagesInput {
            project_id: project.to_db(),
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("photos")
    .iter()
    .map(|p| p.id.clone())
    .collect::<Vec<_>>();
    for id in photos.iter().filter(|_| save_initial_recipes) {
        let photo = PhotoId::from_db(id).expect("photo");
        let recipe = crate::develop_commands::neutral_recipe(&state, photo);
        state
            .recipe_store()
            .save(&project, &photo, &recipe, &[], "Initial edit")
            .expect("recipe");
    }
    Fixture {
        directory,
        state,
        project,
        photos,
        originals,
    }
}
fn request(f: &Fixture, action: &str, snapshot: Option<&Snapshot>) -> Input {
    let mut hashes = if snapshot.is_some() {
        f.photos
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    aura_recipe::recipe_hash(
                        &crate::develop_commands::load_or_neutral(
                            &f.state,
                            PhotoId::from_db(id).expect("id"),
                        )
                        .expect("recipe"),
                    )
                    .expect("hash"),
                )
            })
            .collect()
    } else {
        BTreeMap::new()
    };
    if let Some(snapshot) = snapshot {
        for photo in &snapshot.photos {
            hashes.insert(photo.photo_id.clone(), photo.recipe_hash.clone());
        }
    }
    Input {
        project_id: f.project.to_db(),
        action: action.into(),
        photo_ids: f.photos.clone(),
        expected_revision: snapshot.map(|s| s.revision.clone()),
        recipe_hashes: hashes,
        ..Input::default()
    }
}
fn read(f: &Fixture) -> Snapshot {
    run(&f.state, &request(f, "read", None)).expect("workspace")
}
fn edited(f: &Fixture, id: &str, edit: impl FnOnce(&mut Recipe)) {
    let photo = PhotoId::from_db(id).expect("photo");
    let mut recipe = crate::develop_commands::load_or_neutral(&f.state, photo).expect("recipe");
    edit(&mut recipe);
    schema::Validation::check(&recipe).expect("valid");
    f.state
        .recipe_store()
        .save(
            &f.project,
            &photo,
            &recipe,
            &["global.exposure".into()],
            "Test edit",
        )
        .expect("save");
}
fn unchanged_sources(f: &Fixture) {
    for (path, bytes) in &f.originals {
        assert_eq!(&std::fs::read(path).expect("original"), bytes);
    }
}

#[test]
fn review_signoff_binds_to_recipe_and_workspace_persists() {
    let f = fixture();
    let mut snapshot = read(&f);
    let mut input = request(&f, "criteria", Some(&snapshot));
    input.text = Some("Colour\nDetail".into());
    snapshot = run(&f.state, &input).expect("criteria");
    for criterion in ["Colour", "Detail"] {
        let mut input = request(&f, "signoff", Some(&snapshot));
        input.criterion = Some(criterion.into());
        input.passed = true;
        snapshot = run(&f.state, &input).expect("signoff");
    }
    assert!(snapshot.photos.iter().all(|p| p.status == "approved"));
    let manifest = run(&f.state, &request(&f, "handoff", Some(&snapshot))).expect("manifest");
    assert!(manifest
        .document
        .expect("document")
        .contains("\"ready\":true"));
    let mut input = request(&f, "queue", Some(&snapshot));
    input.photo_ids.reverse();
    snapshot = run(&f.state, &input).expect("queue");
    assert_eq!(snapshot.queue.first(), f.photos.last());
    let mut input = request(&f, "assign", Some(&snapshot));
    input.reviewer = Some("Alex".into());
    snapshot = run(&f.state, &input).expect("assign");
    let mut input = request(&f, "schedule", Some(&snapshot));
    input.due_date = Some("2020-01-01".into());
    snapshot = run(&f.state, &input).expect("due");
    edited(&f, &f.photos[0], |r| r.global.exposure = 0.5);
    let current = read(&f);
    assert_eq!(current.photos[0].status, "changed_since_review");
    assert!(current.photos[0].overdue);
    assert!(run(&f.state, &request(&f, "verify", Some(&snapshot))).is_err());
    let saved = f.directory.path().join("catalog.aura");
    unchanged_sources(&f);
    drop(f.state);
    let reopened = AppState::open(&saved).expect("reopen");
    let mut input = Input {
        project_id: f.project.to_db(),
        action: "read".into(),
        photo_ids: f.photos,
        ..Input::default()
    };
    let result = run(&reopened, &input).expect("durable");
    assert_eq!(result.criteria, vec!["Colour", "Detail"]);
    assert!(result.reviews.values().all(|r| r.reviewer == "Alex"));
    assert!(result.audit.len() >= 5);
    input.action = "handoff".into();
    assert!(run(&reopened, &input)
        .expect("blocked manifest")
        .document
        .expect("document")
        .contains("\"ready\":false"));
}

#[test]
fn cohort_checkpoint_relative_transfer_dry_run_and_compare_and_save() {
    let f = fixture();
    let mut snapshot = read(&f);
    let mut input = request(&f, "checkpoint", Some(&snapshot));
    input.name = Some("Before client changes".into());
    snapshot = run(&f.state, &input).expect("checkpoint");
    let checkpoint = snapshot.checkpoints[0].id.clone();
    edited(&f, &f.photos[0], |r| {
        r.global.exposure = 0.7;
        r.global.contrast = 12;
    });
    edited(&f, &f.photos[1], |r| {
        r.global.exposure = -0.2;
        r.geometry.crop = [0.1, 0.1, 0.9, 0.9];
    });
    snapshot = read(&f);
    let original = snapshot.photos[1].recipe_hash.clone();
    let mut input = request(&f, "delta_plan", Some(&snapshot));
    input.checkpoint_id = Some(checkpoint.clone());
    input.sections = vec!["tone".into()];
    input.source_photo_id = Some(f.photos[0].clone());
    snapshot = run(&f.state, &input).expect("dry run");
    assert_eq!(snapshot.photos[1].recipe_hash, original);
    assert!(!snapshot.plan.as_ref().expect("plan").entries[0]
        .changed
        .is_empty());
    snapshot = run(&f.state, &request(&f, "apply_plan", Some(&snapshot))).expect("apply");
    let target = crate::develop_commands::load_or_neutral(
        &f.state,
        PhotoId::from_db(&f.photos[1]).expect("id"),
    )
    .expect("target");
    assert!((target.global.exposure - 0.5).abs() < 0.001);
    assert_eq!(target.global.contrast, 12);
    assert_eq!(target.geometry.crop, [0.1, 0.1, 0.9, 0.9]);
    assert_eq!(snapshot.rows.len(), 1);
    let mut input = request(&f, "drift", Some(&snapshot));
    input.checkpoint_id = Some(checkpoint.clone());
    let drift = run(&f.state, &input).expect("drift");
    assert!(drift.rows[1].changed.contains(&"tone".into()));
    assert!(drift.rows[1].changed.contains(&"geometry".into()));
    let mut input = request(&f, "restore_plan", Some(&snapshot));
    input.checkpoint_id = Some(checkpoint);
    input.sections = vec!["tone".into()];
    snapshot = run(&f.state, &input).expect("restore plan");
    edited(&f, &f.photos[1], |r| r.global.exposure = 1.0);
    assert!(run(&f.state, &request(&f, "apply_plan", Some(&snapshot))).is_err());
    let target = crate::develop_commands::load_or_neutral(
        &f.state,
        PhotoId::from_db(&f.photos[1]).expect("id"),
    )
    .expect("target");
    assert_eq!(target.global.exposure, 1.0);
    unchanged_sources(&f);
}

#[test]
fn dependency_gates_corrections_and_import_never_forges_approval() {
    let f = fixture();
    let mut snapshot = read(&f);
    let mut input = request(&f, "criteria", Some(&snapshot));
    input.text = Some("Detail".into());
    snapshot = run(&f.state, &input).expect("criteria");
    let mut input = request(&f, "issue", Some(&snapshot));
    input.photo_ids = vec![f.photos[0].clone()];
    input.recipe_hashes.retain(|id, _| id == &f.photos[0]);
    input.text = Some("Remove dust first".into());
    input.point = Some([0.2, 0.4]);
    snapshot = run(&f.state, &input).expect("issue");
    let first = snapshot.issues[0].id.clone();
    let mut input = request(&f, "issue", Some(&snapshot));
    input.photo_ids = vec![f.photos[0].clone()];
    input.depends_on = vec![first.clone()];
    input.text = Some("Check texture afterwards".into());
    snapshot = run(&f.state, &input).expect("dependency");
    let second = snapshot.issues[1].id.clone();
    let mut input = request(&f, "resolve_issue", Some(&snapshot));
    input.issue_id = Some(second.clone());
    input.passed = true;
    assert!(run(&f.state, &input).is_err());
    input.issue_id = Some(first.clone());
    snapshot = run(&f.state, &input).expect("resolve predecessor");
    let mut input = request(&f, "resolve_issue", Some(&snapshot));
    input.issue_id = Some(second);
    input.passed = true;
    snapshot = run(&f.state, &input).expect("resolve next");
    let mut input = request(&f, "propagate_issue", Some(&snapshot));
    input.photo_ids = f.photos.clone();
    input.recipe_hashes = read(&f)
        .photos
        .iter()
        .map(|p| (p.photo_id.clone(), p.recipe_hash.clone()))
        .collect();
    input.issue_id = Some(first);
    snapshot = run(&f.state, &input).expect("propagate");
    assert_eq!(snapshot.issues.last().expect("issue").point, None);
    assert!(!snapshot.issues.last().expect("issue").resolved);
    let export = run(&f.state, &request(&f, "export", Some(&snapshot)))
        .expect("export")
        .document
        .expect("doc");
    assert!(!export.contains("recipes"));
    assert!(!export.contains("rel_path"));
    let mut input = request(&f, "import", Some(&snapshot));
    input.document = Some(export);
    snapshot = run(&f.state, &input).expect("import");
    assert!(snapshot.issues.iter().all(|i| !i.resolved));
    assert!(snapshot.reviews.values().all(|r| r.passed.is_empty()));
    unchanged_sources(&f);
}

#[test]
fn baseline_merge_flags_divergence_without_overwriting_and_filters_find_changed_work() {
    let f = fixture();
    let mut snapshot = read(&f);
    let mut input = request(&f, "checkpoint", Some(&snapshot));
    input.name = Some("Base".into());
    snapshot = run(&f.state, &input).expect("base");
    let checkpoint = snapshot.checkpoints[0].id.clone();
    edited(&f, &f.photos[0], |r| r.global.exposure = 0.5);
    edited(&f, &f.photos[1], |r| r.global.exposure = -0.5);
    snapshot = read(&f);
    let mut input = request(&f, "merge_plan", Some(&snapshot));
    input.checkpoint_id = Some(checkpoint);
    input.source_photo_id = Some(f.photos[0].clone());
    input.sections = vec!["tone".into()];
    snapshot = run(&f.state, &input).expect("conflicted merge");
    assert_eq!(
        snapshot.plan.as_ref().expect("plan").entries[0].conflicts,
        vec!["tone"]
    );
    assert!(run(&f.state, &request(&f, "apply_plan", Some(&snapshot))).is_err());
    let mut input = request(&f, "compare", Some(&snapshot));
    input.winner = Some("tie".into());
    snapshot = run(&f.state, &input).expect("pair");
    let tied = run(&f.state, &request(&f, "undecided", Some(&snapshot))).expect("ties");
    assert_eq!(tied.selected.len(), 2);
    let mut input = request(&f, "filter", Some(&snapshot));
    input.filter = Some(SavedFilter {
        name: "Unreviewed".into(),
        state: "unreviewed".into(),
        ..SavedFilter::default()
    });
    snapshot = run(&f.state, &input).expect("filter");
    let mut input = request(&f, "apply_filter", Some(&snapshot));
    input.name = Some("Unreviewed".into());
    assert_eq!(run(&f.state, &input).expect("matches").selected.len(), 2);
    unchanged_sources(&f);
}

#[test]
fn stale_metadata_foreign_membership_bounds_and_bad_dates_are_rejected() {
    let f = fixture();
    let snapshot = read(&f);
    let mut input = request(&f, "queue", Some(&snapshot));
    let newer = run(&f.state, &input).expect("queue");
    assert_ne!(snapshot.revision, newer.revision);
    assert!(run(&f.state, &input).is_err());
    input = request(&f, "schedule", Some(&newer));
    input.due_date = Some("2026-02-30".into());
    assert!(run(&f.state, &input).is_err());
    input = request(&f, "read", None);
    input.photo_ids.push(PhotoId::new().to_db());
    assert!(run(&f.state, &input).is_err());
    input = request(&f, "queue", Some(&newer));
    input.photo_ids = vec![f.photos[0].clone(); 33];
    assert!(run(&f.state, &input).is_err());
    input = request(&f, "import", Some(&newer));
    input.document = Some("x".repeat(256 * 1024 + 1));
    assert!(run(&f.state, &input).is_err());
    assert_eq!(read(&f).revision, newer.revision);
    unchanged_sources(&f);
}

#[test]
fn merge_uses_each_targets_own_checkpoint_baseline() {
    let f = fixture();
    edited(&f, &f.photos[0], |r| r.global.exposure = 0.4);
    edited(&f, &f.photos[1], |r| r.global.exposure = -0.3);
    let mut snapshot = read(&f);
    let mut input = request(&f, "checkpoint", Some(&snapshot));
    input.name = Some("Distinct photo baselines".into());
    snapshot = run(&f.state, &input).expect("checkpoint");
    let checkpoint = snapshot.checkpoints[0].id.clone();
    edited(&f, &f.photos[0], |r| r.global.exposure = 0.9);
    snapshot = read(&f);
    let mut input = request(&f, "merge_plan", Some(&snapshot));
    input.checkpoint_id = Some(checkpoint);
    input.source_photo_id = Some(f.photos[0].clone());
    input.sections = vec!["tone".into()];
    snapshot = run(&f.state, &input).expect("merge preview");
    assert!(snapshot.plan.as_ref().expect("plan").entries[0]
        .conflicts
        .is_empty());
    snapshot = run(&f.state, &request(&f, "apply_plan", Some(&snapshot))).expect("merge apply");
    assert_eq!(snapshot.rows.len(), 1);
    let target = crate::develop_commands::load_or_neutral(
        &f.state,
        PhotoId::from_db(&f.photos[1]).expect("target"),
    )
    .expect("recipe");
    assert_eq!(target.global.exposure, 0.9);
    unchanged_sources(&f);
}

#[test]
fn relative_tone_transfers_presence_controls_and_rollback_binds_checkpoint() {
    let f = fixture();
    let mut snapshot = read(&f);
    let mut input = request(&f, "checkpoint", Some(&snapshot));
    input.name = Some("Before".into());
    snapshot = run(&f.state, &input).expect("checkpoint");
    let original = snapshot.checkpoints[0].id.clone();
    edited(&f, &f.photos[0], |r| {
        r.global.clarity = 12;
        r.global.texture = -18;
        r.global.dehaze = 21;
    });
    snapshot = read(&f);
    let mut input = request(&f, "checkpoint", Some(&snapshot));
    input.name = Some("Other checkpoint".into());
    snapshot = run(&f.state, &input).expect("second checkpoint");
    let other = snapshot.checkpoints[1].id.clone();
    let mut input = request(&f, "delta_plan", Some(&snapshot));
    input.checkpoint_id = Some(original.clone());
    input.source_photo_id = Some(f.photos[0].clone());
    input.sections = vec!["tone".into()];
    snapshot = run(&f.state, &input).expect("relative preview");
    snapshot = run(&f.state, &request(&f, "apply_plan", Some(&snapshot))).expect("relative apply");
    let target = crate::develop_commands::load_or_neutral(
        &f.state,
        PhotoId::from_db(&f.photos[1]).expect("target"),
    )
    .expect("recipe");
    assert_eq!(
        (
            target.global.clarity,
            target.global.texture,
            target.global.dehaze
        ),
        (12, -18, 21)
    );
    let mut input = request(&f, "rollback_plan", Some(&snapshot));
    input.photo_ids = vec![f.photos[1].clone()];
    input.checkpoint_id = Some(other);
    input.sections = vec!["tone".into()];
    assert!(run(&f.state, &input).is_err());
    input.checkpoint_id = Some(original);
    snapshot = run(&f.state, &input).expect("original rollback");
    assert!(snapshot.plan.as_ref().expect("rollback plan").entries[0]
        .conflicts
        .is_empty());
    unchanged_sources(&f);
}

#[test]
fn reopening_prerequisite_reopens_dependents_and_revokes_checklist_signoff() {
    let f = fixture();
    let mut snapshot = read(&f);
    let mut input = request(&f, "criteria", Some(&snapshot));
    input.text = Some("Detail".into());
    snapshot = run(&f.state, &input).expect("criteria");
    let mut ids = Vec::new();
    for text in ["Dust", "Texture", "Final check"] {
        let mut input = request(&f, "issue", Some(&snapshot));
        input.photo_ids = vec![f.photos[0].clone()];
        input.text = Some(text.into());
        input.depends_on = ids.last().cloned().into_iter().collect();
        snapshot = run(&f.state, &input).expect("issue");
        ids.push(snapshot.issues.last().expect("created issue").id.clone());
    }
    for id in &ids {
        let mut input = request(&f, "resolve_issue", Some(&snapshot));
        input.issue_id = Some(id.clone());
        input.passed = true;
        snapshot = run(&f.state, &input).expect("resolved");
    }
    let mut input = request(&f, "signoff", Some(&snapshot));
    input.criterion = Some("Detail".into());
    input.passed = true;
    snapshot = run(&f.state, &input).expect("signoff");
    assert!(snapshot.photos.iter().all(|p| p.status == "approved"));
    let mut input = request(&f, "resolve_issue", Some(&snapshot));
    input.issue_id = Some(ids[0].clone());
    snapshot = run(&f.state, &input).expect("reopen");
    assert!(snapshot.issues.iter().all(|issue| !issue.resolved));
    assert!(snapshot.reviews[&f.photos[0]].passed.is_empty());
    assert_eq!(snapshot.photos[0].status, "changes_requested");
    unchanged_sources(&f);
}

#[test]
fn transfer_requires_source_and_target_and_import_rejects_duplicate_metadata() {
    let f = fixture();
    let mut snapshot = read(&f);
    let mut input = request(&f, "checkpoint", Some(&snapshot));
    input.name = Some("Baseline".into());
    snapshot = run(&f.state, &input).expect("checkpoint");
    for action in ["delta_plan", "merge_plan"] {
        let mut input = request(&f, action, Some(&snapshot));
        input.photo_ids = vec![f.photos[0].clone()];
        input.source_photo_id = Some(f.photos[0].clone());
        input.sections = vec!["tone".into()];
        input.checkpoint_id = Some(snapshot.checkpoints[0].id.clone());
        assert!(run(&f.state, &input).is_err());
    }
    let document = run(&f.state, &request(&f, "export", Some(&snapshot)))
        .expect("export")
        .document
        .expect("document");
    let mut document: serde_json::Value = serde_json::from_str(&document).expect("json");
    document["criteria"] = serde_json::json!(["Detail", "Detail"]);
    let mut input = request(&f, "import", Some(&snapshot));
    input.document = Some(document.to_string());
    assert!(run(&f.state, &input).is_err());
    assert_eq!(read(&f).revision, snapshot.revision);
    unchanged_sources(&f);
}

#[test]
fn plan_saves_first_edit_for_a_freshly_imported_photograph() {
    let f = fixture_with_recipes(false);
    let target = PhotoId::from_db(&f.photos[1]).expect("target");
    assert!(f
        .state
        .recipe_store()
        .load(&target)
        .expect("unsaved target")
        .is_none());
    let mut snapshot = read(&f);
    let mut input = request(&f, "checkpoint", Some(&snapshot));
    input.name = Some("Imported originals".into());
    snapshot = run(&f.state, &input).expect("checkpoint without saved recipes");
    let checkpoint = snapshot.checkpoints[0].id.clone();
    edited(&f, &f.photos[0], |r| r.global.exposure = 0.6);
    snapshot = read(&f);
    let mut input = request(&f, "delta_plan", Some(&snapshot));
    input.checkpoint_id = Some(checkpoint);
    input.source_photo_id = Some(f.photos[0].clone());
    input.sections = vec!["tone".into()];
    snapshot = run(&f.state, &input).expect("prepare first edit");
    snapshot =
        run(&f.state, &request(&f, "apply_plan", Some(&snapshot))).expect("apply first edit");
    assert_eq!(snapshot.rows.len(), 1);
    assert!(snapshot.rows[0].conflicts.is_empty());
    let saved = f
        .state
        .recipe_store()
        .load(&target)
        .expect("target load")
        .expect("first saved recipe");
    assert_eq!(saved.global.exposure, 0.6);
    let saved_hash = aura_recipe::recipe_hash(&saved).expect("saved hash");
    for action in ["undo", "redo"] {
        crate::develop_commands::history_step(
            &f.state,
            &crate::contract::ipc::HistoryStepInput {
                project_id: f.project.to_db(),
                photo_id: target.to_db(),
                action: action.into(),
            },
        )
        .expect("first-save history step");
        let current =
            crate::develop_commands::load_or_neutral(&f.state, target).expect("history recipe");
        if action == "undo" {
            assert_eq!(current.global.exposure, 0.0);
        } else {
            assert_eq!(
                aura_recipe::recipe_hash(&current).expect("redo hash"),
                saved_hash
            );
        }
    }
    unchanged_sources(&f);
}

#[test]
fn legacy_plan_encoding_preserves_workspace_revision() {
    let legacy = r#"{"kind":"restore_plan","source":null,"entries":[]}"#;
    let plan: Plan = serde_json::from_str(legacy).expect("legacy plan");
    assert!(plan.checkpoint_id.is_none());
    assert_eq!(encode(&plan).expect("legacy encoding"), legacy);
}
