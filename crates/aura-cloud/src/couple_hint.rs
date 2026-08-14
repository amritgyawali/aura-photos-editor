//! `CoupleHint` - the one cloud call phase 06 is allowed to make.
//!
//! Section 7 permits an optional hint when the local evidence cannot separate the top
//! two candidate couples, and it constrains the call tightly: at most two calls per
//! project, cached, up to eight blurred thumbnails, and a local fallback that keeps the
//! pipeline complete. Every one of those constraints is in this file as a constant, a
//! type or a `Validate` rule rather than as a comment.
//!
//! ## The thing this task is most likely to get wrong, and how it is prevented
//!
//! A vision model asked "which two of these people are the couple" will, unprompted,
//! volunteer what it thinks the people in the frame are: their gender, their
//! relationship, sometimes their ethnicity or religion. That is not a capability this
//! product wants at any confidence, and a prompt rule alone would not stop it appearing
//! in `reasons[]`.
//!
//! So the **output cannot express it**. The candidates are opaque handles -
//! `person_1` to `person_8`, from [`PERSON_HANDLES`] - assigned locally, and the mapping
//! from handle to identity never leaves the machine. `couple` is an array of those
//! handles and nothing else. [`CoupleHintOutput::validate`] refuses any label that is
//! not in the fixed vocabulary, so a model that answers "the bride" or "the woman in
//! red" fails the schema and is repaired once, then falls back to local evidence.
//!
//! The prompt still carries the rule, because a model that is told not to infer gender
//! writes better reasons than one that is merely prevented from doing so. But the rule
//! is not what enforces it.
//!
//! ## What actually leaves the machine
//!
//! Counts, and blurred thumbnails. No name, no identity id, no template, no landmark, no
//! embedding. `aura-cloud` has no dependency on `aura-people`, so a template could not
//! be attached to this call even by mistake, and `aura_vision::face::redact` is what
//! makes "blurred" a measured property rather than a claim.
//!
//! ## Why the trigger is so narrow
//!
//! Because the local answer is usually right and the cloud answer is usually the same.
//! Section 7's trigger - the top two pairs within 0.05 - is the case where the local
//! evidence genuinely does not separate them, which today is mostly the case where
//! *there are no scene labels* (phase 07 has not run). When phase 07 lands, the local
//! confidence rises above `roles::SCENELESS_CONFIDENCE_CEILING` and phase 04's rule
//! takes over: a cloud answer may not overrule a local decision at 0.90 or above.

use std::collections::BTreeSet;

use aura_core::AuraError;
use serde::{Deserialize, Serialize};

use crate::contract::cloud::{CloudTask, ImagePart, PromptSpec, Tier, Validate};
use crate::tasks::{Scored, REASONS_REQUIRED_ABOVE};

/// The fixed vocabulary of candidate handles.
///
/// Eight, matching section 7's thumbnail cap. A closed list rather than an open one is
/// what lets [`Validate`] refuse a hallucinated person, and what stops the model
/// answering with a description of somebody.
pub const PERSON_HANDLES: &[&str] = &[
    "person_1", "person_2", "person_3", "person_4", "person_5", "person_6", "person_7", "person_8",
];

/// The most thumbnails one call may carry. Section 7.
pub const MAX_THUMBNAILS: usize = 8;

/// The most calls one project may make. Section 7's cost control.
pub const MAX_CALLS_PER_PROJECT: u32 = 2;

/// How close the top two pairs must be before the call is worth making.
///
/// The same 0.05 as `aura_vision::face::roles::COUPLE_AMBIGUOUS_MARGIN`, and it is the
/// same number twice on purpose: the question "should we ask the photographer" and the
/// question "should we ask a model" have the same answer, and two constants would drift.
pub const ASK_WITHIN_MARGIN: f32 = 0.05;

