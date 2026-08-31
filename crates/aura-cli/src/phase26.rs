//! The phase 26 mechanical gate.
//!
//! The assembly proof for multi-camera matching: migration 26 and its objects, the policy table a
//! product manager owns and the widened bound it refuses, the eight bundled brand baselines and the
//! composition property that makes them safe, a whole synthetic two-camera wedding through the real
//! pass, the ordering phase 25 depends on, the bounds on every stored row, the shooter cap, and what
//! a photographer's own decisions survive.
//!
//! **Nothing here proves anything about a real photograph.** There are no multi-camera weddings in
//! this repository, no measured body and no photographed colour target; every fixture below is a
//! wedding whose per-brand colour response this file already knows the size of, and every bundled
//! baseline was fabricated. That is conditions C1 to C4 of the exit report, and they are printed at
//! the end of every run rather than hidden in a helper.
//!
//! The unit tests prove the pieces and `tests/eval/camera_eval.rs` proves the gates. This proves the
//! assembly - and it is the only place that checks the things that only exist when a catalog, a
//! policy file, a baseline library and a pass are in the same process.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_brain_gallery::camera::baseline::{self, Library};
use aura_brain_gallery::camera::fingerprint::CameraFrame;
use aura_brain_gallery::camera::fixtures::{Body, Shape};
use aura_brain_gallery::camera::policy::Matching;
use aura_brain_gallery::camera::store::CameraStore;
use aura_brain_gallery::camera::{
    api, fixtures, pairs, report, shooter, transform, CameraMatching, Field, MatchingPass,
};
use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::camera::{
    Brand, CameraCode, CameraMatchService, CameraOverride, FlashState, TransformBound,
    TransformSource, MAX_CHANNEL_GAIN, MAX_SHOOTER_EV, MAX_T_CCT_K,
};
use aura_core::contract::moment::CameraId;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{ProjectId, SceneId};
use rusqlite::params;

