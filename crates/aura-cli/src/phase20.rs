//! The phase 20 mechanical gate.
//!
//! This is the assembly proof for portrait retouch: migration 20 and its objects, the preset
//! table, the detector, the protect veto, the cross-frame permanence rule, the two operators,
//! the texture guard and its withdrawal, the per-identity strength, the store and its two
//! protections - a photographer preset and a tattoo that cannot be cleared.
//!
//! **Nothing here proves a retouch is invisible.** Section 10.1's last gate is a blind study
//! against Retouch4me, Evoto and Aperty judged by retouchers, and there is no such study in this
//! repository - condition C4 of the exit report. Every number below is measured against
//! synthetic faces whose marks were painted in, through masks this phase does not own. The
//! distinction is printed at the end of every run rather than hidden in a test helper.
//!
//! The tests prove the pieces; this proves the assembly. `tests/eval/retouch_eval.rs` is the
//! other half and runs under `cargo test`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::composition::Box2;
use aura_core::contract::retouch::{
    InpaintMethod, ProtectedFeature, ProtectedKind, ProtectedSource, RetouchCode, RetouchOp,
    RetouchOverride, RetouchPreset, RetouchService, POLISHED_FLOOR, TEXTURE_FLOOR,
};
use aura_core::{AuraResult, IdentityId, PhotoId, ProjectId};
use aura_retouch::ops::{Analyser, BLEMISH_HEAD_TRAINED, PERMANENT_HEAD_TRAINED};
use aura_retouch::presets::PresetTable;
use aura_retouch::store::{RetouchStore, BYTES_PER_IMAGE};
use aura_retouch::{blemish, fixtures, texture_guard, Retouch};
use rusqlite::params;

