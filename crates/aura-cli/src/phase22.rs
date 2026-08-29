//! The phase 22 mechanical gate.
//!
//! This is the assembly proof for the restoration stack: migration 23 and its objects, the scene
//! profile table and the twenty camera noise models, the bounds the code owns rather than the
//! files, the evidence-driven tier ladder, the four sharpening preconditions, the identity
//! constraint end to end, the self-check and its two levers, the store, and the promise the
//! database keeps rather than the application - a recovered face that cannot be delivered past
//! the identity ceiling.
//!
//! **Nothing here proves a restored photograph looks good.** Section 10.1's headline row is an
//! expert preference study at four ISO steps against three named competitors, and there is no
//! such study in this repository - condition C4 of the exit report. Every number below is measured
//! against synthetic frames whose noise, blur and structure were painted in. The distinction is
//! printed at the end of every run rather than hidden in a test helper.
//!
//! The tests prove the pieces; this proves the assembly. `tests/eval/restore_eval.rs` is the other
//! half and runs under `cargo test`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::restore::{
    DenoiseTier, RestoreCode, RestoreOverride, RestoreService, RunWhere, MAX_FACE_RECOVERY,
    MAX_IDENTITY_DRIFT, MAX_RINGING, MAX_SHARPEN_AMOUNT, MIN_TEXTURE_RETENTION, SHARPEN_KERNEL_LO,
    SKIN_ATTENUATION,
};
use aura_core::{AuraResult, IdentityId, PhotoId, ProjectId};
use aura_restore::decide::Analyser;
use aura_restore::face_recovery::FACE_RECOVERY_HEAD_TRAINED;
use aura_restore::profiles::{NoiseTable, RestoreProfiles, PROFILE_FILE};
use aura_restore::schedule::{self, Capacity};
use aura_restore::store::{RestoreStore, BYTES_PER_IMAGE};
use aura_restore::{denoise, fixtures, kernel, Restore};
use rusqlite::params;

