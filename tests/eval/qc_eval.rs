//! The phase 27 quality gates. PHASE-27 section 10.1.
//!
//! Run as an ordinary test so a red gate is a red build, beside the phase 05 to 26 harnesses.
//!
//! ## What these gates prove, and what they do not
//!
//! Every fixture here is a set of **readings** whose numbers this file chose. `fixtures::defects`
//! authors twenty frames each with exactly one thing wrong, `fixtures::clean_gallery` authors frames
//! with nothing wrong, and the gates measure whether the checks find the first set and stay quiet on
//! the second.
//!
//! That proves the thresholds, the triage ordering, the loop's improvement test, the collateral
//! re-check, the replacement filter and the store. **It proves nothing about a wedding photograph**,
//! because the numbers phases 09 to 26 would produce on a real frame come from placeholder heads -
//! phase 05's condition C10 and the seven that hang off it. That is condition C1 of this phase's
//! exit report and it closes with C10 rather than separately.
//!
//! Section 10.1's seventh gate - "photographer agreement with tickets >= 80 % in the dogfood
//! study" - is not here and cannot be: it needs ten real weddings and a person. It is condition C3.

use aura_core::contract::qc::{
    QcCategory, QcCode, QcTicket, Remedy, TicketStatus, MAX_ROUNDS, MIN_GAIN_SHARE,
};
use aura_core::contract::scene::SceneId;
use aura_qc::checks::{self, Frame};
use aura_qc::policy::Thresholds;
use aura_qc::reedit::{realised_share, Loop, Remediator};
use aura_qc::replace::{self, CandidateMetric, CoverageEffect, Verdict};
use aura_qc::{fixtures, remedy, triage};

/// Section 10.1: at least this share of injected defects must be caught.
const DETECTION_FLOOR: f32 = 0.90;

/// Section 10.1: at most this share of clean frames may produce a ticket.
const FALSE_TICKET_CEILING: f32 = 0.08;

/// Section 10.1: at least this share of accepted tickets must resolve within two rounds.
const AUTOFIX_FLOOR: f32 = 0.85;

// ---------------------------------------------------------------------------
// Gate 1 - detection
// ---------------------------------------------------------------------------

#[test]
fn gate_1_catches_at_least_ninety_per_cent_of_injected_defects() {
    let thresholds = Thresholds::shipped().expect("the shipped thresholds table loads");
    let defects = fixtures::defects();
    let total = defects.len();
    let mut caught = 0usize;
    let mut missed = Vec::new();

    for defect in &defects {
        let findings = checks::findings_for(&defect.frame, &thresholds);
        if findings.iter().any(|finding| finding.code == defect.code) {
            caught += 1;
        } else {
            missed.push(defect.name);
        }
    }

    let rate = caught as f32 / total as f32;
    println!("gate 1: detection {caught}/{total} = {rate:.3} (floor {DETECTION_FLOOR:.2})");
    assert!(
        rate >= DETECTION_FLOOR,
        "detection rate {rate:.3} below {DETECTION_FLOOR}; missed: {missed:?}"
    );
}

#[test]
fn gate_1b_names_the_right_code_and_not_merely_the_right_category() {
    // A build that reported a skin drift as a guard excursion would pass a category-level gate and
    // send every photographer to the wrong remedy. The two have opposite root causes: one is the
    // light and one is the grade.
    let thresholds = Thresholds::shipped().expect("the shipped table loads");
    for defect in fixtures::defects() {
        let findings = checks::findings_for(&defect.frame, &thresholds);
        let codes: Vec<QcCode> = findings.iter().map(|finding| finding.code).collect();
        assert!(
            codes.contains(&defect.code),
            "'{}' reported {codes:?} rather than {}",
            defect.name,
            defect.code
        );
    }
}

// ---------------------------------------------------------------------------
// Gate 2 - false tickets
// ---------------------------------------------------------------------------

