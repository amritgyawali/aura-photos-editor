//! Native colour authoring: measurements, pixels, history, stale writes and original safety.
#![allow(clippy::expect_used, clippy::too_many_lines, clippy::float_cmp)]
use aura_app::contract::ipc::{CreateProjectInput, HistoryStepInput, ListImagesInput};
use aura_app::studio_color_commands::{self, MeasureStudioColorInput, SaveStudioColorInput};
use aura_core::{
    progress::{CancelToken, NullProgress},
    ImportId, PhotoId, ProjectId,
};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};

#[test]
fn colour_drafts_are_measured_then_saved_atomically_with_reversible_native_pixels() {
    let directory = tempfile::tempdir().expect("isolated colour workflow");
    let path = directory.path().join("colour.png");
    image::RgbImage::from_fn(64, 48, |x, y| {
        image::Rgb([(40 + x * 2) as u8, (35 + y * 3) as u8, (30 + x + y) as u8])
    })
    .save(&path)
    .expect("photo");
    let original = std::fs::read(&path).expect("original");
    let state = aura_app::AppState::open(&directory.path().join("catalog.aura"))
        .expect("catalog")
        .with_cache_root(&directory.path().join("cache"));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Colour tools".into(),
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
    baseline.geometry.crop = [0.1, 0.1, 0.95, 0.9];
    let mask: serde_json::Value = serde_json::from_str(r#"[{"id":"manual","name":"Hand light","enabled":true,"amount":0.4,"components":[{"mode":"add","source":{"type":"linear","start":[0.0,0.0],"end":[1.0,0.0]}}],"params":{"exposure":0.1}}]"#).expect("manual mask fixture");
    baseline
        .extra
        .insert(aura_recipe::local_masks::KEY.into(), mask.clone());
    state
        .recipe_store()
        .save(
            &project_id,
            &photo,
            &baseline,
            &[aura_recipe::local_masks::KEY.into(), "geometry.crop".into()],
            "Manual mask and crop",
        )
        .expect("baseline");
    let hash = aura_recipe::recipe_hash(&baseline).expect("hash");
    let count = || {
        state
            .recipe_store()
            .history(&photo, baseline.clone())
            .expect("history")
            .entries()
            .len()
    };
    let render = || {
        aura_app::native_retouch::preview(&state, &project.id, &photo.to_db(), false)
            .expect("preview")
            .rgb_base64
    };
    let original_pixels = render();
    let mut measure = MeasureStudioColorInput {
        project_id: project.id.clone(),
        photo_id: photo.to_db(),
        recipe_hash: hash.clone(),
        kind: "auto".into(),
        channel: "rgb".into(),
        x: 0.2,
        y: 0.2,
    };
    let measured = studio_color_commands::measure(&state, &measure).expect("measured levels");
    assert!(measured.color.levels.rgb.input_white > measured.color.levels.rgb.input_black);
    assert_eq!(
        count(),
        1,
        "proposals never add history after the manual baseline"
    );
    measure.kind = "black".into();
    let black = studio_color_commands::measure(&state, &measure).expect("black patch");
    assert!(black.color.levels.rgb.input_black > 0.0);
    measure.kind = "white".into();
    let white = studio_color_commands::measure(&state, &measure).expect("white patch");
    assert!(white.color.levels.rgb.input_white < 1.0);
    let mut color = measured.color;
    color.levels.rgb.gamma = 1.2;
    let mut input = SaveStudioColorInput {
        project_id: project.id.clone(),
        photo_id: photo.to_db(),
        recipe_hash: hash.clone(),
        color,
    };
    let saved = studio_color_commands::save(&state, &input).expect("atomic save");
    assert_eq!(count(), 2);
    let edited = render();
    assert_ne!(edited, original_pixels);
    let stored = state
        .recipe_store()
        .load(&photo)
        .expect("read")
        .expect("saved");
    assert_eq!(
        aura_recipe::local_masks::read(&stored).expect("saved manual masks"),
        aura_recipe::local_masks::read(&baseline).expect("original manual masks")
    );
    assert_eq!(stored.geometry, baseline.geometry);
    assert!(
        studio_color_commands::save(&state, &input).is_err(),
        "stale draft is refused"
    );
    assert_eq!(count(), 2);
    measure.kind = "auto".into();
    assert!(
        studio_color_commands::measure(&state, &measure).is_err(),
        "stale measurement is refused"
    );
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
    input.recipe_hash = saved.recipe.recipe_hash;
    input.color = aura_recipe::studio_color::StudioColor::default();
    studio_color_commands::save(&state, &input).expect("reset colours");
    assert_eq!(render(), original_pixels);
    assert_eq!(std::fs::read(path).expect("original after edit"), original);
}