/// What is known about one candidate, as counts.
///
/// Per-mille integers rather than floats, for the reason phase 04's task registry gives:
/// `CloudTask::Input` is bound by `Hash`, and a classifier returning `0.640_000_1` on one
/// run and `0.639_999_9` on the next would miss the cache and bill the user twice for the
/// same question.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CandidateStats {
    /// The opaque handle, from [`PERSON_HANDLES`].
    pub handle: String,
    /// Photographs this candidate appears in.
    pub frames: u32,
    /// Appearances during getting-ready. Zero until phase 07 assigns scenes.
    pub getting_ready_frames: u32,
    /// Appearances during the ceremony. Zero until phase 07.
    pub ceremony_frames: u32,
    /// Appearances in couple portraits. Zero until phase 07.
    pub portrait_frames: u32,
    /// Mean fraction of the frame their face covers, per mille.
    pub mean_area_milli: u16,
    /// Mean centrality, per mille.
    pub mean_centrality_milli: u16,
}

/// What is known about one pair, as counts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PairStats {
    /// One handle.
    pub a: String,
    /// The other.
    pub b: String,
    /// Photographs both appear in.
    pub frames: u32,
    /// Shared appearances in couple portraits. Zero until phase 07.
    pub portrait_frames: u32,
    /// The local scorer's own score for this pair, per mille.
    pub local_score_milli: u16,
}

/// Everything the caller supplies.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CoupleHintInput {
    /// Stable identifier for the decision, used as the decision reference in the audit
    /// row. The project id, which is a random UUID and identifies no person.
    pub decision_ref: String,
    /// The candidates, in handle order.
    pub candidates: Vec<CandidateStats>,
    /// The pairs the local scorer ranked highest, best first.
    pub pairs: Vec<PairStats>,
    /// True when phase 07 has assigned scene labels. False today.
    ///
    /// Sent because it changes what the model should conclude: with no scene labels, a
    /// bride and her mother are as co-occurrent as a bride and a groom, and the model
    /// needs to know that its strongest available signal is missing rather than zero.
    pub scenes_available: bool,
    /// True when the photographer allowed faces to be visible in the thumbnails.
    pub faces_visible: bool,
    /// blake3 of each thumbnail's bytes, in order.
    ///
    /// In the input as well as in the prompt's images, so the audit row can prove which
    /// frames a decision was made from after the derivative itself is discarded.
    pub thumbnail_hashes: Vec<String>,
}

impl CoupleHintInput {
    /// The local scorer's best pair, if it had one.
    #[must_use]
    pub fn best_pair(&self) -> Option<&PairStats> {
        self.pairs.first()
    }

    /// The gap between the top two local scores, `0..1`.
    #[must_use]
    pub fn local_margin(&self) -> f32 {
        let best = self
            .pairs
            .first()
            .map_or(0u16, |pair| pair.local_score_milli);
        let second = self
            .pairs
            .get(1)
            .map_or(0u16, |pair| pair.local_score_milli);
        f32::from(best.saturating_sub(second)) / 1000.0
    }

    /// The candidates rendered for the prompt, one per line, in a fixed order.
    #[must_use]
    pub fn render_candidates(&self) -> String {
        use std::fmt::Write as _;

        let mut out = String::with_capacity(512);
        for candidate in &self.candidates {
            // Writing into a String cannot fail; the result is discarded deliberately.
            let _ = writeln!(
                out,
                "  {}: {} frames, {} getting-ready, {} ceremony, {} couple-portrait, mean face \
                 area {:.3}, mean centrality {:.2}",
                candidate.handle,
                candidate.frames,
                candidate.getting_ready_frames,
                candidate.ceremony_frames,
                candidate.portrait_frames,
                f32::from(candidate.mean_area_milli) / 1000.0,
                f32::from(candidate.mean_centrality_milli) / 1000.0
            );
        }
        out
    }