#[test]
fn gate_2_stays_quiet_on_a_clean_gallery() {
    let thresholds = Thresholds::shipped().expect("the shipped thresholds table loads");
    let gallery = fixtures::clean_gallery(200);
    let total = gallery.len();
    let mut noisy = 0usize;
    let mut examples = Vec::new();

    for frame in &gallery {
        let findings = checks::findings_for(frame, &thresholds);
        if !findings.is_empty() {
            noisy += 1;
            if examples.len() < 4 {
                examples.push((
                    frame.scene,
                    findings.iter().map(|f| f.code).collect::<Vec<_>>(),
                ));
            }
        }
    }

    let rate = noisy as f32 / total as f32;
    println!(
        "gate 2: false tickets {noisy}/{total} = {rate:.3} (ceiling {FALSE_TICKET_CEILING:.2})"
    );
    assert!(
        rate <= FALSE_TICKET_CEILING,
        "false-ticket rate {rate:.3} above {FALSE_TICKET_CEILING}; examples: {examples:?}"
    );
}

// ---------------------------------------------------------------------------
// Gate 3 - auto-fix within two rounds
// ---------------------------------------------------------------------------

/// A remediator that moves a frame toward health by a fixed share of the gap each round.
///
/// Not a stub that returns a perfect frame: a remediator that always succeeded would make the loop's
/// improvement test unfalsifiable. This one realises `share` of the distance to a healthy reading,
/// so the gate measures what the loop does with a *partial* repair - which is the case the whole
/// `MIN_GAIN_SHARE` argument is about.
#[derive(Debug)]
struct Improver {
    healthy: Frame,
    current: Frame,
    share: f32,
    applies: usize,
}

impl Improver {
    fn new(defective: Frame, healthy: Frame, share: f32) -> Self {
        Self {
            healthy,
            current: defective,
            share,
            applies: 0,
        }
    }

    /// Move each reading `share` of the way from the current value to the healthy one.
    fn step(&mut self) {
        let share = self.share;
        let healthy = self.healthy.clone();
        if let (Some(now), Some(good)) = (self.current.node.as_mut(), healthy.node.as_ref()) {
            now.frame_cct_k += (good.frame_cct_k - now.frame_cct_k) * share;
            now.frame_tint += (good.frame_tint - now.frame_tint) * share;
            now.frame_luma += (good.frame_luma - now.frame_luma) * share;
            if let (Some(mut now_sig), Some(good_sig)) = (now.frame_signature, good.frame_signature)
            {
                for (slot, target) in now_sig.iter_mut().zip(good_sig.iter()) {
                    *slot += (target - *slot) * share;
                }
                now.frame_signature = Some(now_sig);
            }
        }
        if let (Some(now), Some(good)) = (self.current.skin.as_mut(), healthy.skin.as_ref()) {
            for (index, entry) in now.per_identity_de00.iter_mut().enumerate() {
                let target = good
                    .per_identity_de00
                    .get(index)
                    .map_or(0.0, |(_, value)| *value);
                entry.1 += (target - entry.1) * share;
            }
            now.guard_hue_shift_deg += (good.guard_hue_shift_deg - now.guard_hue_shift_deg) * share;
            now.guard_chroma_change += (good.guard_chroma_change - now.guard_chroma_change) * share;
        }
        if let (Some(now), Some(good)) = (self.current.exposure.as_mut(), healthy.exposure.as_ref())
        {
            if let (Some(from), Some(to)) = (now.subject_luma, good.subject_luma) {
                now.subject_luma = Some(from + (to - from) * share);
            }
            now.clip_hi_after += (good.clip_hi_after - now.clip_hi_after) * share;
            now.clip_lo_after += (good.clip_lo_after - now.clip_lo_after) * share;
            if let (Some(from), Some(to)) = (now.shadow_headroom, good.shadow_headroom) {
                now.shadow_headroom = Some(from + (to - from) * share);
            }
        }
        if let (Some(now), Some(good)) =
            (self.current.sharpness.as_mut(), healthy.sharpness.as_ref())
        {
            now.relative_sharpness += (good.relative_sharpness - now.relative_sharpness) * share;
            now.subject_sharpness += (good.subject_sharpness - now.subject_sharpness) * share;
            now.ringing += (good.ringing - now.ringing) * share;
            now.texture_retention += (good.texture_retention - now.texture_retention) * share;
        }
        if let (Some(now), Some(good)) = (self.current.retouch.as_mut(), healthy.retouch.as_ref()) {
            now.texture_band_ratio += (good.texture_band_ratio - now.texture_band_ratio) * share;
            now.teeth_excursion += (good.teeth_excursion - now.teeth_excursion) * share;
            now.catchlight_ratio += (good.catchlight_ratio - now.catchlight_ratio) * share;
            now.hair_energy_ratio += (good.hair_energy_ratio - now.hair_energy_ratio) * share;
            now.allowance_used += (good.allowance_used - now.allowance_used) * share;
        }
        if let (Some(now), Some(good)) = (self.current.mask.as_mut(), healthy.mask.as_ref()) {
            for (region, target) in now.regions.iter_mut().zip(good.regions.iter()) {
                region.applied_strength +=
                    (target.applied_strength - region.applied_strength) * share;
            }
        }
    }
}

