//! The record of how an agent got to its answer.
//!
//! Two consumers, and they want the same thing for different reasons.
//!
//! The **model** needs the scratchpad rendered back into the prompt, because that
//! is how a stateless completion API remembers what it already tried. The
//! rendering is deterministic and stable - a scratchpad that formatted its steps
//! differently between runs would change the prompt hash of every step after the
//! first and defeat the cache.
//!
//! A **person** needs it in the audit row, because `AURA-CLOUD-6011` - the agent
//! ran out of steps - is unanswerable without seeing what the steps were. The
//! usual finding is the same tool called twice with identical arguments, which is
//! visible in one glance at the JSON and invisible in any summary of it.
//!
//! Tool results are truncated on the way in. A tool that returns a large document
//! would otherwise fill the context window by step three, and the model gets more
//! from six short observations than from one long one.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How much of a tool result is kept.
pub const MAX_RESULT_CHARS: usize = 2_000;

/// What happened at one step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StepKind {
    /// The model asked for a tool.
    Tool {
        /// Which tool.
        name: String,
        /// The arguments it passed, as JSON text with sorted keys.
        arguments: String,
        /// What came back, truncated.
        result: String,
        /// Set when the tool failed; the model sees this and may try something
        /// else, which is why a tool error does not abort the loop.
        error: Option<String>,
    },
    /// The model produced its answer.
    Answer {
        /// The answer text, which the caller then validates against the schema.
        text: String,
    },
    /// The loop stopped without an answer.
    Stopped {
        /// Which ceiling, or `cancelled`.
        reason: String,
    },
}

/// One entry, with what it cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Step {
    /// Zero-based position in the run.
    pub index: u16,
    /// What happened.
    pub kind: StepKind,
    /// Prompt tokens billed for this step.
    pub tokens_in: u32,
    /// Completion tokens billed for this step.
    pub tokens_out: u32,
    /// Milliseconds this step took.
    pub elapsed_ms: u64,
}

/// Everything that happened, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Scratchpad {
    /// The steps, oldest first.
    pub steps: Vec<Step>,
}

impl Scratchpad {
    /// An empty scratchpad.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a step.
    pub fn push(&mut self, kind: StepKind, tokens_in: u32, tokens_out: u32, elapsed_ms: u64) {
        let index = u16::try_from(self.steps.len()).unwrap_or(u16::MAX);
        self.steps.push(Step {
            index,
            kind,
            tokens_in,
            tokens_out,
            elapsed_ms,
        });
    }

    /// Append a tool step, truncating the result.
    pub fn push_tool(
        &mut self,
        name: &str,
        arguments: &Value,
        result: Result<&Value, &str>,
        elapsed_ms: u64,
    ) {
        // Serialised through `serde_json` rather than `Display`, and with the
        // object's own key order, which `serde_json::Map` keeps sorted by default
        // in this workspace's feature set. Two runs therefore render the same
        // arguments identically.
        let arguments = serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string());
        let (text, error) = match result {
            Ok(value) => (
                truncate(&serde_json::to_string(value).unwrap_or_default()),
                None,
            ),
            Err(message) => (String::new(), Some(truncate(message))),
        };
        self.push(
            StepKind::Tool {
                name: name.to_string(),
                arguments,
                result: text,
                error,
            },
            0,
            0,
            elapsed_ms,
        );
    }

    /// How many steps have been taken.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// True when nothing has happened yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The answer, when the loop produced one.
    #[must_use]
    pub fn answer(&self) -> Option<&str> {
        self.steps.iter().rev().find_map(|step| match &step.kind {
            StepKind::Answer { text } => Some(text.as_str()),
            _ => None,
        })
    }

    /// True when this exact tool call has already been made.
    ///
    /// The loop uses it to break a cycle one step early: repeating a call that
    /// is a pure function of its arguments cannot produce a new observation, so
    /// the model is told so rather than being allowed to spend another step.
    #[must_use]
    pub fn already_called(&self, name: &str, arguments: &Value) -> bool {
        let rendered = serde_json::to_string(arguments).unwrap_or_default();
        self.steps.iter().any(|step| match &step.kind {
            StepKind::Tool {
                name: seen,
                arguments: seen_arguments,
                ..
            } => seen == name && *seen_arguments == rendered,
            _ => false,
        })
    }

    /// The scratchpad as the model reads it back.
    ///
    /// Writing into the buffer rather than concatenating formatted strings: this
    /// runs once per agent step with the whole history each time, so the
    /// allocation per line is the one that would show up.
    #[must_use]
    pub fn render(&self) -> String {
        use std::fmt::Write as _;

        let mut out = String::new();
        for step in &self.steps {
            // `write!` into a `String` is infallible; the result is discarded
            // rather than unwrapped, which R1 bans even where it cannot fire.
            match &step.kind {
                StepKind::Tool {
                    name,
                    arguments,
                    result,
                    error,
                } => {
                    let _ = writeln!(out, "step {}: called {name}({arguments})", step.index);
                    match error {
                        Some(message) => {
                            let _ = writeln!(out, "  failed: {message}");
                        }
                        None => {
                            let _ = writeln!(out, "  result: {result}");
                        }
                    }
                }
                StepKind::Answer { text } => {
                    let _ = writeln!(out, "step {}: answered {text}", step.index);
                }
                StepKind::Stopped { reason } => {
                    let _ = writeln!(out, "step {}: stopped ({reason})", step.index);
                }
            }
        }
        out
    }

    /// The scratchpad as it is stored in the audit row.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"steps\":[]}".to_string())
    }

    /// Total tokens across every step.
    #[must_use]
    pub fn tokens(&self) -> u32 {
        self.steps
            .iter()
            .map(|step| step.tokens_in.saturating_add(step.tokens_out))
            .fold(0u32, u32::saturating_add)
    }
}

fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_RESULT_CHARS {
        return text.to_string();
    }
    let head: String = text.chars().take(MAX_RESULT_CHARS).collect();
    format!("{head}... [truncated]")
}
