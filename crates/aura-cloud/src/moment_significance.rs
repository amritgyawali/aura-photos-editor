//! `MomentSignificance` - the one cloud call phase 10 is allowed to make.
//!
//! Section 7 permits an optional call when the local ranker cannot separate two moments,
//! and it constrains it tightly: at most twenty-five calls per wedding, batched per
//! moment, cached, up to six 768 px thumbnails, and a local fallback that keeps the
//! pipeline complete. Every one of those constraints is in this file as a constant, a
//! type or a `Validate` rule rather than as a comment.
//!
//! ## The thing this task is most likely to get wrong, and how it is prevented
//!
//! A vision model asked "how important is this moment" will, unprompted, describe the
//! people in it: how they look, what they are wearing, who is attractive, what they must
//! be feeling. Section 2.2 puts psychological claims permanently out of scope and section
//! 12's second failure mode is overclaiming emotion recognition, and a prompt rule alone
//! would not keep either out of `reasons[]`.
//!
//! So three things are structural rather than instructed:
//!
//! * **The subjects are roles, not people.** [`ROLE_HANDLES`] is a fixed six-word
//!   vocabulary - `primary_a`, `primary_b`, `close_family`, `guest`, `child`, `vendor` -
//!   assigned locally from phase 06's `Role`, and the mapping from handle to identity
//!   never leaves the machine. No name, no identity id, no face template, no landmark, no
//!   embedding.
//! * **The output cannot name anybody.** [`MomentSignificanceOutput`] has a number, an
//!   index and reasons. There is no field a description of a person could go in except
//!   `reasons`, and [`MomentSignificanceOutput::validate`] refuses a reason containing
//!   any of the banned appearance words.
//! * **The answer cannot cull.** `significance` raises
//!   `ImageEmotion::narrative_weight`, which is carried *beside* `emotion_score` rather
//!   than folded into it, so a caller can always see what the local model said on its
//!   own. Phase 04's rule - cloud proposes, deterministic code decides - as a column.
//!
//! ## Why the trigger is so narrow
//!
//! Because the local answer is usually right and the cloud answer is usually the same.
//! Section 7's trigger is two moment-level scores within 0.05, or an unrecognised ritual
//! peak - the cases where the local ranker genuinely does not separate them. Twenty-five
//! calls over a wedding of eight hundred moments is three percent of them.

use aura_core::AuraError;
use serde::{Deserialize, Serialize};

use crate::contract::cloud::{CloudTask, ImagePart, PromptSpec, Tier, Validate};
use crate::tasks::{Scored, REASONS_REQUIRED_ABOVE};

/// The fixed vocabulary of subject handles.
///
/// Six roles, assigned locally from phase 06's `Role`. A closed list rather than an open
/// one is what lets [`Validate`] refuse a hallucinated subject, and what stops the model
/// answering with a description of somebody.
///
/// `primary_a` and `primary_b` rather than `bride` and `groom`, for the reason phase 06's
/// `Role::Couple` exists: which of two people is the bride is not a photographic fact,
/// and a vocabulary that could express it would invite a model to guess.
pub const ROLE_HANDLES: &[&str] = &[
    "primary_a",
    "primary_b",
    "close_family",
    "guest",
    "child",
    "vendor",
];

/// The most thumbnails one call may carry. Section 7.
pub const MAX_THUMBNAILS: usize = 6;

/// The most calls one wedding may make. Section 7's cost control.
pub const MAX_CALLS_PER_PROJECT: u32 = 25;

/// How close two moment scores must be before the call is worth making.
///
/// Section 7's 0.05. The same number as `CoupleHint::ASK_WITHIN_MARGIN` and
/// `aura_vision::face::roles::COUPLE_AMBIGUOUS_MARGIN`, and it is the same number a third
/// time on purpose: "the local evidence does not separate these" has one threshold in
/// this product.
pub const ASK_WITHIN_MARGIN: f32 = 0.05;

