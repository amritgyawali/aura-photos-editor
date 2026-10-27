#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::too_many_lines
)]
use super::plane::Plane;
use super::*;
use aura_recipe::local_masks::LocalMask;
use std::collections::BTreeMap;
mod laboratory;

fn plane(alpha: &[u8], width: usize) -> Plane {
    Plane {
        width,
        height: alpha.len() / width,
        alpha: alpha.to_vec(),
    }
}
fn edit(source: &[u8], width: usize, action: Action, value: u32) -> Vec<u8> {
    let mut plane = plane(source, width);
    plane
        .edit(action, value, None, &CancelToken::new())
        .expect("edit selection");
    plane.alpha
}
#[test]
fn inversion_is_of_the_composed_plane_and_full_frame_not_each_component() {
    let mut recipe = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "test");
    let mask = LocalMask {
        id: "selection".into(),
        name: "Selection".into(),
        amount: 0.3,
        enabled: true,
        params: aura_recipe::MaskParams::default(),
        components: vec![
            Component {
                mode: Mode::Add,
                invert: false,
                source: Source::Radial {
                    centre: [0.5, 0.5],
                    radii: [0.4, 0.4],
                    angle: 0.0,
                    feather: 0.0,
                },
            },
            Component {
                mode: Mode::Subtract,
                invert: false,
                source: Source::Radial {
                    centre: [0.5, 0.5],
                    radii: [0.15, 0.15],
                    angle: 0.0,
                    feather: 0.0,
                },
            },
        ],
    };
    local_masks::write(&mut recipe, &[mask], &BTreeMap::new()).expect("mask");
    let coverage = aura_render::local_masks::coverage(
        &vec![0.2; 40 * 40 * 3],
        40,
        40,
        &recipe,
        None,
        "selection",
    )
    .expect("native coverage");
    let source: Vec<u8> = coverage
        .iter()
        .map(|a| if *a > 0.5 { 255 } else { 0 })
        .collect();
    let inverse = edit(&source, 40, Action::Invert, 0);
    assert_eq!(inverse[0], 255, "outside former selection is selected");
    assert_eq!(inverse[20 * 40 + 20], 255, "subtracted hole is selected");
    assert_eq!(inverse[20 * 40 + 10], 0, "annulus is unselected");
    let frozen = plane(&inverse, 40).matte();
    assert_eq!(frozen.bounds, [0.0, 0.0, 1.0, 1.0]);
    assert!(!frozen.refine_edges);
}
#[test]
fn saved_mask_combinations_match_native_union_intersection_and_subtraction() {
    for (action, expected) in [
        (Action::Union, vec![255, 128, 255]),
        (Action::Intersection, vec![128, 64, 0]),
        (Action::Subtraction, vec![127, 64, 0]),
    ] {
        let mut first = plane(&[255, 128, 0], 3);
        first
            .edit(
                action,
                0,
                Some(&plane(&[128, 128, 255], 3)),
                &CancelToken::new(),
            )
            .expect("combine");
        assert_eq!(first.alpha, expected);
    }
}
#[test]
fn morphology_feather_and_contrast_change_real_coverage() {
    let dot = [0, 0, 0, 0, 255, 0, 0, 0, 0];
    assert_eq!(edit(&dot, 3, Action::Expand, 1), vec![255; 9]);
    assert_eq!(edit(&dot, 3, Action::Contract, 1), vec![0; 9]);
    let soft = edit(&dot, 3, Action::Feather, 1);
    assert!(soft.iter().all(|a| *a > 0 && *a < 255));
    assert_eq!(
        edit(&[0, 85, 170, 255], 4, Action::EdgeContrast, 100),
        vec![0, 0, 255, 255]
    );
    assert_eq!(edit(&dot, 3, Action::Feather, 0), dot);
}
#[test]
fn cleanup_removes_only_small_islands_and_fills_only_enclosed_small_holes() {
    let source = [255, 0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 255, 255];
    let clean = edit(&source, 5, Action::RemoveIslands, 1);
    assert_eq!(clean[0], 0);
    assert_eq!(clean[8], 255);
    let ring = [
        0, 0, 0, 0, 0, 0, 255, 255, 255, 0, 0, 255, 0, 255, 0, 0, 255, 255, 255, 0, 0, 0, 0, 0, 0,
    ];
    let filled = edit(&ring, 5, Action::FillHoles, 1);
    assert_eq!(filled[12], 255);
    assert_eq!(filled[0], 0);
    let mut cancelled = plane(&ring, 5);
    let token = CancelToken::new();
    token.cancel();
    assert!(cancelled.edit(Action::FillHoles, 1, None, &token).is_err());
}
#[test]
fn grid_is_bounded_and_stored_matte_round_trips_through_native_renderer() {
    let rgb: Vec<u8> = (0..1024 * 512)
        .flat_map(|index| [if index % 1024 < 512 { 255 } else { 0 }; 3])
        .collect();
    let coverage = Plane::from_rgb(&rgb, 1024, 512).expect("reduce");
    assert_eq!((coverage.width, coverage.height), (512, 256));
    let mut recipe = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "test");
    let mask = LocalMask {
        id: "frozen".into(),
        name: "Frozen".into(),
        enabled: true,
        amount: 0.5,
        params: aura_recipe::MaskParams {
            exposure: Some(1.0),
            ..Default::default()
        },
        components: vec![Component {
            mode: Mode::Add,
            invert: false,
            source: Source::Matte {
                matte: "selection".into(),
                what: "frozen".into(),
            },
        }],
    };
    local_masks::write(
        &mut recipe,
        &[mask],
        &BTreeMap::from([("selection".into(), coverage.matte())]),
    )
    .expect("write");
    let rendered = aura_render::local_masks::coverage(
        &vec![0.2; 1024 * 512 * 3],
        1024,
        512,
        &recipe,
        None,
        "frozen",
    )
    .expect("native");
    assert_eq!(rendered[200], 1.0);
    assert_eq!(rendered[800], 0.0);
}

