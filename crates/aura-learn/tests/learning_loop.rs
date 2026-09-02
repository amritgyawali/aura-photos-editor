#![allow(clippy::assertions_on_constants)]

//! The loop end to end, over a real catalog and a real ledger.
//!
//! Section 10.1 asks for four things this file proves: that the loop improves on held-out
//! corrections after three corrected weddings, that rollback restores the previous profile
//! **exactly**, that no update is adopted without explicit user action, and that opt-in dataset
//! contribution is off by default and recorded with consent.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::ids::ProfileId;
use aura_core::contract::learn::{
    Consent, LearnService, Learnable, MIN_CORRECTIONS, MIN_OFFERABLE_IMPROVEMENT, MIN_PROJECTS,
};
use aura_core::contract::ledger::{
    DecisionKind, DecisionSource, DecisionSubject, ExplainService, LedgerReason,
};
use aura_core::contract::scene::SceneId;
use aura_core::contract::tone::ToneCode;
use aura_core::ProjectId;
use aura_explain::api::Explain;
use aura_explain::decision::DecisionBuilder;
use aura_explain::ledger::Ledger;
use aura_explain::policy::Risk;
use aura_learn::api::{offsets_from, Learn};
use aura_learn::fixtures;
use aura_learn::store::LearnStore;

/// A catalog, a ledger and an explain service over one temporary directory.
fn wired(dir: &std::path::Path) -> (Arc<Catalog>, Arc<Explain>) {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let catalog = Arc::new(
        Catalog::open(&dir.join("c.sqlite"), Arc::clone(&clock), "test").expect("catalog"),
    );
    let ledger = Arc::new(Ledger::new(Arc::clone(&catalog), Arc::clone(&clock)));
    let explain = Arc::new(Explain::new(ledger, clock).expect("explain"));
    (catalog, explain)
}

fn seed_project(catalog: &Arc<Catalog>, project: ProjectId) {
    let key = project.to_db();
    catalog
        .writer()
        .with(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'wedding', '2026-05-16T00:00:00Z', '2026-05-16T00:00:00Z')",
                rusqlite::params![key],
            )
            .map(|_| ())
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))
        })
        .expect("project");
}

fn seed_photo(catalog: &Arc<Catalog>, project: ProjectId, photo: aura_core::PhotoId, ix: i64) {
    let key = project.to_db();
    let id = photo.to_db();
    catalog
        .writer()
        .with(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO photo (photo_id, project_id, capture_time, timeline_time,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3, ?4, ?4)",
                rusqlite::params![id, key, 1_760_000_000_000_i64 + ix, "2026-05-16T00:00:00Z"],
            )
            .map(|_| ())
            .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))
        })
        .expect("photo");
}

/// Record a decision in the ledger so a correction has something to be a correction *of*, and
/// return the id it was given.
///
/// `DecisionId` is assigned by `record_built` rather than chosen, which is the ledger working: an
/// id a caller picked would be an id a caller could reuse. The correction is re-pointed at whatever
/// came back, which is also what the real capture path does.
fn record_decision(
    explain: &Explain,
    project: ProjectId,
    photo: aura_core::PhotoId,
    kind: DecisionKind,
) -> aura_core::contract::ids::DecisionId {
    // A reason code from the shipped registry. Phase 30 extended the registry with phases 15,
    // 16, 27 and 29's vocabularies, because `DecisionKind` had six members and the registry had
    // words for one - so an Edit decision could not be recorded at all before this phase.
    let code = match kind {
        DecisionKind::Edit => ToneCode::SubjectUnderexposed.as_str(),
        _ => aura_core::contract::cull::CullCode::MomentWinner.as_str(),
    };
    let text = match kind {
        DecisionKind::Edit => ToneCode::SubjectUnderexposed.user_text(),
        _ => aura_core::contract::cull::CullCode::MomentWinner.user_text(),
    };
    let builder = DecisionBuilder::new(project, kind, DecisionSubject::Image(photo))
        .output_num("value", 0.0)
        .confidence(0.8)
        .source(DecisionSource::Local)
        .input("image", photo.to_db())
        .reason(LedgerReason::plain(code, text, 0.6));
    explain
        .record_built(builder, Risk::NONE)
        .expect("record")
        .id
}

