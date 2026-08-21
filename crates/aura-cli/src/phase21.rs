//! The phase 21 mechanical gate.
//!
//! This is the assembly proof for the micro-retouch suite: migration 21 and its objects, the
//! opt-in matrix and the bound the code owns rather than the file, the four measured detectors,
//! the naturalness guard and its per-family withdrawal, the borrow rule and its five disclosures,
//! the store, and the two promises the database keeps rather than the application - a composite
//! that cannot lose its source, and an opt-in operation that cannot arrive by default.
//!
//! **Nothing here proves a correction looks natural.** Section 10.1's headline row is four
//! hundred frames judged by retouchers and there is no such audit in this repository - condition
//! C2 of the exit report. Every number below is measured against synthetic frames whose strands,
//! sheets, marks and teeth were painted in, through regions this phase does not own. The
//! distinction is printed at the end of every run rather than hidden in a test helper.
//!
//! The tests prove the pieces; this proves the assembly. `tests/eval/micro_eval.rs` is the other
//! half and runs under `cargo test`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::micro::{
    MicroCode, MicroOp, MicroOverride, MicroService, NaturalnessGuard, CATCHLIGHT_FLOOR,
    HAIR_ENERGY_FLOOR, MIN_ALIGNMENT, MIN_SPECULAR_FRACTION,
};
use aura_core::{AuraResult, IdentityId, PhotoId, ProjectId};
use aura_retouch::micro::matrix::MicroTable;
use aura_retouch::micro::ops::{
    to_linear, upsample, Analyser, FLYAWAY_HEAD_TRAINED, GLARE_HEAD_TRAINED, LINT_HEAD_TRAINED,
};
use aura_retouch::micro::store::{MicroStore, BYTES_PER_IMAGE};
use aura_retouch::micro::{borrow, fixtures, glare, Micro};
use rusqlite::params;

