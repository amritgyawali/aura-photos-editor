//! `CleanupJudgement` - the one cloud call phase 24 makes, and the first in the product that
//! **can only make AURA do less**.
//!
//! Section 7 permits a vision-reasoning call for candidates that pass every mechanical safety check
//! but whose removability confidence sits between 0.60 and 0.90, and constrains it tightly: at most
//! twenty calls per wedding, cached, one 1024 px crop with context, skipped when cloud is off, and
//! an offline fallback of "do not remove". Every one of those is a constant, a type or a `Validate`
//! rule in this file rather than a comment.
//!
//! ## Why this one was built and phase 12's tie-breaker was not
//!
//! Phase 12 declined to build its cull tie-breaker and recorded why: with four placeholder heads
//! underneath, a 0.02 score difference is noise, and every call would have spent a photographer's
//! money asking a vision model to arbitrate between two random projections.
//!
//! The difference here is the **direction** this call can move a decision, and it is a property of
//! the output type rather than of the caller's discipline:
//!
//! * A `remove: false` answer turns a proposed removal into a refusal.
//! * A `remove: true` answer changes nothing at all. The mechanical confidence is not raised, the
//!   autonomy band is not moved, and the proposal still waits for a person.
//! * An unreachable provider, an invalid response, an exhausted budget and a cautious model all
//!   produce the same outcome, which is that the proposal waits for a person.
//!
//! A cloud call whose every failure mode is identical to its most conservative answer is a call
//! whose failure modes are all safe. `aura_generative::judgement::Answer` has no approving variant
//! for the same reason, so the property survives the trip from this crate into that one.
//!
//! ## What is sent, and what cannot be
//!
//! One crop with context, the detected class, the area fraction, the scene slug and the proposed
//! method. **No identity, no role handle, no face box and no count of who is in the frame** - phase
//! 06's rule, and this task has no reason to know: the safety engine has already established that
//! no person is inside the region, and the question is whether an *object* is part of the wedding.
//!
//! There is also no field a description of what should replace the object could go in, which is
//! `docs/generative-policy.md`'s promise expressed as a shape.

use aura_core::AuraError;
use serde::{Deserialize, Serialize};

use crate::contract::cloud::{CloudTask, ImagePart, PromptSpec, Tier, Validate};
use crate::tasks::{Scored, REASONS_REQUIRED_ABOVE};

/// The most calls one wedding may make. Section 7's cost control.
pub const MAX_CALLS_PER_PROJECT: u32 = 20;

/// The lowest removability confidence that may reach this task, per mille.
///
/// Section 7's 0.60. Below it the mechanical answer is already "not confident enough to propose",
/// so there is nothing to ask about.
pub const ASK_ABOVE_MILLI: u16 = 600;

/// The highest removability confidence that may reach this task, per mille.
///
/// Section 7's 0.90. Above it the mechanical answer is confident and a model's opinion would be
/// bought at the price of a call per frame.
pub const ASK_BELOW_MILLI: u16 = 900;

/// The crop's longest side, in pixels. Section 7.
///
/// Large enough that the *context* around the object is legible, which is the whole question: a
/// bin is extraneous in a portrait and is the subject in a photograph of the caterers packing up.
pub const CROP_PX: u32 = 1024;

/// The closed vocabulary a candidate's class is sent as.
///
/// `DistractionClass`'s own slugs, copied rather than imported: `aura-cloud` does not depend on
/// `aura-generative`, and a task's wire vocabulary is part of its prompt contract - a generated
/// list that reordered itself would change the prompt hash and invalidate every cached answer.
///
/// `background_person` and `unclassified` are **not here**, and that is not an omission. Neither
/// can reach a proposal at all - the safety engine refuses both, and migration 24 has a CHECK that
/// refuses them again - so a candidate of either class never gets as far as a judgement, and a
/// vocabulary that could express one would invite a model to reason about removing a guest.
pub const ALLOWED_CLASSES: &[&str] = &[
    "exit_sign",
    "bin",
    "cable",
    "gaffer_tape",
    "bottle",
    "chair",
    "phone_screen",
    "stray_hand",
];

/// Words a reason may not contain.
///
/// Shorter than `MomentSignificance`'s list and pointed at a different risk. That task had to be
/// stopped from describing people; this one is about objects, and the risk is a model that
/// *invents* what should be there instead - "replace it with a plant", "extend the wall". A reason
/// that reads like an instruction to a generator is a reason that will eventually be read as one.
pub const BANNED_WORDS: &[&str] = &[
    "replace it with",
    "generate",
    "fill it with",
    "add a",
    "paint in",
    "imagine",
    "beautiful",
    "ugly",
];

