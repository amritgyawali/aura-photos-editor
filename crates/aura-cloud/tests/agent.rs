// R1 exemption: `serde_json::json!` unwraps internally, and `scripts/check-banned.sh`
// already exempts `*/tests/*` from the no-unwrap rule. The allow is here so
// `clippy -D warnings` agrees with that policy rather than fighting it.
#![allow(clippy::disallowed_methods)]

//! The bounded loop, tested without a language model.
//!
//! Every interesting failure of an agent loop is a property of the loop rather
//! than of the model: cycling, running out of steps, a tool erroring mid-run, a
//! cancel arriving between steps, and a tool list whose order changes between
//! machines. A scripted [`AgentBrain`] reproduces all of them in microseconds.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use aura_cloud::agent::{
    AgentBrain, AgentLoop, AgentMove, Limits, Scratchpad, StaticTool, StepKind, ToolRegistry,
    TurnCost,
};
use aura_core::clock::{Clock, FixedClock};
use aura_core::progress::CancelToken;
use aura_core::AuraResult;
use serde_json::{json, Value};
use time::OffsetDateTime;

/// A brain that plays a fixed script of moves.
#[derive(Debug)]
struct Script {
    moves: Vec<AgentMove>,
    next: AtomicUsize,
    cost: TurnCost,
}

impl Script {
    fn new(moves: Vec<AgentMove>) -> Self {
        Self {
            moves,
            next: AtomicUsize::new(0),
            cost: TurnCost {
                tokens_in: 100,
                tokens_out: 20,
                cost_usd: 0.001,
            },
        }
    }

    fn turns(&self) -> usize {
        self.next.load(Ordering::SeqCst)
    }
}

impl AgentBrain for Script {
    fn next(&self, _question: &str, _scratchpad: &Scratchpad) -> AuraResult<(AgentMove, TurnCost)> {
        let index = self.next.fetch_add(1, Ordering::SeqCst);
        // Past the end of the script it repeats the last move, which is how the
        // cycling and step-cap cases are written.
        let chosen = self
            .moves
            .get(index)
            .or_else(|| self.moves.last())
            .cloned()
            .unwrap_or(AgentMove::Finish {
                text: "{}".to_string(),
            });
        Ok((chosen, self.cost))
    }
}

/// A tool that always fails, to prove an error is an observation not an abort.
#[derive(Debug)]
struct BrokenTool;

impl aura_cloud::agent::Tool for BrokenTool {
    fn spec(&self) -> aura_cloud::agent::ToolSpec {
        aura_cloud::agent::ToolSpec {
            name: "broken".to_string(),
            description: "always fails".to_string(),
            input_schema: "{\"type\":\"object\"}".to_string(),
        }
    }

    fn call(&self, _arguments: &Value) -> AuraResult<Value> {
        Err(aura_core::errors::cloud::provider_error(
            "tool",
            0,
            "the index was not built",
        ))
    }
}

/// A tool that says it writes, which must be refused at registration.
#[derive(Debug)]
struct WritingTool;

impl aura_cloud::agent::Tool for WritingTool {
    fn spec(&self) -> aura_cloud::agent::ToolSpec {
        aura_cloud::agent::ToolSpec {
            name: "delete_photo".to_string(),
            description: "deletes a photograph".to_string(),
            input_schema: "{\"type\":\"object\"}".to_string(),
        }
    }

    fn call(&self, _arguments: &Value) -> AuraResult<Value> {
        Ok(Value::Null)
    }

    fn is_read_only(&self) -> bool {
        false
    }
}

fn clock() -> Arc<FixedClock> {
    FixedClock::at(OffsetDateTime::UNIX_EPOCH)
}

fn registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    let mut scenes = BTreeMap::new();
    scenes.insert("segment-1".to_string(), json!({ "scene": "ceremony" }));
    registry
        .register(Arc::new(StaticTool::new(
            "scene_lookup",
            "the local classifier's guess for a segment",
            scenes,
        )))
        .expect("registers");
    let mut times = BTreeMap::new();
    times.insert("segment-1".to_string(), json!({ "minutes": 32 }));
    registry
        .register(Arc::new(StaticTool::new(
            "timeline_lookup",
            "how long a segment lasted",
            times,
        )))
        .expect("registers");
    registry
}

#[test]
fn a_tool_that_writes_is_refused_at_registration() {
    let mut registry = ToolRegistry::new();
    let err = registry
        .register(Arc::new(WritingTool))
        .expect_err("agents propose, they do not act");
    assert_eq!(err.code.0, "AURA-CLOUD-6011");
    assert!(registry.is_empty());
}

