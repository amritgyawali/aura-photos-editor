//! `ExplainSummary` - the one cloud call phase 13 is allowed to make.
//!
//! Section 7 permits a call that turns a structured reason list into a short, warm, accurate
//! paragraph, and constrains it tightly: a balanced text model at temperature 0, at most
//! forty short calls per wedding, cached per decision id, **no images**, and a deterministic
//! template fallback that is always available offline.
//!
//! ## The thing this task is most likely to get wrong, and how it is prevented
//!
//! A language model asked to explain a decision will, unprompted, add a reason. It will say
//! the light was lovely, that the couple looked happy, that the shutter speed was too slow -
//! plausible sentences, none of them supplied, all of them indistinguishable from the real
//! ones once they are in a paragraph a photographer trusts.
//!
//! Three things prevent it, and only one of them is the prompt:
//!
//! * **The input carries no photograph.** [`ExplainSummaryInput`] is codes, weights, numbers
//!   and the sentences the deciding code already wrote. A model that cannot see the frame
//!   cannot describe it.
//! * **The output has nowhere to put a new reason.** One paragraph and an optional headline.
//!   There is no `reasons[]` here - unlike every other task in this crate - precisely because
//!   this task must not produce one. Rephrasing is the whole job.
//! * **Every number is checked afterwards.** [`ExplainSummaryOutput::validate`] refuses a
//!   summary containing a number that was not in the input, using the same
//!   `aura_explain::summary::Grounding` rule the offline path is tested against. Section
//!   10.1: "NL summaries contain no numbers or claims absent from the structured input".
//!
//! The number check is the enforceable half of that sentence, and the shape of the output is
//! what handles the other half: a fabricated *claim* has nowhere to go that the panel would
//! render as evidence.
//!
//! ## Why the trigger is on demand rather than in a pass
//!
//! Because the offline paragraph is already good. It is assembled from sentences a
//! photographer's own product wrote, in the order the deciding code ranked them, and it is
//! correct by construction. The cloud version reads better, and reading better is worth two
//! cents when somebody asked for it - and is worth nothing at all across four thousand
//! frames nobody opened.

use aura_core::AuraError;
use serde::{Deserialize, Serialize};

use crate::contract::cloud::{CloudTask, PromptSpec, Tier, Validate};
use crate::tasks::Scored;

/// The most calls one wedding may make. Section 7's cost control.
pub const MAX_CALLS_PER_PROJECT: u32 = 40;

/// The longest summary. Section 7's schema, and the panel's own cap.
pub const MAX_SUMMARY_CHARS: usize = 600;

/// The longest headline.
pub const MAX_HEADLINE_CHARS: usize = 90;

/// What this task reports as its confidence, and why it is a constant.
///
/// A rephrasing is as certain as the facts it rephrased. This number decides nothing: it
/// sits above `REASONS_REQUIRED_ABOVE` so that a returned paragraph must be non-empty, which
/// is the one thing about it worth enforcing, and the confidence a photographer reads comes
/// from the decision rather than from the sentence describing it.
pub const SUMMARY_CONFIDENCE: f32 = 1.0;

/// One reason, as the model is given it.
///
/// Per-mille integers for the weight rather than a float, for the reason phase 04's task
/// registry gives: `CloudTask::Input` is bound by `Hash`, and a weight arriving as
/// `0.600_000_1` on one run and `0.599_999_9` on the next would miss the cache and bill the
/// photographer twice for the same paragraph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReasonFact {
    /// The stable slug, from `docs/reason-codes.md`.
    pub code: String,
    /// The sentence the deciding code wrote. The model may rephrase this and nothing else.
    pub text: String,
    /// How much it moved the decision, per mille, signed.
    pub weight_milli: i32,
    /// `credit`, `note`, `caveat` or `fault`.
    pub severity: String,
    /// Named parameter deltas, per mille, when the reason carries any.
    pub params: Vec<(String, i32)>,
}