/// Run the phase 20 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase20-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. Migration 20 and every object it owns.
    let catalog_path = work.join("phase20.sqlite");
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
        ("table", "retouch_plan"),
        ("table", "retouch_identity"),
        ("table", "retouch_protected"),
        ("table", "retouch_op"),
        ("view", "v_retouch_coverage"),
        ("index", "idx_retouch_review"),
        ("index", "idx_retouch_versions"),
        ("index", "idx_retouch_texture"),
        ("index", "idx_retouch_protected_identity"),
        ("trigger", "retouch_protected_absolute"),
        ("trigger", "retouch_protected_absolute_update"),
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

    // 2. There is nowhere in this schema to reshape anybody.
    //
    // Section 11 of `docs/plan/CLAUDE.md` forbids body reshaping, skin lightening and face
    // swapping permanently, and the way a phase quietly acquires one is by growing a column for
    // it. This is the scan that would catch it.
    match forbidden_columns(&catalog) {
        Ok(found) if found.is_empty() => {
            println!(
                "  no reshaping, slimming or skin-tone-target column anywhere in migration 20"
            );
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

    // 3. The preset table, and the bound the code owns rather than the file.
    println!();
    let presets = match PresetTable::embedded() {
        Ok(table) => {
            println!("presets: version {} loaded", table.version());
            let mut ok = true;
            for preset in RetouchPreset::ALL {
                let row = table.preset(preset);
                if row.texture_floor + 1e-6 < preset.floor() {
                    eprintln!(
                        "  {} sets a floor of {:.3}, below its bound of {:.2}",
                        preset.as_str(),
                        row.texture_floor,
                        preset.floor()
                    );
                    failures += 1;
                    ok = false;
                }
                if row.reason.trim().is_empty() {
                    eprintln!("  {} has no written reason", preset.as_str());
                    failures += 1;
                    ok = false;
                }
            }
            if ok {
                println!(
                    "  every preset keeps its own texture floor, and none may go below {POLISHED_FLOOR:.2}"
                );
            }
            let unpreset = table.unpreset();
            if unpreset.is_empty() {
                println!("  every scene in the taxonomy has a row");
            } else {
                eprintln!("  scenes with no row: {}", unpreset.join(", "));
                failures += 1;
            }
            Some(table)
        }
        Err(err) => {
            eprintln!("presets: [{}] {}", err.code, err.detail);
            failures += 1;
            None
        }
    };
    let Some(presets) = presets else {
        return ExitCode::FAILURE;
    };

    // A text file must not be able to retract the phase's headline guarantee.
    let lowered = include_str!("../../aura-retouch/config/retouch_presets.toml")
        .replace("texture_floor = 0.84", "texture_floor = 0.55");
    match PresetTable::parse(&lowered, "gate") {
        Err(err) if err.code.0 == "AURA-ML-5093" => {
            println!("  a preset file that lowered the floor to 0.55 was refused");
        }
        Err(err) => {
            eprintln!(
                "  a lowered floor was refused with the wrong code: {}",
                err.code
            );
            failures += 1;
        }
        Ok(_) => {
            eprintln!("  a preset file lowered the texture floor to 0.55 and was accepted");
            failures += 1;
        }
    }

    // 4. The detector: a spot is found, a mole is not removable, even skin produces nothing.
    println!();
    let spot = blemish::detect(&fixtures::face_with_blemish());
    let mole = blemish::detect(&fixtures::face_with_mole());
    let even = blemish::detect(&fixtures::even_face());
    let freckles = blemish::detect(&fixtures::face_with_freckles());

    if spot.iter().any(blemish::Candidate::is_removable) {
        println!("detector: an inflamed spot is found and is removable");
    } else {
        eprintln!("detector: the inflamed spot was not removable");
        failures += 1;
    }
    if mole.iter().all(|c| !c.is_removable()) && freckles.iter().all(|c| !c.is_removable()) {
        println!("  no mole and no freckle is removable");
    } else {
        eprintln!("  a permanent feature was removable");
        failures += 1;
    }
    if even.is_empty() {
        println!("  even skin with pores produces no candidates at all");
    } else {
        eprintln!("  even skin produced {} candidates", even.len());
        failures += 1;
    }

    // 5. The texture guard: it passes an ordinary heal, and it withdraws rather than shipping.
    println!();
    let (frame, context, area) = fixtures::frame_with_blemish();
    let ops = vec![RetouchOp::Blemish {
        area,
        method: InpaintMethod::Patch,
        strength: 1.0,
    }];
    let guarded = texture_guard::enforce(&frame, &ops, &context, TEXTURE_FLOOR);
    if guarded.report.passed && !guarded.report.withdrawn {
        println!(
            "texture: an ordinary heal kept {:.3} of the skin texture, against a floor of {TEXTURE_FLOOR:.2}",
            guarded.report.band_ratio
        );
    } else {
        eprintln!(
            "texture: an ordinary heal failed its floor at {:.3}",
            guarded.report.band_ratio
        );
        failures += 1;
    }
    let impossible = texture_guard::enforce(&frame, &ops, &context, 1.5);
    if impossible.report.withdrawn && impossible.ops.is_empty() {
        println!("  a floor no retouch could meet withdrew the whole plan rather than shipping it");
    } else {
        eprintln!("  an unmeetable floor did not withdraw the retouch");
        failures += 1;
    }

    // 6. A whole frame, planned, with and without a protected mole over the mark.
    println!();
    let analyser = Analyser::with_presets(presets.clone());
    let (image, pixels, frame_context) = fixtures::planned_frame();
    let outcome = match analyser.analyse(image, &pixels, &frame_context) {
        Ok(outcome) => outcome,
        Err(err) => {
            eprintln!("plan: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    if let Some(problem) = outcome.plan.broken_guarantee() {
        eprintln!("plan: {problem}");
        failures += 1;
    }
    if outcome.plan.count_of("blemish") > 0 {
        println!("plan: the mark on the fixture face was removed");
    } else {
        eprintln!("plan: nothing was removed from the fixture face");
        failures += 1;
    }
    if outcome
        .plan
        .reasons
        .iter()
        .any(|reason| reason.code == RetouchCode::HeadUntrained)
    {
        println!("  every plan says the heads are untrained");
    } else {
        eprintln!("  a plan did not say the heads are untrained");
        failures += 1;
    }

    let (_, _, protected_context) = fixtures::planned_frame_with_protected_mole();
    match analyser.analyse(image, &pixels, &protected_context) {
        Ok(vetoed) => {
            let named = vetoed
                .plan
                .reasons
                .iter()
                .any(|reason| reason.code == RetouchCode::VetoedByProtection);
            let touched = vetoed
                .plan
                .ops
                .iter()
                .filter_map(RetouchOp::area)
                .any(|area| vetoed.plan.is_protected(area));
            if named && !touched {
                println!("  a protected mole vetoed the operation over it, and said so");
            } else {
                eprintln!("  the protect veto did not fire, or fired and edited anyway");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("  protected plan: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 7. The store: the plan round-trips, an override survives a re-analysis, and a tattoo
    //    cannot be deleted.
    println!();
    let project = ProjectId::new();
    let identity = fixtures::identity();
    if let Err(err) = seed(&catalog, &project, &[image], &[identity]) {
        eprintln!("seed: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }
    let store = Arc::new(RetouchStore::new(Arc::clone(&catalog), Arc::clone(&clock)));
    if let Err(err) = store.put(&project, &outcome.plan) {
        eprintln!("store: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    match store.get(image) {
        Ok(Some(read)) => {
            if read.ops.len() == outcome.plan.ops.len()
                && (read.texture_report.band_ratio - outcome.plan.texture_report.band_ratio).abs()
                    < 1e-4
            {
                println!("store: a plan round-trips with its operations and its measurement");
            } else {
                eprintln!("store: the plan came back different");
                failures += 1;
            }
        }
        Ok(None) => {
            eprintln!("store: the plan did not come back");
            failures += 1;
        }
        Err(err) => {
            eprintln!("store: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    let service = Retouch::new(Arc::clone(&store), project);
    if let Err(err) = service.set_override(image, RetouchOverride::preset(RetouchPreset::Light)) {
        eprintln!("  override: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    if let Err(err) = store.put(&project, &outcome.plan) {
        eprintln!("  re-analysis: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    match store.get(image) {
        Ok(Some(read)) if read.preset == RetouchPreset::Light && read.user_edited => {
            println!("  a photographer preset survived a re-analysis");
        }
        Ok(_) => {
            eprintln!("  a re-analysis overwrote a photographer preset");
            failures += 1;
        }
        Err(err) => {
            eprintln!("  re-read: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    let tattoo = ProtectedFeature {
        identity,
        kind: ProtectedKind::Tattoo,
        area: Box2 {
            x: -0.30,
            y: 0.50,
            w: 0.20,
            h: 0.20,
        },
        confidence: 1.0,
        source: ProtectedSource::User,
        frames: 12,
        span_minutes: 300.0,
        first_seen: image,
    };
    if let Err(err) = service.set_protection(tattoo, true) {
        eprintln!("  protecting a tattoo: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    match service.set_protection(tattoo, false) {
        Err(err) if err.code.0 == "AURA-ML-5091" => {
            println!("  a tattoo cannot be unprotected, and the refusal is AURA-ML-5091");
        }
        Err(err) => {
            eprintln!(
                "  clearing a tattoo was refused with the wrong code: {}",
                err.code
            );
            failures += 1;
        }
        Ok(()) => {
            eprintln!("  a tattoo protection was cleared, which this product does not permit");
            failures += 1;
        }
    }
    // And the database refuses it too, from a caller that never asked the service.
    match delete_tattoo_directly(&catalog) {
        Ok(false) => println!("  the database aborts a direct DELETE of a protected tattoo"),
        Ok(true) => {
            eprintln!("  a direct DELETE removed a protected tattoo");
            failures += 1;
        }
        Err(err) => {
            eprintln!("  direct delete: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    match service.outline(project) {
        Ok(outline) => {
            println!(
                "  outline: {} planned of {} photographs, {} protected features",
                outline.planned,
                outline.photos,
                outline.protected_histogram.iter().sum::<u32>()
            );
        }
        Err(err) => {
            eprintln!("  outline: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 8. What this build's numbers are and are not.
    println!();
    println!("what this run did and did not prove:");
    println!(
        "  the heads are untrained (blemish {BLEMISH_HEAD_TRAINED}, permanent \
         {PERMANENT_HEAD_TRAINED}); what ran is the measured detector"
    );
    println!("  every face above was painted by the fixture generator, not photographed");
    println!("  no skin mask reached the pass from phase 18, so a real frame is not retouched");
    println!("  no blind study against Retouch4me, Evoto or Aperty has been run");
    println!("  storage budget: {BYTES_PER_IMAGE} B per image, asserted by aura-perf");

    if failures == 0 {
        println!();
        println!("phase 20: all checks passed");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("phase 20: {failures} check(s) failed");
        ExitCode::FAILURE
    }
}

/// Whether a named schema object exists.
fn schema_object(catalog: &Catalog, kind: &str, name: &str) -> AuraResult<bool> {
    let kind = kind.to_string();
    let name = name.to_string();
    catalog.read(move |conn| {
        let found: Result<i64, rusqlite::Error> = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
            params![kind, name],
            |row| row.get(0),
        );
        Ok(found.unwrap_or(0) > 0)
    })
}

/// Any column in migration 20 whose name would be a feature this product does not build.
fn forbidden_columns(catalog: &Catalog) -> AuraResult<Vec<String>> {
    catalog.read(|conn| {
        let mut found = Vec::new();
        for table in [
            "retouch_plan",
            "retouch_identity",
            "retouch_protected",
            "retouch_op",
        ] {
            let mut statement = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .map_err(|e| aura_core::errors::db::statement_failed("table info", &e))?;
            let mut cursor = statement
                .query([])
                .map_err(|e| aura_core::errors::db::statement_failed("table info", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| aura_core::errors::db::statement_failed("table info", &e))?
            {
                let column: String = row.get(1).unwrap_or_default();
                let lower = column.to_lowercase();
                for banned in ["reshape", "slim", "waist", "skin_tone", "lighten", "swap"] {
                    if lower.contains(banned) {
                        found.push(format!("{table}.{column}"));
                    }
                }
            }
        }
        Ok(found)
    })
}

/// Try to delete a protected tattoo without going through the service.
///
/// Returns whether the row went. The trigger in migration 20 is what has to stop it: a promise
/// enforced in one layer is a promise until somebody writes a second caller.
fn delete_tattoo_directly(catalog: &Catalog) -> AuraResult<bool> {
    let removed = catalog.writer().transact(|conn| {
        let result = conn.execute("DELETE FROM retouch_protected WHERE kind = 'tattoo'", []);
        Ok(result.unwrap_or(0))
    })?;
    Ok(removed > 0)
}

/// A project, its photographs and its people.
fn seed(
    catalog: &Catalog,
    project: &ProjectId,
    photos: &[PhotoId],
    people: &[IdentityId],
) -> AuraResult<()> {
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "phase 20".to_string(),
        couple_label: Some("A and B".to_string()),
        event_date: Some("2026-08-20".to_string()),
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-08-20T00:00:00Z".to_string(),
        updated_at: "2026-08-20T00:00:00Z".to_string(),
    };
    catalog
        .writer()
        .transact(move |conn| repo::create_project(conn, &row))?;

    let project_key = project.to_db();
    let ids: Vec<String> = photos.iter().map(PhotoId::to_db).collect();
    let identities: Vec<String> = people.iter().map(IdentityId::to_db).collect();
    let identity_project = project.to_db();
    catalog.writer().transact(move |tx| {
        for identity in &identities {
            tx.execute(
                "INSERT INTO identities (id, project_id, created_at, updated_at)
                 VALUES (?1, ?2, '2026-08-20T00:00:00Z', '2026-08-20T00:00:00Z')",
                params![identity, identity_project],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("identity", &e))?;
        }
        for (index, photo) in ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                    camera_make, camera_model, iso, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 800,
                         '2026-08-20T00:00:00Z', '2026-08-20T00:00:00Z')",
                params![
                    photo,
                    project_key,
                    format!("2026-08-20T{:02}:{:02}:00Z", index / 60 % 24, index % 60),
                ],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
        }
        Ok(())
    })
}
