//! The safety engine. Section 6.2, and the first module of this phase written.
//!
//! Five checks, in a fixed order, run **before** anything is scored. A candidate that fails one is
//! discarded and recorded with the check that discarded it; a candidate that passes all five
//! becomes a [`SafeCandidate`], which is the only thing `CleanupProposal::new` will accept.
//!
//! ## Why this is a filter rather than a term in a score
//!
//! A penalty is a trade. Any penalty large enough to be safe across four hundred frames loses on
//! the one frame where the salience term is most confident - and the frame where a distraction is
//! most salient is the frame where it is nearest the subject. Phase 12 wrote this rule for
//! coverage guarantees, phase 23 wrote it for crop safety, and this is the phase where the cost of
//! getting it wrong is highest.
//!
//! The ordering is not a convention. [`SafeCandidate`] has no public constructor and is produced
//! only by [`check`] returning `Allowed`, so a future caller cannot score an unchecked candidate:
//! it cannot obtain the argument.
//!
//! ## Every refusal is a row
//!
//! [`Outcome::Blocked`] carries the check and the reason code, and the store writes it. That makes
//! the engine auditable, it is how section 10.1's adversarial audit is scored, and it is how a
//! photographer learns what the product will never do. Phase 22's rule: here the refusal is the
//! product working.

use aura_core::contract::cleanup::{
    Box2, CleanupCode, DistractionClass, SafetyCheck, SafetyVerdict,
};

use crate::denylist::{self, Coverage, Verdict};
use crate::policy::ScenePolicy;

/// A region somebody might want removed, before anything has judged it.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// Where, normalised to the frame.
    pub region: Box2,
    /// What it is, as far as anything can tell.
    pub class: DistractionClass,
    /// How much attention it draws, `0..1`.
    pub salience: f32,
    /// How confident the detector is that it can be removed, `0..1`.
    pub removability: f32,
    /// True when the region crosses a long straight line or a repeating pattern.
    ///
    /// Measured by the detector from the frame's own gradients, because it is a property of the
    /// pixels rather than of the policy. Inpainting warps these predictably, which is why it is a
    /// hard check rather than a confidence penalty.
    pub crosses_structure: bool,
    /// True when the region touches a primary identity's body, not merely their face.
    ///
    /// Separate from the denylist because it is a different question: the denylist asks what kind
    /// of thing this is, and this asks whose it is.
    pub touches_identity: bool,
}

impl Candidate {
    /// The share of the frame this region covers.
    #[must_use]
    pub fn area_frac(&self) -> f32 {
        (self.region.w * self.region.h).clamp(0.0, 1.0)
    }
}

/// A candidate that passed all five checks.
///
/// **No public constructor.** The only way to hold one is to have called [`check`] and received
/// [`Outcome::Allowed`], which is what makes "the safety filter runs before the score" a property
/// of the type system rather than an ordering in a function.
#[derive(Debug, Clone, PartialEq)]
pub struct SafeCandidate {
    candidate: Candidate,
    verdict: SafetyVerdict,
}

impl SafeCandidate {
    /// The region and its measurements.
    #[must_use]
    pub fn candidate(&self) -> &Candidate {
        &self.candidate
    }

    /// The verdict to store on the proposal. Always `allowed`.
    #[must_use]
    pub fn verdict(&self) -> &SafetyVerdict {
        &self.verdict
    }

    /// Consume this into the parts a proposal needs.
    #[must_use]
    pub fn into_parts(self) -> (Candidate, SafetyVerdict) {
        (self.candidate, self.verdict)
    }
}

/// What the engine decided about one candidate.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Every check passed.
    Allowed(Box<SafeCandidate>),
    /// One check failed. The candidate is discarded and this is written down.
    Blocked {
        /// Which check.
        check: SafetyCheck,
        /// Which reason code, for the panel and the ledger.
        code: CleanupCode,
        /// The verdict to store, carrying every check as it was found.
        verdict: SafetyVerdict,
    },
}

impl Outcome {
    /// True when the candidate may go on to be scored.
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed(_))
    }

    /// The check that failed, when one did.
    #[must_use]
    pub const fn blocked_by(&self) -> Option<SafetyCheck> {
        match self {
            Self::Allowed(_) => None,
            Self::Blocked { check, .. } => Some(*check),
        }
    }
}