#[test]
fn a_duplicate_tool_name_is_refused() {
    let mut registry = registry();
    let err = registry
        .register(Arc::new(StaticTool::new(
            "scene_lookup",
            "a second one",
            BTreeMap::new(),
        )))
        .expect_err("two tools cannot share a name");
    assert_eq!(err.code.0, "AURA-CLOUD-6011");
}

#[test]
fn the_tool_list_renders_in_a_stable_order() {
    // The list goes into the prompt, the prompt goes into the prompt hash, and
    // the prompt hash is the cache key. Insertion order must not reach it.
    let mut forwards = ToolRegistry::new();
    let mut backwards = ToolRegistry::new();
    let names = ["zebra", "alpha", "middle"];
    for name in names {
        forwards
            .register(Arc::new(StaticTool::new(name, "t", BTreeMap::new())))
            .expect("registers");
    }
    for name in names.iter().rev() {
        backwards
            .register(Arc::new(StaticTool::new(name, "t", BTreeMap::new())))
            .expect("registers");
    }
    assert_eq!(forwards.render(), backwards.render());
    assert!(forwards.render().starts_with("- alpha"));
}

#[test]
fn a_finishing_brain_produces_an_answer_and_a_scratchpad() {
    let agent = AgentLoop::new(registry(), clock() as Arc<dyn Clock>);
    let brain = Script::new(vec![
        AgentMove::UseTool {
            name: "scene_lookup".to_string(),
            arguments: json!({ "query": "segment-1" }),
        },
        AgentMove::Finish {
            text: "{\"scene\":\"ceremony\"}".to_string(),
        },
    ]);

    let outcome = agent
        .run(&brain, "what is this segment", &CancelToken::new())
        .expect("the loop finishes");

    assert!(outcome.finished());
    assert_eq!(outcome.answer.as_deref(), Some("{\"scene\":\"ceremony\"}"));
    assert_eq!(outcome.steps, 2);
    assert_eq!(outcome.tokens, 240);
    assert_eq!(outcome.scratchpad.len(), 2);
    assert!(outcome.scratchpad.render().contains("scene_lookup"));
    assert!(outcome.scratchpad.to_json().contains("ceremony"));
}

#[test]
fn a_repeated_tool_call_costs_nothing_and_says_so() {
    let agent = AgentLoop::new(registry(), clock() as Arc<dyn Clock>);
    let call = AgentMove::UseTool {
        name: "scene_lookup".to_string(),
        arguments: json!({ "query": "segment-1" }),
    };
    let brain = Script::new(vec![
        call.clone(),
        call,
        AgentMove::Finish {
            text: "{}".to_string(),
        },
    ]);

    let outcome = agent
        .run(&brain, "what is this segment", &CancelToken::new())
        .expect("the loop finishes");

    let repeated = outcome.scratchpad.steps.iter().any(|step| {
        matches!(&step.kind, StepKind::Tool { error: Some(message), .. }
            if message.contains("already called"))
    });
    assert!(
        repeated,
        "the second identical call was not short-circuited"
    );
    assert!(outcome.finished());
}

#[test]
fn an_unknown_tool_name_is_an_observation_not_an_abort() {
    let agent = AgentLoop::new(registry(), clock() as Arc<dyn Clock>);
    let brain = Script::new(vec![
        AgentMove::UseTool {
            name: "does_not_exist".to_string(),
            arguments: json!({}),
        },
        AgentMove::Finish {
            text: "{}".to_string(),
        },
    ]);

    let outcome = agent
        .run(&brain, "q", &CancelToken::new())
        .expect("the loop recovers");
    assert!(outcome.finished());
    assert!(outcome.scratchpad.render().contains("no such tool"));
}

#[test]
fn a_failing_tool_is_an_observation_not_an_abort() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(BrokenTool)).expect("registers");
    let agent = AgentLoop::new(registry, clock() as Arc<dyn Clock>);
    let brain = Script::new(vec![
        AgentMove::UseTool {
            name: "broken".to_string(),
            arguments: json!({}),
        },
        AgentMove::Finish {
            text: "{}".to_string(),
        },
    ]);

    let outcome = agent
        .run(&brain, "q", &CancelToken::new())
        .expect("the loop recovers");
    assert!(outcome.finished());
    let failed = outcome
        .scratchpad
        .steps
        .iter()
        .any(|step| matches!(&step.kind, StepKind::Tool { error: Some(_), .. }));
    assert!(failed);
}