impl Remediator for Improver {
    fn apply(
        &mut self,
        _image: aura_core::contract::qc::ImageId,
        _remedy: &Remedy,
    ) -> Result<Frame, aura_core::AuraError> {
        self.applies += 1;
        self.step();
        Ok(self.current.clone())
    }

    fn revert(
        &mut self,
        _image: aura_core::contract::qc::ImageId,
        _remedy: &Remedy,
    ) -> Result<Frame, aura_core::AuraError> {
        Ok(self.current.clone())
    }
}

#[test]
fn gate_3_resolves_most_mechanically_fixable_findings_within_two_rounds() {
    let thresholds = Thresholds::reference();
    let reedit = Loop::new(&thresholds);
    let project = aura_core::ProjectId::new();

    // The findings a remedy can address at all. The five categories whose remedy is `Escalate` by
    // construction - because `expected_gain` is zero - are excluded from the denominator, which is
    // what section 10.1's "of accepted tickets" means: a finding the product never claimed it could
    // fix is not a fix it failed to make.
    let mut fixable = 0usize;
    let mut resolved = 0usize;
    let mut stubborn = Vec::new();

    for defect in fixtures::defects() {
        let healthy = fixtures::healthy(defect.frame.scene);
        let findings = checks::findings_for(&defect.frame, &thresholds);
        let Some(finding) = findings
            .into_iter()
            .find(|finding| finding.code == defect.code)
        else {
            continue;
        };
        let proposed = remedy::propose(&finding, &defect.frame, 0);
        if !proposed.mutates() {
            continue;
        }
        fixable += 1;

        let mut ticket =
            aura_qc::ticket::from_finding(project, &defect.frame, finding, proposed, 0);
        let mut frame = defect.frame.clone();
        let mut improver = Improver::new(defect.frame.clone(), healthy, 0.85);
        let mut closed = false;

        for _ in 0..MAX_ROUNDS {
            let remedy = remedy::propose(
                &checks::Finding::new(
                    ticket.category,
                    ticket.code,
                    ticket.deviation,
                    ticket.threshold,
                    ticket.expected_gain,
                    ticket.confidence,
                ),
                &frame,
                ticket.round,
            );
            let Ok(validated) = remedy::validate(remedy, &ticket, &frame, thresholds.loop_policy())
            else {
                break;
            };
            let outcome = reedit
                .run(&ticket, &frame, &validated, &mut improver, 0, 10)
                .expect("the improver cannot fail");
            frame = outcome.frame.clone();
            ticket.round = outcome.round.round;
            ticket.deviation = outcome.deviation;
            ticket.status = outcome.status;
            if outcome.status == TicketStatus::Fixed {
                closed = true;
                break;
            }
            if outcome.status == TicketStatus::Escalated {
                break;
            }
        }

        if closed {
            resolved += 1;
        } else {
            stubborn.push((defect.name, ticket.status, ticket.deviation));
        }
    }

    let rate = resolved as f32 / fixable as f32;
    println!("gate 3: auto-fix {resolved}/{fixable} = {rate:.3} (floor {AUTOFIX_FLOOR:.2})");
    assert!(
        rate >= AUTOFIX_FLOOR,
        "auto-fix rate {rate:.3} below {AUTOFIX_FLOOR}; unresolved: {stubborn:?}"
    );
}

