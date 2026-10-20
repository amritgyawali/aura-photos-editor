//! The strength sliders over automatic retouch, through the native commands and the renderer.
//! ADR-0130.
#![allow(clippy::expect_used, clippy::float_cmp, clippy::too_many_lines)]
use std::collections::BTreeMap;
use std::sync::Arc;

use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::native_retouch::{self, RetouchInput};
use aura_app::retouch_intensity::{intensity, IntensityInput};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_raw::codec::{encode_jpeg, Rgb8};
use aura_recipe::retouch_intensity::Category;
use aura_recipe::retouch_tools::{self, Edit, Tool};

fn pixels(seed: u8) -> Vec<u8> {
    (0..96 * 72u32)
        .flat_map(|i| {
            let x = i % 96;
            let y = i / 96;
            // Fine detail for smoothing to remove, and bright patches for shine control.
            let fine = if (x + y) % 3 == 0 { 40 } else { 0 };
            let bright = if (20..40).contains(&x) && (20..40).contains(&y) {
                90
            } else {
                0
            };
            let base = 70u8
                .saturating_add(seed)
                .saturating_add(fine)
                .saturating_add(bright);
            [base, base.saturating_sub(25), base.saturating_sub(45)]
        })
        .collect()
}

fn op(id: &str, tool: Tool, amount: f32) -> Edit {
    Edit {
        id: id.into(),
        tool,
        enabled: true,
        repair_passes: None,
        region: [0.5, 0.5, 0.75, 0.75],
        source: None,
        source_scale: 1.0,
        amount,
        feather: 0.3,
        radius: 0.02,
        texture: 0.2,
        preserve_microtexture: false,
        texture_heal: false,
        clean_ring_fit: false,
        curved_heal: false,
        heal_samples: Vec::new(),
        texture_sources: Vec::new(),
        sensitivity: None,
        keep_dark_marks: false,
        mole_edge: None,
        tone: 0.8,
        warmth: 0.0,
        tint: 0.0,
        target_color: None,
        mask: None,
        skin: None,
        selection: None,
        matte: None,
    }
}

fn save_stack(state: &AppState, project: &str, photo: &str, edits: &[Edit]) {
    let photo_id = PhotoId::from_db(photo).expect("photo id");
    let mut recipe = aura_app::develop_commands::load_or_neutral(state, photo_id).expect("recipe");
    retouch_tools::write(&mut recipe, edits).expect("write stack");
    state
        .recipe_store()
        .save(
            &ProjectId::from_db(project).expect("project id"),
            &photo_id,
            &recipe,
            &[retouch_tools::KEY.to_owned()],
            "fixture",
        )
        .expect("save stack");
}

fn request(
    project: &str,
    photo: &str,
    action: &str,
    factors: &[(Category, f32)],
) -> IntensityInput {
    IntensityInput {
        project_id: project.into(),
        photo_id: photo.into(),
        action: action.into(),
        factors: factors.iter().copied().collect(),
    }
}

fn render(state: &AppState, project: &str, photo: &str) -> String {
    native_retouch::preview(state, project, photo, false)
        .expect("render retouch")
        .rgb_base64
}

