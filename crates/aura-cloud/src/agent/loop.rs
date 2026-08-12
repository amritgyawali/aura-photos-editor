//! The bounded loop itself.
//!
//! Phases 27 (QC triage) and 29 (album sequencing) both want an agent, and both
//! want the same four properties, so they are built once here rather than twice
//! there:
//!
//! * a **step cap** checked before each step, never after;
//! * **deterministic tool ordering**, so the prompt - and therefore the prompt
//!   hash, and therefore the cache key - is identical on every machine;
//! * a **structured scratchpad** that goes into the audit row verbatim;
//! * **timeout and cancellation** honoured between every step, so Stop takes
//!   effect within one step rather than at the end of the run.
//!
//! ## The brain is a port
//!
//! [`AgentBrain`] is what decides the next move. In production it is a provider
//! call; in tests it is a script. That split is what makes an agent loop testable
//! at all: the interesting failures - cycling, running out of steps, a tool
//! erroring mid-run, a cancel arriving between steps - are all properties of the
//! *loop*, and none of them need a language model to reproduce.
//!
//! ## A repeated call is refused, not paid for
//!
//! Every tool is a pure function of its arguments, so asking the same question
//! twice cannot produce a new answer. When the model does it anyway - the single
//! most common way an agent burns its step budget - the loop answers from the
//! scratchpad and tells the model it has already seen this, which costs nothing
//! and usually breaks the cycle.

use std::fmt;
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::errors::cloud::cancelled;
use aura_core::progress::CancelToken;
use aura_core::AuraResult;
use serde_json::Value;

use crate::agent::limits::{LimitTracker, Limits};
use crate::agent::scratchpad::{Scratchpad, StepKind};
use crate::agent::tools::ToolRegistry;

/// What the brain decided to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentMove {
    /// Call a tool and show me the result.
    UseTool {
        /// Registered tool name.
        name: String,
        /// Arguments, which the tool validates itself.
        arguments: Value,
    },
    /// Here is the answer.
    Finish {
        /// The answer text, which the caller validates against a schema.
        text: String,
    },
}

/// What one turn of the brain cost.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TurnCost {
    /// Prompt tokens billed.
    pub tokens_in: u32,
    /// Completion tokens billed.
    pub tokens_out: u32,
    /// US dollars billed.
    pub cost_usd: f64,
}

/// Deciding the next move. A provider call in production, a script in tests.
pub trait AgentBrain: Send + Sync + fmt::Debug {
    /// Choose the next move, given everything that has happened.
    ///
    /// # Errors
    ///
    /// Whatever the provider raised. The loop stops and the caller falls back
    /// locally; a brain that cannot answer is not something to retry inside the
    /// loop, because the loop's own retry policy lives in the provider client.
    fn next(&self, question: &str, scratchpad: &Scratchpad) -> AuraResult<(AgentMove, TurnCost)>;
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentOutcome {
    /// The answer, when there was one.
    pub answer: Option<String>,
    /// Everything that happened.
    pub scratchpad: Scratchpad,
    /// Steps taken.
    pub steps: u16,
    /// Tokens across the whole run.
    pub tokens: u32,
    /// US dollars across the whole run.
    pub cost_usd: f64,
    /// Milliseconds across the whole run.
    pub elapsed_ms: u64,
}

impl AgentOutcome {
    /// True when the loop produced an answer.
    #[must_use]
    pub const fn finished(&self) -> bool {
        self.answer.is_some()
    }
}

/// A bounded agent.
#[derive(Debug)]
pub struct AgentLoop {
    tools: ToolRegistry,
    limits: Limits,
    clock: Arc<dyn Clock>,
}

impl AgentLoop {
    /// A loop over a registry, with the default limits.
    #[must_use]
    pub fn new(tools: ToolRegistry, clock: Arc<dyn Clock>) -> Self {
        Self {
            tools,
            limits: Limits::default(),
            clock,
        }
    }

    /// Override the limits.
    #[must_use]
    pub const fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// The registry, whose rendering goes into the prompt.
    #[must_use]
    pub const fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    /// The limits in force.
    #[must_use]
    pub const fn limits(&self) -> Limits {
        self.limits
    }