// ---------------------------------------------------------------------------
// Gate 4 - no regression
// ---------------------------------------------------------------------------

/// A remediator that fixes its target and breaks something else.
#[derive(Debug)]
struct Vandal {
    after: Frame,
}

impl Remediator for Vandal {
    fn apply(
        &mut self,
        _image: aura_core::contract::qc::ImageId,
        _remedy: &Remedy,
    ) -> Result<Frame, aura_core::AuraError> {
        Ok(self.after.clone())
    }

    fn revert(
        &mut self,
        _image: aura_core::contract::qc::ImageId,
        _remedy: &Remedy,
    ) -> Result<Frame, aura_core::AuraError> {
        Ok(Frame::default())
    }
}

#[test]
fn gate_4_a_remedy_that_breaks_another_check_is_reverted() {
    let thresholds = Thresholds::reference();
    let reedit = Loop::new(&thresholds);
    let project = aura_core::ProjectId::new();

    // A consistency finding, remediated by a grade re-solve - which `Remedy::collateral_checks`
    // says can reach exposure. The remedy fixes the colour and wrecks the exposure.
    let mut before = fixtures::healthy(SceneId::Ceremony);
    if let Some(node) = before.node.as_mut() {
        node.frame_cct_k = 5200.0 + 4.0 * node.cct_tol;
    }
    let mut after = fixtures::healthy(SceneId::Ceremony);
    after.image_id = before.image_id;
    if let Some(exposure) = after.exposure.as_mut() {
        exposure.subject_luma = Some(0.95);
        exposure.clip_hi_before = 0.0;
        exposure.clip_hi_after = 0.8;
    }

    let finding = checks::findings_for(&before, &thresholds)
        .into_iter()
        .find(|finding| finding.category == QcCategory::Consistency)
        .expect("a consistency finding");
    let ticket = aura_qc::ticket::from_finding(
        project,
        &before,
        finding,
        Remedy::ResolveParam {
            target: aura_core::contract::qc::SolveTarget::Grade,
            constraint: "hold the exposure".into(),
        },
        0,
    );

    let mut vandal = Vandal { after };
    let outcome = reedit
        .run(&ticket, &before, &ticket.remedy, &mut vandal, 0, 10)
        .expect("the vandal cannot fail");

    println!(
        "gate 4: collateral {:.3} in {:?}, kept = {}",
        outcome.round.collateral, outcome.round.collateral_category, outcome.round.kept
    );
    assert!(
        !outcome.round.kept,
        "a remedy that broke another check was kept"
    );
    assert_eq!(outcome.round.outcome, QcCode::CollateralDamage);
    assert_eq!(
        outcome.round.collateral_category,
        Some(QcCategory::Exposure)
    );
    assert_eq!(outcome.status, TicketStatus::Escalated);
}

// ---------------------------------------------------------------------------
// Gate 5 - replacement never breaks coverage
// ---------------------------------------------------------------------------

