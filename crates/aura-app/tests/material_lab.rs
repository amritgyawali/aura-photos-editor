//! Native material stack persistence, rendering, reversible history, revision and source safety.
#![allow(clippy::expect_used, clippy::too_many_lines)]
use aura_app::{
    contract::ipc::{CreateProjectInput, HistoryStepInput, ListImagesInput},
    material_commands::{self, Input},
};
use aura_core::{
    progress::{CancelToken, NullProgress},
    ImportId, PhotoId, ProjectId,
};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_recipe::material_lab::{Operation, Tool};

#[test]
fn material_stack_and_inspection_use_native_pixels_and_reversible_atomic_history() {
    let directory = tempfile::tempdir().expect("isolated material workflow");
    let path = directory.path().join("material.png");
    image::RgbImage::from_fn(64, 48, |x, y| {
        image::Rgb([
            160 + (x % 8) as u8,
            150 + (y % 8) as u8,
            130 + (x % 4) as u8,
        ])
    })
    .save(&path)
    .expect("fixture");
    let original = std::fs::read(&path).expect("original bytes");
    let db = directory.path().join("catalog.aura");
    let state = aura_app::AppState::open(&db)
        .expect("catalog")
        .with_cache_root(&directory.path().join("cache"));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Materials".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    let project_id = ProjectId::from_db(&project.id).expect("project id");
    aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id,
            roots: vec![path.clone()],
            mode: ImportMode::Reference,
            extensions: vec![],
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("ingest");
    let images = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("photos");
    let photo = PhotoId::from_db(&images.first().expect("photo").id).expect("photo id");
    let mut baseline = aura_app::develop_commands::neutral_recipe(&state, photo);
    let mask = serde_json::from_str(r#"[{"id":"material","name":"Selected material","enabled":true,"amount":1.0,"components":[{"mode":"add","source":{"type":"radial","centre":[0.5,0.5],"radii":[0.4,0.4],"angle":0,"feather":0.2}}],"params":{}}]"#).expect("mask fixture");
    baseline
        .extra
        .insert(aura_recipe::local_masks::KEY.into(), mask);
    state
        .recipe_store()
        .save(
            &project_id,
            &photo,
            &baseline,
            &[aura_recipe::local_masks::KEY.into()],
            "Manual selection",
        )
        .expect("baseline");
    let render = || {
        aura_app::native_retouch::preview(&state, &project.id, &photo.to_db(), false)
            .expect("native preview")
            .rgb_base64
    };
    let original_pixels = render();
    let hash = aura_recipe::recipe_hash(&baseline).expect("hash");
    let mut input = Input {
        project_id: project.id.clone(),
        photo_id: photo.to_db(),
        recipe_hash: hash.clone(),
        operations: vec![Operation {
            id: "one".into(),
            mask_id: "material".into(),
            tool: Tool::SilverHair,
            enabled: true,
            amount: 0.9,
            radius: 0.05,
            angle: 0.0,
            threshold: 0.02,
            detail: 1.0,
            coarse: 0.25,
        }],
    };
    let saved = material_commands::save(&state, &input).expect("atomic save");
    let edited = render();
    assert_ne!(edited, original_pixels);
    let count = || {
        state
            .recipe_store()
            .history(&photo, baseline.clone())
            .expect("history")
            .entries()
            .len()
    };
    assert_eq!(count(), 2);
    let unchanged_input = Input {
        recipe_hash: saved.recipe.recipe_hash.clone(),
        ..input.clone()
    };
    let unchanged = material_commands::save(&state, &unchanged_input).expect("same saved stack");
    assert_eq!(unchanged.recipe.recipe_hash, saved.recipe.recipe_hash);
    assert_eq!(
        count(),
        2,
        "unchanged decoded controls must not append history"
    );
    let mut rounded_input = unchanged_input;
    rounded_input.operations[0].amount += 0.000_000_1;
    let rounded = material_commands::save(&state, &rounded_input).expect("canonical same stack");
    assert_eq!(rounded.recipe.recipe_hash, saved.recipe.recipe_hash);
    assert_eq!(
        count(),
        2,
        "canonical-equivalent controls must not append history"
    );
    let saved_history = state
        .recipe_store()
        .history(&photo, baseline.clone())
        .expect("saved history");
    assert_eq!(
        saved_history
            .entries()
            .last()
            .expect("material step")
            .changed,
        vec![aura_recipe::material_lab::KEY.to_owned()]
    );
    assert!(material_commands::save(&state, &input).is_err());
    assert_eq!(count(), 2);
    let mut step = HistoryStepInput {
        project_id: project.id.clone(),
        photo_id: photo.to_db(),
        action: "undo".into(),
    };
    aura_app::history_step(&state, &step).expect("undo");
    assert_eq!(render(), original_pixels);
    step.action = "redo".into();
    aura_app::history_step(&state, &step).expect("redo");
    assert_eq!(render(), edited);
    let loaded = state
        .recipe_store()
        .load(&photo)
        .expect("load")
        .expect("recipe");
    assert_eq!(
        aura_recipe::local_masks::read(&loaded).expect("mask"),
        aura_recipe::local_masks::read(&baseline).expect("original mask")
    );
    assert_eq!(
        aura_recipe::material_lab::read(&loaded).expect("operations"),
        input.operations
    );
    let inspection = aura_app::photo_analysis::Input {
        project_id: project.id.clone(),
        photo_id: photo.to_db(),
        recipe_hash: saved.recipe.recipe_hash.clone(),
    };
    let after_history = count();
    let report = aura_app::photo_analysis::inspect(&state, &inspection).expect("inspection");
    assert_eq!(report.diagnostics.len(), 95);
    let diagnostic_ids: std::collections::BTreeSet<_> =
        report.diagnostics.iter().map(|tool| &tool.id).collect();
    assert_eq!(diagnostic_ids.len(), 95);
    assert_eq!(count(), after_history);
    assert_eq!(render(), edited);
    let stale = aura_app::photo_analysis::Input {
        recipe_hash: hash,
        ..inspection
    };
    assert!(aura_app::photo_analysis::inspect(&state, &stale).is_err());
    input.recipe_hash = saved.recipe.recipe_hash;
    input.operations[0].mask_id = "missing".into();
    assert!(material_commands::save(&state, &input).is_err());
    assert_eq!(count(), after_history);
    input.operations.clear();
    material_commands::save(&state, &input).expect("clear stack");
    assert_eq!(render(), original_pixels);
    drop(state);
    let reopened = aura_app::AppState::open(&db).expect("reopen");
    assert!(aura_recipe::material_lab::read(
        &reopened
            .recipe_store()
            .load(&photo)
            .expect("load reopened")
            .expect("reopened recipe")
    )
    .expect("reopened stack")
    .is_empty());
    assert_eq!(std::fs::read(path).expect("source unchanged"), original);
}
