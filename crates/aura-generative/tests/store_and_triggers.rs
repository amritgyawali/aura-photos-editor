//! Migration 24 against a real catalog: the disclosure rules, the person CHECK and the
//! photographer's own decision surviving a re-analysis.
//!
//! ## Every refusal test runs a control first
//!
//! Phase 21's lesson, and it is the reason this file is longer than it looks like it needs to be:
//! **a refusal test that cannot tell a working guard from a broken fixture proves nothing.** An
//! INSERT rejected for a missing foreign key looks exactly like one rejected by the promise it is
//! supposed to be testing.
//!
//! So each of the three trigger tests below inserts the *legal* version of the same row first and
//! asserts it succeeds. If the control fails, the test says so rather than passing.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, FixedClock};
use aura_core::contract::cleanup::{
    Box2, CleanupCode, CleanupMethod, CleanupProposal, CleanupReason, DistractionClass, SafetyCheck,
    SafetyVerdict,
};
use aura_core::contract::ids::ProposalId;
use aura_core::contract::ledger::Autonomy;
use aura_core::{PhotoId, ProjectId, SceneId};
use aura_generative::queue::{Blocked, Plan, Prepared};
use aura_generative::selfcheck::ArtefactReport;
use aura_generative::store::CleanupStore;
use aura_generative::Image;
use rusqlite::params;

/// A catalog with one project and two photographs.
fn catalog() -> (tempfile::TempDir, Arc<Catalog>, ProjectId, PhotoId, PhotoId) {
    let dir = tempfile::tempdir().expect("tempdir");
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::default());
    let catalog = Catalog::open(&dir.path().join("c.sqlite"), Arc::clone(&clock), "test")
        .expect("the catalog opens and migrates to 24");
    let catalog = Arc::new(catalog);

    let project = ProjectId::new();
    let first = PhotoId::new();
    let second = PhotoId::new();
    let (p, a, b) = (project.to_db(), first.to_db(), second.to_db());

    catalog
        .writer()
        .transact(move |conn| {
            conn.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                      VALUES (?1, 'wedding', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                params![p],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))?;
            for photo in [&a, &b] {
                conn.execute(
                    "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                        created_at, updated_at)
                          VALUES (?1, ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
                                  '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    params![photo, p],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
            }
            Ok(())
        })
        .expect("the fixture rows are inserted");

    (dir, catalog, project, first, second)
}

fn store(catalog: &Arc<Catalog>) -> CleanupStore {
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::default());
    CleanupStore::new(Arc::clone(catalog), clock)
}

fn region() -> Box2 {
    Box2 {
        x: 0.02,
        y: 0.85,
        w: 0.06,
        h: 0.06,
    }
}

/// One prepared proposal, ready to store.
fn prepared(image: PhotoId, method: CleanupMethod, class: DistractionClass) -> Prepared {
    let mut proposal = CleanupProposal::new(
        ProposalId::new(),
        image,
        region(),
        class,
        method,
        SafetyVerdict::allow(),
        vec![CleanupReason::plain(CleanupCode::TextureUniform, 1.0)],
    )
    .expect("a well-formed proposal");
    proposal.confidence = 0.72;
    proposal.salience = 0.8;
    proposal.autonomy = Autonomy::RequireReview;
    proposal.scene = SceneId::ReceptionEntrance;
    proposal.detector_ver = 1;
    proposal.analysis_ver = 1;
    proposal.policy_ver = 1;
    Prepared {
        proposal,
        patch: Image::black(4, 4),
        artefact: ArtefactReport::CLEAN,
    }
}

fn plan_of(prepared: Vec<Prepared>, blocked: Vec<Blocked>) -> Plan {
    Plan {
        prepared,
        blocked,
        reverted: 0,
        mask_complete: true,
        judged: 0,
        declined: 0,
    }
}