/// Everything the caller supplies.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CleanupJudgementInput {
    /// Stable identifier for the decision, used as the decision reference in the audit row.
    ///
    /// The proposal id, which is a digest of a rectangle and identifies no person.
    pub decision_ref: String,
    /// What the detector thinks the object is, from [`ALLOWED_CLASSES`].
    pub class: String,
    /// The share of the frame the region covers, per mille.
    ///
    /// Per-mille integers rather than floats for phase 04's reason: `CloudTask::Input` is bound by
    /// `Hash`, and a detector returning `0.010_000_1` on one run and `0.009_999_9` on the next
    /// would miss the cache and bill the user twice for the same question.
    pub area_milli: u16,
    /// Which scene, as phase 07's slug.
    pub scene: String,
    /// How the pixels would be replaced: `borrow`, `fill` or `inpaint`.
    pub method: String,
    /// The mechanical removability confidence that put this in the band, per mille.
    pub confidence_milli: u16,
    /// Where in the frame the region sits, as a coarse ninth: `top_left` to `bottom_right`.
    ///
    /// A ninth rather than coordinates, because "near the edge" is the editorially relevant fact
    /// and a precise rectangle would be a number the model would try to reason about.
    pub position: String,
    /// blake3 of the crop's bytes.
    ///
    /// In the input as well as in the prompt's image, so the audit row can prove which pixels a
    /// decision was made from after the derivative itself is discarded.
    pub crop_hash: String,
}

impl CleanupJudgementInput {
    /// The mechanical confidence, `0..1`.
    #[must_use]
    pub fn confidence(&self) -> f32 {
        f32::from(self.confidence_milli) / 1000.0
    }

    /// The area fraction, `0..1`.
    #[must_use]
    pub fn area(&self) -> f32 {
        f32::from(self.area_milli) / 1000.0
    }

    /// True when the class is one this task is allowed to be asked about.
    #[must_use]
    pub fn class_is_allowed(&self) -> bool {
        ALLOWED_CLASSES.contains(&self.class.as_str())
    }
}

/// What the model must return. Section 7's schema, exactly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CleanupJudgementOutput {
    /// Whether removing this object is safe and appropriate.
    pub remove: bool,
    /// Whether the object is part of the wedding's story.
    ///
    /// Carried beside `remove` rather than folded into it, because the two are different reasons to
    /// say no and a photographer reading the panel wants to know which: "this is part of your
    /// wedding" and "removing this would damage the photograph" lead to different actions.
    #[serde(default)]
    pub story_relevant: bool,
    /// 0.0 to 1.0.
    pub confidence: f32,
    /// Up to four short editorial reasons.
    pub reasons: Vec<String>,
}

impl Scored for CleanupJudgementOutput {
    fn confidence(&self) -> f32 {
        self.confidence.clamp(0.0, 1.0)
    }

    fn reasons(&self) -> &[String] {
        &self.reasons
    }
}

impl Validate for CleanupJudgementOutput {
    /// The rules the JSON Schema cannot express.
    ///
    /// The most important one is the last: a `remove: true` at low confidence is refused, because
    /// section 7's own instruction is "when uncertain, say NO" and an uncertain yes is exactly the
    /// answer that instruction exists to prevent. Refusing it here means an ambivalent model
    /// produces a repair retry and then the local fallback, which is a refusal.
    fn validate(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(format!("/confidence: {} is outside 0..1", self.confidence));
        }
        if self.confidence > REASONS_REQUIRED_ABOVE && self.reasons.is_empty() {
            return Err(format!(
                "/reasons: a judgement at confidence {:.2} must give at least one reason",
                self.confidence
            ));
        }
        if self.reasons.len() > 4 {
            return Err(format!(
                "/reasons: {} entries; at most four",
                self.reasons.len()
            ));
        }
        if self.reasons.iter().any(|reason| reason.trim().is_empty()) {
            return Err("/reasons: an empty reason is not a reason".to_string());
        }
        for reason in &self.reasons {
            if let Some(word) = banned_word_in(reason) {
                return Err(format!(
                    "/reasons: \"{word}\" describes what should be generated instead. Say what the \
                     object is and whether it belongs in the photograph; AURA never invents \
                     content."
                ));
            }
        }
        if self.remove && self.story_relevant {
            return Err(
                "/remove: an object that is part of the wedding story is never removed".to_string(),
            );
        }
        if self.remove && self.confidence < 0.60 {
            return Err(format!(
                "/remove: an approval at confidence {:.2} is an uncertain yes, and the instruction \
                 is to say no when uncertain",
                self.confidence
            ));
        }
        Ok(())
    }
}

