//! Four vocabularies, one model.
//!
//! Section 8's first step: "freeze the `Reason`/`Decision` model and refactor Phases 09-12
//! to emit it (this is why it lands now, not later)." This module is that refactor, and the
//! form it takes is worth stating plainly because it is a deliberate departure from the
//! literal instruction.
//!
//! ## Why an adapter rather than four rewritten contracts
//!
//! Phases 09, 10, 11 and 12 each froze a reason type, and each of those types is referenced
//! by a migration, an IPC surface, a `contracts.lock` digest and a body of tests. Rewriting
//! all four to emit `LedgerReason` directly would be four contract changes, four ADRs and
//! four re-locks - and it would buy nothing, because the thing that has to be true is that
//! *the deciding code owns the reason*, and it already does.
//!
//! So the deciding code keeps its own typed vocabulary, and this module maps it. The rule
//! section 12 asks for - "a lint forbids constructing reasons outside the deciding module" -
//! is kept in a stronger form than a lint: [`crate::reason::Catalog`] is assembled from
//! those same four enums, and a reason whose code is not one of theirs is refused at record
//! time. There is no way to write a reason into the ledger that no deciding phase can emit.
//!
//! ADR-0027 section 4 records the argument.
//!
//! ## What is mapped, and what is not
//!
//! The code, the sentence, the weight and the evidence rectangle - which is to say
//! everything the four types carry. Nothing is invented, nothing is re-worded and no weight
//! is rescaled. A mapper that normalised phase 09's negative weights against phase 10's
//! positive ones would be a mapper that changed what a deciding phase said, and the panel
//! would then be explaining the adapter rather than the decision.

use aura_core::contract::composition::{CompositionReason, CompositionResult};
use aura_core::contract::cull::{Decision as CullDecision, KeepScore, MustHave};
use aura_core::contract::emotion::{EmotionReason, ImageEmotion};
use aura_core::contract::integrity::{IntegrityResult, Reason as TechnicalReason};
use aura_core::contract::ledger::{
    DecisionKind, DecisionSource, DecisionSubject, Evidence, LedgerReason,
};
use aura_core::ProjectId;

use crate::decision::DecisionBuilder;

/// Everything a cull decision needs beyond the decision itself.
///
/// The versions are here rather than read from a service, because a decision records what
/// was underneath it *at the time* and a lookup would record what is underneath it now.
#[derive(Debug, Clone, Default)]
pub struct CullContext {
    /// The four sub-scores and the fused one, when they are known.
    pub score: Option<KeepScore>,
    /// The scene slug the weights came from.
    pub scene: String,
    /// The chapter slug the quota counted the frame against.
    pub chapter: String,
    /// The must-have this frame satisfies, when it satisfies one.
    ///
    /// The one field that changes what the product is *allowed to do*: section 6.4 raises a
    /// decision touching a must-have by one band.
    pub must_have: Option<MustHave>,
    /// The lowest model version among the sub-scores.
    pub model_ver: u16,
    /// The selection-pass version.
    pub analysis_ver: u16,
    /// The per-scene calibration version phase 12 fused with. Not phase 13's.
    pub score_calibration_ver: u16,
    /// The digest of phase 12's two configuration tables.
    pub config_digest: String,
    /// The determinism hash of the selection run this decision came out of.
    pub run_hash: u64,
    /// How long the selection took, in milliseconds.
    pub ms: u32,
}

