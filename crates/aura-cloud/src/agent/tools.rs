//! The tool registry, and why its ordering is a correctness property.
//!
//! Tools are held in a `BTreeMap`, so [`ToolRegistry::specs`] lists them in the
//! same order on every machine and in every run. That is not tidiness. The list
//! of tools goes into the prompt, the prompt goes into the prompt hash, and the
//! prompt hash is the cache key - a registry that iterated in insertion order, or
//! in a hash map's order, would produce a different key on a different machine
//! and turn a 70 % cache hit rate into zero. Determinism invariant 4 reaches all
//! the way down here.
//!
//! Two rules bind every tool a later phase adds:
//!
//! **A tool reads; it does not act.** Article on cloud AI in
//! `docs/plan/CLAUDE.md`: "Cloud reasoning proposes; deterministic code decides
//! and executes. A model never executes an action directly." A tool that deletes
//! a photograph, writes a recipe or sends an email is not a tool, it is a bug.
//! [`Tool::is_read_only`] defaults to true and the loop refuses to register a
//! tool that says otherwise.
//!
//! **A tool is a pure function of its arguments.** The same arguments must give
//! the same result within a run, or the scratchpad in the audit row does not
//! reconstruct what happened.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use aura_core::errors::cloud::agent_limit;
use aura_core::AuraResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What the model is told about one tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// `snake_case` identifier the model calls.
    pub name: String,
    /// One sentence saying what it answers. Written for a model.
    pub description: String,
    /// JSON Schema for the arguments, in the subset [`crate::schema`] supports.
    pub input_schema: String,
}

/// Something the model may ask for while it reasons.
pub trait Tool: Send + Sync + fmt::Debug {
    /// Name, description and argument schema.
    fn spec(&self) -> ToolSpec;

    /// Answer one call.
    ///
    /// # Errors
    ///
    /// Anything the tool needs to say. The loop turns an error into a tool
    /// result the model can read rather than aborting, because "that lookup
    /// failed" is information a reasoning model can use.
    fn call(&self, arguments: &Value) -> AuraResult<Value>;

    /// False only for a tool that changes something, which is never allowed.
    fn is_read_only(&self) -> bool {
        true
    }
}

/// Every tool one agent may use.
#[derive(Debug, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a tool.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6011` when the tool declares that it writes, or when its name
    /// is already taken. Both are programming errors, and both are refused at
    /// registration rather than discovered mid-run.
    pub fn register(&mut self, tool: Arc<dyn Tool>) -> AuraResult<()> {
        let spec = tool.spec();
        if !tool.is_read_only() {
            return Err(agent_limit(
                "read_only",
                format!(
                    "tool `{}` declares that it changes state; agents propose, they do not act",
                    spec.name
                ),
            ));
        }
        if self.tools.contains_key(&spec.name) {
            return Err(agent_limit(
                "duplicate_tool",
                format!("a tool named `{}` is already registered", spec.name),
            ));
        }
        self.tools.insert(spec.name.clone(), tool);
        Ok(())
    }

    /// Every spec, in a stable order.
    #[must_use]
    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|tool| tool.spec()).collect()
    }

    /// Look one up by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// How many tools are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// True when there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// The tool list as it appears in the prompt.
    ///
    /// Rendered here rather than by each caller so that the text - and therefore
    /// the prompt hash - is identical wherever an agent is used.
    #[must_use]
    pub fn render(&self) -> String {
        use std::fmt::Write as _;

        let mut out = String::new();
        for spec in self.specs() {
            let _ = writeln!(
                out,
                "- {}: {}\n  arguments: {}",
                spec.name, spec.description, spec.input_schema
            );
        }
        out
    }
}

/// A tool that answers from a fixed table. Tests and the gate use it.
#[derive(Debug)]
pub struct StaticTool {
    spec: ToolSpec,
    answers: BTreeMap<String, Value>,
}

impl StaticTool {
    /// A tool that answers `arguments["query"]` from a table.
    #[must_use]
    pub fn new(name: &str, description: &str, answers: BTreeMap<String, Value>) -> Self {
        Self {
            spec: ToolSpec {
                name: name.to_string(),
                description: description.to_string(),
                input_schema: r#"{"type":"object","required":["query"],"properties":{"query":{"type":"string"}},"additionalProperties":false}"#
                    .to_string(),
            },
            answers,
        }
    }
}

impl Tool for StaticTool {
    fn spec(&self) -> ToolSpec {
        self.spec.clone()
    }

    fn call(&self, arguments: &Value) -> AuraResult<Value> {
        let query = arguments
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or_default();
        Ok(self.answers.get(query).cloned().unwrap_or(Value::Null))
    }
}
