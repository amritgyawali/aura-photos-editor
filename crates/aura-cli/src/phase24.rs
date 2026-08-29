//! The phase 24 mechanical gate.
//!
//! The assembly proof for generative cleanup: migration 24 and its objects, the policy table a
//! product manager and a security engineer co-own, the five safety checks against an adversarial
//! sweep, the source ordering, the self-check, the store's disclosure rules, and what happens to a
//! whole synthetic wedding.
//!
//! **Nothing here proves anything about a real photograph.** There is no trained distraction
//! detector, no labelled vocabulary and no wedding data in this repository; every fixture below is
//! a frame whose object was painted into it at a rectangle this file already knows. That is
//! conditions C1 and C2 of the exit report, and it is printed at the end of every run rather than
//! hidden in a helper.
//!
//! The unit tests prove the pieces and `tests/eval/cleanup_eval.rs` proves the gates.
//! This proves the assembly - and it is the only place that checks the things that only exist when
//! a catalog, a policy file and a pass are in the same process.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::cleanup::{
    Box2, CleanupCode, CleanupMethod, CleanupProposal, CleanupReason, DistractionClass,
    SafetyCheck, SafetyVerdict, AREA_CAP_DEFAULT, DENYLIST_OVERLAP_MAX, MAX_PROPOSALS_PER_IMAGE,
    ZERO_TOUCH_CONFIDENCE,
};
use aura_core::contract::ids::ProposalId;
use aura_core::{AuraResult, PhotoId, ProjectId, SceneId};
use aura_generative::denylist::{Coverage, Protected};
use aura_generative::queue::{Blocked, Plan, Prepared};
use aura_generative::selfcheck::ArtefactReport;
use aura_generative::store::CleanupStore;
use aura_generative::{fixtures, safety, source, Image, Policy};
use rusqlite::params;

