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
}
