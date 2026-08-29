//! The phase 23 mechanical gate.
//!
//! This is the assembly proof for the geometry suite: migration 20 and its objects, the crop
//! rules a product manager has to be able to argue with, the bundled lens profile table, the
//! straightening gates, the keystone cap, the safety filter, the store and its override
//! protection, and what happens to a whole synthetic wedding.
//!
//! **Nothing here proves a photographer would agree with a crop.** Section 10.1's QAIQ audit
//! is three hundred auto-crops looked at by a person and there is no such audit in this
//! repository - it is condition C1 of the exit report. Every number below is measured against
//! synthetic frames whose geometry was chosen and then painted in. The distinction is printed
//! at the end of every run rather than hidden in a test helper.
//!
//! The tests prove the pieces; this proves the assembly. `tests/eval/geometry_eval.rs` is the
//! other half and runs under `cargo test`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::geometry::{
    CropPurpose, GeometryOverride, GeometryService, MAX_ROTATE_DEG, MAX_STRETCH, MIN_ROTATE_DEG,
    RESOLUTION_FLOOR, STRAIGHTEN_ACT_AT,
};
use aura_core::contract::integrity::CropRect;
use aura_core::{AuraResult, ProjectId, SceneId};
use aura_geometry::plan::{Planner, ANALYSIS_VER};
use aura_geometry::profiles::{ProfileTable, PROFILE_DIR};
use aura_geometry::rules::CropRules;
use aura_geometry::store::GeometryStore;
use aura_geometry::{fixtures, guard, keystone, lens, Geometry};
use rusqlite::params;

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

    // 1. Migration 20 and every object it owns.
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
        Ok(version) if version >= 20 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 20, found {version}");
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
        ("index", "idx_geometry_versions"),
        ("index", "idx_geometry_lens"),
        ("index", "idx_geometry_review"),
        ("index", "idx_geometry_crop_purpose"),
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

    // 2. There is nowhere in this schema to put a pixel, a path or an applied flag.
    //
    // Three boundaries, all structural. Phase 14 owns what a recipe means, phase 24 owns
    // filling a corner, and nothing anywhere in this product writes a rendered file beside a
    // decision. The way a phase quietly acquires somebody else's job is by growing a column
    // for it.
    match forbidden_columns(&catalog) {
        Ok(found) if found.is_empty() => {
            println!("  no pixel, path, fill or applied column anywhere in migration 20");
        }
        Ok(found) => {
            eprintln!(
                "  migration 20 grew a forbidden column: {}",
                found.join(", ")
            );
            failures += 1;
        }
        Err(err) => {
            eprintln!("  column scan: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 3. The crop rules, and the rows a product manager has to be able to argue with.
    println!();
    let rules = match CropRules::shipped() {
        Ok(table) => {
            println!(
                "crop rules: version {} loaded, {} scenes",
                table.version(),
                table.len()
            );
            let croppable = SceneId::ALL
                .into_iter()
                .filter(|scene| table.for_scene(*scene).0.crop)
                .count();
            if croppable * 2 <= SceneId::ALL.len() {
                println!(
                    "  {croppable} of {} scenes may be cropped at all - conservative by design",
                    SceneId::ALL.len()
                );
            } else {
                eprintln!(
                    "  {croppable} of {} scenes may be cropped, which is not conservative",
                    SceneId::ALL.len()
                );
                failures += 1;
            }
            // The six rows a future edit must not quietly flip.
            for scene in [
                SceneId::Kiss,
                SceneId::Ritual,
                SceneId::FamilyPortrait,
                SceneId::GroupPortrait,
                SceneId::DanceFloor,
                SceneId::Unknown,
            ] {
                if table.for_scene(scene).0.crop {
                    eprintln!("  {scene} may be cropped, and it must not be");
                    failures += 1;
                }
            }
            println!("  the kiss, the rites, the group rows and the abstention are never cropped");
            let unpolicied = table.unpolicied();
            if unpolicied.is_empty() {
                println!("  every scene in the vocabulary has a row with a written reason");
            } else {
                eprintln!("  scenes with no row: {unpolicied:?}");
                failures += 1;
            }
            // The loader may only tighten.
            let loose = "[defaults]\nreason = \"d\"\n[[scene]]\nid = \"candid\"\ncrop = true\n\
                         resolution_floor = 0.40\nreason = \"loosened\"\n";
            if CropRules::parse(loose).is_err() {
                println!("  a row that loosens the resolution floor is refused (AURA-ML-5093)");
            } else {
                eprintln!("  a row loosened a safety floor and the loader accepted it");
                failures += 1;
            }
            Some(table)
        }
        Err(err) => {
            eprintln!("crop rules: [{}] {}", err.code, err.detail);
            failures += 1;
            None
        }
    };

    // 4. The bundled lens profile table, and its attribution.
    println!();
    let profiles = {
        let dir = PathBuf::from(PROFILE_DIR);
        match ProfileTable::load_dir(&dir) {
            Ok(table) => {
                println!(
                    "lens profiles: version {} loaded, {} lenses",
                    table.version(),
                    table.len()
                );
                if table.is_synthetic() {
                    println!(
                        "  EVERY PROFILE IS SYNTHETIC. No lens was measured to produce this table."
                    );
                }
                if table.attribution().is_empty() {
                    eprintln!("  the table carries no attribution");
                    failures += 1;
                } else {
                    println!("  attribution: {}", table.attribution());
                }
                let mut bad = ProfileTable::empty();
                let no_credit = "[[lens]]\nid = \"X\"\nmeasured_by = \"\"\n\
                                 [[lens.entry]]\nfocal_mm = 50.0\nk1 = 0.0\n";
                if bad.merge_str(no_credit, "gate").is_err() {
                    println!("  a profile with no `measured_by` is refused");
                } else {
                    eprintln!("  a profile with no attribution was accepted");
                    failures += 1;
                }
                table
            }
            Err(err) => {
                eprintln!("lens profiles: [{}] {}", err.code, err.detail);
                failures += 1;
                ProfileTable::empty()
            }
        }
    };

    // 5. The straightening gates and the keystone cap, as arithmetic.
    println!();
    println!(
        "gates: straighten at {STRAIGHTEN_ACT_AT:.2} confidence, band \
         {MIN_ROTATE_DEG:.2} to {MAX_ROTATE_DEG:.1} degrees, keystone stretch \
         capped at {MAX_STRETCH:.2}, resolution floor {RESOLUTION_FLOOR:.2}"
    );
    for ratio in [0.55f32, 0.40] {
        if keystone::decide(&fixtures::converging(ratio, 6), 0.6667)
            .keystone
            .is_some()
        {
            eprintln!("  a keystone at convergence {ratio} survived the cap");
            failures += 1;
        }
    }
    println!("  a keystone past the cap is refused rather than reduced to it");
    if keystone::decide(&fixtures::converging(0.88, 2), 0.6667)
        .keystone
        .is_some()
    {
        eprintln!("  two lines were treated as a vanishing point");
        failures += 1;
    } else {
        println!("  two lines are never a vanishing point");
    }

    // 6. The manual-lens estimator: it recovers a painted bend and declines on a straight one.
    println!();
    let side = fixtures::DISTORTION_SIDE;
    for painted in [0.050f32, -0.050] {
        let plate = fixtures::grid_plate_at(painted, side);
        let chains = lens::track_edges(&plate, side, side);
        match lens::estimate_k1(&chains, 1.0) {
            Some(found) if found.signum() == painted.signum() && found.abs() <= painted.abs() => {
                println!(
                    "estimator: painted {painted:+.3}, recovered {found:+.4} \
                     ({:.0} % out, never over-corrected)",
                    (found - painted).abs() / painted.abs() * 100.0
                );
            }
            Some(found) => {
                eprintln!("estimator: painted {painted:+.3}, recovered {found:+.4} - wrong");
                failures += 1;
            }
            None => {
                eprintln!("estimator: painted {painted:+.3}, declined");
                failures += 1;
            }
        }
    }
    {
        let plate = fixtures::grid_plate_at(0.0, side);
        let chains = lens::track_edges(&plate, side, side);
        if lens::estimate_k1(&chains, 1.0).is_none() {
            println!("  a straight grid is never corrected");
        } else {
            eprintln!("  a straight grid was corrected");
            failures += 1;
        }
    }

    // 7. The whole synthetic wedding, planned and stored.
    println!();
    let Some(rules) = rules else {
        eprintln!("\nphase 23: FAILED ({failures} problems)");
        return ExitCode::FAILURE;
    };
    let planner = Planner::new(profiles, rules);
    let cases = fixtures::wedding();
    let mut kept = 0usize;
    let mut cut = 0usize;
    for case in &cases {
        let plan = planner.plan(&case.input);
        if let Err(err) = guard::check_plan(&plan) {
            eprintln!("  {}: [{}] {}", case.name, err.code, err.detail);
            failures += 1;
            continue;
        }
        if plan.kept_original_framing() {
            kept += 1;
        }
        if case.must_keep_framing && !plan.kept_original_framing() {
            eprintln!(
                "  {}: the framing was changed and must not have been",
                case.name
            );
            failures += 1;
        }
        if let Some(expert) = case.expert_rotate_deg {
            let delta = (plan.rotate_deg - expert).abs();
            if delta > 0.3 {
                eprintln!(
                    "  {}: levelled {:.2}, expert {expert:.2}",
                    case.name, plan.rotate_deg
                );
                failures += 1;
            }
        }
        // The hard gate: no crop anywhere cuts a face or a primary pair of hands.
        for variant in &plan.crops {
            for region in &case.input.regions {
                if region.is_enforced() && !region.is_inside(variant.rect, 0.0) {
                    eprintln!(
                        "  {}: the {} crop cut a {}",
                        case.name, variant.purpose, region.kind
                    );
                    cut += 1;
                    failures += 1;
                }
            }
        }
    }
    println!(
        "wedding: {} of {} frames delivered as shot ({:.0} %)",
        kept,
        cases.len(),
        kept as f32 / cases.len() as f32 * 100.0
    );
    if kept * 10 >= cases.len() * 7 {
        println!("  at or above the seventy-per-cent restraint target");
    } else {
        eprintln!("  below the seventy-per-cent restraint target");
        failures += 1;
    }
    if cut == 0 {
        println!("  zero crops cut a face or a primary pair of hands");
    }

    // 8. The store: a round trip, the override protection and the version drift.
    println!();
    let project = ProjectId::new();
    if let Err(err) = seed_project(&catalog, &project, &cases) {
        eprintln!("seed: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    let service = match Geometry::shipped(Arc::clone(&catalog), Arc::clone(&clock)) {
        Ok(service) => service,
        Err(err) => {
            eprintln!("service: [{}] {}", err.code, err.detail);
            eprintln!("\nphase 23: FAILED ({} problems)", failures + 1);
            return ExitCode::FAILURE;
        }
    };
    let store = GeometryStore::new(Arc::clone(&catalog), Arc::clone(&clock));
    let mut stored = 0usize;
    for case in &cases {
        let plan = planner.plan(&case.input);
        let rules_row = planner.rules().for_scene(case.input.scene).1;
        if store.put(&plan, rules_row, case.input.aspect).is_ok() {
            stored += 1;
        }
    }
    println!("store: {stored} plans written");

    // A round trip has to reproduce the plan the planner made.
    if let Some(case) = cases.first() {
        let expected = planner.plan(&case.input);
        match service.of_image(case.input.image_id) {
            Ok(Some(read)) => {
                if read.crops.len() == expected.crops.len()
                    && read.primary_crop == expected.primary_crop
                    && (read.rotate_deg - expected.rotate_deg).abs() < 1e-4
                {
                    println!("  a plan round-trips through SQLite");
                } else {
                    eprintln!("  a plan did not round-trip");
                    failures += 1;
                }
                match read.crops.first() {
                    Some(first) if first.purpose == CropPurpose::Original => {
                        println!("  the frame as shot is still the first crop after a round trip");
                    }
                    _ => {
                        eprintln!("  the frame as shot was lost in the round trip");
                        failures += 1;
                    }
                }
            }
            Ok(None) => {
                eprintln!("  the plan was not stored");
                failures += 1;
            }
            Err(err) => {
                eprintln!("  read: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // The override, and what a re-plan does to it.
    if let Some(case) = cases.first() {
        let image = case.input.image_id;
        let chosen = CropRect {
            x: 0.10,
            y: 0.08,
            w: 0.78,
            h: 0.80,
        };
        match service.set_framing(GeometryOverride {
            image_id: image,
            rect: chosen,
            rotate_deg: 1.2,
            aspect: aura_core::contract::geometry::Aspect::Original,
        }) {
            Ok(()) => println!("  a framing override is recorded"),
            Err(err) => {
                eprintln!("  override: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
        // Re-plan the same frame. The override must survive.
        let replanned = planner.plan(&case.input);
        drop(store.put(&replanned, true, case.input.aspect));
        match service.of_image(image) {
            Ok(Some(after)) if after.user_edited => {
                println!("  a re-plan does not overwrite it (checked inside the statement)");
                let primary = after.primary();
                if (primary.rect.x - chosen.x).abs() < 1e-3 {
                    println!(
                        "  and the rectangle the photographer chose is still the one delivered"
                    );
                } else {
                    eprintln!("  the photographer's rectangle was replaced");
                    failures += 1;
                }
            }
            Ok(Some(_)) => {
                eprintln!("  a re-plan cleared the override");
                failures += 1;
            }
            Ok(None) | Err(_) => {
                eprintln!("  the plan disappeared");
                failures += 1;
            }
        }
        // Reverting is an override, not a delete.
        match service.set_framing(GeometryOverride::revert(image)) {
            Ok(()) => match service.of_image(image) {
                Ok(Some(after)) if after.user_edited && after.primary().is_full_frame() => {
                    println!("  reverting restores the frame exactly and is itself an override");
                }
                _ => {
                    eprintln!("  reverting did not restore the frame");
                    failures += 1;
                }
            },
            Err(err) => {
                eprintln!("  revert: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // A degenerate override is refused.
    if let Some(case) = cases.first() {
        let bad = GeometryOverride {
            image_id: case.input.image_id,
            rect: CropRect {
                x: 0.5,
                y: 0.5,
                w: 0.0,
                h: 0.2,
            },
            rotate_deg: 0.0,
            aspect: aura_core::contract::geometry::Aspect::Original,
        };
        match service.set_framing(bad) {
            Err(err) if err.code.0 == "AURA-ML-5091" => {
                println!("  a degenerate rectangle is refused (AURA-ML-5091)");
            }
            _ => {
                eprintln!("  a degenerate rectangle was accepted");
                failures += 1;
            }
        }
    }

    // Version drift is reported rather than happening silently.
    match store.check_versions(&project, (99, 99, 99)) {
        Err(err) if err.code.0 == "AURA-ML-5090" => {
            println!("  a version boundary is reported (AURA-ML-5090)");
        }
        _ => {
            eprintln!("  a version boundary was crossed silently");
            failures += 1;
        }
    }

    // 9. The outline, which is what a photographer actually reads.
    println!();
    match service.outline(project) {
        Ok(outline) => {
            println!(
                "outline: {} of {} planned, {:.0} % delivered as shot, {:.0} % lens-profiled",
                outline.planned,
                outline.photos,
                outline.kept_original * 100.0,
                outline.profile_covered * 100.0
            );
            println!(
                "  refusals: {} face, {} hands, {} resolution, {} content",
                outline.refused_histogram[0],
                outline.refused_histogram[1],
                outline.refused_histogram[2],
                outline.refused_histogram[3]
            );
            if !outline.missing_profiles.is_empty() {
                println!("  no profile for: {}", outline.missing_profiles.join(", "));
            }
            println!(
                "  versions: profile {} analysis {} rules {}",
                outline.profile_ver, outline.analysis_ver, outline.rules_ver
            );
            if ANALYSIS_VER != outline.analysis_ver {
                eprintln!("  the outline's analysis version disagrees with the planner's");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("outline: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 10. Determinism.
    println!();
    let once: Vec<_> = cases.iter().map(|case| planner.plan(&case.input)).collect();
    let twice: Vec<_> = cases.iter().map(|case| planner.plan(&case.input)).collect();
    if once == twice {
        println!("determinism: two runs of the planner agree exactly");
    } else {
        eprintln!("determinism: two runs of the planner disagree");
        failures += 1;
    }

    println!();
    println!("{}", caveats());

    if failures == 0 {
        println!("phase 23: OK");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase 23: FAILED ({failures} problems)");
        ExitCode::FAILURE
    }
}

/// What this gate does not prove. Printed on every run, as phases 09 to 19's gates do.
fn caveats() -> &'static str {
    "PHASE-23 gate passed against SYNTHETIC frames.\n\
     \n\
     C1: there are no wedding photographs and no expert crop labels in this repository.\n\
     Every number above measures a geometry that was chosen, painted into the pixels and read\n\
     back through the real pipeline. It proves the estimator, the tracker, the caps, the safety\n\
     filter, the search and the store. It is NOT evidence that a photographer would agree with\n\
     a crop, and section 10.1's QAIQ audit of 300 auto-crops has not happened.\n\
     \n\
     C2: every lens profile in assets/lens_profiles/ is FABRICATED. No lens was measured. The\n\
     coefficients have the right sign and order of magnitude and are not measurements.\n\
     \n\
     C3: there is no pose estimate in this build, so hands_checked is zero on every photograph\n\
     in the product. The zero-face-cut gate above is a claim; the same gate for hands is\n\
     currently a claim about an empty set."
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

/// Any column in migration 20's tables whose name suggests a pixel, a path or a fill.
///
/// Three boundaries, checked rather than remembered. The list is deliberately broad: this is
/// looking for the *shape* of a mistake somebody would make in good faith while adding a
/// feature two phases from now - a rendered thumbnail cached "just for the panel", a fill
/// radius added "since we are already opening the corners".
fn forbidden_columns(catalog: &Catalog) -> AuraResult<Vec<String>> {
    catalog.read(move |conn| {
        let mut found = Vec::new();
        for table in ["geometry_plan", "geometry_crop"] {
            let mut statement = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .map_err(|e| aura_core::errors::db::statement_failed("table_info", &e))?;
            let mut cursor = statement
                .query([])
                .map_err(|e| aura_core::errors::db::statement_failed("table_info", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| aura_core::errors::db::statement_failed("table_info", &e))?
            {
                let name: String = row.get(1).unwrap_or_default();
                // Matched on whole underscore-separated tokens rather than as substrings.
                // `lens_profile` and `profile_ver` both contain "file", and a scan that reads
                // them as a stored path is a scan that cries wolf on the two columns this
                // phase most needs to keep.
                let tokens: Vec<String> = name
                    .to_ascii_lowercase()
                    .split('_')
                    .map(str::to_string)
                    .collect();
                for needle in [
                    "pixel",
                    "pixels",
                    "blob",
                    "thumb",
                    "thumbnail",
                    "render",
                    "path",
                    "file",
                    "fill",
                    "inpaint",
                    "applied",
                    "deleted",
                    "matte",
                    "alpha",
                ] {
                    if tokens.iter().any(|token| token == needle) {
                        found.push(format!("{table}.{name}"));
                    }
                }
            }
        }
        Ok(found)
    })
}

/// Put the fixture photographs in the catalog so the store has something to join against.
fn seed_project(
    catalog: &Arc<Catalog>,
    project: &ProjectId,
    cases: &[fixtures::Case],
) -> AuraResult<()> {
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "phase 23".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-08-26T00:00:00Z".to_string(),
        updated_at: "2026-08-26T00:00:00Z".to_string(),
    };
    catalog
        .writer()
        .transact(move |conn| repo::create_project(conn, &row))?;
    let project_key = project.to_db();
    let ids: Vec<String> = cases
        .iter()
        .map(|case| case.input.image_id.to_db())
        .collect();
    catalog.writer().transact(move |tx| {
        for (index, photo) in ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                    camera_make, camera_model, iso, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 1600,
                         '2026-08-26T00:00:00Z', '2026-08-26T00:00:00Z')",
                params![
                    photo,
                    project_key,
                    format!("2026-08-26T{:02}:{:02}:00Z", index / 60 % 24, index % 60),
                ],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
        }
        Ok(())
    })
}