/// Run the phase 21 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase21-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. Migration 21 and every object it owns.
    let catalog_path = work.join("phase21.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 21 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 21, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "micro_plan"),
        ("table", "micro_matrix"),
        ("table", "micro_op"),
        ("view", "v_micro_coverage"),
        ("view", "v_micro_composites"),
        ("index", "idx_micro_review"),
        ("index", "idx_micro_versions"),
        ("index", "idx_micro_borrows"),
        ("index", "idx_micro_guarded"),
        ("index", "idx_micro_op_borrow"),
        ("trigger", "micro_op_borrow_disclosed"),
        ("trigger", "micro_op_no_opt_in_by_default"),
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

    // 2. There is nowhere in this schema to reshape anybody, or to swap a face.
    //
    // Section 11 of `docs/plan/CLAUDE.md` forbids body reshaping, skin lightening and face
    // swapping permanently, and the way a phase quietly acquires one is by growing a column for
    // it. This phase is the one where it would be easiest: it already composites two photographs.
    match forbidden_columns(&catalog) {
        Ok(found) if found.is_empty() => {
            println!(
                "  no reshaping, enlarging, swapping or skin-tone-target column in migration 21"
            );
        }
        Ok(found) => {
            eprintln!(
                "  migration 21 grew a forbidden column: {}",
                found.join(", ")
            );
            failures += 1;
        }
        Err(err) => {
            eprintln!("  column scan: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 3. The matrix file, and the ceilings the code owns rather than the file.
    println!();
    println!("the opt-in matrix:");
    let table = match MicroTable::embedded() {
        Ok(table) => {
            println!("  loaded at version {}", table.version());
            Some(table)
        }
        Err(err) => {
            eprintln!("  matrix: [{}] {}", err.code, err.detail);
            failures += 1;
            None
        }
    };
    if let Some(table) = table.as_ref() {
        let unlisted = table.unlisted();
        if unlisted.is_empty() {
            println!("  every scene has a row");
        } else {
            println!(
                "  {} scenes fall back to the neutral row: {}",
                unlisted.len(),
                unlisted.join(", ")
            );
        }
        let guard = table.guard();
        match guard.problem() {
            None => println!("  the file's ceilings are inside the contract's"),
            Some(problem) => {
                eprintln!("  the file raised a ceiling: {problem}");
                failures += 1;
            }
        }
        // The bound is the code's, not the file's. A file that raised one is refused; this is the
        // same attempt, one layer up, so a loader that started clamping instead of refusing
        // fails here.
        let loose = NaturalnessGuard {
            teeth_max_luma: guard.teeth_max_luma + 0.10,
            ..guard
        };
        if loose.problem().is_some() {
            println!("  a table that raised the teeth ceiling would be refused");
        } else {
            eprintln!("  a table that raised the teeth ceiling would be accepted");
            failures += 1;
        }
    }

    // 4. The four measured detectors, on frames whose answer is painted in.
    println!();
    println!("the detectors, on synthetic frames:");
    let analyser = match Analyser::new() {
        Ok(analyser) => analyser,
        Err(err) => {
            eprintln!("  analyser: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };

    let (image, pixels, context) = fixtures::flyaway_frame(false);
    match analyser.analyse(image, &pixels, &context) {
        Ok(outcome) => {
            let calmed = outcome
                .plan
                .ops
                .iter()
                .any(|op| matches!(op, MicroOp::Flyaway { .. }));
            if calmed {
                println!("  a strand over a quiet background is calmed");
            } else {
                eprintln!("  a strand over a quiet background was not calmed");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("  flyaway: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    let (image, pixels, context) = fixtures::flyaway_frame(true);
    match analyser.analyse(image, &pixels, &context) {
        Ok(outcome) => {
            let calmed = outcome
                .plan
                .ops
                .iter()
                .any(|op| matches!(op, MicroOp::Flyaway { .. }));
            if calmed {
                eprintln!("  a strand over a BUSY background was calmed, which is a guess");
                failures += 1;
            } else {
                println!("  the same strand over a busy background is refused");
            }
        }
        Err(err) => {
            eprintln!("  flyaway (busy): [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // The lint detector, one level down, on a lapel the fixture paints.
    let (image, pixels, context) = fixtures::planned_frame();
    match analyser.analyse(image, &pixels, &context) {
        Ok(outcome) => {
            let counts = (
                outcome
                    .plan
                    .ops
                    .iter()
                    .filter(|op| matches!(op, MicroOp::Clothing { .. }))
                    .count(),
                outcome
                    .plan
                    .ops
                    .iter()
                    .filter(|op| matches!(op, MicroOp::Teeth { .. }))
                    .count(),
                outcome
                    .plan
                    .ops
                    .iter()
                    .filter(|op| matches!(op, MicroOp::Eyes { .. }))
                    .count(),
            );
            println!(
                "  the end-to-end frame produced {} operations: {} clothing, {} teeth, {} eyes",
                outcome.plan.ops.len(),
                counts.0,
                counts.1,
                counts.2
            );
            if outcome.plan.ops.is_empty() {
                eprintln!("  the end-to-end frame produced nothing, so nothing below is measured");
                failures += 1;
            }
            if outcome.plan.reasons.is_empty() {
                eprintln!("  a plan with no reasons: invariant 2");
                failures += 1;
            }
            if !outcome
                .plan
                .reasons
                .iter()
                .any(|reason| reason.code == MicroCode::HeadUntrained)
            {
                eprintln!("  the plan did not say its heads are untrained");
                failures += 1;
            }
            let report = outcome.plan.naturalness;
            println!(
                "  the guard measured catchlight {:.4} (floor {CATCHLIGHT_FLOOR:.2}), hair energy \
                 {:.4} (floor {HAIR_ENERGY_FLOOR:.2}) over {} samples",
                report.catchlight_ratio, report.hair_energy_ratio, report.measured_on
            );
            if !report.passed() {
                eprintln!("  the guard did not pass on the end-to-end frame: {report:?}");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("  end-to-end: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 5. The borrow rule: aligned and small, or refused with a reason.
    println!();
    println!("cross-frame borrowing:");
    let (image, pixels, context) = fixtures::glare_frame();
    let target = to_linear(&pixels);
    match (target, context.siblings.first(), context.faces.first()) {
        (Some(target), Some(sibling), Some(face)) => {
            let eyes = context
                .regions
                .get(&aura_core::contract::micro::MicroRegion::Eyes)
                .map(|field| upsample(field, target.width, target.height))
                .unwrap_or_default();
            let sheets = glare::detect(&target, &eyes, &context.faces);
            match sheets.first() {
                None => {
                    eprintln!("  the painted sheet was not detected");
                    failures += 1;
                }
                Some(sheet) => {
                    println!(
                        "  the sheet is {:.3} clipped against a floor of {MIN_SPECULAR_FRACTION:.2}",
                        sheet.clipped_fraction
                    );
                    if !sheet.may_borrow() {
                        eprintln!("  a fully destroyed sheet was not offered for borrowing");
                        failures += 1;
                    }
                    match to_linear(&sibling.pixels).zip(sibling.faces.first()) {
                        None => {
                            eprintln!("  the sibling frame carries no pixels or no face");
                            failures += 1;
                        }
                        Some((frame, sibling_face)) => {
                            let candidates = [borrow::SiblingFrame {
                                image: sibling.image,
                                frame,
                                face: *sibling_face,
                            }];
                            match borrow::choose(&target, sheet.region, face, &candidates) {
                                Ok(candidate) => {
                                    println!(
                                        "  the sibling aligned at {:.3} against a floor of \
                                         {MIN_ALIGNMENT:.2}",
                                        candidate.alignment
                                    );
                                    if candidate.alignment < MIN_ALIGNMENT {
                                        eprintln!("  a borrow was chosen below the floor");
                                        failures += 1;
                                    }
                                }
                                Err(refusal) => {
                                    eprintln!("  the aligned sibling was refused: {refusal:?}");
                                    failures += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {
            eprintln!("  the glare fixture is incomplete");
            failures += 1;
        }
    }

    // The same fixture end to end, because the alignment search passing is not the same claim as
    // the pass choosing to use it. Either outcome is legitimate and exactly one of them has to be
    // on the plan: a borrow that happened names its source, and a borrow that did not says why.
    match analyser.analyse(image, &pixels, &context) {
        Ok(outcome) => {
            let sources = outcome.plan.borrowed_from();
            if outcome.plan.is_composite() {
                println!(
                    "  end to end, the pass borrowed and disclosed {} source(s)",
                    sources.len()
                );
                if sources.is_empty() {
                    eprintln!("  a composite plan named no source");
                    failures += 1;
                }
                if !outcome
                    .plan
                    .reasons
                    .iter()
                    .any(|reason| reason.code == MicroCode::BorrowedFromSibling)
                {
                    eprintln!("  a composite plan carried no borrow reason");
                    failures += 1;
                }
            } else {
                let why: Vec<&str> = outcome
                    .plan
                    .reasons
                    .iter()
                    .map(|reason| reason.code.as_str())
                    .collect();
                println!("  end to end, the pass did not borrow: {}", why.join(", "));
                if !sources.is_empty() {
                    eprintln!("  a plan that is not a composite named a source anyway");
                    failures += 1;
                }
            }
        }
        Err(err) => {
            eprintln!("  glare end to end: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // A frame whose record survived is never borrowed into. The rule that separates a glare
    // repair from the eye swap section 2.2 forbids.
    match to_linear(&pixels) {
        None => {
            eprintln!("  the glare fixture carries no pixels");
            failures += 1;
        }
        Some(mut informative) => {
            for value in &mut informative.rgb {
                *value = value.min(0.80);
            }
            let eyes = context
                .regions
                .get(&aura_core::contract::micro::MicroRegion::Eyes)
                .map(|field| upsample(field, informative.width, informative.height))
                .unwrap_or_default();
            let offered = glare::detect(&informative, &eyes, &context.faces)
                .iter()
                .any(glare::Sheet::may_borrow);
            if offered {
                eprintln!("  a region that still carries information was offered for borrowing");
                failures += 1;
            } else {
                println!("  a bright sheen that still carries information is never borrowed into");
            }
        }
    }

    // 6. The store, and the two promises the database keeps rather than the application.
    println!();
    println!("the store:");
    let project = ProjectId::new();
    let photo = PhotoId::new();
    let source = PhotoId::new();
    let identity = IdentityId::new();
    if let Err(err) = seed(&catalog, &project, &[photo, source], &[identity]) {
        eprintln!("  seed: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }
    let store = Arc::new(MicroStore::new(Arc::clone(&catalog), Arc::clone(&clock)));
    let service = match Micro::new(Arc::clone(&store)) {
        Ok(service) => service,
        Err(err) => {
            eprintln!("  service: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };

    let (_, pixels, context) = fixtures::planned_frame();
    match analyser.analyse(photo, &pixels, &context) {
        Ok(outcome) => {
            let mut plan = outcome.plan;
            plan.image_id = photo;
            // The fixture's identity is not in this catalog. Every operation that names a person
            // names this one instead, which is what a real pass would store.
            for op in &mut plan.ops {
                match op {
                    MicroOp::Teeth { identity: who, .. } | MicroOp::Eyes { identity: who, .. } => {
                        *who = identity;
                    }
                    _ => {}
                }
            }
            match store.put(&project, &plan) {
                Ok(()) => println!("  one plan stored"),
                Err(err) => {
                    eprintln!("  put: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            }
            match service.of_image(photo) {
                Ok(Some(read)) => {
                    if read.ops.len() == plan.ops.len() && read.reasons.len() == plan.reasons.len()
                    {
                        println!("  it reads back with all {} operations", read.ops.len());
                    } else {
                        eprintln!(
                            "  it read back with {} operations and {} reasons, not {} and {}",
                            read.ops.len(),
                            read.reasons.len(),
                            plan.ops.len(),
                            plan.reasons.len()
                        );
                        failures += 1;
                    }
                }
                Ok(None) => {
                    eprintln!("  the stored plan did not read back");
                    failures += 1;
                }
                Err(err) => {
                    eprintln!("  of_image: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            }
        }
        Err(err) => {
            eprintln!("  analyse: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // A photographer's matrix survives, and setting nothing is refused rather than stored.
    let override_values = MicroOverride {
        allowed: None,
        clothing: None,
        borrowing: Some(false),
    };
    match service.set_matrix(project, override_values) {
        Ok(()) => match service.matrix(project) {
            Ok(read) => {
                if read.borrowing == Some(false) {
                    println!("  a studio that switched borrowing off stays switched off");
                } else {
                    eprintln!("  the borrowing switch did not survive: {read:?}");
                    failures += 1;
                }
            }
            Err(err) => {
                eprintln!("  matrix: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        },
        Err(err) => {
            eprintln!("  set_matrix: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    let empty = MicroOverride {
        allowed: None,
        clothing: None,
        borrowing: None,
    };
    if service.set_matrix(project, empty).is_err() {
        println!("  an override that sets nothing is refused");
    } else {
        eprintln!("  an override that sets nothing was stored");
        failures += 1;
    }

    // The trigger, not the application: a borrow may never lose its source.
    match strip_borrow_source(&catalog, &photo, &source) {
        Ok(Attempt::Allowed) => {
            eprintln!("  a stored borrow was stripped of its source by a direct UPDATE");
            failures += 1;
        }
        Ok(Attempt::Refused) => {
            println!("  a borrow cannot be stripped of its source, even by a direct UPDATE");
        }
        Ok(Attempt::Inconclusive(why)) => {
            eprintln!("  the borrow trigger was not exercised: {why}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("  borrow trigger: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // The second trigger: an opt-in operation cannot arrive by default.
    match insert_crease_directly(&catalog, &photo) {
        Ok(Attempt::Allowed) => {
            eprintln!("  a crease removal was inserted into a project that never asked for one");
            failures += 1;
        }
        Ok(Attempt::Refused) => {
            println!("  a crease removal cannot be inserted where it is switched off");
        }
        Ok(Attempt::Inconclusive(why)) => {
            eprintln!("  the opt-in trigger was not exercised: {why}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("  opt-in trigger: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 7. The outline the panel reads.
    match service.outline(project) {
        Ok(outline) => {
            println!();
            println!(
                "outline: {} of {} photographs planned, {} acted on, {} borrows",
                outline.planned, outline.photos, outline.acted_on, outline.borrows
            );
            if outline.coverage < 0.0 || outline.coverage > 1.0 {
                eprintln!("  coverage outside 0..1: {}", outline.coverage);
                failures += 1;
            }
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
        "  the heads are untrained (flyaway {FLYAWAY_HEAD_TRAINED}, glare {GLARE_HEAD_TRAINED}, \
         lint {LINT_HEAD_TRAINED}); what ran is the measured detection"
    );
    println!("  every frame above was painted by the fixture generator, not photographed");
    println!("  no region reached the pass from phase 18, so a real frame is not micro-retouched");
    println!("  no naturalness audit has been run, and no 100 % zoom artefact audit either");
    println!("  storage budget: {BYTES_PER_IMAGE} B per image, asserted by aura-perf");
    println!(
        "  the panel is not reachable from the running application: `ui/src/ipc/client.ts` has \
         no wrappers and `App.tsx` mounts no develop panel - exit report condition C6"
    );

    if failures == 0 {
        println!();
        println!("phase 21: all checks passed");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("phase 21: {failures} check(s) failed");
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

/// Any column in migration 21 whose name would be a feature this product does not build.
fn forbidden_columns(catalog: &Catalog) -> AuraResult<Vec<String>> {
    catalog.read(|conn| {
        let mut found = Vec::new();
        for table in ["micro_plan", "micro_matrix", "micro_op"] {
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
                // `enlarge` and `swap` are this phase's own additions to phase 20's list: eye
                // enlargement and face swapping are the two features section 2.2 excludes by name.
                for banned in [
                    "reshape",
                    "slim",
                    "waist",
                    "skin_tone",
                    "lighten",
                    "swap",
                    "enlarge",
                    "whiten",
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

/// What an attempt to defeat one of migration 21 two triggers did.
///
/// Three outcomes rather than two, deliberately. A check that reports "the statement failed" as a
/// pass cannot tell a working trigger from a broken fixture, and this is exactly the place where
/// that mistake would be invisible: an INSERT refused for a missing foreign key looks identical
/// to an INSERT refused by the promise, and both leave the row absent.
#[derive(Debug, PartialEq, Eq)]
enum Attempt {
    /// The database refused it. What the trigger is for.
    Refused,
    /// The database allowed it. The promise is broken.
    Allowed,
    /// The attempt never got far enough to be refused by the thing under test.
    Inconclusive(String),
}

/// Insert a borrow directly, then try to take its source away.
///
/// The trigger in migration 21 is what has to stop the second statement: a promise enforced in
/// one layer is a promise until somebody writes a second caller, and this is the promise the
/// whole borrowing feature rests on.
fn strip_borrow_source(
    catalog: &Catalog,
    photo: &PhotoId,
    source: &PhotoId,
) -> AuraResult<Attempt> {
    let photo_key = photo.to_db();
    let source_key = source.to_db();
    catalog.writer().transact(move |conn| {
        // Sequence 900 so it cannot collide with anything the pass stored.
        if let Err(err) = conn.execute(
            "INSERT OR REPLACE INTO micro_op
                 (photo_id, seq, kind, x, y, w, h, method, borrowed_from, alignment)
             VALUES (?1, 900, 'glare', 0.1, 0.1, 0.01, 0.01, 'borrow', ?2, 0.95)",
            params![photo_key, source_key],
        ) {
            return Ok(Attempt::Inconclusive(format!(
                "the borrow itself would not insert: {err}"
            )));
        }
        match conn.execute(
            "UPDATE micro_op SET borrowed_from = NULL WHERE photo_id = ?1 AND seq = 900",
            params![photo_key],
        ) {
            Ok(0) => Ok(Attempt::Inconclusive(
                "the UPDATE matched no row".to_string(),
            )),
            Ok(_) => Ok(Attempt::Allowed),
            Err(_) => Ok(Attempt::Refused),
        }
    })
}

/// Try to insert a crease removal into a project that has not switched creases on.
fn insert_crease_directly(catalog: &Catalog, photo: &PhotoId) -> AuraResult<Attempt> {
    let photo_key = photo.to_db();
    catalog.writer().transact(move |conn| {
        // The control first: the same row with an ordinary kind, which must go in. Without it a
        // foreign-key failure would read as the trigger doing its job.
        if let Err(err) = conn.execute(
            "INSERT OR REPLACE INTO micro_op
                 (photo_id, seq, kind, x, y, w, h, clothing_kind, strength)
             VALUES (?1, 901, 'clothing', 0.2, 0.2, 0.01, 0.01, 'lint', 0.4)",
            params![photo_key],
        ) {
            return Ok(Attempt::Inconclusive(format!(
                "an ordinary lint row would not insert either: {err}"
            )));
        }
        match conn.execute(
            "INSERT OR REPLACE INTO micro_op
                 (photo_id, seq, kind, x, y, w, h, clothing_kind, strength)
             VALUES (?1, 902, 'clothing', 0.2, 0.2, 0.01, 0.01, 'crease', 0.4)",
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
        name: "phase 21".to_string(),
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
                 VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 800,
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
