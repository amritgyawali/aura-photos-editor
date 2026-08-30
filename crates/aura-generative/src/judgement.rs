//! The editorial judgement port: the one cloud call this phase makes, expressed as a trait that
//! **cannot approve anything**. Section 7, and ADR-0049 section 7.
//!
//! ## Why this is a port rather than a dependency
//!
//! `aura-generative` does not depend on `aura-cloud`. Phase 04's rule is that a provider is reached
//! through `CloudAiGateway` and a task is defined against `aura-core`'s frozen `CloudTask` shape;
//! the task itself is `aura_cloud::cleanup_judgement::CleanupJudgement`, and `aura-app` wires the
//! two together. What lives here is the *question* and the *answer*, so this crate can be tested,
//! run offline and reasoned about with no network code anywhere near it.
//!
//! ## The answer type has no approving variant, and that is the whole design
//!
//! Phase 12 declined to build its cloud tie-breaker and wrote down why: with placeholder heads
//! underneath, every call would spend a photographer's money arbitrating between two random
//! projections. This one is built, and the difference is the **direction** it can move a decision.
//!
//! [`Answer`] is `Decline`, `Stand` or `Unavailable`. There is no `Approve`. A judgement can turn a
//! proposed removal into a refusal; it cannot turn a refusal into a removal, it cannot raise a
//! confidence, and it cannot reach a candidate that failed a mechanical check - because
//! [`ask`](Ask) is only built for candidates that already passed all five.
//!
//! A cloud call that can only ever make the product do less is a cloud call whose failure modes are
//! all safe. An unreachable provider, an invalid response, an exhausted budget and a model having a
//! bad day all produce the same outcome as a cautious answer, which is that the proposal waits for
//! a person.
//!
//! ## The band
//!
//! [`JUDGEMENT_BAND`] is 0.60 to 0.90. Below it the mechanical answer is already "no" and there is
//! nothing to ask about; above it the mechanical answer is confident and a model's opinion would be
//! bought at the price of a call per frame. In between is where "is this bin part of the wedding"
//! is a question a vision model can answer and a structure tensor cannot.
//!
//! [`JUDGEMENT_BAND`]: aura_core::contract::cleanup::JUDGEMENT_BAND

use aura_core::contract::cleanup::{
    Box2, CleanupCode, CleanupMethod, DistractionClass, ImageId, JUDGEMENT_BAND,
};
use aura_core::contract::scene::SceneId;

/// The most judgement calls one wedding may make. Section 7's cost control.
pub const MAX_CALLS_PER_PROJECT: u32 = 20;

/// What a judge is told about one candidate.
///
/// **There is no field for a person.** No identity, no role handle, no face box, no count of who is
/// in the frame. Phase 06's rule - never infer anything about a person - and this task has no
/// reason to know: the question is whether an object is part of the wedding, and the safety engine
/// has already established that no person is inside the region.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    /// The photograph, so the caller can attach the right crop and the audit row can name it.
    pub image: ImageId,
    /// Where, normalised.
    pub region: Box2,
    /// What the detector thinks it is.
    pub class: DistractionClass,
    /// The share of the frame it covers.
    pub area_frac: f32,
    /// Which scene the photograph is.
    pub scene: SceneId,
    /// How the pixels would be replaced.
    pub method: CleanupMethod,
    /// The mechanical removability confidence that put this in the band.
    pub confidence: f32,
}

impl Ask {
    /// True when this candidate is inside section 7's band and therefore worth asking about.
    ///
    /// Inclusive at both ends, so a candidate sitting exactly on 0.60 or 0.90 is asked about. A
    /// half-open band would make one of the two boundaries behave differently from the other for
    /// no reason anybody could state, which is the kind of detail that turns into a support case.
    #[must_use]
    pub fn is_in_band(confidence: f32) -> bool {
        confidence >= JUDGEMENT_BAND.0 && confidence <= JUDGEMENT_BAND.1
    }
}

/// What a judge may say.
///
/// Three variants and none of them approves. See the module header.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// The object is part of the wedding, or removing it would be wrong. The proposal is dropped.
    Decline {
        /// Short editorial reasons, for the panel and the ledger.
        reasons: Vec<String>,
        /// True when the model said the object is part of the story rather than merely risky.
        story_relevant: bool,
    },
    /// The judge agrees there is nothing editorial in the way. The mechanical answer stands
    /// **unchanged** - the confidence is not raised and the autonomy band is not moved.
    Stand,
    /// No judge was reachable, no key was configured, the budget was spent, or the answer did not
    /// validate. The mechanical answer stands and the proposal is marked so a panel can say so.
    Unavailable,
}