#[test]
fn a_cycling_brain_is_stopped_by_the_step_cap() {
    // Two tools, alternating for ever. The step cap is the only thing between
    // this and an unbounded bill.
    let agent = AgentLoop::new(registry(), clock() as Arc<dyn Clock>).with_limits(Limits {
        max_steps: 4,
        ..Limits::default()
    });
    let brain = Script::new(vec![
        AgentMove::UseTool {
            name: "scene_lookup".to_string(),
            arguments: json!({ "query": "segment-1" }),
        },
        AgentMove::UseTool {
            name: "timeline_lookup".to_string(),
            arguments: json!({ "query": "segment-1" }),
        },
        AgentMove::UseTool {
            name: "scene_lookup".to_string(),
            arguments: json!({ "query": "other" }),
        },
        AgentMove::UseTool {
            name: "timeline_lookup".to_string(),
            arguments: json!({ "query": "other" }),
        },
        AgentMove::UseTool {
            name: "scene_lookup".to_string(),
            arguments: json!({ "query": "third" }),
        },
    ]);

    let err = agent
        .run(&brain, "q", &CancelToken::new())
        .expect_err("a cycling agent is stopped");
    assert_eq!(err.code.0, "AURA-CLOUD-6011");
    assert!(err.detail.contains("4 steps"), "{}", err.detail);
    assert_eq!(
        brain.turns(),
        4,
        "the cap is checked before the step, not after"
    );
}

#[test]
fn the_scratchpad_survives_the_failure_that_stopped_the_run() {
    let agent = AgentLoop::new(registry(), clock() as Arc<dyn Clock>).with_limits(Limits {
        max_steps: 2,
        ..Limits::default()
    });
    let brain = Script::new(vec![AgentMove::UseTool {
        name: "scene_lookup".to_string(),
        arguments: json!({ "query": "segment-1" }),
    }]);

    let (outcome, failure) = agent.run_recording(&brain, "q", &CancelToken::new());
    assert!(failure.is_some(), "the run failed");
    assert!(!outcome.scratchpad.is_empty(), "and the steps were kept");
    let stopped = outcome
        .scratchpad
        .steps
        .iter()
        .any(|step| matches!(&step.kind, StepKind::Stopped { .. }));
    assert!(stopped, "the reason it stopped is recorded");
}

#[test]
fn a_cost_ceiling_stops_the_run_before_the_step_cap_does() {
    let agent = AgentLoop::new(registry(), clock() as Arc<dyn Clock>).with_limits(Limits {
        max_steps: 100,
        max_cost_usd: 0.0025,
        ..Limits::default()
    });
    let brain = Script::new(vec![AgentMove::UseTool {
        name: "scene_lookup".to_string(),
        arguments: json!({ "query": "segment-1" }),
    }]);

    let err = agent
        .run(&brain, "q", &CancelToken::new())
        .expect_err("the money ran out");
    assert_eq!(err.code.0, "AURA-CLOUD-6011");
    assert!(err.detail.contains("USD"), "{}", err.detail);
}

#[test]
fn a_cancel_lands_within_one_step() {
    let agent = AgentLoop::new(registry(), clock() as Arc<dyn Clock>);
    let cancel = CancelToken::new();
    cancel.cancel();
    let brain = Script::new(vec![AgentMove::Finish {
        text: "{}".to_string(),
    }]);

    let err = agent
        .run(&brain, "q", &cancel)
        .expect_err("a cancelled agent stops");
    assert_eq!(err.code.0, "AURA-CLOUD-6013");
    assert_eq!(brain.turns(), 0, "the brain was never asked");
}

#[test]
fn the_wall_clock_limit_uses_the_injected_clock() {
    let clock = clock();
    let agent =
        AgentLoop::new(registry(), Arc::clone(&clock) as Arc<dyn Clock>).with_limits(Limits {
            max_wall_ms: 500,
            ..Limits::default()
        });
    clock.advance_ms(900);

    let brain = Script::new(vec![AgentMove::Finish {
        text: "{}".to_string(),
    }]);
    // The tracker starts at the clock's current reading, so a clock that is
    // already past the limit does not stop a run that has not started.
    let outcome = agent
        .run(&brain, "q", &CancelToken::new())
        .expect("the run starts from now, not from the epoch");
    assert!(outcome.finished());
}