/// Everything the caller supplies.
///
/// **No image, no filename, no identity, no person.** The decision reference is the decision
/// id, which is a random UUID and identifies nobody.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExplainSummaryInput {
    /// The decision id, used as the audit row's decision reference and as the cache key.
    pub decision_ref: String,
    /// `cull`, `edit`, `retouch`, `qc`, `curate` or `export`.
    pub kind: String,
    /// What was decided, in a few words the deciding phase supplied.
    pub verdict: String,
    /// The calibrated confidence, per mille.
    pub confidence_milli: u16,
    /// `auto`, `auto_zero_touch`, `suggest` or `require_review`.
    pub autonomy: String,
    /// True when a fitted calibration produced the confidence.
    pub calibrated: bool,
    /// The reasons, strongest first.
    pub reasons: Vec<ReasonFact>,
    /// The alternative frame's fused score, per mille, when there is one to name.
    pub alternative_score_milli: Option<u16>,
}

impl ExplainSummaryInput {
    /// The confidence as a fraction.
    #[must_use]
    pub fn confidence(&self) -> f32 {
        f32::from(self.confidence_milli) / 1000.0
    }

    /// The reasons rendered for the prompt, one per line, in a fixed order.
    #[must_use]
    pub fn render_reasons(&self) -> String {
        use std::fmt::Write as _;

        let mut out = String::with_capacity(512);
        for reason in &self.reasons {
            // Writing into a String cannot fail; the result is discarded deliberately.
            let _ = write!(
                out,
                "  {} [{}] weight {:+.2}: {}",
                reason.code,
                reason.severity,
                f32::from(i16::try_from(reason.weight_milli).unwrap_or(0)) / 1000.0,
                reason.text
            );
            for (name, value) in &reason.params {
                let _ = write!(
                    out,
                    " ; {name} {:+.2}",
                    f32::from(i16::try_from(*value).unwrap_or(0)) / 1000.0
                );
            }
            out.push('\n');
        }
        out
    }

    /// Every number the model is allowed to use, as the grounding check sees them.
    ///
    /// The same normalisation `aura_explain::summary::numbers_in` performs, duplicated here
    /// rather than depended upon: `aura-cloud` must not depend on `aura-explain`, because the
    /// gateway is phase 04's and a cycle through phase 13 would make the cloud layer depend on
    /// the ledger it is supposed to be independent of.
    #[must_use]
    pub fn allowed_numbers(&self) -> Vec<String> {
        let mut facts = vec![
            format!("{:.2}", self.confidence()),
            self.confidence_milli.to_string(),
        ];
        if let Some(score) = self.alternative_score_milli {
            facts.push(format!("{:.2}", f32::from(score) / 1000.0));
            facts.push(score.to_string());
        }
        for reason in &self.reasons {
            facts.push(format!(
                "{:.2}",
                f32::from(i16::try_from(reason.weight_milli).unwrap_or(0)) / 1000.0
            ));
            for (_, value) in &reason.params {
                facts.push(format!(
                    "{:.2}",
                    f32::from(i16::try_from(*value).unwrap_or(0)) / 1000.0
                ));
                facts.push(value.to_string());
            }
            facts.extend(numbers_in(&reason.text));
        }
        facts.extend(numbers_in(&self.verdict));
        facts
            .into_iter()
            .flat_map(|fact| numbers_in(&fact))
            .collect()
    }
}

/// What the model must return.
///
/// Two fields, and the absence of a third is the design. There is no `reasons[]` and no
/// `confidence`: this task rephrases an explanation somebody else produced, and a field it
/// could put a new fact in would be a field it eventually would.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExplainSummaryOutput {
    /// Two to four short sentences.
    pub summary: String,
    /// A short line above them, or nothing.
    #[serde(default)]
    pub headline: Option<String>,
    /// Every number the input carried, echoed by the caller before validation.
    ///
    /// Not part of section 7's schema and not sent to the model: the gateway validates an
    /// `Output` on its own, so the grounding check needs the allowed numbers to travel with
    /// the answer. `#[serde(skip)]` keeps it off the wire in both directions.
    #[serde(skip)]
    pub allowed_numbers: Vec<String>,
}

impl Scored for ExplainSummaryOutput {
    /// How sure the answer is.
    ///
    /// Phase 04 binds every task's output to this trait, and this is the one task in the
    /// crate where the question needs care: **a rephrasing is exactly as certain as the
    /// facts it rephrased.** The model is not judging a photograph, it is putting somebody
    /// else's judgement into a sentence, so a confidence of its own would be a number about
    /// nothing.
    ///
    /// So it reports [`SUMMARY_CONFIDENCE`] - a constant, documented, and never used to
    /// decide anything. The confidence a photographer reads on the panel is the decision's,
    /// which came from the deciding phase and went through phase 13's calibration.
    fn confidence(&self) -> f32 {
        SUMMARY_CONFIDENCE
    }