#[test]
fn gate_5_a_replacement_never_breaks_coverage() {
    let thresholds = Thresholds::reference();
    let project = aura_core::ProjectId::new();
    let mut frame = fixtures::healthy(SceneId::FamilyPortrait);
    frame.runner_up = Some(aura_core::contract::qc::ImageId::new());
    frame.coverage_protected = true;
    if let Some(sharp) = frame.sharpness.as_mut() {
        sharp.relative_sharpness = 0.02;
        sharp.subject_sharpness = 0.02;
    }
    let finding = checks::findings_for(&frame, &thresholds)
        .into_iter()
        .find(|finding| finding.category == QcCategory::Sharpness)
        .expect("a sharpness finding");
    let mut ticket = aura_qc::ticket::from_finding(
        project,
        &frame,
        finding,
        Remedy::Escalate { note: "n".into() },
        0,
    );
    // Maximum confidence, so nothing but the coverage filter can refuse it.
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
    let verdict = replace::consider(&ticket, &frame, perfect, breaks, thresholds.loop_policy());
    println!("gate 5: a perfect candidate that breaks coverage returns {verdict:?}");
    assert_eq!(verdict, Verdict::Refuse(QcCode::ReplacementBreaksCoverage));
    assert_eq!(verdict.accepted(), None);

    // And the same candidate is accepted when it carries the guarantee.
    let holds = CoverageEffect {
        replacement_covers_same: true,
        ..breaks
    };
    assert!(
        replace::consider(&ticket, &frame, perfect, holds, thresholds.loop_policy())
            .accepted()
            .is_some()
    );
}

// ---------------------------------------------------------------------------
// Gate 6 - loop bounds and no thrashing
// ---------------------------------------------------------------------------

/// A remediator that oscillates: every application makes the metric worse.
#[derive(Debug, Default)]
struct Thrash {
    applies: usize,
    reverts: usize,
}

impl Remediator for Thrash {
    fn apply(
        &mut self,
        _image: aura_core::contract::qc::ImageId,
        _remedy: &Remedy,
    ) -> Result<Frame, aura_core::AuraError> {
        self.applies += 1;
        let mut worse = fixtures::healthy(SceneId::CouplePortrait);
        if let Some(sharp) = worse.sharpness.as_mut() {
            sharp.relative_sharpness = 0.0;
            sharp.subject_sharpness = 0.0;
        }
        Ok(worse)
    }

    fn revert(
        &mut self,
        _image: aura_core::contract::qc::ImageId,
        _remedy: &Remedy,
    ) -> Result<Frame, aura_core::AuraError> {
        self.reverts += 1;
        let mut back = fixtures::healthy(SceneId::CouplePortrait);
        if let Some(sharp) = back.sharpness.as_mut() {
            sharp.relative_sharpness = 0.02;
            sharp.subject_sharpness = 0.02;
        }
        Ok(back)
    }
}

#[test]
fn gate_6_the_loop_cannot_thrash() {
    let thresholds = Thresholds::reference();
    let reedit = Loop::new(&thresholds);
    let project = aura_core::ProjectId::new();

    let mut frame = fixtures::healthy(SceneId::CouplePortrait);
    if let Some(sharp) = frame.sharpness.as_mut() {
        sharp.relative_sharpness = 0.02;
        sharp.subject_sharpness = 0.02;
    }
    let finding = checks::findings_for(&frame, &thresholds)
        .into_iter()
        .find(|finding| finding.category == QcCategory::Sharpness)
        .expect("a sharpness finding");
    let mut ticket = aura_qc::ticket::from_finding(
        project,
        &frame,
        finding,
        Remedy::ResolveParam {
            target: aura_core::contract::qc::SolveTarget::Restoration,
            constraint: "sharpen".into(),
        },
        0,
    );

    let mut thrash = Thrash::default();
    let mut rounds = 0usize;
    let mut tickets = vec![ticket.clone()];
    while let Some(next) = triage::next(&tickets).cloned() {
        let outcome = reedit
            .run(&next, &frame, &next.remedy, &mut thrash, 0, 10)
            .expect("the thrasher cannot fail");
        frame = outcome.frame.clone();
        ticket.round = outcome.round.round;
        ticket.status = outcome.status;
        tickets = vec![ticket.clone()];
        rounds += 1;
        assert!(rounds <= 8, "the loop is not terminating");
    }

    println!(
        "gate 6: {rounds} round(s), {} applies, {} reverts, final status {}",
        thrash.applies, thrash.reverts, ticket.status
    );
    // One round: the first remedy made it worse, was reverted, and a reverted round escalates.
    // Section 6.3 is explicit that there is no second attempt at a remedy that did not work.
    assert_eq!(rounds, 1);
    assert_eq!(thrash.reverts, 1);
    assert_eq!(ticket.status, TicketStatus::Escalated);
    assert!(ticket.round <= MAX_ROUNDS);
}

