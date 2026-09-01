//! The bounded agentic planner. PHASE-27 sections 6.2 and 7.
//!
//! ## The property this module is built around
//!
//! Section 6.2: "the planner never executes anything; it proposes remedies which the mechanical
//! engine validates against policy before applying."
//!
//! Phase 24 established that a promise like that has to be a property of a *type* rather than a
//! discipline the caller keeps, and this module copies the mechanism. [`ProposedStep`] is **not** a
//! `Remedy`. It holds a remedy kind as a string, a target as a string and an optional magnitude, and
//! the only function in this crate that turns one into the other is [`crate::remedy::validate`],
//! which takes the ticket, the frame and the policy and refuses anything outside the contract's
//! bounds.
//!
//! So every failure mode is identical: an unreachable provider, a spent budget, a malformed
//! response, a hallucinated parameter name, a magnitude of 3.0, a replacement naming a frame from
//! another wedding, and a cautious model that recommends escalation all leave the image with its
//! mechanical triage. There is no configuration of this product in which the cloud being reachable
//! makes it do something it could not otherwise do - only something *different*, from a menu the
//! deterministic code owns.
//!
//! ## The planner is denied identity
//!
//! [`QcPlanInput`] has no field an identity handle, a role, a name or a face count could go in.
//! Phase 06's rule, and this task has no reason to know: the question is whether a set of
//! measurements has a common cause, and "the bride" is not evidence about that. The crops it
//! receives are named by position rather than by person.
//!
//! ## What "bounded" means, precisely
//!
//! Four numbers, all from section 7 and all constants in `aura_core::contract::qc`:
//! `MAX_PLANNER_CALLS` per wedding, `MAX_PLAN_STEPS` per plan, `MAX_TOOL_STEPS` per invocation, and
//! `PLANNER_TICKET_FLOOR` open tickets before an image is eligible at all. The first is checked by
//! the pass, the second and third by [`QcPlanOutput::validate`], and the fourth by
//! [`crate::triage::needs_planner`].

use aura_cloud::contract::cloud::{CloudTask, ImagePart, PromptSpec, Tier, Validate};
use aura_core::contract::qc::{
    QcCategory, QcCode, QcTicket, MAX_PLAN_STEPS, MAX_TOOL_STEPS, MIN_STRENGTH_FACTOR,
};
use aura_core::AuraError;
use serde::{Deserialize, Serialize};

/// The most crops one call may send. Section 7: "up to 3 crops (subject, background, comparison
/// anchor)".
pub const MAX_CROPS: usize = 3;

/// The longest side of each crop, in pixels.
///
/// Matching phase 24's `CROP_PX`. A larger crop buys nothing here: the question is about a colour
/// cast, a halo or a smear, all of which are visible at this size, and the cost is linear in pixels.
pub const CROP_PX: u32 = 1024;

/// The system prompt, verbatim from section 7.
///
/// Kept as a constant rather than assembled, because `CloudTask::VERSION` keys the response cache
/// and a prompt that differed between two builds at the same version would serve one build's answer
/// to the other. Phase 04's rule.
pub const QC_PLANNER_SYSTEM: &str = "\
You are a senior retoucher reviewing an automatically edited wedding photograph that failed several \
quality checks.
Input: quantified findings, the current edit recipe summary, reference-frame statistics for this \
scene, and image crops.
Task: produce an ordered remediation plan using ONLY the allowed remedies, or recommend escalation \
to a human.
Rules:
- Fix root causes before symptoms: if white balance is wrong, do not reduce retouch strength.
- Never propose a remedy that is not in the allowed list. Never invent parameter values outside the \
stated bounds.
- Prefer the smallest change that resolves the finding. Prefer escalation over a risky fix on a \
must-have moment.
- Explain each step in one short sentence referencing the specific finding.
- Return ONLY JSON matching the schema.";

/// The response schema, verbatim from section 7.
pub const QC_PLAN_SCHEMA: &str = r#"{
  "type": "object",
  "required": ["plan", "confidence"],
  "properties": {
    "plan": {
      "type": "array", "maxItems": 4,
      "items": {
        "type": "object",
        "required": ["remedy", "target", "reason"],
        "properties": {
          "remedy": { "type": "string", "enum": ["resolve_param", "reduce_strength", "revert_op", "replace_frame", "escalate"] },
          "target": { "type": "string" },
          "magnitude": { "type": ["number", "null"] },
          "reason": { "type": "string" }
        },
        "additionalProperties": false
      }
    },
    "root_cause": { "type": ["string", "null"] },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
  },
  "additionalProperties": false
}"#;

