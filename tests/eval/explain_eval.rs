//! PHASE-13 section 10.1, as an ordinary `cargo test` so that a red gate is a red build.
//!
//! Six criteria are named there and all six are here, plus the structural properties this
//! phase's ADR turned from promises into assertions.
//!
//! ## What these gates prove
//!
//! The **mechanism**. A decision that cannot explain itself is refused; a reason code that
//! nothing documents is refused; a ledger row cannot be updated; a summary that invents a
//! number is caught; a bundle that carries an identifier is caught; the calibration
//! estimator recovers a known error from a known predictor.
//!
//! ## What they do not prove
//!
//! **That AURA is well calibrated.** Every outcome measured here comes from
//! `aura_explain::fixtures`, whose predictors are synthetic and whose error is authored.
//! Section 6.1's real gate - expected calibration error at or below 0.05 on held-out
//! labelled outcomes from real weddings - needs labelled outcomes from real weddings, and
//! there are none in this repository. That is condition C2 of the phase 13 exit report.
//!
//! This is the phase where a passing gate is most likely to be misread as "the confidence
//! numbers are trustworthy". They are not yet. What passes here is "the machinery that will
//! make them trustworthy works, and says so when it has not been used".

use std::sync::Arc;

use aura_catalog::model::ProjectRow;
use aura_catalog::{repo, Catalog};
use aura_core::clock::{Clock, FixedClock};
use aura_core::contract::ids::DecisionId;
use aura_core::contract::ledger::{
    Autonomy, DecisionKind, DecisionSource, DecisionSubject, Evidence, ExplainService,
    LedgerDecision, LedgerReason,
};
use aura_core::{PhotoId, ProjectId};
use aura_explain::calibration::{Calibrator, Outcome, Reliability, ECE_GATE};
use aura_explain::fixtures::{self, DriftingSource, FaithfulSource, OutcomeShape};
use aura_explain::policy::{AutonomyPolicy, Risk};
use aura_explain::reason::{
    Catalog as ReasonCatalog, ReasonSeverity, COMPOSITION, EMOTION, LEDGER, SELECTION, TECHNICAL,
};
use aura_explain::replay::{ReplayOutcome, Replayer};
use aura_explain::summary::{Grounding, Summariser};
use aura_explain::{Bundle, BundleOptions, Explain, Ledger};

/// Section 11: bytes of ledger per thousand images, for a full pipeline.
const LEDGER_BYTES_PER_THOUSAND: u64 = 6 * 1024 * 1024;

/// A catalog with one project in it, and the explainer over it.
struct Fixture {
    _dir: tempfile::TempDir,
    explain: Explain,
    project: ProjectId,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let clock: Arc<dyn Clock> = FixedClock::at(
        time::OffsetDateTime::from_unix_timestamp(1_770_000_000).expect("a fixed instant"),
    );
    let catalog = Arc::new(
        Catalog::open(
            &dir.path().join("ledger.sqlite"),
            Arc::clone(&clock),
            "0.1.0",
        )
        .expect("the catalog opens"),
    );
    let project = ProjectId::new();
    let now = aura_catalog::rfc3339(clock.now_utc());
    catalog
        .writer()
        .transact({
            let row = ProjectRow {
                project_id: project.to_db(),
                name: "explain eval".to_string(),
                couple_label: None,
                event_date: None,
                timezone: "UTC".to_string(),
                status: "active".to_string(),
                created_at: now.clone(),
                updated_at: now,
            };
            move |conn| repo::create_project(conn, &row)
        })
        .expect("the project inserts");

    let ledger = Arc::new(Ledger::new(Arc::clone(&catalog), Arc::clone(&clock)));
    let explain = Explain::new(ledger, clock).expect("the shipped band table loads");
    Fixture {
        _dir: dir,
        explain,
        project,
    }
}

// ---------------------------------------------------------------------------
// The registry is the vocabulary, and the reference is the registry
// ---------------------------------------------------------------------------