    /// Run until the brain finishes, a limit is reached, or the token is set.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6011` when a ceiling stopped it, `AURA-CLOUD-6013` when the
    /// photographer did. Either way [`AgentOutcome::scratchpad`] is still
    /// available on the error path through [`AgentLoop::run_recording`], because
    /// the scratchpad is the whole diagnostic value of a failed run.
    pub fn run(
        &self,
        brain: &dyn AgentBrain,
        question: &str,
        cancel: &CancelToken,
    ) -> AuraResult<AgentOutcome> {
        let (outcome, failure) = self.run_recording(brain, question, cancel);
        match failure {
            Some(err) => Err(err),
            None => Ok(outcome),
        }
    }

    /// Run, and return the scratchpad whether it succeeded or not.
    ///
    /// The audit row wants the steps even - especially - when the run failed, so
    /// the failing path cannot be allowed to throw them away.
    #[must_use]
    pub fn run_recording(
        &self,
        brain: &dyn AgentBrain,
        question: &str,
        cancel: &CancelToken,
    ) -> (AgentOutcome, Option<aura_core::AuraError>) {
        let mut scratchpad = Scratchpad::new();
        let mut tracker = LimitTracker::start(self.limits, self.clock.monotonic_ms());
        let mut failure = None;

        loop {
            tracker.observe(self.clock.monotonic_ms());

            if cancel.is_cancelled() {
                scratchpad.push(
                    StepKind::Stopped {
                        reason: "cancelled".to_string(),
                    },
                    0,
                    0,
                    0,
                );
                failure = Some(cancelled(format!(
                    "the agent was stopped after {} steps",
                    tracker.steps()
                )));
                break;
            }

            if let Err(err) = tracker.check() {
                scratchpad.push(
                    StepKind::Stopped {
                        reason: err
                            .context
                            .iter()
                            .find(|(key, _)| key == "limit")
                            .map_or_else(|| "limit".to_string(), |(_, value)| value.clone()),
                    },
                    0,
                    0,
                    0,
                );
                failure = Some(err);
                break;
            }

            let step_started = self.clock.monotonic_ms();
            let (next, cost) = match brain.next(question, &scratchpad) {
                Ok(decided) => decided,
                Err(err) => {
                    scratchpad.push(
                        StepKind::Stopped {
                            reason: err.code.0.to_string(),
                        },
                        0,
                        0,
                        0,
                    );
                    failure = Some(err);
                    break;
                }
            };
            tracker.charge(
                cost.tokens_in.saturating_add(cost.tokens_out),
                cost.cost_usd,
            );
            let step_ms = self.clock.monotonic_ms().saturating_sub(step_started);

            match next {
                AgentMove::Finish { text } => {
                    scratchpad.push(
                        StepKind::Answer { text },
                        cost.tokens_in,
                        cost.tokens_out,
                        step_ms,
                    );
                    break;
                }
                AgentMove::UseTool { name, arguments } => {
                    self.dispatch(&mut scratchpad, &name, &arguments);
                }
            }
        }

        tracker.observe(self.clock.monotonic_ms());
        let outcome = build_outcome(&tracker, scratchpad);
        (outcome, failure)
    }

    /// Call one tool and record what happened.
    ///
    /// Three things that look like errors are recorded as observations instead -
    /// a repeated call, an unknown tool name, and a tool that failed. All three
    /// are things the model can react to, and aborting the run throws away the
    /// only information that would let it.
    fn dispatch(&self, scratchpad: &mut Scratchpad, name: &str, arguments: &Value) {
        if scratchpad.already_called(name, arguments) {
            scratchpad.push_tool(
                name,
                arguments,
                Err("already called with these arguments; the answer is above"),
                0,
            );
            return;
        }
        let Some(tool) = self.tools.get(name) else {
            scratchpad.push_tool(
                name,
                arguments,
                Err("no such tool; choose one from the list in the instructions"),
                0,
            );
            return;
        };

        let called = self.clock.monotonic_ms();
        match tool.call(arguments) {
            Ok(value) => scratchpad.push_tool(
                name,
                arguments,
                Ok(&value),
                self.clock.monotonic_ms().saturating_sub(called),
            ),
            Err(err) => scratchpad.push_tool(
                name,
                arguments,
                Err(&crate::redact::scrub(&err.user_message)),
                self.clock.monotonic_ms().saturating_sub(called),
            ),
        }
    }
}

/// Gather the counters and the scratchpad into the outcome.
fn build_outcome(tracker: &LimitTracker, scratchpad: Scratchpad) -> AgentOutcome {
    AgentOutcome {
        answer: scratchpad.answer().map(ToString::to_string),
        steps: tracker.steps(),
        tokens: tracker.tokens(),
        cost_usd: tracker.cost_usd(),
        elapsed_ms: tracker.elapsed_ms(),
        scratchpad,
    }
}