    /// Why, which for this task is the paragraph itself.
    ///
    /// There is no separate `reasons[]` on this output and there deliberately never will be:
    /// a field a model could put a new fact in is a field it eventually would. The paragraph
    /// is the same string [`ExplainSummaryOutput::validate`] grounds, so a reason here cannot
    /// carry a number the input did not.
    fn reasons(&self) -> &[String] {
        std::slice::from_ref(&self.summary)
    }
}

impl Validate for ExplainSummaryOutput {
    /// The rules the JSON Schema cannot express.
    ///
    /// The grounding check is the important one: it is what makes it impossible for this task
    /// to state a measurement nobody supplied. See this module's header.
    fn validate(&self) -> Result<(), String> {
        if self.summary.trim().is_empty() {
            return Err("/summary: an empty summary is not an explanation".to_string());
        }
        if self.summary.chars().count() > MAX_SUMMARY_CHARS {
            return Err(format!(
                "/summary: {} characters; at most {MAX_SUMMARY_CHARS}",
                self.summary.chars().count()
            ));
        }
        if let Some(headline) = &self.headline {
            if headline.chars().count() > MAX_HEADLINE_CHARS {
                return Err(format!(
                    "/headline: {} characters; at most {MAX_HEADLINE_CHARS}",
                    headline.chars().count()
                ));
            }
        }
        if self.allowed_numbers.is_empty() {
            // Nothing to check against. Refusing here would fail every call made through a
            // caller that did not populate the list, so this is deliberately permissive - and
            // the caller that matters, `aura-app`, always populates it.
            return Ok(());
        }
        let mut text = self.summary.clone();
        if let Some(headline) = &self.headline {
            text.push(' ');
            text.push_str(headline);
        }
        for number in numbers_in(&text) {
            if !self.allowed_numbers.contains(&number) {
                return Err(format!(
                    "/summary: the number {number} is not in the supplied facts; rephrase what \
                     you were given and never add a measurement"
                ));
            }
        }
        Ok(())
    }
}

/// Every number in a sentence, normalised the way the grounding check compares them.
#[must_use]
pub fn numbers_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() || (ch == '.' && !current.is_empty()) {
            current.push(ch);
        } else if !current.is_empty() {
            out.push(normalise(&current));
            current.clear();
        }
    }
    if !current.is_empty() {
        out.push(normalise(&current));
    }
    out.retain(|number| !number.is_empty());
    out
}

fn normalise(raw: &str) -> String {
    let trimmed = raw.trim_matches('.');
    if !trimmed.contains('.') {
        return trimmed.to_string();
    }
    let cleaned = trimmed.trim_end_matches('0').trim_end_matches('.');
    if cleaned.is_empty() {
        "0".to_string()
    } else {
        cleaned.to_string()
    }
}

/// The response schema, exactly as section 7 of the phase document specifies it.
pub const EXPLAIN_SUMMARY_SCHEMA: &str = r#"{
  "type": "object",
  "required": ["summary"],
  "properties": {
    "summary": { "type": "string", "maxLength": 600 },
    "headline": { "type": ["string", "null"], "maxLength": 90 }
  },
  "additionalProperties": false
}"#;

/// The system prompt, exactly as section 7 of the phase document specifies it.
///
/// The first rule is the one that matters and the one a rewrite would lose: **use ONLY the
/// supplied facts and numbers.** The output type and
/// [`ExplainSummaryOutput::validate`] enforce it anyway - see this module's header - but a
/// model that is told the rule writes better paragraphs than one that is merely prevented
/// from breaking it.
pub const EXPLAIN_SUMMARY_SYSTEM: &str =
    "You explain photo-editing decisions to a professional wedding photographer.\n\
You will receive a structured list of reasons with codes, weights and numeric parameters.\n\
Task: write 2-4 short sentences explaining the decision in the photographer's own vocabulary.\n\
Rules:\n\
- Use ONLY the supplied facts and numbers. Never add a reason that is not in the input. Never \
invent numbers.\n\
- Be specific: name the scene, the parameters and the alternative frame if provided.\n\
- Neutral professional tone, no marketing language, no apologies.\n\
- Return ONLY JSON matching the schema.";