/// Run the phase 24 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase24-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // ---------------------------------------------------------------------------------------
    // 1. Migration 24 and every object it owns.
    // ---------------------------------------------------------------------------------------
    let catalog_path = work.join("phase24.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 24 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 24, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for (kind, name) in [
        ("table", "cleanup_image"),
        ("table", "cleanup_proposal"),
        ("table", "cleanup_blocked"),
        ("table", "cleanup_disclosure"),
        ("view", "v_cleanup_coverage"),
        ("view", "v_cleanup_disclosure"),
        ("trigger", "cleanup_disclosure_no_update"),
        ("trigger", "cleanup_disclosure_no_delete"),
        ("trigger", "cleanup_applied_needs_disclosure"),
        ("trigger", "cleanup_proposal_no_person"),
        ("index", "idx_cleanup_proposal_photo"),
        ("index", "idx_cleanup_proposal_queue"),
        ("index", "idx_cleanup_blocked_check"),
        ("index", "idx_cleanup_disclosure_project"),
    ] {
        match schema_object(&catalog, kind, name) {
            Ok(true) => {}
            Ok(false) => {
                eprintln!("schema: missing {kind} {name}");
                failures += 1;
            }
            Err(err) => {
                eprintln!("schema: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }
    println!("schema: migration 24 objects present");

    // Note 8 of the migration, checked rather than remembered: there is nowhere in this schema to
    // put image data, a file path, or a description of what should be generated. The list is
    // deliberately broad - it is looking for the *shape* of a mistake somebody would make in good
    // faith two phases from now, like a rendered patch cached "just for the panel".
    match forbidden_columns(&catalog) {
        Ok(found) if found.is_empty() => {
            println!("schema: no pixel, path or prompt column anywhere in migration 24");
        }
        Ok(found) => {
            eprintln!("schema: columns that must not exist: {}", found.join(", "));
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 2. The policy table. Co-owned by PM and SEC, and it may only make the product stricter.
    // ---------------------------------------------------------------------------------------
    let policy = match Policy::shipped() {
        Ok(policy) => policy,
        Err(err) => {
            eprintln!("policy: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let mut missing_scenes = Vec::new();
    for scene in SceneId::ALL {
        match policy.scene(scene) {
            Some(row) => {
                if row.area_cap > AREA_CAP_DEFAULT
                    || row.denylist_overlap_max > DENYLIST_OVERLAP_MAX
                    || row.zero_touch_confidence < ZERO_TOUCH_CONFIDENCE
                {
                    eprintln!("policy: {} relaxes a bound the contract owns", scene.as_str());
                    failures += 1;
                }
                if row.reason.trim().is_empty() {
                    eprintln!("policy: {} has no written reason", scene.as_str());
                    failures += 1;
                }
            }
            None => missing_scenes.push(scene.as_str()),
        }
    }
    if missing_scenes.is_empty() {
        println!(
            "policy: {} scene rows, version {}, none relaxes a contract bound",
            policy.len(),
            policy.version
        );
    } else {
        eprintln!("policy: no row for {}", missing_scenes.join(", "));
        failures += 1;
    }

    // A file that tries to widen a bound is refused, and the refusal is what makes
    // `docs/generative-policy.md` a promise rather than a description of some defaults.
    let widened = Policy::load_str(
        "version = 9\n[scene.reception_entrance]\narea_cap = 0.40\n\
         denylist_overlap_max = 0.01\nzero_touch_confidence = 0.97\nreason = \"a test\"\n",
    );
    match widened {
        Err(err) if err.code.0 == "AURA-ML-5119" => {
            println!("policy: a file that raises the area cap is refused");
        }
        _ => {
            eprintln!("policy: a file that raises the area cap was ACCEPTED");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 3. The adversarial sweep. Section 10.1's last row, and section 13's last criterion.
    // ---------------------------------------------------------------------------------------
    let permissive = fixtures::permissive_policy();
    let mut attempts = 0usize;
    let mut damaged = 0usize;
    for gx in 0..10 {
        for gy in 0..10 {
            let region = Box2 {
                x: gx as f32 * 0.1,
                y: gy as f32 * 0.1,
                w: 0.03,
                h: 0.03,
            };

            let mut person = fixtures::candidate(region, DistractionClass::BackgroundPerson);
            person.removability = 1.0;
            attempts += 1;
            if safety::check(&person, &permissive, &Coverage::known_empty()).is_allowed() {
                damaged += 1;
            }

            let clutter = fixtures::candidate(region, DistractionClass::Bin);
            let kind = Protected::ALL
                .get((gx + gy) % Protected::COUNT)
                .copied()
                .unwrap_or(Protected::Face);
            attempts += 1;
            if safety::check(
                &clutter,
                &permissive,
                &Coverage::known(vec![(kind, region)]),
            )
            .is_allowed()
            {
                damaged += 1;
            }

            attempts += 1;
            if safety::check(&clutter, &permissive, &Coverage::Absent).is_allowed() {
                damaged += 1;
            }
        }
    }
    if damaged == 0 {
        println!("safety: {attempts} adversarial attempts, none got past the engine");
    } else {
        eprintln!("safety: {damaged} of {attempts} adversarial attempts SUCCEEDED");
        failures += 1;
    }

    // Every scene in the shipped table, both forbidden classes.
    let mut class_escapes = 0usize;
    for scene in SceneId::ALL {
        let Some(row) = policy.scene(scene) else {
            continue;
        };
        for class in [
            DistractionClass::BackgroundPerson,
            DistractionClass::Unclassified,
        ] {
            let mut candidate = fixtures::candidate(
                Box2 {
                    x: 0.01,
                    y: 0.94,
                    w: 0.02,
                    h: 0.02,
                },
                class,
            );
            candidate.removability = 1.0;
            if safety::check(&candidate, row, &Coverage::known_empty()).is_allowed() {
                eprintln!("safety: {class:?} was allowed in {}", scene.as_str());
                class_escapes += 1;
            }
        }
    }
    if class_escapes == 0 {
        println!("safety: no scene row permits a person or an unnamed object");
    } else {
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 4. The source ordering, and the tier that must always refuse.
    // ---------------------------------------------------------------------------------------
    let clean = fixtures::clean(fixtures::Background::Busy);
    let (target, region) =
        fixtures::with_object(fixtures::Background::Busy, fixtures::CORNER);
    let safe = match safety::check(
        &fixtures::candidate(region, DistractionClass::Bin),
        &permissive,
        &Coverage::known_empty(),
    ) {
        aura_generative::safety::Outcome::Allowed(safe) => *safe,
        aura_generative::safety::Outcome::Blocked { check, .. } => {
            eprintln!("source: the gate's own fixture was blocked by {check:?}");
            return ExitCode::FAILURE;
        }
    };
    let sibling_id = PhotoId::new();
    let siblings = [source::Sibling {
        id: sibling_id,
        image: &clean,
    }];
    match source::select(
        &source::Sources {
            target: &target,
            siblings: &siblings,
            studio_opted_in: false,
        },
        &safe,
    ) {
        Ok(selection) if selection.method.preference() == 0 => {
            println!("source: a clean sibling is preferred over a fill");
        }
        Ok(selection) => {
            eprintln!(
                "source: a clean sibling was available and {} was chosen",
                selection.method.kind_str()
            );
            failures += 1;
        }
        Err(reasons) => {
            eprintln!(
                "source: nothing could source the gate's own fixture: {:?}",
                reasons.iter().map(|r| r.code).collect::<Vec<_>>()
            );
            failures += 1;
        }
    }
    if aura_generative::INPAINT_PACK_INSTALLED {
        eprintln!("source: a diffusion pack is installed but this build ships none");
        failures += 1;
    } else {
        println!("source: the diffusion tier refuses on every call, with no fallback under it");
    }

    // ---------------------------------------------------------------------------------------
    // 5. The self-check catches a deliberate artefact and passes a clean frame.
    // ---------------------------------------------------------------------------------------
    let mut selfcheck_failures = 0usize;
    for (image, region, what) in [
        {
            let (image, region) =
                fixtures::with_repeat_artefact(fixtures::Background::Busy, fixtures::CENTRE);
            (image, region, "repeated texture")
        },
        {
            let (image, region) = fixtures::with_warp_artefact(fixtures::CENTRE);
            (image, region, "warped line")
        },
        {
            let (image, region) = fixtures::with_ghost_artefact(fixtures::CENTRE);
            (image, region, "ghost edge")
        },
    ] {
        if aura_generative::selfcheck::inspect(&image, &region).passes() {
            eprintln!("selfcheck: a deliberate {what} passed");
            selfcheck_failures += 1;
        }
    }
    for background in [
        fixtures::Background::Grass,
        fixtures::Background::Wall,
        fixtures::Background::Busy,
    ] {
        let clean = fixtures::clean(background);
        let region = fixtures::normalise(fixtures::CENTRE);
        if !aura_generative::selfcheck::inspect(&clean, &region).passes() {
            eprintln!("selfcheck: an untouched {background:?} frame was called an artefact");
            selfcheck_failures += 1;
        }
    }
    if selfcheck_failures == 0 {
        println!("selfcheck: three deliberate artefacts caught, three clean frames passed");
    } else {
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 6. The store: a whole synthetic wedding, and the four promises the SQL keeps.
    // ---------------------------------------------------------------------------------------
    let project = ProjectId::new();
    let photos: Vec<PhotoId> = (0..8).map(|_| PhotoId::new()).collect();
    if let Err(err) = seed(&catalog, &project, &photos) {
        eprintln!("store: [{}] {}", err.code, err.detail);
        return ExitCode::FAILURE;
    }
    let store = CleanupStore::new(Arc::clone(&catalog), Arc::clone(&clock));

    let Some(first) = photos.first().copied() else {
        eprintln!("store: the gate seeded no photographs");
        return ExitCode::FAILURE;
    };
    let Some(second) = photos.get(1).copied() else {
        eprintln!("store: the gate seeded too few photographs");
        return ExitCode::FAILURE;
    };

    let prepared = match sample_proposal(first, CleanupMethod::BorrowFrom(second)) {
        Some(prepared) => prepared,
        None => {
            eprintln!("store: the gate could not build its own proposal");
            return ExitCode::FAILURE;
        }
    };
    let proposal_id = prepared.proposal.id;
    let disclosure = prepared.disclosure(true);
    let plan = Plan {
        prepared: vec![prepared],
        blocked: SafetyCheck::ALL
            .into_iter()
            .map(|check| Blocked {
                region: Box2 {
                    x: 0.4,
                    y: 0.4,
                    w: 0.05,
                    h: 0.05,
                },
                check,
                code: CleanupCode::ProtectionUnknown,
                verdict: SafetyVerdict::block(check, "the gate's own refusal"),
            })
            .collect(),
        reverted: 1,
        mask_complete: false,
        judged: 0,
        declined: 0,
    };
    if let Err(err) = store.put(&project, first, SceneId::ReceptionEntrance, &plan, (1, 1, 1)) {
        eprintln!("store: [{}] {}", err.code, err.detail);
        failures += 1;
    }

    // The disclosure and the applied flag are one transaction.
    if let Err(err) = store.apply(&project, &disclosure, true) {
        eprintln!("store: [{}] {}", err.code, err.detail);
        failures += 1;
    }
    match store.disclosures(project) {
        Ok(rows) if rows.len() == 1 => {
            println!("store: the delivery report lists the one removal that happened");
        }
        Ok(rows) => {
            eprintln!("store: the delivery report lists {} rows, expected 1", rows.len());
            failures += 1;
        }
        Err(err) => {
            eprintln!("store: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // The four refusals the SQL keeps. **Each runs its control first** - phase 21's rule: a
    // refusal test that cannot tell a working guard from a broken fixture proves nothing.
    failures += check_refusal(
        &catalog,
        "a disclosure cannot be edited",
        // The control: reading it works.
        |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM cleanup_disclosure",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false)
        },
        |conn| {
            conn.execute(
                "UPDATE cleanup_disclosure SET method_kind = 'fill', method_source = NULL",
                [],
            )
        },
    );
    let applied_key = proposal_id.to_db();
    failures += check_refusal(
        &catalog,
        "an applied removal cannot lose its disclosure",
        move |conn| {
            conn.query_row(
                "SELECT applied FROM cleanup_proposal WHERE proposal_id = ?1",
                params![applied_key],
                |row| row.get::<_, i64>(0),
            )
            .map(|applied| applied == 1)
            .unwrap_or(false)
        },
        |conn| conn.execute("DELETE FROM cleanup_disclosure", []),
    );
    failures += check_refusal(
        &catalog,
        "a person can never be stored as a proposal",
        |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM cleanup_proposal",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false)
        },
        |conn| {
            conn.execute(
                "UPDATE cleanup_proposal SET class = 'background_person'",
                [],
            )
        },
    );

    // Every stored reason code reads back through the frozen enum. Migration 24 deliberately does
    // not CHECK the code list, because a CHECK naming sixteen of thirty-one variants would be a
    // second copy of `is_refusal` that could drift. This is the check that cannot.
    match unknown_codes(&catalog) {
        Ok(unknown) if unknown.is_empty() => {
            println!("store: every stored reason code parses back through the frozen enum");
        }
        Ok(unknown) => {
            eprintln!("store: unparseable reason codes: {}", unknown.join(", "));
            failures += 1;
        }
        Err(err) => {
            eprintln!("store: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // The outline, and the number a photographer reads first.
    match store.outline(project) {
        Ok(outline) => {
            println!(
                "outline: {} photographs, {} examined ({:.0} %), {} with proposals, \
                 {} applied, {} reverted, masks complete on {:.0} %",
                outline.photos,
                outline.examined,
                outline.coverage * 100.0,
                outline.with_proposals,
                outline.applied,
                outline.reverted,
                outline.mask_covered * 100.0
            );
            if outline.blocked.iter().sum::<u32>() != 5 {
                eprintln!("outline: the blocked histogram lost a refusal");
                failures += 1;
            }
            if outline.inpainted > 0 {
                eprintln!(
                    "outline: {} removals are disclosed as inpaints and this build has no model",
                    outline.inpainted
                );
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("outline: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // Invariant 5: the work remaining is a query. A version bump makes the row pending again.
    match (
        store.pending(&project, (1, 1, 1)),
        store.pending(&project, (1, 1, 2)),
    ) {
        (Ok(current), Ok(bumped)) => {
            if current.contains(&first) {
                eprintln!("resume: an examined photograph is still pending at its own versions");
                failures += 1;
            }
            if !bumped.contains(&first) {
                eprintln!("resume: a policy bump did not make the row pending again");
                failures += 1;
            }
            println!(
                "resume: {} pending at the current versions, {} after a policy bump",
                current.len(),
                bumped.len()
            );
        }
        _ => {
            eprintln!("resume: the pending set could not be read");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 7. The two things this phase promises about itself, checked rather than described.
    // ---------------------------------------------------------------------------------------
    if MAX_PROPOSALS_PER_IMAGE > 3 {
        eprintln!("contract: the proposal cap has grown past a light touch");
        failures += 1;
    }
    if CleanupCode::ALL
        .into_iter()
        .filter(|code| code.is_refusal())
        .count()
        != CleanupCode::REFUSAL_COUNT
    {
        eprintln!("contract: the refusal count no longer matches the enum");
        failures += 1;
    } else {
        println!(
            "contract: {} of {} reason codes are refusals",
            CleanupCode::REFUSAL_COUNT,
            CleanupCode::ALL.len()
        );
    }

    // ---------------------------------------------------------------------------------------
    // The caveat, printed on every run rather than hidden in a helper.
    // ---------------------------------------------------------------------------------------
    println!();
    println!("what this gate does NOT prove:");
    println!("  - that any of these numbers describes a wedding photograph. There is no trained");
    println!("    distraction detector and no labelled vocabulary in this repository, so every");
    println!("    fixture above is a frame whose object was painted in at a known rectangle.");
    println!("    Conditions C1 and C2 of docs/progress/PHASE-24-EXIT.md.");
    println!("  - that a photographer would agree with a removal. Section 10.1's QAIQ audit is");
    println!("    three hundred attempts looked at by a person, and there is no such audit here.");
    println!(
        "  - detector trained: {}, artefact head trained: {}, inpaint pack: {}",
        aura_generative::DISTRACTION_HEAD_TRAINED,
        aura_generative::ARTEFACT_HEAD_TRAINED,
        aura_generative::INPAINT_PACK_INSTALLED
    );

    if failures == 0 {
        println!("\nphase 24: OK");
        ExitCode::SUCCESS
    } else {
        eprintln!("\nphase 24: FAILED ({failures} problems)");
        ExitCode::FAILURE
    }
}

/// Run a control, then an attempt that must be refused.
///
/// Returns 1 when the check did not hold, and prints which half failed. Phase 21's rule: reporting
/// "inconclusive" rather than "pass" when the control never reached the thing under test.
fn check_refusal(
    catalog: &Arc<Catalog>,
    what: &str,
    control: impl Fn(&rusqlite::Connection) -> bool + Send + 'static,
    attempt: impl Fn(&rusqlite::Connection) -> rusqlite::Result<usize> + Send + 'static,
) -> usize {
    let ok = catalog.read(move |conn| Ok(control(conn))).unwrap_or(false);
    if !ok {
        eprintln!("store: INCONCLUSIVE - the control for \"{what}\" did not hold");
        return 1;
    }
    let refused = catalog
        .writer()
        .transact(move |conn| {
            attempt(conn).map_err(|e| aura_core::errors::db::statement_failed("attempt", &e))?;
            Ok(())
        })
        .is_err();
    if refused {
        println!("store: {what}");
        0
    } else {
        eprintln!("store: \"{what}\" was NOT enforced");
        1
    }
}

/// One proposal, ready to store.
fn sample_proposal(image: PhotoId, method: CleanupMethod) -> Option<Prepared> {
    let mut proposal = CleanupProposal::new(
        ProposalId::new(),
        image,
        Box2 {
            x: 0.02,
            y: 0.85,
            w: 0.06,
            h: 0.06,
        },
        DistractionClass::Bin,
        method,
        SafetyVerdict::allow(),
        vec![CleanupReason::plain(CleanupCode::TextureUniform, 1.0)],
    )
    .ok()?;
    proposal.confidence = 0.72;
    proposal.salience = 0.8;
    proposal.scene = SceneId::ReceptionEntrance;
    proposal.detector_ver = 1;
    proposal.analysis_ver = 1;
    proposal.policy_ver = 1;
    Some(Prepared {
        proposal,
        patch: Image::black(4, 4),
        artefact: ArtefactReport::CLEAN,
    })
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

/// Any column in migration 24's tables whose name suggests a pixel, a path or a prompt.
///
/// Note 8 of the migration, checked rather than remembered. The list is deliberately broad: it is
/// looking for the shape of a mistake somebody would make in good faith while adding a feature two
/// phases from now.
fn unknown_columns_named() -> [&'static str; 9] {
    [
        "prompt",
        "instruction",
        "description",
        "pixels",
        "patch_data",
        "thumbnail",
        "path",
        "file",
        "blob",
    ]
}

fn forbidden_columns(catalog: &Catalog) -> AuraResult<Vec<String>> {
    catalog.read(move |conn| {
        let mut found = Vec::new();
        for table in [
            "cleanup_image",
            "cleanup_proposal",
            "cleanup_blocked",
            "cleanup_disclosure",
        ] {
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
                let lowered = column.to_ascii_lowercase();
                if unknown_columns_named()
                    .iter()
                    .any(|needle| lowered.contains(needle))
                {
                    found.push(format!("{table}.{column}"));
                }
            }
        }
        Ok(found)
    })
}

/// Every stored reason code that `CleanupCode::parse` does not recognise.
fn unknown_codes(catalog: &Catalog) -> AuraResult<Vec<String>> {
    catalog.read(move |conn| {
        let mut unknown = Vec::new();
        let mut statement = conn
            .prepare("SELECT code FROM cleanup_blocked")
            .map_err(|e| aura_core::errors::db::statement_failed("codes", &e))?;
        let mut rows = statement
            .query([])
            .map_err(|e| aura_core::errors::db::statement_failed("codes", &e))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| aura_core::errors::db::statement_failed("codes", &e))?
        {
            let code: String = row.get(0).unwrap_or_default();
            if CleanupCode::parse(&code).is_none() {
                unknown.push(code);
            }
        }
        let mut statement = conn
            .prepare("SELECT reasons FROM cleanup_proposal")
            .map_err(|e| aura_core::errors::db::statement_failed("reasons", &e))?;
        let mut rows = statement
            .query([])
            .map_err(|e| aura_core::errors::db::statement_failed("reasons", &e))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| aura_core::errors::db::statement_failed("reasons", &e))?
        {
            let reasons: String = row.get(0).unwrap_or_default();
            for slug in reasons.split(',').filter(|s| !s.trim().is_empty()) {
                if CleanupCode::parse(slug).is_none() {
                    unknown.push(slug.to_string());
                }
            }
        }
        Ok(unknown)
    })
}

/// One project and a handful of photographs.
fn seed(catalog: &Arc<Catalog>, project: &ProjectId, photos: &[PhotoId]) -> AuraResult<()> {
    let row = ProjectRow {
        project_id: project.to_db(),
        name: "phase 24".to_string(),
        couple_label: None,
        event_date: None,
        timezone: "UTC".to_string(),
        status: "active".to_string(),
        created_at: "2026-08-29T00:00:00Z".to_string(),
        updated_at: "2026-08-29T00:00:00Z".to_string(),
    };
    catalog
        .writer()
        .transact(move |conn| repo::create_project(conn, &row))?;
    let project_key = project.to_db();
    let ids: Vec<String> = photos.iter().map(aura_core::PhotoId::to_db).collect();
    catalog.writer().transact(move |tx| {
        for (index, photo) in ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                    camera_make, camera_model, iso, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 1600,
                         '2026-08-29T00:00:00Z', '2026-08-29T00:00:00Z')",
                params![
                    photo,
                    project_key,
                    format!("2026-08-29T{:02}:{:02}:00Z", index / 60 % 24, index % 60),
                ],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
        }
        Ok(())
    })
}