#[test]
fn a_pass_writes_proposals_refusals_and_an_examination_row() {
    let (_dir, catalog, project, photo, _other) = catalog();
    let store = store(&catalog);

    let prepared = prepared(photo, CleanupMethod::ClassicalFill, DistractionClass::Bin);
    let id = prepared.proposal.id;
    let plan = plan_of(
        vec![prepared],
        vec![Blocked {
            region: Box2 {
                x: 0.4,
                y: 0.4,
                w: 0.1,
                h: 0.1,
            },
            check: SafetyCheck::Denylist,
            code: CleanupCode::ProtectionUnknown,
            verdict: SafetyVerdict::block(SafetyCheck::Denylist, "no masks"),
        }],
    );

    store
        .put(&project, photo, SceneId::ReceptionEntrance, &plan, (1, 1, 1))
        .expect("the plan is stored");

    let read = store.proposals(photo).expect("proposals read back");
    assert_eq!(read.len(), 1);
    assert_eq!(read.first().map(|p| p.id), Some(id));
    assert_eq!(read.first().map(|p| p.method.clone()), Some(CleanupMethod::ClassicalFill));

    let blocked = store.blocked(photo).expect("refusals read back");
    assert_eq!(blocked.len(), 1, "a refusal is a row");
    assert_eq!(
        blocked.first().map(|(_, check, code)| (*check, *code)),
        Some((SafetyCheck::Denylist, CleanupCode::ProtectionUnknown))
    );

    // An examined photograph with no proposals is different from an unexamined one, and the
    // outline is where the difference shows.
    let outline = store.outline(project).expect("outline");
    assert_eq!(outline.photos, 2);
    assert_eq!(outline.examined, 1);
    assert_eq!(outline.with_proposals, 1);
    assert!((outline.coverage - 0.5).abs() < 1e-6);
    assert!((outline.mask_covered - 1.0).abs() < 1e-6);
}

#[test]
fn an_applied_removal_carries_a_disclosure_and_the_delivery_report_lists_it() {
    // Section 13's fifth acceptance criterion, end to end.
    let (_dir, catalog, project, photo, source) = catalog();
    let store = store(&catalog);

    let prepared = prepared(
        photo,
        CleanupMethod::BorrowFrom(source),
        DistractionClass::Bin,
    );
    let id = prepared.proposal.id;
    let disclosure = prepared.disclosure(true);
    store
        .put(
            &project,
            photo,
            SceneId::ReceptionEntrance,
            &plan_of(vec![prepared], Vec::new()),
            (1, 1, 1),
        )
        .expect("stored");

    store
        .apply(&project, &disclosure, true)
        .expect("the disclosure and the applied flag are one transaction");

    let listed = store.disclosures(project).expect("the delivery report");
    assert_eq!(listed.len(), 1);
    let row = listed.first().expect("one row");
    assert_eq!(row.proposal_id, id);
    assert_eq!(row.method, CleanupMethod::BorrowFrom(source));
    assert!(row.accepted_by_user);
}

#[test]
fn a_removal_cannot_be_applied_without_a_disclosure() {
    let (_dir, catalog, project, photo, _other) = catalog();
    let store = store(&catalog);
    let prepared = prepared(photo, CleanupMethod::ClassicalFill, DistractionClass::Bin);
    let id = prepared.proposal.id;
    let disclosure = prepared.disclosure(true);
    store
        .put(
            &project,
            photo,
            SceneId::ReceptionEntrance,
            &plan_of(vec![prepared], Vec::new()),
            (1, 1, 1),
        )
        .expect("stored");

    // The control: the legal path succeeds. Without this the assertion below could be passing
    // because the row does not exist rather than because the trigger fired.
    store
        .apply(&project, &disclosure, false)
        .expect("CONTROL: the legal path must work, or this test proves nothing");
    store.unapply(id).expect("and it can be undone");

    // The refusal: setting `applied` with no disclosure behind it.
    let key = id.to_db();
    let refused = catalog.writer().transact(move |conn| {
        conn.execute(
            "UPDATE cleanup_proposal SET applied = 1 WHERE proposal_id = ?1",
            params![key],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("applied", &e))?;
        Ok(())
    });
    assert!(
        refused.is_err(),
        "a cleanup was applied with no disclosure behind it"
    );
}

