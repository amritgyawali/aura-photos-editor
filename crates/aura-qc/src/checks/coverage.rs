//! Must-have rules and identity minimums, after everything else has run. PHASE-27 section 2.1,
//! phase 12.
//!
//! ## The only check in this phase whose subject is the gallery
//!
//! Nine checks ask about one photograph. This one asks whether the *set* still satisfies the
//! guarantees phase 12 made, and it runs once per project rather than once per frame.
//!
//! It exists because this phase can change what is in the gallery. Phase 12's coverage guard ran
//! against the selection phase 12 made, and every `ReplaceFrame` remedy since has swapped a frame
//! for one phase 12 did not choose. [`crate::replace`] re-validates coverage before each swap - a
//! filter, never a score - and this check is the second layer: it asks, at the end, whether the
//! delivered gallery still holds.
//!
//! Two layers rather than one, for the reason migration 27 keeps a CHECK beside a Rust guard: the
//! first lives in code a future caller could route around, and a guarantee about a wedding's cake
//! is worth asserting twice.
//!
//! ## `Missing` means nobody shot it, and this check must not say otherwise
//!
//! Phase 12 wrote that rule and it is the trap here. A must-have that is `Missing` because the
//! wedding had no cake is not a QC finding - there is nothing to fix and nothing to escalate, and a
//! ticket saying "a moment the gallery has to include is not covered" would send a photographer
//! looking for a photograph that does not exist.
//!
//! What this check reports is a rule that **was covered and now is not**, or a rule phase 12 itself
//! reported as `CoveredWeak`. The distinction lives in the reading: `missing_rules` is populated
//! only for rules that had candidates. `api::collect` is where that filtering happens, and it is why
//! this module takes a list of names rather than a `CoverageReport`.

use aura_core::contract::qc::{QcCategory, QcCode};

use super::{Finding, Outcome, SetContext};

/// The threshold a coverage violation is measured against.
///
/// A rule is covered or it is not, so the deviation is 1.0. Half, so a missing must-have comes out
/// at severity 2.0 and leads the queue - which is where the gallery's own guarantees belong.
const RULE_THRESHOLD: f32 = 0.5;

/// Inspect the delivered gallery's guarantees.
///
/// Takes a [`SetContext`] rather than a `Frame`, because coverage is a fact about the set. The two
/// gallery-scoped categories are named by `QcCategory::is_gallery_scoped`, and
/// [`crate::reedit`] re-runs this over the whole gallery rather than over one photograph when it
/// re-inspects a coverage ticket.
#[must_use]
pub fn inspect(context: &SetContext) -> Outcome {
    if !context.coverage_available {
        // A project phase 12 never selected has no coverage to be missing. A skip rather than a
        // pass, because "we checked and the gallery is complete" and "there is no gallery" are the
        // two things this phase exists never to confuse.
        return Outcome::Skipped("no coverage report for this project");
    }
    let mut findings = Vec::new();

    for rule in &context.missing_rules {
        findings.push(
            Finding::new(
                QcCategory::Coverage,
                QcCode::CoverageMissing,
                1.0,
                RULE_THRESHOLD,
                // Zero. A coverage hole is filled by putting a frame back, which is a selection
                // change rather than a parameter change, and the loop judges it on whether the rule
                // became covered.
                0.0,
                // Certain. Phase 12 either covered the rule or it did not, and `api::collect` has
                // already excluded the rules that had no candidates.
                1.0,
            )
            .because(QcCode::EscalatedToHuman, 1.0)
            .with_evidence(evidence(rule)),
        );
    }

    for rule in &context.weak_rules {
        findings.push(
            Finding::new(
                QcCategory::Coverage,
                QcCode::CoverageWeak,
                1.0,
                RULE_THRESHOLD,
                0.0,
                // Lower than a missing rule, because phase 12's guard added this frame on purpose:
                // a blurred photograph of the rings beats no photograph of the rings, and this
                // ticket is a note rather than a complaint.
                0.70,
            )
            .with_evidence(evidence(rule)),
        );
    }

    for (identity, count, minimum) in &context.under_covered {
        let shortfall = minimum.saturating_sub(*count);
        if shortfall == 0 {
            continue;
        }
        findings.push(
            Finding::new(
                QcCategory::Coverage,
                QcCode::IdentityUnderCovered,
                shortfall as f32,
                // One frame short is the smallest finding this can be, so the threshold is just
                // below it and the severity ratio grows with how many frames are missing.
                0.9,
                0.0,
                0.85,
            )
            .about(*identity),
        );
    }

    Outcome::from_findings(findings)
}