#[test]
fn the_registry_covers_every_vocabulary_that_can_reach_it() {
    let catalog = ReasonCatalog::shipped();
    for code in aura_core::contract::integrity::ReasonCode::ALL {
        assert!(
            catalog.contains(code.as_str()),
            "phase 09's `{}` can be emitted and is not in the registry",
            code.as_str()
        );
    }
    for code in aura_core::contract::emotion::EmotionCode::ALL {
        assert!(
            catalog.contains(code.as_str()),
            "phase 10's `{code}` is missing"
        );
    }
    for code in aura_core::contract::composition::CompositionCode::ALL {
        assert!(
            catalog.contains(code.as_str()),
            "phase 11's `{code}` is missing"
        );
    }
    for code in aura_core::CullCode::ALL {
        assert!(
            catalog.contains(code.as_str()),
            "phase 12's `{code}` is missing"
        );
    }
    for domain in [TECHNICAL, EMOTION, COMPOSITION, SELECTION, LEDGER] {
        assert!(
            !catalog.domain(domain).is_empty(),
            "the `{domain}` domain has no codes, so the reference has an empty section"
        );
    }
}

#[test]
fn every_registered_code_has_a_sentence_a_person_could_read() {
    let catalog = ReasonCatalog::shipped();
    for entry in catalog.all() {
        assert!(
            entry.template.len() > 12,
            "`{}` has no usable sentence template",
            entry.code
        );
        assert!(
            !entry.template.contains('_'),
            "`{}`'s sentence reads like a slug rather than a sentence: {}",
            entry.code,
            entry.template
        );
    }
}

#[test]
fn the_public_reference_documents_every_code_and_invents_none() {
    let reference = include_str!("../../docs/reason-codes.md");
    let catalog = ReasonCatalog::shipped();
    for entry in catalog.all() {
        assert!(
            reference.contains(&format!("`{}`", entry.code)),
            "docs/reason-codes.md does not document `{}`, and section 6.2 makes that \
             reference public",
            entry.code
        );
    }
}

#[test]
fn a_code_that_admits_an_absence_is_a_caveat_and_never_a_fault() {
    let catalog = ReasonCatalog::shipped();
    // The four phases each have a way of saying "nothing was measured", and every one of
    // them has to read as a caveat. A build that showed `keypoints_unavailable` as a fault
    // would be telling a photographer their photograph is badly framed because AURA did not
    // look at it.
    for code in [
        "no_subject",
        "uncalibrated",
        "no_faces",
        "no_scene",
        "keypoints_unavailable",
        "aesthetic_unavailable",
        "not_analysed",
    ] {
        assert_eq!(
            catalog.severity(code),
            ReasonSeverity::Caveat,
            "`{code}` says nothing was measured and must not read as a fault"
        );
    }
}

// ---------------------------------------------------------------------------
// Invariant 2, as something the product cannot break
// ---------------------------------------------------------------------------

#[test]
fn a_decision_with_no_reason_is_refused_rather_than_stored() {
    let fixture = fixture();
    let bare = LedgerDecision {
        id: DecisionId::new(),
        project: fixture.project,
        kind: DecisionKind::Cull,
        subject: DecisionSubject::Image(PhotoId::new()),
        inputs_hash: 1,
        outputs_json: "{}".to_string(),
        reasons: Vec::new(),
        raw_confidence: 0.99,
        calibrated_confidence: 0.99,
        autonomy: Autonomy::Auto,
        source: DecisionSource::Local,
        model_versions: Vec::new(),
        config_versions: Vec::new(),
        calibration_ver: 0,
        ms: 1,
        created_at: 0,
        supersedes: None,
    };
    let err = fixture
        .explain
        .record(&bare)
        .expect_err("a decision with no reason must not be recorded");
    assert_eq!(err.code.0, "AURA-ML-5054");

    let outline = fixture
        .explain
        .outline(fixture.project)
        .expect("an outline");
    assert_eq!(
        outline.decisions, 0,
        "the refused decision must not be in the ledger at all"
    );
}