    /// The pairs rendered for the prompt.
    #[must_use]
    pub fn render_pairs(&self) -> String {
        use std::fmt::Write as _;

        let mut out = String::with_capacity(512);
        for pair in self.pairs.iter().take(6) {
            let _ = writeln!(
                out,
                "  {} + {}: {} frames together, {} couple-portrait frames together, local score \
                 {:.2}",
                pair.a,
                pair.b,
                pair.frames,
                pair.portrait_frames,
                f32::from(pair.local_score_milli) / 1000.0
            );
        }
        out
    }
}

/// What the model must return.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CoupleHintOutput {
    /// Zero or two handles. Never one: a couple of one person is not a couple.
    pub couple: Vec<String>,
    /// Handles the model believes are close family.
    #[serde(default)]
    pub close_family: Vec<String>,
    /// 0.0 to 1.0.
    pub confidence: f32,
    /// Up to six short reasons, each citing photographic role evidence.
    pub reasons: Vec<String>,
}

impl Scored for CoupleHintOutput {
    fn confidence(&self) -> f32 {
        self.confidence.clamp(0.0, 1.0)
    }

    fn reasons(&self) -> &[String] {
        &self.reasons
    }
}

impl Validate for CoupleHintOutput {
    /// The rules the JSON Schema cannot express, and one it can that is worth checking
    /// twice.
    ///
    /// The vocabulary check is the important one: it is what makes it impossible for this
    /// task to return a description of a person rather than a handle. See this module's
    /// header.
    fn validate(&self) -> Result<(), String> {
        let known: BTreeSet<&str> = PERSON_HANDLES.iter().copied().collect();

        if self.couple.len() == 1 {
            return Err(
                "/couple: a couple is two people or none; return an empty array if the evidence \
                 does not support a pair"
                    .to_string(),
            );
        }
        if self.couple.len() > 2 {
            return Err(format!(
                "/couple: {} entries; a couple is at most two",
                self.couple.len()
            ));
        }
        for handle in &self.couple {
            if !known.contains(handle.as_str()) {
                return Err(format!(
                    "/couple: \"{handle}\" is not one of the supplied candidate handles; answer \
                     only with handles from the list, never with a description of a person"
                ));
            }
        }
        if self.couple.len() == 2 && self.couple.first() == self.couple.get(1) {
            return Err("/couple: the two entries are the same person".to_string());
        }
        if self.close_family.len() > 10 {
            return Err(format!(
                "/close_family: {} entries; at most ten",
                self.close_family.len()
            ));
        }
        for handle in &self.close_family {
            if !known.contains(handle.as_str()) {
                return Err(format!(
                    "/close_family: \"{handle}\" is not one of the supplied candidate handles"
                ));
            }
        }
        if self
            .couple
            .iter()
            .any(|entry| self.close_family.contains(entry))
        {
            return Err(
                "/close_family: a member of the couple cannot also be listed as close family"
                    .to_string(),
            );
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(format!("/confidence: {} is outside 0..1", self.confidence));
        }
        if self.confidence > REASONS_REQUIRED_ABOVE && self.reasons.is_empty() {
            return Err(format!(
                "/reasons: a decision at confidence {:.2} must give at least one reason",
                self.confidence
            ));
        }
        if self.reasons.len() > 6 {
            return Err(format!(
                "/reasons: {} entries; at most six",
                self.reasons.len()
            ));
        }
        if self.reasons.iter().any(|reason| reason.trim().is_empty()) {
            return Err("/reasons: an empty reason is not a reason".to_string());
        }
        Ok(())
    }
}

/// The response schema, exactly as section 7 of the phase document specifies it.
///
/// Copied rather than generated. It is a contract with a model, the wording of the
/// constraints is part of what the model is told, and a generator that reordered the keys
/// would change the prompt hash and invalidate every cached answer in the product.
pub const COUPLE_HINT_SCHEMA: &str = r#"{
  "type": "object",
  "required": ["couple", "confidence", "reasons"],
  "properties": {
    "couple": { "type": "array", "items": { "type": "string" }, "minItems": 0, "maxItems": 2 },
    "close_family": { "type": "array", "items": { "type": "string" }, "maxItems": 10 },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 6 }
  },
  "additionalProperties": false
}"#;

