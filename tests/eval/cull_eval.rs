//! PHASE-12 section 10.1, as an ordinary `cargo test` so that a red gate is a red build.
//!
//! Seven criteria are named there and all seven are here, plus the four structural
//! properties this phase's ADR turned from promises into assertions.
//!
//! ## What these gates prove
//!
//! The **algorithms**. Every fixture in `aura_cull::fixtures` has a right answer by
//! construction: this wedding has exactly four photographs of the ring exchange, so a
//! gallery with none of them is a failure that can be asserted rather than judged.
//!
//! ## What they do not prove
//!
//! Anything about the **weights**. The sub-scores in the fixtures are authored numbers, and
//! even in production they come from phase 06, 09, 10 and 11 heads that all ship as
//! placeholders. That is condition C1 in the exit report, it is a Sev 2 trigger, and it
//! closes with phase 05's condition C10 rather than separately.
//!
//! This is the phase where a passing gate is most likely to be misread as "the culling is
//! good". It is not. It is "the culling does what it says it does".

use std::collections::BTreeSet;
use std::time::Instant;

use aura_core::contract::integrity::{ExposureVerdict, IntegrityFlags};
use aura_core::{Coverage, CullCode, CullMode, MustHave, SceneId};
use aura_cull::engine::{Engine, Request};
use aura_cull::fixtures::{self, Wedding};
use aura_cull::gather::veto_for;
use aura_cull::rules::RuleTable;
use aura_cull::weights::WeightTable;

/// Section 10.1: photographer agreement on keepers, Jaccard at moment level.
const AGREEMENT_GATE: f64 = 0.85;

/// Section 11: a slider move re-selects in two seconds.
const SLIDER_BUDGET_MS: u128 = 2_000;

/// Section 11: the selection passes over four thousand analysed frames.
const PASS_BUDGET_MS: u128 = 1_500;

fn tables() -> (WeightTable, RuleTable) {
    let weights = WeightTable::embedded().expect("the shipped weight table must load");
    let rules = RuleTable::embedded().expect("the shipped rule table must load");
    (weights, rules)
}

fn select(wedding: &Wedding, mode: CullMode, target: Option<u32>) -> aura_core::SelectionResult {
    let (weights, rules) = tables();
    let engine = Engine::new(&weights, &rules);
    let request = Request {
        mode,
        target,
        overrides: std::collections::BTreeMap::new(),
    };
    engine.select(wedding.input.clone(), &request).0
}

// ---------------------------------------------------------------------------
// The configuration is a product decision, and the loader says so
// ---------------------------------------------------------------------------

#[test]
fn both_shipped_tables_load_and_are_complete() {
    let (weights, rules) = tables();
    assert!(
        weights.unweighted().is_empty(),
        "every named scene needs its own weight row; missing {:?}",
        weights.unweighted()
    );
    assert_eq!(
        weights.rows(),
        SceneId::ALL.len() - 1,
        "the weight table must cover the frozen taxonomy exactly once"
    );
    assert_eq!(
        rules.rules().len(),
        MustHave::COUNT,
        "all twelve guarantees must be defined"
    );
}

#[test]
fn a_weight_row_may_not_put_taste_above_substance() {
    // ADR-0023 section 3 decided that taste breaks ties rather than overriding substance,
    // and this is the phase where that decision is spent. A row that asks for it is refused
    // rather than clamped.
    let text = r#"
version = 1
[neutral]
technical = 0.4
emotion = 0.25
composition = 0.2
prominence = 0.15
floor = 0.3
max_keepers = 2
rationale = "the documented fallback"
[[scene]]
id = "kiss"
technical = 0.2
emotion = 0.3
composition = 0.5
prominence = 0.1
floor = 0.3
max_keepers = 2
rationale = "framing above whether it worked"
[[mode]]
id = "conservative"
floor_delta = 0.0
keeper_delta = 0
target_scale = 1.0
rationale = "unchanged here"
[[mode]]
id = "balanced"
floor_delta = 0.0
keeper_delta = 0
target_scale = 1.0
rationale = "unchanged here"
[[mode]]
id = "aggressive"
floor_delta = 0.0
keeper_delta = 0
target_scale = 1.0
rationale = "unchanged here"
"#;
    let error = WeightTable::parse(text, "test").expect_err("must be refused");
    assert_eq!(error.code.0, "AURA-ML-5051");
}

