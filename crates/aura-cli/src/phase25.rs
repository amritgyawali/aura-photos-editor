//! The phase 25 mechanical gate.
//!
//! The assembly proof for gallery consistency: migration 25 and its objects, the policy table a
//! product manager owns, the tree over a whole synthetic wedding, the change-point split that keeps
//! a candle-lit vow out of a bright ceremony, the solver's bounds and idempotence, the skin
//! arithmetic, the outlier queue, the store's protections and what a photographer's own decisions
//! survive.
//!
//! **Nothing here proves anything about a real photograph.** There are no weddings in this
//! repository and no labelled lighting transitions; every fixture below is a gallery whose drift
//! this file already knows the size of. That is conditions C1 to C3 of the exit report, and it is
//! printed at the end of every run rather than hidden in a helper.
//!
//! The unit tests prove the pieces and `tests/eval/consistency_eval.rs` proves the gates. This
//! proves the assembly - and it is the only place that checks the things that only exist when a
//! catalog, a policy file and a pass are in the same process.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_brain_gallery::api::{ConsistencyPass, Gallery};
use aura_brain_gallery::policy::Consistency;
use aura_brain_gallery::tree::Frame;
use aura_brain_gallery::{anchors, changepoint, fixtures, normalise, outlier, stats, tree};
use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::gallery::{
    Bound, GalleryCode, GalleryOverride, GalleryService, MAX_D_CCT_K, MIN_ANCHORS,
    SKIN_DE00_SPREAD_CEILING,
};
use aura_core::contract::ids::NodeId;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{AuraResult, ProjectId, SceneId, SegmentId};
use rusqlite::params;