#[test]
fn a_disclosure_can_never_be_edited() {
    let (_dir, catalog, project, photo, source) = catalog();
    let store = store(&catalog);
    let prepared = prepared(
        photo,
        CleanupMethod::BorrowFrom(source),
        DistractionClass::Bin,
    );
    let id = prepared.proposal.id;
    let disclosure = prepared.disclosure(true);
    store
        .put(
            &project,
            photo,
            SceneId::ReceptionEntrance,
            &plan_of(vec![prepared], Vec::new()),
            (1, 1, 1),
        )
        .expect("stored");
    store.apply(&project, &disclosure, true).expect("applied");

    // The control: the row exists and can be read.
    let key = id.to_db();
    let control = key.clone();
    let found: i64 = catalog
        .read(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM cleanup_disclosure WHERE proposal_id = ?1",
                params![control],
                |row| row.get(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("count", &e))
        })
        .expect("CONTROL: the disclosure must exist, or this test proves nothing");
    assert_eq!(found, 1);

    // The refusal: any UPDATE at all, including one that would launder a borrow into a fill.
    let refused = catalog.writer().transact(move |conn| {
        conn.execute(
            "UPDATE cleanup_disclosure SET method_kind = 'fill', method_source = NULL
              WHERE proposal_id = ?1",
            params![key],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("edit", &e))?;
        Ok(())
    });
    assert!(refused.is_err(), "a disclosure was edited");
}

#[test]
fn a_person_can_never_be_stored_as_a_proposal() {
    // The CHECK, tested from the raw SQL side, because the type system already refuses this from
    // every path in the crate and the constraint exists for the path somebody adds later.
    let (_dir, catalog, project, photo, _other) = catalog();

    // The control: the same INSERT with a legal class succeeds.
    let (p, ph) = (project.to_db(), photo.to_db());
    let control = insert_raw(&catalog, &p, &ph, "bin");
    assert!(
        control.is_ok(),
        "CONTROL: a legal class must insert, or this test proves nothing: {control:?}"
    );

    for class in ["background_person", "unclassified"] {
        let refused = insert_raw(&catalog, &p, &ph, class);
        assert!(refused.is_err(), "{class} was stored as a proposal");
    }
}

/// A raw INSERT of one proposal with a given class, bypassing every type in the crate.
fn insert_raw(
    catalog: &Arc<Catalog>,
    project: &str,
    photo: &str,
    class: &str,
) -> Result<(), aura_core::AuraError> {
    let (project, photo, class) = (
        project.to_string(),
        photo.to_string(),
        class.to_string(),
    );
    let id = ProposalId::new().to_db();
    catalog.writer().transact(move |conn| {
        conn.execute(
            "INSERT INTO cleanup_proposal (
                    proposal_id, photo_id, project_id, x, y, w, h, class, area_frac,
                    salience, confidence, method_kind, checks, autonomy, scene, reasons,
                    artefact_score, applied, detector_ver, analysis_ver, policy_ver, proposed_at
                 ) VALUES (?1, ?2, ?3, 0.02, 0.85, 0.06, 0.06, ?4, 0.0036,
                           0.8, 0.72, 'fill', '11111', 'require_review', 'reception_entrance', '',
                           0.0, 0, 1, 1, 1, '2026-01-01T00:00:00Z')",
            params![id, photo, project, class],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("raw insert", &e))?;
        Ok(())
    })
}

#[test]
fn a_blocked_candidate_can_never_be_stored_as_a_proposal() {
    // The `checks` CHECK. `CleanupProposal::new` refuses it first; this is the second layer, for
    // the raw INSERT a future caller writes.
    let (_dir, catalog, project, photo, _other) = catalog();
    let (p, ph) = (project.to_db(), photo.to_db());

    let control = insert_raw_checks(&catalog, &p, &ph, "11111");
    assert!(
        control.is_ok(),
        "CONTROL: an all-passed verdict must insert: {control:?}"
    );
    for failed in ["11110", "01111", "00000", "1111"] {
        assert!(
            insert_raw_checks(&catalog, &p, &ph, failed).is_err(),
            "a verdict of {failed} became a proposal"
        );
    }
}

