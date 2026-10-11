//! Evoto's "Beauty & Fashion" pass on a painted studio portrait, end to end. ADR-0109.
//!
//! The portrait is `aura_portrait::fixtures`' painted face on a grey seamless at sRGB 206 - the
//! backdrop Evoto's homepage example starts from. The test runs Auto advanced retouch with the
//! `beauty_fashion` preset through the application layer and renders before and after. It proves
//! the plumbing and the arithmetic on a painted face; it is not evidence about a photograph.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::print_stdout,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use aura_app::advanced_retouch::{self, AdvancedRetouchInput, Stage};
use aura_app::contract::ipc::{CreateProjectInput, ListImagesInput};
use aura_app::AppState;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ImportId, PhotoId, ProjectId};
use aura_ingest::contract::ingest::{ImportMode, ImportPlan};
use aura_portrait::fixtures::{portrait, PortraitSpec};
use aura_raw::codec::{encode_jpeg, Rgb8};
use aura_render::RenderService;
use std::sync::Arc;

fn render(
    state: &AppState,
    photo: PhotoId,
    recipe: &aura_recipe::Recipe,
    full: bool,
) -> (Vec<u8>, u32, u32) {
    let mut recipe = recipe.clone();
    if full {
        recipe.geometry = aura_recipe::Geometry::default();
    }
    let result = state
        .render()
        .expect("renderer")
        .render(aura_render::RenderRequest {
            image_id: photo,
            recipe,
            level: if full {
                aura_render::RenderLevel::Full
            } else {
                aura_render::RenderLevel::Screen(1600, 1200)
            },
            purpose: aura_render::RenderPurpose::Export,
            output: aura_render::OutputSpec {
                colour_space: aura_render::OutputColour::Srgb,
                bit_depth: 8,
                icc: None,
            },
        })
        .expect("render");
    let bytes = match result.data {
        aura_render::RenderedData::Eight(v) => v,
        aura_render::RenderedData::Sixteen(v) => v.iter().map(|x| (x >> 8) as u8).collect(),
    };
    (bytes, result.width, result.height)
}

#[test]
fn beauty_and_fashion_lifts_a_grey_studio_backdrop_toward_where_evoto_lands_it() {
    let dir = tempfile::tempdir().expect("temp");
    let spec = PortraitSpec {
        width: 512,
        height: 512,
        ground: [206, 206, 206],
        ..PortraitSpec::default()
    };
    let painted = portrait(&spec);
    let path = dir.path().join("studio.jpg");
    let jpeg = encode_jpeg(
        &Rgb8 {
            width: painted.width,
            height: painted.height,
            data: painted.rgb.clone(),
        },
        95,
    )
    .expect("jpeg");
    std::fs::write(&path, jpeg).expect("write");
    let state = AppState::open(&dir.path().join("catalog.aura"))
        .expect("state")
        .with_cache_root(&dir.path().join("cache"))
        .with_key_store(Arc::new(aura_cloud::keys::MemoryKeyStore::default()));
    let project = aura_app::create_project(
        &state,
        CreateProjectInput {
            name: "Studio".into(),
            couple_names: None,
            event_date: None,
        },
    )
    .expect("project");
    aura_ingest::run(
        state.catalog(),
        &ImportPlan {
            import_id: ImportId::new(),
            project_id: ProjectId::from_db(&project.id).expect("id"),
            roots: vec![path],
            mode: ImportMode::Reference,
            extensions: vec![],
            extract_embedded_previews: false,
            settle_window_ms: 0,
        },
        &CancelToken::new(),
        &NullProgress,
    )
    .expect("import");
    let photo = aura_app::list_images(
        &state,
        &ListImagesInput {
            project_id: project.id.clone(),
            offset: 0,
            limit: 10,
            order_by: None,
        },
    )
    .expect("photos")[0]
        .id
        .clone();
    let photo_id = PhotoId::from_db(&photo).expect("photo id");
    let original = aura_app::develop_commands::load_or_neutral(&state, photo_id).expect("recipe");
    let result = advanced_retouch::run(
        &state,
        &AdvancedRetouchInput {
            project_id: project.id.clone(),
            photo_id: photo.clone(),
            options: None,
            preset: Some("beauty_fashion".into()),
        },
        &|_| {},
    )
    .expect("beauty and fashion pass");
    let toning = result
        .report
        .stages
        .iter()
        .find(|s| s.stage == Stage::BackgroundToning)
        .expect("background toning stage");
    println!(
        "faces {} | toning changes {:?} | checks {:?}",
        result.report.faces, toning.changes, toning.checks
    );
    let after_recipe =
        aura_app::develop_commands::load_or_neutral(&state, photo_id).expect("recipe");
    let before = render(&state, photo_id, &original, true);
    let after = render(&state, photo_id, &after_recipe, true);
    // The top-left corner is backdrop, far from the head.
    let corner = |(rgb, w, _): &(Vec<u8>, u32, u32)| {
        let mut sum = [0.0_f32; 3];
        for y in 4..24 {
            for x in 4..24 {
                let i = (y * *w as usize + x) * 3;
                for c in 0..3 {
                    sum[c] += f32::from(rgb[i + c]);
                }
            }
        }
        sum.map(|v| v / 400.0)
    };
    let (b, a) = (corner(&before), corner(&after));
    println!("backdrop before {b:?} after {a:?}");
    assert!(
        toning.changes.iter().any(|c| c.contains("lifted")),
        "a plain grey seamless behind a found face is lifted: {:?}",
        toning.checks
    );
    // Evoto's own example lands sRGB 206 at about 233: within a few code values of it, neutral,
    // and not clipped.
    assert!((a[1] - 233.0).abs() < 4.0, "{b:?} -> {a:?}");
    assert!(
        (a[0] - a[2]).abs() < 2.0 && (a[0] - a[1]).abs() < 2.0,
        "the lift must stay neutral: {a:?}"
    );
    assert!(a[1] < 250.0, "the backdrop must not clip to white: {a:?}");
}