/// The system prompt, exactly as section 7 of the phase document specifies it.
///
/// The second rule is the one that matters most and the one a rewrite would lose:
/// **do not infer gender, ethnicity, religion or relationships beyond couple, close
/// family and guest.** The output type cannot express those things anyway - see this
/// module's header - but a model that is told the rule writes better reasons than one
/// that is merely prevented from breaking it.
pub const COUPLE_HINT_SYSTEM: &str = "You are helping identify which two people are the \
wedding couple from statistical evidence and sample frames.\n\
Rules:\n\
- Base the answer only on photographic role evidence: who appears in getting-ready, who is \
central during the ceremony, who is photographed together in portraits.\n\
- Do not infer gender, ethnicity, religion or relationships beyond 'couple' / 'close family' / \
'guest'.\n\
- If the evidence is ambiguous, return low confidence and explain what is missing.\n\
- Return ONLY JSON matching the schema.";

/// Name the couple from statistics and a handful of blurred frames.
///
/// Holds the thumbnails rather than taking them in the input, for the same reason
/// `SegmentNaming` holds its contact sheet: the input is bound by `Hash` and a megabyte
/// of JPEG in a hash key would be paid for on every cache lookup. The images' content
/// hashes are in [`CoupleHintInput::thumbnail_hashes`] and in the prompt hash, so the
/// cache key still covers the pixels.
#[derive(Debug, Clone)]
pub struct CoupleHint {
    thumbnails: Vec<ImagePart>,
}

impl CoupleHint {
    /// A task instance for a set of already-redacted thumbnails.
    ///
    /// Truncates to [`MAX_THUMBNAILS`] rather than refusing, because a caller that
    /// assembled nine frames has made a mistake about a cap and not about the wedding,
    /// and failing the whole hint over it would cost the photographer an answer.
    #[must_use]
    pub fn for_thumbnails(mut thumbnails: Vec<ImagePart>) -> Self {
        thumbnails.truncate(MAX_THUMBNAILS);
        Self { thumbnails }
    }

    /// The thumbnails this instance will send.
    #[must_use]
    pub fn thumbnails(&self) -> &[ImagePart] {
        &self.thumbnails
    }

    /// True when the local evidence is ambiguous enough to be worth the money.
    ///
    /// Section 7's trigger. Two conditions, both required: the top two pairs are within
    /// [`ASK_WITHIN_MARGIN`], and there is a second pair at all - a wedding with one
    /// candidate pair is not ambiguous, it is under-analysed, and the answer to that is
    /// `AURA-ML-5019` rather than a cloud call.
    #[must_use]
    pub fn is_worth_asking(input: &CoupleHintInput) -> bool {
        input.pairs.len() >= 2 && input.local_margin() < ASK_WITHIN_MARGIN
    }

    /// The user turn. Deterministic, with the fields in a fixed order.
    fn render_user(input: &CoupleHintInput) -> String {
        use std::fmt::Write as _;

        let mut out = String::with_capacity(1536);
        let _ = writeln!(out, "decision: {}", input.decision_ref);
        let _ = writeln!(
            out,
            "candidates ({} of them), by opaque handle:",
            input.candidates.len()
        );
        out.push_str(&input.render_candidates());
        let _ = writeln!(out, "pairs the local scorer ranked highest:");
        out.push_str(&input.render_pairs());
        let _ = writeln!(
            out,
            "local top-two margin: {:.3} (the local scorer cannot separate them)",
            input.local_margin()
        );

        if input.scenes_available {
            let _ = writeln!(
                out,
                "scene labels are available, so the getting-ready, ceremony and couple-portrait \
                 counts above are meaningful."
            );
        } else {
            let _ = writeln!(
                out,
                "scene labels are NOT available yet, so every getting-ready, ceremony and \
                 couple-portrait count above is zero. Judge from co-occurrence, face area, \
                 centrality and the frames, and lower your confidence accordingly."
            );
        }
        let _ = writeln!(
            out,
            "the {} attached frames have their backgrounds blurred{}.",
            input.thumbnail_hashes.len(),
            if input.faces_visible {
                ""
            } else {
                " and their faces blurred as well, because the photographer has not allowed \
                 face-visible cloud reasoning"
            }
        );
        let _ = writeln!(out, "allowed_handles: {}", PERSON_HANDLES.join(", "));
        out.push_str(
            "Answer with handles only. Return the JSON object and nothing else. Return an empty \
             `couple` array if the evidence does not support a pair.",
        );
        out
    }
}