/// The first banned phrase in a sentence, if any.
///
/// A plain case-insensitive substring scan rather than word-boundary matching, because every entry
/// here is a phrase whose harm does not depend on where it starts.
#[must_use]
pub fn banned_word_in(text: &str) -> Option<&'static str> {
    let haystack = text.to_ascii_lowercase();
    BANNED_WORDS
        .iter()
        .copied()
        .find(|word| haystack.contains(word))
}

/// The response schema, exactly as section 7 of the phase document specifies it.
///
/// Copied rather than generated, for `MomentSignificance`'s reason: it is a contract with a model,
/// the wording of the constraints is part of what the model is told, and a generator that reordered
/// the keys would change the prompt hash and invalidate every cached answer in the product.
pub const CLEANUP_JUDGEMENT_SCHEMA: &str = r#"{
  "type": "object",
  "required": ["remove", "confidence", "reasons"],
  "properties": {
    "remove": { "type": "boolean" },
    "story_relevant": { "type": "boolean" },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 4 }
  },
  "additionalProperties": false
}"#;

/// The system prompt, exactly as section 7 of the phase document specifies it.
///
/// Reproduced verbatim rather than reworded, because the fourth rule - "when uncertain, say NO.
/// Leaving a distraction is always better than damaging a photograph" - is the whole task, and a
/// rewrite that made it read better would make it read weaker.
pub const CLEANUP_JUDGEMENT_SYSTEM: &str =
    "You are a cautious wedding retouching supervisor reviewing a proposed object removal.\n\
Input: an image region with context, the detected object class, its size, and the scene.\n\
Task: decide whether removing this object is safe and appropriate, or whether it should be left \
alone.\n\
Rules:\n\
- Say NO if the object is part of the wedding story (decor, ritual items, gifts, cake, signage \
naming the couple, guests interacting).\n\
- Say NO if removal would require inventing structure, or if the object overlaps a person.\n\
- Say YES only for genuinely extraneous clutter (bins, cables, tape, bottles, stands, unrelated \
signage) that is clearly not part of the event.\n\
- When uncertain, say NO. Leaving a distraction is always better than damaging a photograph.\n\
- Return ONLY JSON matching the schema.";

/// Judge whether removing one detected object is editorially safe.
///
/// Holds the crop rather than taking it in the input, for `MomentSignificance`'s reason: the input
/// is bound by `Hash` and a megabyte of JPEG in a hash key would be paid for on every cache lookup.
/// The crop's content hash is in [`CleanupJudgementInput::crop_hash`] and therefore in the cache
/// key, so the key still covers the pixels.
#[derive(Debug, Clone)]
pub struct CleanupJudgement {
    crop: ImagePart,
}

impl CleanupJudgement {
    /// A task instance for one already-derived crop.
    #[must_use]
    pub const fn for_crop(crop: ImagePart) -> Self {
        Self { crop }
    }

    /// The crop this instance will send.
    #[must_use]
    pub const fn crop(&self) -> &ImagePart {
        &self.crop
    }

    /// True when this candidate is worth the money. Section 7's trigger.
    ///
    /// Two conditions and both must hold: the confidence is inside the band, and the class is one
    /// the vocabulary can express. A candidate outside the band already has its answer, and one
    /// whose class is not in [`ALLOWED_CLASSES`] never reached a proposal in the first place.
    #[must_use]
    pub fn is_worth_asking(input: &CleanupJudgementInput) -> bool {
        input.class_is_allowed()
            && input.confidence_milli >= ASK_ABOVE_MILLI
            && input.confidence_milli <= ASK_BELOW_MILLI
    }