fn insert_raw_checks(
    catalog: &Arc<Catalog>,
    project: &str,
    photo: &str,
    checks: &str,
) -> Result<(), aura_core::AuraError> {
    let (project, photo, checks) = (
        project.to_string(),
        photo.to_string(),
        checks.to_string(),
    );
    let id = ProposalId::new().to_db();
    catalog.writer().transact(move |conn| {
        conn.execute(
            "INSERT INTO cleanup_proposal (
                    proposal_id, photo_id, project_id, x, y, w, h, class, area_frac,
                    salience, confidence, method_kind, checks, autonomy, scene, reasons,
                    artefact_score, applied, detector_ver, analysis_ver, policy_ver, proposed_at
                 ) VALUES (?1, ?2, ?3, 0.02, 0.85, 0.06, 0.06, 'bin', 0.0036,
                           0.8, 0.72, 'fill', ?4, 'require_review', 'reception_entrance', '',
                           0.0, 0, 1, 1, 1, '2026-01-01T00:00:00Z')",
            params![id, photo, project, checks],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("raw insert", &e))?;
        Ok(())
    })
}

#[test]
fn a_borrow_cannot_be_stored_without_naming_its_source() {
    let (_dir, catalog, project, photo, source) = catalog();
    let (p, ph, src) = (project.to_db(), photo.to_db(), source.to_db());

    let control = insert_raw_method(&catalog, &p, &ph, "borrow", Some(&src));
    assert!(
        control.is_ok(),
        "CONTROL: a borrow with a source must insert: {control:?}"
    );
    assert!(
        insert_raw_method(&catalog, &p, &ph, "borrow", None).is_err(),
        "a borrow was stored with no source: the disclosure would disclose nothing"
    );
    assert!(
        insert_raw_method(&catalog, &p, &ph, "fill", Some(&src)).is_err(),
        "a fill was stored carrying a source it cannot have had"
    );
}

fn insert_raw_method(
    catalog: &Arc<Catalog>,
    project: &str,
    photo: &str,
    method: &str,
    source: Option<&str>,
) -> Result<(), aura_core::AuraError> {
    let (project, photo, method, source) = (
        project.to_string(),
        photo.to_string(),
        method.to_string(),
        source.map(str::to_string),
    );
    let id = ProposalId::new().to_db();
    catalog.writer().transact(move |conn| {
        conn.execute(
            "INSERT INTO cleanup_proposal (
                    proposal_id, photo_id, project_id, x, y, w, h, class, area_frac,
                    salience, confidence, method_kind, method_source, checks, autonomy, scene,
                    reasons, artefact_score, applied, detector_ver, analysis_ver, policy_ver,
                    proposed_at
                 ) VALUES (?1, ?2, ?3, 0.02, 0.85, 0.06, 0.06, 'bin', 0.0036,
                           0.8, 0.72, ?4, ?5, '11111', 'require_review', 'reception_entrance',
                           '', 0.0, 0, 1, 1, 1, '2026-01-01T00:00:00Z')",
            params![id, photo, project, method, source],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("raw insert", &e))?;
        Ok(())
    })
}