// ---------------------------------------------------------------------------
// What is sent
// ---------------------------------------------------------------------------

/// One finding, as the planner sees it.
///
/// Numbers and slugs. No identity, no name, no role - see this module's header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash)]
pub struct FindingSummary {
    /// Which inspection, as `QcCategory::as_str`.
    pub category: String,
    /// What was found, as `QcCode::as_str`.
    pub code: String,
    /// How far out, in thousandths of the unit, so the input hashes deterministically.
    ///
    /// Integers rather than floats because `CloudTask::Input` is `Hash` and the cache key is built
    /// from it. Two runs whose deviations differ in the seventh decimal must produce the same cache
    /// key, or the cache never hits and section 7's forty-call ceiling buys nothing.
    pub deviation_milli: i64,
    /// The threshold, same units.
    pub threshold_milli: i64,
    /// The unit both are in.
    pub unit: String,
}

impl FindingSummary {
    /// One ticket, flattened.
    #[must_use]
    pub fn of(ticket: &QcTicket) -> Self {
        Self {
            category: ticket.category.as_str().to_string(),
            code: ticket.code.as_str().to_string(),
            deviation_milli: milli(ticket.deviation),
            threshold_milli: milli(ticket.threshold),
            unit: ticket.category.unit().to_string(),
        }
    }
}

/// Everything one planner call is told.
///
/// Section 7's "ticket list with quantified deviations, the recipe summary, node anchor statistics,
/// and up to 3 crops". The crops travel on the [`QcPlanner`] rather than in this struct, because
/// `CloudTask::Input` is `Hash` and image bytes in a hash key would make the cache miss on a
/// re-encode of identical pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash)]
pub struct QcPlanInput {
    /// The frame, as an opaque handle. A `to_db` string, never a path and never a filename.
    pub image_ref: String,
    /// The scene slug. Invariant 7: the model is told what kind of photograph this is.
    pub scene: String,
    /// The findings, worst first.
    pub findings: Vec<FindingSummary>,
    /// The current edit, as a short list of `(name, value_milli)` pairs.
    ///
    /// A summary rather than the recipe: the model is asked to reason about *causes*, and a full
    /// recipe would invite it to propose a value - which no remedy in this phase can express.
    pub recipe_summary: Vec<(String, i64)>,
    /// The scene node's own statistics, as `(name, value_milli)`.
    pub node_stats: Vec<(String, i64)>,
    /// Whether a runner-up frame exists at all.
    ///
    /// A boolean rather than its id, because a plan proposing `replace_frame` is validated against
    /// the frame's actual runner-up anyway - and telling the model the id would let it propose a
    /// specific swap it has no basis to prefer.
    pub has_runner_up: bool,
    /// Whether this frame is protected by a coverage guarantee.
    ///
    /// Section 7's prompt asks it to "prefer escalation over a risky fix on a must-have moment", so
    /// it has to be told which those are.
    pub must_have: bool,
    /// A digest of the crops, so the cache key covers what was seen.
    pub crops_hash: String,
}

/// A value in thousandths, saturating and finite.
fn milli(value: f32) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    (f64::from(value) * 1_000.0).round() as i64
}

// ---------------------------------------------------------------------------
// What comes back
// ---------------------------------------------------------------------------

/// One step of a proposed plan.
///
/// **Not a `Remedy`.** See this module's header: the only route from here to something that can be
/// applied is [`crate::remedy::validate`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedStep {
    /// One of `Remedy::KINDS`. Anything else fails validation.
    pub remedy: String,
    /// What to act on: a solve target slug, an operation name, or the word `runner_up`.
    pub target: String,
    /// The magnitude, for `reduce_strength` only.
    #[serde(default)]
    pub magnitude: Option<f32>,
    /// One sentence referencing the specific finding.
    pub reason: String,
}

/// A whole plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QcPlanOutput {
    /// The steps, in the order they should be applied. At most [`MAX_PLAN_STEPS`].
    pub plan: Vec<ProposedStep>,
    /// What the model thinks is underneath all of it.
    #[serde(default)]
    pub root_cause: Option<String>,
    /// How sure it is, `0..1`.
    pub confidence: f32,
}