    /// The user turn. Deterministic, with the fields in a fixed order.
    fn render_user(input: &CleanupJudgementInput) -> String {
        use std::fmt::Write as _;

        let mut out = String::with_capacity(512);
        let _ = writeln!(out, "decision: {}", input.decision_ref);
        let _ = writeln!(out, "detected object: {}", input.class);
        let _ = writeln!(
            out,
            "size: {:.1} % of the frame",
            f32::from(input.area_milli) / 10.0
        );
        let _ = writeln!(out, "position in frame: {}", input.position);
        let _ = writeln!(out, "scene: {}", input.scene);
        let _ = writeln!(
            out,
            "proposed method: {}",
            match input.method.as_str() {
                "borrow" => "copy the real pixels from another frame of the same moment",
                "fill" => "copy texture from elsewhere in this same photograph",
                _ => "a generative fill",
            }
        );
        let _ = writeln!(
            out,
            "AURA's own removability confidence: {:.2}",
            input.confidence()
        );
        let _ = writeln!(
            out,
            "the attached image is a {CROP_PX} px crop centred on the object, with its surroundings."
        );
        let _ = writeln!(
            out,
            "AURA has already checked that the region overlaps no face, skin, hands, dress, rings \
             or cake, and that it crosses no straight line. You are being asked the editorial \
             question only: does this object belong in this photograph?"
        );
        out.push_str(
            "Return the JSON object and nothing else. If you are not sure, answer remove: false.",
        );
        out
    }
}

impl CloudTask for CleanupJudgement {
    const NAME: &'static str = "cleanup_judgement";
    const VERSION: u16 = 1;
    type Input = CleanupJudgementInput;
    type Output = CleanupJudgementOutput;

    fn prompt(&self, input: &Self::Input) -> PromptSpec {
        PromptSpec::new(CLEANUP_JUDGEMENT_SYSTEM, Self::render_user(input))
            .with_images(vec![self.crop.clone()])
            // 200 is comfortably above the largest valid answer - two booleans, a number and four
            // short reasons - and low enough that a model which starts describing what should be
            // generated instead is truncated into a schema failure rather than billed for it.
            .with_max_tokens(200)
            // Vision reasoning tier. Section 7 asks for it by name: the whole value of the call is
            // what a model sees in the *context around* the object, which no number carries.
            .with_min_tier(Tier::Balanced)
    }