#[test]
fn an_unknown_must_have_slug_is_refused_rather_than_defaulted() {
    // `MustHave` is the one vocabulary in this workspace with no `from_str_or_*`. A typo
    // that silently disabled the vows guarantee would be invisible until a customer found
    // it.
    assert!(MustHave::parse("vowss").is_none());
    assert!(MustHave::parse("vows").is_some());
}

// ---------------------------------------------------------------------------
// Section 6.1: the three hard vetoes
// ---------------------------------------------------------------------------

#[test]
fn the_closed_eye_veto_can_never_fire_on_a_kiss() {
    let (_, rules) = tables();
    let policy = rules.veto();
    assert!(
        !policy.posed_scenes.contains(&SceneId::Kiss),
        "the loader must refuse a table that lists the kiss as posed"
    );
    let verdict = veto_for(
        0.8,
        ExposureVerdict::Good,
        IntegrityFlags::EYES_CLOSED,
        SceneId::Kiss,
        policy.focus_floor,
        &policy.posed_scenes,
    );
    assert_eq!(
        verdict, None,
        "the kiss must never be vetoed for closed eyes"
    );
}

#[test]
fn phase_nine_s_own_exoneration_beats_the_closed_eye_veto() {
    let (_, rules) = tables();
    let policy = rules.veto();
    let with_ok = veto_for(
        0.8,
        ExposureVerdict::Good,
        IntegrityFlags::EYES_CLOSED.union(IntegrityFlags::EYES_CLOSED_OK),
        SceneId::CouplePortrait,
        policy.focus_floor,
        &policy.posed_scenes,
    );
    assert_eq!(with_ok, None, "EYES_CLOSED_OK is honoured, not re-decided");

    let without_ok = veto_for(
        0.8,
        ExposureVerdict::Good,
        IntegrityFlags::EYES_CLOSED,
        SceneId::CouplePortrait,
        policy.focus_floor,
        &policy.posed_scenes,
    );
    assert_eq!(without_ok, Some(CullCode::VetoEyesClosed));
}

#[test]
fn the_focus_veto_is_for_completely_out_of_focus_only() {
    let (_, rules) = tables();
    let policy = rules.veto();
    assert!(
        policy.focus_floor <= 0.2,
        "a veto at {} is a threshold pretending to be a fact",
        policy.focus_floor
    );
    assert_eq!(
        veto_for(
            0.35,
            ExposureVerdict::Good,
            IntegrityFlags::EMPTY,
            SceneId::Ceremony,
            policy.focus_floor,
            &policy.posed_scenes,
        ),
        None,
        "a merely soft frame loses on score, it is not vetoed"
    );
    assert_eq!(
        veto_for(
            0.02,
            ExposureVerdict::Good,
            IntegrityFlags::EMPTY,
            SceneId::Ceremony,
            policy.focus_floor,
            &policy.posed_scenes,
        ),
        Some(CullCode::VetoOutOfFocus)
    );
}

// ---------------------------------------------------------------------------
// Section 10.1, criterion 2: zero missed must-haves
// ---------------------------------------------------------------------------

#[test]
fn no_must_have_is_ever_missed_when_a_candidate_exists() {
    for wedding in fixtures::all() {
        for mode in CullMode::ALL {
            let result = select(&wedding, mode, None);
            for (rule, state) in &result.coverage.must_haves {
                if wedding.labels.must_have_candidates.contains(rule) {
                    assert!(
                        state.is_satisfied(),
                        "{} / {mode}: {rule} had candidates and came out {state}",
                        wedding.name
                    );
                } else {
                    assert_eq!(
                        *state,
                        Coverage::Missing,
                        "{} / {mode}: {rule} had no candidates and must be reported missing",
                        wedding.name
                    );
                }
            }
        }
    }
}