impl CloudTask for CoupleHint {
    const NAME: &'static str = "couple_hint";
    const VERSION: u16 = 1;
    type Input = CoupleHintInput;
    type Output = CoupleHintOutput;

    fn prompt(&self, input: &Self::Input) -> PromptSpec {
        PromptSpec::new(COUPLE_HINT_SYSTEM, Self::render_user(input))
            .with_images(self.thumbnails.clone())
            // 384 is comfortably above the largest valid answer - two handles, ten
            // family handles, a confidence and six short reasons - and low enough that a
            // model which starts writing an essay about who these people are is
            // truncated into a schema failure rather than billed for it.
            .with_max_tokens(384)
            // Vision reasoning tier. A cheap tier can read the statistics and cannot
            // usefully read eight blurred frames, and the frames are the only thing this
            // call adds over the local scorer.
            .with_min_tier(Tier::Balanced)
    }

    fn output_schema(&self) -> &'static str {
        COUPLE_HINT_SCHEMA
    }

    /// The local answer: the scorer's own argmax, at low confidence, saying so.
    ///
    /// It cannot fail, which is what invariant 6 asks of a fallback - not that it be
    /// good, but that it always exist. A project with no pairs at all returns an empty
    /// couple at confidence zero, which is honest and still lets the pipeline finish.
    fn local_fallback(&self, input: &Self::Input) -> Result<Self::Output, AuraError> {
        let Some(best) = input.best_pair() else {
            return Ok(CoupleHintOutput {
                couple: Vec::new(),
                close_family: Vec::new(),
                confidence: 0.0,
                reasons: vec![
                    "no candidate pairs were available, so there is nothing to choose between"
                        .to_string(),
                ],
            });
        };

        // Deliberately capped low. The local argmax is exactly the answer that was
        // ambiguous enough to trigger this call, so returning it at the local score would
        // launder an unresolved question into a confident one.
        let confidence = (input.local_margin() + 0.20).clamp(0.0, 0.45);
        Ok(CoupleHintOutput {
            couple: vec![best.a.clone(), best.b.clone()],
            close_family: Vec::new(),
            confidence,
            reasons: vec![
                format!(
                    "cloud reasoning was unavailable, so the local scorer's best pair was kept: \
                     {} and {} appear together in {} photographs",
                    best.a, best.b, best.frames
                ),
                format!(
                    "the runner-up pair was within {:.3}, so the photographer should confirm this \
                     in the People panel",
                    input.local_margin()
                ),
            ],
        })
    }

    /// Two cents, the same ceiling as `SegmentNaming`, and the arithmetic is different.
    ///
    /// Eight 256 px thumbnails are about 0.5 megapixels in total, near 700 image tokens -
    /// far less than a contact sheet, because the frames are small and there are only
    /// eight. With roughly 500 tokens of statistics and a 384 token ceiling on the
    /// answer, a balanced-tier call is about 1,200 x $3/M + 384 x $15/M, near USD 0.010.
    /// Two of those is two cents for a whole wedding, which is why section 7 can cap the
    /// count rather than the spend.
    fn max_cost_usd(&self) -> f32 {
        0.02
    }
}
