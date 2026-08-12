# aura-cloud

The governed cloud AI gateway. **The only crate in AURA allowed to open a socket**
- `scripts/check-banned.sh` fails the build on a `TcpStream` anywhere else.

## What it is for

A photographer pastes one API key and the product gains a reasoning layer.
Everything difficult about that is in the word *governed*: cloud reasoning is the
only part of AURA that costs money per use, sends a client's photographs to
somebody else's computer, and answers differently on Tuesday than it did on
Monday.

## The one entry point

```rust
let result = gateway.run(&task, &input, &CallContext {
    project: &project_id,
    decision_ref: Some("seg-42"),
    cancel: &cancel_token,
})?;

match result.source {
    Source::Cloud         => { /* a provider answered, and it was billed */ }
    Source::Cache         => { /* an identical earlier call answered, free */ }
    Source::LocalFallback => { /* the task's own function answered */ }
}
```

`run` never fails for an ordinary cloud reason. No key, no consent, no budget, no
network, a malformed answer - all of them become the task's local fallback with
`source = LocalFallback`. It returns an error only when the *local fallback
itself* fails, and when the photographer cancels.

## Writing a task

```rust
impl CloudTask for MyTask {
    const NAME: &'static str = "my_task";
    const VERSION: u16 = 1;          // bump on ANY prompt, schema or ceiling change
    type Input = MyInput;            // Serialize + Hash: no floats, quantise them
    type Output = MyOutput;          // DeserializeOwned + Validate + Scored

    fn prompt(&self, input: &MyInput) -> PromptSpec { .. }
    fn output_schema(&self) -> &'static str { MY_SCHEMA }
    fn local_fallback(&self, input: &MyInput) -> Result<MyOutput, AuraError> { .. }
}
```

Four rules the compiler enforces and one it cannot:

1. **`local_fallback` is required.** Invariant 6 - the product completes a full
   wedding with no network - is checked by the type system, not by review.
2. **`Output: Scored`** means every decision carries `confidence` and `reasons`.
   Invariant 2, as a bound.
3. **`Input: Hash`** means no floats in an input. Quantise scores to per-mille
   `u16`, or a 1e-9 difference misses the cache and bills the user twice.
4. **`Output: Validate`** is the semantic check the JSON Schema cannot express -
   controlled vocabularies, and relationships between fields.
5. *(Not enforced.)* **Bump `VERSION`** whenever the prompt text, the schema or
   `max_tokens` changes. The cache key contains it, and stale answers are
   otherwise served under a contract that no longer exists.

## Module map

| Module | What it owns |
|---|---|
| `contract/cloud` | The frozen contract. Changing it needs an ADR. |
| `gateway` | The seven steps, and every audit row |
| `keys` | The credential store, by command invocation - never FFI, never `argv` |
| `provider`, `anthropic`, `openai`, `google`, `compat` | Four vendors, one shape |
| `http`, `cassette` | A real HTTP/1.1 client; recorded responses for CI |
| `payload`, `redact` | The only things that may be uploaded, and what is stripped |
| `schema`, `validate`, `repair`, `fallback` | Text in, typed value or local answer out |
| `cache`, `audit`, `budget` | The catalog side: `cloud_cache`, `cloud_calls`, `cloud_budget` |
| `agent` | Bounded loop primitives for phases 27 and 29 |
| `tasks` | The task registry and `SegmentNaming` |

## What this build can reach

`HttpTransport` is a complete HTTP/1.1 client and **does not speak TLS**. It
therefore reaches `http://` endpoints - a local or studio-network
OpenAI-compatible server, which is what `compat` exists for - and not the public
HTTPS endpoints of Anthropic, OpenAI or Google. Those providers' request shaping,
response parsing, error mapping and pricing are complete and tested against
cassettes; only the socket underneath is waived. See
`docs/adr/ADR-0009-cloud-ai-policy.md` for why, and for the condition on which
the waiver expires.

## Testing

```sh
cargo test --package aura-cloud                     # 100 tests, no network
cargo run --release -p aura-cli -- verify --phase 04  # the gate, no network
cargo test --release --package aura-perf            # the budgets
```

Recording a new cassette: add a JSON file to `tests/cloud/cassettes/` with a
`matcher` (a URL suffix and strings the request body must contain) and the
provider's response body. Files are matched in sorted-name order, so a recording
that must be tried first gets a lower number.