#[test]
fn sliders_scale_automatic_retouch_after_the_pass_and_sync_across_a_collection() {
    let dir = tempfile::tempdir().expect("isolated catalog directory");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("open isolated catalog")
        .with_cache_root(&dir.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Retouch strength".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("create project");
    let originals = dir.path().join("originals");
    std::fs::create_dir_all(&originals).expect("originals folder");
    for (index, seed) in [0u8, 30, 60].into_iter().enumerate() {
        let rgb = Rgb8 {
            width: 96,
            height: 72,
            data: pixels(seed),
        };
        std::fs::write(
            originals.join(format!("frame-{index}.jpg")),
            encode_jpeg(&rgb, 95).expect("jpeg"),
        )
        .expect("write original");
    }
    let report = aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("id"),
            roots: vec![originals],
            mode: ImportMode::Reference,
            extensions: vec![],
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    assert_eq!(report.files_imported, 3);
    let photos: Vec<String> = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("photos")
    .into_iter()
    .map(|p| p.id)
    .collect();
    let (first, second, third) = (&photos[0], &photos[1], &photos[2]);

    // What an automatic pass leaves behind, named the way it names its operations.
    let automatic = [
        op("auto-portrait-v1-0-pores", Tool::Frequency, 0.6),
        op("auto-portrait-v1-0-shine", Tool::Mattify, 0.5),
    ];
    save_stack(&state, &project.id, first, &automatic);
    save_stack(&state, &project.id, second, &automatic);
    // A photo retouched only by hand: sync leaves it alone.
    save_stack(
        &state,
        &project.id,
        third,
        &[op("hand-1", Tool::Frequency, 0.5)],
    );

    let got = intensity(&state, &request(&project.id, first, "get", &[])).expect("get");
    let names: Vec<Category> = got.sliders.iter().map(|s| s.category).collect();
    assert_eq!(names, vec![Category::Smoothing, Category::Shine]);
    assert!(got
        .sliders
        .iter()
        .all(|s| s.factor == 1.0 && s.operations == 1));
    assert!((got.sliders[0].headroom - 1.0 / 0.6).abs() < 1e-5);
    assert!((got.sliders[1].headroom - 2.0).abs() < 1e-5);

    let full = render(&state, &project.id, first);
    let untouched = native_retouch::preview(&state, &project.id, first, true)
        .expect("render original")
        .rgb_base64;
    assert_ne!(full, untouched, "the fixture retouch must change pixels");

    // Lowering one slider changes the picture and leaves the measured amounts alone.
    let lowered = intensity(
        &state,
        &request(&project.id, first, "set", &[(Category::Smoothing, 0.0)]),
    )
    .expect("set");
    assert_eq!(lowered.sliders[0].factor, 0.0);
    assert_eq!(lowered.sliders[1].factor, 1.0);
    let no_smoothing = render(&state, &project.id, first);
    assert_ne!(no_smoothing, full);
    let stored = native_retouch::edit(
        &state,
        &RetouchInput {
            project_id: project.id.clone(),
            photo_id: first.clone(),
            action: "list".into(),
            edits: Vec::new(),
            id: None,
        },
    )
    .expect("list");
    assert_eq!(
        stored,
        automatic.to_vec(),
        "sliders never rewrite the operations"
    );

    // Every slider at zero is exactly the photo before retouch.
    intensity(
        &state,
        &request(&project.id, first, "set", &[(Category::Shine, 0.0)]),
    )
    .expect("set shine");
    assert_eq!(render(&state, &project.id, first), untouched);

    // Reset returns exactly what the automatic pass decided.
    let reset = intensity(&state, &request(&project.id, first, "reset", &[])).expect("reset");
    assert!(reset.sliders.iter().all(|s| s.factor == 1.0));
    assert_eq!(render(&state, &project.id, first), full);

    // Sync carries this photo's sliders onto the other automatically retouched photos.
    intensity(
        &state,
        &request(&project.id, first, "set", &[(Category::Smoothing, 0.4)]),
    )
    .expect("set again");
    let synced = intensity(&state, &request(&project.id, first, "sync", &[])).expect("sync");
    assert_eq!((synced.synced, synced.skipped), (1, 1));
    let other = intensity(&state, &request(&project.id, second, "get", &[])).expect("get other");
    let factors: BTreeMap<Category, f32> = other
        .sliders
        .iter()
        .map(|s| (s.category, s.factor))
        .collect();
    assert_eq!(factors.get(&Category::Smoothing), Some(&0.4));
    assert_eq!(factors.get(&Category::Shine), Some(&1.0));
    let hand = intensity(&state, &request(&project.id, third, "get", &[])).expect("get hand");
    assert!(
        hand.sliders.is_empty(),
        "a hand-made operation has no slider"
    );

    // Out of range and unknown actions are refused, and nothing is stored.
    assert!(intensity(
        &state,
        &request(&project.id, first, "set", &[(Category::Smoothing, 2.5)])
    )
    .is_err());
    assert!(intensity(&state, &request(&project.id, first, "spin", &[])).is_err());
    assert!(intensity(&state, &request(&project.id, first, "set", &[])).is_err());
    let after = intensity(&state, &request(&project.id, first, "get", &[])).expect("get");
    assert_eq!(after.sliders[0].factor, 0.4);
}
