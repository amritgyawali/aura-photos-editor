//! The phase 31 mechanical gate.
//!
//! The assembly proof for matching a look: migration 31 and its objects, the two triggers that make
//! a promise a property of the database, the schema scanned for a stored sentence and for a skin
//! target, a real folder of reference photographs walked and measured, a look solved and refined
//! through the real renderer, the refusal a photographer meets when they ask for the one route this
//! build does not have, and the IPC surface's three files.
//!
//! **Nothing here proves anything about a real page.** Every reference photograph is a plate this
//! repository authored and every look on one was applied by an analytic transform in
//! `aura_look::fixtures`. There is no photographer, no consented archive of somebody else's work,
//! and no study of whether a person would recognise a page they admire in the result - so the
//! headline claim of the phase is **unmeasured**. Those are the conditions in the exit report, and
//! they are printed at the end of every run rather than hidden in a helper.
//!
//! The unit tests prove the pieces and `tests/eval/look_eval.rs` proves the gates. This proves the
//! assembly - the things that only exist when a catalog, a folder, a renderer and a store are in
//! the same process.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, rfc3339, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::look::{
    LookCode, MediaSource, ReferenceOrigin, MAX_REFERENCES, MIN_REFERENCES, REFERENCE_LONG_EDGE,
    USABLE_REFERENCES,
};
use aura_core::contract::style::{LightingBucket, StyleDelta};
use aura_core::ProjectId;
use aura_look::fixtures::{self, SyntheticLook};
use aura_look::store::LookStore;
use aura_look::{aggregate, solve, source};