impl Answer {
    /// The reason code this answer contributes, when it contributes one.
    #[must_use]
    pub const fn code(&self) -> Option<CleanupCode> {
        match self {
            Self::Decline { .. } => Some(CleanupCode::JudgementDeclined),
            Self::Stand => None,
            Self::Unavailable => Some(CleanupCode::JudgementUnavailable),
        }
    }

    /// True when this answer removes the proposal.
    #[must_use]
    pub const fn is_decline(&self) -> bool {
        matches!(self, Self::Decline { .. })
    }
}

/// Something that can judge whether a removal is editorially appropriate.
///
/// Implemented in `aura-app` over `aura_cloud::cleanup_judgement::CleanupJudgement`, and by
/// [`NeverAsk`] everywhere else. A pass holds `Option<Arc<dyn EditorialJudge>>`, and `None` behaves
/// exactly like [`Answer::Unavailable`] - which is invariant 6: the product completes a full
/// wedding with no network, and the cloud is an accelerator.
pub trait EditorialJudge: Send + Sync {
    /// Judge one candidate.
    ///
    /// Total rather than fallible, because every failure this call can have already has a variant.
    /// A `Result` here would give a caller two ways to express "the cloud did not answer" and they
    /// would eventually be handled differently.
    fn judge(&self, ask: &Ask) -> Answer;

    /// How many calls this judge has left for the project.
    ///
    /// The pass reads it before it builds an [`Ask`], so a wedding that has spent its twenty calls
    /// stops assembling crops rather than assembling them and throwing them away.
    fn remaining(&self) -> u32;
}

/// The judge that is used when there is no cloud: it always says it could not answer.
///
/// Not a stub. It is the shipped behaviour whenever cloud AI is off, no key is configured, or the
/// studio declined - which in this build is every installation, because TLS is waived and no public
/// vision provider is reachable.
#[derive(Debug, Clone, Copy, Default)]
pub struct NeverAsk;

impl EditorialJudge for NeverAsk {
    fn judge(&self, _ask: &Ask) -> Answer {
        Answer::Unavailable
    }

    fn remaining(&self) -> u32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_band_is_inclusive_at_both_ends() {
        assert!(Ask::is_in_band(JUDGEMENT_BAND.0));
        assert!(Ask::is_in_band(JUDGEMENT_BAND.1));
        assert!(!Ask::is_in_band(JUDGEMENT_BAND.0 - 0.01));
        assert!(!Ask::is_in_band(JUDGEMENT_BAND.1 + 0.01));
    }

    #[test]
    fn there_is_no_answer_that_approves_a_removal() {
        // The property the design rests on, asserted rather than described. If somebody adds an
        // approving variant this test does not compile, which is the point at which they have to
        // read ADR-0049 section 7.
        let answers = [
            Answer::Decline {
                reasons: vec!["the sign names the couple".into()],
                story_relevant: true,
            },
            Answer::Stand,
            Answer::Unavailable,
        ];
        for answer in answers {
            match answer {
                Answer::Decline { .. } | Answer::Stand | Answer::Unavailable => {}
            }
        }
    }

    #[test]
    fn an_unreachable_judge_and_a_cautious_one_have_the_same_effect_on_the_photograph() {
        // Both leave the proposal for a person. The difference is only what the panel says.
        assert_eq!(NeverAsk.judge(&ask()), Answer::Unavailable);
        assert!(!NeverAsk.judge(&ask()).is_decline());
        assert_eq!(NeverAsk.remaining(), 0);
    }

    #[test]
    fn a_decline_and_an_unavailable_carry_different_codes() {
        assert_eq!(
            Answer::Decline {
                reasons: Vec::new(),
                story_relevant: false
            }
            .code(),
            Some(CleanupCode::JudgementDeclined)
        );
        assert_eq!(
            Answer::Unavailable.code(),
            Some(CleanupCode::JudgementUnavailable)
        );
        assert_eq!(Answer::Stand.code(), None);
    }

    fn ask() -> Ask {
        Ask {
            image: aura_core::PhotoId::default(),
            region: Box2 {
                x: 0.02,
                y: 0.85,
                w: 0.05,
                h: 0.05,
            },
            class: DistractionClass::Bin,
            area_frac: 0.0025,
            scene: SceneId::Unknown,
            method: CleanupMethod::ClassicalFill,
            confidence: 0.75,
        }
    }
}