#[test]
fn a_wedding_with_no_cake_says_so_rather_than_inventing_one() {
    // The fixture that exists because a gate which only ever sees complete weddings never
    // checks that the product can say "nobody photographed this".
    let wedding = fixtures::sparse_elopement();
    let result = select(&wedding, CullMode::Balanced, None);
    let missing = result.coverage.missing();
    for rule in [
        MustHave::Cake,
        MustHave::FamilyFormals,
        MustHave::ReceptionEntrance,
        MustHave::Exit,
        MustHave::FirstDance,
        MustHave::FirstLook,
    ] {
        assert!(
            missing.contains(&rule),
            "{rule} must be reported missing on an elopement that had none"
        );
    }
    assert!(
        result
            .coverage
            .warnings
            .iter()
            .any(|warning| warning.contains("no candidates found")),
        "the report must say why, in words"
    );
}

// ---------------------------------------------------------------------------
// Section 10.1, criterion 6: aggressive mode still satisfies every rule
// ---------------------------------------------------------------------------

#[test]
fn aggressive_mode_cannot_drop_a_must_have() {
    for wedding in fixtures::all() {
        let tight = select(&wedding, CullMode::Aggressive, None);
        let generous = select(&wedding, CullMode::Conservative, None);
        assert!(
            tight.actual_count <= generous.actual_count,
            "{}: aggressive delivered {} and conservative {}",
            wedding.name,
            tight.actual_count,
            generous.actual_count
        );
        assert!(
            tight.coverage.satisfied_where_possible(
                &wedding
                    .labels
                    .must_have_candidates
                    .iter()
                    .copied()
                    .collect::<Vec<_>>()
            ),
            "{}: aggressive mode broke a guarantee",
            wedding.name
        );
    }
}

// ---------------------------------------------------------------------------
// Section 10.1, criterion 3: every close-family identity appears three times
// ---------------------------------------------------------------------------