#[test]
fn gate_6b_no_ticket_ever_exceeds_two_rounds() {
    // The bound counts attempts rather than successes. A loop that only counted kept rounds would
    // try forever on a frame nothing helps, which is exactly the frame the bound exists for.
    for round in 0..=(MAX_ROUNDS + 2) {
        let mut ticket = sample_ticket();
        ticket.round = round;
        assert_eq!(ticket.may_retry(), round < MAX_ROUNDS);
    }
}

fn sample_ticket() -> QcTicket {
    let frame = fixtures::healthy(SceneId::Ceremony);
    let finding = checks::Finding::new(QcCategory::Skin, QcCode::SkinDrift, 4.0, 2.0, 1.0, 0.9);
    aura_qc::ticket::from_finding(
        aura_core::ProjectId::new(),
        &frame,
        finding,
        Remedy::Escalate { note: "n".into() },
        0,
    )
}

// ---------------------------------------------------------------------------
// Gate 7 - the planner is always schema-valid and policy-validated
// ---------------------------------------------------------------------------

#[test]
fn gate_7_a_plan_is_validated_before_anything_can_act_on_it() {
    use aura_cloud::contract::cloud::{CloudTask, Validate};
    use aura_qc::planner::{ProposedStep, QcPlanOutput, QcPlanner};

    // Every malformed answer a model could give, and each one refused with a sentence the repair
    // retry can send back.
    let malformed = [
        (
            QcPlanOutput {
                plan: vec![ProposedStep {
                    remedy: "delete_photograph".into(),
                    target: "x".into(),
                    magnitude: None,
                    reason: "because".into(),
                }],
                root_cause: None,
                confidence: 0.9,
            },
            "five allowed remedies",
        ),
        (
            QcPlanOutput {
                plan: vec![ProposedStep {
                    remedy: "reduce_strength".into(),
                    target: "retouch".into(),
                    magnitude: Some(3.0),
                    reason: "because".into(),
                }],
                root_cause: None,
                confidence: 0.9,
            },
            "may only reduce",
        ),
        (
            QcPlanOutput {
                plan: Vec::new(),
                root_cause: None,
                confidence: 2.0,
            },
            "between 0 and 1",
        ),
    ];
    for (answer, expected) in malformed {
        let err = answer.validate().expect_err("a malformed plan is refused");
        assert!(err.contains(expected), "got: {err}");
    }

    // And the offline path is fully functional: the fallback validates, is not actionable, and
    // leaves the image with its mechanical triage.
    let task = QcPlanner::default();
    let input = aura_qc::planner::QcPlanInput {
        image_ref: "img".into(),
        scene: "ceremony".into(),
        findings: Vec::new(),
        recipe_summary: Vec::new(),
        node_stats: Vec::new(),
        has_runner_up: false,
        must_have: false,
        crops_hash: "abc".into(),
    };
    let offline = task
        .local_fallback(&input)
        .expect("the fallback cannot fail");
    assert!(offline.validate().is_ok());
    assert!(offline.actionable().is_empty());
    println!("gate 7: three malformed plans refused; the offline path escalates");
}