/// Words a reason may not contain.
///
/// The appearance and psychology vocabulary, refused rather than discouraged. It is a
/// blunt instrument and it is deliberately blunt: the cost of refusing a legitimate
/// sentence is one repair retry, and the cost of letting one through is a product that
/// told a photographer their client looked beautiful.
///
/// Matched case-insensitively on word boundaries, so "guest" is fine and "gorgeous" is
/// not. The list is short because a long one would refuse most valid editorial language;
/// what it covers is the four categories section 7's prompt names - appearance, body,
/// ethnicity, attractiveness - plus the psychological verbs.
pub const BANNED_WORDS: &[&str] = &[
    "attractive",
    "beautiful",
    "beauty",
    "pretty",
    "handsome",
    "gorgeous",
    "ugly",
    "fat",
    "thin",
    "slim",
    "skin",
    "ethnicity",
    "race",
    "felt",
    "feeling",
    "feelings",
    "emotional",
    "overwhelmed",
    "heartbroken",
    "in love",
];

/// What is known about one frame of the moment, as counts and labels.
///
/// Per-mille integers rather than floats, for the reason phase 04's task registry gives:
/// `CloudTask::Input` is bound by `Hash`, and a ranker returning `0.640_000_1` on one run
/// and `0.639_999_9` on the next would miss the cache and bill the user twice for the
/// same question.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FrameStats {
    /// Position in the moment, zero-based. What `best_index` refers to.
    pub index: u32,
    /// The local emotion score, per mille.
    pub score_milli: u16,
    /// The interactions detected, as slugs, strongest first.
    pub interactions: Vec<String>,
    /// The subject roles present, from [`ROLE_HANDLES`], in vocabulary order.
    pub roles: Vec<String>,
    /// How many faces are in the frame. A count, never who they are.
    pub faces: u32,
}

/// Everything the caller supplies.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MomentSignificanceInput {
    /// Stable identifier for the decision, used as the decision reference in the audit
    /// row. The moment id, which is a random UUID and identifies no person.
    pub decision_ref: String,
    /// The chapter this moment sits in, as phase 07's slug.
    pub chapter: String,
    /// The scene most of its frames are, as phase 07's slug.
    pub scene: String,
    /// The rite, when phase 07 named one. A taxonomy slug, never a description.
    pub ritual: Option<String>,
    /// The frames, in timeline order.
    pub frames: Vec<FrameStats>,
    /// The local ranker's top-two margin across the moment, per mille.
    pub local_margin_milli: u16,
    /// True when phase 06 has assigned identities, so the role handles mean something.
    pub roles_available: bool,
    /// blake3 of each thumbnail's bytes, in order.
    ///
    /// In the input as well as in the prompt's images, so the audit row can prove which
    /// frames a decision was made from after the derivative itself is discarded.
    pub thumbnail_hashes: Vec<String>,
}

impl MomentSignificanceInput {
    /// The local ranker's best frame, by score.
    #[must_use]
    pub fn best_frame(&self) -> Option<&FrameStats> {
        self.frames.iter().max_by_key(|frame| frame.score_milli)
    }

    /// The gap between the top two local scores, `0..1`.
    #[must_use]
    pub fn local_margin(&self) -> f32 {
        f32::from(self.local_margin_milli) / 1000.0
    }

    /// The frames rendered for the prompt, one per line, in a fixed order.
    #[must_use]
    pub fn render_frames(&self) -> String {
        use std::fmt::Write as _;

        let mut out = String::with_capacity(512);
        for frame in &self.frames {
            let interactions = if frame.interactions.is_empty() {
                "none detected".to_string()
            } else {
                frame.interactions.join(", ")
            };
            let roles = if frame.roles.is_empty() {
                "unassigned".to_string()
            } else {
                frame.roles.join(", ")
            };
            // Writing into a String cannot fail; the result is discarded deliberately.
            let _ = writeln!(
                out,
                "  frame {}: local score {:.2}, interactions [{}], subjects [{}], {} faces",
                frame.index,
                f32::from(frame.score_milli) / 1000.0,
                interactions,
                roles,
                frame.faces
            );
        }
        out
    }
}