impl QcPlanOutput {
    /// The plan the local fallback returns: escalate, and say why.
    ///
    /// Section 7's offline fallback is "mechanical priority ordering with single-remedy-per-round and
    /// escalation on failure". The mechanical ordering is `crate::triage::order` and runs whether or
    /// not this task was reached; what this fallback contributes is the *escalation*, which is the
    /// answer that changes nothing.
    #[must_use]
    pub fn escalation(note: impl Into<String>) -> Self {
        Self {
            plan: vec![ProposedStep {
                remedy: "escalate".into(),
                target: "image".into(),
                magnitude: None,
                reason: note.into(),
            }],
            root_cause: None,
            // Low and deliberately not zero: deciding to hand a frame to a person is a real
            // decision, and a zero would render in the Explain panel as though nothing had been
            // decided. Phase 24's `CleanupJudgement` fallback makes the same choice.
            confidence: 0.30,
        }
    }

    /// The steps, in order, that are worth trying to validate.
    ///
    /// Escalation steps are dropped here rather than validated: escalating is what happens when no
    /// step survives, so a plan of `[escalate]` and a plan of `[]` reach the loop identically.
    #[must_use]
    pub fn actionable(&self) -> Vec<&ProposedStep> {
        self.plan
            .iter()
            .filter(|step| step.remedy != "escalate")
            .collect()
    }
}

