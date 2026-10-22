//! Persist each new colour/spatial method through the application and real renderer.
#![allow(clippy::expect_used, clippy::too_many_lines)]
use aura_app::{
    contract::ipc::{CreateProjectInput, HistoryStepInput, ListImagesInput},
    AppState,
};
use aura_core::{
    progress::{CancelToken, NullProgress},
    ImportId, PhotoId, ProjectId,
};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
const SPATIAL: [&str; 25] = [
    "anisotropic_diffusion",
    "total_variation",
    "non_local_means",
    "vector_medoid",
    "trimmed_mean",
    "adaptive_wiener",
    "haar_denoise",
    "haar_detail",
    "laplacian_detail",
    "kuwahara",
    "tensor_flow",
    "block_deblock",
    "aperture_bokeh",
    "starburst",
    "rotation_blur",
    "trajectory_blur",
    "harmonic_fill",
    "poisson_donor",
    "channel_register",
    "morphology_clean",
    "local_rank_contrast",
    "mosquito_suppress",
    "edge_directed_smooth",
    "donor_bilateral",
    "texture_quilt",
];
const COLOUR: [&str; 25] = [
    "spatial_neutral_bridge",
    "tonal_neutral_bridge",
    "chroma_reject",
    "palette_axis",
    "decorrelation_stretch",
    "principal_chroma_suppress",
    "soft_gamut",
    "luma_chroma_compress",
    "chroma_cusp",
    "neutral_exclusion",
    "histogram_equalize",
    "clipped_equalize",
    "log_equalize",
    "rank_gaussian",
    "bimodal_separate",
    "percentile_remap",
    "photographic_map",
    "asinh_map",
    "log_toe_shoulder",
    "local_log_contrast",
    "retinex_ratio",
    "chroma_edge_protect",
    "chroma_winsorize",
    "opponent_contrast",
    "sampled_replace",
];
#[test]
fn all_fifty_selected_image_methods_persist_render_and_reverse_without_touching_source() {
    let dir = tempfile::tempdir().expect("isolated");
    let path = dir.path().join("chart.png");
    image::RgbImage::from_fn(48, 36, |x, y| {
        image::Rgb([
            (45 + x * 3 + (y % 3) * 8) as u8,
            (40 + y * 4 + (x % 3) * 6) as u8,
            (30 + x * 2 + y * 2) as u8,
        ])
    })
    .save(&path)
    .expect("chart");
    let source = std::fs::read(&path).expect("source bytes");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("catalog")
        .with_cache_root(&dir.path().join("cache"));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Image lab methods".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    let project_id = ProjectId::from_db(&project.id).expect("id");
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
    .expect("import");
    let rows = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("rows");
    let photo = PhotoId::from_db(&rows.first().expect("photo").id).expect("photo id");
    let mut baseline = aura_app::develop_commands::neutral_recipe(&state, photo);
    let masks=serde_json::from_str(r#"[{"id":"selection","name":"Left material","enabled":true,"amount":0.8,"components":[{"mode":"add","source":{"type":"radial","centre":[0.45,0.5],"radii":[0.3,0.35],"angle":0,"feather":0.2}}],"params":{}}]"#).expect("mask fixture");
    baseline
        .extra
        .insert(aura_recipe::local_masks::KEY.into(), masks);
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
            .expect("native render")
            .rgb_base64
    };
    let original = render();
    for (group, tools) in [("spatial", SPATIAL), ("colour", COLOUR)] {
        for tool in tools {
            let current = state
                .recipe_store()
                .load(&photo)
                .expect("load")
                .expect("recipe");
            let hash = aura_recipe::recipe_hash(&current).expect("hash");
            let before = state
                .recipe_store()
                .history(&photo, baseline.clone())
                .expect("history")
                .entries()
                .len();
            if group == "spatial" {
                let operation = aura_recipe::spatial_lab::Operation {
                    id: "one".into(),
                    mask_id: "selection".into(),
                    tool: serde_json::from_value(serde_json::Value::String(tool.into()))
                        .expect("spatial tool"),
                    enabled: true,
                    amount: 0.7,
                    radius: 0.08,
                    threshold: 0.1,
                    detail: 0.6,
                    angle: 90.0,
                    source: [0.2, 0.2],
                    shift: [0.03, 0.02],
                    path: vec![[-0.03, 0.0], [0.03, 0.02]],
                };
                let input = aura_app::spatial_lab_commands::Input {
                    project_id: project.id.clone(),
                    photo_id: photo.to_db(),
                    recipe_hash: hash.clone(),
                    operations: vec![operation],
                };
                let saved =
                    aura_app::spatial_lab_commands::save(&state, &input).expect("save spatial");
                assert!(
                    aura_app::spatial_lab_commands::save(&state, &input).is_err(),
                    "stale {tool}"
                );
                let mut same = input.clone();
                same.recipe_hash = saved.recipe.recipe_hash.clone();
                assert_eq!(
                    aura_app::spatial_lab_commands::save(&state, &same)
                        .expect("unchanged spatial save")
                        .recipe
                        .recipe_hash,
                    saved.recipe.recipe_hash
                );
            } else {
                let operation = aura_recipe::color_lab::Operation {
                    id: "one".into(),
                    mask_id: "selection".into(),
                    tool: serde_json::from_value(serde_json::Value::String(tool.into()))
                        .expect("colour tool"),
                    enabled: true,
                    amount: 0.7,
                    strength: 0.7,
                    radius: 0.03,
                    pivot: 0.18,
                    width: 0.2,
                    source: [0.2, 0.3],
                    second: [0.8, 0.7],
                    target: [0.2, 0.3, 0.1],
                };
                let input = aura_app::color_lab_commands::Input {
                    project_id: project.id.clone(),
                    photo_id: photo.to_db(),
                    recipe_hash: hash.clone(),
                    operations: vec![operation],
                };
                let saved =
                    aura_app::color_lab_commands::save(&state, &input).expect("save colour");
                assert!(
                    aura_app::color_lab_commands::save(&state, &input).is_err(),
                    "stale {tool}"
                );
                let mut same = input.clone();
                same.recipe_hash = saved.recipe.recipe_hash.clone();
                assert_eq!(
                    aura_app::color_lab_commands::save(&state, &same)
                        .expect("unchanged colour save")
                        .recipe
                        .recipe_hash,
                    saved.recipe.recipe_hash
                );
            }
            assert_eq!(
                state
                    .recipe_store()
                    .history(&photo, baseline.clone())
                    .expect("history")
                    .entries()
                    .len(),
                (before + 1).min(aura_recipe::history::MAX_ENTRIES),
                "one save {tool}"
            );
            let history = state
                .recipe_store()
                .history(&photo, baseline.clone())
                .expect("saved history");
            let entry = history.entries().last().expect("one method entry");
            let key = if group == "spatial" {
                aura_recipe::spatial_lab::KEY
            } else {
                aura_recipe::color_lab::KEY
            };
            assert_eq!(
                entry.changed,
                vec![key.to_string()],
                "one changed path {tool}"
            );
            assert_eq!(entry.source, aura_recipe::EditSource::User);
            assert!(entry
                .recipe
                .provenance
                .user_edited_fields
                .iter()
                .any(|path| path == key));
            // A later automated proposal cannot erase a photographer's lab stack.
            let mut automatic = entry.recipe.clone();
            automatic.extra.insert(key.into(), serde_json::json!([]));
            let (protected, report) =
                aura_recipe::schema::merge(&entry.recipe, &automatic, aura_recipe::EditSource::Ai)
                    .expect("automated merge");
            assert_eq!(protected.extra.get(key), entry.recipe.extra.get(key));
            assert_eq!(report.refused, vec![key.to_string()]);
            let edited = render();
            let mut step = HistoryStepInput {
                project_id: project.id.clone(),
                photo_id: photo.to_db(),
                action: "undo".into(),
            };
            aura_app::history_step(&state, &step).expect("undo");
            assert_eq!(render(), original, "undo {tool}");
            step.action = "redo".into();
            aura_app::history_step(&state, &step).expect("redo");
            assert_eq!(render(), edited, "redo {tool}");
            step.action = "undo".into();
            aura_app::history_step(&state, &step).expect("restore baseline");
        }
    }
    assert_eq!(std::fs::read(path).expect("source after all50"), source);
}