/// What the model must return.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MomentSignificanceOutput {
    /// How important this moment is to the wedding's story, `0..1`.
    pub significance: f32,
    /// Which frame of the moment is the strongest, zero-based.
    pub best_index: u32,
    /// A short label for what kind of moment this is, or nothing.
    ///
    /// Free text and deliberately so: this is the one field where a model naming
    /// something the taxonomy has no word for is *useful* - section 7's trigger includes
    /// "an unrecognised ritual peak". It is stored nowhere, shown in the Explain panel
    /// only, and length-capped so it cannot become a paragraph.
    #[serde(default)]
    pub moment_type: Option<String>,
    /// 0.0 to 1.0.
    pub confidence: f32,
    /// Up to five short editorial reasons.
    pub reasons: Vec<String>,
}

impl Scored for MomentSignificanceOutput {
    fn confidence(&self) -> f32 {
        self.confidence.clamp(0.0, 1.0)
    }

    fn reasons(&self) -> &[String] {
        &self.reasons
    }
}

impl Validate for MomentSignificanceOutput {
    /// The rules the JSON Schema cannot express.
    ///
    /// The banned-word check is the important one: it is what makes it impossible for
    /// this task to return a comment on somebody's appearance or a claim about what
    /// they were feeling. See this module's header.
    fn validate(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.significance) {
            return Err(format!(
                "/significance: {} is outside 0..1",
                self.significance
            ));
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(format!("/confidence: {} is outside 0..1", self.confidence));
        }
        if self.confidence > REASONS_REQUIRED_ABOVE && self.reasons.is_empty() {
            return Err(format!(
                "/reasons: a judgement at confidence {:.2} must give at least one reason",
                self.confidence
            ));
        }
        if self.reasons.len() > 5 {
            return Err(format!(
                "/reasons: {} entries; at most five",
                self.reasons.len()
            ));
        }
        if self.reasons.iter().any(|reason| reason.trim().is_empty()) {
            return Err("/reasons: an empty reason is not a reason".to_string());
        }
        if let Some(label) = &self.moment_type {
            if label.chars().count() > 40 {
                return Err(
                    "/moment_type: a label is a few words; write the rest in a reason".to_string(),
                );
            }
        }
        for reason in &self.reasons {
            if let Some(word) = banned_word_in(reason) {
                return Err(format!(
                    "/reasons: \"{word}\" describes appearance or an inner state; describe what is \
                     visible and what it means for the wedding story instead"
                ));
            }
        }
        if let Some(label) = &self.moment_type {
            if let Some(word) = banned_word_in(label) {
                return Err(format!(
                    "/moment_type: \"{word}\" describes appearance or an inner state; name the \
                     event instead"
                ));
            }
        }
        Ok(())
    }
}

/// The first banned word in a sentence, if any.
///
/// Word-boundary matching on an ASCII-lowercased copy. `BANNED_WORDS` holds two-word
/// entries as well, so a plain token scan would miss "in love"; the check is a substring
/// scan with boundary tests on both ends, which handles both.
#[must_use]
pub fn banned_word_in(text: &str) -> Option<&'static str> {
    let haystack = text.to_ascii_lowercase();
    let bytes = haystack.as_bytes();
    let alphabetic_at =
        |index: usize| -> bool { bytes.get(index).is_some_and(u8::is_ascii_alphabetic) };
    BANNED_WORDS.iter().copied().find(|word| {
        let mut from = 0usize;
        while let Some(offset) = haystack.get(from..).and_then(|rest| rest.find(word)) {
            let start = from + offset;
            let end = start + word.len();
            let left_clear = start == 0 || !alphabetic_at(start - 1);
            let right_clear = end >= bytes.len() || !alphabetic_at(end);
            if left_clear && right_clear {
                return true;
            }
            from = end;
        }
        false
    })
}

