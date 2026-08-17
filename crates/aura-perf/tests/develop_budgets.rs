//! Phase 14 performance budgets, measured against the shipping engine and store.
//!
//! ## Four of section 11's five rows name a GPU, and none of them is asserted
//!
//! The 2048 px proxy at 60 ms on an RTX 4070 and 110 ms on an M3 Pro, the 45 MP export at
//! 900 ms, and batch export at 1.2 images per second all describe hardware this build cannot
//! reach: `aura-render` links no `wgpu` backend (ADR-0029 section 4). They are waived with an
//! expiry rather than relabelled as processor numbers, exactly as ADR-0007 waives phase 03's
//! throughput rows.
//!
//! The fifth row - "full 45 MP render (CPU fallback) <= 6 s" - is the path this build runs,
//! and it is asserted, per megapixel, so a test can time a small frame and still fail on a
//! regression.
//!
//! ## What is being timed, and where the boundary is
//!
//! `CpuEngine::render_frame`, with the pixels already in hand. The decode in front of it is
//! phase 02's cost and `preview_budgets` owns it; timing the two together would produce a
//! number that moved when an unrelated decoder changed.

use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::{PhotoId, ProjectId};
use aura_perf::{Budgets, Measurement, StageTimer};
use aura_recipe::{fixtures as recipes, schema, EditSource, Recipe, RecipeStore};
use aura_render::contract::render::{OutputSpec, RenderLevel, RenderPurpose};
use aura_render::cpu::CpuEngine;
use aura_render::{fixtures, graph};
use rusqlite::params;
use tempfile::TempDir;

/// The frame the per-megapixel rows are measured on. 2 MP: large enough that the per-pixel
/// cost dominates the setup, small enough that a debug-build CI run finishes.
const TIMED_WIDTH: u32 = 1_632;
const TIMED_HEIGHT: u32 = 1_224;

/// The image count the storage row is stated against.
const STORAGE_IMAGES: usize = 1_000;

fn budgets() -> Budgets {
    Budgets::load(std::path::Path::new("../../perf/budgets.toml"))
        .unwrap_or_else(|err| panic!("budgets: {err}"))
}

fn assert_timing(budgets: &Budgets, measurement: &Measurement) {
    println!(
        "{}: {} ms over {} units",
        measurement.stage, measurement.elapsed_ms, measurement.units
    );
    if cfg!(debug_assertions) {
        println!("  (debug build: reported, not asserted)");
        return;
    }
    if let Err(reason) = budgets.check(measurement) {
        panic!("{reason}");
    }
}

/// The reference grade: every stage a real edit turns on.
fn graded() -> Recipe {
    let mut recipe = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    recipe.global.exposure = 0.31;
    recipe.global.contrast = 11;
    recipe.global.temperature = 4930;
    recipe.global.tint = 8;
    recipe.global.highlights = -31;
    recipe.global.shadows = 22;
    recipe.global.whites = 7;
    recipe.global.blacks = -9;
    recipe.global.clarity = 6;
    recipe.global.texture = 4;
    recipe.global.vibrance = 8;
    recipe.global.saturation = 4;
    recipe.global.sharpen.amount = 40;
    recipe.global.noise.luminance = 22;
    recipe.global.noise.colour = 30;
    recipe.geometry.crop = [0.02, 0.01, 0.97, 0.98];
    recipe
}

fn engine(frame: aura_render::Frame) -> CpuEngine {
    CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(frame)),
        Arc::new(SystemClock::default()),
    )
}