#[test]
fn native_workflow_preserves_manual_work_history_stale_safety_membership_and_original() {
    use aura_core::{progress::NullProgress, ImportId};
    use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
    let directory = tempfile::tempdir().expect("catalogue");
    let path = directory.path().join("source.png");
    image::RgbImage::from_fn(80, 60, |x, y| {
        image::Rgb([u8::try_from(30 + x + y).expect("fixture"), 100, 110])
    })
    .save(&path)
    .expect("image");
    let original_bytes = std::fs::read(&path).expect("original");
    let state = AppState::open(&directory.path().join("catalog.aura"))
        .expect("open")
        .with_cache_root(&directory.path().join("cache"));
    let project = crate::create_project(
        &state,
        crate::contract::ipc::CreateProjectInput {
            name: "Selections".into(),
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
            extensions: Vec::new(),
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("ingest");
    let photos = crate::list_images(
        &state,
        &crate::contract::ipc::ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("photos");
    let photo = PhotoId::from_db(&photos.first().expect("photo").id).expect("id");
    let mut base = crate::develop_commands::neutral_recipe(&state, photo);
    base.global.exposure = 0.4;
    let mask = |id: &str, end: [f32; 2]| LocalMask {
        id: id.into(),
        name: id.into(),
        enabled: true,
        amount: 0.4,
        params: aura_recipe::MaskParams {
            exposure: Some(0.5),
            tint: Some(7),
            ..Default::default()
        },
        components: vec![Component {
            mode: Mode::Add,
            invert: false,
            source: Source::Linear {
                start: [0.0, 0.0],
                end,
            },
        }],
    };
    let manual = mask("manual", [0.0, 1.0]);
    let mut color = aura_recipe::studio_color::StudioColor::default();
    color.local.insert(
        "target".into(),
        aura_recipe::studio_color::LocalColor {
            vibrance: 12.0,
            ..Default::default()
        },
    );
    aura_recipe::studio_color::write(&mut base, &color).expect("extended manual color");
    local_masks::write(
        &mut base,
        &[mask("target", [1.0, 0.0]), manual.clone()],
        &BTreeMap::new(),
    )
    .expect("masks");
    state
        .recipe_store()
        .save(&project_id, &photo, &base, &[], "Baseline")
        .expect("save");
    let hash = aura_recipe::hash::recipe_hash(&base).expect("hash");
    let mut input = Input {
        project_id: project.id,
        photo_id: photo.to_db(),
        mask_id: "target".into(),
        recipe_hash: hash.clone(),
        cancel_id: uuid::Uuid::new_v4().to_string(),
        action: Action::Invert,
        component_index: 0,
        target_index: 0,
        source: None,
        mode: None,
        transform: None,
        point: None,
        advanced: None,
        other_mask_id: None,
        value: 0,
    };
    let count = || {
        state
            .catalog()
            .read(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM edit_history WHERE photo_id=?1",
                    [photo.to_db()],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|error| aura_core::errors::db::statement_failed("test history", &error))
            })
            .expect("history")
    };
    let before = count();
    let cancelled = CancelToken::new();
    state.register_job(&input.cancel_id, cancelled.clone());
    assert!(state.cancel_job(&input.cancel_id));
    assert!(
        apply_with_cancel(&state, &input, cancelled).is_err(),
        "cancellation before worker startup"
    );
    assert_eq!(count(), before);
    let edited = apply(&state, &input).expect("invert");
    assert_eq!(count(), before + 1, "one atomic history row");
    assert_eq!(edited.masks[1], manual);
    assert_eq!(
        edited.masks[0].params,
        local_masks::read(&base).expect("base")[0].params
    );
    let stored = crate::develop_commands::load_or_neutral(&state, photo).expect("load");
    assert_eq!(stored.global.exposure, 0.4);
    let (coverage, w, _) = state
        .render()
        .expect("renderer")
        .local_mask_coverage(&photo, &stored, "target")
        .expect("coverage");
    assert!(coverage[0] < 34);
    assert!(coverage[(w as usize - 1) * 3] > 221);
    assert!(apply(&state, &input).is_err(), "stale hash must refuse");
    assert_eq!(count(), before + 1);
    input.recipe_hash = edited.recipe.recipe_hash;
    input.action = Action::Feather;
    input.value = 0;
    apply(&state, &input).expect("zero no-op");
    assert_eq!(count(), before + 1);
    input.action = Action::Duplicate;
    let duplicated = apply(&state, &input).expect("duplicate");
    assert_eq!(duplicated.masks.len(), 3);
    assert_eq!(duplicated.masks[2].params, duplicated.masks[0].params);
    let duplicate_id = duplicated.masks[2].id.clone();
    let duplicate_recipe =
        crate::develop_commands::load_or_neutral(&state, photo).expect("duplicate recipe");
    let duplicate_color = aura_recipe::studio_color::read(&duplicate_recipe).expect("color");
    assert_eq!(
        duplicate_color.local.get(&duplicate_id),
        duplicate_color.local.get("target")
    );
    input.recipe_hash = duplicated.recipe.recipe_hash;
    input.action = Action::Expand;
    input.value = 33;
    assert!(apply(&state, &input).is_err(), "bounded radius");
    input.value = 1;
    input.project_id = ProjectId::new().to_db();
    assert!(apply(&state, &input).is_err(), "membership");
    assert_eq!(std::fs::read(&path).expect("original"), original_bytes);
    crate::develop_commands::history_step(
        &state,
        &crate::contract::ipc::HistoryStepInput {
            project_id: project_id.to_db(),
            photo_id: photo.to_db(),
            action: "undo".into(),
        },
    )
    .expect("undo duplicate");
    assert_eq!(
        local_masks::read(
            &crate::develop_commands::load_or_neutral(&state, photo).expect("undo recipe")
        )
        .expect("masks"),
        edited.masks
    );
    crate::develop_commands::history_step(
        &state,
        &crate::contract::ipc::HistoryStepInput {
            project_id: project_id.to_db(),
            photo_id: photo.to_db(),
            action: "redo".into(),
        },
    )
    .expect("redo duplicate");
    drop(state);
    let reopened = AppState::open(&directory.path().join("catalog.aura")).expect("reopen");
    assert_eq!(
        local_masks::read(
            &crate::develop_commands::load_or_neutral(&reopened, photo).expect("recipe")
        )
        .expect("masks"),
        duplicated.masks
    );
    let mut loaded = crate::develop_commands::load_or_neutral(&reopened, photo).expect("loaded");
    for action in [Action::Feather, Action::Expand, Action::Contract] {
        let request = Input {
            project_id: project_id.to_db(),
            photo_id: photo.to_db(),
            mask_id: "target".into(),
            recipe_hash: aura_recipe::hash::recipe_hash(&loaded).expect("hash"),
            cancel_id: uuid::Uuid::new_v4().to_string(),
            action,
            component_index: 0,
            target_index: 0,
            source: None,
            mode: None,
            transform: None,
            point: None,
            advanced: None,
            other_mask_id: None,
            value: 1,
        };
        apply(&reopened, &request).expect("repeated refinement");
        loaded = crate::develop_commands::load_or_neutral(&reopened, photo).expect("refinement");
        assert!(
            local_masks::read_mattes(&loaded).expect("mattes").len() <= 2,
            "only current target and duplicate selections are retained"
        );
    }
    assert_new_workflows_persist_atomically(&reopened, project_id, photo);
    laboratory::assert_persisted(&reopened, project_id, photo);
    assert_eq!(
        std::fs::read(&path).expect("unchanged source"),
        original_bytes
    );
}

#[test]
fn placement_topology_boundary_median_xor_and_hull_have_distinct_effects() {
    let token = CancelToken::new();
    let mut translated = plane(&[0, 0, 0, 0, 255, 0, 0, 0, 0], 3);
    translated
        .transform(
            authoring::Transform {
                offset: [1.0 / 3.0, 0.0],
                scale: [1.0, 1.0],
                angle: 0.0,
            },
            &token,
        )
        .expect("translate");
    assert_eq!(translated.alpha, vec![0, 0, 0, 0, 0, 255, 0, 0, 0]);
    let mut identity = plane(&[0, 85, 170, 255], 2);
    identity
        .transform(
            authoring::Transform {
                offset: [0.0, 0.0],
                scale: [1.0, 1.0],
                angle: 0.0,
            },
            &token,
        )
        .expect("identity");
    assert_eq!(identity.alpha, vec![0, 85, 170, 255]);
    let mut regions = plane(&[255, 0, 0, 0, 255, 0, 0, 170, 170, 170], 5);
    regions.keep_connected([0.0, 0.0], &token).expect("seed");
    assert_eq!(regions.alpha, vec![255, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert!(regions.keep_connected([0.9, 0.1], &token).is_err());
    assert_eq!(
        edit(
            &[255, 0, 0, 0, 255, 0, 0, 170, 170, 170],
            5,
            Action::KeepLargest,
            0
        ),
        vec![0, 0, 0, 0, 255, 0, 0, 170, 170, 170]
    );
    let ring = edit(
        &[
            0, 0, 0, 0, 0, 0, 255, 255, 255, 0, 0, 255, 255, 255, 0, 0, 255, 255, 255, 0, 0, 0, 0,
            0, 0,
        ],
        5,
        Action::BorderSelection,
        1,
    );
    assert_eq!(ring[12], 0);
    assert_eq!(ring[11], 255);
    assert_eq!(
        edit(&[0, 0, 0, 0, 255, 0, 0, 0, 0], 3, Action::MedianCleanup, 1),
        vec![0; 9]
    );
    let mut xor = plane(&[255, 128, 0], 3);
    xor.edit(
        Action::ExclusiveUnion,
        0,
        Some(&plane(&[255, 0, 255], 3)),
        &token,
    )
    .expect("xor");
    assert_eq!(xor.alpha, vec![0, 128, 255]);
    let hull = edit(
        &[255, 0, 255, 0, 0, 0, 255, 0, 255],
        3,
        Action::ConvexHull,
        0,
    );
    assert_eq!(hull, vec![255; 9]);
    let cancelled = CancelToken::new();
    cancelled.cancel();
    assert!(regions.keep_largest(&cancelled).is_err());
}

fn request(action: Action) -> Input {
    serde_json::from_value(serde_json::json!({"projectId":"p","photoId":"p","maskId":"one","recipeHash":"h","cancelId":"c","action": action_name(action)})).expect("input defaults")
}
fn action_name(action: Action) -> &'static str {
    match action {
        Action::EditLinear => "edit_linear",
        Action::EditRadial => "edit_radial",
        Action::EditBrush => "edit_brush",
        Action::EditLuminance => "edit_luminance",
        Action::ComponentMode => "component_mode",
        Action::ReorderComponent => "reorder_component",
        Action::ExtractComponent => "extract_component",
        Action::CopyComponent => "copy_component",
        _ => "freeze",
    }
}
fn authored_mask() -> LocalMask {
    LocalMask {
        id: "one".into(),
        name: "Authored".into(),
        enabled: true,
        amount: 0.4,
        params: aura_recipe::MaskParams {
            exposure: Some(0.5),
            ..Default::default()
        },
        components: vec![
            Component {
                mode: Mode::Add,
                invert: false,
                source: Source::Linear {
                    start: [0.1, 0.1],
                    end: [0.8, 0.9],
                },
            },
            Component {
                mode: Mode::Subtract,
                invert: false,
                source: Source::Radial {
                    centre: [0.4, 0.5],
                    radii: [0.1, 0.2],
                    angle: 12.0,
                    feather: 0.3,
                },
            },
            Component {
                mode: Mode::Add,
                invert: false,
                source: Source::Brush {
                    strokes: vec![aura_recipe::retouch_tools::BrushStroke {
                        erase: false,
                        radius: 0.02,
                        opacity: 0.6,
                        points: vec![[0.2, 0.2, 0.8], [0.4, 0.4, 0.8]],
                    }],
                    feather: 0.3,
                },
            },
            Component {
                mode: Mode::Intersect,
                invert: false,
                source: Source::Luminance {
                    range: aura_recipe::retouch_tools::LuminanceRange {
                        low: -2.0,
                        high: 2.0,
                        softness: 0.5,
                    },
                },
            },
        ],
    }
}
#[test]
fn authored_geometry_edits_retain_original_components_and_validate_type_and_limits() {
    let original = authored_mask();
    let replacements = [
        (
            Action::EditLinear,
            Source::Linear {
                start: [0.2, 0.3],
                end: [0.9, 0.7],
            },
        ),
        (
            Action::EditRadial,
            Source::Radial {
                centre: [0.2, 0.3],
                radii: [0.3, 0.4],
                angle: 55.0,
                feather: 0.7,
            },
        ),
        (
            Action::EditBrush,
            Source::Brush {
                strokes: vec![aura_recipe::retouch_tools::BrushStroke {
                    erase: true,
                    radius: 0.03,
                    opacity: 0.5,
                    points: vec![[0.3, 0.4, 0.7]],
                }],
                feather: 0.8,
            },
        ),
        (
            Action::EditLuminance,
            Source::Luminance {
                range: aura_recipe::retouch_tools::LuminanceRange {
                    low: -1.0,
                    high: 3.0,
                    softness: 1.0,
                },
            },
        ),
    ];
    for (index, (action, source)) in replacements.into_iter().enumerate() {
        let mut masks = vec![original.clone()];
        let mut input = request(action);
        input.component_index = index;
        input.source = Some(source.clone());
        authoring::edit(&input, &mut masks).expect("edit geometry");
        assert_eq!(masks[0].params, original.params);
        assert_eq!(masks[0].components[index].source, source);
        for (i, component) in masks[0].components.iter().enumerate() {
            if i != index {
                assert_eq!(component, &original.components[i]);
            }
        }
        local_masks::validate(&masks, &BTreeMap::new()).expect("valid edited geometry");
        let rendered = |source: Source| {
            let mut recipe =
                aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "geometry");
            let mut isolated = original.clone();
            isolated.components = vec![Component {
                mode: Mode::Add,
                invert: false,
                source,
            }];
            local_masks::write(&mut recipe, &[isolated], &BTreeMap::new())
                .expect("isolated geometry");
            let pixels: Vec<f32> = (0_u16..32 * 32)
                .flat_map(|i| [0.005 + f32::from(i % 32) / 40.0; 3])
                .collect();
            aura_render::local_masks::coverage(&pixels, 32, 32, &recipe, None, "one")
                .expect("render authored geometry")
        };
        assert_ne!(
            rendered(original.components[index].source.clone()),
            rendered(source),
            "{action:?} changes native coverage"
        );
        input.component_index = (index + 1) % 4;
        assert!(
            authoring::edit(&input, &mut masks).is_err(),
            "source type mismatch"
        );
    }
    let mut masks = vec![original.clone()];
    let mut input = request(Action::ComponentMode);
    input.mode = Some(Mode::Intersect);
    input.component_index = 1;
    authoring::edit(&input, &mut masks).expect("mode");
    assert_eq!(masks[0].components[1].mode, Mode::Intersect);
    input.action = Action::ReorderComponent;
    input.target_index = 3;
    authoring::edit(&input, &mut masks).expect("reorder");
    assert!(matches!(
        masks[0].components[3].source,
        Source::Radial { .. }
    ));
    input.target_index = 4;
    assert!(authoring::edit(&input, &mut masks).is_err());
    input.action = Action::ExtractComponent;
    input.component_index = 3;
    let id = authoring::edit(&input, &mut masks).expect("extract");
    assert_ne!(id, original.id);
    assert_eq!(masks.len(), 2);
    assert_eq!(masks[1].components[0].mode, Mode::Add);
    assert_eq!(masks[1].params, original.params);
    input.action = Action::CopyComponent;
    input.other_mask_id = Some(id);
    authoring::edit(&input, &mut masks).expect("copy");
    assert_eq!(masks[1].components.len(), 2);
    assert_eq!(masks[0].components.len(), 4);
    input.component_index = 999;
    assert!(authoring::edit(&input, &mut masks).is_err());
}

#[allow(clippy::cast_precision_loss)] // Seed coordinates derive from a bounded image proxy.
fn assert_new_workflows_persist_atomically(state: &AppState, project: ProjectId, photo: PhotoId) {
    let mut baseline = crate::develop_commands::load_or_neutral(state, photo).expect("baseline");
    let mut other = authored_mask();
    other.id = "two".into();
    other.name = "Other".into();
    other.components.truncate(1);
    local_masks::write(&mut baseline, &[authored_mask(), other], &BTreeMap::new())
        .expect("authoring baseline");
    let mut color = aura_recipe::studio_color::read(&baseline).expect("color");
    color.local.insert(
        "one".into(),
        aura_recipe::studio_color::LocalColor {
            vibrance: 17.0,
            ..Default::default()
        },
    );
    aura_recipe::studio_color::write(&mut baseline, &color).expect("local color");
    state
        .recipe_store()
        .save(
            &project,
            &photo,
            &baseline,
            &[
                local_masks::KEY.into(),
                aura_recipe::studio_color::KEY.into(),
            ],
            "Authoring baseline",
        )
        .expect("baseline save");
    let baseline = crate::develop_commands::load_or_neutral(state, photo).expect("stored baseline");
    let hash = aura_recipe::hash::recipe_hash(&baseline).expect("hash");
    for (action, index, value) in [
        (Action::EditLinear, 0, 0),
        (Action::EditRadial, 1, 0),
        (Action::EditBrush, 2, 0),
        (Action::EditLuminance, 3, 0),
        (Action::ComponentMode, 1, 0),
        (Action::ReorderComponent, 0, 0),
        (Action::ExtractComponent, 1, 0),
        (Action::CopyComponent, 1, 0),
        (Action::TransformSelection, 0, 0),
        (Action::BorderSelection, 0, 1),
        (Action::KeepLargest, 0, 0),
        (Action::KeepConnected, 0, 0),
        (Action::MedianCleanup, 0, 1),
        (Action::ExclusiveUnion, 0, 0),
        (Action::ConvexHull, 0, 0),
    ] {
        let mut input = request(action);
        input.action = action;
        input.project_id = project.to_db();
        input.photo_id = photo.to_db();
        input.recipe_hash = hash.clone();
        input.cancel_id = uuid::Uuid::new_v4().to_string();
        input.component_index = index;
        input.target_index = 3;
        input.value = value;
        input.other_mask_id = if matches!(action, Action::CopyComponent | Action::ExclusiveUnion) {
            Some("two".into())
        } else {
            None
        };
        input.mode = Some(Mode::Intersect);
        input.transform = Some(authoring::Transform {
            offset: [0.1, 0.05],
            scale: [0.8, 1.1],
            angle: 10.0,
        });
        input.source = match action {
            Action::EditLinear => Some(Source::Linear {
                start: [0.3, 0.1],
                end: [0.8, 0.7],
            }),
            Action::EditRadial => Some(Source::Radial {
                centre: [0.3, 0.2],
                radii: [0.2, 0.2],
                angle: 35.0,
                feather: 0.7,
            }),
            Action::EditBrush => Some(Source::Brush {
                strokes: vec![aura_recipe::retouch_tools::BrushStroke {
                    erase: false,
                    radius: 0.07,
                    opacity: 0.9,
                    points: vec![[0.5, 0.4, 0.8]],
                }],
                feather: 0.9,
            }),
            Action::EditLuminance => Some(Source::Luminance {
                range: aura_recipe::retouch_tools::LuminanceRange {
                    low: -3.0,
                    high: 3.0,
                    softness: 1.0,
                },
            }),
            _ => None,
        };
        if action == Action::KeepConnected {
            let (rgb, width, height) = state
                .render()
                .expect("renderer")
                .local_mask_coverage(&photo, &baseline, "one")
                .expect("coverage");
            let seed = rgb
                .chunks_exact(3)
                .position(|p| p[0] >= 128)
                .expect("selected seed");
            input.point = Some([
                ((seed % width as usize) as f32 + 0.5) / width as f32,
                ((seed / width as usize) as f32 + 0.5) / height as f32,
            ]);
        }
        let edited = apply(state, &input).unwrap_or_else(|e| panic!("{action:?}: {e:?}"));
        assert_ne!(edited.recipe.recipe_hash, hash, "{action:?} changes recipe");
        let saved = crate::develop_commands::load_or_neutral(state, photo).expect("saved");
        assert_eq!(saved.global, baseline.global);
        assert_eq!(
            local_masks::read(&saved).expect("masks")[0].params,
            authored_mask().params
        );
        if action == Action::ExtractComponent {
            let local = aura_recipe::studio_color::read(&saved).expect("color");
            assert_eq!(
                local
                    .local
                    .get(edited.mask_id.as_deref().expect("new mask")),
                local.local.get("one")
            );
        }
        assert!(
            apply(state, &input).is_err(),
            "{action:?} refuses stale request"
        );
        crate::develop_commands::history_step(
            state,
            &crate::contract::ipc::HistoryStepInput {
                project_id: project.to_db(),
                photo_id: photo.to_db(),
                action: "undo".into(),
            },
        )
        .expect("single undo");
        let undone = crate::develop_commands::load_or_neutral(state, photo).expect("undone");
        assert_eq!(
            aura_recipe::hash::recipe_hash(&undone).expect("hash"),
            hash,
            "one-step exact undo for {action:?}"
        );
        crate::develop_commands::history_step(
            state,
            &crate::contract::ipc::HistoryStepInput {
                project_id: project.to_db(),
                photo_id: photo.to_db(),
                action: "redo".into(),
            },
        )
        .expect("redo");
        let redone = crate::develop_commands::load_or_neutral(state, photo).expect("redone");
        assert_eq!(
            aura_recipe::hash::recipe_hash(&redone).expect("hash"),
            edited.recipe.recipe_hash,
            "exact redo for {action:?}"
        );
        crate::develop_commands::history_step(
            state,
            &crate::contract::ipc::HistoryStepInput {
                project_id: project.to_db(),
                photo_id: photo.to_db(),
                action: "undo".into(),
            },
        )
        .expect("return baseline");
    }
}
