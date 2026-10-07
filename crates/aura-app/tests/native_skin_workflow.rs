//! Real portrait import, selection, rendering and repeat retouch through the native commands.
//! Set AURA_SKIN_PORTRAIT to a photograph with a visible face and body skin, then run ignored.
//! The catalog, caches and credentials are isolated from the running desktop application.
#![allow(clippy::expect_used, clippy::too_many_lines)]
use aura_app::contract::ipc::{CreateProjectInput, DevelopImageInput, ListImagesInput};
use aura_app::native_retouch::{self, DraftInput, RetouchInput};
use aura_app::portrait_features::{Options, Scope};
use aura_app::smart_edit::{auto_retouch, AutoRetouchInput};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_recipe::retouch_tools;
use std::sync::Arc;

#[test]
#[ignore = "requires AURA_SKIN_PORTRAIT pointing at a real portrait"]
fn manual_skin_selection_survives_repeated_native_retouch() {
    let path =
        std::path::PathBuf::from(std::env::var_os("AURA_SKIN_PORTRAIT").expect("portrait path"));
    let original = std::fs::read(&path).expect("read source photograph");
    let dir = tempfile::tempdir().expect("isolated catalog directory");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("open isolated catalog")
        .with_cache_root(&dir.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Skin workflow regression".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("create project");
    let imported = aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("project id"),
            roots: vec![path.clone()],
            mode: ImportMode::Reference,
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import portrait");
    assert_eq!(imported.files_imported, 1);
    let photos = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("list imported photograph");
    let photo = photos.first().expect("one photo").id.clone();
    let request = AutoRetouchInput {
        project_id: project.id.clone(),
        photo_id: photo.clone(),
        global: false,
        options: Options {
            scope: Scope::FaceAndBody,
            ..Options::default()
        },
    };
    let mut input = RetouchInput {
        project_id: project.id.clone(),
        photo_id: photo.clone(),
        action: "list".into(),
        edits: Vec::new(),
        id: None,
    };
    auto_retouch(&state, &request).expect("first automatic pass");
    let stack = native_retouch::edit(&state, &input).expect("read automatic steps");
    assert!(stack.iter().any(|e| e.id.ends_with("-body-texture")));
    let mut skin = stack
        .iter()
        .find(|e| e.id.ends_with("-texture") && !e.id.contains("-body-"))
        .expect("face skin operation")
        .clone();
    let original_id = skin.id.clone();
    let rendered =
        native_retouch::preview(&state, &project.id, &photo, false).expect("render retouch");
    let before =
        native_retouch::preview(&state, &project.id, &photo, true).expect("render original");
    assert_eq!(
        (rendered.width, rendered.height),
        (before.width, before.height)
    );
    assert_ne!(rendered.rgb_base64, before.rgb_base64);
    skin.amount = 0.27;
    skin.enabled = false;
    input.action = "update".into();
    input.edits = vec![skin];
    let updated = native_retouch::edit(&state, &input).expect("preserve manual override");
    let manual_id = format!("manual-{original_id}");
    let manual = updated
        .iter()
        .find(|e| e.id == manual_id)
        .expect("manual step")
        .clone();
    assert!(manual
        .matte
        .as_deref()
        .expect("manual mask")
        .starts_with("manual-"));
    let mut selection = DraftInput {
        project_id: project.id.clone(),
        photo_id: photo.clone(),
        edit: manual.clone(),
        replace_id: Some(manual.id.clone()),
        quality: None,
    };
    selection.edit.enabled = true;
    let mask_before =
        native_retouch::selection_preview(&state, &selection).expect("selection before");
    let recipe_input = DevelopImageInput {
        photo_id: photo.clone(),
    };
    let read_recipe = || {
        let dto = aura_app::image_recipe(&state, &recipe_input).expect("saved recipe");
        serde_json::from_str::<aura_recipe::Recipe>(&dto.body).expect("recipe JSON")
    };
    let saved_mattes = retouch_tools::read_mattes(&read_recipe()).expect("saved masks");
    for _ in 0..2 {
        auto_retouch(&state, &request).expect("repeat automatic pass after manual adjustment");
        input.action = "list".into();
        input.edits.clear();
        let repeated = native_retouch::edit(&state, &input).expect("read repeated steps");
        assert_eq!(repeated.iter().find(|e| e.id == manual.id), Some(&manual));
        assert!(!repeated.iter().any(|e| e.id == original_id));
        let matte_id = manual.matte.as_ref().expect("manual mask id");
        let mattes = retouch_tools::read_mattes(&read_recipe()).expect("repeated masks");
        assert_eq!(mattes.get(matte_id), saved_mattes.get(matte_id));
        let mask_after =
            native_retouch::selection_preview(&state, &selection).expect("selection after");
        assert_eq!(mask_after.rgb_base64, mask_before.rgb_base64);
    }
    let before_failure = read_recipe();
    input.action = "append".into();
    let mut broken = manual;
    broken.matte = Some("missing-skin-mask".into());
    input.edits = vec![broken];
    assert!(native_retouch::edit(&state, &input).is_err());
    assert_eq!(
        aura_recipe::recipe_hash(&read_recipe()).expect("after hash"),
        aura_recipe::recipe_hash(&before_failure).expect("before hash")
    );
    assert_eq!(
        std::fs::read(&path).expect("source after retouch"),
        original
    );
}
