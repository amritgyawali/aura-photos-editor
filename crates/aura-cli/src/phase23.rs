//! The phase 23 mechanical gate.
//!
//! This is the assembly proof for the geometry suite: migration 23 and its objects, the crop rule
//! table and the bundled lens profiles, the bounds the code owns rather than the files, the
//! straightening band end to end, the safety filter's zero-tolerance gate with its denominator
//! beside it, the conservatism gate, the keystone cap, the store, the revert, and the two promises
//! the database keeps rather than the application - a delivered crop that is not one the safety
//! filter refused, and an original framing that is always row zero.
//!
//! **Nothing here proves a crop is good.** Section 10.1's crop rows are written against expert
//! labels on two thousand frames and there are none in this repository - condition C2 of the exit
//! report - and section 9's 300-crop perceptual audit did not happen. Every number below is
//! measured against synthetic frames whose tilt, convergence and subject placement were painted
//! in. The distinction is printed at the end of every run rather than hidden in a test helper.
//!
//! The tests prove the pieces; this proves the assembly. `tests/eval/geometry_eval.rs` is the
//! other half and runs under `cargo test`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::composition::Box2;
use aura_core::contract::geometry::{
    AspectRatio, CropVariant, GeometryCode, GeometryOverride, GeometryService, MAX_STRETCH,
    MIN_IMPROVEMENT, MIN_LONG_EDGE_FRACTION, ROTATE_ACT_AT, ROTATE_MAX_DEG, ROTATE_MIN_DEG,
    SAFETY_MARGIN,
};
use aura_core::{AuraResult, PhotoId, ProjectId, SceneId};
use aura_geometry::decide::Analyser;
use aura_geometry::profiles::CropRules;
use aura_geometry::safety::{self, Limits};
use aura_geometry::store::{GeometryStore, BYTES_PER_IMAGE};
use aura_geometry::{fixtures, keystone, straighten, Geometry};
use rusqlite::params;

/// What a refusal attempt actually did.
///
/// Phase 21's rule: a refusal test that cannot tell a working guard from a broken fixture proves
/// nothing. A statement that failed for a missing foreign key looks exactly like one refused by
/// the promise, so every attempt runs a control first and reports [`Attempt::Inconclusive`] rather
/// than success when the attempt never reached the thing under test.
enum Attempt {
    /// The statement was refused, which is the promise working.
    Refused,
    /// The statement went in, which is the promise broken.
    Allowed,
    /// The attempt never reached the guard.
    Inconclusive(String),
}