#[test]
fn a_photographers_decision_survives_a_re_analysis() {
    // The twelfth time this rule has been written into a store, and here it works only because the
    // proposal id is derived from the region rather than issued fresh.
    let (_dir, catalog, project, photo, _other) = catalog();
    let store = store(&catalog);

    let first = prepared(photo, CleanupMethod::ClassicalFill, DistractionClass::Bin);
    let id = first.proposal.id;
    store
        .put(
            &project,
            photo,
            SceneId::ReceptionEntrance,
            &plan_of(vec![first.clone()], Vec::new()),
            (1, 1, 1),
        )
        .expect("stored");

    store.decide(photo, id, false).expect("rejected by a person");

    // The pass runs again and produces the same proposal, because the id is a digest of what the
    // proposal *is*.
    store
        .put(
            &project,
            photo,
            SceneId::ReceptionEntrance,
            &plan_of(vec![first], Vec::new()),
            (1, 1, 1),
        )
        .expect("stored again");

    let key = id.to_db();
    let accepted: Option<i64> = catalog
        .read(move |conn| {
            conn.query_row(
                "SELECT accepted FROM cleanup_proposal WHERE proposal_id = ?1",
                params![key],
                |row| row.get(0),
            )
            .map_err(|e| aura_core::errors::db::statement_failed("accepted", &e))
        })
        .expect("read back");
    assert_eq!(
        accepted,
        Some(0),
        "a photographer's rejection was overwritten by a re-analysis"
    );
}

#[test]
fn a_photograph_a_photographer_switched_off_is_not_pending() {
    let (_dir, catalog, project, photo, other) = catalog();
    let store = store(&catalog);

    let pending = store.pending(&project, (1, 1, 1)).expect("pending");
    assert_eq!(pending.len(), 2, "CONTROL: both photographs start pending");

    store
        .set_disabled(&project, photo, SceneId::ReceptionEntrance, true)
        .expect("switched off");
    assert!(store.is_disabled(photo).expect("read back"));

    let pending = store.pending(&project, (1, 1, 1)).expect("pending");
    assert_eq!(pending, vec![other].into_iter().collect::<Vec<_>>().clone());
}

#[test]
fn a_version_bump_makes_every_row_pending_again() {
    // Invariant 5: the work remaining is a query, so a `policy_ver` bump heals itself.
    let (_dir, catalog, project, photo, _other) = catalog();
    let store = store(&catalog);
    store
        .put(
            &project,
            photo,
            SceneId::ReceptionEntrance,
            &plan_of(Vec::new(), Vec::new()),
            (1, 1, 1),
        )
        .expect("stored");

    assert!(
        !store
            .pending(&project, (1, 1, 1))
            .expect("pending")
            .contains(&photo),
        "CONTROL: an examined photograph at the current versions is not pending"
    );
    assert!(
        store
            .pending(&project, (1, 1, 2))
            .expect("pending")
            .contains(&photo),
        "a policy bump must make the row pending again"
    );
    assert!(store.check_versions(photo, (1, 1, 1)).is_ok());
    let drift = store
        .check_versions(photo, (1, 1, 2))
        .expect_err("a drift is reported");
    assert_eq!(drift.code.0, "AURA-ML-5115");
}

#[test]
fn an_override_that_asks_for_nothing_is_refused() {
    use aura_core::contract::cleanup::{CleanupOverride, CleanupService};
    let (_dir, catalog, _project, photo, _other) = catalog();
    let service =
        aura_generative::Cleanup::new(Arc::new(store(&catalog))).expect("the service builds");
    let err = service
        .decide(photo, &CleanupOverride::default())
        .expect_err("an empty override is refused");
    assert_eq!(err.code.0, "AURA-ML-5117");
}

#[test]
fn the_coverage_view_reports_the_blocked_histogram_by_check() {
    let (_dir, catalog, project, photo, _other) = catalog();
    let store = store(&catalog);
    let blocked: Vec<Blocked> = SafetyCheck::ALL
        .into_iter()
        .map(|check| Blocked {
            region: region(),
            check,
            code: CleanupCode::ConfidenceLow,
            verdict: SafetyVerdict::block(check, "a test"),
        })
        .collect();
    store
        .put(
            &project,
            photo,
            SceneId::ReceptionEntrance,
            &plan_of(Vec::new(), blocked),
            (1, 1, 1),
        )
        .expect("stored");

    let outline = store.outline(project).expect("outline");
    assert_eq!(
        outline.blocked,
        [1, 1, 1, 1, 1],
        "every check must be counted in its own slot"
    );
    assert_eq!(outline.with_proposals, 0);
    assert_eq!(outline.applied, 0);
}