/// Run the phase 22 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase22-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. Migration 23 and every object it owns.
    let catalog_path = work.join("phase22.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 22 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 22, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "restore_plan"),
        ("table", "restore_face"),
        ("view", "v_restore_coverage"),
        ("view", "v_restore_identity"),
        ("index", "idx_restore_review"),
        ("index", "idx_restore_versions"),
        ("index", "idx_restore_tier"),
        ("index", "idx_restore_guarded"),
        ("index", "idx_restore_unmeasured"),
        ("index", "idx_restore_face_refused"),
        ("trigger", "restore_face_drift_disclosed"),
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

    // 2. There is nowhere in this schema to upscale, synthesise or name a skin-tone target.
    //
    // Section 2.2 puts upscaling beyond native resolution and generative reconstruction out of
    // scope for V1, and phase 15's rule forbids an ideal-skin constant anywhere in the product.
    // The way a phase quietly acquires either is by growing a column for it.
    match forbidden_columns(&catalog) {
        Ok(found) if found.is_empty() => {
            println!("  no upscale, synthesis or skin-tone-target column in migration 23");
        }
        Ok(found) => {
            eprintln!(
                "  migration 23 grew a forbidden column: {}",
                found.join(", ")
            );
            failures += 1;
        }
        Err(err) => {
            eprintln!("  column scan: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 3. The two config tables, and the bounds the code owns rather than the files.
    println!();
    println!("the profile tables:");
    let profiles = match RestoreProfiles::embedded() {
        Ok(table) => {
            println!("  scene profiles loaded at version {}", table.version());
            Some(table)
        }
        Err(err) => {
            eprintln!("  scene profiles: [{}] {}", err.code, err.detail);
            failures += 1;
            None
        }
    };
    if let Some(table) = profiles.as_ref() {
        if table.unlisted().is_empty() {
            println!("  every scene has a row");
        } else {
            println!(
                "  {} scenes fall back to the neutral row: {}",
                table.unlisted().len(),
                table.unlisted().join(", ")
            );
        }
        if table.max_sharpen() <= MAX_SHARPEN_AMOUNT
            && table.max_face_recovery() <= MAX_FACE_RECOVERY
            && table.skin_attenuation() >= SKIN_ATTENUATION
        {
            println!("  the file's ceilings are inside the contract's");
        } else {
            eprintln!("  the file widened a ceiling");
            failures += 1;
        }

        // The bound is the code's, not the file's. A file that raised one is refused; this is the
        // same attempt one layer up, so a loader that started clamping instead of refusing fails
        // here.
        let text = std::fs::read_to_string("crates/aura-restore/config/restore_profiles.toml")
            .unwrap_or_default();
        if text.is_empty() {
            println!(
                "  (the on-disk profile file was not readable from here; skipping the raise test)"
            );
        } else {
            let raised = text.replace("max_sharpen      = 0.50", "max_sharpen      = 0.95");
            match RestoreProfiles::parse(&raised, PROFILE_FILE) {
                Err(_) => println!("  a table that raised the sharpening ceiling would be refused"),
                Ok(_) => {
                    eprintln!("  a table that raised the sharpening ceiling would be accepted");
                    failures += 1;
                }
            }
            let weakened = text.replace("skin_attenuation = 0.80", "skin_attenuation = 0.10");
            match RestoreProfiles::parse(&weakened, PROFILE_FILE) {
                Err(_) => println!("  a table that weakened the skin attenuation would be refused"),
                Ok(_) => {
                    eprintln!("  a table that weakened the skin attenuation would be accepted");
                    failures += 1;
                }
            }
        }
    }

    let cameras = match NoiseTable::embedded() {
        Ok(table) => {
            println!("  {} camera noise models loaded", table.len());
            Some(table)
        }
        Err(err) => {
            eprintln!("  noise models: [{}] {}", err.code, err.detail);
            failures += 1;
            None
        }
    };
    if let Some(table) = cameras.as_ref() {
        let measured = table.bodies().iter().filter(|m| m.measured).count();
        if measured == 0 {
            println!(
                "  none of the {} bodies is measured, so none may reach `strong` (ADR-0047 s3)",
                table.len()
            );
        } else {
            // Not a failure: a measured body is what the phase is waiting for. It is reported
            // loudly because it changes what the product may do.
            println!("  {measured} bodies are MEASURED and may now reach `strong`");
        }
        for model in table.bodies() {
            if !model.measured && model.tier_ceiling() != DenoiseTier::Standard {
                eprintln!("  {}: an unmeasured body is not capped", model.camera);
                failures += 1;
            }
        }
    }

    // 4. The tier ladder, driven by evidence rather than by a preference.
    println!();
    println!("the tier ladder, on measured noise:");
    if let (Some(profiles), Some(cameras)) = (profiles.as_ref(), cameras.as_ref()) {
        let mut model = cameras.model_for("SONY", "ILCE-7M3");
        model.measured = true;
        for (relative, expected) in [
            (0.80_f32, DenoiseTier::Off),
            (1.20, DenoiseTier::Light),
            (2.00, DenoiseTier::Standard),
            (3.50, DenoiseTier::Strong),
        ] {
            let choice = denoise::choose(
                denoise::NoiseEvidence {
                    relative: Some(relative),
                    prominence: 0.2,
                    output_long_edge: 1600,
                    iso: 6400,
                },
                &model,
                DenoiseTier::Strong,
                profiles,
            );
            if choice.tier == expected {
                println!("  sigma_rel {relative:.2} -> {expected}");
            } else {
                eprintln!(
                    "  sigma_rel {relative:.2} -> {} (wanted {expected})",
                    choice.tier
                );
                failures += 1;
            }
        }

        // And the frame the scene already tolerates gets nothing, with a reason.
        let choice = denoise::choose(
            denoise::NoiseEvidence {
                relative: Some(0.5),
                prominence: 0.9,
                output_long_edge: 6000,
                iso: 25_600,
            },
            &model,
            DenoiseTier::Strong,
            profiles,
        );
        if choice.tier == DenoiseTier::Off
            && choice
                .reasons
                .iter()
                .any(|r| r.code == RestoreCode::NoiseWithinTolerance)
        {
            println!("  a quiet frame is left alone even at full prominence and print size");
        } else {
            eprintln!("  a quiet frame was denoised anyway: {}", choice.tier);
            failures += 1;
        }
    }

    // 5. The four sharpening preconditions, on frames whose answer is painted in.
    println!();
    println!("the sharpening preconditions:");
    let analyser = match Analyser::embedded(Capacity::default()) {
        Ok(analyser) => analyser,
        Err(err) => {
            eprintln!("  analyser: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };

    for (label, frame, wanted) in [
        (
            "motion is refused",
            fixtures::motion_frame(),
            Some(RestoreCode::MotionDominated),
        ),
        (
            "gross defocus is refused",
            fixtures::back_focus_frame(),
            Some(RestoreCode::GrossDefocus),
        ),
        (
            "a scene that forbids it is never sharpened",
            fixtures::no_sharpen_scene_frame(),
            None,
        ),
    ] {
        match analyser.plan(&frame, None, true) {
            Ok((plan, _)) => {
                let sharpened = plan.sharpen.is_some();
                let named =
                    wanted.is_none_or(|code| plan.reasons.iter().any(|reason| reason.code == code));
                if !sharpened && named {
                    println!("  {label}");
                } else {
                    eprintln!("  {label}: sharpened={sharpened} reason_named={named}");
                    failures += 1;
                }
            }
            Err(err) => {
                eprintln!("  {label}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // The fourth precondition, and the one that refuses every frame on this build.
    let mut unregioned = fixtures::soft_frame();
    unregioned.regions.clear();
    match analyser.plan(&unregioned, None, true) {
        Ok((plan, outcome)) => {
            if plan.sharpen.is_none()
                && !outcome.region_covered
                && plan
                    .reasons
                    .iter()
                    .any(|r| r.code == RestoreCode::SharpenNoRegions)
            {
                println!("  no regions means no sharpening, not a weaker sharpening");
            } else {
                eprintln!("  a frame with no regions was sharpened");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("  unregioned: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // The estimator's own floor sits below the contract's, which is phase 22's own defect as a
    // permanent check. ADR-0047 section 11.1.
    let perfect = fixtures::edge_plate(96, 96, 8);
    let measured = kernel::estimate(&perfect, 96, 96);
    if measured.is_reliable() && measured.sigma < SHARPEN_KERNEL_LO {
        println!(
            "  the sharpest image that can exist measures {:.3}, below the {SHARPEN_KERNEL_LO} floor",
            measured.sigma
        );
    } else {
        eprintln!(
            "  the kernel floor is at or below the estimator's own: {:.3} against {SHARPEN_KERNEL_LO}",
            measured.sigma
        );
        failures += 1;
    }

    // 6. The identity constraint, end to end, through the real renderer.
    println!();
    println!("the identity constraint:");
    if FACE_RECOVERY_HEAD_TRAINED {
        println!("  the face-recovery head reports itself TRAINED; the exit report's C2 has moved");
    } else {
        println!("  the face-recovery head is untrained, so no face in this build is recovered");
    }

    let gentle = fixtures::BandProbe::gentle();
    let severe = fixtures::BandProbe::severe();
    let face_frame = fixtures::soft_face_frame();
    for (label, probe) in [
        ("a harmless operator keeps the face", &gentle),
        ("a drifting operator is refused", &severe),
    ] {
        match analyser.plan(&face_frame, Some(probe), true) {
            Ok((plan, _)) => {
                let broken = plan
                    .recovered
                    .iter()
                    .any(|face| !face.skipped && face.identity_drift > MAX_IDENTITY_DRIFT);
                if broken {
                    eprintln!("  {label}: a delivered face moved past the ceiling");
                    failures += 1;
                } else {
                    println!("  {label}: worst kept drift {:.4}", plan.worst_kept_drift());
                }
            }
            Err(err) => {
                eprintln!("  {label}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 7. The self-check and its two levers.
    println!();
    println!("the artefact self-check:");
    match analyser.plan(&fixtures::noisy_frame(), None, true) {
        Ok((plan, _)) => match plan.selfcheck {
            Some(report) => {
                if report.is_clean() {
                    println!(
                        "  a denoised frame kept {:.3} of its texture and rang at {:.4}",
                        report.texture_retention, report.ringing
                    );
                } else {
                    eprintln!("  a stored plan is outside its own bounds: {report:?}");
                    failures += 1;
                }
                if report.texture_retention < MIN_TEXTURE_RETENTION || report.ringing > MAX_RINGING
                {
                    eprintln!("  the bounds are not being enforced");
                    failures += 1;
                }
            }
            None => {
                eprintln!("  a frame that was denoised carries no self-check");
                failures += 1;
            }
        },
        Err(err) => {
            eprintln!("  noisy frame: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 8. The store, and the promise the database keeps rather than the application.
    println!();
    println!("the store:");
    let project = ProjectId::new();
    let photos: Vec<PhotoId> = (0..8).map(|_| PhotoId::new()).collect();
    let people: Vec<IdentityId> = (0..2).map(|_| IdentityId::new()).collect();
    if let Err(err) = seed(&catalog, &project, &photos, &people) {
        eprintln!("  seed: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }
    let store = Arc::new(RestoreStore::new(Arc::clone(&catalog), Arc::clone(&clock)));

    let mut stored = 0usize;
    for (index, photo) in photos.iter().enumerate() {
        let mut frame = if index % 2 == 0 {
            fixtures::noisy_frame()
        } else {
            fixtures::clean_frame()
        };
        frame.image_id = *photo;
        match analyser.plan(&frame, None, true) {
            Ok((plan, _)) => match store.put(&project, &plan) {
                Ok(()) => stored += 1,
                Err(err) => {
                    eprintln!("  put: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            },
            Err(err) => {
                eprintln!("  plan: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }
    println!("  {stored} of {} plans stored", photos.len());

    let service = match Restore::new(Arc::clone(&store)) {
        Ok(service) => service,
        Err(err) => {
            eprintln!("  service: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match service.outline(project) {
        Ok(outline) => {
            println!(
                "  coverage {:.0}% over {} photographs, {} acted on, {} tiers",
                outline.coverage * 100.0,
                outline.photos,
                outline.acted_on,
                outline.tier_histogram.iter().filter(|c| **c > 0).count()
            );
            if outline.identity_guarantee_broken() {
                eprintln!("  a delivered face in this project moved past the ceiling");
                failures += 1;
            } else {
                println!(
                    "  worst kept identity drift {:.4}, ceiling {MAX_IDENTITY_DRIFT}",
                    outline.worst_identity_drift
                );
            }
            if outline.unmeasured_cameras.is_empty() {
                println!("  no unmeasured camera denoised anything");
            } else {
                println!(
                    "  {} unmeasured bodies: {}",
                    outline.unmeasured_cameras.len(),
                    outline.unmeasured_cameras.join(", ")
                );
            }
        }
        Err(err) => {
            eprintln!("  outline: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // A photographer's tier survives a re-analysis. Eleventh time this rule is checked.
    if let Some(photo) = photos.first() {
        let chosen = RestoreOverride {
            denoise: Some(DenoiseTier::Light),
            ..RestoreOverride::default()
        };
        match service.set_override(*photo, chosen) {
            Ok(()) => {
                let mut frame = fixtures::noisy_frame();
                frame.image_id = *photo;
                if let Ok((plan, _)) = analyser.plan(&frame, None, true) {
                    drop(store.put(&project, &plan));
                }
                match service.of_image(*photo) {
                    Ok(Some(plan)) if plan.user_edited => {
                        println!("  a photographer's choice survived a re-analysis");
                    }
                    Ok(Some(_)) => {
                        eprintln!("  a re-analysis cleared `user_edited`");
                        failures += 1;
                    }
                    Ok(None) => {
                        eprintln!("  the plan vanished");
                        failures += 1;
                    }
                    Err(err) => {
                        eprintln!("  read back: [{}] {}", err.code, err.detail);
                        failures += 1;
                    }
                }
            }
            Err(err) => {
                eprintln!("  set_override: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // The promise the database keeps. See migration 23, note 1.
    println!();
    println!("what the database refuses:");
    if let Some(photo) = photos.first() {
        match deliver_a_drifted_face(&catalog, photo) {
            Ok(Attempt::Refused) => {
                println!("  a recovered face cannot be delivered past the identity ceiling");
            }
            Ok(Attempt::Allowed) => {
                eprintln!("  a face above the identity ceiling was delivered");
                failures += 1;
            }
            Ok(Attempt::Inconclusive(why)) => {
                // Phase 21's correction: a refusal test that cannot tell a working guard from a
                // broken fixture proves nothing.
                eprintln!("  INCONCLUSIVE: {why}");
                failures += 1;
            }
            Err(err) => {
                eprintln!("  identity refusal: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 9. Nothing in this build reaches a provider.
    println!();
    println!("the cloud path:");
    let mut reachable = false;
    for gpu in [false, true] {
        for cloud_consent in [false, true] {
            let destination = schedule::where_to_run(Capacity { gpu, cloud_consent }, 45.0).0;
            if destination == RunWhere::Cloud {
                reachable = true;
            }
        }
    }
    if reachable {
        eprintln!("  a frame reached the cloud; PHASE-22 section 7 says the gateway stays idle");
        failures += 1;
    } else {
        println!("  no combination of capability and consent reaches a provider");
    }

    // 10. What this run does not prove.
    println!();
    println!("what this gate does not prove:");
    println!("  - that a restored photograph looks good. Section 10.1's headline row is an expert");
    println!("    preference study at ISO 3200/6400/12800/25600 against DxO, Topaz and Lightroom,");
    println!("    and there is no panel and no reference wedding here. Condition C4.");
    println!("  - that the identity constraint protects a real identity. Phase 06's recogniser is");
    println!("    an untrained placeholder, so what is measured above is that the constraint");
    println!("    refuses what it should refuse. Condition C2.");
    println!("  - that sharpening works through real regions. Phase 18's segmenter is a");
    println!("    placeholder and no generator is wired into this pass, so no frame in this build");
    println!("    is sharpened at all. Condition C3.");
    println!("  - any performance figure. This build links no `wgpu` backend, so four of section");
    println!("    11's five rows are waived. Condition C6.");
    println!("  storage: {BYTES_PER_IMAGE} B/image budgeted for `restore_plan` and `restore_face`");

    println!();
    if failures == 0 {
        println!("phase 22: all checks passed");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase 22: {failures} check(s) failed");
        ExitCode::FAILURE
    }
}

/// Whether one schema object exists.
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

/// Any column in migration 23's tables that would let this phase do what section 2.2 forbids.
fn forbidden_columns(catalog: &Catalog) -> AuraResult<Vec<String>> {
    catalog.read(move |conn| {
        let mut found = Vec::new();
        for table in ["restore_plan", "restore_face"] {
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
                let column: String = row.get(1).unwrap_or_default();
                let lower = column.to_ascii_lowercase();
                for banned in [
                    "scale",
                    "upscale",
                    "resample",
                    "synth",
                    "generat",
                    "inpaint",
                    "landmark",
                    "reshape",
                    "slim",
                    "lighten",
                    "skin_target",
                    "ideal",
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

/// What happened when the gate tried to do something the database should refuse.
///
/// Phase 21's correction, inherited: a refusal test that cannot tell a working guard from a broken
/// fixture proves nothing, so the attempt runs a control first and reports `Inconclusive` rather
/// than success when it never reached the thing under test.
enum Attempt {
    /// The database allowed it. The guarantee is not being kept.
    Allowed,
    /// The database refused it, which is what should happen.
    Refused,
    /// The attempt never got far enough to be refused by the thing under test.
    Inconclusive(String),
}

/// Insert a skipped face, then try to deliver it with its drift still above the ceiling.
///
/// The trigger in migration 23 is what has to stop the second statement. This is the exact shape
/// of the change a well-meaning "recover this one anyway" button would make.
fn deliver_a_drifted_face(catalog: &Catalog, photo: &PhotoId) -> AuraResult<Attempt> {
    let photo_key = photo.to_db();
    catalog.writer().transact(move |conn| {
        // The control first: a sound row with the same shape, which must go in. Without it a
        // foreign-key failure would read as the trigger doing its job.
        if let Err(err) = conn.execute(
            "INSERT OR REPLACE INTO restore_face
                 (photo_id, seq, x, y, w, h, sharpness, strength, identity_drift, resolves,
                  skipped, skipped_because)
             VALUES (?1, 14, 0.1, 0.1, 0.2, 0.2, 0.55, 0.0, 0.02, 0, 1,
                     'restore_recovery_head_untrained')",
            params![photo_key],
        ) {
            return Ok(Attempt::Inconclusive(format!(
                "an ordinary skipped face would not insert either: {err}"
            )));
        }
        // Now the one that must be refused: the same row, un-skipped, with a drift far above the
        // ceiling.
        if conn
            .execute(
                "UPDATE restore_face SET identity_drift = 0.90 WHERE photo_id = ?1 AND seq = 14",
                params![photo_key],
            )
            .is_err()
        {
            return Ok(Attempt::Inconclusive(
                "the drift could not be raised on a skipped row".to_string(),
            ));
        }
        match conn.execute(
            "UPDATE restore_face SET skipped = 0, skipped_because = NULL
              WHERE photo_id = ?1 AND seq = 14",
            params![photo_key],
        ) {
            Ok(_) => Ok(Attempt::Allowed),
            Err(_) => Ok(Attempt::Refused),
        }
    })
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
        name: "phase 22".to_string(),
        couple_label: Some("A and B".to_string()),
        event_date: Some("2026-08-21".to_string()),
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-08-21T00:00:00Z".to_string(),
        updated_at: "2026-08-21T00:00:00Z".to_string(),
    };
    catalog
        .writer()
        .transact(move |conn| repo::create_project(conn, &row))?;

    let ids: Vec<String> = photos.iter().map(PhotoId::to_db).collect();
    let identities: Vec<String> = people.iter().map(IdentityId::to_db).collect();
    let project_key = project.to_db();
    let identity_project = project.to_db();
    catalog.writer().transact(move |tx| {
        for identity in &identities {
            tx.execute(
                "INSERT INTO identities (id, project_id, created_at, updated_at)
                 VALUES (?1, ?2, '2026-08-21T00:00:00Z', '2026-08-21T00:00:00Z')",
                params![identity, identity_project],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("identity", &e))?;
        }
        for (index, photo) in ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                    camera_make, camera_model, iso, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 6400,
                         '2026-08-21T00:00:00Z', '2026-08-21T00:00:00Z')",
                params![
                    photo,
                    project_key,
                    format!("2026-08-21T10:{:02}:00Z", index % 60),
                ],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
        }
        Ok(())
    })
}