/// Seed a whole archive: projects, photographs, decisions and consent.
fn seed_archive(
    catalog: &Arc<Catalog>,
    explain: &Explain,
    store: &LearnStore,
    archive: &mut fixtures::Archive,
) {
    for project in &archive.projects {
        seed_project(catalog, *project);
        store
            .set_consent(&fixtures::learning_only(*project))
            .expect("consent");
    }
    for (ix, (correction, context)) in archive.corrections.iter_mut().enumerate() {
        seed_photo(catalog, context.project, context.image, ix as i64);
        correction.decision_id =
            record_decision(explain, context.project, context.image, correction.kind);
    }
}

#[test]
fn three_corrected_weddings_produce_an_update_that_improves_on_corrections_it_never_saw() {
    // Section 10.1's headline row, measured on authored corrections. What it proves is the
    // arithmetic and the split; what it does not prove is that a real photographer would recognise
    // the result - `FITTED_ON_REAL_CORRECTIONS` is false and says so.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, explain) = wired(dir.path());
    let store = LearnStore::new(Arc::clone(&catalog));
    let mut archive = fixtures::archive(Learnable::Exposure, 0.30, 3, 20, 7);
    seed_archive(&catalog, &explain, &store, &mut archive);

    let profile = fixtures::derived_profile(7);
    let learn = Learn::new(
        Arc::clone(&catalog),
        Arc::clone(&explain) as Arc<dyn ExplainService>,
        "0.1.0",
    )
    .with_current(profile, offsets_from(&[]));

    for (correction, context) in &archive.corrections {
        learn.capture(correction, context).expect("captured");
    }

    let outline = learn.outline().expect("outline");
    assert_eq!(outline.corrections, 60);
    assert_eq!(outline.projects, 3);
    assert!(outline.actionable_buckets >= 1);

    let update = learn
        .compute(profile)
        .expect("compute")
        .expect("a candidate");
    assert!(!update.adopted, "computing adopts nothing");
    assert!(update.corrections_used >= MIN_CORRECTIONS);
    assert!(
        update.expected_improvement >= MIN_OFFERABLE_IMPROVEMENT,
        "improvement {} below the noise floor",
        update.expected_improvement
    );
    assert!(update.is_offerable());

    let comparison = learn
        .compare(profile)
        .expect("compare")
        .expect("a comparison");
    assert!(comparison.candidate_error < comparison.current_error);
    assert!(comparison.held_out > 0);
    assert_eq!(comparison.rows.len(), 1);
    assert_eq!(comparison.rows[0].learnable, Learnable::Exposure);
}

#[test]
fn nothing_is_adopted_until_a_person_adopts_it() {
    // Section 10.1, and the property `learn_update_no_self_adopt` enforces from the other side.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, explain) = wired(dir.path());
    let store = LearnStore::new(Arc::clone(&catalog));
    let mut archive = fixtures::archive(Learnable::Contrast, 0.12, 3, 20, 11);
    seed_archive(&catalog, &explain, &store, &mut archive);

    let profile = fixtures::derived_profile(11);
    let learn = Learn::new(
        Arc::clone(&catalog),
        Arc::clone(&explain) as Arc<dyn ExplainService>,
        "0.1.0",
    )
    .with_current(profile, offsets_from(&[]));
    for (c, ctx) in &archive.corrections {
        learn.capture(c, ctx).expect("captured");
    }
    learn.compute(profile).expect("compute").expect("candidate");

    // Computing, comparing and reading all leave it unadopted.
    assert!(
        !store
            .candidate(profile)
            .expect("read")
            .expect("some")
            .0
            .adopted
    );
    learn.compare(profile).expect("compare");
    assert!(
        !store
            .candidate(profile)
            .expect("read")
            .expect("some")
            .0
            .adopted
    );
    assert_eq!(learn.outline().expect("outline").adopted, 0);

    // A person adopts it.
    let adopted = learn.adopt(profile).expect("adopt");
    assert!(adopted.adopted);
    assert_eq!(learn.outline().expect("outline").adopted, 1);
}