// ---------------------------------------------------------------------------
// Gate 8 - triage works root causes first
// ---------------------------------------------------------------------------

#[test]
fn gate_8_the_root_cause_is_remediated_before_its_symptoms() {
    // Section 7's rule: "if white balance is wrong, do not reduce retouch strength". The fixture is
    // a frame whose colour is out and whose skin and retouch both read badly as a consequence.
    let thresholds = Thresholds::reference();
    let project = aura_core::ProjectId::new();
    let (_name, frame) = fixtures::multi_symptom()
        .into_iter()
        .next()
        .expect("the first multi-symptom fixture");

    let tickets: Vec<QcTicket> = checks::findings_for(&frame, &thresholds)
        .into_iter()
        .map(|finding| {
            let proposed = remedy::propose(&finding, &frame, 0);
            aura_qc::ticket::from_finding(project, &frame, finding, proposed, 0)
        })
        .collect();

    let ordered = triage::order(&tickets);
    println!(
        "gate 8: order = {:?}",
        ordered.iter().map(|t| t.category).collect::<Vec<_>>()
    );
    assert!(ordered.len() >= 2);
    assert_eq!(ordered[0].category, QcCategory::Consistency);
    // And the planner is asked, because three or more findings on one frame is section 7's trigger.
    assert!(triage::needs_planner(&tickets));
}

// ---------------------------------------------------------------------------
// Gate 9 - improvement is measured against what the ticket opened with
// ---------------------------------------------------------------------------

#[test]
fn gate_9_a_partial_repair_on_a_hard_frame_is_kept() {
    // ADR-0055 section 4, as a gate. A ticket at 4.2 against a 2.5 threshold remediated to 3.9 has
    // improved and still fails; a build that kept only what *passes* would throw away every partial
    // repair on the hardest frames, which are the frames a photographer most wants helped.
    let realised = realised_share(4.2, 3.9, 0.5);
    println!("gate 9: realised {realised:.3} of a 0.5 prediction (floor {MIN_GAIN_SHARE})");
    assert!(realised >= MIN_GAIN_SHARE);

    // And a remedy that promised nothing cannot be half-realised, so it reverts.
    assert_eq!(realised_share(4.2, 1.0, 0.0), 0.0);
}

// ---------------------------------------------------------------------------
// The conditions, printed on every run
// ---------------------------------------------------------------------------

#[test]
fn zzz_the_conditions_these_gates_run_under() {
    println!();
    println!("PHASE-27 gate conditions - read these before quoting any number above.");
    println!();
    println!(
        "C1 (Sev 2): every fixture here is a set of readings this file chose. The numbers phases \
         09 to 26 would produce on a real photograph come from placeholder heads, so nothing above \
         is a claim about a wedding. Closes with phase 05's C10."
    );
    println!(
        "C2 (Sev 2): section 10.1's photographer-agreement study did not happen. The headline KPI \
         of this phase - do photographers agree with the tickets - is unmeasured, and the \
         false-ticket rate above is measured against frames this file authored as clean rather \
         than against a photographer's judgement."
    );
    println!(
        "C3: no defect-detection model ships. `DETECTOR_TRAINED` is false and every check is a \
         measurement against another phase's stored number. That is the deliberate choice - a \
         measurement finds fewer problems rather than inventing them - and it means the detection \
         rate above is a property of thresholds rather than of a model."
    );
    println!(
        "C4: the planner has never reached a provider in this repository. Gate 7 proves the schema \
         refuses malformed answers and the offline path works; no recorded cassette of a real \
         reasoning-tier answer exists."
    );
    println!();
    // A constant today, and that is the point: the day somebody trains a detector this line stops
    // compiling as written and they have to come back here and re-read the four conditions above.
    #[allow(clippy::assertions_on_constants)]
    {
        assert!(!aura_qc::DETECTOR_TRAINED, "this build ships no detector");
    }
}