#[test]
fn every_close_family_identity_is_in_the_gallery() {
    let (_, rules) = tables();
    let minimum = rules.identity().close_family_min;
    for wedding in fixtures::all() {
        let result = select(&wedding, CullMode::Balanced, None);
        for identity in &wedding.labels.close_family {
            let count = result
                .coverage
                .identity_coverage
                .iter()
                .find(|(id, _)| id == identity)
                .map_or(0, |(_, count)| *count);
            // The available frames are the ceiling: a person the photographer only took two
            // photographs of cannot appear three times, and the guard says so in a warning
            // rather than inventing one.
            let available = wedding
                .input
                .candidates
                .iter()
                .filter(|candidate| candidate.identities.contains(identity))
                .count() as u32;
            assert!(
                count >= minimum.min(available),
                "{}: an identity appears {count} times against a minimum of {minimum} \
                 with {available} available",
                wedding.name
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Section 10.1, criterion 1: photographer agreement
// ---------------------------------------------------------------------------

#[test]
fn photographer_agreement_meets_the_gate() {
    let mut worst = 1.0f64;
    for wedding in fixtures::all() {
        let result = select(&wedding, CullMode::Balanced, None);

        // Jaccard at *moment* level, which is what section 10.1 specifies: two editors who
        // pick different frames of the same instant agree about the wedding, and an
        // agreement measured frame by frame would call that a disagreement.
        let engine_moments: BTreeSet<_> = result
            .selected
            .iter()
            .filter_map(|keep| keep.moment_id)
            .collect();
        let human_moments: BTreeSet<_> = wedding
            .input
            .candidates
            .iter()
            .filter(|candidate| wedding.labels.keepers.contains(&candidate.image_id))
            .filter_map(|candidate| candidate.moment_id)
            .collect();

        let intersection = engine_moments.intersection(&human_moments).count() as f64;
        let union = engine_moments.union(&human_moments).count() as f64;
        let jaccard = if union == 0.0 {
            1.0
        } else {
            intersection / union
        };
        worst = worst.min(jaccard);
        assert!(
            jaccard >= AGREEMENT_GATE,
            "{}: moment-level agreement {jaccard:.3} is below the {AGREEMENT_GATE} gate",
            wedding.name
        );
    }
    println!("agreement: worst fixture {worst:.3} against a {AGREEMENT_GATE} gate");
}

// ---------------------------------------------------------------------------
// Section 10.1, criterion 4: determinism
// ---------------------------------------------------------------------------

#[test]
fn two_runs_produce_byte_identical_selections() {
    for wedding in fixtures::all() {
        let first = select(&wedding, CullMode::Balanced, None);
        let second = select(&wedding, CullMode::Balanced, None);
        assert_eq!(
            first.deterministic_hash, second.deterministic_hash,
            "{}: the hash identifies the question and the question did not change",
            wedding.name
        );
        assert_eq!(first.selected, second.selected, "{}", wedding.name);
        assert_eq!(first.rejected, second.rejected, "{}", wedding.name);
        assert_eq!(first.coverage, second.coverage, "{}", wedding.name);
    }
}

#[test]
fn the_hash_covers_the_question_and_not_the_answer() {
    let wedding = fixtures::christian_daylight();
    let balanced = select(&wedding, CullMode::Balanced, None);
    let aggressive = select(&wedding, CullMode::Aggressive, None);
    assert_ne!(
        balanced.deterministic_hash, aggressive.deterministic_hash,
        "a different mode is a different question"
    );

    let same_mode_other_target = select(&wedding, CullMode::Balanced, Some(400));
    assert_ne!(
        balanced.deterministic_hash, same_mode_other_target.deterministic_hash,
        "a different target is a different question"
    );
}

// ---------------------------------------------------------------------------
// Section 10.1, criterion 5: the slider
// ---------------------------------------------------------------------------

#[test]
fn the_slider_re_selects_inside_its_budget_and_never_breaks_coverage() {
    let wedding = fixtures::nepali_reception();
    let candidates: Vec<MustHave> = wedding
        .labels
        .must_have_candidates
        .iter()
        .copied()
        .collect();
    let mut worst_ms = 0u128;
    for target in [500u32, 700, 900, 1_100, 1_200] {
        let started = Instant::now();
        let result = select(&wedding, CullMode::Balanced, Some(target));
        let elapsed = started.elapsed().as_millis();
        worst_ms = worst_ms.max(elapsed);
        assert!(
            elapsed <= SLIDER_BUDGET_MS,
            "a slider move at {target} took {elapsed} ms against a {SLIDER_BUDGET_MS} ms budget"
        );
        assert!(
            result.coverage.satisfied_where_possible(&candidates),
            "shrinking to {target} broke a guarantee"
        );
    }
    println!("slider: worst move {worst_ms} ms against a {SLIDER_BUDGET_MS} ms budget");
}

#[test]
fn the_selection_passes_stay_inside_their_budget() {
    // Section 11 budgets 1.5 s for the passes over four thousand analysed frames. The
    // largest fixture is smaller than that, so this is a headroom check rather than the
    // budget itself - `crates/aura-perf` asserts the real figure against a real catalog.
    let wedding = fixtures::hindu_night();
    let started = Instant::now();
    let result = select(&wedding, CullMode::Balanced, None);
    let elapsed = started.elapsed().as_millis();
    assert!(
        elapsed <= PASS_BUDGET_MS,
        "{} frames took {elapsed} ms against a {PASS_BUDGET_MS} ms budget",
        wedding.input.candidates.len()
    );
    println!(
        "passes: {} frames in {elapsed} ms, {} delivered",
        wedding.input.candidates.len(),
        result.actual_count
    );
}

// ---------------------------------------------------------------------------
// Section 10.1, criterion 7: everything is explained
// ---------------------------------------------------------------------------

#[test]
fn every_decision_in_both_directions_carries_a_reason() {
    for wedding in fixtures::all() {
        for mode in CullMode::ALL {
            let result = select(&wedding, mode, None);
            assert!(
                result.is_explained(),
                "{} / {mode}: a decision has no reason",
                wedding.name
            );
            for keep in &result.selected {
                assert!(
                    keep.reasons.len() <= aura_core::Selected::MAX_REASONS,
                    "{}: a keeper carries {} reasons",
                    wedding.name,
                    keep.reasons.len()
                );
            }
            for drop in &result.rejected {
                assert!(drop.reasons.len() <= aura_core::Rejected::MAX_REASONS);
            }
        }
    }
}

#[test]
fn a_moment_with_alternatives_offers_a_runner_up() {
    let wedding = fixtures::christian_daylight();
    let result = select(&wedding, CullMode::Balanced, None);
    let with_moment: Vec<_> = result
        .selected
        .iter()
        .filter(|keep| keep.moment_id.is_some())
        .collect();
    let offered = with_moment
        .iter()
        .filter(|keep| keep.runner_up.is_some())
        .count();
    assert!(
        offered * 2 > with_moment.len(),
        "only {offered} of {} keepers offer an alternative to compare",
        with_moment.len()
    );
}

#[test]
fn a_rejected_peak_says_so_out_loud() {
    // Section 6.2: "guarantee the moment's peak frame is either selected or explicitly
    // rejected with a reason - never silently dropped."
    let mut checked = 0usize;
    for wedding in fixtures::all() {
        let result = select(&wedding, CullMode::Aggressive, None);
        let peaks: BTreeSet<_> = wedding
            .input
            .candidates
            .iter()
            .filter(|candidate| candidate.is_peak && !candidate.peak_flat)
            .map(|candidate| candidate.image_id)
            .collect();
        for drop in &result.rejected {
            if peaks.contains(&drop.image_id) {
                assert!(
                    drop.was_peak(),
                    "{}: a peak was dropped without saying so",
                    wedding.name
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 0,
        "the fixtures must exercise at least one dropped peak"
    );
}

#[test]
fn an_unanalysed_frame_is_explained_rather_than_rejected() {
    // "We have not checked this yet" and "this is not good enough" must never look the same
    // in a gallery. `AURA-ML-5050`.
    let mut wedding = fixtures::sparse_elopement();
    for candidate in wedding.input.candidates.iter_mut().take(20) {
        candidate.analysed = false;
    }
    let result = select(&wedding, CullMode::Balanced, None);
    let unexplained = result
        .rejected
        .iter()
        .filter(|drop| {
            drop.reasons
                .iter()
                .any(|reason| reason.code == CullCode::NotAnalysed)
        })
        .count();
    assert_eq!(unexplained, 20, "every unanalysed frame must say so");
}

// ---------------------------------------------------------------------------
// The structural properties ADR-0025 turned into assertions
// ---------------------------------------------------------------------------

#[test]
fn a_forced_reject_can_degrade_a_guarantee_but_never_silently() {
    let wedding = fixtures::sparse_elopement();
    let (weights, rules) = tables();
    let engine = Engine::new(&weights, &rules);

    // Remove every photograph of the kiss by hand. The rule must degrade and say why; it
    // must not be quietly overruled in either direction.
    let mut overrides = std::collections::BTreeMap::new();
    for candidate in &wedding.input.candidates {
        if candidate.scene == SceneId::Kiss {
            overrides.insert(candidate.image_id, false);
        }
    }
    let request = Request {
        mode: CullMode::Balanced,
        target: None,
        overrides,
    };
    let result = engine.select(wedding.input.clone(), &request).0;
    let kiss = result
        .coverage
        .must_haves
        .iter()
        .find(|(rule, _)| *rule == MustHave::Kiss)
        .map(|(_, state)| *state);
    assert_eq!(
        kiss,
        Some(Coverage::Missing),
        "a guarantee whose every candidate was removed by hand is missing, not covered"
    );
    assert!(
        result
            .coverage
            .warnings
            .iter()
            .any(|warning| warning.contains("removed by hand")),
        "the report must name the override"
    );
}

#[test]
fn the_keeper_rate_lands_in_the_band_section_six_four_states() {
    for wedding in fixtures::all() {
        let result = select(&wedding, CullMode::Balanced, None);
        let rate = result.keeper_rate();
        assert!(
            (0.15..=0.55).contains(&rate),
            "{}: delivered {:.1} % of the wedding",
            wedding.name,
            rate * 100.0
        );
    }
}

#[test]
fn a_near_identical_pair_never_both_reach_the_gallery() {
    let mut wedding = fixtures::christian_daylight();
    // Put the first two frames of the first moment into one duplicate group.
    let first_moment = wedding.input.candidates[0].moment_id;
    let pair: Vec<_> = wedding
        .input
        .candidates
        .iter()
        .filter(|candidate| candidate.moment_id == first_moment)
        .take(2)
        .map(|candidate| candidate.image_id)
        .collect();
    assert_eq!(pair.len(), 2);
    for candidate in &mut wedding.input.candidates {
        if pair.contains(&candidate.image_id) {
            candidate.duplicate_group = Some(0);
        }
    }
    if let Some(moment) = wedding
        .input
        .moments
        .iter_mut()
        .find(|moment| Some(moment.id) == first_moment)
    {
        moment.duplicate_groups = vec![pair.clone()];
        moment.diversity = 1.0;
        moment.significance = 1.0;
    }

    let result = select(&wedding, CullMode::Conservative, None);
    let delivered = result
        .selected
        .iter()
        .filter(|keep| pair.contains(&keep.image_id))
        .count();
    assert!(
        delivered <= 1,
        "both halves of a near-identical pair reached the gallery"
    );
}

// ---------------------------------------------------------------------------
// The documentation is generated against the vocabulary, and a test says so
// ---------------------------------------------------------------------------

#[test]
fn every_reason_code_is_documented() {
    // Section 9 gives DOC "write 'How AURA culls', the coverage guarantee page and the mode
    // guide", which is only a finishable job if the vocabulary is enumerable. Phase 11 wrote
    // this test for `CompositionCode`; this is the same test one phase later, and it is the
    // reason a new reason code cannot ship without a sentence a photographer can read.
    let doc = std::fs::read_to_string("../../docs/how-aura-culls.md")
        .expect("docs/how-aura-culls.md must exist");
    for code in CullCode::ALL {
        assert!(
            doc.contains(code.as_str()),
            "`{}` is not documented in docs/how-aura-culls.md",
            code.as_str()
        );
    }
    for rule in MustHave::ALL {
        assert!(
            doc.contains(rule.title()),
            "the `{}` guarantee is not documented",
            rule.as_str()
        );
    }
    // The mode guide names the modes as a photographer sees them - "Conservative" - rather
    // than as slugs, so the comparison is case-insensitive here and exact everywhere else.
    let lowered = doc.to_lowercase();
    for mode in CullMode::ALL {
        assert!(
            lowered.contains(mode.as_str()),
            "the `{}` mode is not documented",
            mode.as_str()
        );
    }
}

#[test]
fn every_reason_code_has_a_distinct_sentence() {
    // A vocabulary with two codes that say the same thing is a vocabulary with one code and
    // a bug. This catches a copy-paste in `user_text` before it reaches a panel.
    let mut seen: BTreeSet<&'static str> = BTreeSet::new();
    for code in CullCode::ALL {
        assert!(
            seen.insert(code.user_text()),
            "`{}` repeats another code's sentence",
            code.as_str()
        );
        assert!(
            code.user_text().len() > 12,
            "`{}` has no real sentence",
            code.as_str()
        );
    }
}
