//! The phase 19 mechanical gate.
//!
//! This is the assembly proof for local light sculpting: migration 19 and its objects, the
//! policy table, the measurement pass, the luminosity split, the joint face solve, the paired
//! subject/background move, the frequency separation, the shaping zones, the shine detector,
//! the governor, the store and its override protection, and what happens to a whole synthetic
//! wedding when phase 18 is not installed.
//!
//! **Nothing here proves an edit is invisible.** Section 10.1's seventh gate is an expert
//! subtlety study over four hundred frames and there is no such study in this repository - it
//! is condition C3 of the exit report. Every number below is measured against synthetic frames
//! whose faces, backgrounds and hot spots were chosen and then painted in, through masks this
//! phase does not own and cannot make. The distinction is printed at the end of every run
//! rather than hidden in a test helper.
//!
//! The tests prove the pieces; this proves the assembly. `tests/eval/local_eval.rs` is the
//! other half and runs under `cargo test`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_brain_photo::local::fixtures;
use aura_brain_photo::local::plan::{Analyser, ANALYSIS_VER, MODEL_VER, TARGET_HEAD_TRAINED};
use aura_brain_photo::local::policy::PolicyTable;
use aura_brain_photo::local::store::{LocalStore, BYTES_PER_IMAGE};
use aura_brain_photo::local::{governor, SHAPING_VER};
use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::local::{
    LocalCode, LocalLightPlan, LocalOp, LocalOverride, LocalService, MAX_FACE_LIFT_EV,
    MAX_INTER_FACE_SPREAD, MAX_MEAN_LUMA_DRIFT, PERCEPTUAL_BUDGET,
};
use aura_core::{AuraResult, PhotoId, ProjectId, SceneId};
use rusqlite::params;