#[test]
fn the_database_refuses_an_update_that_arrives_already_adopted() {
    // `learn_update_no_self_adopt`. A promise enforced in one layer lasts until somebody writes a
    // second caller, which phase 21 wrote down after finding it twice. A control runs first, so a
    // refusal caused by a broken fixture cannot read as the trigger working.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, _) = wired(dir.path());
    let profile = ProfileId::new();

    // The control: an unadopted insert is accepted.
    let key = profile.to_db();
    let ok = catalog.writer().with(move |conn| {
        conn.execute(
            "INSERT INTO learn_update (update_id, profile_id, from_version, to_version,
                 corrections_used, held_out_used, current_error, candidate_error,
                 expected_improvement, diff_summary, adopted, computed_at)
             VALUES ('u1', ?1, 1, 2, 40, 10, 0.2, 0.1, 0.5, '[]', 0, '2026-05-16T00:00:00Z')",
            rusqlite::params![key],
        )
        .map(|_| ())
        .map_err(|e| aura_core::errors::db::statement_failed("learn_update", &e))
    });
    assert!(ok.is_ok(), "the control insert must succeed: {ok:?}");

    let key = profile.to_db();
    let refused = catalog.writer().with(move |conn| {
        conn.execute(
            "INSERT INTO learn_update (update_id, profile_id, from_version, to_version,
                 corrections_used, held_out_used, current_error, candidate_error,
                 expected_improvement, diff_summary, adopted, computed_at)
             VALUES ('u2', ?1, 2, 3, 40, 10, 0.2, 0.1, 0.5, '[]', 1, '2026-05-16T00:00:00Z')",
            rusqlite::params![key],
        )
        .map(|_| ())
        .map_err(|e| aura_core::errors::db::statement_failed("learn_update", &e))
    });
    assert!(
        refused.is_err(),
        "an update that arrives adopted must be refused"
    );
}

#[test]
fn a_correction_with_no_decision_behind_it_is_refused_and_the_change_is_kept() {
    // Phase 17's condition C4, in the phase that would carry it into every future wedding.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, explain) = wired(dir.path());
    let store = LearnStore::new(Arc::clone(&catalog));
    let archive = fixtures::archive(Learnable::Exposure, 0.3, 1, 1, 3);
    let project = archive.projects[0];
    seed_project(&catalog, project);
    store
        .set_consent(&fixtures::learning_only(project))
        .expect("consent");
    // ...and deliberately no decision recorded.

    let learn = Learn::new(
        Arc::clone(&catalog),
        Arc::clone(&explain) as Arc<dyn ExplainService>,
        "0.1.0",
    );
    let (correction, context) = &archive.corrections[0];
    let err = learn.capture(correction, context).expect_err("refused");
    assert_eq!(err.code.0, "AURA-LRN-11004");
    // A warning, not a failure: the photograph is exactly as the photographer left it.
    assert_eq!(err.severity, aura_core::Severity::Warning);
    assert_eq!(learn.outline().expect("outline").corrections, 0);
}

#[test]
fn a_project_that_has_not_consented_is_not_learned_from() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, explain) = wired(dir.path());
    let store = LearnStore::new(Arc::clone(&catalog));
    let mut archive = fixtures::archive(Learnable::Exposure, 0.3, 1, 2, 5);
    let project = archive.projects[0];
    seed_project(&catalog, project);
    for (ix, (c, ctx)) in archive.corrections.iter_mut().enumerate() {
        seed_photo(&catalog, project, ctx.image, ix as i64);
        c.decision_id = record_decision(&explain, project, ctx.image, c.kind);
    }
    // No consent row at all, which is the default.
    let consent = store.consent(project, "0.1.0").expect("consent");
    assert!(!consent.local_learning);
    assert!(!consent.dataset_contribution, "off by default");
    assert!(!consent.anything_leaves(), "nothing leaves by default");

    let learn = Learn::new(
        Arc::clone(&catalog),
        Arc::clone(&explain) as Arc<dyn ExplainService>,
        "0.1.0",
    );
    let (c, ctx) = &archive.corrections[0];
    assert_eq!(
        learn.capture(c, ctx).expect_err("refused").code.0,
        "AURA-LRN-11004"
    );
}