/// One phase 12 decision, as something the ledger will accept.
///
/// The subject is the photograph, the kind is [`DecisionKind::Cull`], and the outputs are
/// the four facts a replay compares: kept or not, the score, the runner-up and the
/// coverage role. Everything else is a reason or an input.
#[must_use]
pub fn cull_decision(
    project: ProjectId,
    decision: &CullDecision,
    context: &CullContext,
) -> DecisionBuilder {
    let image = decision.image_id();
    let mut builder =
        DecisionBuilder::new(project, DecisionKind::Cull, DecisionSubject::Image(image))
            .output_bool("kept", decision.is_keep())
            .ms(context.ms)
            .source(if user_decided(decision) {
                // A frame the photographer kept or removed by hand was not decided by a model, and
                // recording it as local would put a person's choice into a calibration fit.
                DecisionSource::User
            } else {
                DecisionSource::Local
            })
            .input("image", image.to_db())
            .input("scene", context.scene.clone())
            .input("chapter", context.chapter.clone())
            .input("config_digest", context.config_digest.clone())
            .input("run_hash", format!("{:016x}", context.run_hash))
            .model("cull_subscores", context.model_ver)
            .config("cull_analysis", context.analysis_ver)
            .config("cull_calibration", context.score_calibration_ver);

    match decision {
        CullDecision::Keep(keep) => {
            builder = builder
                .output_num("keep_score", keep.keep_score)
                .output_num("confidence", keep.confidence)
                .confidence(keep.confidence);
            if let Some(runner_up) = keep.runner_up {
                builder = builder.output("runner_up", runner_up.to_db());
            }
            if let Some(role) = keep.coverage_role {
                builder = builder.output("coverage_role", role.as_str());
            }
            builder = builder.reasons(keep.reasons.iter().map(|reason| {
                let evidence = keep
                    .runner_up
                    .filter(|_| reason.code == aura_core::CullCode::MomentWinner)
                    .map_or(Evidence::None, |id| Evidence::Frames(vec![id]));
                LedgerReason {
                    code: reason.code.as_str().to_string(),
                    text: reason.text.clone(),
                    weight: reason.weight,
                    evidence,
                }
            }));
        }
        CullDecision::Drop(drop) => {
            builder = builder
                .output_num("keep_score", drop.keep_score)
                // A rejection's confidence is the score it lost with, and that is honest
                // rather than convenient: phase 12 does not compute a separate confidence
                // for a rejection, and inventing one here would be this module doing exactly
                // what its header says it must not.
                .confidence(drop.keep_score);
            if let Some(kept) = drop.kept_instead {
                builder = builder.output("kept_instead", kept.to_db());
            }
            builder = builder.reasons(drop.reasons.iter().map(|reason| {
                let evidence = drop
                    .kept_instead
                    .map_or(Evidence::None, |id| Evidence::Frames(vec![id]));
                LedgerReason {
                    code: reason.code.as_str().to_string(),
                    text: reason.text.clone(),
                    weight: reason.weight,
                    evidence,
                }
            }));
        }
    }

    if let Some(score) = context.score {
        builder = builder
            .input_num("technical", score.technical)
            .input_num("emotion", score.emotion)
            .input_num("composition", score.composition)
            .input_num("prominence", score.prominence)
            .input_num("scene_weighted", score.scene_weighted)
            .output_num("technical", score.technical)
            .output_num("emotion", score.emotion)
            .output_num("composition", score.composition)
            .output_num("prominence", score.prominence);
    }

    builder
}

/// True when the photographer, rather than the engine, put this frame where it is.
fn user_decided(decision: &CullDecision) -> bool {
    decision.reasons().iter().any(|reason| {
        matches!(
            reason.code,
            aura_core::CullCode::UserKept | aura_core::CullCode::UserRejected
        )
    })
}

/// Phase 09's reasons, in the unified model.
#[must_use]
pub fn technical_reasons(result: &IntegrityResult) -> Vec<LedgerReason> {
    result.reasons.iter().map(technical_reason).collect()
}

/// One of phase 09's reasons.
#[must_use]
pub fn technical_reason(reason: &TechnicalReason) -> LedgerReason {
    LedgerReason {
        code: reason.code.as_str().to_string(),
        text: reason.text.clone(),
        weight: reason.weight,
        evidence: reason.evidence.map_or(Evidence::None, Evidence::Crop),
    }
}

/// Phase 10's reasons, in the unified model.
#[must_use]
pub fn emotion_reasons(image: &ImageEmotion) -> Vec<LedgerReason> {
    image.reasons.iter().map(emotion_reason).collect()
}

/// One of phase 10's reasons.
#[must_use]
pub fn emotion_reason(reason: &EmotionReason) -> LedgerReason {
    LedgerReason {
        code: reason.code.as_str().to_string(),
        text: reason.text.clone(),
        weight: reason.weight,
        evidence: reason.evidence.map_or(Evidence::None, Evidence::Crop),
    }
}

/// Phase 11's reasons, in the unified model.
#[must_use]
pub fn composition_reasons(result: &CompositionResult) -> Vec<LedgerReason> {
    result.reasons.iter().map(composition_reason).collect()
}

/// One of phase 11's reasons.
#[must_use]
pub fn composition_reason(reason: &CompositionReason) -> LedgerReason {
    LedgerReason {
        code: reason.code.as_str().to_string(),
        text: reason.text.clone(),
        weight: reason.weight,
        evidence: reason.evidence.map_or(Evidence::None, Evidence::Crop),
    }
}