    fn output_schema(&self) -> &'static str {
        CLEANUP_JUDGEMENT_SCHEMA
    }

    /// The local answer: **do not remove**.
    ///
    /// Section 7's offline fallback verbatim - "do not remove; leave the proposal in the review
    /// queue for the user". It cannot fail, which is what invariant 6 asks of a fallback, and it is
    /// the only fallback in the product that is *identical to the task's most cautious answer*.
    ///
    /// That identity is the point. An unreachable provider, a spent budget, a malformed response
    /// and a model that says no all leave the photograph in exactly the same state, so there is no
    /// configuration of this product in which the cloud being available makes it remove more than
    /// it otherwise would - only less.
    fn local_fallback(&self, input: &Self::Input) -> Result<Self::Output, AuraError> {
        Ok(CleanupJudgementOutput {
            remove: false,
            story_relevant: false,
            // Low, and deliberately not zero: the fallback is a real decision to leave the
            // photograph alone rather than an absence of one, and a zero would render in the
            // Explain panel as though nothing had been decided.
            confidence: 0.30,
            reasons: vec![
                "editorial review was unavailable, so the object was left in the photograph"
                    .to_string(),
                format!(
                    "AURA's own removability confidence was {:.2}, which is inside the band where \
                     it asks for a second opinion",
                    input.confidence()
                ),
            ],
        })
    }

    /// One cent, the lowest ceiling of any task in the product.
    ///
    /// One 1024 px crop is about one megapixel, near 1,400 image tokens, plus roughly 200 tokens of
    /// context and a 200 token ceiling on the answer. A balanced-tier call is about 1,600 x $3/M +
    /// 200 x $15/M, near USD 0.008.
    ///
    /// **Twenty of those is sixteen cents a wedding**, which is what section 7's cap buys. It is
    /// the cheapest cloud task here because it asks the smallest question: one object, one crop,
    /// yes or no.
    fn max_cost_usd(&self) -> f32 {
        0.01
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A crop stand-in. The bytes never leave this test.
    fn crop() -> ImagePart {
        ImagePart {
            media_type: "image/jpeg".into(),
            bytes: vec![0u8; 4],
            content_hash: "abc123".into(),
            width: CROP_PX,
            height: CROP_PX,
        }
    }

    fn input() -> CleanupJudgementInput {
        CleanupJudgementInput {
            decision_ref: "prp_00000000-0000-4000-8000-000000000024".into(),
            class: "bin".into(),
            area_milli: 8,
            scene: "reception_entrance".into(),
            method: "fill".into(),
            confidence_milli: 750,
            position: "bottom_left".into(),
            crop_hash: "abc123".into(),
        }
    }

    #[test]
    fn the_band_is_section_sevens_and_nothing_outside_it_is_asked_about() {
        assert!(CleanupJudgement::is_worth_asking(&input()));
        let mut low = input();
        low.confidence_milli = 599;
        assert!(!CleanupJudgement::is_worth_asking(&low));
        let mut high = input();
        high.confidence_milli = 901;
        assert!(!CleanupJudgement::is_worth_asking(&high));
    }

    #[test]
    fn a_person_and_an_unnamed_object_are_not_in_the_vocabulary_at_all() {
        // Neither can reach a proposal, so neither can reach a judgement. A vocabulary that could
        // express one would invite a model to reason about removing a guest.
        assert!(!ALLOWED_CLASSES.contains(&"background_person"));
        assert!(!ALLOWED_CLASSES.contains(&"unclassified"));
        let mut person = input();
        person.class = "background_person".into();
        assert!(!CleanupJudgement::is_worth_asking(&person));
    }

    #[test]
    fn the_offline_fallback_is_do_not_remove() {
        // Section 7's offline fallback, and the property the whole design rests on: the cloud being
        // unavailable and the cloud saying no leave the photograph in the same state.
        let task = CleanupJudgement::for_crop(crop());
        let answer = task.local_fallback(&input()).expect("cannot fail");
        assert!(!answer.remove);
        assert!(!answer.reasons.is_empty());
    }

    #[test]
    fn an_uncertain_yes_is_refused() {
        // "When uncertain, say NO" as a validation rule rather than as a sentence in a prompt.
        let uncertain = CleanupJudgementOutput {
            remove: true,
            story_relevant: false,
            confidence: 0.45,
            reasons: vec!["it might be a bin".into()],
        };
        let err = uncertain
            .validate()
            .expect_err("an uncertain yes is refused");
        assert!(err.contains("uncertain"), "{err}");
    }

    #[test]
    fn a_story_relevant_object_may_never_be_approved() {
        let contradictory = CleanupJudgementOutput {
            remove: true,
            story_relevant: true,
            confidence: 0.95,
            reasons: vec!["it is a gift table but it is untidy".into()],
        };
        assert!(contradictory.validate().is_err());
    }

    #[test]
    fn a_reason_that_describes_what_to_generate_is_refused() {
        let generative = CleanupJudgementOutput {
            remove: true,
            story_relevant: false,
            confidence: 0.88,
            reasons: vec!["remove the bin and replace it with a plant".into()],
        };
        let err = generative.validate().expect_err("refused");
        assert!(err.contains("never invents"), "{err}");
    }

    #[test]
    fn a_plain_refusal_validates() {
        let no = CleanupJudgementOutput {
            remove: false,
            story_relevant: true,
            confidence: 0.91,
            reasons: vec!["the sign names the couple".into()],
        };
        assert_eq!(no.validate(), Ok(()));
    }

    #[test]
    fn a_confident_approval_validates() {
        let yes = CleanupJudgementOutput {
            remove: true,
            story_relevant: false,
            confidence: 0.93,
            reasons: vec!["a catering crate at the frame edge, unrelated to the event".into()],
        };
        assert_eq!(yes.validate(), Ok(()));
    }

    #[test]
    fn the_prompt_is_deterministic_and_names_no_person() {
        let task = CleanupJudgement::for_crop(crop());
        let first = task.prompt(&input());
        let second = task.prompt(&input());
        assert_eq!(first.user, second.user);
        // Nothing that *identifies* a person. The prompt does say the region overlaps no face,
        // which is the safety engine's finding and is the reason the model is only being asked the
        // editorial question - so `face` is not on this list and `identity` is.
        for banned in ["identity", "identity_id", "bride", "groom", "pht_", "idt_"] {
            assert!(
                !first.user.contains(banned),
                "the prompt must carry nothing that identifies a person, found {banned}"
            );
        }
    }

    #[test]
    fn the_cost_ceiling_is_the_lowest_in_the_product() {
        let task = CleanupJudgement::for_crop(crop());
        assert!(task.max_cost_usd() <= 0.01);
        let wedding = f64::from(MAX_CALLS_PER_PROJECT) * f64::from(task.max_cost_usd());
        assert!(
            (wedding - 0.20).abs() < 1e-6,
            "twenty calls at a cent is twenty cents a wedding, got {wedding}"
        );
    }
}