impl Validate for QcPlanOutput {
    fn validate(&self) -> Result<(), String> {
        if self.plan.len() > MAX_PLAN_STEPS {
            return Err(format!(
                "plan has {} steps; at most {MAX_PLAN_STEPS} are allowed",
                self.plan.len()
            ));
        }
        if !(0.0..=1.0).contains(&self.confidence) || !self.confidence.is_finite() {
            return Err(format!(
                "confidence must be between 0 and 1, not {}",
                self.confidence
            ));
        }
        for (index, step) in self.plan.iter().enumerate() {
            if !aura_core::contract::qc::Remedy::KINDS.contains(&step.remedy.as_str()) {
                return Err(format!(
                    "step {index} names remedy '{}', which is not one of the five allowed \
                     remedies",
                    step.remedy
                ));
            }
            if step.target.trim().is_empty() {
                return Err(format!("step {index} has an empty target"));
            }
            if step.reason.trim().is_empty() {
                return Err(format!(
                    "step {index} has no reason; every step must reference the specific finding it \
                     addresses"
                ));
            }
            match (step.remedy.as_str(), step.magnitude) {
                ("reduce_strength", None) => {
                    return Err(format!(
                        "step {index} reduces a strength without a magnitude"
                    ))
                }
                ("reduce_strength", Some(value)) => {
                    // The bound is checked again in `remedy::validate`, and both layers are here on
                    // purpose: this one produces a sentence the repair retry sends back to the
                    // model, and that one is what stops a value reaching a photograph.
                    if !value.is_finite()
                        || !(MIN_STRENGTH_FACTOR..=aura_core::contract::qc::MAX_STRENGTH_FACTOR)
                            .contains(&value)
                    {
                        return Err(format!(
                            "step {index} proposes a strength of {value}; strengths must sit \
                             between {MIN_STRENGTH_FACTOR} and \
                             {} and this remedy may only reduce",
                            aura_core::contract::qc::MAX_STRENGTH_FACTOR
                        ));
                    }
                }
                (_, Some(_)) => {
                    return Err(format!(
                        "step {index} carries a magnitude, which only reduce_strength takes"
                    ))
                }
                _ => {}
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The task
// ---------------------------------------------------------------------------

/// Section 7's one cloud call.
#[derive(Debug, Clone, Default)]
pub struct QcPlanner {
    /// The crops. At most [`MAX_CROPS`], each at most [`CROP_PX`] on its long side.
    pub crops: Vec<ImagePart>,
}

impl QcPlanner {
    /// The user message: the numbers, then what they are about.
    fn render_user(input: &QcPlanInput) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "scene: {}. this photograph failed {} quality checks.",
            input.scene,
            input.findings.len()
        );
        out.push_str("findings, worst first:\n");
        for finding in &input.findings {
            let _ = writeln!(
                out,
                "- {} ({}): {:.3} {} against a {:.3} {} threshold",
                finding.category,
                finding.code,
                finding.deviation_milli as f64 / 1000.0,
                finding.unit,
                finding.threshold_milli as f64 / 1000.0,
                finding.unit
            );
        }
        if !input.recipe_summary.is_empty() {
            out.push_str("current edit:\n");
            for (name, value) in &input.recipe_summary {
                let _ = writeln!(out, "- {name}: {:.3}", *value as f64 / 1000.0);
            }
        }
        if !input.node_stats.is_empty() {
            out.push_str("what the reference frames for this part of the day measure:\n");
            for (name, value) in &input.node_stats {
                let _ = writeln!(out, "- {name}: {:.3}", *value as f64 / 1000.0);
            }
        }
        let _ = writeln!(
            out,
            "an alternative frame from the same moment {}.",
            if input.has_runner_up {
                "exists"
            } else {
                "does not exist, so replace_frame is not available"
            }
        );
        if input.must_have {
            out.push_str(
                "this frame is the only coverage of a moment the gallery has to include, so \
                 prefer escalation over a risky fix.\n",
            );
        }
        out.push_str(
            "Return the JSON object and nothing else. If no ordered plan resolves these findings, \
             return a single escalate step.",
        );
        out
    }
}

impl CloudTask for QcPlanner {
    const NAME: &'static str = "qc_plan";
    const VERSION: u16 = 1;
    type Input = QcPlanInput;
    type Output = QcPlanOutput;

    fn prompt(&self, input: &Self::Input) -> PromptSpec {
        PromptSpec::new(QC_PLANNER_SYSTEM, Self::render_user(input))
            .with_images(self.crops.clone())
            // Four steps of one sentence each, a root cause and a number. 400 is comfortably above
            // the largest valid answer and low enough that a model which starts writing an essay is
            // truncated into a schema failure rather than billed for it.
            .with_max_tokens(400)
            // Section 7: "reasoning tier with vision". This is the only task in the product at the
            // reasoning tier, and the reason is that the question is genuinely a planning one -
            // which of several interacting problems is the cause of the others.
            .with_min_tier(Tier::Reasoning)
    }

    fn output_schema(&self) -> &'static str {
        QC_PLAN_SCHEMA
    }

    /// The local answer: **escalate**.
    ///
    /// Section 7's offline fallback, and the same shape phase 24's `CleanupJudgement` uses: the
    /// fallback is identical to the task's most cautious answer, so an unreachable provider and a
    /// cautious model leave the photograph in exactly the same state.
    ///
    /// What the offline path does *not* lose is the mechanical ordering. `triage::order` runs
    /// whether or not this task was reached, so a wedding edited with the cloud switched off still
    /// gets root-cause-first remediation - it simply gets no second opinion on the frames where the
    /// mechanical rules disagree with themselves.
    fn local_fallback(&self, input: &Self::Input) -> Result<Self::Output, AuraError> {
        Ok(QcPlanOutput::escalation(format!(
            "a second opinion was unavailable, and this frame has {} findings that mechanical \
             rules could not order confidently",
            input.findings.len()
        )))
    }

    /// Six cents, the highest ceiling of any task in the product.
    ///
    /// Three 1024 px crops is about three megapixels, near 4,200 image tokens, plus roughly 500
    /// tokens of findings and recipe context and a 400 token ceiling on the answer. A reasoning-tier
    /// call is about 4,700 x $15/M + 400 x $60/M, near USD 0.095 at the worst - and most calls send
    /// one crop rather than three.
    ///
    /// **Forty of those is the section 7 ceiling and is the most expensive thing this product does
    /// per wedding.** It is justified by what it is spent on: the frames with three or more
    /// interacting problems, which are the frames a photographer would otherwise have to open
    /// themselves.
    fn max_cost_usd(&self) -> f32 {
        0.06
    }
}

/// The most read-only tool calls one invocation may make. Section 7's bound, re-exported so a caller
/// wiring the agent loop has one place to read it from.
#[must_use]
pub const fn max_tool_steps() -> u8 {
    MAX_TOOL_STEPS
}

/// Build the input for one image's open tickets.
///
/// Deliberately takes tickets rather than a `Frame`: the planner is told what was *found*, never
/// what was measured but passed. A model handed every reading would reason about the readings, and
/// the question is about the findings.
#[must_use]
pub fn input_for(
    tickets: &[&QcTicket],
    scene: &str,
    recipe_summary: Vec<(String, i64)>,
    node_stats: Vec<(String, i64)>,
    has_runner_up: bool,
    must_have: bool,
    crops_hash: String,
) -> Option<QcPlanInput> {
    let first = tickets.first()?;
    Some(QcPlanInput {
        image_ref: first.image_id.to_db(),
        scene: scene.to_string(),
        findings: tickets
            .iter()
            .map(|ticket| FindingSummary::of(ticket))
            .collect(),
        recipe_summary,
        node_stats,
        has_runner_up,
        must_have,
        crops_hash,
    })
}

/// Which reason code a planner outcome is recorded as.
#[must_use]
pub const fn outcome_code(reached: bool, any_step_survived: bool) -> QcCode {
    if !reached {
        QcCode::PlannerUnavailable
    } else if any_step_survived {
        QcCode::MultiSymptom
    } else {
        QcCode::PlannerRefused
    }
}

/// Whether a category is one the planner may be consulted about at all.
///
/// The two gallery-scoped ones are not: coverage and duplicates are facts about the set, their
/// remedy is a selection change rather than a parameter, and a model looking at three crops has
/// nothing to contribute about whether the gallery contains a photograph of the cake.
#[must_use]
pub const fn consultable(category: QcCategory) -> bool {
    !category.is_gallery_scoped()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::qc::Remedy;

    fn plan(steps: Vec<ProposedStep>) -> QcPlanOutput {
        QcPlanOutput {
            plan: steps,
            root_cause: Some("the white balance".into()),
            confidence: 0.8,
        }
    }

    fn step(remedy: &str, magnitude: Option<f32>) -> ProposedStep {
        ProposedStep {
            remedy: remedy.into(),
            target: "colour.grade".into(),
            magnitude,
            reason: "the skin drifted".into(),
        }
    }

    #[test]
    fn a_proposed_step_is_not_a_remedy_and_cannot_become_one_by_deserialising() {
        // The whole of ADR-0055 section 7. `ProposedStep` has no variant, no constructor and no
        // `From` that produces a `Remedy`; `remedy::validate` is the only route, and it takes the
        // ticket, the frame and the policy as well.
        //
        // The strongest half of this is a compile-time fact rather than an assertion:
        // `aura_core::contract::qc::Remedy` deliberately derives neither `Serialize` nor
        // `Deserialize`, so **there is no way to turn a model's JSON into one**. A line reading
        // `serde_json::from_str::<Remedy>(..)` does not compile, which is why this test asserts on
        // the step instead - if somebody added the derive to make a wire type convenient, that line
        // would start compiling and this comment is where they would find out why not to.
        let step = step("reduce_strength", Some(0.75));
        let as_json = serde_json::to_string(&step).unwrap();
        assert!(as_json.contains("reduce_strength"));
        let back: ProposedStep = serde_json::from_str(&as_json).unwrap();
        assert_eq!(back, step);
        // And a step's remedy is a *string* at this point, which is what forces it through
        // validation before anything can act on it.
        assert_eq!(back.remedy, "reduce_strength");
        let _: fn(
            Remedy,
            &QcTicket,
            &crate::checks::Frame,
            crate::policy::LoopPolicy,
        ) -> Result<Remedy, crate::remedy::Refusal> = crate::remedy::validate;
    }

    #[test]
    fn a_plan_longer_than_the_contract_allows_is_refused() {
        let long = plan(vec![step("revert_op", None); MAX_PLAN_STEPS + 1]);
        let err = long.validate().expect_err("too many steps");
        assert!(err.contains("at most"));
    }

    #[test]
    fn a_remedy_outside_the_five_is_refused_with_a_sentence_a_model_can_act_on() {
        let bad = plan(vec![step("delete_photograph", None)]);
        let err = bad.validate().expect_err("not one of the five");
        assert!(err.contains("delete_photograph"));
        assert!(err.contains("five allowed remedies"));
    }

    #[test]
    fn a_hallucinated_magnitude_is_refused_at_the_schema_layer_as_well() {
        let bad = plan(vec![step("reduce_strength", Some(3.0))]);
        let err = bad.validate().expect_err("outside the bounds");
        assert!(err.contains("may only reduce"));
    }

    #[test]
    fn a_strength_reduction_without_a_magnitude_is_refused() {
        let bad = plan(vec![step("reduce_strength", None)]);
        assert!(bad.validate().is_err());
    }

    #[test]
    fn a_magnitude_on_a_remedy_that_does_not_take_one_is_refused() {
        let bad = plan(vec![step("revert_op", Some(0.5))]);
        let err = bad.validate().expect_err("only reduce_strength takes one");
        assert!(err.contains("only reduce_strength"));
    }

    #[test]
    fn a_step_with_no_reason_is_refused() {
        let mut bad = step("revert_op", None);
        bad.reason = "  ".into();
        let err = plan(vec![bad])
            .validate()
            .expect_err("every step explains itself");
        assert!(err.contains("reference the specific finding"));
    }

    #[test]
    fn a_valid_plan_passes() {
        assert!(plan(vec![
            step("resolve_param", None),
            step("reduce_strength", Some(0.75))
        ])
        .validate()
        .is_ok());
    }

    #[test]
    fn the_offline_fallback_escalates_and_changes_nothing() {
        let task = QcPlanner::default();
        let input = QcPlanInput {
            image_ref: "img_x".into(),
            scene: "ceremony".into(),
            findings: Vec::new(),
            recipe_summary: Vec::new(),
            node_stats: Vec::new(),
            has_runner_up: false,
            must_have: false,
            crops_hash: "abc".into(),
        };
        let answer = task
            .local_fallback(&input)
            .expect("the fallback cannot fail");
        assert!(answer.validate().is_ok());
        assert!(
            answer.actionable().is_empty(),
            "escalation is not an action"
        );
        // Not zero: deciding to hand a frame to a person is a real decision, and a zero renders in
        // the Explain panel as though nothing had been decided.
        assert!(answer.confidence > 0.0);
    }

    #[test]
    fn an_escalation_step_is_never_actionable() {
        let answer = plan(vec![step("escalate", None), step("revert_op", None)]);
        assert_eq!(answer.actionable().len(), 1);
        assert_eq!(answer.actionable()[0].remedy, "revert_op");
    }

    #[test]
    fn the_input_carries_no_identity_and_no_filename() {
        // Phase 06's rule. The compiler is most of the assertion - there is no field one could go
        // in - and this catches a field being added later.
        let json = serde_json::to_string(&QcPlanInput {
            image_ref: "img_x".into(),
            scene: "ceremony".into(),
            findings: Vec::new(),
            recipe_summary: Vec::new(),
            node_stats: Vec::new(),
            has_runner_up: false,
            must_have: false,
            crops_hash: "abc".into(),
        })
        .unwrap();
        for banned in [
            "identity", "role", "name", "path", "filename", "bride", "groom",
        ] {
            assert!(!json.contains(banned), "{banned} must never be sent");
        }
    }

    #[test]
    fn deviations_travel_as_integers_so_the_cache_can_hit() {
        // `CloudTask::Input` is `Hash` and the cache key is built from it. Two runs whose
        // deviations differ in the seventh decimal must hash identically, or the cache never hits
        // and section 7's forty-call ceiling buys nothing.
        assert_eq!(milli(4.2004), milli(4.2003));
        assert_eq!(milli(f32::NAN), 0);
    }

    #[test]
    fn the_two_gallery_scoped_categories_are_never_asked_about() {
        assert!(!consultable(QcCategory::Coverage));
        assert!(!consultable(QcCategory::Duplicate));
        assert!(consultable(QcCategory::Skin));
    }

    #[test]
    fn an_unreachable_planner_and_a_refused_plan_are_different_codes() {
        assert_eq!(outcome_code(false, false), QcCode::PlannerUnavailable);
        assert_eq!(outcome_code(true, false), QcCode::PlannerRefused);
        // Two different runbooks, because "we could not ask" and "we asked and the answer was not
        // allowed" send somebody to two different places.
        assert_ne!(outcome_code(false, false), outcome_code(true, false));
    }

    #[test]
    fn the_bounds_are_section_sevens() {
        assert_eq!(max_tool_steps(), 6);
        assert_eq!(MAX_PLAN_STEPS, 4);
        assert_eq!(MAX_CROPS, 3);
        assert_eq!(QcPlanner::VERSION, 1);
    }
}