#[test]
fn consent_is_recorded_with_the_wording_it_was_given_to() {
    // A consent given to one release's wording is a consent to *that wording*. A privacy page that
    // changes while the consent does not is a consent that has quietly become about something else.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, explain) = wired(dir.path());
    let project = ProjectId::new();
    seed_project(&catalog, project);

    let learn = Learn::new(
        Arc::clone(&catalog),
        Arc::clone(&explain) as Arc<dyn ExplainService>,
        "0.2.0",
    );
    let mut consent = Consent::none(project, "0.1.0");
    consent.local_learning = true;
    consent.dataset_contribution = true;
    consent.decided_at = 1_760_000_000_000;
    learn.set_consent(&consent).expect("set");

    let read = learn.consent(project).expect("read");
    assert!(read.local_learning);
    assert!(read.dataset_contribution);
    assert!(read.anything_leaves());
    assert_eq!(
        read.app_version, "0.1.0",
        "the version that asked, not the one reading"
    );
    assert_eq!(read.decided_at, 1_760_000_000_000);
}

#[test]
fn a_rollback_restores_the_previous_profile_byte_for_byte() {
    // Section 10.1: "rollback restores the previous profile exactly". Exactly is a byte
    // comparison, which is why a snapshot is a whole document rather than a delta.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, _) = wired(dir.path());
    let store = LearnStore::new(Arc::clone(&catalog));
    let profile = fixtures::derived_profile(21);

    let v1 = r#"{"name":"personal","version":1,"global":{"exposure":0.00}}"#;
    let v2 = r#"{"name":"personal","version":2,"global":{"exposure":0.15}}"#;
    store.write_snapshot(profile, 1, v1).expect("v1");
    store.write_snapshot(profile, 2, v2).expect("v2");
    assert_eq!(store.current_version(profile).expect("current"), Some(2));

    let (restored, reasons) = aura_learn::rollback::restore(&store, profile).expect("rollback");
    assert_eq!(restored.version, 1);
    assert_eq!(restored.body, v1, "byte for byte");
    assert_eq!(
        restored.body_hash,
        blake3::hash(v1.as_bytes()).to_hex().to_string()
    );
    assert!(reasons
        .iter()
        .any(|r| r.code == aura_core::contract::learn::LearnCode::RollbackExact));
    assert_eq!(store.current_version(profile).expect("after"), Some(1));

    // ...and there is nothing behind version 1.
    let err = aura_learn::rollback::restore(&store, profile).expect_err("nothing behind");
    assert_eq!(err.code.0, "AURA-LRN-11005");
    assert!(!aura_learn::rollback::can_roll_back(&store, profile).expect("can"));
}

#[test]
fn a_corrupt_snapshot_is_refused_rather_than_put_back() {
    // Putting back a profile nobody wrote is worse than refusing.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, _) = wired(dir.path());
    let store = LearnStore::new(Arc::clone(&catalog));
    let profile = fixtures::derived_profile(23);
    store.write_snapshot(profile, 1, "{\"v\":1}").expect("v1");
    store.write_snapshot(profile, 2, "{\"v\":2}").expect("v2");

    // Corrupt the stored body without touching its digest, which is what a bad sector does.
    let key = profile.to_db();
    catalog
        .writer()
        .with(move |conn| {
            conn.execute(
                "UPDATE learn_profile_snapshot SET body = '{\"v\":9}'
                 WHERE profile_id = ?1 AND version = 1",
                rusqlite::params![key],
            )
            .map(|_| ())
            .map_err(|e| aura_core::errors::db::statement_failed("snapshot", &e))
        })
        .expect("corrupt");

    let err = aura_learn::rollback::restore(&store, profile).expect_err("refused");
    assert_eq!(err.code.0, "AURA-LRN-11005");
}

