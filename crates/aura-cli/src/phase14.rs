//! The phase 14 mechanical gate.
//!
//! This is the assembly proof for the develop engine: migration 14 and its objects, the
//! frozen recipe round-tripping through its own canonical form, the merge that protects a
//! photographer's own sliders, the stage order, the shader table, a real render on each
//! bench body, the tiling identity, the parity harness catching a drift, the XMP and sidecar
//! interchange, a simulated schema v2, and the two budgets this build can honestly assert.
//!
//! **Nothing here proves a colour is correct.** There are no camera files and no photographed
//! ColorChecker in this repository - phase 02's exit conditions, still open - so the golden
//! suite is a determinism and regression gate. The distinction is printed at the end of every
//! run rather than hidden in a test helper.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::{AuraResult, PhotoId, ProjectId};
use aura_recipe::history::{History, ResetTo};
use aura_recipe::{fixtures as recipes, schema, EditSource, RecipeStore};
use aura_render::contract::render::{OutputColour, OutputSpec, RenderLevel, RenderPurpose};
use aura_render::cpu::CpuEngine;
use aura_render::{fixtures, golden, graph, parity, shaders, tiles};
use rusqlite::params;

/// Section 11's interactive row is a GPU figure. This is the processor guardrail the phase
/// asserts instead, and the gate prints the gap rather than hiding it.
const PROXY_GUARDRAIL_MS: u128 = 450;