#[test]
fn a_reason_nothing_documents_is_refused() {
    let fixture = fixture();
    let decision = fixtures::kept(fixture.project, PhotoId::new(), PhotoId::new(), 0.9)
        .reason(LedgerReason::plain("vibes_were_off", "it felt wrong", -0.5))
        .build(DecisionId::new(), 0);
    let err = fixture
        .explain
        .record(&decision)
        .expect_err("an undocumented code must be refused");
    assert_eq!(err.code.0, "AURA-ML-5054");
}

#[test]
fn every_recorded_decision_is_explained_and_carries_a_confidence() {
    let fixture = fixture();
    for decision in fixtures::wedding(fixture.project, 60, 1_770_000_000_000) {
        fixture
            .explain
            .record(&decision)
            .expect("a decision records");
    }
    let outline = fixture
        .explain
        .outline(fixture.project)
        .expect("an outline");
    assert_eq!(outline.decisions, 60);
    assert_eq!(
        outline.explained, outline.decisions,
        "section 10.1's explanation coverage gate is 100 %"
    );
    assert!((outline.explanation_coverage() - 1.0).abs() < f32::EPSILON);
}

// ---------------------------------------------------------------------------
// A caller cannot grant itself autonomy
// ---------------------------------------------------------------------------

#[test]
fn a_caller_cannot_set_its_own_band() {
    let fixture = fixture();
    let mut decision = fixtures::kept(fixture.project, PhotoId::new(), PhotoId::new(), 0.10)
        .build(DecisionId::new(), 0);
    decision.autonomy = Autonomy::Auto;
    decision.calibrated_confidence = 0.99;

    let id = fixture.explain.record(&decision).expect("it records");
    let stored = fixture
        .explain
        .decision(id)
        .expect("it reads back")
        .expect("it is there");
    assert_eq!(
        stored.autonomy,
        Autonomy::RequireReview,
        "a decision at 0.10 confidence must ask, whatever the caller claimed"
    );
    assert!(
        (stored.calibrated_confidence - 0.10).abs() < 0.001,
        "the calibrated confidence is recomputed from the raw one, not taken from the caller"
    );
}

#[test]
fn an_uncalibrated_build_asks_more_often_and_says_why() {
    let fixture = fixture();
    let decision = fixtures::kept(fixture.project, PhotoId::new(), PhotoId::new(), 0.99)
        .build(DecisionId::new(), 0);
    let stored = fixture
        .explain
        .record_with_risk(&decision, Risk::NONE)
        .expect("it records");

    assert_eq!(
        stored.autonomy,
        Autonomy::AutoZeroTouch,
        "with no fitted calibration the band is raised one step"
    );
    assert!(
        stored
            .reasons
            .iter()
            .any(|reason| reason.code == "uncalibrated_confidence"),
        "the raise is explained in the panel rather than only in a log"
    );
    assert!(fixture.explain.calibration_warning().is_some());
}

#[test]
fn a_must_have_decision_is_raised_one_band() {
    let policy = AutonomyPolicy::embedded().expect("the shipped table loads");
    let ordinary = policy.decide(DecisionKind::Cull, 0.99, Risk::NONE);
    let protected = policy.decide(DecisionKind::Cull, 0.99, Risk::must_have());
    assert_eq!(ordinary, Autonomy::Auto);
    assert_eq!(
        protected,
        Autonomy::AutoZeroTouch,
        "section 6.4: a decision touching a must-have moment is raised one band"
    );
}

#[test]
fn an_irreversible_kind_is_raised_from_the_enum_and_not_from_a_file() {
    let policy = AutonomyPolicy::embedded().expect("the shipped table loads");
    for kind in DecisionKind::ALL {
        let band = policy.decide(kind, 1.0, Risk::NONE.for_kind(kind));
        if kind.is_irreversible() {
            assert_ne!(
                band,
                Autonomy::Auto,
                "{} is irreversible and must never act unattended at any confidence",
                kind.as_str()
            );
        }
    }
}