/// Run the phase 23 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase23-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. Migration 23 and every object it owns.
    let catalog_path = work.join("phase23.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 23 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 23, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "geometry_plan"),
        ("table", "geometry_crop"),
        ("view", "v_geometry_coverage"),
        ("view", "v_geometry_safety"),
        ("index", "idx_geometry_project"),
        ("index", "idx_geometry_review"),
        ("index", "idx_geometry_acted"),
        ("index", "idx_geometry_lens"),
        ("index", "idx_geometry_crop_safe"),
        ("trigger", "geometry_primary_is_safe_insert"),
        ("trigger", "geometry_primary_is_safe_update"),
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

    // 2. There is nowhere in this schema to upscale, fill a corner or name a second photograph.
    //
    // Section 2.2 puts content-aware fill in phase 24 and panoramas out of scope entirely. The way
    // a phase quietly acquires either is by growing a column for it.
    match forbidden_columns(&catalog) {
        Ok(found) if found.is_empty() => {
            println!("  no scale, fill or source-frame column in migration 23");
        }
        Ok(found) => {
            eprintln!("  migration 23 grew a forbidden column: {}", found.join(", "));
            failures += 1;
        }
        Err(err) => {
            eprintln!("  column scan: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 3. The two tables, and the bounds the code owns rather than the files.
    println!();
    println!("the two tables:");
    let rules = match CropRules::embedded() {
        Ok(table) => {
            println!("  crop rules loaded at version {}", table.version);
            Some(table)
        }
        Err(err) => {
            eprintln!("  crop rules: [{}] {}", err.code, err.detail);
            failures += 1;
            None
        }
    };
    if let Some(table) = rules.as_ref() {
        let missing: Vec<&str> = SceneId::ALL
            .into_iter()
            .filter(|scene| !table.has_row(*scene))
            .map(SceneId::as_str)
            .collect();
        if missing.is_empty() {
            println!("  every scene has a row");
        } else {
            eprintln!("  scenes with no row: {}", missing.join(", "));
            failures += 1;
        }
        let forbidding = SceneId::ALL
            .into_iter()
            .filter(|scene| !table.scene(*scene).crop)
            .count();
        println!(
            "  {forbidding} of {} scenes forbid an automatic crop entirely",
            SceneId::ALL.len()
        );
        if table.bounds.min_long_edge >= MIN_LONG_EDGE_FRACTION
            && table.bounds.min_improvement >= MIN_IMPROVEMENT
            && table.bounds.safety_margin >= SAFETY_MARGIN
            && table.bounds.max_rotate_deg <= ROTATE_MAX_DEG
        {
            println!("  the file's bounds are inside the contract's");
        } else {
            eprintln!("  the file widened a bound");
            failures += 1;
        }

        // The bound is the code's, not the file's. A file that loosened one is refused; this is
        // the same attempt one layer up, so a loader that started clamping instead of refusing
        // fails here.
        let text = std::fs::read_to_string("crates/aura-geometry/config/crop_rules.toml")
            .unwrap_or_default();
        if text.is_empty() {
            println!("  (the on-disk rule file was not readable from here; skipping the raise test)");
        } else {
            for (what, from, to) in [
                ("resolution floor", "min_long_edge = 0.60", "min_long_edge = 0.30"),
                (
                    "improvement margin",
                    "min_improvement = 0.06",
                    "min_improvement = 0.001",
                ),
                ("safety margin", "safety_margin = 0.01", "safety_margin = 0.0"),
                (
                    "rotation ceiling",
                    "max_rotate_deg = 8.0",
                    "max_rotate_deg = 40.0",
                ),
            ] {
                let loosened = text.replacen(from, to, 1);
                if loosened == text {
                    eprintln!("  the {what} line was not found, so the raise test proved nothing");
                    failures += 1;
                    continue;
                }
                match CropRules::parse(&loosened) {
                    Err(_) => println!("  a table that loosened the {what} would be refused"),
                    Ok(_) => {
                        eprintln!("  a table that loosened the {what} would be accepted");
                        failures += 1;
                    }
                }
            }
        }
    }

    let lenses = aura_render::geometry::database();
    if lenses.rows.is_empty() {
        eprintln!("  the bundled lens table did not parse");
        failures += 1;
    } else {
        println!(
            "  {} lens profiles loaded at version {}",
            lenses.rows.len(),
            lenses.version
        );
        if lenses.is_all_reference() {
            println!(
                "  none of the {} rows was measured, so every correction is a reference model \
                 (condition C3)",
                lenses.rows.len()
            );
        } else {
            eprintln!("  a row calls itself measured and nothing in this repository was");
            failures += 1;
        }
    }

    // 4. The straightening band, end to end.
    println!();
    println!("straightening:");
    let analyser = match Analyser::embedded() {
        Ok(analyser) => analyser,
        Err(err) => {
            eprintln!("  analyser: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let (analysis_ver, profile_ver) = analyser.versions();
    println!("  analysis {analysis_ver}, profiles {profile_ver}");
    if profile_ver == 0 {
        eprintln!("  the profile version hashed to zero, which reads as never analysed");
        failures += 1;
    }
    for (label, degrees, confidence, intentional, expected) in [
        ("level", 0.05f32, 0.95f32, false, GeometryCode::TiltNegligible),
        ("off level", 3.0, 0.90, false, GeometryCode::Straightened),
        ("unsure", 3.0, 0.60, false, GeometryCode::HorizonUnsure),
        ("deliberate", 3.0, 0.95, true, GeometryCode::TiltIntentional),
        ("dutch", 20.0, 0.95, false, GeometryCode::TiltTooLarge),
    ] {
        let frame = fixtures::tilted_frame(SceneId::Candid, degrees, confidence, intentional);
        match analyser.plan(&frame) {
            Ok((plan, _)) if plan.has(expected) => {
                println!("  {label}: {expected}");
            }
            Ok((plan, _)) => {
                eprintln!(
                    "  {label}: expected {expected}, got {:?}",
                    plan.reasons.iter().map(|r| r.code).collect::<Vec<_>>()
                );
                failures += 1;
            }
            Err(err) => {
                eprintln!("  {label}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }
    println!(
        "  the band is {ROTATE_MIN_DEG} to {ROTATE_MAX_DEG} deg above {ROTATE_ACT_AT} confidence"
    );

    // 5. The safety filter, and its denominator.
    println!();
    println!("the safety filter:");
    let wedding = fixtures::wedding();
    let mut checked = 0u64;
    let mut cut = 0u64;
    let mut kept = 0usize;
    let mut acted = 0usize;
    let mut plans = Vec::with_capacity(wedding.len());
    for frame in &wedding {
        match analyser.plan(frame) {
            Ok((plan, outcome)) => {
                let delivered = plan.primary();
                let aspect = frame.full_width as f32 / frame.full_height as f32;
                for region in &frame.protected {
                    checked += 1;
                    let projected =
                        straighten::project(region.area, delivered, plan.rotate_deg, aspect);
                    if !safety::rect_inside(projected, delivered, SAFETY_MARGIN) {
                        cut += 1;
                    }
                }
                if outcome.kept_original {
                    kept += 1;
                }
                if outcome.acted {
                    acted += 1;
                }
                plans.push(plan);
            }
            Err(err) => {
                eprintln!("  plan: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }
    if cut == 0 {
        println!("  {cut} of {checked} protected regions were cut");
    } else {
        eprintln!("  {cut} of {checked} protected regions were cut");
        failures += 1;
    }
    if checked == 0 {
        eprintln!("  nothing was protected, so the zero above is arithmetic rather than evidence");
        failures += 1;
    }
    // The floor, on every safe variant of every plan.
    let mut below = 0usize;
    for (plan, frame) in plans.iter().zip(wedding.iter()) {
        let aspect = frame.full_width as f32 / frame.full_height as f32;
        for variant in &plan.crops {
            if variant.safe && variant.long_edge_fraction(aspect) < MIN_LONG_EDGE_FRACTION - 1e-4 {
                below += 1;
            }
        }
    }
    if below == 0 {
        println!("  every safe variant keeps at least {MIN_LONG_EDGE_FRACTION} of the long edge");
    } else {
        eprintln!("  {below} safe variants fell below the resolution floor");
        failures += 1;
    }

    // 6. Conservatism.
    println!();
    println!("conservatism:");
    let conservatism = kept as f32 / wedding.len().max(1) as f32;
    if conservatism >= 0.70 {
        println!(
            "  {kept} of {} frames keep the framing they were shot at ({conservatism:.2})",
            wedding.len()
        );
    } else {
        eprintln!(
            "  only {conservatism:.2} of the fixture wedding kept its original framing; \
             section 10.1 asks for 0.70"
        );
        failures += 1;
    }
    println!("  {acted} frames had at least one pixel moved");

    // 7. The keystone cap.
    println!();
    println!("the keystone cap:");
    let mut over = 0usize;
    for convergence in [0.4f32, 0.6, 0.8, 1.0] {
        for aspect in [1.5f32, 0.8, 16.0 / 9.0] {
            let correction = keystone::solve(
                keystone::Verticals {
                    convergence,
                    share: 0.4,
                },
                aspect,
                &[],
                Limits {
                    frame_aspect: aspect,
                    ..Limits::default()
                },
            );
            if correction
                .keystone
                .is_some_and(|k| k.stretch > MAX_STRETCH + 1e-4)
            {
                over += 1;
            }
        }
    }
    if over == 0 {
        println!("  no correction exceeded {MAX_STRETCH} at any convergence or frame shape");
    } else {
        eprintln!("  {over} corrections exceeded the cap");
        failures += 1;
    }

    // 8. The store: a round trip, the two triggers, and the budget.
    println!();
    println!("the store:");
    let project = ProjectId::new();
    let photos: Vec<PhotoId> = wedding.iter().map(|frame| frame.image_id).collect();
    if let Err(err) = seed(&catalog, &project, &photos) {
        eprintln!("  seed: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }
    let store = Arc::new(GeometryStore::new(
        Arc::clone(&catalog),
        Arc::clone(&clock),
    ));
    let mut written = 0usize;
    for plan in &plans {
        match store.put(&project, plan) {
            Ok(()) => written += 1,
            Err(err) => {
                eprintln!("  put: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }
    println!("  {written} plans written");

    let service = match Geometry::new(Arc::clone(&store)) {
        Ok(service) => service,
        Err(err) => {
            eprintln!("  service: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match service.outline(project) {
        Ok(outline) => {
            println!(
                "  outline: {} planned of {} photographs, {} kept original, conservatism {:.2}",
                outline.planned,
                outline.photos,
                outline.kept_original,
                outline.conservatism()
            );
            if outline.faces_cut > 0 {
                eprintln!("  the outline reports {} cut faces", outline.faces_cut);
                failures += 1;
            }
            if outline.faces_checked == 0 {
                eprintln!("  the outline checked no faces, so its zero means nothing");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("  outline: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // The round trip. A plan written and read back must agree about the things a later phase
    // reads: the delivered rectangle, the aspect list and the versions.
    if let Some(first) = plans.first() {
        match service.of_image(first.image_id) {
            Ok(Some(back)) => {
                let same = back.primary() == first.primary()
                    && back.crops.len() == first.crops.len()
                    && back.analysis_ver == first.analysis_ver
                    && back.profile_ver == first.profile_ver
                    && back.lens == first.lens;
                if same {
                    println!("  a plan round-trips through the catalog");
                } else {
                    eprintln!("  a plan changed on the way through the catalog");
                    failures += 1;
                }
            }
            Ok(None) => {
                eprintln!("  a plan that was written could not be read back");
                failures += 1;
            }
            Err(err) => {
                eprintln!("  read back: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // The promise the database keeps rather than the application.
    if let Some(photo) = photos.first() {
        match deliver_an_unsafe_crop(&catalog, photo) {
            Ok(Attempt::Refused) => {
                println!("  the database refuses to deliver a crop the safety filter refused");
            }
            Ok(Attempt::Allowed) => {
                eprintln!("  the database allowed an unsafe crop to be delivered");
                failures += 1;
            }
            Ok(Attempt::Inconclusive(why)) => {
                eprintln!("  the unsafe-crop attempt never reached the trigger: {why}");
                failures += 1;
            }
            Err(err) => {
                eprintln!("  unsafe crop: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 9. The revert, which section 13 calls "always one click away".
    println!();
    println!("the revert:");
    if let Some(photo) = photos.first() {
        let tightened = GeometryOverride {
            crop: Some(Box2 {
                x: 0.1,
                y: 0.1,
                w: 0.7,
                h: 0.7,
            }),
            ..GeometryOverride::default()
        };
        match service.set_override(*photo, &tightened) {
            Ok(plan) if plan.user_edited && !plan.primary().is_empty() => {
                println!("  a photographer's rectangle is stored with user_edited set");
            }
            Ok(_) => {
                eprintln!("  an override did not set user_edited");
                failures += 1;
            }
            Err(err) => {
                eprintln!("  override: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
        // And a re-analysis must not touch it.
        match store.pending(&project, (analysis_ver, profile_ver.wrapping_add(1))) {
            Ok(pending) if !pending.contains(photo) => {
                println!("  a hand-framed photograph is never offered to a re-analysis");
            }
            Ok(_) => {
                eprintln!("  a hand-framed photograph was offered to a re-analysis");
                failures += 1;
            }
            Err(err) => {
                eprintln!("  pending: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
        match service.set_override(*photo, &GeometryOverride::reverted()) {
            Ok(plan) => {
                let restored = plan.primary() == Box2::FULL
                    && plan.rotate_deg.abs() < f32::EPSILON
                    && plan.keystone.is_none()
                    && !plan.user_edited;
                if restored {
                    println!("  reverting restores the exact framing and lets automation resume");
                } else {
                    eprintln!("  reverting did not restore the original framing");
                    failures += 1;
                }
            }
            Err(err) => {
                eprintln!("  revert: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 10. The storage budget.
    println!();
    println!("storage:");
    match measured_bytes(&catalog, written) {
        Ok(bytes) if bytes <= BYTES_PER_IMAGE => {
            println!("  {bytes} B per image against a budget of {BYTES_PER_IMAGE} B");
        }
        Ok(bytes) => {
            eprintln!("  {bytes} B per image, above the budget of {BYTES_PER_IMAGE} B");
            failures += 1;
        }
        Err(err) => {
            eprintln!("  budget: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 11. What this gate does not prove.
    println!();
    println!("what this build does not have:");
    println!(
        "  - no expert crop labels. Section 9's DATA row asks for 2,000 and this repository has \
         none, so every crop number above is a statement about the safety filter and the \
         improvement margin rather than about whether a photographer would prefer AURA's \
         rectangle. Condition C2."
    );
    println!(
        "  - no perceptual audit. Section 9 gives QAIQ 300 auto-crops to look at and that did not \
         happen, so the phase's own headline is proven for faces and unproven for framing quality."
    );
    println!(
        "  - no measured lens profile. Every row in `assets/lens_profiles/` is a reference model \
         for a class or a family, and `ATTRIBUTION.md` says so. Condition C3."
    );
    println!(
        "  - no protected hands and no protected moment key. Phase 11's keypoint head is a \
         placeholder and phase 08 records a key frame rather than a key region, so two of the five \
         `ProtectedContent` kinds are never filled. The scenes where they matter most are the \
         scenes `crop_rules.toml` forbids cropping in. Condition C4."
    );
    println!(
        "  - phase 06's detector is a placeholder, so on a real photograph in this build \
         `CropSafetyReport::considered` is zero. Condition C1, which closes with phase 05's C10."
    );

    println!();
    if failures == 0 {
        println!("phase 23: OK");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase 23: {failures} failure(s)");
        ExitCode::FAILURE
    }
}

/// True when a schema object of this kind and name exists.
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
            .unwrap_or(0);
        Ok(count > 0)
    })
}

/// Any column in this migration that could carry a scale, a fill or a second photograph.
fn forbidden_columns(catalog: &Catalog) -> AuraResult<Vec<String>> {
    catalog.read(|conn| {
        let mut found = Vec::new();
        for table in ["geometry_plan", "geometry_crop"] {
            let mut statement = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .map_err(|e| aura_core::errors::db::statement_failed("table_info", &e))?;
            let mut rows = statement
                .query([])
                .map_err(|e| aura_core::errors::db::statement_failed("table_info", &e))?;
            while let Some(row) = rows
                .next()
                .map_err(|e| aura_core::errors::db::statement_failed("table_info", &e))?
            {
                let column: String = row.get(1).unwrap_or_default();
                let lower = column.to_lowercase();
                for banned in [
                    "scale", "upscale", "fill", "synth", "source_photo", "donor", "patch",
                    "resolution_px", "output_width", "output_height",
                ] {
                    if lower.contains(banned) {
                        found.push(format!("{table}.{column}"));
                    }
                }
            }
        }
        Ok(found)
    })
}

/// Try to point a plan's delivered crop at a rectangle the safety filter refused.
///
/// Phase 21's rule: the control runs first. Without it a statement refused for a missing foreign
/// key would look exactly like one refused by the promise, and the gate would report a working
/// guard over a broken fixture.
fn deliver_an_unsafe_crop(catalog: &Catalog, photo: &PhotoId) -> AuraResult<Attempt> {
    let photo_key = photo.to_db();
    catalog.writer().transact(move |conn| {
        // The control: an ordinary *safe* variant at ordinal 4, which must go in.
        if let Err(err) = conn.execute(
            "INSERT OR REPLACE INTO geometry_crop
                 (photo_id, ordinal, aspect, purpose, rect_x, rect_y, rect_w, rect_h, score,
                  safe, refusal)
             VALUES (?1, 4, '16:9', 'album', 0.0, 0.2, 1.0, 0.5, 0.5, 1, NULL)",
            params![photo_key],
        ) {
            return Ok(Attempt::Inconclusive(format!(
                "an ordinary safe variant would not insert either: {err}"
            )));
        }
        // Point the plan at it, which must also succeed while it is safe.
        if conn
            .execute(
                "UPDATE geometry_plan SET primary_crop = 4 WHERE photo_id = ?1",
                params![photo_key],
            )
            .is_err()
        {
            return Ok(Attempt::Inconclusive(
                "the delivered crop could not be moved to a safe variant".to_string(),
            ));
        }
        // Now the one that must be refused: making the delivered variant unsafe.
        match conn.execute(
            "UPDATE geometry_crop SET safe = 0, refusal = 'geometry_crop_cuts_face'
              WHERE photo_id = ?1 AND ordinal = 4",
            params![photo_key],
        ) {
            Ok(_) => Ok(Attempt::Allowed),
            Err(_) => Ok(Attempt::Refused),
        }
    })
}

/// Bytes per image in `geometry_plan`, `geometry_crop` and their indexes.
///
/// `dbstat` payload rather than `PRAGMA page_count`, which quantises to 4 KiB - phase 19's
/// correction, and the reason phase 09's budget read "exactly 1,024" for ten phases.
fn measured_bytes(catalog: &Catalog, images: usize) -> AuraResult<usize> {
    if images == 0 {
        return Ok(0);
    }
    catalog.read(move |conn| {
        let payload: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(payload), 0) FROM dbstat
                  WHERE name IN ('geometry_plan', 'geometry_crop',
                                 'idx_geometry_project', 'idx_geometry_review',
                                 'idx_geometry_acted', 'idx_geometry_lens',
                                 'idx_geometry_crop_safe')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok((payload.max(0) as usize) / images)
    })
}

/// A project and its photographs.
fn seed(catalog: &Catalog, project: &ProjectId, photos: &[PhotoId]) -> AuraResult<()> {
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "phase 23".to_string(),
        couple_label: Some("A and B".to_string()),
        event_date: Some("2026-08-25".to_string()),
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-08-25T00:00:00Z".to_string(),
        updated_at: "2026-08-25T00:00:00Z".to_string(),
    };
    catalog
        .writer()
        .transact(move |conn| repo::create_project(conn, &row))?;

    let ids: Vec<String> = photos.iter().map(PhotoId::to_db).collect();
    let project_key = project.to_db();
    catalog.writer().transact(move |tx| {
        for (index, photo) in ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                    camera_make, camera_model, iso, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3, 'CANON', 'EOS R5', 400,
                         '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z')",
                params![
                    photo,
                    project_key,
                    format!("2026-08-25T10:{:02}:00Z", index % 60),
                ],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
        }
        Ok(())
    })
}

/// The aspects a plan is expected to carry, for the panel and the gate.
///
/// One original plus the four section 2.1 names. Written here rather than inlined because the gate
/// prints it and a reader should be able to see the list without reading a loop.
#[must_use]
pub fn expected_aspects() -> Vec<AspectRatio> {
    AspectRatio::ALL.to_vec()
}

/// The variant a plan delivers, for the gate's own reporting.
#[must_use]
pub fn delivered(plan: &aura_core::contract::geometry::GeometryPlan) -> Option<&CropVariant> {
    plan.crops.get(plan.primary_crop)
}