/// Run the five checks of section 6.2, in order.
///
/// The order is the one `SafetyCheck::ALL` declares and it is not arbitrary: the cheap geometric
/// checks run before the ones that need masks, so a candidate that is simply too large never
/// causes a mask to be resolved.
///
/// `policy` is the row for this photograph's scene. A scene with `enabled = false` blocks
/// everything through [`SafetyCheck::SizeCap`], because its `area_cap` is zero - which is the same
/// mechanism rather than a special case, and means one fewer branch that could be got wrong.
#[must_use]
pub fn check(candidate: &Candidate, policy: &ScenePolicy, coverage: &Coverage) -> Outcome {
    // 1. Size. A region larger than the cap is refused *automation*, not refused outright - it
    //    becomes a manual action somebody takes while looking at the photograph.
    if candidate.area_frac() > policy.area_cap {
        return blocked(
            SafetyCheck::SizeCap,
            CleanupCode::TooLarge,
            format!(
                "the region covers {:.1} % of the frame and this scene allows {:.1} %",
                candidate.area_frac() * 100.0,
                policy.area_cap * 100.0
            ),
        );
    }

    // 2. The denylist. An absent mask blocks; see the module docs of `denylist`.
    match denylist::judge(&candidate.region, coverage, policy.denylist_overlap_max) {
        Verdict::Clear => {}
        Verdict::Overlaps(kind, share) => {
            return blocked(
                SafetyCheck::Denylist,
                CleanupCode::OverlapsProtected,
                format!("{:.1} % of the region is {}", share * 100.0, kind.as_str()),
            );
        }
        Verdict::Unknown => {
            return blocked(
                SafetyCheck::Denylist,
                CleanupCode::ProtectionUnknown,
                "the regions that would show this is clear of people have not been produced"
                    .to_string(),
            );
        }
    }

    // 3. Identity. A different question from the denylist: not what kind of thing, but whose.
    if candidate.touches_identity {
        return blocked(
            SafetyCheck::IdentityProtect,
            CleanupCode::OverlapsIdentity,
            "the region touches somebody this wedding is about".to_string(),
        );
    }
    if candidate.class == DistractionClass::BackgroundPerson {
        // A person is never proposed automatically, in any scene, at any size. Removing a person
        // is a human decision about a human being; it exists as a manual, confirmed tool and
        // never reaches this path.
        return blocked(
            SafetyCheck::IdentityProtect,
            CleanupCode::PersonPresent,
            "removing a person is a manual, confirmed action rather than a proposal".to_string(),
        );
    }

    // 4. Structure. Inpainting warps a long straight line or a repeating pattern predictably, and
    //    "predictably" is why this is a hard check rather than a penalty.
    if candidate.crosses_structure {
        return blocked(
            SafetyCheck::StructureSpan,
            CleanupCode::StructureSpanned,
            "the region crosses a straight line or a repeating pattern".to_string(),
        );
    }

    // 5. Confidence, and story relevance, which is where an unknown class stops.
    if !candidate.class.story_safe() {
        let code = if candidate.class == DistractionClass::Unclassified {
            CleanupCode::ClassUnknown
        } else {
            CleanupCode::StoryRelevant
        };
        return blocked(
            SafetyCheck::Confidence,
            code,
            "this cannot be shown to be extraneous to the wedding".to_string(),
        );
    }
    if candidate.removability < policy.zero_touch_confidence.min(0.5) {
        return blocked(
            SafetyCheck::Confidence,
            CleanupCode::ConfidenceLow,
            format!(
                "removability {:.2} is too low to propose anything",
                candidate.removability
            ),
        );
    }

    Outcome::Allowed(Box::new(SafeCandidate {
        candidate: candidate.clone(),
        verdict: SafetyVerdict::allow(),
    }))
}