#[test]
fn the_shipped_band_table_descends_and_carries_reasons() {
    let policy = AutonomyPolicy::embedded().expect("the shipped table loads");
    assert!(policy.version() >= 1);
    assert!(policy.rows() >= 5, "five kinds carry their own row");
    for kind in DecisionKind::ALL {
        let bands = policy.bands(kind);
        assert!(
            bands.is_ordered(),
            "{}'s thresholds must descend from auto to suggest",
            kind.as_str()
        );
    }
    // A row with no written reason is refused, which is what makes the file arguable.
    let err = AutonomyPolicy::parse(
        "version = 1\n[defaults]\nauto_at=0.98\nzero_touch_at=0.9\nsuggest_at=0.75\n\
         [[kind]]\nid=\"cull\"\nauto_at=0.98\nzero_touch_at=0.9\nsuggest_at=0.75\n",
    )
    .expect_err("a row with no reason must be refused");
    assert_eq!(err.code.0, "AURA-ML-5055");
}

#[test]
fn a_band_table_that_does_not_descend_is_refused() {
    let err = AutonomyPolicy::parse(
        "version = 1\n[defaults]\nauto_at=0.5\nzero_touch_at=0.9\nsuggest_at=0.75\n",
    )
    .expect_err("an inverted table must be refused");
    assert_eq!(err.code.0, "AURA-ML-5055");
}

// ---------------------------------------------------------------------------
// The ledger is append-only, and corrections supersede
// ---------------------------------------------------------------------------

#[test]
fn a_correction_supersedes_rather_than_overwrites() {
    let fixture = fixture();
    let image = PhotoId::new();
    let first =
        fixtures::kept(fixture.project, image, PhotoId::new(), 0.8).build(DecisionId::new(), 1_000);
    let first_id = fixture.explain.record(&first).expect("the first records");

    let correction = fixtures::user_kept(fixture.project, image)
        .supersedes(first_id)
        .build(DecisionId::new(), 2_000);
    fixture
        .explain
        .record(&correction)
        .expect("the correction records");

    let history = fixture
        .explain
        .history(DecisionSubject::Image(image))
        .expect("a history");
    assert_eq!(history.len(), 2, "the earlier decision is still on record");
    assert_eq!(
        history.first().map(|d| d.source),
        Some(DecisionSource::User),
        "the newest decision comes first"
    );

    let outline = fixture
        .explain
        .outline(fixture.project)
        .expect("an outline");
    assert_eq!(outline.superseded, 1);
    assert_eq!(outline.current, 1);
}