/// The response schema, exactly as section 7 of the phase document specifies it.
///
/// Copied rather than generated. It is a contract with a model, the wording of the
/// constraints is part of what the model is told, and a generator that reordered the keys
/// would change the prompt hash and invalidate every cached answer in the product.
pub const MOMENT_SIGNIFICANCE_SCHEMA: &str = r#"{
  "type": "object",
  "required": ["significance", "best_index", "confidence", "reasons"],
  "properties": {
    "significance": { "type": "number", "minimum": 0, "maximum": 1 },
    "best_index": { "type": "integer", "minimum": 0 },
    "moment_type": { "type": ["string", "null"] },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 5 }
  },
  "additionalProperties": false
}"#;

/// The system prompt, exactly as section 7 of the phase document specifies it.
///
/// The second and third rules are the ones that matter most and the ones a rewrite would
/// lose: **do not comment on appearance, body, ethnicity or attractiveness**, and **do
/// not describe emotions as psychological facts**. The output type and
/// [`MomentSignificanceOutput::validate`] enforce both anyway - see this module's header -
/// but a model that is told the rules writes better reasons than one that is merely
/// prevented from breaking them.
pub const MOMENT_SIGNIFICANCE_SYSTEM: &str =
    "You are a wedding photo editor deciding how important a moment is to the wedding story.\n\
Input: frames from one moment, the chapter, detected interactions and anonymised subject roles.\n\
Task: rate narrative significance 0-1, pick the single strongest frame index, and explain in \
short editorial reasons.\n\
Rules:\n\
- Judge storytelling value: is this a milestone, a peak reaction, a unique moment, or a repeat?\n\
- Do not comment on appearance, body, ethnicity or attractiveness. Never speculate about \
relationships beyond the given roles.\n\
- Do not describe emotions as psychological facts; describe what is visible.\n\
- Return ONLY JSON matching the schema.";

/// Rate one moment's narrative significance from a contact sheet.
///
/// Holds the thumbnails rather than taking them in the input, for `CoupleHint`'s reason:
/// the input is bound by `Hash` and a megabyte of JPEG in a hash key would be paid for on
/// every cache lookup. The images' content hashes are in
/// [`MomentSignificanceInput::thumbnail_hashes`] and in the prompt hash, so the cache key
/// still covers the pixels.
#[derive(Debug, Clone)]
pub struct MomentSignificance {
    thumbnails: Vec<ImagePart>,
}

impl MomentSignificance {
    /// A task instance for a set of already-derived thumbnails.
    ///
    /// Truncates to [`MAX_THUMBNAILS`] rather than refusing, for `CoupleHint`'s reason: a
    /// caller that assembled nine frames has made a mistake about a cap and not about the
    /// wedding.
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
    /// Section 7's trigger, both halves of it: the moment's top two frames are within
    /// [`ASK_WITHIN_MARGIN`], **or** phase 07 named no rite in a chapter where it usually
    /// would. Two frames minimum, because a moment of one frame has no ambiguity to
    /// resolve - it has one answer, and asking a model to confirm it is a call nobody
    /// gets anything from.
    #[must_use]
    pub fn is_worth_asking(input: &MomentSignificanceInput) -> bool {
        if input.frames.len() < 2 {
            return false;
        }
        let ambiguous = input.local_margin() < ASK_WITHIN_MARGIN;
        let unnamed_rite = input.ritual.is_none() && input.chapter == "rituals";
        ambiguous || unnamed_rite
    }

    /// The user turn. Deterministic, with the fields in a fixed order.
    fn render_user(input: &MomentSignificanceInput) -> String {
        use std::fmt::Write as _;

        let mut out = String::with_capacity(1024);
        let _ = writeln!(out, "decision: {}", input.decision_ref);
        let _ = writeln!(out, "chapter: {}", input.chapter);
        let _ = writeln!(out, "scene: {}", input.scene);
        let _ = writeln!(
            out,
            "rite: {}",
            input
                .ritual
                .as_deref()
                .unwrap_or("none named by the local classifier")
        );
        let _ = writeln!(out, "frames ({} of them):", input.frames.len());
        out.push_str(&input.render_frames());
        let _ = writeln!(
            out,
            "local top-two margin across the moment: {:.3}",
            input.local_margin()
        );

        if input.roles_available {
            let _ = writeln!(
                out,
                "subject roles are anonymised handles from a fixed list and are meaningful."
            );
        } else {
            let _ = writeln!(
                out,
                "subject identities have NOT been assigned yet, so every role above reads \
                 'unassigned'. Judge from the interactions and the frames, and lower your \
                 confidence accordingly."
            );
        }
        let _ = writeln!(
            out,
            "the {} attached frames are 768 px derivatives of this moment, in the order listed.",
            input.thumbnail_hashes.len()
        );
        let _ = writeln!(out, "allowed_roles: {}", ROLE_HANDLES.join(", "));
        out.push_str(
            "Return the JSON object and nothing else. `best_index` must be one of the frame \
             indices listed above.",
        );
        out
    }
}