fn blocked(check: SafetyCheck, code: CleanupCode, reason: String) -> Outcome {
    Outcome::Blocked {
        check,
        code,
        verdict: SafetyVerdict::block(check, reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::denylist::Protected;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Box2 {
        Box2 { x, y, w, h }
    }

    fn policy() -> ScenePolicy {
        ScenePolicy {
            area_cap: 0.04,
            denylist_overlap_max: 0.01,
            zero_touch_confidence: 0.97,
            enabled: true,
            reason: "a test".into(),
        }
    }

    fn bin() -> Candidate {
        Candidate {
            region: rect(0.02, 0.80, 0.10, 0.10),
            class: DistractionClass::Bin,
            salience: 0.7,
            removability: 0.8,
            crosses_structure: false,
            touches_identity: false,
        }
    }

    fn small_bin() -> Candidate {
        Candidate {
            region: rect(0.02, 0.80, 0.10, 0.10),
            ..bin()
        }
    }

    #[test]
    fn a_clean_small_bin_over_known_masks_is_allowed() {
        let mut c = small_bin();
        c.region = rect(0.02, 0.80, 0.10, 0.10); // 1 % of the frame
        let outcome = check(&c, &policy(), &Coverage::known_empty());
        assert!(outcome.is_allowed(), "got {outcome:?}");
    }

    #[test]
    fn a_region_over_the_cap_is_blocked_by_the_size_check() {
        let mut c = bin();
        c.region = rect(0.10, 0.10, 0.40, 0.40); // 16 % of the frame
        let outcome = check(&c, &policy(), &Coverage::known_empty());
        assert_eq!(outcome.blocked_by(), Some(SafetyCheck::SizeCap));
    }

    #[test]
    fn a_scene_with_cleanup_off_blocks_everything_through_the_size_cap() {
        let off = ScenePolicy::off("the ritual is the wedding");
        let outcome = check(&small_bin(), &off, &Coverage::known_empty());
        assert_eq!(outcome.blocked_by(), Some(SafetyCheck::SizeCap));
    }

    #[test]
    fn an_absent_mask_blocks_and_says_protection_unknown_rather_than_overlaps_protected() {
        let outcome = check(&small_bin(), &policy(), &Coverage::Absent);
        match outcome {
            Outcome::Blocked { check, code, .. } => {
                assert_eq!(check, SafetyCheck::Denylist);
                assert_eq!(code, CleanupCode::ProtectionUnknown);
            }
            Outcome::Allowed(_) => panic!("an absent mask must never allow a removal"),
        }
    }

    #[test]
    fn a_hand_in_the_region_blocks_and_says_overlaps_protected() {
        let coverage = Coverage::known(vec![(Protected::Hands, rect(0.0, 0.75, 0.20, 0.20))]);
        let outcome = check(&small_bin(), &policy(), &coverage);
        match outcome {
            Outcome::Blocked { check, code, .. } => {
                assert_eq!(check, SafetyCheck::Denylist);
                assert_eq!(code, CleanupCode::OverlapsProtected);
            }
            Outcome::Allowed(_) => panic!("a hand must block"),
        }
    }

    #[test]
    fn a_person_is_never_allowed_however_small_and_however_confident() {
        let mut c = small_bin();
        c.class = DistractionClass::BackgroundPerson;
        c.region = rect(0.01, 0.01, 0.01, 0.01);
        c.removability = 1.0;
        let outcome = check(&c, &policy(), &Coverage::known_empty());
        match outcome {
            Outcome::Blocked { check, code, .. } => {
                assert_eq!(check, SafetyCheck::IdentityProtect);
                assert_eq!(code, CleanupCode::PersonPresent);
            }
            Outcome::Allowed(_) => panic!("a person must never be proposed automatically"),
        }
    }

    #[test]
    fn an_unclassified_object_is_blocked_by_confidence_rather_than_allowed() {
        let mut c = small_bin();
        c.class = DistractionClass::Unclassified;
        c.removability = 1.0;
        match check(&c, &policy(), &Coverage::known_empty()) {
            Outcome::Blocked { check, code, .. } => {
                assert_eq!(check, SafetyCheck::Confidence);
                assert_eq!(code, CleanupCode::ClassUnknown);
            }
            Outcome::Allowed(_) => panic!("an unknown class cannot be shown to be extraneous"),
        }
    }

    #[test]
    fn a_region_crossing_structure_is_blocked() {
        let mut c = small_bin();
        c.crosses_structure = true;
        assert_eq!(
            check(&c, &policy(), &Coverage::known_empty()).blocked_by(),
            Some(SafetyCheck::StructureSpan)
        );
    }

    #[test]
    fn a_blocked_outcome_carries_a_well_formed_verdict_that_names_its_check() {
        let mut c = bin();
        c.region = rect(0.1, 0.1, 0.5, 0.5);
        match check(&c, &policy(), &Coverage::known_empty()) {
            Outcome::Blocked { verdict, check, .. } => {
                assert!(verdict.is_well_formed());
                assert!(!verdict.allowed);
                assert_eq!(verdict.failed_check(), Some(check));
                assert!(verdict.blocked_reason.is_some());
            }
            Outcome::Allowed(_) => panic!("expected a block"),
        }
    }

    #[test]
    fn an_allowed_outcome_carries_a_verdict_that_passes_every_check() {
        match check(&small_bin(), &policy(), &Coverage::known_empty()) {
            Outcome::Allowed(safe) => {
                assert!(safe.verdict().is_well_formed());
                assert!(safe.verdict().allowed);
                assert_eq!(safe.verdict().failed_check(), None);
                assert_eq!(safe.verdict().checks.len(), SafetyCheck::COUNT);
            }
            Outcome::Blocked { check, .. } => panic!("blocked by {check:?}"),
        }
    }

    #[test]
    fn the_checks_run_in_the_declared_order_so_a_large_region_never_needs_a_mask() {
        // A candidate that fails both the size cap and the denylist reports the size cap, which
        // is what lets the pass avoid resolving masks for regions it was never going to allow.
        let mut c = bin();
        c.region = rect(0.1, 0.1, 0.5, 0.5);
        let coverage = Coverage::known(vec![(Protected::Face, rect(0.1, 0.1, 0.5, 0.5))]);
        assert_eq!(
            check(&c, &policy(), &coverage).blocked_by(),
            Some(SafetyCheck::SizeCap)
        );
    }
}