#[test]
fn compaction_keeps_the_newest_decision_and_every_user_override() {
    let fixture = fixture();
    let image = PhotoId::new();
    let machine =
        fixtures::kept(fixture.project, image, PhotoId::new(), 0.8).build(DecisionId::new(), 1_000);
    let machine_id = fixture.explain.record(&machine).expect("it records");
    let correction = fixtures::user_kept(fixture.project, image)
        .supersedes(machine_id)
        .build(DecisionId::new(), 2_000);
    let correction_id = fixture.explain.record(&correction).expect("it records");

    // A third decision supersedes the photographer's own, to prove the filter is about the
    // source rather than about position in the chain.
    let later = fixtures::kept(fixture.project, image, PhotoId::new(), 0.9)
        .supersedes(correction_id)
        .build(DecisionId::new(), 3_000);
    fixture.explain.record(&later).expect("it records");

    let removed = fixture
        .explain
        .ledger()
        .compact(fixture.project, aura_explain::Compaction::everything())
        .expect("compaction runs");
    assert_eq!(removed, 1, "only the superseded machine decision may go");

    let history = fixture
        .explain
        .history(DecisionSubject::Image(image))
        .expect("a history");
    assert!(
        history
            .iter()
            .any(|decision| decision.source == DecisionSource::User),
        "a photographer's own decision survives compaction, whatever superseded it"
    );
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------

#[test]
fn replay_reproduces_a_stored_decision_exactly() {
    let project = ProjectId::new();
    let source = FaithfulSource;
    let replayer = Replayer::new(&source);
    for decision in fixtures::wedding(project, 24, 0) {
        assert!(
            replayer
                .replay(&decision)
                .expect("a replay runs")
                .is_identical(),
            "an unchanged pipeline must reproduce every decision exactly"
        );
        replayer
            .assert_identical(&decision)
            .expect("and the gate form agrees");
    }
}

#[test]
fn replay_tells_a_determinism_bug_from_an_upgrade() {
    let project = ProjectId::new();
    let decision = fixtures::wedding(project, 2, 0)
        .into_iter()
        .next()
        .expect("a decision");

    let nondeterministic = DriftingSource {
        moved_inputs: false,
    };
    let outcome = Replayer::new(&nondeterministic)
        .replay(&decision)
        .expect("a replay runs");
    assert!(
        matches!(outcome, ReplayOutcome::Nondeterministic { .. }),
        "the same question answered differently is a determinism failure"
    );

    let upgraded = DriftingSource { moved_inputs: true };
    let outcome = Replayer::new(&upgraded)
        .replay(&decision)
        .expect("a replay runs");
    assert!(
        matches!(outcome, ReplayOutcome::Drifted { .. }),
        "a different question is an upgrade, not a defect"
    );

    let err = Replayer::new(&upgraded)
        .assert_identical(&decision)
        .expect_err("the gate form still fails");
    assert_eq!(err.code.0, "AURA-ML-5057");
}

#[test]
fn the_inputs_hash_is_over_the_question_and_not_the_answer() {
    let project = ProjectId::new();
    let image = PhotoId::new();
    let other = PhotoId::new();
    let keep = fixtures::kept(project, image, other, 0.9).build(DecisionId::new(), 0);
    let drop = fixtures::dropped(project, image, other, 0.9).build(DecisionId::new(), 0);
    assert_ne!(
        keep.outputs_json, drop.outputs_json,
        "two opposite decisions have different outputs"
    );
    // ...and the inputs differ too here, because the two fixtures declare different inputs.
    // What must never happen is the reverse: identical declared inputs producing different
    // hashes, which is what this second assertion pins.
    let again = fixtures::kept(project, image, other, 0.9).build(DecisionId::new(), 5_000);
    assert_eq!(
        keep.inputs_hash, again.inputs_hash,
        "the hash must not depend on the id or the timestamp"
    );
    assert_eq!(keep.outputs_json, again.outputs_json);
}

// ---------------------------------------------------------------------------
// Calibration
// ---------------------------------------------------------------------------

#[test]
fn the_estimator_recovers_a_known_calibration_error() {
    let calibrated = Reliability::measure(&fixtures::outcomes(OutcomeShape::Calibrated, 4_000, 7));
    assert!(
        calibrated.ece <= ECE_GATE,
        "a well-calibrated predictor must measure below the gate; got {:.4}",
        calibrated.ece
    );

    let overconfident =
        Reliability::measure(&fixtures::outcomes(OutcomeShape::Overconfident, 4_000, 11));
    assert!(
        overconfident.ece > ECE_GATE,
        "an overconfident predictor must be caught by the same gate; got {:.4}",
        overconfident.ece
    );
    assert!(
        !overconfident.passes_gate(),
        "and it must fail section 6.1's gate at four thousand samples"
    );
}

#[test]
fn isotonic_regression_fixes_an_overconfident_predictor() {
    let training = fixtures::outcomes(OutcomeShape::Overconfident, 4_000, 3);
    let model = Calibrator::fit_isotonic(&training, 1);
    assert!(model.is_fitted());

    let held_out = fixtures::outcomes(OutcomeShape::Overconfident, 4_000, 99);
    let mapped: Vec<Outcome> = held_out
        .iter()
        .map(|outcome| Outcome::new(model.apply(outcome.raw), outcome.correct))
        .collect();
    let before = Reliability::measure(&held_out);
    let after = Reliability::measure(&mapped);
    assert!(
        after.ece < before.ece,
        "calibration must improve held-out ECE: {:.4} -> {:.4}",
        before.ece,
        after.ece
    );
    assert!(
        after.ece <= ECE_GATE,
        "and must bring it under section 6.1's gate; got {:.4}",
        after.ece
    );
}

#[test]
fn a_calibration_never_reorders_two_confidences() {
    let model = Calibrator::fit_isotonic(
        &fixtures::outcomes(OutcomeShape::Overconfident, 2_000, 5),
        1,
    );
    let mut previous = -1.0f32;
    let mut value = 0.0f32;
    while value <= 1.0 {
        let mapped = model.apply(value);
        assert!(
            mapped >= previous - 1e-4,
            "a calibration that reordered two confidences would reorder the review queue"
        );
        previous = mapped;
        value += 0.01;
    }
}

#[test]
fn temperature_scaling_is_available_for_thin_data() {
    let thin = fixtures::outcomes(OutcomeShape::Overconfident, 40, 13);
    let model = Calibrator::fit_temperature(&thin, 1);
    assert!(model.is_fitted());
    assert!(model.temperature > 0.0);
    // Monotone, which is the property section 6.1 requires of the data-poor path.
    assert!(model.apply(0.9) >= model.apply(0.6));
}

#[test]
fn the_shipped_calibration_is_the_identity_map_and_says_so() {
    let set = aura_explain::CalibrationSet::shipped();
    assert!(!set.is_fitted(), "nothing in this build has been fitted");
    assert_eq!(set.version(), 0);
    assert_eq!(
        set.uncalibrated().len(),
        DecisionKind::COUNT * 2,
        "every kind, for both calibratable sources"
    );
    for kind in DecisionKind::ALL {
        let (mapped, version) = set.apply(kind, DecisionSource::Local, 0.77);
        assert!((mapped - 0.77).abs() < f32::EPSILON);
        assert_eq!(version, 0);
    }
}

// ---------------------------------------------------------------------------
// Summaries say only what they were given
// ---------------------------------------------------------------------------

#[test]
fn a_template_summary_invents_no_number() {
    let project = ProjectId::new();
    for decision in fixtures::wedding(project, 20, 0) {
        let summary = Summariser::template(&decision);
        let facts = Summariser::facts(&decision);
        let grounding = Grounding::check(&summary.summary, &facts);
        assert!(
            grounding.is_grounded(),
            "the offline summary invented {:?} for {}",
            grounding.ungrounded,
            decision.id
        );
        assert!(summary.is_bounded());
    }
}

#[test]
fn the_grounding_check_catches_an_invented_measurement() {
    let project = ProjectId::new();
    let decision =
        fixtures::kept(project, PhotoId::new(), PhotoId::new(), 0.9).build(DecisionId::new(), 0);
    let facts = Summariser::facts(&decision);
    let fabricated = "AURA kept this frame; the shutter speed was 1/60 and exposure moved 0.42 EV.";
    let grounding = Grounding::check(fabricated, &facts);
    assert!(
        !grounding.is_grounded(),
        "a summary citing numbers nobody supplied must be caught"
    );
    assert!(
        grounding.ungrounded.iter().any(|number| number == "0.42"),
        "the invented exposure delta must be named: {:?}",
        grounding.ungrounded
    );
}

// ---------------------------------------------------------------------------
// The support bundle
// ---------------------------------------------------------------------------

#[test]
fn a_support_bundle_carries_no_identifier_and_no_pixel() {
    let fixture = fixture();
    let decisions = fixtures::wedding(fixture.project, 40, 1_770_000_000_000);
    for decision in &decisions {
        fixture.explain.record(decision).expect("it records");
    }
    let stored = fixture
        .explain
        .ledger()
        .project(fixture.project)
        .expect("the ledger reads back");
    let outline = fixture
        .explain
        .outline(fixture.project)
        .expect("an outline");
    let bundle = Bundle::build(&stored, &outline, &BundleOptions::default()).expect("a bundle");

    assert!(bundle.is_safe(), "forbidden content: {:?}", bundle.scan());
    assert!(bundle.decisions > 0);
    for decision in &stored {
        assert!(
            !bundle.json.contains(&decision.subject.id_str()),
            "a raw subject id reached the bundle"
        );
        assert!(
            !bundle.json.contains(&decision.id.to_db()),
            "a raw decision id reached the bundle"
        );
    }
    assert!(
        !bundle.json.contains("data:image"),
        "there is no way for a pixel to reach a bundle, and this asserts it anyway"
    );
    // The structure survives anonymisation: the same frame is the same handle everywhere.
    assert!(bundle.json.contains("image_0001") || bundle.json.contains("decision_0001"));
}

#[test]
fn an_empty_ledger_produces_a_refusal_rather_than_an_empty_bundle() {
    let outline = aura_core::contract::ledger::LedgerOutline::default();
    let err = Bundle::build(&[], &outline, &BundleOptions::default())
        .expect_err("an empty bundle looks like evidence that nothing happened");
    assert_eq!(err.code.0, "AURA-ML-5059");
}

// ---------------------------------------------------------------------------
// Size
// ---------------------------------------------------------------------------

#[test]
fn the_ledger_stays_inside_its_size_budget() {
    let fixture = fixture();
    // One decision per image is what a cull produces; section 11's budget is stated for a
    // full pipeline, which is this plus the five kinds no phase writes yet.
    let decisions = fixtures::wedding(fixture.project, 1_000, 1_770_000_000_000);
    for decision in &decisions {
        fixture.explain.record(decision).expect("it records");
    }
    let outline = fixture
        .explain
        .outline(fixture.project)
        .expect("an outline");
    assert_eq!(outline.decisions, 1_000);
    assert!(
        outline.bytes <= LEDGER_BYTES_PER_THOUSAND,
        "a thousand decisions took {} bytes against a {LEDGER_BYTES_PER_THOUSAND} budget",
        outline.bytes
    );
}

// ---------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------

#[test]
fn evidence_survives_a_round_trip_through_the_catalog() {
    let fixture = fixture();
    let image = PhotoId::new();
    let other = PhotoId::new();
    let decision = fixtures::kept(fixture.project, image, other, 0.9)
        .reason(LedgerReason::with_params(
            "clean",
            "focus, motion, exposure, noise and eyes all read normally here",
            0.1,
            vec![
                ("temperature_k".to_string(), -610.0),
                ("exposure_ev".to_string(), 0.42),
            ],
        ))
        .build(DecisionId::new(), 0);
    let id = fixture.explain.record(&decision).expect("it records");

    let stored = fixture
        .explain
        .decision(id)
        .expect("it reads back")
        .expect("it is there");
    let crops = stored
        .reasons
        .iter()
        .filter(|reason| reason.evidence.crop().is_some())
        .count();
    assert!(crops >= 1, "a crop rectangle survived the round trip");
    let params: Vec<&(String, f32)> = stored
        .reasons
        .iter()
        .flat_map(|reason| reason.evidence.params())
        .collect();
    assert!(
        params
            .iter()
            .any(|(name, value)| name == "temperature_k" && (*value + 610.0).abs() < 0.01),
        "a parameter delta survived the round trip: {params:?}"
    );
    assert!(stored.evidenced() >= 2);
}

#[test]
fn a_reason_whose_sentence_is_the_template_does_not_store_it_twice() {
    let fixture = fixture();
    let decision = fixtures::user_kept(fixture.project, PhotoId::new()).build(DecisionId::new(), 0);
    let id = fixture.explain.record(&decision).expect("it records");
    let stored = fixture
        .explain
        .decision(id)
        .expect("it reads back")
        .expect("it is there");
    let reason = stored.reasons.first().expect("a reason");
    assert_eq!(
        reason.text,
        aura_core::CullCode::UserKept.user_text(),
        "the sentence is rebuilt from the registry rather than read from the row"
    );
    assert_eq!(reason.evidence, Evidence::None);
}
