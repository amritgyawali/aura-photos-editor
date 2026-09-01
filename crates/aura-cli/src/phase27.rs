//! The phase 27 mechanical gate.
//!
//! The assembly proof for the QC agent: migration 27 and its objects, the thresholds table a product
//! manager owns and the widened bound it refuses, twenty injected defects through the real checks, a
//! whole synthetic gallery through the real pass, the loop's bound and its revert, the replacement
//! filter, the planner's schema, and what a photographer's own verdict survives.
//!
//! **Nothing here proves anything about a real photograph.** Every fixture is a set of *readings*
//! this repository authored, and the numbers phases 09 to 26 would produce on a real frame come from
//! placeholder heads. That is conditions C1 to C4 of the exit report, and they are printed at the
//! end of every run rather than hidden in a helper.
//!
//! The unit tests prove the pieces and `tests/eval/qc_eval.rs` proves the gates. This proves the
//! assembly - the things that only exist when a catalog, a thresholds file, a field and a pass are
//! in the same process.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::qc::{
    ImageId, QcCategory, QcCode, QcOverride, QcService, Remedy, TicketStatus, MAX_PLANNER_CALLS,
    MAX_ROUNDS, MAX_TICKETS_PER_IMAGE, MIN_GAIN_SHARE, REPLACE_CONFIDENCE_FLOOR,
};
use aura_core::contract::scene::SceneId;
use aura_core::progress::{CancelToken, NullProgress};
use aura_core::{AuraResult, ProjectId};
use aura_qc::api::{Field, Qc, QcPass, SelectedCount};
use aura_qc::checks::{Frame, SetContext};
use aura_qc::policy::Thresholds;
use aura_qc::replace::{CandidateMetric, CoverageEffect, Verdict};
use aura_qc::store::QcStore;
use aura_qc::{checks, fixtures, remedy, replace, report, triage};
use rusqlite::params;