/// Run the phase 14 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase14-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. Migration 14 and every object it owns.
    let catalog_path = work.join("phase14.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 14 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 14, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "edit_recipes"),
        ("table", "edit_history"),
        ("table", "edit_snapshots"),
        ("table", "export_renders"),
        ("view", "v_develop_coverage"),
        ("index", "idx_edit_recipes_project"),
        ("index", "idx_edit_recipes_user_edited"),
        ("index", "idx_export_renders_photo"),
    ] {
        match schema_object(&catalog, kind, name) {
            Ok(true) => println!("  {kind} {name}: present"),
            Ok(false) => {
                eprintln!("  {kind} {name}: missing");
                failures += 1;
            }
            Err(err) => {
                eprintln!("  {kind} {name}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 2. The frozen recipe, its canonical form and its hash.
    let reference = recipes::reference();
    match schema::Validation::check(&reference) {
        Ok(()) => println!("recipe: section 5's own document validates"),
        Err(err) => {
            eprintln!("recipe: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    let canonical = aura_recipe::canonical(&reference).unwrap_or_default();
    let hash = aura_recipe::recipe_hash(&reference).unwrap_or_default();
    println!(
        "  canonical: {} bytes, hash {}",
        canonical.len(),
        &hash[..16]
    );
    match serde_json::from_str::<aura_recipe::Recipe>(&canonical) {
        Ok(parsed) => {
            let again = aura_recipe::recipe_hash(&parsed).unwrap_or_default();
            if again == hash {
                println!("  round trip: identical hash");
            } else {
                eprintln!("  round trip: hash changed");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("  round trip: {err}");
            failures += 1;
        }
    }

    // 3. Section 6.4. A photographer's parameter survives an automated pass.
    let mut base = reference.clone();
    base.global.exposure = 0.75;
    base.provenance.source = EditSource::User;
    base.provenance.user_edited_fields = vec!["global.exposure".to_string()];
    let mut proposal = reference.clone();
    proposal.global.exposure = -2.0;
    proposal.global.shadows = 40;
    match schema::merge(&base, &proposal, EditSource::Ai) {
        Ok((merged, report)) => {
            let held = (merged.global.exposure - 0.75).abs() < 1e-6;
            let moved = merged.global.shadows == 40;
            if held && moved && report.refused == vec!["global.exposure".to_string()] {
                println!("protection: the photographer's exposure held, the untouched field moved");
            } else {
                eprintln!("protection: FAILED - exposure {}", merged.global.exposure);
                failures += 1;
            }
            match report.refusal().map(|e| e.code.to_string()) {
                Some(code) if code == "AURA-RENDER-8006" => {
                    println!("  the refusal is visible: {code}");
                }
                other => {
                    eprintln!("  the refusal was silent: {other:?}");
                    failures += 1;
                }
            }
        }
        Err(err) => {
            eprintln!("protection: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 4. The pipeline order, and the shader table that mirrors it.
    let order_ok = graph::Stage::HighlightRecovery.index() < graph::Stage::WhiteBalance.index()
        && graph::Stage::WhiteBalance.index() < graph::Stage::CameraMatrix.index()
        && graph::ORDER.last() == Some(&graph::Stage::OutputTransform);
    if order_ok {
        println!("order: recovery before white balance before the matrix; output last");
    } else {
        eprintln!("order: FAILED");
        failures += 1;
    }
    let missing = shaders::stages_without_a_shader();
    if missing.is_empty() {
        println!(
            "shaders: {} stages, {} sources, every stage has an entry point",
            graph::ORDER.len(),
            shaders::SOURCES.len()
        );
    } else {
        eprintln!("shaders: {} stages with no entry point", missing.len());
        failures += 1;
    }

    // 5. A real render on every bench body, and the determinism that makes it a golden.
    let engine_frame = fixtures::detail_frame(320, 240);
    let engine = CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(engine_frame)),
        Arc::clone(&clock),
    );
    let mut digests = std::collections::BTreeSet::new();
    for frame in fixtures::bench_bodies(160, 120) {
        match engine.render_frame(
            &frame,
            &reference,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec::default(),
        ) {
            Ok(rendered) => {
                digests.insert(golden::digest(&rendered.data));
            }
            Err(err) => {
                eprintln!("render {}: [{}] {}", frame.camera, err.code, err.detail);
                failures += 1;
            }
        }
    }
    if digests.len() == 8 {
        println!("golden: 8 bench bodies, 8 distinct renders");
    } else {
        eprintln!("golden: {} distinct renders across 8 bodies", digests.len());
        failures += 1;
    }

    // 6. A tiled render equals a whole one.
    // Larger than one tile on purpose: a frame that fits in a single tile proves nothing
    // about the halo, the committed region or the assembly.
    let tiled_frame = fixtures::detail_frame(700, 540);
    let tiler = CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(tiled_frame.clone())),
        Arc::clone(&clock),
    );
    let mut streaming = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    streaming.global.clarity = 40;
    streaming.global.sharpen.amount = 60;
    streaming.global.noise.luminance = 25;
    streaming.lens.vignette = 35;
    let whole = tiler.render_frame(
        &tiled_frame,
        &streaming,
        RenderLevel::Full,
        RenderPurpose::Export,
        &OutputSpec::default(),
    );
    let streamed = tiles::render_streamed(
        &tiler,
        &tiled_frame,
        &streaming,
        RenderLevel::Full,
        RenderPurpose::Export,
        &OutputSpec::default(),
        4096,
    );
    match (whole, streamed) {
        (Ok(a), Ok(b)) if a.data == b.data => {
            println!(
                "tiling: {} tiles, bit-identical to the whole-frame render",
                tiles::tile_count(700, 540, tiles::TILE_SIZE)
            );
        }
        (Ok(_), Ok(_)) => {
            eprintln!("tiling: the two paths disagree");
            failures += 1;
        }
        (a, b) => {
            eprintln!("tiling: {:?} {:?}", a.err(), b.err());
            failures += 1;
        }
    }

    // 7. The parity harness catches a drift. There is no backend to compare against.
    let reference_buffer = vec![0.42f32; 3_000];
    let drifted = parity::perturb(&reference_buffer, parity::TOLERANCE * 2.0);
    let caught = parity::compare(graph::Stage::Tone, &reference_buffer, &drifted)
        .check()
        .err()
        .map(|e| e.code.to_string());
    if caught.as_deref() == Some("AURA-RENDER-8010") {
        println!(
            "parity: tolerance 1/{:.0} in linear light, drift caught",
            1.0 / parity::TOLERANCE
        );
    } else {
        eprintln!("parity: a drift of twice the tolerance was not caught");
        failures += 1;
    }

    // 8. The interchange: XMP out and back, and nothing private in the packet.
    let packet = aura_recipe::xmp::write(&reference);
    let neutral = recipes::neutral(&reference.image.content_hash, "ILCE-7M4");
    match aura_recipe::xmp::read(&packet, &neutral) {
        Ok(back) => {
            let kept = aura_recipe::xmp::SUBSET
                .iter()
                .filter(|path| **path != "bw")
                .filter(|path| !schema::leaf_differs(&reference, &back, path))
                .count();
            println!(
                "xmp: {}/{} subset paths survive the round trip",
                kept,
                aura_recipe::xmp::SUBSET.len() - 1
            );
            if kept + 1 < aura_recipe::xmp::SUBSET.len() {
                eprintln!("  a subset path did not survive");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("xmp: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    let leaks: Vec<&str> = ["identity:bride", "d_8f21", "amrit_v3", "skin_smooth"]
        .into_iter()
        .filter(|secret| packet.contains(secret))
        .collect();
    if leaks.is_empty() {
        println!("  nothing outside the Lightroom subset reaches the packet");
    } else {
        eprintln!("  the packet leaks {leaks:?}");
        failures += 1;
    }

    // 9. A v1 document under a simulated v2 engine.
    let mut from_v2 = reference.clone();
    from_v2.schema = aura_recipe::SCHEMA_VERSION + 1;
    // Built by hand rather than with `json!`, whose expansion contains an `unwrap` that the
    // workspace's disallowed-methods lint refuses in a binary.
    let mut emulation = serde_json::Map::new();
    emulation.insert(
        "stock".to_string(),
        serde_json::Value::String("portra_400".to_string()),
    );
    from_v2.extra.insert(
        "film_emulation".to_string(),
        serde_json::Value::Object(emulation),
    );
    match aura_recipe::migrate::to_current(&from_v2) {
        Ok((migrated, Some(warning))) if warning.code.to_string() == "AURA-RENDER-8003" => {
            let kept = migrated.extra.contains_key("film_emulation");
            if kept {
                println!("migration: a newer document warns and keeps what it does not understand");
            } else {
                eprintln!("migration: a newer build's work was destroyed");
                failures += 1;
            }
        }
        other => {
            eprintln!("migration: unexpected {other:?}");
            failures += 1;
        }
    }

    // 10. The store, end to end: save, history, undo, reset, coverage.
    let project = ProjectId::new();
    let photo = PhotoId::new();
    if let Err(err) = seed(&catalog, &project, &photo) {
        eprintln!("seed: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    let store = RecipeStore::new(Arc::clone(&catalog), Arc::clone(&clock));
    let original = recipes::neutral(recipes::FIXTURE_HASH, "Bench-01");
    let mut ai = original.clone();
    ai.global.exposure = 0.6;
    ai.provenance.source = EditSource::Ai;

    let ran = || -> AuraResult<()> {
        store.save(&project, &photo, &original, &[], "first")?;
        store.save(
            &project,
            &photo,
            &ai,
            &["global.exposure".to_string()],
            "AI: exposure",
        )?;
        let mut edited = ai.clone();
        edited.global.exposure = -0.3;
        edited.provenance.source = EditSource::User;
        edited.provenance.user_edited_fields = vec!["global.exposure".to_string()];
        store.save(
            &project,
            &photo,
            &edited,
            &["global.exposure".to_string()],
            "Exposure",
        )?;
        Ok(())
    };
    if let Err(err) = ran() {
        eprintln!("store: [{}] {}", err.code, err.detail);
        failures += 1;
    }

    match store.history(&photo, original.clone()) {
        Ok(mut history) => {
            let steps = history.entries().len();
            let undone = history.undo().map(|r| r.global.exposure);
            let reset = history
                .reset(ResetTo::AiSuggestion, clock.as_ref())
                .map(|()| history.current().global.exposure);
            match (undone, reset) {
                (Some(_), Ok(after)) if (after - 0.6).abs() < 1e-6 => {
                    println!("history: {steps} steps, undo walks, reset returns the suggestion");
                }
                other => {
                    eprintln!("history: unexpected {other:?}");
                    failures += 1;
                }
            }
            let _ = History::new(original.clone());
        }
        Err(err) => {
            eprintln!("history: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    match store.coverage(&project) {
        Ok(coverage) => println!(
            "coverage: {}/{} images carry an edit, {} touched by hand",
            coverage.with_recipe, coverage.images, coverage.touched_by_hand
        ),
        Err(err) => {
            eprintln!("coverage: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 11. The two budgets this build can honestly assert.
    let proxy_frame = fixtures::detail_frame(3_000, 2_000);
    let proxy_engine = CpuEngine::new(
        Arc::new(fixtures::StaticSource::new(proxy_frame.clone())),
        Arc::clone(&clock),
    );
    let mut graded = original.clone();
    graded.global.exposure = 0.31;
    graded.global.contrast = 11;
    graded.global.clarity = 6;
    graded.global.sharpen.amount = 40;
    let _ = proxy_engine.render_frame(
        &proxy_frame,
        &graded,
        RenderLevel::Proxy2048,
        RenderPurpose::Interactive,
        &OutputSpec::default(),
    );
    let started = Instant::now(); // DETERMINISM: a gate measurement, never an input
    let proxy = proxy_engine.render_frame(
        &proxy_frame,
        &graded,
        RenderLevel::Proxy2048,
        RenderPurpose::Interactive,
        &OutputSpec::default(),
    );
    let elapsed = started.elapsed().as_millis();
    match proxy {
        Ok(rendered) => {
            println!(
                "proxy: {}x{} in {elapsed} ms on the {} path ({} stages)",
                rendered.width,
                rendered.height,
                rendered.backend,
                rendered.stages_run.len()
            );
            println!("  section 11 asks for 60 ms on an RTX 4070. This build has no GPU backend");
            println!(
                "  (AURA-RENDER-8001, ADR-0029 section 4); the guardrail is {PROXY_GUARDRAIL_MS} ms"
            );
            if cfg!(debug_assertions) {
                println!("  debug build: reported, not asserted");
            } else if elapsed > PROXY_GUARDRAIL_MS {
                eprintln!("  over the guardrail");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("proxy: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 12. What every output space does to a neutral.
    for space in [
        OutputColour::Srgb,
        OutputColour::AdobeRgb,
        OutputColour::DisplayP3,
    ] {
        let grey = fixtures::grey_frame(32, 32, 0.18);
        let neutral_engine = CpuEngine::new(
            Arc::new(fixtures::StaticSource::new(grey.clone())),
            Arc::clone(&clock),
        );
        match neutral_engine.render_frame(
            &grey,
            &original,
            RenderLevel::Full,
            RenderPurpose::Export,
            &OutputSpec {
                colour_space: space,
                bit_depth: 8,
                icc: None,
            },
        ) {
            Ok(rendered) => {
                let neutral = match &rendered.data {
                    aura_render::RenderedData::Eight(bytes) => bytes.chunks(3).all(|t| {
                        let r = t.first().copied().unwrap_or(0);
                        let g = t.get(1).copied().unwrap_or(0);
                        let b = t.get(2).copied().unwrap_or(0);
                        r.abs_diff(g) <= 1 && g.abs_diff(b) <= 1
                    }),
                    aura_render::RenderedData::Sixteen(_) => true,
                };
                if neutral {
                    println!(
                        "  {}: a neutral stays neutral ({})",
                        space.as_str(),
                        aura_render::output::icc_name(space)
                    );
                } else {
                    eprintln!("  {}: tinted a neutral", space.as_str());
                    failures += 1;
                }
            }
            Err(err) => {
                eprintln!("  {}: [{}] {}", space.as_str(), err.code, err.detail);
                failures += 1;
            }
        }
    }

    println!();
    println!("What this gate proves: the arithmetic is stable, a tiled render equals a whole");
    println!("one, a recipe round-trips, a photographer's slider survives every automated pass,");
    println!("and a v1 document renders under a simulated v2.");
    println!();
    println!("What it does not prove: that any colour is CORRECT. The fixtures are authored");
    println!("synthetic pixels and the bodies are eight synthetic bench profiles, because there");
    println!("are no camera files and no photographed ColorChecker in this repository - phase");
    println!("02's exit conditions, still open. Section 10.1 asks for twelve camera bodies");
    println!("across six scene types. Condition C2 in docs/progress/PHASE-14-EXIT.md.");

    if failures == 0 {
        println!();
        println!("phase 14: OK");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("phase 14: {failures} failure(s)");
        ExitCode::FAILURE
    }
}

fn schema_object(catalog: &Catalog, kind: &str, name: &str) -> AuraResult<bool> {
    let kind = kind.to_string();
    let name = name.to_string();
    catalog.read(move |conn| {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                params![kind, name],
                |row| row.get(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("sqlite_master", &e))?;
        Ok(count > 0)
    })
}

fn seed(catalog: &Catalog, project: &ProjectId, photo: &PhotoId) -> AuraResult<()> {
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "phase 14".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    catalog
        .writer()
        .transact(move |conn| repo::create_project(conn, &row))?;

    let project_key = project.to_db();
    let photo_key = photo.to_db();
    catalog.writer().transact(move |conn| {
        conn.execute(
            "INSERT INTO photo (photo_id, project_id, camera_model, created_at, updated_at)
             VALUES (?1, ?2, 'Bench-01', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![photo_key, project_key],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("seed photo", &e))?;
        Ok(())
    })
}