/// Run the phase 26 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase26-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // ---------------------------------------------------------------------------------------
    // 1. Migration 26 and every object it owns.
    // ---------------------------------------------------------------------------------------
    let catalog_path = work.join("phase26.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 26 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 26, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "camera_fingerprint"),
        ("table", "camera_transform"),
        ("table", "camera_pair"),
        ("table", "camera_shooter_bias"),
        ("table", "camera_reference"),
        ("view", "v_camera_evidence"),
        ("view", "v_camera_unmatched"),
        ("trigger", "camera_reference_keep_user"),
        ("trigger", "camera_transform_keep_user_edit"),
        ("trigger", "camera_pair_heldout_is_fixed"),
        ("index", "idx_camera_fingerprint_project"),
        ("index", "idx_camera_transform_project"),
        ("index", "idx_camera_transform_source"),
        ("index", "idx_camera_pair_camera"),
        ("index", "idx_camera_pair_node"),
        ("index", "idx_camera_pair_frames"),
        ("index", "idx_camera_shooter_project"),
    ] {
        let found = catalog.read(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                params![kind, name],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|err| aura_core::errors::db::statement_failed("sqlite_master", &err))
        });
        match found {
            Ok(1) => {}
            Ok(_) => {
                eprintln!("migration 26: {kind} {name} is missing");
                failures += 1;
            }
            Err(err) => {
                eprintln!("migration 26: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }
    println!("migration 26: five tables, two views, three triggers, seven indexes");

    // ---------------------------------------------------------------------------------------
    // 2. The schema cannot express an absolute, and it has no ideal-skin constant.
    // ---------------------------------------------------------------------------------------
    //
    // Phase 15's rule and phase 25's scan, run over this phase's own tables. A `camera_transform`
    // with a `cct_k` column would be a second answer to "what colour was the light", which is phase
    // 15's question and phase 15's row; a table with an ideal-skin constant in it would be the
    // fixed target `docs/skin-fairness.md` promises does not exist.
    let schema: String = match catalog.read(|conn| {
        let mut statement = conn
            .prepare("SELECT COALESCE(sql, '') FROM sqlite_master WHERE name LIKE 'camera%'")
            .map_err(|err| aura_core::errors::db::statement_failed("schema", &err))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|err| aura_core::errors::db::statement_failed("schema", &err))?;
        let mut text = String::new();
        for row in rows {
            text.push_str(
                &row.map_err(|err| aura_core::errors::db::statement_failed("schema row", &err))?,
            );
            text.push('\n');
        }
        Ok(text)
    }) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("schema scan: [{}] {}", err.code, err.detail);
            failures += 1;
            String::new()
        }
    };
    for banned in [
        "ideal_skin",
        "skin_target_uv",
        "reference_skin",
        "preferred_skin",
    ] {
        if schema.contains(banned) {
            eprintln!("schema: `{banned}` is a fixed skin target and must not exist");
            failures += 1;
        }
    }
    // A movement column that is not a residual would let this phase become a second answer to a
    // question phase 15 already owns.
    for column in ["cct_k ", "tint REAL", "exposure_ev "] {
        if schema.contains(column) {
            eprintln!("schema: `{column}` is an absolute; every movement here is a residual");
            failures += 1;
        }
    }
    println!("schema: no absolute, no ideal-skin constant");

    // ---------------------------------------------------------------------------------------
    // 3. The policy table, and the two directions a studio may move it in.
    // ---------------------------------------------------------------------------------------
    let policy = match Matching::load(aura_brain_gallery::camera::policy::BUNDLED) {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("policy: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    if !policy.untargeted().is_empty() {
        eprintln!(
            "policy: {} scenes have no argued-over row: {:?}",
            policy.untargeted().len(),
            policy.untargeted()
        );
        failures += 1;
    }
    for bound in TransformBound::ALL {
        if policy.bound(bound) > bound.ceiling() {
            eprintln!("policy: {bound} is wider than the contract's own ceiling");
            failures += 1;
        }
    }
    // A ceiling can be lowered by a studio and raised by nobody.
    let widened = format!(
        "version = 1\n[bounds]\nmax_channel_gain = {}\n",
        MAX_CHANNEL_GAIN * 2.0
    );
    match Matching::load(&widened) {
        Err(err) if err.code.0 == "AURA-ML-5133" => {}
        Err(err) => {
            eprintln!(
                "policy: a widened bound raised {} rather than 5133",
                err.code
            );
            failures += 1;
        }
        Ok(_) => {
            eprintln!("policy: a widened bound was accepted; the ceiling is not a ceiling");
            failures += 1;
        }
    }
    // An evidence threshold moves the other way: it may be raised and never lowered, because
    // "fewer pairs are enough" is a way of widening every bound at once without touching one.
    match Matching::load("version = 1\n[evidence]\nmin_pairs = 4\n") {
        Err(err) if err.code.0 == "AURA-ML-5133" => {}
        _ => {
            eprintln!("policy: a loosened evidence threshold was accepted");
            failures += 1;
        }
    }
    match Matching::load("version = 1\n[shooter]\nshare = 1.0\n") {
        Err(err) if err.code.0 == "AURA-ML-5133" => {}
        _ => {
            eprintln!("policy: a shooter share of one was accepted; it erases a photographer");
            failures += 1;
        }
    }
    println!(
        "policy: version {}, {} scenes, bounds lowerable and evidence raisable only",
        policy.version,
        policy.scene_count()
    );

    // ---------------------------------------------------------------------------------------
    // 4. The bundled baselines, and the property that makes composing them safe.
    // ---------------------------------------------------------------------------------------
    let library = Library::bundled();
    if library.len() != Brand::COUNT {
        eprintln!("baselines: {} of {} loaded", library.len(), Brand::COUNT);
        failures += 1;
    }
    for brand in Brand::ALL {
        for flash in FlashState::ALL {
            let (departure, bound) = baseline::between(&library, brand, brand, flash);
            if !departure.is_neutral() || bound.is_some() {
                eprintln!("baselines: {brand} composed with itself moved something under {flash}");
                failures += 1;
            }
        }
    }
    // An unknown manufacturer is the identity rather than the nearest brand's numbers.
    let unknown = aura_brain_gallery::camera::solve::from_baseline(
        &CameraId::new("cam_x"),
        FlashState::Ambient,
        &CameraId::new("cam_a"),
        Brand::Canon,
        Brand::Other,
        &library,
        &policy,
    );
    if !unknown.is_identity() || unknown.confidence != 0.0 {
        eprintln!("baselines: an unknown manufacturer was corrected by guesswork");
        failures += 1;
    }
    println!(
        "baselines: {} brands, self-composition is the identity, unknown brands change nothing",
        library.len()
    );

    // ---------------------------------------------------------------------------------------
    // 5. A whole synthetic two-camera wedding through the real pass.
    // ---------------------------------------------------------------------------------------
    let project = ProjectId::new();
    let frames = fixtures::wedding(
        &[Body::REFERENCE, Body::SECOND],
        Shape {
            nodes: 4,
            per_node: 20,
            ..Shape::default()
        },
    );
    if let Err(err) = seed_project(&catalog, project, &clock)
        .and_then(|()| seed_photos(&catalog, project, &frames, &clock))
    {
        eprintln!("seed: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }
    let pass = MatchingPass::new(Arc::clone(&catalog), Arc::clone(&clock));
    let started = std::time::Instant::now(); // DETERMINISM: measuring section 11's budget, not deciding
    let run = match pass.run(project, &frames, &[], &NullProgress, &CancelToken::new()) {
        Ok(run) => run,
        Err(err) => {
            eprintln!("pass: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let elapsed = started.elapsed();
    println!(
        "pass: {} cameras, {} verified pairs ({} rejected, {} held out), {} solved, {} blended, \
         {} from a baseline, in {} ms",
        run.cameras,
        run.pairs,
        run.pairs_rejected,
        run.heldout_pairs,
        run.solved,
        run.blended,
        run.baseline_only,
        elapsed.as_millis()
    );
    if run.cameras != 2 {
        eprintln!("pass: expected two cameras, found {}", run.cameras);
        failures += 1;
    }
    if run.solved == 0 {
        eprintln!("pass: nothing was solved from this wedding's own evidence");
        failures += 1;
    }
    if run.reference.as_ref().map(CameraId::as_str) != Some(Body::REFERENCE.id) {
        eprintln!("pass: the primary shooter's body was not chosen as the reference");
        failures += 1;
    }
    if run.signature_reduction() < 0.65 {
        eprintln!(
            "pass: grade-signature distance fell only {:.0} %, below the 65 % promised",
            run.signature_reduction() * 100.0
        );
        failures += 1;
    }
    // Section 11 budgets 25 s for a whole wedding's matching pass. The fixture is 160 frames rather
    // than 3,000, so this is a smoke test on the order of magnitude rather than the budget itself -
    // `crates/aura-perf` owns the budget.
    if elapsed.as_secs() > 25 {
        eprintln!(
            "pass: {} s, past section 11's 25 s budget",
            elapsed.as_secs()
        );
        failures += 1;
    }

    let matching = CameraMatching::new(Arc::clone(&catalog), Arc::clone(&clock));
    let stored = match matching.transforms(project) {
        Ok(rows) => rows,
        Err(err) => {
            eprintln!("transforms: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    if stored.len() != 4 {
        eprintln!(
            "transforms: expected two bodies times two flash states, found {}",
            stored.len()
        );
        failures += 1;
    }
    for row in &stored {
        if !row.within_bounds() {
            eprintln!(
                "transforms: {} / {} is outside its documented movement",
                row.camera_id, row.flash
            );
            failures += 1;
        }
        if row.reasons.is_empty() {
            eprintln!(
                "transforms: {} / {} carries no reason; invariant 2",
                row.camera_id, row.flash
            );
            failures += 1;
        }
    }
    println!(
        "transforms: {} rows, every one bounded and explained",
        stored.len()
    );

    // ---------------------------------------------------------------------------------------
    // 6. The ordering phase 25 depends on. Section 6.4.
    // ---------------------------------------------------------------------------------------
    //
    // The one property in this phase that another phase's correctness rests on: a gallery frame
    // that reaches the consistency pass must already carry the camera correction, or every node's
    // target is the average of two brands' colour science.
    let second = stored
        .iter()
        .find(|row| row.camera_id.as_str() == Body::SECOND.id && row.flash == FlashState::Ambient);
    match second {
        Some(row) if !row.is_identity() => {
            let image = frames
                .iter()
                .find(|frame| frame.camera.as_str() == Body::SECOND.id)
                .map(|frame| frame.image);
            if let Some(image) = image {
                let field = Field::from_transforms(
                    std::slice::from_ref(row),
                    &[(image, CameraId::new(Body::SECOND.id), FlashState::Ambient)],
                );
                let mut gallery = vec![fixtures::plain_gallery_frame(image)];
                let before = gallery.first().and_then(|frame| frame.cct_k).unwrap_or(0.0);
                let moved = field.apply_to_gallery_frames(&mut gallery);
                let after = gallery.first().and_then(|frame| frame.cct_k).unwrap_or(0.0);
                if moved != 1 || (after - before - row.d_cct).abs() > 1.0 {
                    eprintln!(
                        "ordering: a gallery frame did not carry the camera correction \
                         ({before:.0} K -> {after:.0} K, expected {:.0} K of movement)",
                        row.d_cct
                    );
                    failures += 1;
                } else {
                    println!(
                        "ordering: a gallery frame carries the camera correction before phase 25 \
                         builds its tree ({before:.0} K -> {after:.0} K)"
                    );
                }
            }
        }
        Some(_) => {
            eprintln!("ordering: the second body's transform is the identity; nothing to check");
            failures += 1;
        }
        None => {
            eprintln!("ordering: the second body has no ambient transform");
            failures += 1;
        }
    }

    // A disabled body is absent from the field rather than present as an identity: the two look the
    // same in a gallery and mean opposite things.
    if let Some(row) = second {
        let mut off = row.clone();
        off.enabled = false;
        let image = frames.first().map(|frame| frame.image).unwrap_or_default();
        let field = Field::from_transforms(
            &[off],
            &[(image, CameraId::new(Body::SECOND.id), FlashState::Ambient)],
        );
        if !field.is_empty() {
            eprintln!("ordering: a disabled camera reached phase 25 as an identity");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 7. The fallback path, and that the report says so honestly.
    // ---------------------------------------------------------------------------------------
    let apart = fixtures::wedding_with_no_overlap(
        &[Body::REFERENCE, Body::SECOND],
        Shape {
            nodes: 4,
            per_node: 20,
            ..Shape::default()
        },
    );
    let candidates = pairs::find(
        &apart,
        &CameraId::new(Body::REFERENCE.id),
        &CameraId::new(Body::SECOND.id),
        &policy,
    );
    if !candidates.is_empty() {
        eprintln!(
            "fallback: a wedding with no overlap produced {} pairs",
            candidates.len()
        );
        failures += 1;
    }
    let fallback_project = ProjectId::new();
    if let Err(err) = seed_project(&catalog, fallback_project, &clock)
        .and_then(|()| seed_photos(&catalog, fallback_project, &apart, &clock))
    {
        eprintln!("seed: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }
    match pass.run(
        fallback_project,
        &apart,
        &[],
        &NullProgress,
        &CancelToken::new(),
    ) {
        Ok(run) => {
            if run.baseline_only == 0 {
                eprintln!("fallback: no body fell back on a brand baseline");
                failures += 1;
            }
            let rows = matching.transforms(fallback_project).unwrap_or_default();
            let honest = rows.iter().any(|row| {
                row.source == TransformSource::BrandBaseline
                    && row
                        .reasons
                        .iter()
                        .any(|reason| reason.code == CameraCode::PairsAbsent)
            });
            if !honest {
                eprintln!("fallback: no row says why it fell back");
                failures += 1;
            }
            let outline = matching.outline(fallback_project).unwrap_or_default();
            let text = report::summary(&outline);
            if !text.contains("knows about the brand") {
                eprintln!("fallback: the report does not say the correction came from the brand");
                failures += 1;
            }
            println!("fallback: {}", text);
        }
        Err(err) => {
            eprintln!("fallback: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 8. The shooter cap. Section 6.3's product decision, as arithmetic.
    // ---------------------------------------------------------------------------------------
    let bias = shooter::measure(&frames, &CameraId::new(Body::REFERENCE.id), &policy);
    let usable: Vec<_> = bias.iter().filter(|row| row.is_usable()).collect();
    if usable.is_empty() {
        eprintln!("shooter: no habit was measured on a wedding built to contain one");
        failures += 1;
    }
    for row in &usable {
        if row.applied_ev.abs() >= row.measured_ev.abs() {
            eprintln!(
                "shooter: {} in {} was corrected by all of a {:.2} EV habit",
                row.shooter, row.scene, row.measured_ev
            );
            failures += 1;
        }
        if row.applied_ev.abs() > MAX_SHOOTER_EV + f32::EPSILON {
            eprintln!(
                "shooter: a correction of {:.2} EV is past the cap",
                row.applied_ev
            );
            failures += 1;
        }
        if row.applied_ev * row.measured_ev > 0.0 {
            eprintln!("shooter: the correction has the same sign as the habit it corrects");
            failures += 1;
        }
    }
    println!(
        "shooter: {} habits measured, every correction smaller than the habit and opposite in sign",
        usable.len()
    );

    // ---------------------------------------------------------------------------------------
    // 9. What a photographer decided survives a re-pass.
    // ---------------------------------------------------------------------------------------
    let store = CameraStore::new(Arc::clone(&catalog), Arc::clone(&clock));
    if let Err(err) = matching.set_reference(project, &CameraId::new(Body::SECOND.id)) {
        eprintln!(
            "decisions: set_reference raised [{}] {}",
            err.code, err.detail
        );
        failures += 1;
    }
    if let Err(err) = matching.set_enabled(project, &CameraId::new(Body::SECOND.id), false) {
        eprintln!(
            "decisions: set_enabled raised [{}] {}",
            err.code, err.detail
        );
        failures += 1;
    }
    if let Err(err) = matching.set_override(
        project,
        &CameraId::new(Body::SECOND.id),
        FlashState::Ambient,
        CameraOverride {
            d_cct: Some(120.0),
            ..CameraOverride::default()
        },
    ) {
        eprintln!(
            "decisions: set_override raised [{}] {}",
            err.code, err.detail
        );
        failures += 1;
    }
    // A value past the ceiling is refused rather than clamped.
    match matching.set_override(
        project,
        &CameraId::new(Body::SECOND.id),
        FlashState::Ambient,
        CameraOverride {
            d_cct: Some(MAX_T_CCT_K * 2.0),
            ..CameraOverride::default()
        },
    ) {
        Err(err) if err.code.0 == "AURA-ML-5131" => {}
        _ => {
            eprintln!("decisions: an override past the ceiling was accepted");
            failures += 1;
        }
    }
    // And an empty one, which would take a camera out of automation without changing anything.
    match matching.set_override(
        project,
        &CameraId::new(Body::SECOND.id),
        FlashState::Ambient,
        CameraOverride::default(),
    ) {
        Err(err) if err.code.0 == "AURA-ML-5131" => {}
        _ => {
            eprintln!("decisions: an empty override was accepted");
            failures += 1;
        }
    }

    let taken = store.take_decisions(project).unwrap_or_default();
    if taken.reference.as_ref().map(CameraId::as_str) != Some(Body::SECOND.id) {
        eprintln!("decisions: the chosen reference was not read back");
        failures += 1;
    }
    if !taken.disabled.contains(Body::SECOND.id) {
        eprintln!("decisions: the disabled body was not read back");
        failures += 1;
    }
    if taken.overrides.is_empty() {
        eprintln!("decisions: the hand-set transform was not read back");
        failures += 1;
    }

    if let Err(err) = pass.run(project, &frames, &[], &NullProgress, &CancelToken::new()) {
        eprintln!("re-pass: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    let after = matching.reference(project).ok().flatten();
    if after.as_ref().map(|row| row.camera_id.as_str()) != Some(Body::SECOND.id) {
        eprintln!("re-pass: automation replaced the reference the photographer chose");
        failures += 1;
    }
    let survived = matching
        .transforms(project)
        .unwrap_or_default()
        .into_iter()
        .find(|row| row.camera_id.as_str() == Body::SECOND.id && row.flash == FlashState::Ambient);
    match survived {
        Some(row) if row.user_edited && !row.enabled => {
            println!("decisions: a chosen reference, a disabled body and a hand-set correction all survived a re-pass");
        }
        Some(row) => {
            eprintln!(
                "re-pass: user_edited = {}, enabled = {}; a photographer's decision was overwritten",
                row.user_edited, row.enabled
            );
            failures += 1;
        }
        None => {
            eprintln!("re-pass: the second body lost its transform");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 10. The triggers, each with a control first.
    // ---------------------------------------------------------------------------------------
    //
    // Phase 21's lesson: a refusal test that cannot tell a working trigger from a broken fixture
    // proves nothing. Each check below runs a statement that *must* succeed before running the one
    // that must fail, and reports `inconclusive` rather than success when the control did not.
    for (name, control, attempt) in [
        (
            "camera_transform_keep_user_edit",
            "UPDATE camera_transform SET confidence = confidence WHERE project_id = ?1",
            "UPDATE camera_transform SET user_edited = 0 WHERE project_id = ?1 AND user_edited = 1",
        ),
        (
            "camera_pair_heldout_is_fixed",
            "UPDATE camera_pair SET gap_ms = gap_ms WHERE project_id = ?1",
            "UPDATE camera_pair SET held_out = 1 - held_out WHERE project_id = ?1",
        ),
    ] {
        let key = project.to_db();
        let control_ok = catalog
            .writer()
            .transact({
                let key = key.clone();
                move |tx| {
                    tx.execute(control, params![key])
                        .map_err(|err| aura_core::errors::db::statement_failed("control", &err))
                }
            })
            .is_ok();
        if !control_ok {
            eprintln!(
                "trigger {name}: inconclusive - the control statement did not reach the table"
            );
            failures += 1;
            continue;
        }
        let refused = catalog
            .writer()
            .transact(move |tx| {
                tx.execute(attempt, params![key])
                    .map_err(|err| aura_core::errors::db::statement_failed("attempt", &err))
            })
            .is_err();
        if refused {
            println!("trigger {name}: refused, with a control that succeeded");
        } else {
            eprintln!("trigger {name}: the statement it exists to refuse succeeded");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 11. What this gate does not prove.
    // ---------------------------------------------------------------------------------------
    println!("\n--- what phase 26 does not prove ---");
    println!(
        "C1  Every number above came from a synthetic wedding whose per-brand colour response was\n\
         \x20   authored. There are no multi-camera weddings in this repository. Sev 2."
    );
    println!(
        "C2  All {} bundled brand baselines were fabricated; any measured = {}. The fallback path\n\
         \x20   is proved to run and to report itself honestly, and nothing is proved about the\n\
         \x20   numbers it falls back on. The first measured baseline reopens these criteria. Sev 2.",
        library.len(),
        library.any_measured()
    );
    println!(
        "C3  Phase 25's SKIN_FIELD_AVAILABLE is {}, so no photograph in this build carries an\n\
         \x20   identity-scoped skin region. The skin term of the appearance distance is\n\
         \x20   unmeasured rather than met.",
        aura_brain_gallery::SKIN_FIELD_AVAILABLE
    );
    println!(
        "C4  Section 9's blind study - can a photographer pick out the second camera after\n\
         \x20   matching - did not happen. The phase's own headline acceptance criterion is\n\
         \x20   unmeasured.\n"
    );

    if failures == 0 {
        println!("phase 26: pass");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase 26: {failures} failure(s)");
        ExitCode::FAILURE
    }
}

/// A project row, so the foreign keys the five tables carry have something to point at.
fn seed_project(
    catalog: &Arc<Catalog>,
    project: ProjectId,
    clock: &Arc<dyn Clock>,
) -> aura_core::AuraResult<()> {
    let key = project.to_db();
    let now = aura_catalog::rfc3339(clock.now_utc());
    catalog.writer().transact(move |tx| {
        tx.execute(
            "INSERT INTO project (project_id, name, created_at, updated_at)
             VALUES (?1, 'phase 26 gate', ?2, ?2)",
            params![key, now],
        )
        .map_err(|err| aura_core::errors::db::statement_failed("project insert", &err))?;
        Ok(())
    })
}

/// Give the fixture frames a `photo` row each, so the pair table's foreign keys resolve.
///
/// `camera_pair.left_image` and `right_image` both reference `photo(photo_id)`, which is the
/// constraint that stops a stored pair naming two photographs the catalog has never heard of. The
/// gate's first run failed on it - the constraint working, and the same thing phase 25's gate found
/// about a skin correction naming an identity that did not exist.
fn seed_photos(
    catalog: &Arc<Catalog>,
    project: ProjectId,
    frames: &[CameraFrame],
    clock: &Arc<dyn Clock>,
) -> aura_core::AuraResult<()> {
    let key = project.to_db();
    let now = aura_catalog::rfc3339(clock.now_utc());
    let rows: Vec<(String, i64)> = frames
        .iter()
        .map(|frame| (frame.image.to_db(), frame.timeline_ms))
        .collect();
    catalog.writer().transact(move |tx| {
        for (photo, ms) in &rows {
            // A zero-padded ordinal, so a text sort is a time sort - the same form phase 25's own
            // gate and budget fixtures use.
            let stamp = format!("{:016}", (*ms).max(0));
            tx.execute(
                "INSERT OR IGNORE INTO photo (photo_id, project_id, capture_time, timeline_time,
                                              created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3, ?4, ?4)",
                params![photo, key, stamp, now],
            )
            .map_err(|err| aura_core::errors::db::statement_failed("photo insert", &err))?;
        }
        Ok(())
    })
}

/// Unused imports kept honest: the gate reads these two through the pass rather than directly, and
/// naming them here is what stops a future edit quietly dropping the dependency.
#[allow(dead_code)]
fn _modules_in_use() {
    let _ = transform::signature_distance;
    let _ = api::field_for;
    let _: Option<SceneId> = None;
}