/// Run the phase 25 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase25-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // ---------------------------------------------------------------------------------------
    // 1. Migration 25 and every object it owns.
    // ---------------------------------------------------------------------------------------
    let catalog_path = work.join("phase25.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 25 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 25, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "gallery_node"),
        ("table", "gallery_anchor"),
        ("table", "gallery_delta"),
        ("table", "gallery_skin_target"),
        ("table", "gallery_outlier"),
        ("view", "v_gallery_coverage"),
        ("view", "v_gallery_drift"),
        ("trigger", "gallery_outlier_needs_delta"),
        ("trigger", "gallery_anchor_pin_is_final"),
        ("trigger", "gallery_skin_target_needs_frames"),
        ("index", "idx_gallery_node_project"),
        ("index", "idx_gallery_node_segment"),
        ("index", "idx_gallery_anchor_photo"),
        ("index", "idx_gallery_delta_node"),
        ("index", "idx_gallery_delta_project"),
        ("index", "idx_gallery_skin_project"),
        ("index", "idx_gallery_outlier_queue"),
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
                eprintln!("migration 25: {kind} {name} is missing");
                failures += 1;
            }
            Err(err) => {
                eprintln!("migration 25: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }
    println!("migration 25: five tables, two views, three triggers, seven indexes");

    // ---------------------------------------------------------------------------------------
    // 2. The schema cannot express an absolute, and it has no ideal-skin constant.
    // ---------------------------------------------------------------------------------------
    //
    // Two scans, and the second is the one that matters. Section 6.3's fairness argument is that a
    // fixed skin target is how an editor lightens dark skin while believing it is correcting a
    // cast, and the defence is that nothing in the code path has a constant it could compare a
    // person against. The gate scans the schema for one on every run - phase 15's rule.
    let schema: String = match catalog.read(|conn| {
        let mut statement = conn
            .prepare("SELECT COALESCE(sql, '') FROM sqlite_master WHERE name LIKE 'gallery%'")
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
        "reference_skin",
        "target_skin",
        "skin_reference",
        "monk",
        "skin_bucket",
        "fitzpatrick",
    ] {
        if schema.to_lowercase().contains(banned) {
            eprintln!("schema: `{banned}` appears in migration 25; a skin target is measured");
            failures += 1;
        }
    }
    // And there is no absolute temperature on a delta - only a residual and the three `from_`
    // columns that say what it is a residual from.
    //
    // The scan is over **column names** rather than over the text, and that is a correction rather
    // than a nicety: the first version matched the substring `cct_k`, which is inside `from_cct_k`
    // - so it flagged the very column that makes a residual auditable. A scan that fires on the
    // thing it exists to protect is worse than no scan, because the fix somebody reaches for is to
    // delete the column.
    if schema.contains("gallery_delta") {
        let delta_sql = schema
            .split("CREATE TABLE gallery_delta")
            .nth(1)
            .unwrap_or_default()
            .split("CREATE ")
            .next()
            .unwrap_or_default()
            .to_string();
        let columns: Vec<String> = delta_sql
            .lines()
            .filter_map(|line| line.split_whitespace().next().map(str::to_string))
            .collect();
        for banned in ["temperature_k", "cct_k", "tint", "exposure_ev", "absolute"] {
            if columns.iter().any(|column| column == banned) {
                eprintln!(
                    "gallery_delta: a `{banned}` column would make this a second tone answer"
                );
                failures += 1;
            }
        }
        // The control: the residual and its origin *are* there, so a clean scan above means the
        // scan looked at something. Phase 21's lesson about refusal tests.
        for wanted in ["d_cct", "from_cct_k"] {
            if !columns.iter().any(|column| column == wanted) {
                eprintln!("gallery_delta: `{wanted}` is missing; the scan read nothing");
                failures += 1;
            }
        }
    }
    println!("schema: no ideal-skin constant, no absolute on a delta");

    // ---------------------------------------------------------------------------------------
    // 3. The policy table, and the one thing it may never do.
    // ---------------------------------------------------------------------------------------
    let policy = match Consistency::load(aura_brain_gallery::policy::BUNDLED) {
        Ok(table) => table,
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
    for bound in Bound::ALL {
        if policy.bound(bound) > bound.ceiling() {
            eprintln!("policy: {bound} is wider than the contract");
            failures += 1;
        }
    }
    let widened = format!(
        "version = 2\n[bounds]\nmax_d_cct_k = {}\n",
        MAX_D_CCT_K * 2.0
    );
    match Consistency::load(&widened) {
        Ok(_) => {
            eprintln!("policy: a file that widened a bound was accepted");
            failures += 1;
        }
        Err(err) if err.code.0 == "AURA-ML-5129" => {}
        Err(err) => {
            eprintln!(
                "policy: a widened bound raised {} rather than 5129",
                err.code
            );
            failures += 1;
        }
    }
    // The control: a *narrowed* bound is a studio's business and must be accepted, so a refusal
    // above is the rule rather than a loader that refuses everything. Phase 21's lesson.
    match Consistency::load("version = 3\n[bounds]\nmax_d_cct_k = 200.0\n") {
        Ok(_) => {}
        Err(err) => {
            eprintln!(
                "policy: a narrowed bound was refused ({}); the control failed",
                err.code
            );
            failures += 1;
        }
    }
    println!(
        "policy: {} scenes, every bound at or under the contract, a widened one refused",
        policy.len()
    );

    // ---------------------------------------------------------------------------------------
    // 4. A whole synthetic wedding, through the real pass.
    // ---------------------------------------------------------------------------------------
    let frames = fixtures::wedding();
    let project = ProjectId::new();
    if let Err(err) = seed(&catalog, project, &frames, &clock) {
        eprintln!("fixture: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }

    let pass = ConsistencyPass::new(Arc::clone(&catalog), Arc::clone(&clock));
    let report = match pass.run(project, &frames, None, &NullProgress, &CancelToken::new()) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("pass: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    println!(
        "pass: {} frames, {} nodes ({} anchored, {} split), {} outliers in {} ms",
        report.normalised,
        report.nodes,
        report.anchored,
        report.split,
        report.outliers,
        report.elapsed_ms
    );
    if report.normalised == 0 {
        eprintln!("pass: nothing was normalised on a fixture wedding");
        failures += 1;
    }
    if report.split == 0 {
        eprintln!("pass: the fixture wedding has a flash transition and nothing was split");
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 5. Idempotence, on the store rather than in the solver.
    // ---------------------------------------------------------------------------------------
    let gallery = Gallery::new(Arc::clone(&catalog), Arc::clone(&clock));
    let before: Vec<_> = frames
        .iter()
        .filter_map(|frame| gallery.delta(frame.image).ok().flatten())
        .collect();
    if let Err(err) = pass.run(project, &frames, None, &NullProgress, &CancelToken::new()) {
        eprintln!("second pass: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    let after: Vec<_> = frames
        .iter()
        .filter_map(|frame| gallery.delta(frame.image).ok().flatten())
        .collect();
    let mut moved = 0usize;
    for (a, b) in before.iter().zip(after.iter()) {
        if !a.agrees_with(b) {
            moved += 1;
        }
    }
    if moved > 0 || before.len() != after.len() {
        eprintln!("idempotence: {moved} frames moved again on a second pass");
        failures += 1;
    } else {
        println!(
            "idempotence: {} frames, none moved on a second pass",
            after.len()
        );
    }

    // ---------------------------------------------------------------------------------------
    // 6. Bounds, on every stored row.
    // ---------------------------------------------------------------------------------------
    let mut escaped = 0usize;
    for delta in &after {
        if !delta.within_bounds() {
            escaped += 1;
        }
    }
    if escaped > 0 {
        eprintln!("bounds: {escaped} stored deltas are outside the contract ceilings");
        failures += 1;
    } else {
        println!("bounds: every stored delta is inside its five ceilings");
    }

    // ---------------------------------------------------------------------------------------
    // 7. An intentional light is left alone, however far from its node it is.
    // ---------------------------------------------------------------------------------------
    let intentional: Vec<_> = frames
        .iter()
        .filter(|frame| frame.intentional_light)
        .collect();
    let mut touched = 0usize;
    for frame in &intentional {
        if let Ok(Some(delta)) = gallery.delta(frame.image) {
            if !delta.is_zero() {
                touched += 1;
            }
            if !delta
                .reasons
                .iter()
                .any(|reason| reason.code == GalleryCode::MoodPreserved)
            {
                touched += 1;
            }
        }
    }
    if touched > 0 {
        eprintln!("mood: {touched} intentionally-lit frames were normalised or unlabelled");
        failures += 1;
    } else {
        println!(
            "mood: all {} intentionally-lit frames left alone and labelled",
            intentional.len()
        );
    }

    // ---------------------------------------------------------------------------------------
    // 8. A photographer's pin survives a whole re-pass, and a wild override is refused.
    // ---------------------------------------------------------------------------------------
    let nodes = gallery.nodes(project).unwrap_or_default();
    if nodes.is_empty() {
        eprintln!("nodes: the pass wrote none");
        failures += 1;
    } else {
        let node = &nodes[0];
        let unchosen = node
            .image_ids
            .iter()
            .find(|image| !node.anchors.contains(image))
            .copied();
        if let Some(image) = unchosen {
            match gallery.pin_anchor(node.id, image) {
                Ok(()) => {
                    drop(pass.run(project, &frames, None, &NullProgress, &CancelToken::new()));
                    let survived = gallery
                        .nodes(project)
                        .unwrap_or_default()
                        .iter()
                        .any(|candidate| candidate.anchors.first() == Some(&image));
                    if survived {
                        println!("pins: a pinned anchor survived a whole re-pass");
                    } else {
                        eprintln!("pins: a pinned anchor did not survive a re-pass");
                        failures += 1;
                    }
                }
                Err(err) => {
                    eprintln!("pins: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            }
        }

        if let Some(image) = node.image_ids.first().copied() {
            let empty = gallery.set_override(image, GalleryOverride::default());
            let wild = gallery.set_override(
                image,
                GalleryOverride {
                    d_cct: Some(MAX_D_CCT_K * 3.0),
                    ..GalleryOverride::default()
                },
            );
            // The control first: a legal override must be accepted, so a refusal above is the rule
            // rather than a service that refuses everything.
            let legal = gallery.set_override(
                image,
                GalleryOverride {
                    d_cct: Some(-90.0),
                    ..GalleryOverride::default()
                },
            );
            if empty.is_err() && wild.is_err() && legal.is_ok() {
                println!("overrides: an empty one and a wild one refused, a legal one accepted");
            } else {
                eprintln!(
                    "overrides: empty {:?}, wild {:?}, legal {:?}",
                    empty.is_err(),
                    wild.is_err(),
                    legal.is_ok()
                );
                failures += 1;
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // 9. The two headline gates, on the fixture wedding.
    // ---------------------------------------------------------------------------------------
    let scene = SceneId::Ceremony;
    let segment = SegmentId::new();
    let drifting = fixtures::drifting_chapter(segment, scene, 60, 600.0);
    let (reduction, clamped) = measure_reduction(&drifting, scene, &policy);
    if reduction >= 0.60 {
        println!(
            "gate: warmth spread reduced {:.0} % ({clamped} clamped)",
            reduction * 100.0
        );
    } else {
        eprintln!(
            "gate: warmth spread reduced only {:.0} %",
            reduction * 100.0
        );
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 10. The transition that must not be flattened.
    // ---------------------------------------------------------------------------------------
    let transition = fixtures::transitioning_chapter(SegmentId::new(), scene, 24);
    let raw = tree::build(&transition);
    let split = raw
        .first()
        .map(|node| changepoint::split(node, policy.split_sigma))
        .unwrap_or_default();
    if split.len() >= 2 {
        println!(
            "transition: the flash split the node into {} parts",
            split.len()
        );
    } else {
        eprintln!("transition: a 2,400 K flash was not detected");
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 11. The skin arithmetic, on authored readings.
    // ---------------------------------------------------------------------------------------
    // Its own wedding, not the one above: a segment id belongs to exactly one chapter, and seeding
    // a second project against the same chapters is a primary-key collision rather than a test.
    let skin_frames = fixtures::wedding();
    let field = fixtures::AuthoredSkin::new(&skin_frames, [0.240, 0.500], 0.45, 0.030);
    let skin_project = ProjectId::new();
    // The identity has to exist before a correction can name it. `gallery_delta.skin_identity` is a
    // foreign key onto `identities`, which is what stops a stored correction from describing what
    // was done to somebody the catalog has never heard of - and the first version of this gate
    // discovered that by failing here, which is the constraint working.
    let seeded = seed(&catalog, skin_project, &skin_frames, &clock)
        .and_then(|()| seed_identity(&catalog, skin_project, &clock));
    if let Err(err) = seeded {
        eprintln!("skin fixture: [{}] {}", err.code, err.detail);
        failures += 1;
    } else {
        match pass.run(
            skin_project,
            &skin_frames,
            Some(&field),
            &NullProgress,
            &CancelToken::new(),
        ) {
            Ok(skin_report) => {
                let targets = gallery.skin_targets(skin_project).unwrap_or_default();
                let worst = targets
                    .iter()
                    .map(|target| target.spread_after)
                    .fold(0.0_f32, f32::max);
                if skin_report.skin_targets > 0 && worst <= SKIN_DE00_SPREAD_CEILING {
                    println!(
                        "skin: {} targets, worst spread {worst:.2} dE00 against a {SKIN_DE00_SPREAD_CEILING} ceiling",
                        skin_report.skin_targets
                    );
                } else {
                    eprintln!(
                        "skin: {} targets, worst spread {worst:.2} dE00",
                        skin_report.skin_targets
                    );
                    failures += 1;
                }
            }
            Err(err) => {
                eprintln!("skin pass: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // 12. An unanchored node is a different row from a consistent one.
    // ---------------------------------------------------------------------------------------
    let doubtful: Vec<Frame> = (0..20)
        .map(|i| {
            let mut frame = fixtures::frame_at(SegmentId::new(), i * 2_000, scene);
            frame.wb_conf = 0.15;
            frame
        })
        .collect();
    let node = tree::build(&doubtful);
    let anchored = node.first().map(|node| {
        anchors::select(
            node,
            policy.scene(scene),
            &BTreeSet::new(),
            &BTreeSet::new(),
            policy.target_anchors,
        )
    });
    match anchored {
        Some(result)
            if !result.is_anchored() && result.reasons.contains(&GalleryCode::NodeUnanchored) =>
        {
            println!("refusal: a node of doubtful frames is unanchored and says so");
        }
        _ => {
            eprintln!("refusal: a node of doubtful frames was anchored anyway");
            failures += 1;
        }
    }
    if MIN_ANCHORS != 3 {
        eprintln!("contract: MIN_ANCHORS moved to {MIN_ANCHORS}");
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 13. Outliers are quantified.
    // ---------------------------------------------------------------------------------------
    let stray = fixtures::chapter_with_a_stray(SegmentId::new(), SceneId::Speeches, 40);
    let found = detect_outliers(&stray, SceneId::Speeches, &policy);
    if found.len() == 1 && found[0].residual_cct.abs() > 100.0 {
        println!("outliers: one stray found, {}", found[0].describe());
    } else {
        eprintln!(
            "outliers: found {} rather than one authored stray",
            found.len()
        );
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // What this gate does not prove.
    // ---------------------------------------------------------------------------------------
    println!(
        "\nWhat phase 25's gate does NOT prove\n\
         -----------------------------------\n\
         * Every gallery above is synthetic. There are no weddings in this repository, so the\n\
           drift, the transitions and the skin wander were authored and read back. These are\n\
           measurements of algorithms, not of photographs. Exit report condition C1, Sev 2.\n\
         * SKIN_FIELD_AVAILABLE is {}. Phase 18's segmentation head is untrained, so no photograph\n\
           in this build has an identity-scoped skin region, and section 11 above ran on authored\n\
           readings. It is a measurement of the mechanism, not of five people. Condition C2.\n\
         * No photographer has looked at a before-and-after gallery from this build. Section 9's\n\
           QAIQ audit of five weddings did not happen, so the phase's own headline - that a wedding\n\
           reads as one coherent body of work - is unmeasured. Condition C3.\n\
         * The anchor ranking reads phase 15's white-balance confidence and phase 06's face\n\
           detector, both placeholder-backed. Closes with phase 05's condition C10.",
        aura_brain_gallery::SKIN_FIELD_AVAILABLE
    );

    if failures == 0 {
        println!("\nphase 25: OK");
        ExitCode::SUCCESS
    } else {
        eprintln!("\nphase 25: {failures} check(s) failed");
        ExitCode::FAILURE
    }
}

/// Insert the project, the chapters and the photographs a pass needs to write against.
///
/// Raw statements rather than `repo::create_project`, because the gate is checking migration 25
/// and not phase 01's ingest path - and because the fixture chapters have to carry the reason
/// strings phase 07's own CHECK requires before a confidence above zero is legal.
fn seed(
    catalog: &Arc<Catalog>,
    project: ProjectId,
    frames: &[Frame],
    clock: &Arc<dyn Clock>,
) -> AuraResult<()> {
    let now = aura_catalog::rfc3339(clock.now_utc());
    let key = project.to_db();
    let rows: Vec<(String, String)> = frames
        .iter()
        .map(|frame| (frame.image.to_db(), frame.segment.to_db()))
        .collect();
    let mut segments: Vec<String> = rows.iter().map(|(_, segment)| segment.clone()).collect();
    segments.sort();
    segments.dedup();

    catalog.writer().transact(move |conn| {
        conn.execute(
            "INSERT INTO project (project_id, name, created_at, updated_at)
             VALUES (?1, 'phase 25 fixture', ?2, ?2)",
            params![key, now],
        )
        .map_err(|err| aura_core::errors::db::statement_failed("project", &err))?;
        for (ordinal, segment) in segments.iter().enumerate() {
            conn.execute(
                "INSERT INTO segments (id, project_id, ordinal, chapter, start_ts, end_ts,
                                       dominant_scene, confidence, reasons, image_count,
                                       created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'ceremony', 0, 100000000, 'ceremony', 0.9, ?4, 0, ?5, ?5)",
                params![
                    segment,
                    key,
                    i64::try_from(ordinal).unwrap_or(0),
                    FIXTURE_REASONS,
                    now
                ],
            )
            .map_err(|err| aura_core::errors::db::statement_failed("segments", &err))?;
        }
        for (index, (photo, _)) in rows.iter().enumerate() {
            // A zero-padded ordinal, so a text sort is a time sort - which is what
            // `GalleryStore::deltas_in` orders a strip by.
            let stamp = format!("{:016}", index * 1_000);
            conn.execute(
                "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                    created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3, ?4, ?4)",
                params![photo, key, stamp, now],
            )
            .map_err(|err| aura_core::errors::db::statement_failed("photo", &err))?;
        }
        Ok(())
    })
}

/// Insert the one person every fixture frame contains.
///
/// A correction that named an identity the catalog does not have would be a stored statement about
/// what was done to nobody's skin, and `gallery_delta.skin_identity` is a foreign key precisely so
/// that cannot happen. Phase 06's own `identities` CHECK needs a reason beside any role confidence
/// above zero, so the fixture supplies one.
fn seed_identity(
    catalog: &Arc<Catalog>,
    project: ProjectId,
    clock: &Arc<dyn Clock>,
) -> AuraResult<()> {
    let now = aura_catalog::rfc3339(clock.now_utc());
    let key = project.to_db();
    let identity = fixtures::fixture_identity().to_db();
    catalog.writer().transact(move |conn| {
        conn.execute(
            "INSERT OR IGNORE INTO identities (id, project_id, role, role_confidence,
                                               role_reasons, created_at, updated_at)
             VALUES (?1, ?2, 'guest', 0.5, ?3, ?4, ?4)",
            params![identity, key, FIXTURE_REASONS, now],
        )
        .map_err(|err| aura_core::errors::db::statement_failed("identities", &err))?;
        Ok(())
    })
}

/// Phase 07's `segments` table refuses a confidence above zero with no reasons behind it -
/// invariant 2, as a CHECK. The fixture supplies one rather than lowering the confidence, because a
/// chapter at zero confidence is a different fixture from the one this gate wants.
const FIXTURE_REASONS: &str = "[\"phase 25 fixture\"]";

/// Solve one chapter as a single node and report how much of its spread came out.
fn measure_reduction(frames: &[Frame], scene: SceneId, policy: &Consistency) -> (f32, usize) {
    let scene_policy = policy.scene(scene);
    let Some(node) = tree::build(frames).into_iter().next() else {
        return (0.0, 0);
    };
    let anchored = anchors::select(
        &node,
        scene_policy,
        &BTreeSet::new(),
        &BTreeSet::new(),
        policy.target_anchors,
    );
    let Some(target) = anchored.target else {
        return (0.0, 0);
    };
    let confidence = anchors::target_confidence(&anchored);
    let before: Vec<f32> = frames.iter().filter_map(|frame| frame.cct_k).collect();
    let mut after = Vec::with_capacity(frames.len());
    let mut clamped = 0usize;
    let id = NodeId::new();
    for frame in frames {
        let solved = normalise::solve(frame, id, &target, scene_policy, policy, confidence);
        if solved.delta.bounded_by.is_some() {
            clamped += 1;
        }
        after.push(frame.cct_k.unwrap_or(0.0) + solved.delta.d_cct);
    }
    let before_spread = stats::mean_abs_deviation(&before);
    let after_spread = stats::mean_abs_deviation(&after);
    if before_spread <= f32::EPSILON {
        return (0.0, clamped);
    }
    (
        (1.0 - after_spread / before_spread).clamp(0.0, 1.0),
        clamped,
    )
}

/// Solve one chapter and return whatever would not come.
fn detect_outliers(
    frames: &[Frame],
    scene: SceneId,
    policy: &Consistency,
) -> Vec<aura_core::contract::gallery::Outlier> {
    let scene_policy = policy.scene(scene);
    let Some(node) = tree::build(frames).into_iter().next() else {
        return Vec::new();
    };
    let anchored = anchors::select(
        &node,
        scene_policy,
        &BTreeSet::new(),
        &BTreeSet::new(),
        policy.target_anchors,
    );
    let Some(target) = anchored.target else {
        return Vec::new();
    };
    let confidence = anchors::target_confidence(&anchored);
    let id = NodeId::new();
    let mut found: Vec<_> = frames
        .iter()
        .filter_map(|frame| {
            let solved = normalise::solve(frame, id, &target, scene_policy, policy, confidence);
            outlier::detect(&solved, id, policy, false)
        })
        .collect();
    outlier::rank(&mut found);
    found
}