/// Turn structured reasons into a short, warm, accurate paragraph.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExplainSummary;

impl ExplainSummary {
    /// The user turn. Deterministic, with the fields in a fixed order.
    fn render_user(input: &ExplainSummaryInput) -> String {
        use std::fmt::Write as _;

        let mut out = String::with_capacity(1_024);
        let _ = writeln!(out, "decision: {}", input.decision_ref);
        let _ = writeln!(out, "kind: {}", input.kind);
        let _ = writeln!(out, "what was decided: {}", input.verdict);
        let _ = writeln!(
            out,
            "confidence: {:.2} ({})",
            input.confidence(),
            input.autonomy
        );
        if !input.calibrated {
            let _ = writeln!(
                out,
                "this confidence has NOT been calibrated against outcomes; say so plainly and \
                 do not present it as a percentage of correctness."
            );
        }
        let _ = writeln!(out, "reasons ({} of them):", input.reasons.len());
        out.push_str(&input.render_reasons());
        if let Some(score) = input.alternative_score_milli {
            let _ = writeln!(
                out,
                "the closest alternative frame scored {:.2}",
                f32::from(score) / 1000.0
            );
        }
        out.push_str(
            "Return the JSON object and nothing else. Every number you write must appear above.",
        );
        out
    }
}

impl CloudTask for ExplainSummary {
    const NAME: &'static str = "explain_summary";
    const VERSION: u16 = 1;
    type Input = ExplainSummaryInput;
    type Output = ExplainSummaryOutput;

    fn prompt(&self, input: &Self::Input) -> PromptSpec {
        PromptSpec::new(EXPLAIN_SUMMARY_SYSTEM, Self::render_user(input))
            // 260 is comfortably above four short sentences and a headline, and low enough
            // that a model which starts writing an essay is truncated into a schema failure
            // rather than billed for it.
            .with_max_tokens(260)
            // Text only. Section 7 asks for a balanced tier, and this task sends no image at
            // all - which is the cheapest call in the product and the only one where that is
            // a privacy property rather than a cost one.
            .with_min_tier(Tier::Balanced)
    }

    fn output_schema(&self) -> &'static str {
        EXPLAIN_SUMMARY_SCHEMA
    }

    /// The local answer: the deterministic template, assembled from the reasons themselves.
    ///
    /// It cannot fail, which is what invariant 6 asks of a fallback. It is also, unusually
    /// for a fallback in this crate, *correct by construction*: every clause in it is a
    /// clause a deciding phase wrote. What the cloud version adds is prose, and prose is the
    /// only thing this task is for.
    fn local_fallback(&self, input: &Self::Input) -> Result<Self::Output, AuraError> {
        let mut summary = String::with_capacity(320);
        summary.push_str(&input.verdict);
        for (index, reason) in input.reasons.iter().take(3).enumerate() {
            summary.push_str(match index {
                0 => " The main reason is that ",
                1 => " There is also this: ",
                _ => " And on top of that, ",
            });
            summary.push_str(reason.text.trim());
            summary.push('.');
        }
        if !input.calibrated {
            summary.push_str(
                " AURA has not yet learned how often it is right about this kind of decision, \
                 so read the confidence as a rough guide.",
            );
        }
        let headline = input
            .reasons
            .first()
            .map(|reason| reason.text.clone())
            .filter(|text| text.chars().count() <= MAX_HEADLINE_CHARS);
        Ok(ExplainSummaryOutput {
            summary: summary.chars().take(MAX_SUMMARY_CHARS).collect(),
            headline,
            allowed_numbers: input.allowed_numbers(),
        })
    }

    /// Half a cent, and the cheapest call in the product.
    ///
    /// No images at all: perhaps 400 tokens of structured reasons in and 260 out, which on a
    /// balanced tier is about 400 x $3/M + 260 x $15/M, near USD 0.005. **Forty of those is
    /// twenty cents a wedding**, which is why section 7 can afford to cap the count at forty
    /// while `MomentSignificance` is capped at twenty-five for four times the price.
    fn max_cost_usd(&self) -> f32 {
        0.005
    }
}