impl CloudTask for MomentSignificance {
    const NAME: &'static str = "moment_significance";
    const VERSION: u16 = 1;
    type Input = MomentSignificanceInput;
    type Output = MomentSignificanceOutput;

    fn prompt(&self, input: &Self::Input) -> PromptSpec {
        PromptSpec::new(MOMENT_SIGNIFICANCE_SYSTEM, Self::render_user(input))
            .with_images(self.thumbnails.clone())
            // 320 is comfortably above the largest valid answer - two numbers, an index,
            // a short label and five short reasons - and low enough that a model which
            // starts describing the people in the frames is truncated into a schema
            // failure rather than billed for it.
            .with_max_tokens(320)
            // Vision reasoning tier. Section 7 asks for it by name, and the reason is
            // that the whole value of this call is what a model sees in six frames that
            // nine numbers do not say.
            .with_min_tier(Tier::Balanced)
    }

    fn output_schema(&self) -> &'static str {
        MOMENT_SIGNIFICANCE_SCHEMA
    }

    /// The local answer: the ranker's own argmax, at zero significance, saying so.
    ///
    /// It cannot fail, which is what invariant 6 asks of a fallback - not that it be
    /// good, but that it always exist. **`significance` is zero rather than a guess**,
    /// because section 7's offline fallback is "local ranker output only, with
    /// `narrative_weight = 0` and lower confidence": a narrative judgement is exactly the
    /// thing the local model cannot make, and inventing one would be worse than admitting
    /// it.
    fn local_fallback(&self, input: &Self::Input) -> Result<Self::Output, AuraError> {
        let best = input.best_frame();
        Ok(MomentSignificanceOutput {
            significance: 0.0,
            best_index: best.map_or(0, |frame| frame.index),
            moment_type: None,
            // Capped low for `CoupleHint::local_fallback`'s reason: the local argmax is
            // exactly the answer that was ambiguous enough to trigger this call, so
            // returning it at the local score would launder an unresolved question into a
            // confident one.
            confidence: (input.local_margin() + 0.15).clamp(0.0, 0.40),
            reasons: vec![
                "cloud reasoning was unavailable, so the local ranker's strongest frame was kept \
                 and no narrative weight was added"
                    .to_string(),
                format!(
                    "the top two frames of this moment were within {:.3}, so the choice between \
                     them is not one the local ranker separated",
                    input.local_margin()
                ),
            ],
        })
    }

    /// Two cents, the same ceiling as `SegmentNaming` and `CoupleHint`, and the
    /// arithmetic is different again.
    ///
    /// Six 768 px thumbnails are about 3.5 megapixels in total, near 4,700 image tokens -
    /// far more than `CoupleHint`'s eight small frames, because section 7 asks for 768 px
    /// so that an expression is legible. With roughly 350 tokens of statistics and a 320
    /// token ceiling on the answer, a balanced-tier call is about 5,050 x $3/M + 320 x
    /// $15/M, near USD 0.020.
    ///
    /// **Twenty-five of those is fifty cents a wedding**, which is why section 7 caps the
    /// count at twenty-five and why the trigger is two conditions rather than one. The
    /// cost governor prices this before it calls, and a project that would exceed its
    /// budget gets the local fallback with its reason saying so.
    fn max_cost_usd(&self) -> f32 {
        0.02
    }
}