/// Run the phase 19 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase19-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. Migration 19 and every object it owns.
    let catalog_path = work.join("phase19.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 19 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 19, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "local_light_plan"),
        ("table", "local_light_face"),
        ("table", "local_light_gate"),
        ("view", "v_local_coverage"),
        ("index", "idx_local_project"),
        ("index", "idx_local_review"),
        ("index", "idx_local_evened"),
        ("index", "idx_local_face_identity"),
        ("index", "idx_local_gate_kind"),
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

    // 2. There is nowhere in this schema to put a mask, and nowhere to put a blur.
    //
    // Two boundaries, both structural. Phase 18 owns masks and phase 20 owns texture, and the
    // way a phase quietly acquires somebody else's job is by growing a column for it.
    match forbidden_columns(&catalog) {
        Ok(found) if found.is_empty() => {
            println!("  no mask, matte or smoothing column anywhere in migration 19");
        }
        Ok(found) => {
            eprintln!(
                "  migration 19 grew a forbidden column: {}",
                found.join(", ")
            );
            failures += 1;
        }
        Err(err) => {
            eprintln!("  column scan: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 3. The policy table, and the rows a product manager has to be able to argue with.
    println!();
    let policy = match PolicyTable::embedded() {
        Ok(table) => {
            println!(
                "policy: version {} loaded, {} scenes",
                table.version(),
                table.rows()
            );
            let dance = table.get(SceneId::DanceFloor);
            let family = table.get(SceneId::FamilyPortrait);
            if dance.declines(LocalOp::DodgeBurnLow) {
                println!("  the dance floor is not form-shaped (section 6.4)");
            } else {
                eprintln!("  the dance floor is form-shaped, and section 6.4 says it must not be");
                failures += 1;
            }
            if family.budget > dance.budget {
                println!("  a family portrait gets more of the allowance than a dance floor");
            } else {
                eprintln!("  a dance floor gets as much of the allowance as a family portrait");
                failures += 1;
            }
            let unpolicied = table.unpolicied();
            if unpolicied.is_empty() {
                println!("  every scene in the taxonomy has a row");
            } else {
                eprintln!("  scenes with no row: {}", unpolicied.join(", "));
                failures += 1;
            }
            Some(table)
        }
        Err(err) => {
            eprintln!("policy: [{}] {}", err.code, err.detail);
            failures += 1;
            None
        }
    };
    let Some(policy) = policy else {
        return ExitCode::FAILURE;
    };

    // 4. The seven fixtures, planned.
    println!();
    let analyser = Analyser::new(policy.clone());
    let frames = fixtures::all();
    let mut plans: Vec<(&'static str, LocalLightPlan)> = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let id = fixture_photo(index);
        let outcome = analyser.analyse(&frame.buffer, id, &frame.context);
        if let Some(problem) = outcome.plan.broken_guarantee() {
            eprintln!("  {}: {problem}", frame.name);
            failures += 1;
        }
        plans.push((frame.name, outcome.plan));
    }
    println!("fixtures: {} planned", plans.len());

    // Face lighting reaches the band on a dark face and leaves a correct one alone.
    match (
        find(&plans, "face_in_shadow"),
        find(&plans, "already_right"),
    ) {
        (Some(dark), Some(right)) => {
            let lifted = dark
                .face_light
                .first()
                .is_some_and(|(_, d)| d.luma_after > d.luma_before + 0.02);
            let left_alone = right
                .face_light
                .first()
                .is_some_and(|(_, d)| d.exposure_ev.abs() < 0.05);
            if lifted {
                println!("  a face under a mandap was lifted toward the band");
            } else {
                eprintln!("  a face two stops down was left where it was");
                failures += 1;
            }
            if left_alone {
                println!("  a correctly lit face was left alone");
            } else {
                eprintln!("  a correctly lit face was moved anyway");
                failures += 1;
            }
        }
        _ => {
            eprintln!("  the face fixtures did not plan");
            failures += 1;
        }
    }

    // The paired operation holds the frame's mean luminance.
    let mut worst_drift = 0.0f32;
    for (name, plan) in &plans {
        if plan.background.is_noop() {
            continue;
        }
        let drift = plan.background.luma_drift();
        worst_drift = worst_drift.max(drift);
        if drift > MAX_MEAN_LUMA_DRIFT + 1e-4 {
            eprintln!("  {name} moved the frame's mean luminance by {drift:.4}");
            failures += 1;
        }
    }
    println!(
        "  the worst mean-luminance drift is {worst_drift:.4} against {MAX_MEAN_LUMA_DRIFT:.3}"
    );

    // Group fairness.
    if let Some(group) = find(&plans, "uneven_group") {
        let before = group.inter_face_spread_before();
        let after = group.inter_face_spread();
        if after < before && group.group_is_fair() {
            println!("  a group {before:.3} apart ended {after:.3} apart, against a threshold of {MAX_INTER_FACE_SPREAD:.3}");
        } else {
            eprintln!(
                "  the group solve did not make the group more even: {before:.3} then {after:.3}"
            );
            failures += 1;
        }
        if group
            .face_light
            .iter()
            .all(|(_, d)| d.luma_after >= d.luma_before.min(0.50) - 1e-3)
        {
            println!("  nobody was darkened below the band to close the gap");
        } else {
            eprintln!("  somebody was darkened for the group's sake");
            failures += 1;
        }
    }

    // Shaping, and its texture guarantee.
    if let Some(modelled) = find(&plans, "modelled_face") {
        match &modelled.dodge_burn {
            Some(maps) if maps.texture_preserved() => {
                let zones: usize = maps.faces.iter().map(|f| f.zones.len()).sum();
                println!("  a large face was shaped with {zones} zones and kept its texture");
            }
            Some(_) => {
                eprintln!("  the shaping moved the mid-frequency band past its tolerance");
                failures += 1;
            }
            None => {
                eprintln!("  a large modelled face was not shaped at all");
                failures += 1;
            }
        }
    }
    if let Some(dance) = find(&plans, "dance_floor") {
        if dance.strength(LocalOp::DodgeBurnLow) <= 0.0 {
            println!("  a dance-floor face was not form-shaped");
        } else {
            eprintln!("  a dance-floor face was form-shaped");
            failures += 1;
        }
    }

    // Shine.
    if let Some(shiny) = find(&plans, "shiny_forehead") {
        match &shiny.shine {
            Some(shine) if shine.reduction_ev < 0.0 => {
                println!(
                    "  {} specular region(s) reduced by {:.2} EV, luminance only",
                    shine.regions.len(),
                    shine.reduction_ev
                );
            }
            _ => {
                eprintln!("  a bright desaturated forehead patch was not reduced");
                failures += 1;
            }
        }
    }

    // 5. What happens when phase 18 is not installed. **The state of this build.**
    println!();
    let bare = fixtures::bright_window().without_masks();
    let bare_plan = analyser
        .analyse(&bare.buffer, fixture_photo(90), &bare.context)
        .plan;
    if bare_plan.is_noop() && !bare_plan.gated_by_mask_quality.is_empty() {
        println!(
            "no masks: every operation gated ({} of them), nothing edited, plan says {}",
            bare_plan.gated_by_mask_quality.len(),
            bare_plan
                .reasons
                .iter()
                .find(|r| r.code == LocalCode::MaskUnavailable)
                .map_or("nothing", |r| r.code.as_str())
        );
    } else {
        eprintln!("no masks: the pass edited a frame it could not see the subject of");
        failures += 1;
    }
    for (op, kind) in &bare_plan.gated_by_mask_quality {
        if *kind != op.requires() {
            eprintln!("  {op} was gated on {kind}, which is not what it needs");
            failures += 1;
        }
    }

    // 6. The governor, on the frame that would spend everything.
    println!();
    let ledger = governor::allocate([PERCEPTUAL_BUDGET; LocalOp::COUNT], 1.0);
    if ledger.allowed(LocalOp::FaceLight) >= 1.0 && ledger.allowed(LocalOp::DodgeBurnMid) <= 0.0 {
        println!("governor: face lighting keeps its claim and dodge and burn gives its up");
    } else {
        eprintln!("governor: the priority order is not the one section 6.4 names");
        failures += 1;
    }

    // 7. The store, the override protection and the size budget.
    println!();
    let project = ProjectId::new();
    let photos: Vec<PhotoId> = (0..plans.len()).map(fixture_photo).collect();
    // Every identity the fixtures invented, because `local_light_face.identity_id` references
    // `identities` and a lit face belonging to nobody the catalog has heard of is a row with
    // nobody attached to it.
    let people: Vec<aura_core::IdentityId> = plans
        .iter()
        .flat_map(|(_, plan)| plan.face_light.iter().filter_map(|(id, _)| *id))
        .collect();
    if let Err(err) = seed(&catalog, &project, &photos, &people) {
        eprintln!("seed: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }
    let store = Arc::new(LocalStore::new(Arc::clone(&catalog), Arc::clone(&clock)));
    for (_, plan) in &plans {
        if let Err(err) = store.put(&project, plan) {
            eprintln!("store: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    let service = aura_brain_photo::local::Local::new(Arc::clone(&store));
    match service.outline(project) {
        Ok(outline) => {
            println!(
                "store: {} planned of {} photographs, {:.0}% acted on, {:.0}% fully masked",
                outline.planned,
                outline.photos,
                outline.acted_on * 100.0,
                outline.mask_covered * 100.0
            );
            if outline.planned as usize != plans.len() {
                eprintln!("  not every plan was stored");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("store: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // The photographer's own strengths survive a re-analysis. Eighth phase, same rule.
    let owned = photos.first().copied().unwrap_or_else(PhotoId::new);
    if let Err(err) = service.set_override(owned, LocalOverride::one(LocalOp::DodgeBurnLow, 0.0)) {
        eprintln!("override: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    if let Some((_, plan)) = plans.first() {
        if let Err(err) = store.put(&project, plan) {
            eprintln!("re-analysis: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    match store.override_of(owned) {
        Ok(Some(values)) if !values.is_empty() => {
            println!("override: a photographer's strength survived a re-analysis");
        }
        Ok(_) => {
            eprintln!("override: a re-analysis lost the photographer's own strength");
            failures += 1;
        }
        Err(err) => {
            eprintln!("override: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    match service.of_image(owned) {
        Ok(Some(plan)) if plan.user_edited => {
            println!("override: the row is marked as the photographer's");
        }
        Ok(_) => {
            eprintln!("override: the row does not say a person set it");
            failures += 1;
        }
        Err(err) => {
            eprintln!("override: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    match stored_rows(&catalog) {
        Ok(rows) => {
            println!(
                "store: {} plan rows, {} lit faces, {} gates",
                rows.0, rows.1, rows.2
            );
            // The per-image size budget is measured against a real catalog with enough rows
            // to amortise SQLite's page granularity, which seven fixtures cannot do. That
            // lives in `crates/aura-perf/tests/local_budgets.rs`, exactly as phase 15's does,
            // and asserting it here would be a number rather than a measurement.
            println!("  the {BYTES_PER_IMAGE} B per-image budget is asserted in aura-perf");
        }
        Err(err) => {
            eprintln!("store: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 8. Determinism, invariant 4.
    println!();
    let mut deterministic = true;
    for (index, frame) in frames.iter().enumerate() {
        let a = analyser
            .analyse(&frame.buffer, fixture_photo(index), &frame.context)
            .plan;
        let b = analyser
            .analyse(&frame.buffer, fixture_photo(index), &frame.context)
            .plan;
        if a != b {
            eprintln!("  {} is not deterministic", frame.name);
            deterministic = false;
            failures += 1;
        }
    }
    if deterministic {
        println!("determinism: every fixture plans identically twice");
    }

    // 9. What this gate does not prove.
    println!();
    println!(
        "versions: model {MODEL_VER}, analysis {ANALYSIS_VER}, policy {}, shaping {SHAPING_VER}",
        policy.version()
    );
    println!(
        "caps: a face may be lifted at most {MAX_FACE_LIFT_EV:.2} EV before the noise cap, and \
         at most {:.3} of the allowance is spendable",
        PERCEPTUAL_BUDGET
    );
    println!();
    println!("WHAT THIS GATE DOES NOT PROVE");
    println!("  * that any edit is invisible. Section 10.1's expert subtlety study over 400");
    println!("    frames does not exist in this repository (condition C3).");
    println!("  * that the masks are real. Phase 18 ships a MaskService, but nothing wires it");
    println!("    into LocalPass::with_masks yet, so every mask above was built by the");
    println!("    fixtures and is perfect by construction (condition C1).");
    if TARGET_HEAD_TRAINED {
        println!("  * the learned targets are marked trained; re-read condition C2.");
    } else {
        println!("  * that the targets are learned. The head is untrained and is never");
        println!("    consulted; phase 15's own per-scene bands are what ran (condition C2).");
    }

    println!();
    if failures == 0 {
        println!("phase-19 verify: all checks clean");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase-19 verify: {failures} failures");
        ExitCode::FAILURE
    }
}

/// One fixture's plan, by name.
fn find<'a>(plans: &'a [(&'static str, LocalLightPlan)], name: &str) -> Option<&'a LocalLightPlan> {
    plans
        .iter()
        .find(|(fixture, _)| *fixture == name)
        .map(|(_, plan)| plan)
}

/// A stable photo id for one fixture, so a re-run addresses the same frame.
fn fixture_photo(index: usize) -> PhotoId {
    let text = format!("pht_00000000-0000-4000-8000-0000000{:05x}", index + 1);
    PhotoId::from_db(&text).unwrap_or_else(|_| PhotoId::new())
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

/// Any column in migration 19's tables whose name suggests a mask or a blur.
///
/// Two boundaries, checked rather than remembered. The list is deliberately broad: this is
/// looking for the *shape* of a mistake somebody would make in good faith while adding a
/// feature two phases from now - a matte cached "just for speed", a smoothing radius added
/// "since we are already in the skin".
fn forbidden_columns(catalog: &Catalog) -> AuraResult<Vec<String>> {
    const NEEDLES: [&str; 9] = [
        "matte",
        "alpha",
        "mask_data",
        "mask_blob",
        "blur",
        "smooth",
        "radius_px",
        "soften",
        "texture_blur",
    ];
    catalog.read(move |conn| {
        let mut found = Vec::new();
        for table in ["local_light_plan", "local_light_face", "local_light_gate"] {
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
                if NEEDLES.iter().any(|needle| lower.contains(needle)) {
                    found.push(format!("{table}.{column}"));
                }
            }
        }
        Ok(found)
    })
}

/// How many rows each of the three tables holds.
fn stored_rows(catalog: &Catalog) -> AuraResult<(i64, i64, i64)> {
    catalog.read(move |conn| {
        let count = |table: &str| -> i64 {
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap_or(0)
        };
        Ok((
            count("local_light_plan"),
            count("local_light_face"),
            count("local_light_gate"),
        ))
    })
}

/// A project and its photographs, so the foreign keys hold.
fn seed(
    catalog: &Arc<Catalog>,
    project: &ProjectId,
    photos: &[PhotoId],
    people: &[aura_core::IdentityId],
) -> AuraResult<()> {
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "phase 19".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-08-19T00:00:00Z".to_string(),
        updated_at: "2026-08-19T00:00:00Z".to_string(),
    };
    catalog
        .writer()
        .transact(move |conn| repo::create_project(conn, &row))?;
    let project_key = project.to_db();
    let ids: Vec<String> = photos.iter().map(PhotoId::to_db).collect();
    let mut identities: Vec<String> = people.iter().map(aura_core::IdentityId::to_db).collect();
    identities.sort_unstable();
    identities.dedup();
    let identity_project = project.to_db();
    catalog.writer().transact(move |tx| {
        for identity in &identities {
            tx.execute(
                "INSERT INTO identities (id, project_id, created_at, updated_at)
                 VALUES (?1, ?2, '2026-08-19T00:00:00Z', '2026-08-19T00:00:00Z')",
                params![identity, identity_project],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("identity", &e))?;
        }
        for (index, photo) in ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                    camera_make, camera_model, iso, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 1600,
                         '2026-08-19T00:00:00Z', '2026-08-19T00:00:00Z')",
                params![
                    photo,
                    project_key,
                    format!("2026-08-19T{:02}:{:02}:00Z", index / 60 % 24, index % 60),
                ],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
        }
        Ok(())
    })
}
