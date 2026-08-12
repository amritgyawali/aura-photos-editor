//! Bounded agent primitives, shared by every phase that reasons in steps.
//!
//! Nothing in here is a feature. Phases 27 and 29 build the QC triage agent and
//! the album agent on top of it, and both get the same four guarantees - a step
//! cap, deterministic tool ordering, a scratchpad in the audit row, and a cancel
//! that lands within one step - because those guarantees live here rather than in
//! each of them.
//!
//! The module is named `agent` and its files are the four the phase document
//! lists: `loop`, `tools`, `scratchpad`, `limits`. `loop` is a Rust keyword, so
//! the module is declared as `r#loop` and re-exported below under names that do
//! not need the escape at a call site.

pub mod limits;
#[path = "loop.rs"]
pub mod r#loop;
pub mod scratchpad;
pub mod tools;

pub use limits::{Exceeded, LimitTracker, Limits};
pub use r#loop::{AgentBrain, AgentLoop, AgentMove, AgentOutcome, TurnCost};
pub use scratchpad::{Scratchpad, Step, StepKind};
pub use tools::{StaticTool, Tool, ToolRegistry, ToolSpec};