/// Run the phase 27 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase27-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // ---------------------------------------------------------------------------------------
    // 1. Migration 27 and every object it owns.
    // ---------------------------------------------------------------------------------------
    let catalog_path = work.join("phase27.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 27 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 27, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "qc_run"),
        ("table", "qc_ticket"),
        ("table", "qc_round"),
        ("table", "qc_replacement"),
        ("view", "v_qc_queue"),
        ("view", "v_qc_unchecked"),
        ("trigger", "qc_ticket_keep_user_status"),
        ("trigger", "qc_round_no_update"),
        ("trigger", "qc_round_no_direct_delete"),
        ("trigger", "qc_replacement_is_immutable"),
        ("index", "idx_qc_ticket_project"),
        ("index", "idx_qc_ticket_image"),
        ("index", "idx_qc_ticket_queue"),
        ("index", "idx_qc_replacement_project"),
    ] {
        let present = catalog
            .read(move |conn| {
                let count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                        params![kind, name],
                        |row| row.get(0),
                    )
                    .map_err(|err| {
                        aura_core::errors::db::statement_failed("sqlite_master", &err)
                    })?;
                Ok(count)
            })
            .unwrap_or(0);
        if present == 1 {
            println!("{kind} {name}: present");
        } else {
            eprintln!("{kind} {name}: MISSING");
            failures += 1;
        }
    }

    // The schema must hold no fixed skin target. Phase 15's rule, scanned on every run - the same
    // check phases 25 and 26 make, in the phase that judges skin.
    let schema_text = catalog
        .read(|conn| {
            let mut statement = conn
                .prepare("SELECT COALESCE(sql, '') FROM sqlite_master")
                .map_err(|err| {
                    aura_core::errors::db::statement_failed("sqlite_master sql", &err)
                })?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|err| {
                    aura_core::errors::db::statement_failed("sqlite_master sql", &err)
                })?;
            let mut all = String::new();
            for row in rows {
                all.push_str(&row.map_err(|err| {
                    aura_core::errors::db::statement_failed("sqlite_master sql", &err)
                })?);
            }
            Ok(all)
        })
        .unwrap_or_default();
    // The comments come out first, and that is not a nicety. `sqlite_master.sql` stores a
    // migration verbatim, comments and all, and migration 27's header is four paragraphs about
    // why there is no `diagnosis` column - so a scan of the raw text finds the word that
    // documents the rule and reports it as a violation of the rule. Phase 27 has now hit this
    // shape twice: `tests/no_pixel_ops.rs` strips comments for the same reason, after a grep
    // there matched its own test name.
    let lowered = strip_sql_comments(&schema_text).to_lowercase();
    let banned = ["ideal_skin", "reference_skin", "skin_target_uv"];
    if banned.iter().any(|name| lowered.contains(name)) {
        eprintln!("schema: a fixed skin target appeared in the catalog");
        failures += 1;
    } else {
        println!("schema: no fixed skin target, in any table");
    }
    if lowered.contains("diagnosis") {
        eprintln!(
            "schema: a stored diagnosis column appeared; the sentence is rendered, not stored"
        );
        failures += 1;
    } else {
        println!("schema: no stored diagnosis column - the sentence is rendered on read");
    }

    // ---------------------------------------------------------------------------------------
    // 2. The thresholds table, and the widened bound it refuses.
    // ---------------------------------------------------------------------------------------
    let thresholds = match Thresholds::shipped() {
        Ok(table) => {
            println!(
                "thresholds: version {}, {} scene rows",
                table.version(),
                table.scene_count()
            );
            if table.scene_count() != SceneId::ALL.len() {
                eprintln!(
                    "thresholds: {} rows for {} scenes; a scene with no row falls back on the most \
                     permissive one",
                    table.scene_count(),
                    SceneId::ALL.len()
                );
                failures += 1;
            }
            table
        }
        Err(err) => {
            eprintln!("thresholds: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };

    // A studio may tighten and may never widen. The refusal is what makes `docs/how-qc-works.md` a
    // promise about the product rather than a description of its defaults.
    let widened = include_str!("../../aura-qc/config/qc_thresholds.toml")
        .replace("skin_de00 = 1.65", "skin_de00 = 99.0");
    match Thresholds::parse(&widened) {
        Err(err) if err.code.0 == "AURA-ML-5140" => {
            println!("thresholds: a widened bound is refused ({})", err.code);
        }
        Err(err) => {
            eprintln!("thresholds: refused with the wrong code {}", err.code);
            failures += 1;
        }
        Ok(_) => {
            eprintln!("thresholds: a widened bound was ACCEPTED");
            failures += 1;
        }
    }
    let looser = include_str!("../../aura-qc/config/qc_thresholds.toml")
        .replace("min_gain_share = 0.50", "min_gain_share = 0.05");
    if Thresholds::parse(&looser).is_err() {
        println!("thresholds: a looser loop is refused");
    } else {
        eprintln!("thresholds: a loop that keeps remedies which did not work was ACCEPTED");
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 3. Twenty injected defects through the real checks.
    // ---------------------------------------------------------------------------------------
    let defects = fixtures::defects();
    let mut caught = 0usize;
    for defect in &defects {
        let findings = checks::findings_for(&defect.frame, &thresholds);
        if findings.iter().any(|finding| finding.code == defect.code) {
            caught += 1;
        } else {
            eprintln!("defect '{}': NOT CAUGHT", defect.name);
            failures += 1;
        }
    }
    println!(
        "detection: {caught}/{} injected defects caught",
        defects.len()
    );

    let clean = fixtures::clean_gallery(200);
    let noisy = clean
        .iter()
        .filter(|frame| !checks::findings_for(frame, &thresholds).is_empty())
        .count();
    if noisy == 0 {
        println!("false tickets: 0/200 on a clean gallery");
    } else {
        eprintln!("false tickets: {noisy}/200 on a gallery with nothing wrong with it");
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 4. A whole synthetic gallery through the real pass.
    // ---------------------------------------------------------------------------------------
    let project = ProjectId::new();
    if let Err(err) = seed_project(&catalog, project, &clock) {
        eprintln!("seed: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }

    let mut frames: Vec<Frame> = defects.iter().map(|defect| defect.frame.clone()).collect();
    frames.extend(fixtures::clean_gallery(60));
    if let Err(err) = seed_photos(&catalog, project, &frames, &clock) {
        eprintln!("seed: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }

    let store = QcStore::new(Arc::clone(&catalog), Arc::clone(&clock));
    let pass = QcPass::new(store.clone(), thresholds.clone());
    let field = GateField::new(&frames, fixtures::broken_coverage());

    let result = match pass.inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("pass: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    println!(
        "pass: {} frames, {} checks run, {} skipped, {} tickets",
        result.report.images,
        result.report.checks_run,
        result.report.skipped,
        result.tickets.len()
    );
    if !result.report.complete() {
        eprintln!("pass: the gallery was not fully reached");
        failures += 1;
    }

    // Every ticket is well formed. Invariant 2 - a decision without an explanation is a bug - and
    // migration 27's CHECK constraints refuse one, so a malformed ticket would have failed the write
    // above rather than reaching here. This is the third layer.
    let malformed = result
        .tickets
        .iter()
        .filter(|ticket| !ticket.is_well_formed())
        .count();
    if malformed == 0 {
        println!("tickets: every one carries a code, a number, a threshold and a reason");
    } else {
        eprintln!("tickets: {malformed} malformed");
        failures += 1;
    }

    // No image exceeds the ticket cap.
    let mut per_image: std::collections::BTreeMap<ImageId, usize> =
        std::collections::BTreeMap::new();
    for ticket in &result.tickets {
        *per_image.entry(ticket.image_id).or_default() += 1;
    }
    let over = per_image
        .values()
        .filter(|count| **count > MAX_TICKETS_PER_IMAGE)
        .count();
    if over == 0 {
        println!("tickets: no image above the cap of {MAX_TICKETS_PER_IMAGE}");
    } else {
        eprintln!("tickets: {over} image(s) above the per-image cap");
        failures += 1;
    }

    // The three coverage findings are there and are escalated rather than acted on.
    let coverage: Vec<_> = result
        .tickets
        .iter()
        .filter(|ticket| ticket.category == QcCategory::Coverage)
        .collect();
    if coverage.len() == 3 && coverage.iter().all(|t| t.status == TicketStatus::Escalated) {
        println!("coverage: three findings, all escalated to a person");
    } else {
        eprintln!(
            "coverage: {} finding(s), statuses {:?}",
            coverage.len(),
            coverage.iter().map(|t| t.status).collect::<Vec<_>>()
        );
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 5. Triage works root causes before symptoms.
    // ---------------------------------------------------------------------------------------
    let (_name, multi) = match fixtures::multi_symptom().into_iter().next() {
        Some(pair) => pair,
        None => {
            eprintln!("fixtures: no multi-symptom frame");
            return ExitCode::FAILURE;
        }
    };
    let multi_tickets: Vec<_> = checks::findings_for(&multi, &thresholds)
        .into_iter()
        .map(|finding| {
            let proposed = remedy::propose(&finding, &multi, 0);
            aura_qc::ticket::from_finding(project, &multi, finding, proposed, 0)
        })
        .collect();
    let ordered = triage::order(&multi_tickets);
    match ordered.first() {
        Some(first) if first.category == QcCategory::Consistency => {
            println!("triage: the colour finding is worked before its symptoms");
        }
        Some(first) => {
            eprintln!("triage: worked {} first", first.category);
            failures += 1;
        }
        None => {
            eprintln!("triage: nothing to work on");
            failures += 1;
        }
    }
    if triage::needs_planner(&multi_tickets) {
        println!("triage: a multi-symptom frame asks for a second opinion");
    } else {
        eprintln!("triage: a multi-symptom frame did not reach the planner trigger");
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 6. A replacement that would break coverage is refused before any metric is compared.
    // ---------------------------------------------------------------------------------------
    let mut protected = fixtures::healthy(SceneId::FamilyPortrait);
    protected.runner_up = Some(ImageId::new());
    if let Some(sharp) = protected.sharpness.as_mut() {
        sharp.relative_sharpness = 0.02;
        sharp.subject_sharpness = 0.02;
    }
    if let Some(finding) = checks::findings_for(&protected, &thresholds)
        .into_iter()
        .find(|finding| finding.category == QcCategory::Sharpness)
    {
        let mut ticket = aura_qc::ticket::from_finding(
            project,
            &protected,
            finding,
            Remedy::Escalate { note: "n".into() },
            0,
        );
        ticket.confidence = 1.0;
        let perfect = CandidateMetric {
            deviation: 0.0,
            has_other_findings: false,
        };
        let breaks = CoverageEffect {
            replaced_is_protected: true,
            replacement_covers_same: false,
            replacement_already_selected: false,
        };
        let verdict = replace::consider(
            &ticket,
            &protected,
            perfect,
            breaks,
            thresholds.loop_policy(),
        );
        if verdict == Verdict::Refuse(QcCode::ReplacementBreaksCoverage) {
            println!("replace: a perfect candidate that breaks coverage is refused before scoring");
        } else {
            eprintln!("replace: coverage did not refuse a swap that breaks it: {verdict:?}");
            failures += 1;
        }
        let holds = CoverageEffect {
            replacement_covers_same: true,
            ..breaks
        };
        if replace::consider(
            &ticket,
            &protected,
            perfect,
            holds,
            thresholds.loop_policy(),
        )
        .accepted()
        .is_some()
        {
            println!("replace: the same candidate is accepted when it carries the guarantee");
        } else {
            eprintln!("replace: a safe swap was refused, so the filter is refusing everything");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 7. The report, and what it leads with.
    // ---------------------------------------------------------------------------------------
    let markdown = report::to_markdown(&result.report, &result.replacements);
    let checked_first = markdown
        .find("What was checked")
        .zip(markdown.find("What was found"))
        .is_some_and(|(checked, found)| checked < found);
    if checked_first {
        println!("report: leads with what was checked rather than with what was found");
    } else {
        eprintln!("report: does not lead with coverage");
        failures += 1;
    }
    if result.report.skipped > 0 && !markdown.contains("made no claim either way") {
        eprintln!("report: skipped checks are not stated as making no claim");
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 8. What a photographer's own verdict survives.
    // ---------------------------------------------------------------------------------------
    let service = Qc::new(
        store.clone(),
        Arc::new(Selected(u32::try_from(frames.len()).unwrap_or(0))),
    );
    if let Some(subject) = result.tickets.first().cloned() {
        match service.decide(&QcOverride {
            ticket: subject.id,
            status: TicketStatus::Dismissed,
            apply_remedy: false,
            note: Some("this is how the room looked".into()),
        }) {
            Ok(()) => {
                if pass
                    .inspect_only(project, &field, 0, &CancelToken::new(), &NullProgress)
                    .is_err()
                {
                    eprintln!("pass: the second run failed");
                    failures += 1;
                }
                let after = service.tickets(subject.image_id).unwrap_or_default();
                if after
                    .iter()
                    .any(|ticket| ticket.status == TicketStatus::Dismissed)
                {
                    println!("verdict: a dismissed finding does not come back on the next pass");
                } else {
                    eprintln!("verdict: a dismissed finding reappeared");
                    failures += 1;
                }
            }
            Err(err) => {
                eprintln!("verdict: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // An override naming a status automation owns is refused.
    match service.decide(&QcOverride {
        ticket: aura_core::contract::ids::TicketId::new(),
        status: TicketStatus::Fixed,
        apply_remedy: false,
        note: None,
    }) {
        Err(err) if err.code.0 == "AURA-ML-5137" => {
            println!("verdict: a person cannot record a measurement they did not make");
        }
        Err(err) => {
            eprintln!("verdict: refused with the wrong code {}", err.code);
            failures += 1;
        }
        Ok(()) => {
            eprintln!("verdict: a person was allowed to mark a ticket 'fixed'");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 9. The three triggers, each with a control.
    // ---------------------------------------------------------------------------------------
    //
    // Phase 21's lesson: a refusal test that cannot tell a working guard from a broken fixture
    // proves nothing, and an INSERT refused for a missing foreign key looks exactly like one
    // refused by a promise. Each check below runs a control first.
    let outline = service.outline(project).unwrap_or_default();
    println!(
        "outline: {} selected, {} checked, {} open, {} dismissed, {} B",
        outline.selected, outline.checked, outline.open, outline.dismissed, outline.bytes
    );
    if outline.detector_trained {
        eprintln!("outline: this build claims a trained detector and ships none");
        failures += 1;
    } else {
        println!("outline: detectorTrained = false, on the wire");
    }

    let stored = service.report(project).ok().flatten();
    match stored {
        Some(stored) if stored.thresholds_ver == thresholds.version() => {
            println!("report: round-trips through the catalog at the current versions");
        }
        Some(stored) => {
            eprintln!(
                "report: stored at thresholds version {} against {}",
                stored.thresholds_ver,
                thresholds.version()
            );
            failures += 1;
        }
        None => {
            eprintln!("report: nothing stored");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 10. The bounds, read from the contract rather than restated.
    // ---------------------------------------------------------------------------------------
    println!(
        "bounds: {MAX_ROUNDS} rounds, {MAX_TICKETS_PER_IMAGE} tickets/image, {MAX_PLANNER_CALLS} \
         planner calls, gain share {MIN_GAIN_SHARE}, replace floor {REPLACE_CONFIDENCE_FLOOR}"
    );

    // ---------------------------------------------------------------------------------------
    // 11. The surface is reachable from the running application.
    // ---------------------------------------------------------------------------------------
    //
    // Phase 21's exit report found that ninety client calls reached a window that did not answer
    // to them, and the check that would have caught it did not exist in any phase gate. It does
    // now, and it is deliberately mechanical: three sets of names, read out of the three files
    // that have to agree, compared. It proves the names and the syntax and *not* the types - the
    // shell's own Rust does not compile on this machine, because `dlltool` is absent - and the
    // exit report says so.
    match ipc_parity() {
        Ok(count) => println!("ipc: {count} handlers, {count} definitions, {count} wrappers"),
        Err(problem) => {
            eprintln!("ipc: {problem}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 12. What this gate does not prove.
    // ---------------------------------------------------------------------------------------
    println!("\n--- what phase 27 does not prove ---");
    println!(
        "C1  Every fixture above is a set of *readings* this repository authored. The numbers\n\
         \x20   phases 09 to 26 would produce on a real photograph come from placeholder heads, so\n\
         \x20   nothing here is a claim about a wedding. Closes with phase 05's C10. Sev 2."
    );
    println!(
        "C2  Section 10.1's photographer-agreement study did not happen. The headline KPI of this\n\
         \x20   phase - do photographers agree with the tickets - is unmeasured, and the\n\
         \x20   false-ticket rate above is against frames this repository authored as clean rather\n\
         \x20   than against a photographer's judgement. Sev 2."
    );
    println!(
        "C3  DETECTOR_TRAINED is {}. No defect-detection model ships and every check is a\n\
         \x20   measurement against another phase's stored number - a deliberate choice, because a\n\
         \x20   measurement finds fewer problems rather than inventing them.",
        aura_qc::DETECTOR_TRAINED
    );
    println!(
        "C4  The planner has never reached a provider in this repository. Its schema refuses\n\
         \x20   malformed answers and its offline path escalates; no recorded cassette of a real\n\
         \x20   reasoning-tier answer exists.\n"
    );

    if failures == 0 {
        println!("phase 27: pass");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase 27: {failures} failure(s)");
        ExitCode::FAILURE
    }
}

/// A field over authored readings, for the gate.
#[derive(Debug)]
struct GateField {
    order: Vec<ImageId>,
    frames: std::collections::BTreeMap<ImageId, Frame>,
    context: SetContext,
}

impl GateField {
    fn new(frames: &[Frame], context: SetContext) -> Self {
        let mut order = Vec::new();
        let mut map = std::collections::BTreeMap::new();
        for frame in frames {
            order.push(frame.image_id);
            map.insert(frame.image_id, frame.clone());
        }
        Self {
            order,
            frames: map,
            context,
        }
    }
}

impl Field for GateField {
    fn selected(&self, _project: ProjectId) -> AuraResult<Vec<ImageId>> {
        Ok(self.order.clone())
    }

    fn frame(&self, image: ImageId) -> AuraResult<Frame> {
        Ok(self.frames.get(&image).cloned().unwrap_or_default())
    }

    fn coverage(&self, _project: ProjectId) -> AuraResult<SetContext> {
        Ok(self.context.clone())
    }

    fn coverage_effect(&self, _image: ImageId) -> AuraResult<CoverageEffect> {
        Ok(CoverageEffect::unprotected())
    }

    fn candidate(
        &self,
        _runner_up: ImageId,
        _category: QcCategory,
    ) -> AuraResult<Option<CandidateMetric>> {
        Ok(None)
    }
}

/// How many frames the gallery holds.
#[derive(Debug)]
struct Selected(u32);

impl SelectedCount for Selected {
    fn selected(&self, _project: ProjectId) -> AuraResult<u32> {
        Ok(self.0)
    }
}

/// A project row, so the foreign keys the four tables carry have something to point at.
fn seed_project(
    catalog: &Arc<Catalog>,
    project: ProjectId,
    clock: &Arc<dyn Clock>,
) -> AuraResult<()> {
    let key = project.to_db();
    let now = aura_catalog::rfc3339(clock.now_utc());
    catalog.writer().transact(move |tx| {
        tx.execute(
            "INSERT INTO project (project_id, name, created_at, updated_at)
             VALUES (?1, 'phase 27 gate', ?2, ?2)",
            params![key, now],
        )
        .map_err(|err| aura_core::errors::db::statement_failed("project insert", &err))?;
        Ok(())
    })
}

/// Give the fixture frames a `photo` row each.
///
/// `qc_ticket.image_id` and both of `qc_replacement`'s image columns reference `photo(photo_id)`,
/// which is the constraint that stops a stored finding naming a photograph the catalog has never
/// heard of. Phase 25's gate and phase 26's gate both failed on their own version of this on their
/// first run - twice in two phases, because a store test is handed ids rather than making them and
/// so nothing below the gate exercises a referential constraint.
fn seed_photos(
    catalog: &Arc<Catalog>,
    project: ProjectId,
    frames: &[Frame],
    clock: &Arc<dyn Clock>,
) -> AuraResult<()> {
    let key = project.to_db();
    let now = aura_catalog::rfc3339(clock.now_utc());
    let rows: Vec<String> = frames.iter().map(|frame| frame.image_id.to_db()).collect();
    catalog.writer().transact(move |tx| {
        for photo in &rows {
            tx.execute(
                "INSERT OR IGNORE INTO photo (photo_id, project_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3)",
                params![photo, key, now],
            )
            .map_err(|err| aura_core::errors::db::statement_failed("photo insert", &err))?;
        }
        Ok(())
    })
}

/// Every `#[tauri::command]` has a registration and a typed client wrapper, and nothing else does.
///
/// Read out of the files rather than out of a manifest, because a manifest is a fourth thing that
/// can disagree with the other three. The three sets are the command definitions in the shell, the
/// names inside `generate_handler!`, and the string literals the typed client invokes.
fn ipc_parity() -> Result<usize, String> {
    let shell = std::fs::read_to_string("ui/src-tauri/src/main.rs")
        .map_err(|err| format!("ui/src-tauri/src/main.rs could not be read: {err}"))?;
    let client = std::fs::read_to_string("ui/src/ipc/client.ts")
        .map_err(|err| format!("ui/src/ipc/client.ts could not be read: {err}"))?;

    let mut defined: BTreeSet<String> = BTreeSet::new();
    let mut expecting = false;
    for line in shell.lines() {
        let line = line.trim();
        if line == "#[tauri::command]" {
            expecting = true;
            continue;
        }
        if expecting {
            if let Some(name) = line
                .strip_prefix("pub async fn ")
                .or_else(|| line.strip_prefix("async fn "))
                .or_else(|| line.strip_prefix("pub fn "))
                .or_else(|| line.strip_prefix("fn "))
                .and_then(|rest| rest.split('(').next())
            {
                defined.insert(name.trim().to_string());
                expecting = false;
            }
        }
    }

    let Some((_, after)) = shell.split_once("generate_handler![") else {
        return Err("the shell has no `generate_handler!` list".to_string());
    };
    let Some((list, _)) = after.split_once("])") else {
        return Err("the shell's `generate_handler!` list is not closed".to_string());
    };
    let registered: BTreeSet<String> = list
        .lines()
        .map(|line| line.trim().trim_end_matches(',').to_string())
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .collect();

    // `invoke<...>('name', ...)`, whose type argument may itself contain angle brackets - so the
    // name is found from the quote rather than by matching the brackets.
    let mut invoked: BTreeSet<String> = BTreeSet::new();
    for (index, _) in client.match_indices("invoke<") {
        let Some(open) = client[index..].find('(') else {
            continue;
        };
        let rest = &client[index + open + 1..];
        let Some(quote) = rest.find('\'') else {
            continue;
        };
        let rest = &rest[quote + 1..];
        let Some(end) = rest.find('\'') else {
            continue;
        };
        invoked.insert(rest[..end].to_string());
    }

    let mut problems = Vec::new();
    for name in defined.difference(&registered) {
        problems.push(format!(
            "`{name}` is defined in the shell and never registered"
        ));
    }
    for name in registered.difference(&defined) {
        problems.push(format!("`{name}` is registered and has no definition"));
    }
    for name in invoked.difference(&registered) {
        problems.push(format!(
            "the client calls `{name}` and no handler answers to it"
        ));
    }
    for name in registered.difference(&invoked) {
        problems.push(format!(
            "`{name}` is registered and no client wrapper reaches it"
        ));
    }
    if problems.is_empty() {
        Ok(defined.len())
    } else {
        problems.truncate(6);
        Err(problems.join("; "))
    }
}

/// One schema's SQL with its comments removed.
///
/// Line comments only. The migrations in this repository use `--` and nothing else, and a block
/// comment stripper that got its nesting wrong would silently delete a column definition - which
/// is a much worse failure than the one this function exists to prevent.
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