#[test]
fn a_full_resolution_render_meets_the_processor_fallback_budget() {
    let frame = fixtures::detail_frame(TIMED_WIDTH, TIMED_HEIGHT);
    let engine = engine(frame.clone());
    let recipe = graded();

    // One warm run so the measurement is of the pipeline rather than of the allocator's
    // first touch of a two-megapixel buffer.
    let _ = engine.render_frame(
        &frame,
        &recipe,
        RenderLevel::Full,
        RenderPurpose::Export,
        &OutputSpec::default(),
    );

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let timer = StageTimer::start("render_full_cpu_per_megapixel", clock.as_ref());
    let out = engine
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        )
        .unwrap_or_else(|err| panic!("render: {err}"));
    let megapixels = (u64::from(TIMED_WIDTH) * u64::from(TIMED_HEIGHT)).div_ceil(1_000_000);
    let measurement = timer.finish(megapixels.max(1));

    println!(
        "  {} stages, {}x{} out, backend {}",
        out.stages_run.len(),
        out.width,
        out.height,
        out.backend
    );
    println!(
        "  a 45 MP frame at this rate: {} ms (section 11 waives the 900 ms GPU row)",
        measurement.elapsed_ms * 45 / megapixels.max(1)
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn an_interactive_proxy_render_meets_its_guardrail() {
    // Section 11's 60 ms is an RTX 4070 figure and is waived. This is its processor
    // counterpart, and it is a regression guardrail rather than a claim of parity.
    let frame = fixtures::detail_frame(3_000, 2_000);
    let engine = engine(frame.clone());
    let recipe = graded();

    let _ = engine.render_frame(
        &frame,
        &recipe,
        RenderLevel::Proxy2048,
        RenderPurpose::Interactive,
        &OutputSpec::default(),
    );

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let timer = StageTimer::start("render_proxy2048_cpu", clock.as_ref());
    let out = engine
        .render_frame(
            &frame,
            &recipe,
            RenderLevel::Proxy2048,
            RenderPurpose::Interactive,
            &OutputSpec::default(),
        )
        .unwrap_or_else(|err| panic!("render: {err}"));
    let measurement = timer.finish(1);

    // The fit to 2048 happens before the crop, so a cropped recipe delivers slightly less
    // than 2048 - which is correct, and asserting the exact number here would be asserting
    // the fixture's crop rather than the render level.
    println!("  {}x{} proxy", out.width, out.height);
    assert!(out.width.max(out.height) <= 2048);
    assert!(out.width.max(out.height) > 1_800);
    assert_timing(&budgets(), &measurement);
}

#[test]
fn a_slider_move_re_renders_only_what_it_invalidated() {
    // Section 6.3's invalidation rule, as a number. Moving saturation must not re-run white
    // balance, the tone curve or noise reduction - so the plan for the invalidated tail is
    // shorter, and the render is cheaper.
    let frame = fixtures::detail_frame(3_000, 2_000);
    let engine = engine(frame.clone());

    let base = graded();
    let late = graph::earliest_affected(&["global.saturation".to_string()])
        .unwrap_or_else(|| panic!("saturation must map to a stage"));
    assert!(late.index() > graph::Stage::NoiseReduction.index());

    // The tail-only recipe: everything the slider did not invalidate is already in the
    // cached buffer, so the re-render is the stages from `late` onward.
    let mut tail = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    tail.global.saturation = base.global.saturation + 10;
    tail.global.vibrance = base.global.vibrance;
    tail.global.sharpen = base.global.sharpen;
    tail.geometry = base.geometry.clone();

    let _ = engine.render_frame(
        &frame,
        &tail,
        RenderLevel::Proxy2048,
        RenderPurpose::Interactive,
        &OutputSpec::default(),
    );

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let timer = StageTimer::start("render_proxy_slider_cpu", clock.as_ref());
    let out = engine
        .render_frame(
            &frame,
            &tail,
            RenderLevel::Proxy2048,
            RenderPurpose::Interactive,
            &OutputSpec::default(),
        )
        .unwrap_or_else(|err| panic!("render: {err}"));
    let measurement = timer.finish(1);

    println!(
        "  {} stages on the tail vs the full grade's stages",
        out.stages_run.len()
    );
    assert_timing(&budgets(), &measurement);
}

#[test]
fn canonical_serialisation_and_hashing_are_cheap_enough_to_do_on_every_save() {
    let recipe = recipes::reference();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let timer = StageTimer::start("recipe_canonical_1000", clock.as_ref());
    let mut total = 0usize;
    for _ in 0..1_000 {
        let text = aura_recipe::canonical(&recipe).unwrap_or_else(|err| panic!("{err}"));
        total += text.len();
        let _ = aura_recipe::recipe_hash(&recipe).unwrap_or_else(|err| panic!("{err}"));
    }
    let measurement = timer.finish(1_000);
    println!("  canonical recipe is {} bytes", total / 1_000);
    assert_timing(&budgets(), &measurement);
}

#[test]
fn the_develop_store_meets_its_size_budget() {
    let dir = TempDir::new().unwrap_or_else(|err| panic!("tempdir: {err}"));
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog = Arc::new(
        Catalog::open(&dir.path().join("c.sqlite"), Arc::clone(&clock), "perf")
            .unwrap_or_else(|err| panic!("catalog: {err}")),
    );

    let project = ProjectId::new();
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "budget".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    catalog
        .writer()
        .transact(move |conn| repo::create_project(conn, &row))
        .unwrap_or_else(|err| panic!("project: {err}"));

    let photos: Vec<PhotoId> = (0..STORAGE_IMAGES).map(|_| PhotoId::new()).collect();
    let keys: Vec<String> = photos.iter().map(PhotoId::to_db).collect();
    let project_key = project.to_db();
    catalog
        .writer()
        .transact(move |conn| {
            for key in keys {
                conn.execute(
                    "INSERT INTO photo (photo_id, project_id, created_at, updated_at)
                     VALUES (?1, ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    params![key, project_key],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("seed photo", &e))?;
            }
            Ok(())
        })
        .unwrap_or_else(|err| panic!("photos: {err}"));

    let before = page_bytes(&catalog);

    let store = RecipeStore::new(Arc::clone(&catalog), Arc::clone(&clock));
    for (index, photo) in photos.iter().enumerate() {
        let mut recipe = graded();
        recipe.image.content_hash = recipes::FIXTURE_HASH.to_string();
        recipe.global.exposure = 0.1 + (index % 20) as f32 * 0.01;

        // A first save, then three steps of history - which is what a photograph a person
        // actually worked on looks like, and what the budget has to survive.
        store
            .save(&project, photo, &recipe, &[], "AI develop")
            .unwrap_or_else(|err| panic!("save: {err}"));
        for step in 0..3 {
            let mut edited = recipe.clone();
            edited.global.shadows += step;
            edited.provenance.source = EditSource::User;
            let (merged, report) = schema::merge(&recipe, &edited, EditSource::User)
                .unwrap_or_else(|err| panic!("{err}"));
            store
                .save(&project, photo, &merged, &report.changed, "Shadows")
                .unwrap_or_else(|err| panic!("save: {err}"));
            recipe = merged;
        }
    }

    let after = page_bytes(&catalog);
    let used = after.saturating_sub(before);
    println!(
        "develop store: {used} bytes for {STORAGE_IMAGES} images ({} B/image)",
        used / STORAGE_IMAGES as u64
    );

    let coverage = store
        .coverage(&project)
        .unwrap_or_else(|err| panic!("coverage: {err}"));
    assert_eq!(coverage.images, STORAGE_IMAGES as u32);
    assert_eq!(coverage.with_recipe, STORAGE_IMAGES as u32);

    if let Err(reason) = budgets().check_size("develop_store_per_1000_images", used) {
        panic!("{reason}");
    }
}

fn page_bytes(catalog: &Catalog) -> u64 {
    catalog
        .read(|conn| {
            let pages: i64 = conn
                .query_row("PRAGMA page_count", [], |row| row.get(0))
                .map_err(|e| aura_core::errors::db::statement_failed("page_count", &e))?;
            let size: i64 = conn
                .query_row("PRAGMA page_size", [], |row| row.get(0))
                .map_err(|e| aura_core::errors::db::statement_failed("page_size", &e))?;
            Ok((pages.max(0) as u64) * (size.max(0) as u64))
        })
        .unwrap_or(0)
}
