# AURA-CLOUD-6011 - The agent loop hit its step, token or time limit

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and a local decision for that one step.

## What actually happened

A bounded agent loop reached one of its four limits - maximum steps, maximum total tokens, maximum wall clock, or maximum cost - without the model producing a final answer. Usually the model is cycling between two tools.

## What AURA does automatically

Stops immediately. The scratchpad up to that point is kept and written to the audit row so the loop is reconstructible, and the local fallback answers. **The loop cannot run away**: the limits are checked before each step, not after.

## Operator steps

1. Read the scratchpad in the audit row. A repeated tool call with identical arguments means the tool returned something the model could not use.
2. Tools are dispatched in a deterministic order; a loop that only reproduces sometimes means a tool is non-deterministic, which is a bug in that tool.
3. Raising a limit is a task-level change and needs a task version bump.

## Related

- Agent limits: `crates/aura-cloud/src/agent/limits.rs`