#[test]
fn corrections_from_one_wedding_are_not_acted_on_however_many_there_are() {
    // Section 6.3's "require a minimum count before acting", and the half that matters more: a
    // marquee's yellow canvas learned from a single Saturday and applied to every wedding after.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, explain) = wired(dir.path());
    let store = LearnStore::new(Arc::clone(&catalog));
    let mut archive = fixtures::archive(Learnable::TemperatureK, 300.0, 1, 80, 13);
    seed_archive(&catalog, &explain, &store, &mut archive);

    let profile = fixtures::derived_profile(13);
    let learn = Learn::new(
        Arc::clone(&catalog),
        Arc::clone(&explain) as Arc<dyn ExplainService>,
        "0.1.0",
    )
    .with_current(profile, offsets_from(&[]));
    for (c, ctx) in &archive.corrections {
        learn.capture(c, ctx).expect("captured");
    }

    let outline = learn.outline().expect("outline");
    assert_eq!(outline.corrections, 80);
    assert_eq!(outline.projects, 1);
    assert_eq!(
        outline.actionable_buckets, 0,
        "eighty corrections from one wedding is still one wedding"
    );
    assert!(learn.compute(profile).expect("compute").is_none());
    assert!(MIN_PROJECTS >= 2);
}

#[test]
fn one_extreme_rescue_does_not_move_the_offset() {
    // A mean would carry the badly-lit room into every future wedding. A trimmed median does not.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, explain) = wired(dir.path());
    let store = LearnStore::new(Arc::clone(&catalog));
    let mut archive = fixtures::archive(Learnable::Exposure, 0.20, 3, 20, 17);
    for i in 0..4 {
        let (mut c, mut ctx) = fixtures::outlier(Learnable::Exposure, 3.5, archive.projects[0], 17);
        c.decision_id = fixtures::derived_decision(17, 500, i);
        ctx.image = fixtures::derived_image(17, 500, i);
        archive.corrections.push((c, ctx));
    }
    seed_archive(&catalog, &explain, &store, &mut archive);

    let profile = fixtures::derived_profile(17);
    let learn = Learn::new(
        Arc::clone(&catalog),
        Arc::clone(&explain) as Arc<dyn ExplainService>,
        "0.1.0",
    )
    .with_current(profile, offsets_from(&[]));
    for (c, ctx) in &archive.corrections {
        let _ = learn.capture(c, ctx);
    }

    let aggregates = learn.aggregates(profile).expect("aggregates");
    let exposure = aggregates
        .iter()
        .find(|a| a.bucket.learnable == Learnable::Exposure)
        .expect("bucket");
    assert!(
        (exposure.central - 0.20).abs() < 0.03,
        "the centre moved to {}",
        exposure.central
    );
    assert!(exposure.outliers_dropped > 0);
    // Half of 0.20, not half of the mean with four 3.5s in it.
    assert!((exposure.proposed_offset() - 0.10).abs() < 0.02);
}

#[test]
fn a_scene_that_was_never_corrected_gets_no_offset() {
    // Invariant 7 in this phase: no threshold is global, so an offset learned in one scene must
    // not travel to a scene nobody corrected.
    let dir = tempfile::tempdir().expect("tempdir");
    let (catalog, explain) = wired(dir.path());
    let store = LearnStore::new(Arc::clone(&catalog));
    let mut archive = fixtures::archive(Learnable::Exposure, 0.3, 3, 20, 29);
    seed_archive(&catalog, &explain, &store, &mut archive);

    let profile = fixtures::derived_profile(29);
    let learn = Learn::new(
        Arc::clone(&catalog),
        Arc::clone(&explain) as Arc<dyn ExplainService>,
        "0.1.0",
    )
    .with_current(profile, offsets_from(&[]));
    for (c, ctx) in &archive.corrections {
        learn.capture(c, ctx).expect("captured");
    }
    learn.compute(profile).expect("compute").expect("candidate");
    let comparison = learn.compare(profile).expect("compare").expect("some");
    for row in &comparison.rows {
        assert_eq!(
            row.scene,
            SceneId::Unknown,
            "an offset appeared in a scene nobody corrected"
        );
    }
}