/// Run the phase 31 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase31-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // ---------------------------------------------------------------------------------------
    // 1. Migration 31 and every object it owns.
    // ---------------------------------------------------------------------------------------
    let catalog_path = work.join("phase31.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 31 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 31, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    let expected_tables = [
        "look_profile",
        "look_bucket",
        "look_reference",
        "project_look",
        "look_match",
        "look_reason",
    ];
    let expected_views = ["look_catalogue", "look_match_effect"];
    let expected_triggers = [
        "look_match_no_update",
        "project_look_needs_a_match",
        "project_look_update_needs_a_match",
    ];
    match objects(&catalog) {
        Ok(found) => {
            let missing: Vec<&str> = expected_tables
                .iter()
                .chain(expected_views.iter())
                .chain(expected_triggers.iter())
                .filter(|name| !found.contains(**name))
                .copied()
                .collect();
            if missing.is_empty() {
                println!(
                    "migration 31: {} tables, {} views, {} triggers",
                    expected_tables.len(),
                    expected_views.len(),
                    expected_triggers.len()
                );
            } else {
                eprintln!("migration 31: missing {missing:?}");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("migration 31: {err}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 2. The schema carries no stored sentence and no skin target.
    //
    // Comments are stripped first. Phase 27 found this exact check matching its own
    // documentation twice, and migration 31 has six numbered paragraphs about why neither of
    // these columns exists - which name every word being scanned for.
    // ---------------------------------------------------------------------------------------
    let scanned: Vec<&str> = expected_tables
        .iter()
        .chain(expected_views.iter())
        .copied()
        .collect();
    match schema_text_for(&catalog, &scanned) {
        Ok(sql) => {
            let code = strip_sql_comments(&sql);
            let banned = [
                "diagnosis",
                "sentence",
                "narrative",
                "summary_text",
                "skin_target",
                "skin_hue",
                "preferred_skin",
                "ideal_skin",
            ];
            let found: Vec<&str> = banned
                .iter()
                .filter(|word| declares_column(&code, word))
                .copied()
                .collect();
            if found.is_empty() {
                println!(
                    "schema scan: no stored sentence and no skin target in {} objects",
                    scanned.len()
                );
            } else {
                eprintln!("schema scan: migration 31 declares {found:?}");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("schema scan: {err}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 3. The refusal a photographer meets when they ask for the route this build does not have.
    //
    // Checked before the folder walk, because that is the order `source::resolve` does it in and
    // the order matters: a refusal that arrived after a disk scan would be a refusal that had
    // already done the work.
    // ---------------------------------------------------------------------------------------
    match source::resolve("https://instagram.com/somebody", MediaSource::PublicUrl, None) {
        Ok(_) => {
            eprintln!("fetch refusal: this build claimed it can fetch a page");
            failures += 1;
        }
        Err(error) => {
            let text = format!("{} {}", error.detail, error.user_message);
            if text.contains("folder") {
                println!("fetch refusal: [{}] names the route that works", error.code);
            } else {
                eprintln!("fetch refusal: says no without saying what to do instead");
                failures += 1;
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // 4. A real folder of reference photographs, walked and measured.
    // ---------------------------------------------------------------------------------------
    let reference_dir = work.join("reference");
    drop(std::fs::remove_dir_all(&reference_dir));
    let wanted = SyntheticLook::light_and_airy();
    match fixtures::write_gallery(&reference_dir, 30, wanted, encode_jpeg) {
        Ok(written) => println!("reference: wrote {} photographs", written.len()),
        Err(err) => {
            eprintln!("reference: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    }

    let reference = match source::resolve(
        "https://instagram.com/the.photographer",
        MediaSource::Folder,
        Some(&reference_dir),
    ) {
        Ok(resolved) => resolved,
        Err(err) => {
            eprintln!("walk: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    if reference.origin
        == (ReferenceOrigin::Instagram {
            handle: "the.photographer".to_string(),
        })
    {
        println!("provenance: the page is recorded although the files came from a folder");
    } else {
        eprintln!("provenance: the page was lost: {:?}", reference.origin);
        failures += 1;
    }
    if reference
        .reasons
        .iter()
        .any(|reason| reason.code == LookCode::OriginRecordedNotFetched)
    {
        println!("provenance: and the report says nothing was fetched from it");
    } else {
        eprintln!("provenance: nothing said the page was not visited");
        failures += 1;
    }

    let readings: Vec<_> = reference
        .files
        .iter()
        .filter_map(|file| {
            source::decode(file)
                .ok()
                .map(|image| aura_look::measure::read(file.key.clone(), &image))
        })
        .collect();
    if readings.len() == reference.len() {
        println!("measure: read all {} reference photographs", readings.len());
    } else {
        eprintln!(
            "measure: read {} of {} reference photographs",
            readings.len(),
            reference.len()
        );
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 5. The look the fixture applied is the look that comes back out.
    //
    // A direction check rather than a magnitude one. The fixture's transform and the recipe's
    // parameters are not the same vocabulary - deliberately, see `fixtures.rs` - so what can be
    // asserted is that a brighter, lifted, warmer page asks for exposure, blacks and warmth.
    // ---------------------------------------------------------------------------------------
    let baseline_readings = fixtures::readings(30, SyntheticLook::neutral());
    let reference_aggregate = aggregate::fold(&aggregate::all(&readings));
    let baseline_aggregate = aggregate::fold(&aggregate::all(&baseline_readings));
    let delta = solve::initial(&reference_aggregate, &baseline_aggregate);

    let mut wrong = Vec::new();
    if delta.exposure <= 0.0 {
        wrong.push("exposure");
    }
    if delta.blacks <= 0.0 {
        wrong.push("blacks");
    }
    if delta.temperature_k <= 0.0 {
        wrong.push("temperature");
    }
    if wrong.is_empty() {
        println!(
            "solve: +{:.2} EV, +{:.0} K, blacks +{:.1} from a light and airy page",
            delta.exposure, delta.temperature_k, delta.blacks
        );
    } else {
        eprintln!("solve: the wrong direction on {wrong:?}: {delta:?}");
        failures += 1;
    }

    // Every bound held, including the one that is tighter than phase 17's.
    if delta.exposure.abs() <= aura_core::contract::look::MAX_EXPOSURE_DELTA_EV + 1e-4 {
        println!(
            "bounds: exposure inside {:.2} EV, which is below phase 17's {:.2}",
            aura_core::contract::look::MAX_EXPOSURE_DELTA_EV,
            aura_core::contract::style::MAX_EXPOSURE_DELTA_EV
        );
    } else {
        eprintln!("bounds: exposure left its bound at {}", delta.exposure);
        failures += 1;
    }

    // No hue was rotated, in any band. The structural half of this phase's skin defence.
    let rotated: Vec<&str> = aura_core::contract::colour::HslBand::ALL
        .into_iter()
        .filter(|band| delta.hsl.get(*band).h.abs() > 1e-6)
        .map(aura_core::contract::colour::HslBand::as_str)
        .collect();
    if rotated.is_empty() {
        println!("skin: no hue rotation in any of the eight bands, and no skin term at all");
    } else {
        eprintln!("skin: the solver rotated {rotated:?}");
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 6. The store, the triggers, and the control each refusal needs.
    //
    // Phase 21's rule: a refusal test that cannot tell a working guard from a broken fixture
    // proves nothing, so every forbidden statement is preceded by the allowed form of itself.
    // ---------------------------------------------------------------------------------------
    let project = ProjectId::new();
    let now = rfc3339(catalog.clock().now_utc());
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "Phase 31".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    if let Err(err) = catalog
        .writer()
        .transact(move |tx| repo::create_project(tx, &row))
    {
        eprintln!("project: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }

    let store = Arc::new(LookStore::new(Arc::clone(&catalog), Arc::clone(&clock)));
    let mut look = aura_core::contract::look::LookProfile::empty(
        aura_core::ProfileId::new(),
        "The page",
        aura_recipe::contract::recipe::ENGINE,
    );
    look.origin = reference.origin.clone();
    look.source = MediaSource::Folder;
    look.references = u32::try_from(readings.len()).unwrap_or(0);
    look.global = delta.clamped();
    look.buckets.insert(
        LightingBucket::Daylight,
        aura_core::contract::look::LookBucket {
            lighting: LightingBucket::Daylight,
            reference: reference_aggregate.clone(),
            baseline: baseline_aggregate.clone(),
            delta: StyleDelta::neutral(),
            confidence: 0.8,
            reasons: Vec::new(),
        },
    );
    if let Err(err) = store.put(&look, &readings) {
        eprintln!("store: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }
    match store.profile(look.id) {
        Ok(Some(read)) if read.buckets.len() == 1 && read.references == look.references => {
            println!("store: a look survives a round trip with its evidence");
        }
        Ok(_) => {
            eprintln!("store: the look did not come back the way it went in");
            failures += 1;
        }
        Err(err) => {
            eprintln!("store: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // `project_look_needs_a_match`: the control comes second here, because the refusal is the
    // state the database starts in.
    let unmeasured = store.select(project, Some(look.id));
    let report = aura_core::contract::look::LookMatchReport {
        profile: look.id,
        project,
        buckets: vec![aura_core::contract::look::BucketResidual {
            lighting: LightingBucket::Daylight,
            before_de00: 6.0,
            after_de00: 2.2,
            frames: 30,
        }],
        before_de00: 6.0,
        after_de00: 2.2,
        frames: 30,
        user_edited: 0,
        reasons: Vec::new(),
    };
    let recorded = store.put_match(&report, &look.engine_ver);
    let measured = store.select(project, Some(look.id));
    match (unmeasured, recorded, measured) {
        (Err(_), Ok(()), Ok(())) => {
            println!("trigger: an unmeasured look is refused and a measured one is accepted");
        }
        (Ok(()), _, _) => {
            eprintln!("trigger: a look nobody had measured was selected");
            failures += 1;
        }
        (_, Err(err), _) | (_, _, Err(err)) => {
            eprintln!(
                "trigger: INCONCLUSIVE - the control failed too: [{}] {}",
                err.code, err.detail
            );
            failures += 1;
        }
    }

    // `look_match_no_update`: the control is a second INSERT, which must succeed.
    let second = store.put_match(&report, &look.engine_ver);
    let rewritten = catalog.writer().transact(|conn| {
        conn.execute("UPDATE look_match SET after_de00 = 0.0", [])
            .map_err(|err| aura_core::errors::db::statement_failed("update look_match", &err))?;
        Ok(())
    });
    match (second, rewritten) {
        (Ok(()), Err(_)) => println!("trigger: a measured match can be added and never edited"),
        (Ok(()), Ok(())) => {
            eprintln!("trigger: a match record was rewritten");
            failures += 1;
        }
        (Err(err), _) => {
            eprintln!(
                "trigger: INCONCLUSIVE - the control failed: [{}] {}",
                err.code, err.detail
            );
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 7. The IPC surface is NOT checked here, and that is deliberate.
    //
    // `scripts/check-ipc-surface.sh` owns it. Phase 27 put the check inside its own gate and
    // phase 30 lifted it out, for the reason that comment gives: a check that only runs inside
    // one phase's gate is a check that stops running the day that phase is finished. Copying it
    // back in here would be a second implementation of a comparison that has to have one answer.
    // ---------------------------------------------------------------------------------------
    println!("ipc surface: checked by scripts/check-ipc-surface.sh, not by this gate");

    // ---------------------------------------------------------------------------------------
    // What this run did NOT prove.
    //
    // Printed every time rather than left in a document nobody opens. Phase 28 started this and
    // it is the most important eight lines in the file.
    // ---------------------------------------------------------------------------------------
    println!();
    println!("conditions this gate does not close:");
    println!("  C1  Every reference photograph above is a plate this repository authored and");
    println!("      every look on one was applied by an analytic transform. No page has been");
    println!("      measured. Sev 2.");
    println!("  C2  No photographer has been shown a matched gallery beside the page it was");
    println!("      matched to, so 'it looks like that account' is UNMEASURED. Sev 2, and it is");
    println!("      the headline claim of the phase.");
    println!("  C3  Nothing can be fetched. `MediaSource::PublicUrl` refuses on every call, for");
    println!("      two separate reasons - this repository allows no socket outside the cloud");
    println!("      gateway and that transport has no TLS, and a page's media is the platform's");
    println!("      to grant. ADR-0063 section 4.");
    println!("  C4  The baseline is only as good as phases 15 and 16, and every head underneath");
    println!("      them is a placeholder. A look is a residual from a decision made by");
    println!("      untrained models. Closes with phase 05's C10.");
    println!("  C5  The scale constants in `solve::initial` are AUTHORED. The refinement measures");
    println!("      its way off them through the real renderer, so what ships is measured - but");
    println!("      a project with no analysed frames gets the authored answer, labelled.");
    println!("  C6  No scene axis. A reference photograph does not say what it is of, so a look");
    println!("      is about light and not about subject, and every scene group gets the same");
    println!("      answer. `LookCode::SceneAxisNotLearned` is on the wire.");
    println!(
        "  bounds: {MIN_REFERENCES} to {MAX_REFERENCES} photographs, {USABLE_REFERENCES} before \
         a look is strong, measured at {REFERENCE_LONG_EDGE} px"
    );

    if failures == 0 {
        println!();
        println!("phase 31: all checks passed");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("phase 31: {failures} check(s) failed");
        ExitCode::FAILURE
    }
}

/// Encode one fixture plate as a JPEG, through the writer the product delivers with.
///
/// Phase 30's encoder rather than a second one, and at quality 95 rather than 100: a reference
/// photograph from a page has been through a lossy step, and a fixture that skipped it would be
/// measuring a cleaner image than the feature ever sees.
fn encode_jpeg(image: &aura_raw::codec::Rgb8) -> aura_core::AuraResult<Vec<u8>> {
    let rendered = aura_export::read::Rendered {
        width: image.width,
        height: image.height,
        data: aura_export::read::Samples::Eight(image.data.clone()),
        colour: aura_core::contract::delivery::DeliveryColour::Srgb,
        render_hash: String::new(),
    };
    let policy = aura_core::contract::delivery::MetadataPolicy::default();
    aura_export::jpeg::encode(&rendered, 95, &policy).map(|(bytes, _)| bytes)
}

/// Every object in the catalog, by name.
fn objects(catalog: &Arc<Catalog>) -> Result<BTreeSet<String>, String> {
    catalog
        .read(|conn| {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master")
                .map_err(|e| aura_core::errors::db::statement_failed("sqlite_master", &e))?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| aura_core::errors::db::statement_failed("sqlite_master", &e))?;
            Ok(rows.flatten().collect())
        })
        .map_err(|e| format!("[{}] {}", e.code, e.detail))
}

/// The SQL of the named objects only.
fn schema_text_for(catalog: &Arc<Catalog>, names: &[&str]) -> Result<String, String> {
    let wanted: Vec<String> = names.iter().map(|n| (*n).to_owned()).collect();
    catalog
        .read(move |conn| {
            let mut out = String::new();
            for name in &wanted {
                if let Ok(sql) = conn.query_row(
                    "SELECT COALESCE(sql, '') FROM sqlite_master WHERE name = ?1",
                    rusqlite::params![name],
                    |row| row.get::<_, String>(0),
                ) {
                    out.push_str(&sql);
                    out.push('\n');
                }
            }
            Ok(out)
        })
        .map_err(|e| format!("[{}] {}", e.code, e.detail))
}

/// Whether the SQL declares a column whose *whole name* is the banned word.
fn declares_column(sql: &str, word: &str) -> bool {
    for at in sql.match_indices(word).map(|(i, _)| i) {
        let before = sql[..at].chars().next_back();
        let after = sql[at + word.len()..].chars().next();
        let bounded_before = before.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        let bounded_after = after.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        if bounded_before && bounded_after {
            return true;
        }
    }
    false
}

/// One schema's SQL with its comments removed.
fn strip_sql_comments(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    for line in sql.lines() {
        let code = match line.find("--") {
            Some(index) => &line[..index],
            None => line,
        };
        out.push_str(code);
        out.push('\n');
    }
    out
}