/// A rule's name as named evidence.
///
/// `Evidence::Params` rather than `Evidence::Frames`, because the thing to point at is the rule
/// itself and there are no frames to show - that is what "missing" means. The value is 0.0 and
/// carries nothing; the name is the payload.
fn evidence(rule: &str) -> aura_core::contract::qc::Evidence {
    aura_core::contract::qc::Evidence::Params(vec![(rule.to_string(), 0.0)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::ids::IdentityId;

    fn context() -> SetContext {
        SetContext {
            coverage_available: true,
            ..SetContext::default()
        }
    }

    #[test]
    fn a_complete_gallery_is_clean() {
        assert_eq!(inspect(&context()), Outcome::Clean);
    }

    #[test]
    fn a_project_with_no_selection_skips_rather_than_passes() {
        let outcome = inspect(&SetContext::default());
        assert!(matches!(outcome, Outcome::Skipped(_)));
        assert_ne!(
            outcome,
            Outcome::Clean,
            "no gallery and a complete gallery are the two things this phase must never confuse"
        );
    }

    #[test]
    fn a_missing_rule_is_certain_and_leads_the_queue() {
        let mut ctx = context();
        ctx.missing_rules.push("rings".into());
        let findings = inspect(&ctx).findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, QcCode::CoverageMissing);
        assert_eq!(findings[0].confidence, 1.0);
        assert!(findings[0].severity() >= 2.0);
    }

    #[test]
    fn a_weak_rule_is_a_note_rather_than_a_complaint() {
        let mut ctx = context();
        ctx.weak_rules.push("cake".into());
        let findings = inspect(&ctx).findings();
        assert_eq!(findings[0].code, QcCode::CoverageWeak);
        // Phase 12's guard added that frame on purpose: a blurred photograph of the rings beats no
        // photograph of the rings.
        assert!(findings[0].confidence < 1.0);
    }

    #[test]
    fn a_missing_rule_carries_its_own_name_rather_than_a_frame() {
        let mut ctx = context();
        ctx.missing_rules.push("first_dance".into());
        let findings = inspect(&ctx).findings();
        match &findings[0].evidence {
            aura_core::contract::qc::Evidence::Params(list) => {
                assert_eq!(list[0].0, "first_dance");
            }
            other => panic!("a missing rule has no frame to point at: {other:?}"),
        }
    }

    #[test]
    fn an_under_covered_identity_grows_in_severity_with_the_shortfall() {
        let person = IdentityId::new();
        let mut one_short = context();
        one_short.under_covered.push((person, 4, 5));
        let mut four_short = context();
        four_short.under_covered.push((person, 1, 5));
        let a = inspect(&one_short).findings();
        let b = inspect(&four_short).findings();
        assert!(b[0].severity() > a[0].severity());
        assert_eq!(a[0].identity, Some(person));
    }

    #[test]
    fn an_identity_that_is_not_short_produces_nothing() {
        let mut ctx = context();
        ctx.under_covered.push((IdentityId::new(), 5, 5));
        assert_eq!(inspect(&ctx), Outcome::Clean);
    }

    #[test]
    fn coverage_predicts_no_gain_because_the_remedy_is_a_selection_change() {
        let mut ctx = context();
        ctx.missing_rules.push("rings".into());
        assert_eq!(inspect(&ctx).findings()[0].expected_gain, 0.0);
    }
}
